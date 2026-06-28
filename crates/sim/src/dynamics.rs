//! Dynamiques anthropiques (système S1) : capacité de charge, croissance de la
//! population (logistique), croissance du développement, famine/dévastation.
//!
//! Tout est en fonctions pures sur une case (sauf l'écriture finale), pour rester
//! déterministe et testable. La **formule de capacité est centralisée ici** : elle
//! est partagée par la population, l'essaimage et le savoir (cf. `CLAUDE.md`).

use crate::tile::{Tile, TileKind};

/// Plafond de population d'une excellente case (avant bonus dev/tech).
pub const MAX_POP_PER_TILE: f32 = 5000.0;

/// Taux de croissance mensuel de base (logistique).
const GROWTH_RATE: f32 = 0.08;

/// Capacité de charge d'une case, dérivée du terrain + développement + tech Terroir.
/// Fonction PURE (jamais stockée) — l'unique source de vérité du plafond.
pub fn carrying_capacity(tile: &Tile, terroir_tier: u8) -> f32 {
    if tile.kind != TileKind::Land {
        return 0.0;
    }
    let warmth = ((tile.mean_temperature + 10.0) / 40.0).clamp(0.0, 1.0);
    let quality = (tile.soil_fertility * 0.5 + tile.precipitation * 0.3 + warmth * 0.2)
        * (1.0 - 0.4 * tile.ruggedness);
    let dev_bonus = 1.0 + 0.5 * tile.development;
    let tech_mult = 1.0 + 0.25 * terroir_tier as f32;
    (quality.clamp(0.0, 1.0) * MAX_POP_PER_TILE * dev_bonus * tech_mult).max(0.0)
}

/// Croissance logistique de la population vers la capacité ; surpopulation => famine.
pub fn grow_population(tile: &mut Tile, capacity: f32) {
    if capacity <= 0.0 {
        tile.population *= 0.9; // pas de support : déclin
        if tile.population < 0.01 {
            tile.population = 0.0;
        }
        return;
    }
    let delta = GROWTH_RATE * tile.population * (1.0 - tile.population / capacity);
    tile.population = (tile.population + delta).max(0.0);
    if tile.population > capacity {
        let over = (tile.population - capacity) / capacity;
        tile.devastation = (tile.devastation + 0.1 * over).clamp(0.0, 1.0);
        tile.population = capacity;
    }
}

/// Croissance du développement : dépend de la pop locale, des voisins, du terrain
/// et de la météo (cf. formule de `docs/GAMEPLAY.md` S1).
pub fn grow_development(tile: &mut Tile, local_pop: f32, neighbor_pop: f32) {
    let support = (local_pop + 0.25 * neighbor_pop) / 1000.0;
    let terrain = (tile.soil_fertility * 0.5 + (1.0 - tile.ruggedness) * 0.5).clamp(0.0, 1.0);
    let weather = ((tile.temperature + 10.0) / 40.0).clamp(0.0, 1.0);
    let target = (support * terrain * (0.5 + 0.5 * weather)).clamp(0.0, 1.0);
    tile.development += (target - tile.development) * 0.05;
    // La dévastation érode lentement le développement.
    tile.development = (tile.development - tile.devastation * 0.01).clamp(0.0, 1.0);
}
