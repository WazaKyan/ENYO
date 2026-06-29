//! Fenêtre de jeu ENYO (minifb) — Phases A→C.
//!
//! Affichage pixel-art (viewport `render`) + jeu TOUR PAR TOUR.
//! Contrôles : WASD/flèches = se déplacer · molette = zoom · **Espace = Fin de
//! tour** (Step + Directeur + IA) · clic = inspecter une case · **F** = outil
//! Fonder · **E** = outil Essaimer (2 clics) · **N** = aucun outil · Échap = quitter.
//! Recherche (mode joueur) : **1/2/3/4** = Essor / Terroir / Fer / Lien.
//! Spectateur (`--spectator`) : **0/1/2** = pause / ×1 / ×2 (auto-tour).
//! Le HUD textuel est dans la BARRE DE TITRE (pas de police à coder).
//! Mode agent : `--headless --shot f.png` (même image que la fenêtre).

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use proto::{Command, Event};
use sim::World;

const WIN_W: u32 = 1280;
const WIN_H: u32 = 720;

#[derive(PartialEq, Clone, Copy)]
enum Tool {
    None,
    Found,
    Swarm,
}

/// Résout un tour : Step + Directeur + IA des autres nations (toutes si spectateur).
fn end_turn(world: &mut World, player: u16, nations: u16, spectator: bool) {
    world.apply(Command::Step);
    for cmd in ai::direct(world, player) {
        world.apply(cmd);
    }
    for nid in 0..nations {
        if !spectator && nid == player {
            continue; // le joueur contrôle sa nation lui-même
        }
        for cmd in ai::plan(world, nid) {
            world.apply(cmd);
        }
    }
}

