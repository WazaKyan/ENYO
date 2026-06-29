//! IA **baseline déterministe** (Phase 4) : des nations non-joueuses qui
//! s'implantent, recherchent, s'étendent — et se font la **guerre** quand le
//! grief monte (Phase conflit).
//!
//! L'IA est PURE : elle observe le monde et renvoie des [`Command`]. Elle ne
//! modifie jamais l'état directement — `sim` reste sans IA et tout reste
//! rejouable/auditable. C'est aussi la base que le futur Directeur (Phase 5)
//! orchestrera.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use proto::{Building, Command, UnitKind};
use sim::nation::{ESSOR, FER, LIEN, TERROIR};
use sim::rng::Rng;
use sim::tile::TileKind;
use sim::unit::{unit_stats, Unit};
use sim::World;

mod director;
pub use director::{Director, Intent, Stance};

/// Branches **économie / expansion** que l'IA fait toujours progresser : capacité
/// de charge (Terroir), portée d'essaimage (Essor), liens navals (Lien). Le
/// **militaire (Fer)** s'ajoute dès qu'elle a une caserne — cf. [`plan`] — pour
/// fielder Archers puis Cavalerie au lieu de rester à l'Infanterie.
const AI_ECON_BRANCHES: [usize; 3] = [TERROIR, ESSOR, LIEN];

/// Seuil de grief au-delà duquel l'IA déclare la guerre.
const WAR_THRESHOLD: f32 = 3.0;

/// Plan d'un tour pour une nation IA : recherche + expansion + diplomatie/guerre.
pub fn plan(world: &World, nation: u16) -> Vec<Command> {
    let mut cmds = Vec::new();

    // Recherche : économie + expansion en continu ; militaire (Fer) une fois la
    // nation militarisée (caserne). On pousse la branche au plus bas palier
    // d'abord (round-robin équilibré), si le savoir suffit.
    if let Some(n) = world.nation(nation) {
        let mut branches: Vec<usize> = AI_ECON_BRANCHES.to_vec();
        if has_barracks(world, nation) {
            branches.push(FER);
        }
        let (branch, tier) = branches
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

/// Essaimage glouton **orienté vers le rival le plus proche** (agression : les
/// territoires se rejoignent → friction frontalière → guerre atteignable). Chaque
/// case ≥1000 hab. s'étend vers la terre adjacente libre qui **rapproche le plus du
/// rival** (à défaut, la première libre), dans la limite de l'influence (E5).
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
    // Ancre du rival le plus proche : on pousse l'expansion dans sa direction.
    let rival = nearest_rival(world, nation).and_then(|r| nation_anchor(world, r));
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
        // Voisin libre qui rapproche le plus du rival (sinon le premier libre).
        let mut best: Option<(usize, u32, u32, u32)> = None; // (v, nx, ny, dist_rival)
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let nx = (x + dx).rem_euclid(w);
            let ny = y + dy;
            if ny < 0 || ny >= h {
                continue;
            }
            let v = (ny * w + nx) as usize;
            let nt = &world.tiles[v];
            if nt.kind != TileKind::Land || nt.owner.is_some() || targeted.contains(&v) {
                continue;
            }
            let dist = rival
                .map(|(rx, ry)| manhattan(nx as u32, ny as u32, rx, ry, world.width))
                .unwrap_or(0);
            let better = match best {
                None => true,
                Some((_, _, _, bd)) => dist < bd,
            };
            if better {
                best = Some((v, nx as u32, ny as u32, dist));
            }
        }
        if let Some((v, nx, ny, _)) = best {
            targeted.insert(v);
            cmds.push(Command::Swarm {
                from_x: x as u32,
                from_y: y as u32,
                to_x: nx,
                to_y: ny,
            });
            budget -= 1;
        }
    }
    cmds
}

