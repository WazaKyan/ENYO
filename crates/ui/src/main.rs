//! Fenêtre de jeu ENYO (minifb) — Phase 7b.
//!
//! Affiche le monde via un viewport pixel-art (`render::viewport_argb`), avec
//! **pan** (WASD / flèches) et **zoom** (molette). La fenêtre est un simple
//! consommateur passif du buffer de `render` : `sim` reste intouché, le
//! déterminisme intact.
//!
//! Mode **agent** : `ui --headless --shot f.png` rend EXACTEMENT la même image
//! que la fenêtre (même `RgbImage`), pour que l'agent puisse vérifier le rendu
//! sans ouvrir de fenêtre.
//!
//! Phase A : affichage + navigation (le tour-par-tour jouable vient en Phase B+).

use minifb::{Key, Window, WindowOptions};
use proto::Command;
use sim::World;

const WIN_W: u32 = 1280;
const WIN_H: u32 = 720;

fn main() {
    let args = Args::parse();

    // Construit un monde peuplé (même boucle que le harness) pour avoir à voir.
    let mut world = World::new(args.seed, 800, 500);
    for cmd in ai::spawn_nations(&world, args.nations) {
        world.apply(cmd);
    }
    for _ in 0..args.pre_turns {
        world.apply(Command::Step);
        for cmd in ai::direct(&world, args.player) {
            world.apply(cmd);
        }
        for nid in 0..args.nations {
            for cmd in ai::plan(&world, nid) {
                world.apply(cmd);
            }
        }
    }

    // Caméra initiale : centrée sur la nation du joueur (sinon centre de carte).
    let (cam0_x, cam0_y) = render::nation_bbox(&world, args.player, 0)
        .map(|(x, y, w, h)| (x + w / 2, y + h / 2))
        .unwrap_or((world.width / 2, world.height / 2));

    // Mode agent : rend la même image que la fenêtre, en PNG.
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

    // Fenêtre interactive.
    let mut window = Window::new(
        "ENYO",
        WIN_W as usize,
        WIN_H as usize,
        WindowOptions::default(),
    )
    .expect("ouverture de la fenêtre");
    window.set_target_fps(30);

    let mut cam_x = cam0_x;
    let mut cam_y = cam0_y;
    let mut px = args.px;

    while window.is_open() && !window.is_key_down(Key::Escape) {
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

        let buf = render::viewport_argb(&world, cam_x, cam_y, px, WIN_W, WIN_H);
        window
            .update_with_buffer(&buf, WIN_W as usize, WIN_H as usize)
            .expect("affichage");
    }
}

/// Arguments de la ligne de commande.
struct Args {
    seed: u64,
    nations: u16,
    player: u16,
    pre_turns: usize,
    px: u32,
    headless: bool,
    shot: Option<String>,
}

impl Args {
    fn parse() -> Self {
        let mut a = Args {
            seed: 2026,
            nations: 8,
            player: 0,
            pre_turns: 120,
            px: 16,
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
                "--headless" => a.headless = true,
                "--shot" => a.shot = it.next(),
                other => eprintln!("argument ignoré : {other}"),
            }
        }
        a
    }
}
