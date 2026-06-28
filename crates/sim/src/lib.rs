//! Cœur de simulation d'ENYO : le monde (grille de cases + nations) et
//! l'application des commandes. Pur, déterministe, headless (cf. `CLAUDE.md`).
//!
//! L'unique façon de modifier l'état est [`World::apply`], qui transforme une
//! [`Command`] en [`Event`]s (event-sourcing). Chaque tour embarque un `checksum`
//! du monde et toute commande rejetée est loguée : l'audit se fait depuis le
//! seul journal.

pub mod climate;
pub mod dynamics;
pub mod nation;
pub mod noise;
pub mod path;
pub mod province;
pub mod rng;
pub mod tile;
pub mod worldgen;

use nation::Nation;
use proto::{Command, Event};
use rng::Rng;
use serde::{Deserialize, Serialize};
use tile::{Tile, TileKind};

/// Constante FNV-1a (prime 64 bits) pour le checksum d'audit.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

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
            rng: Rng::new(seed),
            tiles: gen.tiles,
        }
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

    /// Implante une population de départ (S2 — implantation).
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
                return reject("cible possédée par une autre nation");
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

    /// Résout un tour : météo + biosphère, puis dynamiques anthropiques (S1).
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

        // Passe 2 — anthropique (capacité, population, développement, savoir).
        self.resolve_anthropic();

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

    /// Dynamiques anthropiques d'un tour (S1). Les deltas dépendent des populations
    /// du DÉBUT de tour (`old_pop`) pour être indépendants de l'ordre de parcours.
    fn resolve_anthropic(&mut self) {
        if self.nations.is_empty() {
            return;
        }
        let width = self.width;
        let height = self.height;
        let old_pop: Vec<f32> = self.tiles.iter().map(|t| t.population).collect();
        let mut knowledge_gain = vec![0.0f32; self.nations.len()];

        for y in 0..height {
            for x in 0..width {
                let idx = y as usize * width as usize + x as usize;
                let pop = old_pop[idx];
                let owner = self.tiles[idx].owner;
                if pop <= 0.0 && owner.is_none() {
                    continue;
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
                    knowledge_gain[i] += dev * (newpop / 1000.0).min(1.0) * 0.1;
                }
            }
        }
        for (i, g) in knowledge_gain.iter().enumerate() {
            self.nations[i].knowledge += g;
        }
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
            fnv_u32(&mut h, t.temperature.to_bits());
            fnv_u32(&mut h, t.precip_now.to_bits());
            fnv_u32(&mut h, t.vegetation.to_bits());
            fnv_u32(&mut h, t.population.to_bits());
            fnv_u32(&mut h, t.development.to_bits());
            fnv_u32(&mut h, t.devastation.to_bits());
            fnv_u32(&mut h, t.owner.map(|o| o as u32 + 1).unwrap_or(0));
            h ^= match t.kind {
                TileKind::Ocean => 1,
                TileKind::Land => 2,
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
        }
        h
    }
}

/// Coût en savoir pour passer du palier `tier` au suivant.
fn tech_cost(tier: u8) -> f32 {
    50.0 * (tier as f32 + 1.0)
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