/// Économie IA (déterministe) : choisit, parmi les cases libres possédées, **le
/// bâtiment le plus utile au développement** et **la meilleure case** où le poser
/// (un bâtiment/tour). La stratégie encode une vraie logique économique : un
/// moteur de population (**villes**) nourri (**fermes**), alimenté en
/// matériaux→argent/habitation (**industrie**→**commerce**), qui progresse en
/// science (**éducation** → tech), se défend (**caserne**) et **connecte** un
/// territoire étalé (**infra**, sinon les bâtiments isolés chôment). On bâtit le
/// PREMIER souhait ABORDABLE (jamais de blocage si le plus prioritaire est trop
/// cher). Respecte les coûts (argent / matériaux / habitation).
fn economy(world: &World, nation: u16) -> Vec<Command> {
    let Some(n) = world.nation(nation) else {
        return Vec::new();
    };
    let (mut cities, mut industries, mut farms) = (0i64, 0i64, 0i64);
    let (mut commerces, mut educations, mut infras, mut militaries) = (0i64, 0i64, 0i64, 0i64);
    let mut empty: Vec<usize> = Vec::new();
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.owner != Some(nation) {
            continue;
        }
        match t.building {
            Some(Building::City) => cities += 1,
            Some(Building::Industry) => industries += 1,
            Some(Building::Farm) => farms += 1,
            Some(Building::Commerce) => commerces += 1,
            Some(Building::Education) => educations += 1,
            Some(Building::Infrastructure) => infras += 1,
            Some(Building::Military) => militaries += 1,
            Some(Building::Port) => {}
            None if t.kind == TileKind::Land => empty.push(idx),
            _ => {}
        }
    }
    if empty.is_empty() {
        return Vec::new(); // pas de case terre libre où bâtir
    }
    let tiles_owned = world.nation_stats(nation).1 as i64;
    let at_war = at_war_with_anyone(world, nation);
    // AGRESSION MAXIMALE : on s'arme dès qu'un rival existe (peu importe la
    // distance), pour pouvoir l'attaquer dès que possible. Plus de caserne tardive.
    let militarize = at_war || nearest_rival_distance(world, nation).is_some();

    // Liste de souhaits ORDONNÉE par priorité de développement ; on bâtira le
    // premier réellement abordable.
    let mut wish: Vec<Building> = Vec::new();
    // 1) Amorce de la chaîne : un exemplaire de chaque maillon essentiel.
    if cities == 0 {
        wish.push(Building::City);
    }
    if industries == 0 {
        wish.push(Building::Industry);
    }
    if farms == 0 {
        wish.push(Building::Farm);
    }
    if militaries == 0 && militarize {
        // Caserne face à une menace, et **tôt** (tant que les matériaux sont frais
        // — c'est le bâtiment le plus gourmand) : défense, et surtout débloque la
        // recherche **Fer** + le **recrutement** d'unités. Sinon le commerce épuise
        // les matériaux et la nation resterait désarmée au mauvais moment.
        wish.push(Building::Military);
    }
    if commerces == 0 {
        wish.push(Building::Commerce);
    }
    if educations == 0 {
        // Science → tech. Exige un commerce connecté pour produire (donc après).
        wish.push(Building::Education);
    }
    // 2) Équilibrage proportionnel au nombre de villes (le moteur de pop).
    if farms < cities {
        wish.push(Building::Farm); // nourrir chaque ville dense
    }
    if commerces < cities {
        wish.push(Building::Commerce); // argent + habitation
    }
    if industries < cities {
        wish.push(Building::Industry); // matériaux
    }
    if educations * 2 < cities {
        wish.push(Building::Education); // plus de science
    }
    if militarize && militaries < cities {
        wish.push(Building::Military); // plus de casernes = plus de recrutement
    }
    // 3) Connecter un territoire étalé.
    if infras * 6 < tiles_owned {
        wish.push(Building::Infrastructure);
    }
    // 4) Sinon : grandir — une ville de plus = plus de population = plus de tout.
    wish.push(Building::City);

    let affordable = |b: Building| {
        let (m, mat, h) = sim::build_cost(b);
        n.money >= m && n.materials >= mat && n.housing >= h
    };
    let Some(pick) = wish.into_iter().find(|&b| affordable(b)) else {
        return Vec::new();
    };
    let idx = best_build_tile(world, &empty, pick);
    let (x, y) = coords(idx, world.width);
    vec![Command::Build {
        x,
        y,
        nation,
        building: pick,
    }]
}

