//! Atteignabilité sur la grille (système S2) : coût de terrain + Dijkstra borné.
//!
//! **Entiers + ordre canonique** (tie-break par index) pour un déterminisme
//! parfait du replay/audit (cf. contrat dans `CLAUDE.md`). Enroulement sur X.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

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
    // `dist` en HashMap (et NON `vec![u32::MAX; tiles.len()]`) : la recherche est
    // **bornée** par `budget` (`d >= budget` coupe l'expansion), donc seule une
    // petite région autour de `from` est explorée. Allouer/zéroer 400 000 cases à
    // CHAQUE déplacement d'unité était le principal point chaud de perf (lag avec
    // beaucoup d'unités). Lookups seulement (jamais d'itération) → rejeu identique.
    let mut dist: HashMap<usize, u32> = HashMap::new();
    dist.insert(from, 0);
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u32, from)));

    while let Some(Reverse((d, u))) = heap.pop() {
        if u == to {
            return Some(d);
        }
        if d >= budget || d > *dist.get(&u).unwrap_or(&u32::MAX) {
            continue;
        }
        for nb in neighbors(u, width, height).into_iter().flatten() {
            let c = cost(&tiles[nb]);
            if c == u32::MAX {
                continue;
            }
            let nd = d.saturating_add(c);
            if nd <= budget && nd < *dist.get(&nb).unwrap_or(&u32::MAX) {
                dist.insert(nb, nd);
                heap.push(Reverse((nd, nb)));
            }
        }
    }
    None
}

/// Distance de Manhattan (X enroulé) entre deux index de case.
fn manhattan_idx(a: usize, b: usize, width: u32) -> u32 {
    let w = width as i64;
    let (ax, ay) = (a as i64 % w, a as i64 / w);
    let (bx, by) = (b as i64 % w, b as i64 / w);
    let dx = (ax - bx).abs();
    let dx = dx.min(w - dx);
    (dx + (ay - by).abs()) as u32
}

/// **Prochaine destination** d'un ordre de marche : Dijkstra borné déterministe (tas
/// min sur (coût, index)) depuis `from` vers `to`, en **contournant les obstacles**.
/// Reconstruit le chemin (ou vise le nœud exploré le plus proche du but si `to` est
/// hors d'atteinte) et renvoie la case la **plus loin atteignable ce tour** (coût ≤
/// `budget`) **libre** d'unité (`occupied` = cases occupées). Règle « au moins une
/// case » : à pleins points (`budget == full_moves`), au moins le premier pas. `None`
/// si aucun progrès possible. HashMap en lecture seule → rejeu déterministe.
#[allow(clippy::too_many_arguments)]
pub fn march_step<F: Fn(&Tile) -> u32>(
    tiles: &[Tile],
    width: u32,
    height: u32,
    from: usize,
    to: usize,
    budget: u32,
    full_moves: u32,
    occupied: &HashSet<usize>,
    cost: F,
) -> Option<usize> {
    if from == to {
        return None;
    }
    let mut dist: HashMap<usize, u32> = HashMap::new();
    let mut prev: HashMap<usize, usize> = HashMap::new();
    dist.insert(from, 0);
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u32, from)));
    let mut best_goal = from;
    let mut best_key = (manhattan_idx(from, to, width), 0u32, from);
    let mut reached = false;
    let mut explored = 0usize;
    while let Some(Reverse((d, idx))) = heap.pop() {
        if d > *dist.get(&idx).unwrap_or(&u32::MAX) {
            continue;
        }
        if idx == to {
            reached = true;
            break;
        }
        let key = (manhattan_idx(idx, to, width), d, idx);
        if key < best_key {
            best_key = key;
            best_goal = idx;
        }
        explored += 1;
        if explored > 4000 {
            break;
        }
        for nb in neighbors(idx, width, height).into_iter().flatten() {
            let c = cost(&tiles[nb]);
            if c == u32::MAX {
                continue;
            }
            let nd = d.saturating_add(c);
            if nd < *dist.get(&nb).unwrap_or(&u32::MAX) {
                dist.insert(nb, nd);
                prev.insert(nb, idx);
                heap.push(Reverse((nd, nb)));
            }
        }
    }
    let goal = if reached { to } else { best_goal };
    if goal == from {
        return None;
    }
    let mut path = vec![goal];
    let mut cur = goal;
    while cur != from {
        cur = *prev.get(&cur)?;
        path.push(cur);
    }
    path.reverse();
    let mut dest = None;
    for &idx in &path {
        if idx == from {
            continue;
        }
        if *dist.get(&idx).unwrap_or(&u32::MAX) > budget {
            break;
        }
        if !occupied.contains(&idx) {
            dest = Some(idx);
        }
    }
    // « Au moins une case » : à pleins points, on tente le premier pas (le sim
    // l'autorise même si trop cher) → l'unité n'est jamais gelée par la météo.
    if dest.is_none() && budget == full_moves {
        if let Some(&first) = path.get(1) {
            if !occupied.contains(&first) {
                dest = Some(first);
            }
        }
    }
    dest
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
