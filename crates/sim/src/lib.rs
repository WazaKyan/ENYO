//! Cœur de simulation d'ENYO : le monde (grille + nations + diplomatie) et
//! l'application des commandes. Pur, déterministe, headless (cf. `CLAUDE.md`).
//!
//! L'unique façon de modifier l'état est [`World::apply`], qui transforme une
//! [`Command`] en [`Event`]s (event-sourcing). Chaque tour embarque un `checksum`
//! du monde et toute commande rejetée est loguée : l'audit se fait depuis le
//! seul journal.

pub mod climate;
pub mod diplo;
pub mod dynamics;
pub mod nation;
pub mod noise;
pub mod path;
pub mod province;
pub mod rng;
pub mod tile;
pub mod worldgen;

use std::collections::BTreeSet;

use diplo::Diplomacy;
use nation::Nation;
use proto::{Building, Command, Event};
use rng::Rng;
use serde::{Deserialize, Serialize};
use tile::{Tile, TileKind};

/// Constante FNV-1a (prime 64 bits) pour le checksum d'audit.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Savoir produit par tour par une case développée et peuplée (calibrage S3).
const KNOWLEDGE_RATE: f32 = 1.0;

// --- Calibrage économie interne S8 (single-source) ---
/// Influence gagnée par nation et par mois (de base).
const INFLUENCE_BASE: i64 = 1;
/// Matériaux max/mois d'une industrie idéale, pleinement dotée en main-d'œuvre.
const INDUSTRY_BASE: f32 = 8.0;
/// Population connectée pour une main-d'œuvre pleine (au-delà : plafonnée).
const INDUSTRY_WORKFORCE: f32 = 1000.0;
/// Dévastation ajoutée chaque mois par une industrie (pollution).
const INDUSTRY_POLLUTION: f32 = 0.01;

/// L'état complet de la partie — reconstructible depuis une graine et une suite
/// de commandes (donc rejouable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct World {
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub turn: u64,
    pub land_tiles: u32,
    pub ocean_tiles: u32,
    pub nations: Vec<Nation>,
    pub diplomacy: Diplomacy,
    rng: Rng,
    pub tiles: Vec<Tile>,
}

impl World {
    /// Génère un monde neuf `width` × `height` à partir d'une graine.
    pub fn new(seed: u64, width: u32, height: u32) -> Self {
        let gen = worldgen::generate(seed, width, height);
        World {
            seed,
            width,
            height,
            turn: 0,
            land_tiles: gen.land,
            ocean_tiles: gen.ocean,
            nations: Vec::new(),
            diplomacy: Diplomacy::default(),
            rng: Rng::new(seed),
            tiles: gen.tiles,
        }
    }

    /// Index linéaire d'une case (x, y).
    fn index(&self, x: u32, y: u32) -> usize {
        y as usize * self.width as usize + x as usize
    }

    /// Coordonnées (x, y) d'un index linéaire.
    fn coords(&self, idx: usize) -> (u32, u32) {
        (idx as u32 % self.width, idx as u32 / self.width)
    }

    /// Référence vers la case (x, y).
    pub fn tile(&self, x: u32, y: u32) -> &Tile {
        &self.tiles[self.index(x, y)]
    }

    /// Nation par id, si elle existe.
    pub fn nation(&self, id: u16) -> Option<&Nation> {
        self.nations.iter().find(|n| n.id == id)
    }

    /// (population totale, nombre de cases) possédées par une nation.
    pub fn nation_stats(&self, id: u16) -> (f32, u32) {
        let mut pop = 0.0;
        let mut tiles = 0;
        for t in &self.tiles {
            if t.owner == Some(id) {
                pop += t.population;
                tiles += 1;
            }
        }
        (pop, tiles)
    }

    /// Capacité de charge actuelle d'une case (fonction pure, jamais stockée).
    pub fn capacity_at(&self, x: u32, y: u32) -> f32 {
        let idx = self.index(x, y);
        let t = &self.tiles[idx];
        let terroir = t
            .owner
            .and_then(|o| self.nation(o))
            .map(|n| n.tech[nation::TERROIR])
            .unwrap_or(0);
        dynamics::carrying_capacity(t, terroir)
    }