/// Meilleure case libre pour un bâtiment : **villes & fermes** sur la **terre la
/// plus fertile** (capacité de charge max → plus de population / de nourriture) ;
/// les autres bâtiments sur la **moins fertile** (on préserve la bonne terre pour
/// ce qui en profite). Égalités tranchées par index croissant → déterministe.
fn best_build_tile(world: &World, empty: &[usize], b: Building) -> usize {
    let prefer_high = matches!(b, Building::City | Building::Farm);
    let width = world.width;
    let mut best = empty[0];
    let (bx, by) = coords(best, width);
    let mut best_cap = world.capacity_at(bx, by);
    for &idx in &empty[1..] {
        let (x, y) = coords(idx, width);
        let cap = world.capacity_at(x, y);
        if (prefer_high && cap > best_cap) || (!prefer_high && cap < best_cap) {
            best_cap = cap;
            best = idx;
        }
    }
    best
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

/// Distance (Manhattan, ancre) au rival le plus proche — sert à décider de se
/// militariser ou non (menace de proximité).
fn nearest_rival_distance(world: &World, nation: u16) -> Option<u32> {
    let me = nation_anchor(world, nation)?;
    world
        .nations
        .iter()
        .filter(|o| o.id != nation)
        .filter_map(|o| nation_anchor(world, o.id))
        .map(|p| manhattan(me.0, me.1, p.0, p.1, world.width))
        .min()
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

/// Prochaine destination vers (tx,ty) en **contournant les obstacles** (l'ancienne
/// descente gloutonne se coinçait sur le moindre relief/golfe → les unités
/// n'atteignaient jamais l'ennemi). Dijkstra **borné et déterministe** (tas min sur
/// (coût, index), tie-break index) depuis l'unité, coût d'entrée selon le domaine
/// (terre/eau + intempéries). Reconstruit le chemin vers la cible (ou, si elle est
/// hors d'atteinte, vise le nœud exploré le plus proche du but → progrès chaque
/// tour) et renvoie la case la **plus loin atteignable ce tour** (coût cumulé ≤
/// `moves_left`) qui soit **libre d'unité**. HashMap utilisée en LECTURE seule
/// (jamais itérée pour décider) → rejeu identique.
fn march_toward(world: &World, nation: u16, u: &Unit, tx: u32, ty: u32) -> Option<(u32, u32)> {
    let is_naval = unit_stats(u.kind).naval;
    let naval_tier = world.nation(nation).map(|n| n.tech[LIEN]).unwrap_or(0);
    let width = world.width;
    let (w, h) = (world.width as i64, world.height as i64);
    let start = (u.y * width + u.x) as usize;
    let target = (ty * width + tx) as usize;
    if start == target {
        return None;
    }
    let mut dist: HashMap<usize, u32> = HashMap::new();
    let mut prev: HashMap<usize, usize> = HashMap::new();
    dist.insert(start, 0);
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u32, start)));
    // Cible hors d'atteinte ? on retiendra le nœud exploré le plus proche du but
    // (min (manhattan, coût, index)), mis à jour à chaque extraction (ordre du tas
    // déterministe) → l'unité progresse quand même vers l'ennemi.
    let mut best_goal = start;
    let mut best_key = (manhattan(u.x, u.y, tx, ty, width), 0u32, start);
    let mut reached = false;
    let mut explored = 0usize;
    while let Some(Reverse((d, idx))) = heap.pop() {
        if d > *dist.get(&idx).unwrap_or(&u32::MAX) {
            continue;
        }
        if idx == target {
            reached = true;
            break;
        }
        let (cx, cy) = (idx as u32 % width, idx as u32 / width);
        let key = (manhattan(cx, cy, tx, ty, width), d, idx);
        if key < best_key {
            best_key = key;
            best_goal = idx;
        }
        explored += 1;
        if explored > 4000 {
            break; // plafond : borne le coût d'une recherche sur grande carte
        }
        let (x, y) = (idx as i64 % w, idx as i64 / w);
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let nx = (x + dx).rem_euclid(w);
            let ny = y + dy;
            if ny < 0 || ny >= h {
                continue;
            }
            let v = (ny * w + nx) as usize;
            // Coût selon le domaine (terre/eau + intempéries). MAX = infranchissable.
            let cost = if is_naval {
                sim::path::naval_move_cost(world.tile(nx as u32, ny as u32))
            } else {
                sim::path::unit_move_cost(world.tile(nx as u32, ny as u32), naval_tier)
            };
            if cost == u32::MAX {
                continue;
            }
            let nd = d.saturating_add(cost);
            if nd < *dist.get(&v).unwrap_or(&u32::MAX) {
                dist.insert(v, nd);
                prev.insert(v, idx);
                heap.push(Reverse((nd, v)));
            }
        }
    }
    let goal = if reached { target } else { best_goal };
    if goal == start {
        return None;
    }
    // Reconstruit le chemin start..goal, puis prend la case la plus loin
    // atteignable ce tour (coût ≤ moves_left) qui soit libre d'unité.
    let mut path = vec![goal];
    let mut cur = goal;
    while cur != start {
        cur = *prev.get(&cur)?;
        path.push(cur);
    }
    path.reverse();
    let mut dest = None;
    for &idx in &path {
        if idx == start {
            continue;
        }
        if *dist.get(&idx).unwrap_or(&u32::MAX) > u.moves_left {
            break;
        }
        let (cx, cy) = (idx as u32 % width, idx as u32 / width);
        if !world.units.iter().any(|x| x.x == cx && x.y == cy) {
            dest = Some((cx, cy));
        }
    }
    // « Au moins une case » : sinon, premier pas à pleins points de mouvement.
    dest.or_else(|| step_at_full_moves(world, u, &path, width))
}

