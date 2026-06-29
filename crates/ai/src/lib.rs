//! IA **baseline déterministe** (Phase 4) : des nations non-joueuses qui
//! s'implantent, recherchent, s'étendent — et se font la **guerre** quand le
//! grief monte (Phase conflit).
//!
//! L'IA est PURE : elle observe le monde et renvoie des [`Command`]. Elle ne
//! modifie jamais l'état directement — `sim` reste sans IA et tout reste
//! rejouable/auditable. C'est aussi la base que le futur Directeur (Phase 5)
//! orchestrera.

use std::collections::HashSet;

use proto::{Building, Command, UnitKind};
use sim::nation::{ESSOR, FER, LIEN, TERROIR};
use sim::rng::Rng;
use sim::tile::TileKind;
use sim::unit::{unit_stats, Unit};
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

    // Économie : bâtir (industrie → ferme → commerce → ville) pour soutenir la
    // population — qui ne croît que via les villes, et qui doit être nourrie.
    cmds.extend(economy(world, nation));

    cmds.extend(expansion(world, nation));

    // Diplomatie : guerre sur grief élevé ; sinon, **conquête proactive** du
    // rival le plus proche dès qu'on a une caserne (capacité militaire) et qu'on
    // n'est en guerre avec personne. Le combat passe par les unités (occupation).
    let mut declared = false;
    if let Some((target, amount)) = world.diplomacy.top_grievance(nation) {
        if amount >= WAR_THRESHOLD && !world.diplomacy.at_war(nation, target) {
            cmds.push(Command::DeclareWar { nation, target });
            declared = true;
        }
    }
    if !declared && has_barracks(world, nation) && !at_war_with_anyone(world, nation) {
        if let Some(target) = nearest_rival(world, nation) {
            cmds.push(Command::DeclareWar { nation, target });
        }
    }

    // Militaire : recruter / déplacer / attaquer / occuper (si en guerre).
    cmds.extend(military(world, nation));

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

/// Économie IA (minimale, déterministe) : bâtit AU PLUS UN bâtiment/tour sur la
/// première case possédée et vide (ordre d'index), selon une priorité qui amorce
/// la chaîne ville → population : **industrie** (matériaux) → **ferme** (nourriture,
/// pour étendre les villes au-delà de la subsistance) → **commerce** (habitation)
/// → **ville** (nouveau foyer). Respecte les coûts (argent / matériaux / habitation).
fn economy(world: &World, nation: u16) -> Vec<Command> {
    let Some(n) = world.nation(nation) else {
        return Vec::new();
    };
    let mut industries = 0;
    let mut farms = 0;
    let mut commerces = 0;
    let mut militaries = 0;
    let mut empty: Option<usize> = None;
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.owner != Some(nation) {
            continue;
        }
        match t.building {
            Some(Building::Industry) => industries += 1,
            Some(Building::Farm) => farms += 1,
            Some(Building::Commerce) => commerces += 1,
            Some(Building::Military) => militaries += 1,
            None if t.kind == TileKind::Land && empty.is_none() => empty = Some(idx),
            _ => {}
        }
    }
    let Some(idx) = empty else {
        return Vec::new(); // pas de case libre où bâtir
    };
    let affordable = |b: Building| {
        let (m, mat, h) = sim::build_cost(b);
        n.money >= m && n.materials >= mat && n.housing >= h
    };
    // Chaîne : industrie (matériaux) → ferme (nourrir) → caserne (force/unités) →
    // commerce (habitation) → fermes en plus → villes (population). La caserne
    // arrive tôt pour que l'IA puisse faire la guerre.
    let pick = if industries == 0 && affordable(Building::Industry) {
        Building::Industry
    } else if farms == 0 && affordable(Building::Farm) {
        Building::Farm
    } else if militaries == 0 && affordable(Building::Military) {
        Building::Military
    } else if commerces == 0 && affordable(Building::Commerce) {
        Building::Commerce
    } else if farms < industries + 1 && affordable(Building::Farm) {
        Building::Farm
    } else if affordable(Building::City) {
        Building::City
    } else {
        return Vec::new();
    };
    let (x, y) = coords(idx, world.width);
    vec![Command::Build {
        x,
        y,
        nation,
        building: pick,
    }]
}