    /// Événement de genèse (audit) : résumé du monde + checksum.
    pub fn genesis_event(&self) -> Event {
        Event::WorldGenerated {
            seed: self.seed,
            width: self.width,
            height: self.height,
            land_tiles: self.land_tiles,
            ocean_tiles: self.ocean_tiles,
            checksum: self.checksum(),
        }
    }

    /// Applique une commande et renvoie les événements produits.
    /// UNIQUE porte d'entrée pour modifier le monde.
    pub fn apply(&mut self, command: Command) -> Vec<Event> {
        match command {
            Command::Step => self.resolve_turn(),
            Command::Settle {
                x,
                y,
                nation,
                population,
            } => self.settle(x, y, nation, population),
            Command::Swarm {
                from_x,
                from_y,
                to_x,
                to_y,
            } => self.swarm(from_x, from_y, to_x, to_y),
            Command::Research { nation, branch } => self.research(nation, branch),
            Command::Mobilize {
                x,
                y,
                nation,
                amount,
            } => self.mobilize(x, y, nation, amount),
            Command::March {
                from_x,
                from_y,
                to_x,
                to_y,
            } => self.march(from_x, from_y, to_x, to_y),
            Command::Build {
                x,
                y,
                nation,
                building,
            } => self.build(x, y, nation, building),
            Command::DeclareWar { nation, target } => self.declare_war(nation, target),
            Command::MakePeace { nation, target } => self.make_peace(nation, target),
            Command::DirectorGrievance { from, to, amount } => {
                // Défense en profondeur (cohérent avec declare_war/settle/…) :
                // refuser un grief réflexif ou depuis/vers une nation inexistante.
                if from == to || self.nation(from).is_none() || self.nation(to).is_none() {
                    reject("grief invalide (réflexif ou nation inexistante)")
                } else {
                    self.diplomacy.add_grievance(from, to, amount as f32);
                    vec![Event::OpinionNudged {
                        from,
                        to,
                        amount: amount as f32,
                    }]
                }
            }
            Command::DirectorBlight { x, y, amount } => self.blight(x, y, amount),
            Command::DirectorWindfall { x, y, amount } => self.windfall(x, y, amount),
        }
    }

    /// Crée la nation `id` si absente ; renvoie son index dans `nations`.
    fn ensure_nation(&mut self, id: u16) -> usize {
        if let Some(i) = self.nations.iter().position(|n| n.id == id) {
            i
        } else {
            self.nations.push(Nation::new(id));
            self.nations.len() - 1
        }
    }

    /// Implante une population de départ (S2).
    fn settle(&mut self, x: u32, y: u32, nation: u16, population: u32) -> Vec<Event> {
        if x >= self.width || y >= self.height {
            return reject("hors carte");
        }
        let idx = self.index(x, y);
        if self.tiles[idx].kind != TileKind::Land {
            return reject("case d'eau");
        }
        if let Some(o) = self.tiles[idx].owner {
            if o != nation {
                return reject("case déjà possédée");
            }
        }
        self.ensure_nation(nation);
        let t = &mut self.tiles[idx];
        t.owner = Some(nation);
        t.population += population as f32;
        vec![Event::Settled {
            nation,
            x,
            y,
            population,
        }]
    }

