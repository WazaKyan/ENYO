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

/// Stock d'argent au départ d'une nation (S8 — de quoi bâtir ses premières cases).
pub const STARTING_MONEY: i64 = 500;

/// Une nation (le joueur est la nation 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Nation {
    pub id: u16,
    /// Savoir/science accumulé, dépensé pour la recherche (S3 ; alimenté par
    /// l'éducation en S8, + un flux de base par densité).
    pub knowledge: f32,
    /// Palier atteint dans chaque branche (Essor, Terroir, Fer, Lien).
    pub tech: [u8; BRANCHES],

    // --- Ressources S8 (économie interne), entières → déterminisme sans dérive ---
    /// Argent : bâtir + entretien mensuel.
    pub money: i64,
    /// Matériaux : produits par l'industrie, consommés par le commerce / la construction.
    pub materials: i64,
    /// Influence : +1/mois de base ; étendre le territoire.
    pub influence: i64,
    /// Habitation : produite par le commerce ; bâtir des villes (E6).
    pub housing: i64,
    /// Nourriture : produite par les fermes ; nourrit la population (E6).
    pub food: i64,
}

impl Nation {
    pub fn new(id: u16) -> Self {
        Self {
            id,
            knowledge: 0.0,
            tech: [0; BRANCHES],
            money: STARTING_MONEY,
            materials: 0,
            influence: 0,
            housing: 0,
            food: 0,
        }
    }
}