/// Distance de Manhattan (X enroulé) entre deux cases.
fn manhattan(x0: u32, y0: u32, x1: u32, y1: u32, width: u32) -> u32 {
    let w = width as i64;
    let dx = (x0 as i64 - x1 as i64).abs();
    let dx = dx.min(w - dx);
    let dy = (y0 as i64 - y1 as i64).abs();
    (dx + dy) as u32
}

/// La nation a-t-elle une **caserne** (capacité à fielder des unités) ?
fn has_barracks(world: &World, nation: u16) -> bool {
    world
        .tiles
        .iter()
        .any(|t| t.owner == Some(nation) && t.building == Some(Building::Military))
}

/// La nation est-elle déjà en guerre avec quelqu'un ?
fn at_war_with_anyone(world: &World, nation: u16) -> bool {
    world
        .nations
        .iter()
        .any(|o| o.id != nation && world.diplomacy.at_war(nation, o.id))
}

/// Position « ancre » d'une nation (sa case de plus petit index).
fn nation_anchor(world: &World, nation: u16) -> Option<(u32, u32)> {
    world
        .tiles
        .iter()
        .position(|t| t.owner == Some(nation))
        .map(|idx| coords(idx, world.width))
}

/// Nation rivale la plus proche (par distance d'ancre) — cible de conquête.
fn nearest_rival(world: &World, nation: u16) -> Option<u16> {
    let me = nation_anchor(world, nation)?;
    let mut best = None;
    let mut best_d = u32::MAX;
    for o in &world.nations {
        if o.id == nation {
            continue;
        }
        if let Some(p) = nation_anchor(world, o.id) {
            let d = manhattan(me.0, me.1, p.0, p.1, world.width);
            if d < best_d {
                best_d = d;
                best = Some(o.id);
            }
        }
    }
    best
}

/// Destination (case terre, libre d'unité) atteinte en marchant **plusieurs pas**
/// vers la cible dans la limite des points de mouvement — pour traverser vite. La
/// case finale est libre ; le chemin (validé ensuite par `MoveUnit`) peut traverser.
fn march_toward(world: &World, nation: u16, u: &Unit, tx: u32, ty: u32) -> Option<(u32, u32)> {
    let is_naval = unit_stats(u.kind).naval;
    let naval_tier = world.nation(nation).map(|n| n.tech[LIEN]).unwrap_or(0);
    let (w, h) = (world.width as i64, world.height as i64);
    let (mut cx, mut cy) = (u.x, u.y);
    let mut budget = u.moves_left;
    let mut dest: Option<(u32, u32)> = None;
    for _ in 0..8 {
        if (cx, cy) == (tx, ty) {
            break;
        }
        let cur_d = manhattan(cx, cy, tx, ty, world.width);
        let mut nb: Option<(u32, u32, u32)> = None; // (x, y, coût)
        let mut nb_d = cur_d;
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let nx = (cx as i64 + dx).rem_euclid(w);
            let ny = cy as i64 + dy;
            if ny < 0 || ny >= h {
                continue;
            }
            let (nx, ny) = (nx as u32, ny as u32);
            // Coût selon le domaine : navale = eau (terre infranchissable) ;
            // terrestre = terre (eau selon la tech navale). MAX = infranchissable.
            let cost = if is_naval {
                sim::path::naval_move_cost(world.tile(nx, ny))
            } else {
                sim::path::unit_move_cost(world.tile(nx, ny), naval_tier)
            };
            if cost == u32::MAX || cost > budget {
                continue;
            }
            let d = manhattan(nx, ny, tx, ty, world.width);
            if d < nb_d {
                nb_d = d;
                nb = Some((nx, ny, cost));
            }
        }
        match nb {
            Some((nx, ny, cost)) => {
                budget -= cost;
                cx = nx;
                cy = ny;
                if !world.units.iter().any(|x| x.x == cx && x.y == cy) {
                    dest = Some((cx, cy)); // dernière case libre atteinte
                }
            }
            None => break,
        }
    }
    dest
}