    /// Essaimage (S2) : déplace la moitié de la population vers une cible à portée.
    fn swarm(&mut self, fx: u32, fy: u32, tx: u32, ty: u32) -> Vec<Event> {
        if fx >= self.width || fy >= self.height || tx >= self.width || ty >= self.height {
            return reject("hors carte");
        }
        let from = self.index(fx, fy);
        let to = self.index(tx, ty);
        if from == to {
            return reject("source = cible");
        }
        let nation = match self.tiles[from].owner {
            Some(o) => o,
            None => return reject("source non possédée"),
        };
        if self.tiles[from].population < 1000.0 {
            return reject("population source < 1000");
        }
        if self.tiles[to].kind != TileKind::Land {
            return reject("cible aquatique");
        }
        if let Some(o) = self.tiles[to].owner {
            if o != nation {
                // Essaimage sur une case ennemie : casus belli, pas d'installation.
                self.diplomacy.add_grievance(nation, o, 1.0);
                return vec![Event::GrievanceRaised {
                    from: nation,
                    to: o,
                    x: tx,
                    y: ty,
                }];
            }
        }

        let ni = self.ensure_nation(nation);
        let essor = self.nations[ni].tech[nation::ESSOR];
        let naval = self.nations[ni].tech[nation::LIEN];
        let budget = path::range_budget(essor);

        match path::reach_cost(
            &self.tiles,
            self.width,
            self.height,
            from,
            to,
            budget,
            naval,
        ) {
            Some(_) => {
                let moved = self.tiles[from].population * 0.5;
                self.tiles[from].population -= moved;
                let t = &mut self.tiles[to];
                t.owner = Some(nation);
                t.population += moved;
                vec![Event::Swarmed {
                    nation,
                    from_x: fx,
                    from_y: fy,
                    to_x: tx,
                    to_y: ty,
                    moved,
                }]
            }
            None => reject("cible hors de portée"),
        }
    }

    /// Recherche (S3) : dépense du savoir pour monter d'un palier une branche.
    fn research(&mut self, nation: u16, branch: u8) -> Vec<Event> {
        if branch as usize >= nation::BRANCHES {
            return reject("branche invalide");
        }
        let ni = self.ensure_nation(nation);
        let tier = self.nations[ni].tech[branch as usize];
        let cost = tech_cost(tier);
        if self.nations[ni].knowledge < cost {
            return reject("savoir insuffisant");
        }
        self.nations[ni].knowledge -= cost;
        let new_tier = tier + 1;
        self.nations[ni].tech[branch as usize] = new_tier;
        vec![Event::Researched {
            nation,
            branch,
            tier: new_tier,
        }]
    }

    /// Mobilisation (S5) : convertit de la population en force sur une case possédée.
    fn mobilize(&mut self, x: u32, y: u32, nation: u16, amount: u32) -> Vec<Event> {
        if x >= self.width || y >= self.height {
            return reject("hors carte");
        }
        let idx = self.index(x, y);
        if self.tiles[idx].owner != Some(nation) {
            return reject("case non possédée");
        }
        let t = &mut self.tiles[idx];
        let m = (amount as f32).min(t.population);
        if m <= 0.0 {
            return reject("population insuffisante");
        }
        t.population -= m;
        t.force += m;
        vec![Event::Mobilized {
            nation,
            x,
            y,
            amount: m,
        }]
    }

    /// Marche / attaque (S5) : déplace toute la force vers une case adjacente.
    fn march(&mut self, fx: u32, fy: u32, tx: u32, ty: u32) -> Vec<Event> {
        if fx >= self.width || fy >= self.height || tx >= self.width || ty >= self.height {
            return reject("hors carte");
        }
        if !self.is_adjacent(fx, fy, tx, ty) {
            return reject("cible non adjacente");
        }
        let from = self.index(fx, fy);
        let to = self.index(tx, ty);
        let nation = match self.tiles[from].owner {
            Some(o) => o,
            None => return reject("source non possédée"),
        };
        let force = self.tiles[from].force;
        if force <= 0.0 {
            return reject("aucune force à déplacer");
        }
        if self.tiles[to].kind != TileKind::Land {
            return reject("cible aquatique");
        }

        match self.tiles[to].owner {
            None => self.move_force(from, to, nation, force),
            Some(o) if o == nation => self.move_force(from, to, nation, force),
            Some(defender) => {
                if !self.diplomacy.at_war(nation, defender) {
                    return reject("pas en guerre avec la cible");
                }
                self.resolve_battle(from, to, nation, defender, force)
            }
        }
    }

    /// Déplacement pacifique de force (case amie ou libre).
    fn move_force(&mut self, from: usize, to: usize, nation: u16, force: f32) -> Vec<Event> {
        self.tiles[from].force = 0.0;
        self.tiles[to].force += force;
        let (from_x, from_y) = self.coords(from);
        let (to_x, to_y) = self.coords(to);
        vec![Event::Marched {
            nation,
            from_x,
            from_y,
            to_x,
            to_y,
            force,
        }]
    }

