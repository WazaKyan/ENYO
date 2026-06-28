//! Provinces **émergentes** (début du système S4) : une province est une
//! composante connexe de cases possédées par une même nation (4-connexité, X
//! enroulé). Calculée à la demande — jamais stockée (cf. `CLAUDE.md`).

use crate::World;

/// Agrégat d'une province émergente.
#[derive(Debug, Clone, PartialEq)]
pub struct Province {
    pub owner: u16,
    pub tiles: u32,
    pub population: f32,
    pub development: f32,
}

impl World {
    /// Calcule les provinces émergentes par flood-fill des cases possédées.
    pub fn provinces(&self) -> Vec<Province> {
        let w = self.width as i64;
        let h = self.height as i64;
        let n = self.tiles.len();
        let mut seen = vec![false; n];
        let mut out = Vec::new();

        for start in 0..n {
            if seen[start] {
                continue;
            }
            let owner = match self.tiles[start].owner {
                Some(o) => o,
                None => {
                    seen[start] = true;
                    continue;
                }
            };

            let mut stack = vec![start];
            seen[start] = true;
            let mut prov = Province {
                owner,
                tiles: 0,
                population: 0.0,
                development: 0.0,
            };

            while let Some(u) = stack.pop() {
                let t = &self.tiles[u];
                prov.tiles += 1;
                prov.population += t.population;
                prov.development += t.development;

                let x = u as i64 % w;
                let y = u as i64 / w;
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let nx = (x + dx).rem_euclid(w);
                    let ny = y + dy;
                    if ny < 0 || ny >= h {
                        continue;
                    }
                    let v = (ny * w + nx) as usize;
                    if !seen[v] && self.tiles[v].owner == Some(owner) {
                        seen[v] = true;
                        stack.push(v);
                    }
                }
            }
            out.push(prov);
        }
        out
    }
}
