//! Cœur de simulation d'ENYO : le monde (grille de cases) et l'application des
//! commandes. Pur, déterministe, headless (cf. principes dans `CLAUDE.md`).
//!
//! L'unique façon de modifier l'état est [`World::apply`], qui transforme une
//! [`Command`] en [`Event`]s (event-sourcing). Chaque événement embarque un
//! `checksum` du monde : l'audit du déterminisme se fait depuis le seul journal.

pub mod climate;
pub mod noise;
pub mod rng;
pub mod tile;
pub mod worldgen;

use proto::{Command, Event};
use rng::Rng;
use serde::{Deserialize, Serialize};
use tile::{Tile, TileKind};

/// L'état complet de la partie — entièrement reconstructible depuis une graine
/// et une suite de commandes (donc rejouable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct World {
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub turn: u64,
    pub land_tiles: u32,
    pub ocean_tiles: u32,
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
            rng: Rng::new(seed),
            tiles: gen.tiles,
        }
    }

    /// Référence vers la case (x, y).
    pub fn tile(&self, x: u32, y: u32) -> &Tile {
        &self.tiles[y as usize * self.width as usize + x as usize]
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

    /// Applique une commande, met à jour l'état, et renvoie les événements produits.
    /// UNIQUE porte d'entrée pour modifier le monde.
    pub fn apply(&mut self, command: Command) -> Vec<Event> {
        match command {
            Command::Step => self.resolve_turn(),
        }
    }

    /// Résout un tour (un mois) : météo de chaque case + dérive lente de la biosphère.
    fn resolve_turn(&mut self) -> Vec<Event> {
        self.turn += 1;
        let month = climate::month_of(self.turn);
        let weather_seed = self.rng.next_u64();

        let width = self.width;
        let height = self.height;
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

    /// Checksum déterministe de l'état du monde (FNV-1a sur les champs clés).
    /// Sert d'empreinte d'audit : deux runs identiques ⇒ mêmes checksums.
    pub fn checksum(&self) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        h ^= self.turn;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        for t in &self.tiles {
            fnv_u32(&mut h, t.elevation.to_bits());
            fnv_u32(&mut h, t.mean_temperature.to_bits());
            fnv_u32(&mut h, t.temperature.to_bits());
            fnv_u32(&mut h, t.precip_now.to_bits());
            fnv_u32(&mut h, t.vegetation.to_bits());
            h ^= match t.kind {
                TileKind::Ocean => 1,
                TileKind::Land => 2,
            };
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
}

/// Mélange FNV-1a d'un `u32` dans l'accumulateur de checksum.
fn fnv_u32(h: &mut u64, val: u32) {
    for b in val.to_le_bytes() {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x0000_0100_0000_01b3);
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
}