/// Marche vers la case ennemie **réellement atteignable** la plus proche (par coût
/// de chemin RÉEL — un seul Dijkstra borné déterministe depuis l'unité : la
/// première case ennemie extraite du tas est la moins coûteuse à rejoindre). Évite
/// le piège de viser une cible « proche à vol d'oiseau » mais coincée derrière la
/// mer (l'unité restait alors plantée sur sa côte). Renvoie la case la plus loin
/// atteignable ce tour (libre d'unité) sur ce chemin ; `None` si aucune cible
/// ennemie n'est joignable par terre dans l'horizon (→ l'appelant tente le naval).
fn march_to_enemy(world: &World, nation: u16, u: &Unit, enemy: &HashSet<usize>) -> Option<(u32, u32)> {
    let naval_tier = world.nation(nation).map(|n| n.tech[LIEN]).unwrap_or(0);
    let width = world.width;
    let (w, h) = (world.width as i64, world.height as i64);
    let start = (u.y * width + u.x) as usize;
    let mut dist: HashMap<usize, u32> = HashMap::new();
    let mut prev: HashMap<usize, usize> = HashMap::new();
    dist.insert(start, 0);
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u32, start)));
    let mut explored = 0usize;
    let mut goal: Option<usize> = None;
    while let Some(Reverse((d, idx))) = heap.pop() {
        if d > *dist.get(&idx).unwrap_or(&u32::MAX) {
            continue;
        }
        // Première cible ennemie extraite = la moins coûteuse à atteindre (Dijkstra).
        if idx != start && enemy.contains(&idx) {
            goal = Some(idx);
            break;
        }
        explored += 1;
        if explored > 12000 {
            break; // plafond : borne le coût sur très grande carte
        }
        let (x, y) = (idx as i64 % w, idx as i64 / w);
        for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
            let nx = (x + dx).rem_euclid(w);
            let ny = y + dy;
            if ny < 0 || ny >= h {
                continue;
            }
            let v = (ny * w + nx) as usize;
            let cost = sim::path::unit_move_cost(world.tile(nx as u32, ny as u32), naval_tier);
            if cost == u32::MAX {
                continue;
            }
            let nd = d.saturating_add(cost);
            if nd < *dist.get(&v).unwrap_or(&u32::MAX) {
                dist.insert(v, nd);
                prev.insert(v, idx);
                heap.push(Reverse((nd, v)));
            }
        }
    }
    let goal = goal?;
    // Reconstruit start..goal, puis la case la plus loin atteignable ce tour (libre).
    let mut path = vec![goal];
    let mut cur = goal;
    while cur != start {
        cur = *prev.get(&cur)?;
        path.push(cur);
    }
    path.reverse();
    let mut dest = None;
    for &idx in &path {
        if idx == start {
            continue;
        }
        if *dist.get(&idx).unwrap_or(&u32::MAX) > u.moves_left {
            break;
        }
        let (cx, cy) = (idx as u32 % width, idx as u32 / width);
        // La case-cible ennemie peut être libre (on l'occupe) ; une case
        // intermédiaire occupée par une unité est sautée (on prend la précédente).
        if !world.units.iter().any(|x| x.x == cx && x.y == cy) {
            dest = Some((cx, cy));
        }
    }
    // Si rien n'est atteignable dans le budget (case suivante trop chère — météo /
    // dévastation), on tente quand même le PREMIER pas à pleins points de mouvement
    // (le sim applique la règle « au moins une case ») → l'unité progresse toujours.
    dest.or_else(|| step_at_full_moves(world, u, &path, width))
}

