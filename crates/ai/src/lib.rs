//! IA **baseline déterministe** (Phase 4) : des nations non-joueuses qui
//! s'implantent, recherchent, s'étendent — et se font la **guerre** quand le
//! grief monte (Phase conflit).
//!
//! L'IA est PURE : elle observe le monde et renvoie des [`Command`]. Elle ne
//! modifie jamais l'état directement — `sim` reste sans IA et tout reste
//! rejouable/auditable. C'est aussi la base que le futur Directeur (Phase 5)
//! orchestrera.

use std::collections::HashSet;

use proto::Command;
use sim::nation::{ESSOR, LIEN, TERROIR};
use sim::tile::TileKind;
use sim::World;

/// Branches que l'IA fait progresser (Fer = militaire, géré via la mobilisation).
const AI_BRANCHES: [usize; 3] = [TERROIR, ESSOR, LIEN];

/// Seuil de grief au-delà duquel l'IA déclare la guerre.
const WAR_THRESHOLD: f32 = 3.0;

/// Plan d'un tour pour une nation IA : recherche + expansion + diplomatie/guerre.
pub fn plan(world: &World, nation: u16) -> Vec<Command> {
    let mut cmds = Vec::new();

    // Recherche : la branche suivie au plus bas palier, si le savoir suffit.
    if let Some(n) = world.nation(nation) {
        let (branch, tier) = AI_BRANCHES
            .iter()
            .map(|&b| (b, n.tech[b]))
            .min_by_key(|&(b, t)| (t, b))
            .unwrap();
        if n.knowledge >= sim::tech_cost(tier) {
            cmds.push(Command::Research {
                nation,
                branch: branch as u8,
            });
        }
    }

    cmds.extend(expansion(world, nation));

    // Diplomatie : déclarer la guerre à la nation la plus haïe (grief élevé).
    if let Some((target, amount)) = world.diplomacy.top_grievance(nation) {
        if amount >= WAR_THRESHOLD && !world.diplomacy.at_war(nation, target) {
            cmds.push(Command::DeclareWar { nation, target });
        }
    }

    // Guerre : mobiliser puis attaquer une case frontalière ennemie.
    if let Some((from, to)) = find_attack(world, nation) {
        let amount = (world.tiles[from].population * 0.4) as u32;
        if amount > 0 {
            let (fx, fy) = coords(from, world.width);
            let (tx, ty) = coords(to, world.width);
            cmds.push(Command::Mobilize {
                x: fx,
                y: fy,
                nation,
                amount,
            });
            cmds.push(Command::March {
                from_x: fx,
                from_y: fy,
                to_x: tx,
                to_y: ty,
            });
        }
    }

    cmds
}

/// Essaimage glouton : chaque case ≥1000 hab. vers une terre adjacente libre.
fn expansion(world: &World, nation: u16) -> Vec<Command> {
    let w = world.width as i64;
    let h = world.height as i64;
    let mut cmds = Vec::new();
    let mut targeted: HashSet<usize> = HashSet::new();
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.owner != Some(nation) || t.population < 1000.0 {
            continue;
        }
        let x = idx as i64 % w;
        let y = idx as i64 / w;
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let nx = (x + dx).rem_euclid(w);
            let ny = y + dy;
            if ny < 0 || ny >= h {
                continue;
            }
            let v = (ny * w + nx) as usize;
            let nt = &world.tiles[v];
            if nt.kind == TileKind::Land && nt.owner.is_none() && targeted.insert(v) {
                cmds.push(Command::Swarm {
                    from_x: x as u32,
                    from_y: y as u32,
                    to_x: nx as u32,
                    to_y: ny as u32,
                });
                break;
            }
        }
    }
    cmds
}

/// Trouve une case (source ≥1000 hab.) frontalière d'une case ennemie en guerre.
fn find_attack(world: &World, nation: u16) -> Option<(usize, usize)> {
    let w = world.width as i64;
    let h = world.height as i64;
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.owner != Some(nation) || t.population < 1000.0 {
            continue;
        }
        let x = idx as i64 % w;
        let y = idx as i64 / w;
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let nx = (x + dx).rem_euclid(w);
            let ny = y + dy;
            if ny < 0 || ny >= h {
                continue;
            }
            let v = (ny * w + nx) as usize;
            if let Some(m) = world.tiles[v].owner {
                if m != nation && world.diplomacy.at_war(nation, m) {
                    return Some((idx, v));
                }
            }
        }
    }
    None
}

/// Place `count` nations sur des terres productives bien réparties (déterministe).
pub fn spawn_nations(world: &World, count: u16) -> Vec<Command> {
    let mut out = Vec::new();
    if count == 0 {
        return out;
    }
    const MIN_DIST: i64 = 15;
    let mut placed: Vec<(u32, u32)> = Vec::new();
    'scan: for y in (0..world.height).step_by(5) {
        for x in (0..world.width).step_by(5) {
            if world.tile(x, y).kind != TileKind::Land || world.capacity_at(x, y) < 400.0 {
                continue;
            }
            if placed
                .iter()
                .all(|&(px, py)| distance(x, y, px, py, world.width) >= MIN_DIST)
            {
                placed.push((x, y));
                if placed.len() == count as usize {
                    break 'scan;
                }
            }
        }
    }
    for (i, (x, y)) in placed.into_iter().enumerate() {
        out.push(Command::Settle {
            x,
            y,
            nation: i as u16,
            population: 300,
        });
    }
    out
}

/// (x, y) d'un index linéaire.
fn coords(idx: usize, width: u32) -> (u32, u32) {
    (idx as u32 % width, idx as u32 / width)
}

/// Distance de Manhattan avec enroulement sur X.
fn distance(x: u32, y: u32, px: u32, py: u32, width: u32) -> i64 {
    let dx = (x as i64 - px as i64).abs();
    let dxw = dx.min(width as i64 - dx);
    let dy = (y as i64 - py as i64).abs();
    dxw + dy
}
