//! Cœur de simulation d'ENYO : le monde (grille + nations + diplomatie) et
//! l'application des commandes. Pur, déterministe, headless (cf. `CLAUDE.md`).
//!
//! L'unique façon de modifier l'état est [`World::apply`], qui transforme une
//! [`Command`] en [`Event`]s (event-sourcing). Chaque tour embarque un `checksum`
//! du monde et toute commande rejetée est loguée : l'audit se fait depuis le
//! seul journal.

pub mod climate;
pub mod connect;
pub mod diplo;
pub mod dynamics;
pub mod nation;
pub mod noise;
pub mod path;
pub mod province;
pub mod rng;
pub mod tile;
pub mod unit;
pub mod worldgen;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use diplo::Diplomacy;
use nation::Nation;
use proto::{Building, Command, Event, UnitKind};
use rng::Rng;
use serde::{Deserialize, Serialize};
use tile::{Tile, TileKind};
use unit::{CarriedUnit, Unit};

/// Constante FNV-1a (prime 64 bits) pour le checksum d'audit.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Savoir produit par tour par une case développée et peuplée (calibrage S3).
const KNOWLEDGE_RATE: f32 = 1.0;

// --- Calibrage économie interne S8 (single-source) ---
/// Influence : **plancher** de base gagné chaque mois par une nation vivante.
const INFLUENCE_BASE: i64 = 3;
/// Influence/mois par **case possédée** : le **territoire** pèse (rayonnement).
const INFLUENCE_PER_TILE: i64 = 2;
/// Habitants nécessaires pour **+1 influence/mois** : la **population** pèse.
/// Plus une nation est peuplée et étendue, plus elle rayonne — et plus elle peut
/// s'étendre (l'expansion coûte de l'influence). Boucle vertueuse voulue.
const INFLUENCE_POP_DIVISOR: f32 = 1000.0;
/// Coût en influence d'une **expansion** (S2/E5) : étendre son territoire. Public :
/// l'IA s'en sert pour ne pas s'étendre sans influence.
pub const SWARM_INFLUENCE: i64 = 10;
/// Matériaux max/mois d'une industrie idéale, pleinement dotée en main-d'œuvre.
const INDUSTRY_BASE: f32 = 8.0;
/// Population connectée pour une main-d'œuvre pleine (au-delà : plafonnée).
const INDUSTRY_WORKFORCE: f32 = 1000.0;
/// Dévastation ajoutée chaque mois par une industrie (pollution). Faible : une
/// industrie n'abîme la case que **très lentement** (sur plusieurs décennies),
/// d'autant que la dévastation se résorbe (cf. heal dans `resolve_anthropic`).
const INDUSTRY_POLLUTION: f32 = 0.0015;
/// Matériaux/mois qu'un commerce idéal (main-d'œuvre pleine) transforme.
const COMMERCE_BASE: f32 = 10.0;
/// Argent produit par matériau transformé par le commerce.
const MONEY_PER_MAT: i64 = 3;
/// Habitation produite par matériau transformé par le commerce.
const HOUSING_PER_MAT: i64 = 1;
/// Science/mois d'une université idéale (main-d'œuvre pleine) — vraie source de tech.
const SCIENCE_BASE: f32 = 3.0;
/// Entretien mensuel (argent) d'une université ; impayé → elle chôme.
const EDUCATION_UPKEEP: i64 = 3;
/// Force (soldats) recrutée/mois par une caserne idéale (main-d'œuvre pleine).
const SOLDIERS_BASE: f32 = 20.0;
/// Entretien mensuel (argent) d'une caserne ; impayé → pas de recrutement.
const MILITARY_UPKEEP: i64 = 4;
/// Nourriture/mois d'une ferme idéale (main-d'œuvre pleine) ; ∝ terrain (E6).
const FARM_BASE: f32 = 12.0;
// --- Villes & famine (refonte « villes uniquement + famine ») ---
/// Population qu'amène la fondation d'une ville (colons) — amorce la croissance.
const CITY_SEED_POP: f32 = 100.0;
/// Subsistance par case : population nourrie « gratuitement » (cueillette /
/// agriculture vivrière locale). Seule la population **au-delà** de ce seuil, sur
/// une case, réclame de la nourriture cultivée (fermes) — donc seules les **villes
/// denses** subissent la famine. (Garde les petites implantations stables.)
const SUBSISTENCE_PER_TILE: f32 = 1500.0;
/// Habitants nourris par 1 unité de nourriture et par mois (calibrage famine).
const CITIZENS_PER_FOOD: f32 = 100.0;
/// Fraction de la population **non nourrie** qui décline chaque mois (famine).
const FAMINE_DECLINE: f32 = 0.25;
/// Dévastation ajoutée par une famine sévère (marque organique, lisible/Directeur).
const FAMINE_DEVASTATION: f32 = 0.03;
// --- Combat d'unités (S5) — bonus de défense du terrain (%), quantifiés entiers ---
/// Défense (%) max apportée par la végétation (couvert) de la case défendue.
const DEF_VEGETATION: f32 = 35.0;
/// Défense (%) max apportée par le relief (terrain accidenté/vallonné).
const DEF_RUGGEDNESS: f32 = 35.0;
/// Défense (%) max apportée par les intempéries (pluie/orage).
const DEF_WEATHER: f32 = 20.0;
/// Défense (%) ajoutée par la neige/le gel (température < 0 °C).
const DEF_SNOW: i32 = 15;
/// Plafond du bonus de défense total (%).
const DEF_CAP: i32 = 85;
/// Seuil de végétation au-delà duquel une case compte comme « forêt » (malus).
const FOREST_VEG: f32 = 0.5;
/// Seuil de relief au-delà duquel une case compte comme « accidentée » (malus).
const ROUGH_RUG: f32 = 0.4;
/// PV régénérés par mois pour une unité sur son **territoire national** (consomme
/// autant de **manpower**). Pas de régénération en terre étrangère / neutre.
const UNIT_REGEN_HP: i32 = 8;
// --- Score de guerre & capitulation (S5/S6) ---
/// Valeur d'une case pour le score de guerre : vide / bâtiment / ville.
const TILE_VALUE_EMPTY: i64 = 1;
const TILE_VALUE_BUILDING: i64 = 5;
const TILE_VALUE_CITY: i64 = 10;
/// Fraction (3/4 = 75 %) de la valeur totale d'un ennemi à OCCUPER pour le faire
/// capituler (strictement « > 75 % »).
const CAPITULATION_NUM: i64 = 3;
const CAPITULATION_DEN: i64 = 4;

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
    /// Unités militaires (S5) — agents discrets, ordre = ordre de création.
    #[serde(default)]
    pub units: Vec<Unit>,
    /// Prochain id d'unité (monotone → ids stables, rejouables).
    #[serde(default)]
    next_unit_id: u32,
    /// Cargo des galères (id de la galère → unités transportées). BTreeMap pour un
    /// ordre déterministe (checksum).
    #[serde(default)]
    pub cargo: BTreeMap<u32, Vec<CarriedUnit>>,
    /// **Index dérivé** (non sérialisé) : cases possédées par nation, en ordre
    /// d'index croissant. Maintenu à chaque changement de propriétaire → l'IA lit
    /// ses cases en O(possédées) au lieu de scanner les 400k cases à chaque plan
    /// (gros gain de perf par tick). Reconstruit après un chargement de snapshot.
    #[serde(skip)]
    owned_index: HashMap<u16, Vec<usize>>,
    /// Calculer le **checksum d'audit** à chaque tour (dans l'événement `TurnResolved`).
    /// Coûteux (hash des 400k cases / tick) et **inutile en jeu live** (le rejeu le
    /// recalcule depuis les commandes). L'UI le désactive pour fluidifier ; harness,
    /// rejeu et tests le gardent (audit complet). Non sérialisé, défaut = vrai.
    #[serde(skip_serializing, default = "checksum_on")]
    audit_checksum: bool,
}