/// Militaire IA : recrute aux casernes, **occupe** le territoire ennemi (déplace
/// les unités vers la case ennemie non occupée la plus proche) et **attaque** les
/// unités ennemies à portée. N'agit qu'**en guerre**.
fn military(world: &World, nation: u16) -> Vec<Command> {
    let mut cmds = Vec::new();
    let width = world.width;
    let at_war = |o: u16| world.diplomacy.at_war(nation, o);

    // Cibles : unités ennemies + cases ennemies pas encore occupées par nous.
    let enemy_units: Vec<(u32, u32)> = world
        .units
        .iter()
        .filter(|u| u.owner != nation && at_war(u.owner))
        .map(|u| (u.x, u.y))
        .collect();
    let mut enemy_tiles: Vec<(u32, u32)> = Vec::new();
    for (idx, t) in world.tiles.iter().enumerate() {
        if let Some(o) = t.owner {
            if o != nation && at_war(o) && t.occupier != Some(nation) {
                enemy_tiles.push(coords(idx, width));
            }
        }
    }
    if enemy_units.is_empty() && enemy_tiles.is_empty() {
        return cmds; // pas en guerre / plus rien à conquérir
    }

    // 1) Recrutement (1/tour) à une caserne libre, sous un plafond ∝ territoire.
    let my_units = world.units.iter().filter(|u| u.owner == nation).count() as u32;
    let tiles_owned = world.tiles.iter().filter(|t| t.owner == Some(nation)).count() as u32;
    let cap = tiles_owned.clamp(3, 24);
    if my_units < cap {
        if let Some(n) = world.nation(nation) {
            let kind = if n.tech[FER] >= 2 {
                UnitKind::Cavalry
            } else if n.tech[FER] >= 1 {
                UnitKind::Archer
            } else {
                UnitKind::Infantry
            };
            let s = unit_stats(kind);
            // Manpower (national) suffisant pour ce type ?
            if n.manpower >= s.cost_force && n.money >= s.cost_money {
                for (idx, t) in world.tiles.iter().enumerate() {
                    if t.owner == Some(nation) && t.building == Some(Building::Military) {
                        let (x, y) = coords(idx, width);
                        if !world.units.iter().any(|u| u.x == x && u.y == y) {
                            cmds.push(Command::CreateUnit { x, y, nation, kind });
                            break;
                        }
                    }
                }
            }
        }
    }

    // 2) Unités TERRESTRES : attaquer à portée, sinon avancer vers l'ennemi.
    for u in world
        .units
        .iter()
        .filter(|u| u.owner == nation && !unit_stats(u.kind).naval)
    {
        let range = unit_stats(u.kind).range;
        let target = enemy_units
            .iter()
            .map(|&(ex, ey)| (manhattan(u.x, u.y, ex, ey, width), ex, ey))
            .filter(|&(d, _, _)| d >= 1 && d <= range)
            .min();
        if let Some((_, ex, ey)) = target {
            cmds.push(Command::AttackUnit {
                unit: u.id,
                x: ex,
                y: ey,
            });
            continue;
        }
        // Adjacent à une galère chargeable -> rester sur place pour embarquer
        // (sinon l'unité s'éloignerait vers l'ennemi et raterait le bateau).
        if let Some((gx, gy)) = nearest_loadable_galley(world, nation, u) {
            if manhattan(u.x, u.y, gx, gy, width) <= 1 {
                continue;
            }
        }
        // Avancer vers la case ennemie la plus proche (par terre).
        let mut moved = false;
        if let Some(&(tx, ty)) = enemy_tiles
            .iter()
            .min_by_key(|&&(tx, ty)| manhattan(u.x, u.y, tx, ty, width))
        {
            if let Some((nx, ny)) = march_toward(world, nation, u, tx, ty) {
                cmds.push(Command::MoveUnit {
                    unit: u.id,
                    to_x: nx,
                    to_y: ny,
                });
                moved = true;
            }
        }
        // Bloqué par la mer ? rejoindre une galère amie avec de la place (pour
        // embarquer ensuite) — c'est ainsi que l'IA monte une invasion maritime.
        if !moved {
            if let Some((gx, gy)) = nearest_loadable_galley(world, nation, u) {
                if manhattan(u.x, u.y, gx, gy, width) > 1 {
                    if let Some((nx, ny)) = march_toward(world, nation, u, gx, gy) {
                        cmds.push(Command::MoveUnit {
                            unit: u.id,
                            to_x: nx,
                            to_y: ny,
                        });
                    }
                }
            }
        }
    }

    // 3) NAVAL : port → galère → embarquer une unité → débarquer chez l'ennemi.
    let has_port = world
        .tiles
        .iter()
        .any(|t| t.owner == Some(nation) && t.building == Some(Building::Port));
    if !has_port {
        if let Some((px, py)) = coastal_water_to_build(world, nation) {
            let (m, mat, hh) = sim::build_cost(Building::Port);
            if let Some(n) = world.nation(nation) {
                if n.money >= m && n.materials >= mat && n.housing >= hh {
                    cmds.push(Command::Build {
                        x: px,
                        y: py,
                        nation,
                        building: Building::Port,
                    });
                }
            }
        }
    }
    let galleys = world
        .units
        .iter()
        .filter(|u| u.owner == nation && unit_stats(u.kind).naval)
        .count();
    if has_port && galleys < 2 {
        let s = unit_stats(UnitKind::Galley);
        if world.nation(nation).is_some_and(|n| n.manpower >= s.cost_force && n.money >= s.cost_money)
        {
            for (idx, t) in world.tiles.iter().enumerate() {
                if t.owner == Some(nation) && t.building == Some(Building::Port) {
                    let (x, y) = coords(idx, width);
                    if !world.units.iter().any(|u| u.x == x && u.y == y) {
                        cmds.push(Command::CreateUnit {
                            x,
                            y,
                            nation,
                            kind: UnitKind::Galley,
                        });
                        break;
                    }
                }
            }
        }
    }
    for g in world
        .units
        .iter()
        .filter(|u| u.owner == nation && unit_stats(u.kind).naval)
    {
        let cap = unit_stats(g.kind).capacity as usize;
        let load = world.cargo.get(&g.id).map_or(0, |v| v.len());
        // Charger une unité terrestre adjacente (jusqu'à pleine capacité).
        if load < cap {
            if let Some(uid) = adjacent_land_unit(world, nation, g.x, g.y) {
                cmds.push(Command::Embark {
                    unit: uid,
                    transport: g.id,
                });
                continue;
            }
        }
        if load > 0 {
            // Débarquer si une terre ennemie est adjacente.
            if let Some((ex, ey)) = adjacent_enemy_land(world, nation, g.x, g.y) {
                cmds.push(Command::Disembark {
                    transport: g.id,
                    to_x: ex,
                    to_y: ey,
                });
                continue;
            }
            // Sinon naviguer vers la côte ennemie la plus proche.
            if let Some(&(tx, ty)) = enemy_tiles
                .iter()
                .min_by_key(|&&(tx, ty)| manhattan(g.x, g.y, tx, ty, width))
            {
                if let Some((nx, ny)) = march_toward(world, nation, g, tx, ty) {
                    cmds.push(Command::MoveUnit {
                        unit: g.id,
                        to_x: nx,
                        to_y: ny,
                    });
                }
            }
        }
    }
    cmds
}

