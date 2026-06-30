//! Marché du travail (refonte EU5, S8) : la population vit sur les **villes** ; les
//! bâtiments d'une même **région connexe** (cases possédées adjacentes d'une nation)
//! se partagent un **pool d'emplois LIMITÉ**. Pas assez d'habitants → sous-effectif
//! → toute la région produit au **prorata**. Union-find en ordre d'index (racine =
//! plus petit index) ; agrégats par racine en `HashMap` (lecture seule) → rejeu
//! déterministe (le résultat ne dépend pas de l'ordre d'itération).

use std::collections::{HashMap, HashSet};

use crate::tile::Tile;
use proto::Building;

const ORTHO: [(i64, i64); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

fn find(root: &mut [u32], mut i: u32) -> u32 {
    while root[i as usize] != i {
        root[i as usize] = root[root[i as usize] as usize]; // compression de chemin
        i = root[i as usize];
    }
    i
}

fn union(root: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(root, a), find(root, b));
    if ra != rb {
        // Racine = plus petit index → canonique, déterministe.
        if ra < rb {
            root[rb as usize] = ra;
        } else {
            root[ra as usize] = rb;
        }
    }
}

/// Un bâtiment qui **emploie** de la main-d'œuvre (la ville = source de pop, l'infra
/// = simple connecteur, ne comptent pas comme emplois).
fn is_job(b: Building) -> bool {
    matches!(
        b,
        Building::Industry
            | Building::Commerce
            | Building::Education
            | Building::Military
            | Building::Port
            | Building::Farm
    )
}

/// Régions de travail : composantes connexes des cases possédées (par nation), avec
/// leur **taux d'occupation des emplois** (staffing ∈ [0,1]) et la présence d'un
/// commerce. Le staffing d'un bâtiment = la fraction de main-d'œuvre dont dispose
/// SA région (population des villes ÷ demande totale d'emplois de la région).
pub struct Labor {
    root: Vec<u32>,
    staffing: HashMap<u32, f32>,
    commerce: HashSet<u32>,
}

impl Labor {
    /// `job_slots` = nombre d'habitants pour pourvoir **pleinement un** bâtiment.
    pub fn build(tiles: &[Tile], width: u32, height: u32, job_slots: f32) -> Labor {
        let w = width as i64;
        let h = height as i64;
        let n = tiles.len();
        let mut root: Vec<u32> = (0..n as u32).collect();

        // 1) Unir les cases possédées ADJACENTES de même nation → régions connexes.
        for idx in 0..n {
            let Some(owner) = tiles[idx].owner else {
                continue;
            };
            let (x, y) = (idx as i64 % w, idx as i64 / w);
            for (dx, dy) in ORTHO {
                let nx = (x + dx).rem_euclid(w);
                let ny = y + dy;
                if ny < 0 || ny >= h {
                    continue;
                }
                let v = (ny * w + nx) as usize;
                if tiles[v].owner == Some(owner) {
                    union(&mut root, idx as u32, v as u32);
                }
            }
        }
        for i in 0..n as u32 {
            root[i as usize] = find(&mut root, i);
        }

        // 2) Par région : pool (pop des VILLES), nombre d'emplois, présence commerce.
        let mut pool: HashMap<u32, f32> = HashMap::new();
        let mut jobs: HashMap<u32, f32> = HashMap::new();
        let mut commerce: HashSet<u32> = HashSet::new();
        for (idx, t) in tiles.iter().enumerate() {
            if t.owner.is_none() {
                continue;
            }
            let r = root[idx];
            // Pool = population de la région. En jeu, la pop ne vit que sur les VILLES
            // (l'expansion crée des cases vides, seules les villes croissent) → le pool
            // est donc la pop des villes. On somme toute la pop possédée pour rester
            // robuste (cases avec pop résiduelle, parties chargées, tests).
            *pool.entry(r).or_insert(0.0) += t.population;
            if let Some(b) = t.building {
                if is_job(b) {
                    *jobs.entry(r).or_insert(0.0) += 1.0;
                    if b == Building::Commerce {
                        commerce.insert(r);
                    }
                }
            }
        }

        // 3) Taux d'occupation : pop des villes ÷ demande (emplois × postes), borné à 1.
        let mut staffing: HashMap<u32, f32> = HashMap::new();
        for (&r, &j) in &jobs {
            let p = pool.get(&r).copied().unwrap_or(0.0);
            staffing.insert(r, (p / (j * job_slots)).min(1.0));
        }

        Labor {
            root,
            staffing,
            commerce,
        }
    }

    /// Taux d'occupation (0..1) des emplois de la région de la case `idx` : la
    /// fraction de main-d'œuvre disponible pour ses bâtiments (1 si pas d'emplois).
    pub fn staffing_at(&self, idx: usize) -> f32 {
        self.staffing
            .get(&self.root[idx])
            .copied()
            .unwrap_or(1.0)
    }

    /// Un **commerce** est-il présent dans la même région que `idx` ? (Éducation.)
    pub fn has_commerce_at(&self, idx: usize) -> bool {
        self.commerce.contains(&self.root[idx])
    }
}
