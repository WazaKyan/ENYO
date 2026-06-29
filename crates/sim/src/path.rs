//! Atteignabilité sur la grille (système S2) : coût de terrain + Dijkstra borné.
//!
//! **Entiers + ordre canonique** (tie-break par index) pour un déterminisme
//! parfait du replay/audit (cf. contrat dans `CLAUDE.md`). Enroulement sur X.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::tile::{Tile, TileKind};

/// Coût (entier) pour ENTRER dans une case, selon le terrain et la tech navale.
/// Océan = infranchissable sans tech Lien ; relief = plus cher.
pub fn tile_cost(tile: &Tile, naval_tier: u8) -> u32 {
    match tile.kind {
        TileKind::Land => 10 + (tile.ruggedness * 40.0) as u32,
        TileKind::Ocean => {
            if naval_tier == 0 {
                u32::MAX // infranchissable
            } else {
                // moins cher à mesure que la tech navale progresse
                30 + 20 * (3u32.saturating_sub(naval_tier as u32))
            }
        }
    }
}

/// Budget de portée d'essaimage selon le palier de la branche Essor.
pub fn range_budget(essor_tier: u8) -> u32 {
    60 + 40 * essor_tier as u32
}

/// Coût d'entrée pour une **unité** (S5) : terrain (via `tile_cost`) + **intempéries**
/// (pluie/orage, terrain ravagé, gel) qui ralentissent la marche. Entier, déterministe.
pub fn unit_move_cost(tile: &Tile, naval_tier: u8) -> u32 {
    let base = tile_cost(tile, naval_tier);
    if base == u32::MAX {
        return u32::MAX;
    }
    let mut penalty = (tile.precip_now * 15.0) as u32; // pluie / orage
    penalty += (tile.devastation * 25.0) as u32; // terrain ravagé
    if tile.temperature < 0.0 {
        penalty += 8; // neige / gel
    }
    base + penalty
}

/// Voisins (4-connexité) avec enroulement sur X et bornage sur Y.
fn neighbors(idx: usize, width: u32, height: u32) -> [Option<usize>; 4] {
    let w = width as i64;
    let h = height as i64;
    let x = (idx as i64) % w;
    let y = (idx as i64) / w;
    let mut out = [None; 4];
    let dirs = [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)];
    for (k, (dx, dy)) in dirs.iter().enumerate() {
        let nx = (x + dx).rem_euclid(w); // X s'enroule (cylindre)
        let ny = y + dy;
        if ny < 0 || ny >= h {
            continue; // Y ne s'enroule pas (pôles)
        }
        out[k] = Some((ny * w + nx) as usize);
    }
    out
}

/// Coût minimal pour atteindre `to` depuis `from`, borné par `budget`, avec une
/// **fonction de coût d'entrée** arbitraire (essaimage, unités…). Dijkstra
/// déterministe (tas min sur (coût, index) — tie-break par index). C'est l'UNIQUE
/// primitive d'atteignabilité (cf. `CLAUDE.md`).
pub fn reach_cost_with<F: Fn(&Tile) -> u32>(
    tiles: &[Tile],
    width: u32,
    height: u32,
    from: usize,
    to: usize,
    budget: u32,
    cost: F,
) -> Option<u32> {
    if from == to {
        return Some(0);
    }
    let mut dist = vec![u32::MAX; tiles.len()];
    dist[from] = 0;
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u32, from)));

    while let Some(Reverse((d, u))) = heap.pop() {
        if u == to {
            return Some(d);
        }
        if d > dist[u] || d >= budget {
            continue;
        }
        for nb in neighbors(u, width, height).into_iter().flatten() {
            let c = cost(&tiles[nb]);
            if c == u32::MAX {
                continue;
            }
            let nd = d.saturating_add(c);
            if nd <= budget && nd < dist[nb] {
                dist[nb] = nd;
                heap.push(Reverse((nd, nb)));
            }
        }
    }
    None
}

/// Coût d'entrée pour une **unité navale** (galère) : l'eau est rapide, la terre
/// infranchissable (les navires restent en mer). Les intempéries ralentissent.
pub fn naval_move_cost(tile: &Tile) -> u32 {
    match tile.kind {
        TileKind::Ocean => 10 + (tile.precip_now * 12.0) as u32,
        TileKind::Land => u32::MAX,
    }
}

/// Coût d'atteignabilité avec le coût de terrain standard (essaimage, S2).
pub fn reach_cost(
    tiles: &[Tile],
    width: u32,
    height: u32,
    from: usize,
    to: usize,
    budget: u32,
    naval_tier: u8,
) -> Option<u32> {
    reach_cost_with(tiles, width, height, from, to, budget, |t| {
        tile_cost(t, naval_tier)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::TileKind;
    use crate::World;

    #[test]
    fn range_grows_with_essor() {
        assert!(range_budget(2) > range_budget(0));
    }

    #[test]
    fn ocean_blocks_without_naval_tech() {
        // Bande terre/océan/terre/océan (largeur 4, cylindre) : pour aller de la
        // case 0 à la case 2, les deux chemins traversent un océan.
        let mut w = World::new(1, 4, 1);
        for (i, kind) in [
            TileKind::Land,
            TileKind::Ocean,
            TileKind::Land,
            TileKind::Ocean,
        ]
        .into_iter()
        .enumerate()
        {
            w.tiles[i].kind = kind;
            w.tiles[i].ruggedness = 0.0;
        }
        // Sans tech navale : infranchissable.
        assert_eq!(reach_cost(&w.tiles, 4, 1, 0, 2, 1000, 0), None);
        // Avec tech navale : franchissable.
        assert!(reach_cost(&w.tiles, 4, 1, 0, 2, 1000, 2).is_some());
    }
}