    /// Résolution déterministe d'un combat sur la case `to`.
    fn resolve_battle(
        &mut self,
        from: usize,
        to: usize,
        attacker: u16,
        defender: u16,
        force: f32,
    ) -> Vec<Event> {
        let (tx, ty) = self.coords(to);
        self.tiles[from].force = 0.0; // la force est engagée

        let d_force = self.tiles[to].force;
        let defense_bonus = self.tiles[to].ruggedness * 30.0 + self.tiles[to].population * 0.2;
        let resistance = d_force + defense_bonus;

        let conquered;
        let attacker_losses;
        let defender_losses;
        if force > resistance {
            // Conquête : la résistance est balayée, l'attaquant occupe la case.
            let remaining = force - resistance;
            let t = &mut self.tiles[to];
            t.owner = Some(attacker);
            t.force = remaining;
            t.population *= 0.7; // mise à sac
            t.devastation = (t.devastation + 0.4).clamp(0.0, 1.0);
            conquered = true;
            attacker_losses = resistance;
            defender_losses = d_force;
        } else {
            // Repoussé : le bonus de terrain encaisse, puis la force défensive.
            let force_damage = (force - defense_bonus).max(0.0);
            let new_force = (d_force - force_damage).max(0.0);
            let t = &mut self.tiles[to];
            t.force = new_force;
            t.devastation = (t.devastation + 0.2).clamp(0.0, 1.0);
            conquered = false;
            attacker_losses = force;
            defender_losses = d_force - new_force;
        }

        vec![Event::BattleResolved {
            attacker,
            defender,
            x: tx,
            y: ty,
            conquered,
            attacker_losses,
            defender_losses,
        }]
    }

    /// Déclare la guerre (S6).
    fn declare_war(&mut self, nation: u16, target: u16) -> Vec<Event> {
        if nation == target {
            return reject("on ne se déclare pas la guerre à soi-même");
        }
        self.diplomacy.set_war(nation, target, true);
        vec![Event::WarDeclared { nation, target }]
    }

    /// Fait la paix (S6).
    fn make_peace(&mut self, nation: u16, target: u16) -> Vec<Event> {
        if !self.diplomacy.at_war(nation, target) {
            return reject("pas en guerre");
        }
        self.diplomacy.set_war(nation, target, false);
        vec![Event::PeaceMade { nation, target }]
    }

    /// [Directeur] Calamité : biaise la case vers la famine (sécheresse + dégâts).
    fn blight(&mut self, x: u32, y: u32, amount: u32) -> Vec<Event> {
        if x >= self.width || y >= self.height {
            return reject("hors carte");
        }
        let a = (amount as f32 / 100.0).clamp(0.0, 1.0);
        let idx = self.index(x, y);
        let t = &mut self.tiles[idx];
        t.precipitation = (t.precipitation * (1.0 - a)).max(0.0);
        t.soil_fertility = (t.soil_fertility * (1.0 - a)).max(0.0);
        t.devastation = (t.devastation + a).clamp(0.0, 1.0);
        vec![Event::Blighted { x, y, amount: a }]
    }

    /// [Directeur] Aubaine : soigne la dévastation et enrichit la case (salut).
    fn windfall(&mut self, x: u32, y: u32, amount: u32) -> Vec<Event> {
        if x >= self.width || y >= self.height {
            return reject("hors carte");
        }
        let a = (amount as f32 / 100.0).clamp(0.0, 1.0);
        let idx = self.index(x, y);
        let t = &mut self.tiles[idx];
        t.devastation = (t.devastation - a).max(0.0);
        t.soil_fertility = (t.soil_fertility + a * 0.5).min(1.0);
        t.precipitation = (t.precipitation + a * 0.5).min(1.0);
        vec![Event::Windfall { x, y, amount: a }]
    }

    /// (fx,fy) et (tx,ty) sont-elles adjacentes (4-connexité, X enroulé) ?
    fn is_adjacent(&self, fx: u32, fy: u32, tx: u32, ty: u32) -> bool {
        let w = self.width as i64;
        let dxa = (fx as i64 - tx as i64).abs();
        let dx = dxa.min(w - dxa);
        let dy = (fy as i64 - ty as i64).abs();
        dx + dy == 1
    }