fn main() {
    let args = Args::parse();
    let mut world = World::new(args.seed, 800, 500);
    for cmd in ai::spawn_nations(&world, args.nations) {
        world.apply(cmd);
    }
    for _ in 0..args.pre_turns {
        end_turn(&mut world, args.player, args.nations, true);
    }

    let (cam0_x, cam0_y) = render::nation_bbox(&world, args.player, 0)
        .map(|(x, y, w, h)| (x + w / 2, y + h / 2))
        .unwrap_or((world.width / 2, world.height / 2));

    if args.headless {
        let path = args.shot.as_deref().unwrap_or("out/ui.png");
        if let Some(p) = std::path::Path::new(path).parent() {
            if !p.as_os_str().is_empty() {
                std::fs::create_dir_all(p).ok();
            }
        }
        match render::viewport_png(&world, cam0_x, cam0_y, args.px, WIN_W, WIN_H, path) {
            Ok(()) => println!("capture écrite: {path}"),
            Err(e) => eprintln!("échec capture: {e}"),
        }
        return;
    }

    let mut window = Window::new("ENYO", WIN_W as usize, WIN_H as usize, WindowOptions::default())
        .expect("ouverture de la fenêtre");
    window.set_target_fps(30);

    let mut cam_x = cam0_x;
    let mut cam_y = cam0_y;
    let mut px = args.px;
    let mut tool = Tool::None;
    let mut swarm_src: Option<(u32, u32)> = None;
    let mut selected: Option<(u32, u32)> = None;
    let mut speed: u32 = if args.spectator { 1 } else { 0 }; // 0=pause,1=x1,2=x2
    let mut frame: u64 = 0;
    let mut mouse_was_down = false;
    let mut dirty = true;
    let mut last_msg = String::new();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        frame = frame.wrapping_add(1);

        // Déplacement (touche maintenue).
        let pan = 2;
        if window.is_key_down(Key::A) || window.is_key_down(Key::Left) {
            cam_x = cam_x.saturating_sub(pan);
        }
        if window.is_key_down(Key::D) || window.is_key_down(Key::Right) {
            cam_x = (cam_x + pan).min(world.width - 1);
        }
        if window.is_key_down(Key::W) || window.is_key_down(Key::Up) {
            cam_y = cam_y.saturating_sub(pan);
        }
        if window.is_key_down(Key::S) || window.is_key_down(Key::Down) {
            cam_y = (cam_y + pan).min(world.height - 1);
        }
        if let Some((_, sy)) = window.get_scroll_wheel() {
            if sy > 0.0 {
                px = (px + 4).min(40);
            } else if sy < 0.0 {
                px = px.saturating_sub(4).max(6);
            }
        }

        // Outils.
        if window.is_key_pressed(Key::F, KeyRepeat::No) {
            tool = Tool::Found;
            swarm_src = None;
            dirty = true;
        }
        if window.is_key_pressed(Key::E, KeyRepeat::No) {
            tool = Tool::Swarm;
            swarm_src = None;
            dirty = true;
        }
        if window.is_key_pressed(Key::N, KeyRepeat::No) {
            tool = Tool::None;
            swarm_src = None;
            dirty = true;
        }

        // Chiffres : vitesse en spectateur, recherche en mode joueur
        // (les deux modes sont exclusifs, donc pas de conflit de touches).
        if args.spectator {
            if window.is_key_pressed(Key::Key0, KeyRepeat::No) {
                speed = 0;
            }
            if window.is_key_pressed(Key::Key1, KeyRepeat::No) {
                speed = 1;
            }
            if window.is_key_pressed(Key::Key2, KeyRepeat::No) {
                speed = 2;
            }
        } else {
            let branches = [Key::Key1, Key::Key2, Key::Key3, Key::Key4];
            for (b, k) in branches.iter().enumerate() {
                if window.is_key_pressed(*k, KeyRepeat::No) {
                    let ev = world.apply(Command::Research {
                        nation: args.player,
                        branch: b as u8,
                    });
                    if let Some(m) = feedback(&ev) {
                        last_msg = m;
                    }
                    dirty = true;
                }
            }
        }

        // Fin de tour : manuelle (Espace) ou auto (spectateur selon la vitesse).
        let mut stepped = false;
        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            end_turn(&mut world, args.player, args.nations, args.spectator);
            stepped = true;
        }
        if args.spectator && speed > 0 {
            let interval: u64 = if speed >= 2 { 15 } else { 30 };
            if frame.is_multiple_of(interval) {
                end_turn(&mut world, args.player, args.nations, true);
                stepped = true;
            }
        }
        if stepped {
            dirty = true;
        }

        // Clic gauche : inspecter / agir selon l'outil.
        let left_down = window.get_mouse_down(MouseButton::Left);
        if left_down && !mouse_was_down {
            if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Discard) {
                let (x0, y0, _, _, pxe) = render::viewport_rect(&world, cam_x, cam_y, px, WIN_W, WIN_H);
                let tx = x0 + (mx as u32) / pxe;
                let ty = y0 + (my as u32) / pxe;
                if tx < world.width && ty < world.height {
                    selected = Some((tx, ty));
                    match tool {
                        Tool::Found => {
                            let ev = world.apply(Command::Settle {
                                x: tx,
                                y: ty,
                                nation: args.player,
                                population: 300,
                            });
                            if let Some(m) = feedback(&ev) {
                                last_msg = m;
                            }
                        }
                        Tool::Swarm => {
                            if let Some((sx, sy)) = swarm_src.take() {
                                let ev = world.apply(Command::Swarm {
                                    from_x: sx,
                                    from_y: sy,
                                    to_x: tx,
                                    to_y: ty,
                                });
                                if let Some(m) = feedback(&ev) {
                                    last_msg = m;
                                }
                            } else {
                                swarm_src = Some((tx, ty));
                                last_msg = format!("source ({tx},{ty}) — clique la cible");
                            }
                        }
                        Tool::None => {}
                    }
                    dirty = true;
                }
            }
        }
        mouse_was_down = left_down;

        // HUD (barre de titre) — recalculé seulement quand l'état change.
        if dirty {
            window.set_title(&hud(&world, &args, tool, selected, &last_msg));
            dirty = false;
        }

        let buf = render::viewport_argb(&world, cam_x, cam_y, px, WIN_W, WIN_H);
        window
            .update_with_buffer(&buf, WIN_W as usize, WIN_H as usize)
            .expect("affichage");
    }
}