/// Case d'eau côtière (adjacente à une terre possédée), non bâtie, où poser un port.
fn coastal_water_to_build(world: &World, nation: u16) -> Option<(u32, u32)> {
    let (w, h) = (world.width as i64, world.height as i64);
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.kind != TileKind::Ocean
            || t.building.is_some()
            || matches!(t.owner, Some(o) if o != nation)
        {
            continue;
        }
        let (x, y) = (idx as i64 % w, idx as i64 / w);
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let nx = (x + dx).rem_euclid(w);
            let ny = y + dy;
            if ny < 0 || ny >= h {
                continue;
            }
            let v = (ny * w + nx) as usize;
            if world.tiles[v].kind == TileKind::Land && world.tiles[v].owner == Some(nation) {
                return Some(coords(idx, world.width));
            }
        }
    }
    None
}

/// Case de TERRE ennemie (en guerre, libre d'unité) adjacente à (x,y) — débarquement.
fn adjacent_enemy_land(world: &World, nation: u16, x: u32, y: u32) -> Option<(u32, u32)> {
    let (w, h) = (world.width as i64, world.height as i64);
    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
        let nx = (x as i64 + dx).rem_euclid(w);
        let ny = y as i64 + dy;
        if ny < 0 || ny >= h {
            continue;
        }
        let (nx, ny) = (nx as u32, ny as u32);
        let t = world.tile(nx, ny);
        if t.kind == TileKind::Land
            && matches!(t.owner, Some(o) if o != nation && world.diplomacy.at_war(nation, o))
            && !world.units.iter().any(|u| u.x == nx && u.y == ny)
        {
            return Some((nx, ny));
        }
    }
    None
}

