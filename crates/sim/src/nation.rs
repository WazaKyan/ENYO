//! Les nations : acteurs qui possèdent des cases, accumulent du savoir et
//! débloquent un **arbre de recherche** (graphe de techs à prérequis, cf. `tech.rs`).

use serde::{Deserialize, Serialize};

use crate::tech;

/// Stock d'argent au départ d'une nation (S8 — de quoi bâtir ses premières cases).
pub const STARTING_MONEY: i64 = 500;
/// Habitation au départ : de quoi **fonder une première ville** (la genèse pose une
/// ville sur la case d'implantation). Ensuite, l'habitation vient du commerce.
pub const STARTING_HOUSING: i64 = 60;
/// Influence au départ : de quoi **s'étendre** plusieurs fois d'emblée (l'expansion
/// est le seul moyen d'acquérir du territoire — « Fonder » a été retiré). Confortable
/// pour que le joueur ne soit pas étranglé au démarrage face aux IA dotées.
pub const STARTING_INFLUENCE: i64 = 60;

/// Une nation (le joueur est la nation 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Nation {
    pub id: u16,
    /// Savoir/science accumulé, dépensé pour la recherche (S3 ; alimenté par
    /// l'éducation en S8, + un flux de base par densité).
    pub knowledge: f32,
    /// **Techs débloquées** : bitmask (bit `i` = tech d'id `i` de `tech::TREE`). Les
    /// effets se recalculent par fonction pure ([`Nation::effects`]) — jamais stockés.
    /// `serde(default)` : les vieux snapshots (sans ce champ) chargent à 0.
    #[serde(default)]
    pub techs: u64,

    // --- Ressources S8 (économie interne), entières → déterminisme sans dérive ---
    /// Argent : bâtir + entretien mensuel.
    pub money: i64,
    /// Matériaux : produits par l'industrie, consommés par le commerce / la construction.
    pub materials: i64,
    /// Influence : flux mensuel **∝ territoire + population** (plancher de base) ;
    /// dépensée pour **étendre** le territoire (boucle vertueuse grande nation).
    pub influence: i64,
    /// Habitation : produite par le commerce ; **coût pour fonder une ville**.
    pub housing: i64,
    /// Nourriture : produite par les fermes ; **toute la population en consomme**
    /// chaque mois (au-delà d'un seuil de subsistance par case) — pénurie = famine.
    pub food: i64,
    /// Manpower (« force ») : stock national produit par les **casernes** et les
    /// **ports** ; dépensé pour **recruter** des unités et les **régénérer** sur le
    /// territoire national.
    pub manpower: i64,
}

impl Nation {
    pub fn new(id: u16) -> Self {
        Self {
            id,
            knowledge: 0.0,
            techs: 0,
            money: STARTING_MONEY,
            materials: 0,
            influence: STARTING_INFLUENCE,
            housing: STARTING_HOUSING,
            food: 0,
            manpower: 0,
        }
    }

    /// Effets cumulés des techs acquises (dérivé pur — capacité, portée, naval,
    /// unités, multiplicateurs de production…). Lu par S1/S2/S5/S8.
    pub fn effects(&self) -> tech::Effects {
        tech::effects(self.techs)
    }
}