/// Défaut du drapeau d'audit (vrai) — utilisé à la désérialisation des snapshots.
fn checksum_on() -> bool {
    true
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
            units: Vec::new(),
            next_unit_id: 1,
            cargo: BTreeMap::new(),
            owned_index: HashMap::new(),
            audit_checksum: true,
        }
    }

    /// Active/désactive le calcul du checksum d'audit par tour. L'UI le coupe en jeu
    /// live (perf) ; le déterminisme reste garanti par le rejeu des commandes.
    pub fn set_audit_checksum(&mut self, on: bool) {
        self.audit_checksum = on;
    }

    /// Ajoute une case à l'index des cases possédées, en maintenant l'ordre d'index
    /// (insertion triée) → l'IA itère ses cases dans l'ordre canonique (déterminisme).
    fn idx_own(&mut self, nation: u16, idx: usize) {
        let v = self.owned_index.entry(nation).or_default();
        if let Err(pos) = v.binary_search(&idx) {
            v.insert(pos, idx);
        }
    }

    /// Retire une case de l'index d'une nation (perte de territoire).
    fn idx_disown(&mut self, nation: u16, idx: usize) {
        if let Some(v) = self.owned_index.get_mut(&nation) {
            if let Ok(pos) = v.binary_search(&idx) {
                v.remove(pos);
            }
        }
    }

    /// Reconstruit entièrement l'index possédé depuis les cases (après un chargement
    /// de snapshot : l'index n'est pas sérialisé car il est dérivé).
    pub fn rebuild_owned_index(&mut self) {
        let mut idx_map: HashMap<u16, Vec<usize>> = HashMap::new();
        for (i, t) in self.tiles.iter().enumerate() {
            if let Some(o) = t.owner {
                idx_map.entry(o).or_default().push(i); // ordre d'index croissant
            }
        }
        self.owned_index = idx_map;
    }

    /// Cases possédées par `nation`, en ordre d'index croissant (vide si aucune).
    /// Lecture O(1) pour l'IA — évite de scanner toute la grille.
    pub fn owned_tiles(&self, nation: u16) -> &[usize] {
        self.owned_index.get(&nation).map_or(&[], |v| v.as_slice())
    }

    /// Index linéaire d'une case (x, y).
    fn index(&self, x: u32, y: u32) -> usize {
        y as usize * self.width as usize + x as usize
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
            Command::Build {
                x,
                y,
                nation,
                building,
            } => self.build(x, y, nation, building),
            Command::Demolish { x, y, nation } => self.demolish(x, y, nation),
            Command::CreateUnit {
                x,
                y,
                nation,
                kind,
            } => self.create_unit(x, y, nation, kind),
            Command::MoveUnit { unit, to_x, to_y } => self.move_unit(unit, to_x, to_y),
            Command::AttackUnit { unit, x, y } => self.attack_unit(unit, x, y),
            Command::Embark { unit, transport } => self.embark(unit, transport),
            Command::Disembark {
                transport,
                to_x,
                to_y,
            } => self.disembark(transport, to_x, to_y),
            Command::Endow {
                nation,
                money,
                materials,
                influence,
                housing,
                food,
            } => self.endow(nation, money, materials, influence, housing, food),
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
        self.idx_own(nation, idx);
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
        if self.nations[ni].influence < SWARM_INFLUENCE {
            return reject("influence insuffisante");
        }
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
                self.nations[ni].influence -= SWARM_INFLUENCE; // coût d'expansion (E5)
                let moved = self.tiles[from].population * 0.5;
                self.tiles[from].population -= moved;
                let t = &mut self.tiles[to];
                t.owner = Some(nation);
                t.population += moved;
                self.idx_own(nation, to);
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

    /// Dote une nation en ressources (genèse). Additif, jamais négatif.
    fn endow(
        &mut self,
        nation: u16,
        money: i64,
        materials: i64,
        influence: i64,
        housing: i64,
        food: i64,
    ) -> Vec<Event> {
        let ni = self.ensure_nation(nation);
        let n = &mut self.nations[ni];
        n.money += money.max(0);
        n.materials += materials.max(0);
        n.influence += influence.max(0);
        n.housing += housing.max(0);
        n.food += food.max(0);
        vec![Event::Endowed { nation }]
    }

    /// Déclare la guerre (S6).
    fn declare_war(&mut self, nation: u16, target: u16) -> Vec<Event> {
        if nation == target {
            return reject("on ne se déclare pas la guerre à soi-même");
        }
        self.diplomacy.set_war(nation, target, true);
        vec![Event::WarDeclared { nation, target }]
    }

    /// Fait la paix (S6). Les occupations entre les deux nations retombent (les
    /// cases occupées restent à leur propriétaire — rien n'est annexé sans victoire).
    fn make_peace(&mut self, nation: u16, target: u16) -> Vec<Event> {
        if !self.diplomacy.at_war(nation, target) {
            return reject("pas en guerre");
        }
        self.diplomacy.set_war(nation, target, false);
        self.clear_occupations(nation, target);
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

    /// Résout un tour : météo + biosphère, dynamiques anthropiques (S1), puis
    /// retombée des griefs (S6).
    fn resolve_turn(&mut self) -> Vec<Event> {
        self.turn += 1;
        let month = climate::month_of(self.turn);
        let weather_seed = self.rng.next_u64();
        let width = self.width;
        let height = self.height;

        // Passe 1 — météo + biosphère (coût mémoire-borné : on touche les 400k cases).
        let wu = width as usize;
        let mut temp_sum = 0.0f64;
        let mut veg_sum = 0.0f64;
        for (idx, t) in self.tiles.iter_mut().enumerate() {
            update_tile_weather(t, idx, wu, height, month, weather_seed);
            temp_sum += t.temperature as f64;
            veg_sum += t.vegetation as f64;
        }

        // Passe 2 — anthropique (capacité, population, développement, savoir, frontières).
        self.resolve_anthropic();

        // Unités (S5) : recharge des points de mouvement chaque mois.
        for u in &mut self.units {
            u.moves_left = unit::unit_stats(u.kind).moves;
        }

        // Régénération (S5) : une unité sur son **territoire national** récupère des
        // PV chaque mois en consommant du manpower (ordre des unités → déterministe).
        for ui in 0..self.units.len() {
            let (ux, uy, owner, kind, hp) = {
                let u = &self.units[ui];
                (u.x, u.y, u.owner, u.kind, u.hp)
            };
            let max_hp = unit::unit_stats(kind).max_hp;
            if hp >= max_hp || self.tiles[self.index(ux, uy)].owner != Some(owner) {
                continue;
            }
            let Some(ni) = self.nations.iter().position(|n| n.id == owner) else {
                continue;
            };
            let heal = UNIT_REGEN_HP
                .min(max_hp - hp)
                .min(self.nations[ni].manpower.max(0) as i32);
            if heal > 0 {
                self.units[ui].hp += heal;
                self.nations[ni].manpower -= heal as i64;
            }
        }

        // Capitulations (S5/S6) : annexion par occupation + paix imposée.
        let cap_events = self.resolve_capitulations();

        // Les griefs retombent lentement.
        self.diplomacy.decay(0.99);

        let count = width as f64 * height as f64;
        let avg_temperature = (temp_sum / count) as f32;
        let avg_vegetation = (veg_sum / count) as f32;
        // Checksum d'audit : sauté en jeu live (UI) — coûteux (400k cases) et inutile
        // là (le rejeu des commandes le recalcule). Gardé pour harness/rejeu/tests.
        let checksum = if self.audit_checksum { self.checksum() } else { 0 };
        tracing::debug!(
            turn = self.turn,
            month,
            avg_temperature,
            avg_vegetation,
            "tour résolu"
        );
        let mut events = cap_events;
        events.push(Event::TurnResolved {
            turn: self.turn,
            month,
            avg_temperature,
            avg_vegetation,
            checksum,
        });
        events
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
        // Nations encore vivantes (≥ 1 case possédée) : seules elles gagnent de
        // l'influence (sinon une nation conquise continuerait d'en accumuler).
        let mut owned = vec![false; self.nations.len()];
        // Agrégats par nation pour l'influence : population totale + nb de cases.
        // Sommés en ordre d'index (canonique) → flux d'influence rejouable au bit.
        let mut pop_sum = vec![0.0f32; self.nations.len()];
        let mut tile_count = vec![0i64; self.nations.len()];
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
                // Refonte « villes uniquement » : seule une case VILLE engendre de
                // la population (croissance logistique vers la capacité du terrain).
                // Les autres cases gardent leur population (colons, main-d'œuvre).
                if t.building == Some(Building::City) {
                    dynamics::grow_population(t, capacity);
                }
                dynamics::grow_development(t, pop, neighbor);
                // La dévastation se résorbe lentement (1 %/mois) : les cicatrices
                // (pollution, guerre, famine) persistent et s'accumulent dans le temps.
                t.devastation *= 0.99;
                let dev = t.development;
                let newpop = t.population;

                if let Some(i) = ni {
                    owned[i] = true;
                    pop_sum[i] += newpop;
                    tile_count[i] += 1;
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
        // Influence (E5+) : flux ∝ **territoire** ET **population** — plus une
        // nation est grande et peuplée, plus elle rayonne (et plus elle peut
        // s'étendre). Entier, sommé en ordre d'index → rejeu exact.
        for (i, n) in self.nations.iter_mut().enumerate() {
            if owned[i] {
                let from_pop = (pop_sum[i] / INFLUENCE_POP_DIVISOR) as i64;
                n.influence += INFLUENCE_BASE + tile_count[i] * INFLUENCE_PER_TILE + from_pop;
            }
        }
        // Réseaux d'infrastructure (E2) : main-d'œuvre mise en commun par les routes.
        let networks = connect::Networks::build(&self.tiles, width, height);
        // Production des bâtiments. Gains ENTIERS par nation, sommés/appliqués en
        // ordre d'index → indépendant de l'ordre, déterministe. La consommation de
        // matériaux par le commerce se fait depuis le stock (ordre d'index).
        let mut materials_gain = vec![0i64; self.nations.len()];
        let mut money_gain = vec![0i64; self.nations.len()];
        let mut housing_gain = vec![0i64; self.nations.len()];
        let mut science_gain = vec![0.0f32; self.nations.len()];
        let mut food_gain = vec![0i64; self.nations.len()];
        let mut manpower_gain = vec![0i64; self.nations.len()];
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
                let cpop = networks.connected_pop(&self.tiles, idx, owner);
                match building {
                    Building::Industry => {
                        let out = industry_output(&self.tiles[idx], cpop);
                        materials_gain[ni] += out;
                        // Pas de production -> pas de pollution (une usine à l'arrêt
                        // ne dégrade pas la case ; corrige le piège « 0 mat + pollue »).
                        if out > 0 {
                            let t = &mut self.tiles[idx];
                            t.devastation = (t.devastation + INDUSTRY_POLLUTION).min(1.0);
                        }
                    }
                    Building::Commerce if cpop > 0.0 => {
                        // Transforme des matériaux (selon main-d'œuvre, atténué par
                        // la dévastation) en argent + habitation.
                        let workforce = (cpop / INDUSTRY_WORKFORCE).min(1.0);
                        let want = (COMMERCE_BASE * workforce
                            * (1.0 - self.tiles[idx].devastation))
                            .max(0.0)
                            .round() as i64;
                        let used = want.min(self.nations[ni].materials.max(0));
                        if used > 0 {
                            self.nations[ni].materials -= used;
                            money_gain[ni] += used * MONEY_PER_MAT;
                            housing_gain[ni] += used * HOUSING_PER_MAT;
                        }
                    }
                    // Université : exige main-d'œuvre + commerce connecté + entretien
                    // payé ; sinon elle chôme (arm `_`). Produit de la science.
                    Building::Education
                        if cpop > 0.0
                            && networks.has_commerce_connected(&self.tiles, idx, owner)
                            && self.nations[ni].money >= EDUCATION_UPKEEP =>
                    {
                        self.nations[ni].money -= EDUCATION_UPKEEP;
                        let workforce = (cpop / INDUSTRY_WORKFORCE).min(1.0);
                        science_gain[ni] += SCIENCE_BASE * workforce;
                    }
                    // Caserne / port : produisent du **manpower** (national) depuis
                    // la pop connectée, contre un entretien mensuel ; sinon rien.
                    Building::Military | Building::Port
                        if cpop > 0.0 && self.nations[ni].money >= MILITARY_UPKEEP =>
                    {
                        self.nations[ni].money -= MILITARY_UPKEEP;
                        let workforce = (cpop / INDUSTRY_WORKFORCE).min(1.0);
                        manpower_gain[ni] += (SOLDIERS_BASE * workforce).round() as i64;
                    }
                    // Ferme : produit de la nourriture ∝ terrain (humidité, chaleur,
                    // sol) × main-d'œuvre connectée × (1 − dévastation).
                    Building::Farm => {
                        food_gain[ni] += farm_output(&self.tiles[idx], cpop);
                    }
                    // Infrastructure connecte (aucun produit) ; bâtiments à l'arrêt.
                    _ => {}
                }
            }
        }
        for i in 0..self.nations.len() {
            self.nations[i].materials += materials_gain[i];
            self.nations[i].money += money_gain[i];
            self.nations[i].housing += housing_gain[i];
            self.nations[i].knowledge += science_gain[i];
            self.nations[i].food += food_gain[i];
            self.nations[i].manpower += manpower_gain[i];
        }

        // --- Nourriture & famine (refonte « villes uniquement + famine ») ---
        // Toute la population mange chaque mois, AU-DELÀ d'un seuil de subsistance
        // par case (les petites implantations se nourrissent seules ; seules les
        // villes denses réclament des fermes). Consommation ENTIÈRE par nation,
        // en ordre d'index → rejeu exact ; la pénurie fait DÉCLINER la population
        // non nourrie (famine). `ids` est local (pas d'emprunt de self dans les
        // boucles sur les cases).
        let ids: Vec<u16> = self.nations.iter().map(|n| n.id).collect();
        let index_of = |id: u16| ids.iter().position(|&x| x == id);

        // Besoin par nation = somme, sur ses cases, de la population au-delà de la
        // subsistance (les « bouches » à nourrir par l'agriculture).
        let mut needy = vec![0.0f32; self.nations.len()];
        for t in &self.tiles {
            if let Some(i) = t.owner.and_then(index_of) {
                needy[i] += (t.population - SUBSISTENCE_PER_TILE).max(0.0);
            }
        }
        // Fraction nourrie par nation = min(1, réserve / besoin). On consomme la
        // nourriture due ; tout surplus de réserve est reporté (stock tampon).
        let mut fed_fraction = vec![1.0f32; self.nations.len()];
        for i in 0..self.nations.len() {
            let consumption = (needy[i] / CITIZENS_PER_FOOD).round() as i64;
            if consumption <= 0 {
                continue;
            }
            let food = self.nations[i].food;
            if food >= consumption {
                self.nations[i].food = food - consumption;
            } else {
                self.nations[i].food = 0;
                fed_fraction[i] = (food as f32 / consumption as f32).clamp(0.0, 1.0);
            }
        }
        // Déclin de la population non nourrie (au-delà de la subsistance, par case),
        // proportionnel au déficit. Une famine marque la case (dévastation).
        if fed_fraction.iter().any(|&f| f < 1.0) {
            for t in &mut self.tiles {
                let Some(i) = t.owner.and_then(index_of) else {
                    continue;
                };
                if fed_fraction[i] >= 1.0 {
                    continue;
                }
                let excess = (t.population - SUBSISTENCE_PER_TILE).max(0.0);
                if excess <= 0.0 {
                    continue;
                }
                let unfed = excess * (1.0 - fed_fraction[i]);
                t.population -= unfed * FAMINE_DECLINE;
                t.devastation =
                    (t.devastation + FAMINE_DEVASTATION * (1.0 - fed_fraction[i])).min(1.0);
            }
        }
    }

    /// Bâtit (S8) un bâtiment sur une case possédée et vide, si la nation paie.
    /// La case d'eau `idx` est-elle une **côte** pour `nation` (adjacente à une de
    /// ses terres) — condition pour y bâtir un port ?
    fn is_coastal_for(&self, idx: usize, nation: u16) -> bool {
        let (w, h) = (self.width as i64, self.height as i64);
        let (x, y) = (idx as i64 % w, idx as i64 / w);
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let nx = (x + dx).rem_euclid(w);
            let ny = y + dy;
            if ny < 0 || ny >= h {
                continue;
            }
            let v = (ny * w + nx) as usize;
            if self.tiles[v].kind == TileKind::Land && self.tiles[v].owner == Some(nation) {
                return true;
            }
        }
        false
    }

    fn build(&mut self, x: u32, y: u32, nation: u16, building: Building) -> Vec<Event> {
        if x >= self.width || y >= self.height {
            return reject("hors carte");
        }
        let idx = self.index(x, y);
        if self.tiles[idx].building.is_some() {
            return reject("case déjà bâtie");
        }
        // Le **port** est le SEUL bâtiment constructible sur l'eau : sur une case
        // d'océan **côtière** (adjacente à une terre possédée), qu'il revendique.
        // Tout autre bâtiment exige une terre possédée (rien d'autre sur l'eau).
        if building == Building::Port {
            if self.tiles[idx].kind != TileKind::Ocean {
                return reject("le port se construit sur l'eau (côte)");
            }
            if !self.is_coastal_for(idx, nation) {
                return reject("pas une côte (aucune terre à toi adjacente)");
            }
            if matches!(self.tiles[idx].owner, Some(o) if o != nation) {
                return reject("eau déjà possédée");
            }
        } else {
            if self.tiles[idx].owner != Some(nation) {
                return reject("case non possédée");
            }
            if self.tiles[idx].kind != TileKind::Land {
                return reject("on ne bâtit que sur la terre (sauf les ports)");
            }
        }
        let (money_cost, mat_cost, housing_cost) = build_cost(building);
        let ni = self.ensure_nation(nation);
        if self.nations[ni].money < money_cost
            || self.nations[ni].materials < mat_cost
            || self.nations[ni].housing < housing_cost
        {
            return reject("ressources insuffisantes");
        }
        self.nations[ni].money -= money_cost;
        self.nations[ni].materials -= mat_cost;
        self.nations[ni].housing -= housing_cost;
        self.tiles[idx].owner = Some(nation); // le port revendique la case d'eau
        self.idx_own(nation, idx); // idempotent si la case était déjà possédée
        self.tiles[idx].building = Some(building);
        // Fonder une ville amène des colons (amorce la croissance logistique).
        if building == Building::City && self.tiles[idx].population < CITY_SEED_POP {
            self.tiles[idx].population = CITY_SEED_POP;
        }
        vec![Event::Built {
            x,
            y,
            nation,
            building,
        }]
    }

    /// Démolit le bâtiment d'une case possédée. Rembourse **la moitié du coût ×
    /// l'état de la case** (1 − dévastation) : une case ravagée rend moins. La case
    /// redevient vide (on peut rebâtir autre chose).
    fn demolish(&mut self, x: u32, y: u32, nation: u16) -> Vec<Event> {
        if x >= self.width || y >= self.height {
            return reject("hors carte");
        }
        let idx = self.index(x, y);
        if self.tiles[idx].owner != Some(nation) {
            return reject("case non possédée");
        }
        let Some(building) = self.tiles[idx].building else {
            return reject("aucun bâtiment à démolir");
        };
        let (money_cost, mat_cost, housing_cost) = build_cost(building);
        let intact = (1.0 - self.tiles[idx].devastation).clamp(0.0, 1.0);
        let refund = |c: i64| ((c as f32) * 0.5 * intact).round() as i64;
        let (rm, rmat, rh) = (refund(money_cost), refund(mat_cost), refund(housing_cost));
        let ni = self.ensure_nation(nation);
        self.nations[ni].money += rm;
        self.nations[ni].materials += rmat;
        self.nations[ni].housing += rh;
        self.tiles[idx].building = None;
        vec![Event::Demolished {
            x,
            y,
            building,
            refund: rm,
        }]
    }

    // ---- Unités (S5) -----------------------------------------------------

    /// Index de l'unité présente sur (x, y), s'il y en a une (1 unité/case).
    fn unit_index_at(&self, x: u32, y: u32) -> Option<usize> {
        self.units.iter().position(|u| u.x == x && u.y == y)
    }

    /// Index dans `self.units` d'une unité par id.
    fn unit_index_by_id(&self, id: u32) -> Option<usize> {
        self.units.iter().position(|u| u.id == id)
    }

    /// Y a-t-il déjà une unité sur (x, y) ?
    fn unit_occupied(&self, x: u32, y: u32) -> bool {
        self.units.iter().any(|u| u.x == x && u.y == y)
    }

    /// Distance de Manhattan (X enroulé) entre deux cases.
    fn manhattan(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> u32 {
        let w = self.width as i64;
        let dx = (x0 as i64 - x1 as i64).abs();
        let dx = dx.min(w - dx);
        let dy = (y0 as i64 - y1 as i64).abs();
        (dx + dy) as u32
    }

    /// Bonus de défense (%) du terrain d'une case : végétation + relief + intempéries
    /// (neige/pluie). Quantifié en entier (couche de décision scalaire), borné.
    fn defense_bonus(&self, idx: usize) -> i32 {
        let t = &self.tiles[idx];
        let mut b = (t.vegetation * DEF_VEGETATION) as i32
            + (t.ruggedness * DEF_RUGGEDNESS) as i32
            + (t.precip_now * DEF_WEATHER) as i32;
        if t.temperature < 0.0 {
            b += DEF_SNOW;
        }
        b.clamp(0, DEF_CAP)
    }

    /// Malus d'attaque (%) d'un type d'unité depuis le terrain de SA case (ex. des
    /// archers en forêt, de la cavalerie en terrain accidenté).
    fn attack_malus(&self, kind: UnitKind, idx: usize) -> i32 {
        let t = &self.tiles[idx];
        let s = unit::unit_stats(kind);
        let mut m = 0;
        if t.vegetation > FOREST_VEG {
            m = m.max(s.forest_attack_malus);
        }
        if t.ruggedness > ROUGH_RUG {
            m = m.max(s.rough_attack_malus);
        }
        m
    }

    /// Met à jour l'**occupation** d'une case où se trouve l'unité de `mover` : sa
    /// propre case n'est jamais occupée (réclamée) ; une case ennemie EN GUERRE est
    /// marquée occupée (hachurée), de façon **collante** (rapporte du score jusqu'à
    /// la paix ou la reprise par le propriétaire).
    fn update_occupation(&mut self, idx: usize, mover: u16) {
        match self.tiles[idx].owner {
            Some(o) if o == mover => self.tiles[idx].occupier = None,
            Some(o) if self.diplomacy.at_war(mover, o) => {
                self.tiles[idx].occupier = Some(mover);
            }
            _ => {}
        }
    }

    /// Valeur totale du territoire d'une nation (score de guerre). Public : l'UI
    /// l'affiche pour montrer la progression vers la capitulation.
    pub fn nation_value(&self, nation: u16) -> i64 {
        self.tiles
            .iter()
            .filter(|t| t.owner == Some(nation))
            .map(tile_value)
            .sum()
    }

    /// Score de guerre de `attacker` contre `defender` = valeur des cases de
    /// `defender` qu'`attacker` **occupe** (hachurées). Public (HUD).
    pub fn war_score(&self, attacker: u16, defender: u16) -> i64 {
        self.tiles
            .iter()
            .filter(|t| t.owner == Some(defender) && t.occupier == Some(attacker))
            .map(tile_value)
            .sum()
    }

    /// Capitulations (S5/S6) : pour chaque guerre, si un camp **occupe > 75 % de la
    /// valeur** de l'autre, il **annexe les cases occupées** et la paix est imposée.
    fn resolve_capitulations(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        let wars: Vec<(u16, u16)> = self.diplomacy.wars().to_vec();
        for (a, b) in wars {
            for (att, def) in [(a, b), (b, a)] {
                if !self.diplomacy.at_war(att, def) {
                    continue; // une capitulation a déjà clos cette guerre ce tour
                }
                let total = self.nation_value(def);
                if total <= 0 {
                    continue;
                }
                let score = self.war_score(att, def);
                if score * CAPITULATION_DEN <= total * CAPITULATION_NUM {
                    continue; // pas encore > 75 %
                }
                // Annexion des cases occupées par le vainqueur + paix imposée.
                let mut transferred: Vec<usize> = Vec::new();
                for (i, t) in self.tiles.iter_mut().enumerate() {
                    if t.owner == Some(def) && t.occupier == Some(att) {
                        t.owner = Some(att);
                        t.occupier = None;
                        transferred.push(i);
                    }
                }
                let tiles = transferred.len() as u32;
                for i in transferred {
                    self.idx_disown(def, i);
                    self.idx_own(att, i);
                }
                self.clear_occupations(att, def);
                self.diplomacy.set_war(att, def, false);
                events.push(Event::Capitulation {
                    winner: att,
                    loser: def,
                    tiles,
                    score,
                });
            }
        }
        events
    }

    /// Efface toute occupation croisée entre deux nations (fin de guerre).
    fn clear_occupations(&mut self, a: u16, b: u16) {
        for t in &mut self.tiles {
            if t.occupier == Some(a) && t.owner == Some(b)
                || t.occupier == Some(b) && t.owner == Some(a)
            {
                t.occupier = None;
            }
        }
    }

    /// Recrute une unité (S5) sur une case possédée portant une **caserne**. Coûte
    /// de l'argent (nation) + de la force (de la caserne) ; type gaté par la tech Fer.
    fn create_unit(&mut self, x: u32, y: u32, nation: u16, kind: UnitKind) -> Vec<Event> {
        if x >= self.width || y >= self.height {
            return reject("hors carte");
        }
        let idx = self.index(x, y);
        if self.tiles[idx].owner != Some(nation) {
            return reject("case non possédée");
        }
        let stats = unit::unit_stats(kind);
        // Unité navale -> port ; unité terrestre -> caserne.
        let needed = if stats.naval {
            Building::Port
        } else {
            Building::Military
        };
        if self.tiles[idx].building != Some(needed) {
            return reject(if stats.naval {
                "pas de port sur la case"
            } else {
                "pas de caserne sur la case"
            });
        }
        if self.unit_occupied(x, y) {
            return reject("case déjà occupée par une unité");
        }
        let ni = self.ensure_nation(nation);
        if self.nations[ni].tech[nation::FER] < stats.tech_fer {
            return reject("technologie (Fer) insuffisante pour ce type");
        }
        if self.nations[ni].money < stats.cost_money {
            return reject("argent insuffisant");
        }
        if self.nations[ni].manpower < stats.cost_force {
            return reject("manpower insuffisant");
        }
        self.nations[ni].money -= stats.cost_money;
        self.nations[ni].manpower -= stats.cost_force;
        let id = self.next_unit_id;
        self.next_unit_id += 1;
        self.units.push(Unit {
            id,
            owner: nation,
            kind,
            x,
            y,
            hp: stats.max_hp,
            moves_left: stats.moves,
        });
        vec![Event::UnitCreated {
            unit: id,
            nation,
            kind,
            x,
            y,
        }]
    }

    /// Déplace une unité vers une case atteignable dans ses points de mouvement
    /// (coût terrain + intempéries via la primitive `path`). Pas d'empilement.
    fn move_unit(&mut self, unit_id: u32, tx: u32, ty: u32) -> Vec<Event> {
        if tx >= self.width || ty >= self.height {
            return reject("hors carte");
        }
        let Some(ui) = self.unit_index_by_id(unit_id) else {
            return reject("unité inconnue");
        };
        let (fx, fy, owner, kind, moves_left) = {
            let u = &self.units[ui];
            (u.x, u.y, u.owner, u.kind, u.moves_left)
        };
        if (tx, ty) == (fx, fy) {
            return reject("déjà sur place");
        }
        if self.unit_occupied(tx, ty) {
            return reject("case occupée par une unité");
        }
        let from = self.index(fx, fy);
        let to = self.index(tx, ty);
        // Coût de déplacement selon le DOMAINE de l'unité : navale = eau (terre
        // infranchissable) ; terrestre = terre (eau selon la tech navale).
        let cost = if unit::unit_stats(kind).naval {
            path::reach_cost_with(
                &self.tiles,
                self.width,
                self.height,
                from,
                to,
                moves_left,
                path::naval_move_cost,
            )
        } else {
            let naval = self.nation(owner).map(|n| n.tech[nation::LIEN]).unwrap_or(0);
            path::reach_cost_with(&self.tiles, self.width, self.height, from, to, moves_left, |t| {
                path::unit_move_cost(t, naval)
            })
        };
        // Règle « au moins une case » : si la destination est ADJACENTE et
        // franchissable et que l'unité a TOUS ses points de mouvement (n'a pas
        // encore bougé ce tour), elle peut toujours y entrer en les consommant tous.
        // Sans cela, une météo/dévastation forte (coût d'entrée > points de
        // mouvement) gèlerait l'unité définitivement et aucune armée n'atteindrait
        // jamais l'ennemi (bug de fond de l'agression IA).
        let effective = match cost {
            Some(c) => Some(c),
            None => {
                let full = unit::unit_stats(kind).moves;
                let entry = if unit::unit_stats(kind).naval {
                    path::naval_move_cost(&self.tiles[to])
                } else {
                    let naval = self.nation(owner).map(|n| n.tech[nation::LIEN]).unwrap_or(0);
                    path::unit_move_cost(&self.tiles[to], naval)
                };
                let dxw = {
                    let dx = (fx as i64 - tx as i64).abs();
                    dx.min(self.width as i64 - dx)
                };
                let adjacent = dxw + (fy as i64 - ty as i64).abs() == 1;
                if moves_left == full && entry != u32::MAX && adjacent {
                    Some(moves_left) // une case, tout le mouvement consommé
                } else {
                    None
                }
            }
        };
        match effective {
            Some(c) => {
                {
                    let u = &mut self.units[ui];
                    u.x = tx;
                    u.y = ty;
                    u.moves_left = u.moves_left.saturating_sub(c);
                }
                // Occupation (S5) : entrer sur une case ennemie en guerre la marque.
                self.update_occupation(to, owner);
                vec![Event::UnitMoved {
                    unit: unit_id,
                    to_x: tx,
                    to_y: ty,
                    cost: c,
                }]
            }
            None => reject("hors de portée (points de mouvement)"),
        }
    }

    /// Embarque une unité terrestre `unit` sur une **galère** `transport` adjacente.
    fn embark(&mut self, unit_id: u32, transport_id: u32) -> Vec<Event> {
        let Some(ti) = self.unit_index_by_id(transport_id) else {
            return reject("galère inconnue");
        };
        let (gx, gy, gowner, gkind) = {
            let g = &self.units[ti];
            (g.x, g.y, g.owner, g.kind)
        };
        let cap = unit::unit_stats(gkind).capacity;
        if cap == 0 {
            return reject("ce n'est pas un transport");
        }
        let Some(li) = self.unit_index_by_id(unit_id) else {
            return reject("unité inconnue");
        };
        let (lx, ly, lowner, lkind, lhp) = {
            let u = &self.units[li];
            (u.x, u.y, u.owner, u.kind, u.hp)
        };
        if lowner != gowner {
            return reject("unité d'une autre nation");
        }
        if unit::unit_stats(lkind).naval {
            return reject("une unité navale ne s'embarque pas");
        }
        if self.manhattan(lx, ly, gx, gy) > 1 {
            return reject("la galère n'est pas adjacente");
        }
        if self.cargo.get(&transport_id).map_or(0, |v| v.len()) as u8 >= cap {
            return reject("galère pleine");
        }
        self.cargo
            .entry(transport_id)
            .or_default()
            .push(CarriedUnit { kind: lkind, hp: lhp });
        self.units.retain(|u| u.id != unit_id); // l'unité quitte la carte
        vec![Event::Embarked {
            unit: unit_id,
            transport: transport_id,
        }]
    }

    /// Débarque une unité transportée sur une case de **terre** adjacente (libre).
    fn disembark(&mut self, transport_id: u32, tx: u32, ty: u32) -> Vec<Event> {
        if tx >= self.width || ty >= self.height {
            return reject("hors carte");
        }
        let Some(ti) = self.unit_index_by_id(transport_id) else {
            return reject("galère inconnue");
        };
        let (gx, gy, owner) = {
            let g = &self.units[ti];
            (g.x, g.y, g.owner)
        };
        if self.tiles[self.index(tx, ty)].kind != TileKind::Land {
            return reject("on débarque sur la terre");
        }
        if self.manhattan(gx, gy, tx, ty) != 1 {
            return reject("case non adjacente à la galère");
        }
        if self.unit_occupied(tx, ty) {
            return reject("case occupée par une unité");
        }
        let (carried, now_empty) = {
            let Some(load) = self.cargo.get_mut(&transport_id) else {
                return reject("galère vide");
            };
            match load.pop() {
                Some(c) => (c, load.is_empty()),
                None => return reject("galère vide"),
            }
        };
        if now_empty {
            self.cargo.remove(&transport_id);
        }
        let stats = unit::unit_stats(carried.kind);
        let id = self.next_unit_id;
        self.next_unit_id += 1;
        self.units.push(Unit {
            id,
            owner,
            kind: carried.kind,
            x: tx,
            y: ty,
            hp: carried.hp.min(stats.max_hp),
            moves_left: 0,
        });
        let to = self.index(tx, ty);
        self.update_occupation(to, owner); // débarquer sur une terre ennemie l'occupe
        vec![Event::Disembarked {
            transport: transport_id,
            kind: carried.kind,
            x: tx,
            y: ty,
        }]
    }

    /// Attaque avec une unité une case à portée contenant une unité ENNEMIE (guerre
    /// requise). Dégâts = base × (1 − malus terrain attaquant) ÷ (1 + défense terrain
    /// défenseur) ; riposte au corps à corps si le défenseur survit.
    fn attack_unit(&mut self, unit_id: u32, tx: u32, ty: u32) -> Vec<Event> {
        if tx >= self.width || ty >= self.height {
            return reject("hors carte");
        }
        let Some(ai) = self.unit_index_by_id(unit_id) else {
            return reject("unité inconnue");
        };
        let (ax, ay, attacker_owner, akind) = {
            let u = &self.units[ai];
            (u.x, u.y, u.owner, u.kind)
        };
        let Some(di) = self.unit_index_at(tx, ty) else {
            return reject("aucune unité cible");
        };
        let (defender_id, defender_owner, dkind) = {
            let u = &self.units[di];
            (u.id, u.owner, u.kind)
        };
        if defender_owner == attacker_owner {
            return reject("cible alliée");
        }
        if !self.diplomacy.at_war(attacker_owner, defender_owner) {
            return reject("pas en guerre avec la cible");
        }
        let astats = unit::unit_stats(akind);
        if self.manhattan(ax, ay, tx, ty) > astats.range {
            return reject("cible hors de portée");
        }
        let a_tile = self.index(ax, ay);
        let d_tile = self.index(tx, ty);
        // Coup de l'attaquant.
        let malus = self.attack_malus(akind, a_tile);
        let def = self.defense_bonus(d_tile);
        let dealt = ((astats.damage * (100 - malus) / 100) * 100 / (100 + def)).max(1);
        self.units[di].hp -= dealt;
        let killed = self.units[di].hp <= 0;
        // Riposte au corps à corps (échange adjacent, défenseur survivant).
        let mut counter = 0;
        if !killed && self.manhattan(ax, ay, tx, ty) == 1 {
            let dstats = unit::unit_stats(dkind);
            let dmalus = self.attack_malus(dkind, d_tile);
            let adef = self.defense_bonus(a_tile);
            counter = ((dstats.damage * (100 - dmalus) / 100) * 100 / (100 + adef)).max(1);
            self.units[ai].hp -= counter;
        }
        // L'attaque épuise le mouvement de l'attaquant.
        self.units[ai].moves_left = 0;

        let mut events = vec![Event::UnitAttacked {
            attacker: unit_id,
            defender: defender_id,
            x: tx,
            y: ty,
            damage: dealt,
            counter,
            killed,
        }];
        if killed {
            self.units.retain(|u| u.id != defender_id);
            self.cargo.remove(&defender_id); // une galère coulée perd son cargo
            events.push(Event::UnitDestroyed {
                unit: defender_id,
                x: tx,
                y: ty,
            });
        }
        // L'attaquant a pu succomber à la riposte.
        if let Some(ai2) = self.unit_index_by_id(unit_id) {
            if self.units[ai2].hp <= 0 {
                let (ux, uy) = (self.units[ai2].x, self.units[ai2].y);
                self.units.retain(|u| u.id != unit_id);
                self.cargo.remove(&unit_id);
                events.push(Event::UnitDestroyed {
                    unit: unit_id,
                    x: ux,
                    y: uy,
                });
            }
        }
        events
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
            fnv_u32(&mut h, t.owner.map(|o| o as u32 + 1).unwrap_or(0));
            fnv_u32(&mut h, t.occupier.map(|o| o as u32 + 1).unwrap_or(0));
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
                Some(proto::Building::Farm) => 8,
                Some(proto::Building::City) => 9,
                Some(proto::Building::Port) => 10,
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
            h ^= n.housing as u64;
            h = h.wrapping_mul(FNV_PRIME);
            h ^= n.food as u64;
            h = h.wrapping_mul(FNV_PRIME);
            h ^= n.manpower as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        // Unités (S5) : ordre de `units` (création), stable et rejouable.
        for u in &self.units {
            fnv_u32(&mut h, u.id);
            fnv_u32(&mut h, u.owner as u32);
            h ^= unit::kind_code(u.kind);
            h = h.wrapping_mul(FNV_PRIME);
            fnv_u32(&mut h, u.x);
            fnv_u32(&mut h, u.y);
            fnv_u32(&mut h, u.hp as u32);
            fnv_u32(&mut h, u.moves_left);
        }
        // Cargo des galères (BTreeMap → ordre des clés, déterministe).
        for (gid, load) in &self.cargo {
            fnv_u32(&mut h, *gid);
            for c in load {
                h ^= unit::kind_code(c.kind);
                fnv_u32(&mut h, c.hp as u32);
                h = h.wrapping_mul(FNV_PRIME);
            }
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

/// Coût de construction (argent, matériaux, habitation) d'un bâtiment (S8,
/// single-source). Seule la **ville** coûte de l'habitation (logements à bâtir).
pub fn build_cost(b: Building) -> (i64, i64, i64) {
    match b {
        Building::City => (100, 0, 50),
        Building::Industry => (100, 0, 0),
        Building::Commerce => (120, 20, 0),
        Building::Infrastructure => (40, 20, 0),
        Building::Education => (150, 30, 0),
        Building::Military => (120, 40, 0),
        Building::Farm => (80, 15, 0),
        Building::Port => (100, 30, 0),
    }
}

/// Valeur d'une case pour le **score de guerre** : vide 1, bâtiment 5, ville 10.
fn tile_value(t: &Tile) -> i64 {
    match t.building {
        Some(Building::City) => TILE_VALUE_CITY,
        Some(_) => TILE_VALUE_BUILDING,
        None => TILE_VALUE_EMPTY,
    }
}

/// Matériaux/mois produits par une industrie : ∝ stats de case (sol, végétation,
/// intempéries) × main-d'œuvre connectée × (1 − dévastation). Entier (déterminisme).
fn industry_output(t: &Tile, connected_pop: f32) -> i64 {
    let terrain = (t.soil_fertility + t.vegetation + t.precip_now) / 3.0;
    let workforce = (connected_pop / INDUSTRY_WORKFORCE).min(1.0);
    industry_yield(terrain, workforce, t.devastation)
}

/// Rendement d'industrie à partir des scalaires (pur, testable, formule gelée).
pub(crate) fn industry_yield(terrain: f32, workforce: f32, dev: f32) -> i64 {
    (INDUSTRY_BASE * terrain * workforce * (1.0 - dev))
        .max(0.0)
        .round() as i64
}

/// Nourriture/mois d'une ferme : ∝ terrain agricole (sol, humidité/pluie, chaleur)
/// × main-d'œuvre connectée × (1 − dévastation). Entier (déterminisme).
fn farm_output(t: &Tile, connected_pop: f32) -> i64 {
    let warmth = ((t.temperature + 10.0) / 40.0).clamp(0.0, 1.0);
    let terrain = (t.soil_fertility + t.precip_now + warmth) / 3.0;
    let workforce = (connected_pop / INDUSTRY_WORKFORCE).min(1.0);
    farm_yield(terrain, workforce, t.devastation)
}

/// Rendement de ferme à partir des scalaires (pur, testable, formule gelée).
pub(crate) fn farm_yield(terrain: f32, workforce: f32, dev: f32) -> i64 {
    (FARM_BASE * terrain * workforce * (1.0 - dev))
        .max(0.0)
        .round() as i64
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

/// Met à jour la météo + végétation d'**une** case (extrait pour partager entre la
/// passe météo séquentielle et parallèle). Pur : ne dépend que de la case et de
/// (idx, mois, graine météo) → même résultat quel que soit le thread/cœur.
fn update_tile_weather(t: &mut Tile, idx: usize, width: usize, height: u32, month: u8, weather_seed: u64) {
    let x = (idx % width) as i64;
    let y = (idx / width) as i64;
    let v = y as f32 / height as f32;
    let lat = (v - 0.5).abs() * 2.0;
    let north = v < 0.5;
    let wn = noise_signed(weather_seed, x, y);
    climate::update_weather(t, month, lat, north, wn);
    if t.kind == TileKind::Land {
        let target = worldgen::vegetation_target(t.kind, t.mean_temperature, t.precipitation);
        t.vegetation += (target - t.vegetation) * 0.05;
    }
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

    // Formules économiques GELÉES (CLAUDE.md : figer les formules partagées avant
    // leurs dépendants). Tout changement ici est intentionnel et re-béni.
    #[test]
    fn industry_yield_frozen() {
        assert_eq!(industry_yield(1.0, 1.0, 0.0), 8); // INDUSTRY_BASE
        assert_eq!(industry_yield(0.5, 0.5, 0.0), 2); // 8*0.5*0.5
        assert_eq!(industry_yield(1.0, 1.0, 0.5), 4); // dévastation 50 %
        assert_eq!(industry_yield(0.0, 1.0, 0.0), 0); // terrain nul -> rien
        assert_eq!(industry_yield(1.0, 0.0, 0.0), 0); // sans main-d'œuvre -> rien
    }

    #[test]
    fn build_costs_frozen() {
        assert_eq!(build_cost(Building::City), (100, 0, 50));
        assert_eq!(build_cost(Building::Industry), (100, 0, 0));
        assert_eq!(build_cost(Building::Commerce), (120, 20, 0));
        assert_eq!(build_cost(Building::Infrastructure), (40, 20, 0));
        assert_eq!(build_cost(Building::Education), (150, 30, 0));
        assert_eq!(build_cost(Building::Military), (120, 40, 0));
        assert_eq!(build_cost(Building::Farm), (80, 15, 0));
        assert_eq!(build_cost(Building::Port), (100, 30, 0));
    }

    #[test]
    fn farm_yield_frozen() {
        assert_eq!(farm_yield(1.0, 1.0, 0.0), 12); // FARM_BASE
        assert_eq!(farm_yield(0.5, 1.0, 0.0), 6);
        assert_eq!(farm_yield(1.0, 0.0, 0.0), 0); // sans main-d'œuvre -> rien
    }
}
