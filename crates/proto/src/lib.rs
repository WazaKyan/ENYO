//! Le « langage » du jeu : commandes (entrées) et événements (sorties).
//!
//! Tout changement d'état passe par une [`Command`] qui produit des [`Event`].
//! Le journal JSONL suffit à auditer et rejouer une partie (event-sourcing).
//! Chaque événement de tour porte un `checksum` déterministe du monde, et les
//! commandes rejetées sont elles aussi loguées (audit complet).

use serde::{Deserialize, Serialize};

/// Une action demandée à la simulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Avance la partie d'un tour (un mois de jeu).
    Step,
    /// Implante une population de départ d'une nation sur une case de terre.
    Settle {
        x: u32,
        y: u32,
        nation: u16,
        population: u32,
    },
    /// Essaimage : déplace la moitié de la population d'une case vers une cible
    /// atteignable (selon la portée technologique). Source ≥ 1000 requis.
    Swarm {
        from_x: u32,
        from_y: u32,
        to_x: u32,
        to_y: u32,
    },
    /// Investit le savoir d'une nation dans une branche de l'arbre de tech (0..4).
    Research { nation: u16, branch: u8 },
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
    /// Une population de départ a été implantée.
    Settled {
        nation: u16,
        x: u32,
        y: u32,
        population: u32,
    },
    /// Un essaimage a eu lieu (population déplacée vers la cible).
    Swarmed {
        nation: u16,
        from_x: u32,
        from_y: u32,
        to_x: u32,
        to_y: u32,
        moved: f32,
    },
    /// Une technologie a été débloquée (nouveau palier d'une branche).
    Researched { nation: u16, branch: u8, tier: u8 },
    /// Une commande a été rejetée — logué pour l'audit (rien n'est silencieux).
    CommandRejected { reason: String },
}