/// Position de la galère amie la plus proche AYANT de la place (à rejoindre pour
/// embarquer).
fn nearest_loadable_galley(world: &World, nation: u16, u: &Unit) -> Option<(u32, u32)> {
    let mut best = None;
    let mut best_d = u32::MAX;
    for g in &world.units {
        if g.owner != nation || !unit_stats(g.kind).naval {
            continue;
        }
        let cap = unit_stats(g.kind).capacity as usize;
        if world.cargo.get(&g.id).map_or(0, |v| v.len()) >= cap {
            continue;
        }
        let d = manhattan(u.x, u.y, g.x, g.y, world.width);
        if d < best_d {
            best_d = d;
            best = Some((g.x, g.y));
        }
    }
    best
}

/// Id d'une unité TERRESTRE amie adjacente à (x,y) (à embarquer).
fn adjacent_land_unit(world: &World, nation: u16, x: u32, y: u32) -> Option<u32> {
    let (w, h) = (world.width as i64, world.height as i64);
    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
        let nx = (x as i64 + dx).rem_euclid(w);
        let ny = y as i64 + dy;
        if ny < 0 || ny >= h {
            continue;
        }
        let (nx, ny) = (nx as u32, ny as u32);
        if let Some(u) = world
            .units
            .iter()
            .find(|u| u.x == nx && u.y == ny && u.owner == nation && !unit_stats(u.kind).naval)
        {
            return Some(u.id);
        }
    }
    None
}

/// Capacité minimale d'une case « accueillante » : la ville de départ peut alors
/// croître bien au-delà de 1000 (donc s'étendre), sans soft-lock de démarrage.
const HOSPITABLE_CAP: f32 = 1500.0;

/// Place `count` nations sur des terres **accueillantes** (haute capacité), tirées
/// **aléatoirement mais de façon seedée** : même graine ⇒ même placement ⇒ rejeu
/// identique (contrat de déterminisme). On conserve un espacement minimal, avec
/// repli (capacité plus basse, puis distance relâchée) si la carte est avare.
pub fn spawn_nations(world: &World, count: u16, player: u16) -> Vec<Command> {
    let mut out = Vec::new();
    if count == 0 {
        return out;
    }
    // Candidats : on descend les paliers de capacité jusqu'à en avoir assez.
    let mut candidates: Vec<(u32, u32)> = Vec::new();
    for &thr in &[HOSPITABLE_CAP, 1000.0, 600.0, 400.0, 1.0] {
        candidates.clear();
        for y in 0..world.height {
            for x in 0..world.width {
                if world.tile(x, y).kind == TileKind::Land && world.capacity_at(x, y) >= thr {
                    candidates.push((x, y));
                }
            }
        }
        if candidates.len() >= count as usize {
            break;
        }
    }
    if candidates.is_empty() {
        return out;
    }
    // Mélange déterministe (Fisher–Yates) avec un RNG seedé indépendant de la
    // worldgen (sel sur la graine) → tirage « aléatoire » mais rejouable.
    let mut rng = Rng::new(world.seed ^ 0xA53C_9E2D_7F10_4B6B);
    for i in (1..candidates.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        candidates.swap(i, j);
    }
    // Distance min pour répartir les nations, puis repli sans contrainte.
    let span = world.width.min(world.height) as f32;
    let min_dist = (span / (2.0 * (count as f32).sqrt())).max(10.0) as i64;
    let mut placed: Vec<(u32, u32)> = Vec::new();
    for &(x, y) in &candidates {
        if placed.len() == count as usize {
            break;
        }
        if placed
            .iter()
            .all(|&(px, py)| distance(x, y, px, py, world.width) >= min_dist)
        {
            placed.push((x, y));
        }
    }
    if placed.len() < count as usize {
        for &(x, y) in &candidates {
            if placed.len() == count as usize {
                break;
            }
            if !placed.contains(&(x, y)) {
                placed.push((x, y));
            }
        }
    }
    let n = placed.len() as u16;
    for (i, (x, y)) in placed.into_iter().enumerate() {
        let nation = i as u16;
        out.push(Command::Settle {
            x,
            y,
            nation,
            population: 300,
        });
        // La case d'implantation devient une VILLE : la population ne croît que sur
        // les villes, donc chaque nation démarre avec un foyer en croissance.
        out.push(Command::Build {
            x,
            y,
            nation,
            building: Building::City,
        });
    }
    // Coup de pouce aux IA (toutes sauf le joueur) : ressources de départ pour
    // qu'elles se développent vite. Commandes ENREGISTRÉES (rejeu déterministe).
    for id in 0..n {
        if id == player {
            continue;
        }
        out.push(Command::Endow {
            nation: id,
            money: 3000,
            materials: 800,
            influence: 60,
            housing: 250,
            food: 600,
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