    /// Résout un tour : météo + biosphère, dynamiques anthropiques (S1), puis
    /// retombée des griefs (S6).
    fn resolve_turn(&mut self) -> Vec<Event> {
        self.turn += 1;
        let month = climate::month_of(self.turn);
        let weather_seed = self.rng.next_u64();
        let width = self.width;
        let height = self.height;

        // Passe 1 — météo + biosphère.
        let mut temp_sum = 0.0f64;
        let mut veg_sum = 0.0f64;
        for y in 0..height {
            let v = y as f32 / height as f32;
            let lat = (v - 0.5).abs() * 2.0;
            let north = v < 0.5;
            for x in 0..width {
                let idx = y as usize * width as usize + x as usize;
                let wn = noise_signed(weather_seed, x as i64, y as i64);
                let t = &mut self.tiles[idx];
                climate::update_weather(t, month, lat, north, wn);
                if t.kind == TileKind::Land {
                    let target =
                        worldgen::vegetation_target(t.kind, t.mean_temperature, t.precipitation);
                    t.vegetation += (target - t.vegetation) * 0.05;
                }
                temp_sum += t.temperature as f64;
                veg_sum += t.vegetation as f64;
            }
        }

        // Passe 2 — anthropique (capacité, population, développement, savoir, frontières).
        self.resolve_anthropic();

        // Les griefs retombent lentement.
        self.diplomacy.decay(0.99);

        let count = width as f64 * height as f64;
        let avg_temperature = (temp_sum / count) as f32;
        let avg_vegetation = (veg_sum / count) as f32;
        let checksum = self.checksum();
        tracing::debug!(
            turn = self.turn,
            month,
            avg_temperature,
            avg_vegetation,
            "tour résolu"
        );
        vec![Event::TurnResolved {
            turn: self.turn,
            month,
            avg_temperature,
            avg_vegetation,
            checksum,
        }]
    }

    /// Dynamiques anthropiques d'un tour (S1) + friction de frontière (S6).
    fn resolve_anthropic(&mut self) {
        if self.nations.is_empty() {
            return;
        }
        let width = self.width;
        let height = self.height;
        let old_pop: Vec<f32> = self.tiles.iter().map(|t| t.population).collect();
        let mut knowledge_gain = vec![0.0f32; self.nations.len()];
        // BTreeSet (et non HashSet) : ordre d'itération déterministe → griefs
        // appliqués dans un ordre stable → checksum reproductible.
        let mut borders: BTreeSet<(u16, u16)> = BTreeSet::new();

        for y in 0..height {
            for x in 0..width {
                let idx = y as usize * width as usize + x as usize;
                let pop = old_pop[idx];
                let owner = self.tiles[idx].owner;
                if pop <= 0.0 && owner.is_none() {
                    continue;
                }

                // Friction de frontière : voisin appartenant à une autre nation.
                if let Some(o) = owner {
                    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                        let nx = (x as i64 + dx).rem_euclid(width as i64);
                        let ny = y as i64 + dy;
                        if ny < 0 || ny >= height as i64 {
                            continue;
                        }
                        let v = (ny * width as i64 + nx) as usize;
                        if let Some(m) = self.tiles[v].owner {
                            if m != o {
                                borders.insert((o, m));
                            }
                        }
                    }
                }

                let ni = owner.and_then(|o| self.nations.iter().position(|n| n.id == o));
                let terroir = ni
                    .map(|i| self.nations[i].tech[nation::TERROIR])
                    .unwrap_or(0);
                let capacity = dynamics::carrying_capacity(&self.tiles[idx], terroir);
                let neighbor = neighbor_pop_sum(&old_pop, width, height, x, y);

                let t = &mut self.tiles[idx];
                dynamics::grow_population(t, capacity);
                dynamics::grow_development(t, pop, neighbor);
                t.devastation *= 0.95;
                let dev = t.development;
                let newpop = t.population;

                if let Some(i) = ni {
                    knowledge_gain[i] += dev * (newpop / 1000.0).min(1.0) * KNOWLEDGE_RATE;
                }
            }
        }