/// Règle « au moins une case » côté IA : à pleins points de mouvement, renvoie la
/// première case du chemin (adjacente, libre) même si son coût dépasse le budget —
/// le sim l'autorise. Évite les unités gelées par une case trop chère.
fn step_at_full_moves(world: &World, u: &Unit, path: &[usize], width: u32) -> Option<(u32, u32)> {
    if u.moves_left != unit_stats(u.kind).moves {
        return None;
    }
    let &first = path.get(1)?;
    let (cx, cy) = (first as u32 % width, first as u32 / width);
    if world.units.iter().any(|x| x.x == cx && x.y == cy) {
        return None; // occupée par une unité : on attend
    }
    Some((cx, cy))
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
    let mut enemy_set: HashSet<usize> = HashSet::new();
    for (idx, t) in world.tiles.iter().enumerate() {
        if let Some(o) = t.owner {
            if o != nation && at_war(o) && t.occupier != Some(nation) {
                enemy_tiles.push(coords(idx, width));
                enemy_set.insert(idx);
            }
        }
    }
    if enemy_units.is_empty() && enemy_tiles.is_empty() {
        return cmds; // pas en guerre / plus rien à conquérir
    }

    // 1) Recrutement AGRESSIF : à CHAQUE caserne libre ce tour, sous un gros
    //    plafond ∝ territoire, dans la limite de ce qu'on peut payer (argent +
    //    manpower). Plus d'unités = on submerge l'occupation adverse → capitulation.
    let my_units = world.units.iter().filter(|u| u.owner == nation).count() as u32;
    let tiles_owned = world.tiles.iter().filter(|t| t.owner == Some(nation)).count() as u32;
    let cap = (tiles_owned * 2).clamp(6, 40);
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
            // Combien d'unités on peut s'offrir ce tour (plafond, argent, manpower).
            let mut budget = cap - my_units;
            if s.cost_money > 0 {
                budget = budget.min((n.money / s.cost_money) as u32);
            }
            if s.cost_force > 0 {
                budget = budget.min((n.manpower / s.cost_force) as u32);
            }
            for (idx, t) in world.tiles.iter().enumerate() {
                if budget == 0 {
                    break;
                }
                if t.owner == Some(nation) && t.building == Some(Building::Military) {
                    let (x, y) = coords(idx, width);
                    if !world.units.iter().any(|u| u.x == x && u.y == y) {
                        cmds.push(Command::CreateUnit { x, y, nation, kind });
                        budget -= 1;
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
        // ASSAUT TERRESTRE (priorité) : marcher vers la case ennemie réellement
        // ATTEIGNABLE la plus proche (coût de chemin RÉEL, pas à vol d'oiseau) → on
        // entre en territoire ennemi et on l'OCCUPE (collant) ; c'est ce qui fait
        // grimper le score de guerre jusqu'à la capitulation.
        if let Some((nx, ny)) = march_to_enemy(world, nation, u, &enemy_set) {
            cmds.push(Command::MoveUnit {
                unit: u.id,
                to_x: nx,
                to_y: ny,
            });
            continue;
        }
        // Aucune route terrestre vers l'ennemi → voie NAVALE : rejoindre une galère
        // chargeable (puis embarquer) pour franchir la mer.
        if let Some((gx, gy)) = nearest_loadable_galley(world, nation, u) {
            if manhattan(u.x, u.y, gx, gy, width) <= 1 {
                continue; // adjacent : attendre l'embarquement
            }
            if let Some((nx, ny)) = march_toward(world, nation, u, gx, gy) {
                cmds.push(Command::MoveUnit {
                    unit: u.id,
                    to_x: nx,
                    to_y: ny,
                });
                continue;
            }
        }
        // Sinon, fluage directionnel vers l'ennemi le plus proche à vol d'oiseau
        // (hors horizon de recherche mais peut-être atteignable de proche en proche).
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

/// Masque de la **plus grande masse continentale connexe** (4-connexité, X
/// enroulé). On y implante toutes les nations → elles sont **joignables par voie
/// terrestre**, condition pour que l'agression aboutisse (sinon chaque nation est
/// isolée sur son île). Flood-fill en ordre d'index → déterministe.
fn largest_land_component(world: &World) -> Vec<bool> {
    let (w, h) = (world.width as i64, world.height as i64);
    let n = world.tiles.len();
    let mut comp = vec![u32::MAX; n];
    let mut sizes: Vec<u32> = Vec::new();
    for s in 0..n {
        if world.tiles[s].kind != TileKind::Land || comp[s] != u32::MAX {
            continue;
        }
        let id = sizes.len() as u32;
        let mut size = 0u32;
        let mut stack = vec![s];
        comp[s] = id;
        while let Some(c) = stack.pop() {
            size += 1;
            let (x, y) = (c as i64 % w, c as i64 / w);
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let nx = (x + dx).rem_euclid(w);
                let ny = y + dy;
                if ny < 0 || ny >= h {
                    continue;
                }
                let v = (ny * w + nx) as usize;
                if world.tiles[v].kind == TileKind::Land && comp[v] == u32::MAX {
                    comp[v] = id;
                    stack.push(v);
                }
            }
        }
        sizes.push(size);
    }
    // Plus grande composante (égalité → plus petit id → déterministe).
    let best = sizes
        .iter()
        .enumerate()
        .max_by_key(|&(i, &s)| (s, Reverse(i)))
        .map(|(i, _)| i as u32);
    match best {
        Some(b) => comp.iter().map(|&c| c == b).collect(),
        None => vec![false; n],
    }
}

/// Place `count` nations sur des terres **accueillantes** (haute capacité), tirées
/// **aléatoirement mais de façon seedée** : même graine ⇒ même placement ⇒ rejeu
/// identique (contrat de déterminisme). On conserve un espacement minimal, avec
/// repli (capacité plus basse, puis distance relâchée) si la carte est avare.
pub fn spawn_nations(world: &World, count: u16, player: u16) -> Vec<Command> {
    let mut out = Vec::new();
    if count == 0 {
        return out;
    }
    // Toutes les nations sur la PLUS GRANDE masse continentale connexe : ainsi
    // elles peuvent se rejoindre par voie terrestre et l'agression aboutit (sinon
    // chaque nation reste sur son île, les armées ne se rencontrent jamais).
    let continent = largest_land_component(world);
    // Candidats accueillants DANS ce continent : paliers de capacité décroissants
    // jusqu'à en avoir assez (dernier palier = n'importe quelle terre du continent).
    let mut candidates: Vec<(u32, u32)> = Vec::new();
    for &thr in &[HOSPITABLE_CAP, 1000.0, 600.0, 400.0, 1.0] {
        candidates.clear();
        for y in 0..world.height {
            for x in 0..world.width {
                let i = (y * world.width + x) as usize;
                if continent[i] && world.capacity_at(x, y) >= thr {
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
    // Espacement minimal plus SERRÉ qu'avant (contact rapide → friction → guerre),
    // avec repli sans contrainte si le continent est petit.
    let span = world.width.min(world.height) as f32;
    let min_dist = (span / (3.0 * (count as f32).sqrt())).max(5.0) as i64;
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
