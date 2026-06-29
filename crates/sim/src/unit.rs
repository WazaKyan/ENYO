//! Unités militaires (système S5) : des **agents discrets** (contrairement à la
//! population/force qui sont des stats de case) recrutés aux casernes. Chaque
//! unité a une position, des PV, des points de mouvement, et un **type** aux stats
//! figées (table `const`, single-source). Le type est débloqué par la branche Fer.
//!
//! Déterminisme : tout est **entier** ; les bonus de terrain sont quantifiés depuis
//! les stats de case (couche de décision scalaire → entier), jamais en `f32` dans
//! une op spatiale (le mouvement réutilise la primitive d'atteignabilité `path`).

use proto::UnitKind;
use serde::{Deserialize, Serialize};

/// Une unité sur la carte (S5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    pub id: u32,
    pub owner: u16,
    pub kind: UnitKind,
    pub x: u32,
    pub y: u32,
    /// Points de vie courants (≤ 0 ⇒ détruite).
    pub hp: i32,
    /// Points de mouvement restants ce tour (rechargés chaque mois).
    pub moves_left: u32,
}

/// Stats figées d'un type d'unité (calibrage S5, single-source).
#[derive(Debug, Clone, Copy)]
pub struct UnitStats {
    /// PV maximum.
    pub max_hp: i32,
    /// Dégâts de base d'une attaque.
    pub damage: i32,
    /// Portée d'attaque en cases (distance de Manhattan, X enroulé). 1 = corps à corps.
    pub range: u32,
    /// Points de mouvement par tour (coût terrain de base : 10/case de plaine).
    pub moves: u32,
    /// Coût de recrutement : argent (nation) + force (de la caserne).
    pub cost_money: i64,
    pub cost_force: i64,
    /// Palier minimal de la branche **Fer** pour débloquer ce type.
    pub tech_fer: u8,
    /// Malus d'attaque (%) si l'unité attaque depuis une case très **boisée**.
    pub forest_attack_malus: i32,
    /// Malus d'attaque (%) si l'unité attaque depuis une case très **accidentée**.
    pub rough_attack_malus: i32,
    /// Unité **navale** (se déplace sur l'eau, recrutée au port).
    pub naval: bool,
    /// Capacité de transport (nombre d'unités terrestres embarquables ; 0 = aucune).
    pub capacity: u8,
}

/// Unité **transportée** à bord d'une galère (cargo) : son type et ses PV.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CarriedUnit {
    pub kind: UnitKind,
    pub hp: i32,
}

/// Table des stats par type (figée — à régler par golden). **Single-source.**
pub fn unit_stats(kind: UnitKind) -> UnitStats {
    match kind {
        // Polyvalente, robuste, corps à corps. Aucun malus de terrain.
        UnitKind::Infantry => UnitStats {
            max_hp: 100,
            damage: 25,
            range: 1,
            moves: 30,
            cost_money: 60,
            cost_force: 40,
            tech_fer: 0,
            forest_attack_malus: 0,
            rough_attack_malus: 0,
            naval: false,
            capacity: 0,
        },
        // Distance (portée 2), fragile. Mauvaise en forêt (visée gênée).
        UnitKind::Archer => UnitStats {
            max_hp: 70,
            damage: 30,
            range: 2,
            moves: 25,
            cost_money: 80,
            cost_force: 35,
            tech_fer: 1,
            forest_attack_malus: 40,
            rough_attack_malus: 0,
            naval: false,
            capacity: 0,
        },
        // Rapide et puissante, mais a besoin de terrain ouvert.
        UnitKind::Cavalry => UnitStats {
            max_hp: 120,
            damage: 35,
            range: 1,
            moves: 55,
            cost_money: 120,
            cost_force: 55,
            tech_fer: 2,
            forest_attack_malus: 30,
            rough_attack_malus: 35,
            naval: false,
            capacity: 0,
        },
        // Galère : navale, rapide sur l'eau, transporte 2 unités terrestres.
        UnitKind::Galley => UnitStats {
            max_hp: 80,
            damage: 20,
            range: 1,
            moves: 60,
            cost_money: 100,
            cost_force: 30,
            tech_fer: 0,
            forest_attack_malus: 0,
            rough_attack_malus: 0,
            naval: true,
            capacity: 2,
        },
    }
}

/// Code stable d'un type pour le checksum (audit de déterminisme).
pub fn kind_code(kind: UnitKind) -> u64 {
    match kind {
        UnitKind::Infantry => 1,
        UnitKind::Archer => 2,
        UnitKind::Cavalry => 3,
        UnitKind::Galley => 4,
    }
}
