//! Les nations : acteurs qui possèdent des cases, accumulent du savoir et
//! débloquent un arbre de technologie à 4 branches.

use serde::{Deserialize, Serialize};

/// Indices des branches de l'arbre de tech (cf. `docs/GAMEPLAY.md`).
pub const ESSOR: usize = 0; // portée d'essaimage
pub const TERROIR: usize = 1; // capacité de charge
pub const FER: usize = 2; // militaire (Phase 4+)
pub const LIEN: usize = 3; // naval / liens (franchir l'eau)

/// Nombre de branches.
pub const BRANCHES: usize = 4;

/// Une nation (le joueur est la nation 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Nation {
    pub id: u16,
    /// Savoir accumulé, dépensé pour la recherche.
    pub knowledge: f32,
    /// Palier atteint dans chaque branche (Essor, Terroir, Fer, Lien).
    pub tech: [u8; BRANCHES],
}

impl Nation {
    pub fn new(id: u16) -> Self {
        Self {
            id,
            knowledge: 0.0,
            tech: [0; BRANCHES],
        }
    }
}