        for (i, g) in knowledge_gain.iter().enumerate() {
            self.nations[i].knowledge += g;
        }
        for (a, b) in borders {
            self.diplomacy.add_grievance(a, b, 0.1);
        }

        // --- Économie interne S8 ---
        for n in self.nations.iter_mut() {
            n.influence += INFLUENCE_BASE;
        }
        // Production des bâtiments (E1 : industrie). Contributions ENTIÈRES par
        // case, sommées en ordre d'index → indépendant de l'ordre, déterministe.
        let mut materials_gain = vec![0i64; self.nations.len()];
        for y in 0..height {
            for x in 0..width {
                let idx = y as usize * width as usize + x as usize;
                let Some(building) = self.tiles[idx].building else {
                    continue;
                };
                let Some(owner) = self.tiles[idx].owner else {
                    continue;
                };
                let Some(ni) = self.nations.iter().position(|n| n.id == owner) else {
                    continue;
                };
                if building == Building::Industry {
                    // Main-d'œuvre connectée (E1 : la case + ses 4 voisines de la
                    // même nation ; le réseau d'infrastructure viendra en E2).
                    let mut wpop = self.tiles[idx].population;
                    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                        let nx = (x as i64 + dx).rem_euclid(width as i64);
                        let ny = y as i64 + dy;
                        if ny < 0 || ny >= height as i64 {
                            continue;
                        }
                        let v = (ny * width as i64 + nx) as usize;
                        if self.tiles[v].owner == Some(owner) {
                            wpop += self.tiles[v].population;
                        }
                    }
                    materials_gain[ni] += industry_output(&self.tiles[idx], wpop);
                    let t = &mut self.tiles[idx];
                    t.devastation = (t.devastation + INDUSTRY_POLLUTION).min(1.0);
                }
            }
        }
        for (i, g) in materials_gain.iter().enumerate() {
            self.nations[i].materials += g;
        }
    }

    /// Bâtit (S8) un bâtiment sur une case possédée et vide, si la nation paie.
    fn build(&mut self, x: u32, y: u32, nation: u16, building: Building) -> Vec<Event> {
        if x >= self.width || y >= self.height {
            return reject("hors carte");
        }
        let idx = self.index(x, y);
        if self.tiles[idx].owner != Some(nation) {
            return reject("case non possédée");
        }
        if self.tiles[idx].building.is_some() {
            return reject("case déjà bâtie");
        }
        let (money_cost, mat_cost) = build_cost(building);
        let ni = self.ensure_nation(nation);
        if self.nations[ni].money < money_cost || self.nations[ni].materials < mat_cost {
            return reject("ressources insuffisantes");
        }
        self.nations[ni].money -= money_cost;
        self.nations[ni].materials -= mat_cost;
        self.tiles[idx].building = Some(building);
        vec![Event::Built {
            x,
            y,
            nation,
            building,
        }]
    }

    /// Checksum déterministe de l'état (FNV-1a). Empreinte d'audit : deux runs
    /// identiques ⇒ mêmes checksums.
    pub fn checksum(&self) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        h ^= self.turn;
        h = h.wrapping_mul(FNV_PRIME);
        for t in &self.tiles {
            fnv_u32(&mut h, t.elevation.to_bits());
            fnv_u32(&mut h, t.mean_temperature.to_bits());
            fnv_u32(&mut h, t.precipitation.to_bits());
            fnv_u32(&mut h, t.soil_fertility.to_bits());
            fnv_u32(&mut h, t.temperature.to_bits());
            fnv_u32(&mut h, t.precip_now.to_bits());
            fnv_u32(&mut h, t.vegetation.to_bits());
            fnv_u32(&mut h, t.population.to_bits());
            fnv_u32(&mut h, t.development.to_bits());
            fnv_u32(&mut h, t.devastation.to_bits());
            fnv_u32(&mut h, t.force.to_bits());
            fnv_u32(&mut h, t.owner.map(|o| o as u32 + 1).unwrap_or(0));
            h ^= match t.kind {
                TileKind::Ocean => 1,
                TileKind::Land => 2,
            };
            h = h.wrapping_mul(FNV_PRIME);
            h ^= match t.building {
                None => 0,
                Some(proto::Building::Industry) => 3,
                Some(proto::Building::Commerce) => 4,
                Some(proto::Building::Infrastructure) => 5,
                Some(proto::Building::Education) => 6,
                Some(proto::Building::Military) => 7,
            };
            h = h.wrapping_mul(FNV_PRIME);
        }
        for n in &self.nations {
            fnv_u32(&mut h, n.id as u32);
            fnv_u32(&mut h, n.knowledge.to_bits());
            for tier in n.tech {
                h ^= tier as u64;
                h = h.wrapping_mul(FNV_PRIME);
            }
            h ^= n.money as u64;
            h = h.wrapping_mul(FNV_PRIME);
            h ^= n.materials as u64;
            h = h.wrapping_mul(FNV_PRIME);
            h ^= n.influence as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        for &(a, b) in self.diplomacy.wars() {
            fnv_u32(&mut h, a as u32);
            fnv_u32(&mut h, b as u32);
            h = h.wrapping_mul(FNV_PRIME);
        }
        for &(from, to, amount) in self.diplomacy.grievances() {
            fnv_u32(&mut h, from as u32);
            fnv_u32(&mut h, to as u32);
            fnv_u32(&mut h, amount.to_bits());
            h = h.wrapping_mul(FNV_PRIME);
        }
        h
    }
}

