//! Génération procédurale du monde (déterministe, seedée).
//!
//! Construit les couches géologie + climat + biosphère initiale de chaque case,
//! à partir d'un bruit fractal enroulé sur X (monde cylindrique).

use crate::noise::fbm;
use crate::tile::{Biome, Tile, TileKind};

/// Seuil d'altitude (0..1) séparant océan et terre.
pub const SEA_LEVEL: f32 = 0.5;

/// Résultat de génération : les cases + des compteurs d'audit.
pub struct GenOutcome {
    pub tiles: Vec<Tile>,
    pub land: u32,
    pub ocean: u32,
}

/// Génère un monde `width` × `height` pour une graine donnée.
pub fn generate(seed: u64, width: u32, height: u32) -> GenOutcome {
    let mut tiles = Vec::with_capacity(width as usize * height as usize);
    let mut land = 0u32;
    let mut ocean = 0u32;

    let elev_seed = seed ^ 0x1111_1111_1111_1111;
    let precip_seed = seed ^ 0x2222_2222_2222_2222;
    let rugged_seed = seed ^ 0x3333_3333_3333_3333;

    for y in 0..height {
        let v = y as f32 / height as f32;
        let lat = (v - 0.5).abs() * 2.0; // 0 à l'équateur, 1 aux pôles
        for x in 0..width {
            let u = x as f32 / width as f32;

            let elevation = fbm(elev_seed, u, v, 6, 6, 4);
            let kind = if elevation < SEA_LEVEL {
                TileKind::Ocean
            } else {
                TileKind::Land
            };

            // Hauteur terrestre normalisée (0 au littoral, 1 au sommet).
            let above = ((elevation - SEA_LEVEL).max(0.0)) / (1.0 - SEA_LEVEL);
            let mean_temperature = 30.0 - 55.0 * lat - 25.0 * above;

            let precipitation = {
                let n = fbm(precip_seed, u, v, 5, 5, 3);
                (n * 0.7 + (1.0 - lat) * 0.3).clamp(0.0, 1.0)
            };
            let ruggedness = fbm(rugged_seed, u, v, 3, 24, 16);

            let biome = classify_biome(kind, mean_temperature, precipitation);
            let veg_target = vegetation_target(kind, mean_temperature, precipitation);

            let soil_fertility = if kind == TileKind::Land {
                (precipitation * 0.6 + (1.0 - ruggedness) * 0.4).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let wildlife = if kind == TileKind::Land {
                veg_target
            } else {
                0.0
            };
            let marine_life = if kind == TileKind::Ocean {
                ((1.0 - lat) * 0.5 + 0.5 * (elevation / SEA_LEVEL).min(1.0)).clamp(0.0, 1.0)
            } else {
                0.0
            };

            match kind {
                TileKind::Land => land += 1,
                TileKind::Ocean => ocean += 1,
            }

            tiles.push(Tile {
                kind,
                elevation,
                ruggedness,
                mean_temperature,
                precipitation,
                biome,
                vegetation: veg_target * 0.5, // partira de la moitié et croîtra
                soil_fertility,
                wildlife,
                marine_life,
                temperature: mean_temperature,
                precip_now: precipitation,
                owner: None,
                occupier: None,
                population: 0.0,
                development: 0.0,
                devastation: 0.0,
                building: None,
            });
        }
    }

    GenOutcome { tiles, land, ocean }
}

/// Cible de couvert végétal selon le climat (vers laquelle la biosphère tend).
pub fn vegetation_target(kind: TileKind, temp: f32, precip: f32) -> f32 {
    if kind != TileKind::Land {
        return 0.0;
    }
    // Chaud + humide => dense ; froid ou sec => clairsemé.
    let warmth = ((temp + 10.0) / 40.0).clamp(0.0, 1.0); // -10°C..30°C -> 0..1
    (warmth * precip).clamp(0.0, 1.0)
}

/// Classe le biome à partir de la température et des précipitations.
fn classify_biome(kind: TileKind, temp: f32, precip: f32) -> Biome {
    if kind == TileKind::Ocean {
        return Biome::Ocean;
    }
    if temp < -5.0 {
        Biome::Ice
    } else if temp < 0.0 {
        Biome::Tundra
    } else if temp < 7.0 {
        Biome::Boreal
    } else if temp < 20.0 {
        if precip < 0.3 {
            Biome::Grassland
        } else {
            Biome::TemperateForest
        }
    } else if precip < 0.2 {
        Biome::Desert
    } else if precip < 0.5 {
        Biome::Savanna
    } else {
        Biome::TropicalForest
    }
}
