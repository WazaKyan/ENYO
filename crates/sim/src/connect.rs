//! Connexion (S8) : réseaux d'**infrastructure**. Une case bâtie tire sa
//! main-d'œuvre soit de son **voisinage direct** (adjacence — « à côté ça
//! marche »), soit, si elle touche un **réseau d'infrastructure**, de TOUTE la
//! population **desservie** par ce réseau (les routes mettent en commun la
//! population de la région). Pur et déterministe : union-find en ordre d'index,
//! racine = plus petit index ; sommes en ordre d'index.

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

/// Réseaux d'infrastructure d'un monde + population desservie par réseau.
pub struct Networks {
    width: i64,
    height: i64,
    root: Vec<u32>,   // racine union-find par index (pertinent pour les cases infra)
    served: Vec<f32>, // population desservie par le réseau dont la racine est cet index
}

impl Networks {
    /// Calcule les réseaux (infra adjacentes de même nation) et la population
    /// desservie (chaque case peuplée nourrit les réseaux infra qu'elle touche).
    pub fn build(tiles: &[Tile], width: u32, height: u32) -> Networks {
        let w = width as i64;
        let h = height as i64;
        let n = tiles.len();
        let mut root: Vec<u32> = (0..n as u32).collect();

        // 1) Unir les cases d'infrastructure adjacentes de même nation.
        for idx in 0..n {
            if tiles[idx].building != Some(Building::Infrastructure) {
                continue;
            }
            let owner = tiles[idx].owner;
            let (x, y) = (idx as i64 % w, idx as i64 / w);
            for (dx, dy) in ORTHO {
                let nx = (x + dx).rem_euclid(w);
                let ny = y + dy;
                if ny < 0 || ny >= h {
                    continue;
                }
                let v = (ny * w + nx) as usize;
                if tiles[v].building == Some(Building::Infrastructure) && tiles[v].owner == owner {
                    union(&mut root, idx as u32, v as u32);
                }
            }
        }
        // Aplatir les racines (chemins compressés une bonne fois).
        for i in 0..n as u32 {
            root[i as usize] = find(&mut root, i);
        }

        // 2) Population desservie : chaque case peuplée ajoute sa pop aux réseaux
        //    infra adjacents (dédupliqué par racine de réseau).
        let mut served = vec![0.0f32; n];
        let mut seen: Vec<u32> = Vec::with_capacity(4);
        for idx in 0..n {
            let pop = tiles[idx].population;
            if pop <= 0.0 {
                continue;
            }
            let Some(owner) = tiles[idx].owner else {
                continue;
            };
            let (x, y) = (idx as i64 % w, idx as i64 / w);
            seen.clear();
            for (dx, dy) in ORTHO {
                let nx = (x + dx).rem_euclid(w);
                let ny = y + dy;
                if ny < 0 || ny >= h {
                    continue;
                }
                let v = (ny * w + nx) as usize;
                if tiles[v].building == Some(Building::Infrastructure)
                    && tiles[v].owner == Some(owner)
                {
                    let r = root[v];
                    if !seen.contains(&r) {
                        seen.push(r);
                        served[r as usize] += pop;
                    }
                }
            }
        }

        Networks {
            width: w,
            height: h,
            root,
            served,
        }
    }

    /// Main-d'œuvre connectée d'une case bâtie : le **max** entre son voisinage
    /// local (la case + ses 4 voisines de même nation) et le meilleur **réseau
    /// d'infrastructure** adjacent (qui met en commun toute la région desservie).
    pub fn connected_pop(&self, tiles: &[Tile], idx: usize, owner: u16) -> f32 {
        let (x, y) = (idx as i64 % self.width, idx as i64 / self.width);
        let mut local = tiles[idx].population;
        let mut best_net = 0.0f32;
        for (dx, dy) in ORTHO {
            let nx = (x + dx).rem_euclid(self.width);
            let ny = y + dy;
            if ny < 0 || ny >= self.height {
                continue;
            }
            let v = (ny * self.width + nx) as usize;
            if tiles[v].owner == Some(owner) {
                local += tiles[v].population;
                if tiles[v].building == Some(Building::Infrastructure) {
                    let s = self.served[self.root[v] as usize];
                    if s > best_net {
                        best_net = s;
                    }
                }
            }
        }
        local.max(best_net)
    }

    /// Y a-t-il une case `building` dans la région desservie par un réseau infra
    /// adjacent à `idx` (ou dans son voisinage direct) ? (Servira l'éducation, E3.)
    pub fn cluster_has(&self, tiles: &[Tile], idx: usize, owner: u16, building: Building) -> bool {
        let (x, y) = (idx as i64 % self.width, idx as i64 / self.width);
        for (dx, dy) in ORTHO {
            let nx = (x + dx).rem_euclid(self.width);
            let ny = y + dy;
            if ny < 0 || ny >= self.height {
                continue;
            }
            let v = (ny * self.width + nx) as usize;
            if tiles[v].owner == Some(owner) && tiles[v].building == Some(building) {
                return true;
            }
        }
        false
    }
}
