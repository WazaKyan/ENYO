//! Le modèle de **case**, organisé en couches (cf. `PLAN.md` §4.2) selon leur
//! fréquence de changement : géologie (statique) · climat (lent) · biosphère
//! (dynamique lente) · météo (chaque tour) · anthropique (Phase 2).

use serde::{Deserialize, Serialize};

/// Nature de la case. (Phase 1 : terre ou océan ; lacs/côtes viendront plus tard.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TileKind {
    Ocean,
    Land,
}

/// Biome dérivé du climat (classification de type Whittaker, simplifiée).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Biome {
    Ocean,
    Ice,
    Tundra,
    Boreal,
    Grassland,
    Desert,
    TemperateForest,
    Savanna,
    TropicalForest,
}

/// Saison, dérivée du mois.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Season {
    Winter,
    Spring,
    Summer,
    Autumn,
}

impl Season {
    /// Saison (hémisphère nord) pour un mois 1..=12.
    pub fn from_month(month: u8) -> Season {
        match month {
            12 | 1 | 2 => Season::Winter,
            3..=5 => Season::Spring,
            6..=8 => Season::Summer,
            _ => Season::Autumn,
        }
    }
}

/// Une case du monde, avec toutes ses couches de stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tile {
    // --- Géologie (statique) ---
    pub kind: TileKind,
    /// Altitude normalisée 0..1 (le niveau de la mer est un seuil, cf. worldgen).
    pub elevation: f32,
    /// Relief / rugosité 0..1 (indépendant de l'altitude).
    pub ruggedness: f32,

    // --- Climat (lent / annuel) ---
    /// Température moyenne annuelle (°C).
    pub mean_temperature: f32,
    /// Précipitations annuelles 0..1.
    pub precipitation: f32,
    pub biome: Biome,

    // --- Biosphère (dynamique lente) ---
    /// Couvert végétal 0..1 (évolue vers une cible climatique).
    pub vegetation: f32,
    /// Fertilité du sol 0..1.
    pub soil_fertility: f32,
    /// Faune terrestre 0..1.
    pub wildlife: f32,
    /// Faune marine 0..1 (cases d'eau).
    pub marine_life: f32,

    // --- Météo (dynamique, chaque tour) ---
    /// Température courante (°C).
    pub temperature: f32,
    /// Précipitations courantes 0..1.
    pub precip_now: f32,

    // --- Anthropique (Phase 2 — réservé, à 0 pour l'instant) ---
    pub population: f32,
    pub development: f32,
    pub devastation: f32,
}
