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

mod director;
pub use director::{Director, Intent, Stance};

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

/// Essaimage glouton : chaque case ≥1000 hab. vers une terre adjacente libre,
/// dans la limite de l'influence disponible (E5 : essaimer coûte de l'influence).
fn expansion(world: &World, nation: u16) -> Vec<Command> {
    let w = world.width as i64;
    let h = world.height as i64;
    // Nombre d'essaimages que la nation peut s'offrir ce tour.
    let mut budget = world
        .nation(nation)
        .map(|n| n.influence / sim::SWARM_INFLUENCE)
        .unwrap_or(0);
    if budget <= 0 {
        return Vec::new();
    }
    let mut cmds = Vec::new();
    let mut targeted: HashSet<usize> = HashSet::new();
    for (idx, t) in world.tiles.iter().enumerate() {
        if budget <= 0 {
            break;
        }
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
                budget -= 1;
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
    // Distance min calculée pour répartir les nations en 2D sur la carte.
    let span = world.width.min(world.height) as f32;
    let min_dist = (span / (2.0 * (count as f32).sqrt())).max(10.0) as i64;
    let mut placed: Vec<(u32, u32)> = Vec::new();
    'scan: for y in (0..world.height).step_by(5) {
        for x in (0..world.width).step_by(5) {
            if world.tile(x, y).kind != TileKind::Land || world.capacity_at(x, y) < 400.0 {
                continue;
            }
            if placed
                .iter()
                .all(|&(px, py)| distance(x, y, px, py, world.width) >= min_dist)
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

// ---------------------------------------------------------------------------
// Le DIRECTEUR (Phase 5a, déterministe) — invisible : il biaise les entrées de
// S1/S6 pour servir l'intérêt du joueur. Le LLM (5b) viendra CHOISIR parmi ces
// leviers déjà légaux.
// ---------------------------------------------------------------------------

/// Au-delà de cette domination, on attise une coalition contre le joueur.
pub(crate) const DOMINANCE_PRESSURE: f32 = 0.10;
/// Domination écrasante : on ajoute une calamité « naturelle ».
pub(crate) const DOMINANCE_BLIGHT: f32 = 0.25;
const DIRECTOR_PRESSURE: u32 = 5;
const DIRECTOR_BLIGHT: u32 = 25;
const DIRECTOR_RELIEF: u32 = 30;

/// Indice de Drame : lecture déterministe de l'« intérêt » pour le joueur.
pub(crate) struct Drama {
    pub(crate) nations: usize,
    pub(crate) dominance: f32,
    pub(crate) struggling: bool,
    pub(crate) strongest_rival: Option<u16>,
    pub(crate) player_best_tile: Option<(u32, u32)>,
    pub(crate) player_worst_tile: Option<(u32, u32)>,
}

pub(crate) fn assess(world: &World, player: u16) -> Drama {
    let mut total = 0.0f32;
    let mut player_pop = 0.0f32;
    let mut best_other = 0.0f32;
    let mut strongest_rival = None;
    for n in &world.nations {
        let (pop, _) = world.nation_stats(n.id);
        total += pop;
        if n.id == player {
            player_pop = pop;
        } else if pop > best_other {
            best_other = pop;
            strongest_rival = Some(n.id);
        }
    }
    let player_share = if total > 0.0 { player_pop / total } else { 0.0 };
    let best_other_share = if total > 0.0 { best_other / total } else { 0.0 };
    let dominance = player_share - best_other_share;
    let struggling = best_other > 0.0 && player_pop < 0.5 * best_other;

    let mut player_best_tile = None;
    let mut best_pop = -1.0f32;
    let mut player_worst_tile = None;
    let mut worst_dev = -1.0f32;
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.owner != Some(player) {
            continue;
        }
        if t.population > best_pop {
            best_pop = t.population;
            player_best_tile = Some(coords(idx, world.width));
        }
        if t.devastation > worst_dev {
            worst_dev = t.devastation;
            player_worst_tile = Some(coords(idx, world.width));
        }
    }

    Drama {
        nations: world.nations.len(),
        dominance,
        struggling,
        strongest_rival,
        player_best_tile,
        player_worst_tile,
    }
}

/// Le Directeur (déterministe) : oriente la partie pour servir l'intérêt du
/// `player`, de façon invisible (biais sur S1/S6). Le LLM (5b) ne fera ensuite
/// que CHOISIR parmi ces leviers déjà légaux et bornés.
pub fn direct(world: &World, player: u16) -> Vec<Command> {
    let d = assess(world, player);
    let mut cmds = Vec::new();
    if d.nations < 2 {
        return cmds; // pas de rival : rien à mettre en scène
    }

    if d.dominance > DOMINANCE_PRESSURE {
        // Le joueur domine : attiser une coalition (grief d'un rival envers lui).
        if let Some(rival) = d.strongest_rival {
            cmds.push(Command::DirectorGrievance {
                from: rival,
                to: player,
                amount: DIRECTOR_PRESSURE,
            });
        }
        // Domination écrasante : une calamité « naturelle » sur sa meilleure case.
        if d.dominance > DOMINANCE_BLIGHT {
            if let Some((x, y)) = d.player_best_tile {
                cmds.push(Command::DirectorBlight {
                    x,
                    y,
                    amount: DIRECTOR_BLIGHT,
                });
            }
        }
    } else if d.struggling {
        // Le joueur souffre (probablement injustement) : un salut discret.
        if let Some((x, y)) = d.player_worst_tile {
            cmds.push(Command::DirectorWindfall {
                x,
                y,
                amount: DIRECTOR_RELIEF,
            });
        }
    }
    cmds
}
