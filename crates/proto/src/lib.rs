//! Le « langage » du jeu : commandes (entrées) et événements (sorties).
//!
//! Tout changement d'état de la simulation passe par une [`Command`] qui produit
//! des [`Event`]. Le journal des événements suffit à rejouer une partie
//! (event-sourcing). Ces types sont volontairement minimalistes pour la Phase 0.

use serde::{Deserialize, Serialize};

/// Une action demandée à la simulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Avance la partie d'un tour (un mois de jeu).
    Step,
}

/// Un fait advenu dans la simulation, produit par l'application d'une [`Command`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// Un tour s'est écoulé. `turn` est le numéro du nouveau tour ; `roll` est un
    /// tirage déterministe du RNG (présent surtout pour prouver le déterminisme).
    TurnAdvanced { turn: u64, roll: u64 },
}