/// Traduit le résultat d'une commande joueur en message court pour le HUD
/// (rejet ou succès) — rien n'est silencieux côté joueur non plus.
fn feedback(events: &[Event]) -> Option<String> {
    for e in events {
        match e {
            Event::CommandRejected { reason } => return Some(format!("REJET : {reason}")),
            Event::Settled {
                x, y, population, ..
            } => return Some(format!("fonde ({x},{y}) +{population} hab")),
            Event::Swarmed {
                to_x, to_y, moved, ..
            } => return Some(format!("essaimage vers ({to_x},{to_y}) +{moved:.0} hab")),
            Event::Researched { branch, tier, .. } => {
                let b = ["Essor", "Terroir", "Fer", "Lien"]
                    .get(*branch as usize)
                    .copied()
                    .unwrap_or("?");
                return Some(format!("{b} palier {tier}"));
            }
            _ => {}
        }
    }
    None
}

/// Construit la ligne de HUD affichée dans la barre de titre.
fn hud(world: &World, args: &Args, tool: Tool, selected: Option<(u32, u32)>, last_msg: &str) -> String {
    let (pop, tiles) = world.nation_stats(args.player);
    let prov = world
        .provinces()
        .iter()
        .filter(|p| p.owner == args.player)
        .count();
    let n = world.nation(args.player);
    let kn = n.map(|n| n.knowledge).unwrap_or(0.0);
    let t = n.map(|n| n.tech).unwrap_or_default();
    let year = world.turn / 12;
    let month = world.turn % 12 + 1;
    let mode = if args.spectator { "SPECTATEUR" } else { "JEU" };
    let toolname = match tool {
        Tool::None => "—",
        Tool::Found => "Fonder",
        Tool::Swarm => "Essaimer",
    };
    let research_hint = if args.spectator {
        "0/1/2=vitesse"
    } else {
        "1-4=recherche"
    };
    let mut s = format!(
        "ENYO | An {year} M{month} | {mode} N{} : {pop:.0} hab, {tiles} cases, {prov} prov | savoir {kn:.0} | tech E{} T{} F{} L{} | Outil: {toolname} (F/E/N, Espace=fin de tour, {research_hint})",
        args.player, t[0], t[1], t[2], t[3]
    );
    if !last_msg.is_empty() {
        s.push_str(&format!(" | >> {last_msg}"));
    }
    if let Some((tx, ty)) = selected {
        let t = world.tile(tx, ty);
        let owner = t
            .owner
            .map(|o| format!("N{o}"))
            .unwrap_or_else(|| "libre".to_string());
        s.push_str(&format!(
            " || Case ({tx},{ty}) {:?} | pop {:.0} | dev {:.2} | cap {:.0} | force {:.0} | {owner}",
            t.biome,
            t.population,
            t.development,
            world.capacity_at(tx, ty),
            t.force,
        ));
    }
    s
}

/// Arguments de la ligne de commande.
struct Args {
    seed: u64,
    nations: u16,
    player: u16,
    pre_turns: usize,
    px: u32,
    spectator: bool,
    headless: bool,
    shot: Option<String>,
}

impl Args {
    fn parse() -> Self {
        let mut a = Args {
            seed: 2026,
            nations: 8,
            player: 0,
            pre_turns: 0,
            px: 16,
            spectator: false,
            headless: false,
            shot: None,
        };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--seed" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        a.seed = v;
                    }
                }
                "--nations" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        a.nations = v;
                    }
                }
                "--player" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        a.player = v;
                    }
                }
                "--pre-turns" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        a.pre_turns = v;
                    }
                }
                "--px" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        a.px = v;
                    }
                }
                "--spectator" => a.spectator = true,
                "--headless" => a.headless = true,
                "--shot" => a.shot = it.next(),
                other => eprintln!("argument ignoré : {other}"),
            }
        }
        a
    }
}
