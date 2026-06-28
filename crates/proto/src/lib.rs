//! Le « langage » du jeu : commandes (entrées) et événements (sorties).
//!
//! Tout changement d'état de la simulation passe par une [`Command`] qui produit
//! des [`Event`]. Le journal des événements (JSONL) suffit à auditer et à rejouer
//! une partie (event-sourcing). Chaque événement porte un `checksum` déterministe
//! du monde : le déterminisme est ainsi vérifiable depuis le seul journal.

use serde::{Deserialize, Serialize};

/// Une action demandée à la simulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Avance la partie d'un tour (un mois de jeu).
    Step,
}

/// Un fait advenu dans la simulation, produit par l'application d'une [`Command`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// Le monde a été généré. Résumé + `checksum` (audit de reproductibilité).
    WorldGenerated {
        seed: u64,
        width: u32,
        height: u32,
        land_tiles: u32,
        ocean_tiles: u32,
        checksum: u64,
    },
    /// Un tour (mois) a été résolu. Agrégats + `checksum` (audit de déterminisme).
    TurnResolved {
        turn: u64,
        month: u8,
        avg_temperature: f32,
        avg_vegetation: f32,
        checksum: u64,
    },
}
