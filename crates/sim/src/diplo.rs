//! Diplomatie (système S6, base) : guerres et **griefs**.
//!
//! Représentation compatible serde-JSON (pas de clés tuple) : des `Vec`. L'ordre
//! d'insertion est déterministe (commandes appliquées dans l'ordre), donc le
//! checksum reste stable.

use serde::{Deserialize, Serialize};

/// État diplomatique global.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Diplomacy {
    /// Paires de nations en guerre (normalisées a < b).
    wars: Vec<(u16, u16)>,
    /// Griefs orientés : (de, vers, montant).
    grievances: Vec<(u16, u16, f32)>,
}

impl Diplomacy {
    fn pair(a: u16, b: u16) -> (u16, u16) {
        if a < b {
            (a, b)
        } else {
            (b, a)
        }
    }

    /// Deux nations sont-elles en guerre ?
    pub fn at_war(&self, a: u16, b: u16) -> bool {
        self.wars.contains(&Self::pair(a, b))
    }

    /// Active ou désactive l'état de guerre entre deux nations.
    pub fn set_war(&mut self, a: u16, b: u16, on: bool) {
        let p = Self::pair(a, b);
        if on {
            if !self.wars.contains(&p) {
                self.wars.push(p);
            }
        } else {
            self.wars.retain(|&x| x != p);
        }
    }

    /// Grief orienté de `from` envers `to`.
    pub fn grievance(&self, from: u16, to: u16) -> f32 {
        self.grievances
            .iter()
            .find(|g| g.0 == from && g.1 == to)
            .map(|g| g.2)
            .unwrap_or(0.0)
    }

    /// Ajoute du grief de `from` envers `to`.
    pub fn add_grievance(&mut self, from: u16, to: u16, amount: f32) {
        if let Some(g) = self
            .grievances
            .iter_mut()
            .find(|g| g.0 == from && g.1 == to)
        {
            g.2 += amount;
        } else {
            self.grievances.push((from, to, amount));
        }
    }

    /// Cible envers laquelle `from` a le plus de grief (et le montant).
    pub fn top_grievance(&self, from: u16) -> Option<(u16, f32)> {
        self.grievances
            .iter()
            .filter(|g| g.0 == from)
            .max_by(|a, b| a.2.total_cmp(&b.2))
            .map(|g| (g.1, g.2))
    }

    /// Décroît tous les griefs (l'animosité retombe avec le temps).
    pub fn decay(&mut self, factor: f32) {
        for g in &mut self.grievances {
            g.2 *= factor;
        }
    }

    /// Itère les paires en guerre (pour l'audit/checksum).
    pub fn wars(&self) -> &[(u16, u16)] {
        &self.wars
    }

    /// Itère les griefs (pour l'audit/checksum).
    pub fn grievances(&self) -> &[(u16, u16, f32)] {
        &self.grievances
    }
}