/// Coût en savoir pour passer du palier `tier` au suivant. Public : utilisé par
/// la crate `ai` pour décider quand chercher.
pub fn tech_cost(tier: u8) -> f32 {
    25.0 * (tier as f32 + 1.0)
}

/// Coût de construction (argent, matériaux) d'un bâtiment (S8, single-source).
pub fn build_cost(b: Building) -> (i64, i64) {
    match b {
        Building::Industry => (100, 0),
        Building::Commerce => (120, 20),
        Building::Infrastructure => (40, 20),
        Building::Education => (150, 30),
        Building::Military => (120, 40),
    }
}

/// Matériaux/mois produits par une industrie : ∝ stats de case (sol, végétation,
/// intempéries) × main-d'œuvre connectée × (1 − dévastation). Entier (déterminisme).
fn industry_output(t: &Tile, connected_pop: f32) -> i64 {
    let terrain = (t.soil_fertility + t.vegetation + t.precip_now) / 3.0;
    let workforce = (connected_pop / INDUSTRY_WORKFORCE).min(1.0);
    let out = INDUSTRY_BASE * terrain * workforce * (1.0 - t.devastation);
    out.max(0.0).round() as i64
}

/// Une commande rejetée, loguée pour l'audit.
fn reject(reason: &str) -> Vec<Event> {
    vec![Event::CommandRejected {
        reason: reason.to_string(),
    }]
}

/// Mélange FNV-1a d'un `u32` dans l'accumulateur de checksum.
fn fnv_u32(h: &mut u64, val: u32) {
    for b in val.to_le_bytes() {
        *h ^= b as u64;
        *h = h.wrapping_mul(FNV_PRIME);
    }
}

/// Somme des populations des 4 voisins (X enroulé, Y borné).
fn neighbor_pop_sum(pop: &[f32], width: u32, height: u32, x: u32, y: u32) -> f32 {
    let w = width as i64;
    let h = height as i64;
    let mut s = 0.0;
    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
        let nx = (x as i64 + dx).rem_euclid(w);
        let ny = y as i64 + dy;
        if ny < 0 || ny >= h {
            continue;
        }
        s += pop[(ny * w + nx) as usize];
    }
    s
}

/// Petit bruit déterministe signé (~[-1, 1]) pour (seed, x, y), sans état.
fn noise_signed(seed: u64, x: i64, y: i64) -> f32 {
    let mut h = seed;
    h ^= (x as u64).wrapping_mul(0xA076_1D64_78BD_642F);
    h ^= (y as u64).wrapping_mul(0xE703_7ED1_A0B4_28DB);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    let unit = (h >> 11) as f64 / (1u64 << 53) as f64; // [0,1)
    (unit * 2.0 - 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_advances_turn() {
        let mut world = World::new(7, 64, 48);
        let events = world.apply(Command::Step);
        assert_eq!(world.turn, 1);
        assert_eq!(events.len(), 1);
    }
}
