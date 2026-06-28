//! Météo & saisons : la couche qui change **à chaque tour** (mois).
//!
//! Déterministe et sans transcendantes : le cycle saisonnier est une table de 12
//! facteurs (≈ sinusoïde), inversée pour l'hémisphère sud.

use crate::tile::Tile;

/// Facteur saisonnier par mois (1..=12), hémisphère nord. +1 = plein été, -1 = plein hiver.
const MONTHLY: [f32; 12] = [
    -1.0, -0.87, -0.5, 0.0, 0.5, 0.87, 1.0, 0.87, 0.5, 0.0, -0.5, -0.87,
];

/// Mois (1..=12) correspondant à un numéro de tour (1 tour = 1 mois).
pub fn month_of(turn: u64) -> u8 {
    (((turn - 1) % 12) + 1) as u8
}

/// Amplitude saisonnière (°C) : faible à l'équateur, forte aux hautes latitudes.
fn seasonal_amplitude(lat: f32) -> f32 {
    4.0 + 16.0 * lat
}

/// Met à jour la météo (température + précip courantes) d'une case pour un mois.
///
/// `noise` est un petit bruit déterministe propre au couple (tour, case), ~[-1, 1].
pub fn update_weather(tile: &mut Tile, month: u8, lat: f32, hemisphere_north: bool, noise: f32) {
    let factor = MONTHLY[(month - 1) as usize];
    let signed = if hemisphere_north { factor } else { -factor };
    let amp = seasonal_amplitude(lat);
    tile.temperature = tile.mean_temperature + signed * amp + noise * 2.0;
    tile.precip_now = (tile.precipitation + noise * 0.1).clamp(0.0, 1.0);
}
