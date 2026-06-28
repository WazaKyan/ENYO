//! Harness CLI : pilote la simulation sans interface graphique. Outil principal
//! d'exécution, de traçage et d'**audit**.
//!
//! Modes :
//! - normal : génère un monde, joue des tours, écrit le journal d'événements ET
//!   un **enregistrement rejouable** des commandes.
//! - `--replay f.rec.jsonl` : rejoue un enregistrement et vérifie le déterminisme.
//! - `--load f.json` : reprend depuis un snapshot puis joue `--turns` tours.
//! - `--repl` : console interactive pour piloter la sim à la main.
//!
//! Options : `--seed N --turns N --width N --height N --settle x,y
//!            --log f.jsonl --rec f.rec.jsonl --snapshot f.json --inspect x,y`

use std::fs::File;
use std::io::{self, BufRead, Write};

use persist::{Header, Recorder};
use proto::{Command, Event};
use sim::tile::TileKind;
use sim::World;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    // Mode replay : rejoue + vérifie, puis sort.
    if let Some(path) = &args.replay {
        run_replay(path);
        return;
    }

    // Construit ou recharge le monde.
    let mut world = match &args.load {
        Some(snap) => persist::load_snapshot(snap).expect("chargement du snapshot"),
        None => World::new(args.seed, args.width, args.height),
    };
    tracing::info!(
        seed = world.seed,
        width = world.width,
        height = world.height,
        turn = world.turn,
        "monde prêt"
    );

    // Mode REPL : console interactive.
    if args.repl {
        run_repl(&mut world);
        return;
    }

    // Mode normal (batch) : journal d'événements + enregistrement des commandes.
    let mut log = create_file(&args.log);
    write_line(
        &mut log,
        &serde_json::to_string(&world.genesis_event()).unwrap(),
    );

    let header = Header {
        seed: world.seed,
        width: world.width,
        height: world.height,
    };
    let mut rec = Recorder::create(&args.rec, &header).expect("création de l'enregistrement");

    if let Some((x, y)) = args.settle {
        run_command(
            &mut world,
            &mut rec,
            &mut log,
            Command::Settle {
                x,
                y,
                nation: 0,
                population: 300,
            },
        );
    }
    for _ in 0..args.turns {
        run_command(&mut world, &mut rec, &mut log, Command::Step);
        if args.auto_expand {
            for cmd in auto_expansion(&world, 0) {
                run_command(&mut world, &mut rec, &mut log, cmd);
            }
        }
    }

    if let Some(path) = &args.snapshot {
        persist::save_snapshot(&world, path).expect("écriture du snapshot");
        tracing::info!(snapshot = %path, "snapshot écrit");
    }
    if let Some((x, y)) = args.inspect {
        if x < world.width && y < world.height {
            println!("Case ({x},{y}) = {:#?}", world.tile(x, y));
        } else {
            eprintln!("inspect hors limites ({x},{y})");
        }
    }

    println!(
        "OK — monde {}x{} (terre {} / océan {}), tour {}, journal {} + rejouable {}",
        world.width,
        world.height,
        world.land_tiles,
        world.ocean_tiles,
        world.turn,
        args.log,
        args.rec
    );
    if args.settle.is_some() {
        let (pop, tiles) = world.nation_stats(0);
        let provinces = world.provinces().iter().filter(|p| p.owner == 0).count();
        println!("Nation 0 : {pop:.0} habitants sur {tiles} case(s), {provinces} province(s)");
    }
}

/// Enregistre la commande, l'applique, et écrit les événements produits.
fn run_command(world: &mut World, rec: &mut Recorder, log: &mut File, cmd: Command) {
    rec.record(&cmd).expect("enregistrement de la commande");
    for ev in world.apply(cmd) {
        write_line(log, &serde_json::to_string(&ev).unwrap());
    }
}

/// Rejoue un enregistrement et vérifie que le replay est déterministe.
fn run_replay(path: &str) {
    let (header, commands) = persist::read_recording(path).expect("lecture de l'enregistrement");
    let (world, events) = persist::replay(&header, &commands);
    let (world2, _) = persist::replay(&header, &commands);
    let deterministic = world.checksum() == world2.checksum();

    println!(
        "Replay « {path} » : {} commandes, tour {}, checksum {}",
        commands.len(),
        world.turn,
        world.checksum()
    );
    println!("Déterministe (2 replays identiques) : {deterministic}");
    let last = events.iter().rev().find_map(|e| match e {
        Event::TurnResolved { checksum, .. } => Some(*checksum),
        _ => None,
    });
    if let Some(c) = last {
        println!("Checksum du dernier tour : {c}");
    }
    let (pop, tiles) = world.nation_stats(0);
    if tiles > 0 {
        println!("Nation 0 : {pop:.0} habitants sur {tiles} case(s)");
    }
}

/// Console interactive pour piloter la simulation à la main.
fn run_repl(world: &mut World) {
    println!(
        "REPL ENYO. Commandes : step [n] | settle x y [pop] | swarm fx fy tx ty | \
         research nation branch | inspect x y | capacity x y | nation id | checksum | quit"
    );
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        // Retire un éventuel BOM (ex. entrée pipée sous Windows).
        let line = line.trim_start_matches('\u{feff}');
        let p: Vec<&str> = line.split_whitespace().collect();
        match p.as_slice() {
            [] => {}
            ["quit"] | ["exit"] => break,
            ["checksum"] => println!("checksum = {}", world.checksum()),
            ["capacity", x, y] => match (u(x), u(y)) {
                (Some(x), Some(y)) if x < world.width && y < world.height => {
                    println!("capacité ({x},{y}) = {:.0}", world.capacity_at(x, y))
                }
                _ => println!("coordonnées hors limites"),
            },
            ["step"] => emit(world.apply(Command::Step)),
            ["step", n] => {
                let n: u32 = n.parse().unwrap_or(1);
                for _ in 0..n {
                    world.apply(Command::Step);
                }
                println!("-> tour {}", world.turn);
            }
            ["settle", x, y] => emit(apply_settle(world, x, y, "300")),
            ["settle", x, y, pop] => emit(apply_settle(world, x, y, pop)),
            ["swarm", a, b, c, d] => match (u(a), u(b), u(c), u(d)) {
                (Some(fx), Some(fy), Some(tx), Some(ty)) => emit(world.apply(Command::Swarm {
                    from_x: fx,
                    from_y: fy,
                    to_x: tx,
                    to_y: ty,
                })),
                _ => println!("coordonnées invalides"),
            },
            ["research", nat, br] => match (u(nat), u(br)) {
                (Some(nat), Some(br)) => emit(world.apply(Command::Research {
                    nation: nat as u16,
                    branch: br as u8,
                })),
                _ => println!("arguments invalides"),
            },
            ["inspect", x, y] => match (u(x), u(y)) {
                (Some(x), Some(y)) if x < world.width && y < world.height => {
                    println!("{:#?}", world.tile(x, y))
                }
                _ => println!("coordonnées hors limites"),
            },
            ["nation", id] => match u(id) {
                Some(id) => match world.nation(id as u16) {
                    Some(n) => println!("{n:#?}"),
                    None => println!("nation {id} inexistante"),
                },
                None => println!("id invalide"),
            },
            _ => println!("commande inconnue : {line}"),
        }
    }
}

fn apply_settle(world: &mut World, x: &str, y: &str, pop: &str) -> Vec<Event> {
    match (u(x), u(y), pop.parse::<u32>().ok()) {
        (Some(x), Some(y), Some(p)) => world.apply(Command::Settle {
            x,
            y,
            nation: 0,
            population: p,
        }),
        _ => vec![Event::CommandRejected {
            reason: "arguments invalides".into(),
        }],
    }
}

fn emit(events: Vec<Event>) {
    for e in events {
        println!("  {e:?}");
    }
}

fn u(s: &str) -> Option<u32> {
    s.parse().ok()
}

fn create_file(path: &str) -> File {
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).ok();
        }
    }
    File::create(path).expect("création du fichier")
}

fn write_line(f: &mut File, s: &str) {
    writeln!(f, "{s}").expect("écriture");
}

/// Arguments de la ligne de commande.
struct Args {
    seed: u64,
    turns: usize,
    width: u32,
    height: u32,
    log: String,
    rec: String,
    snapshot: Option<String>,
    inspect: Option<(u32, u32)>,
    settle: Option<(u32, u32)>,
    load: Option<String>,
    replay: Option<String>,
    repl: bool,
    auto_expand: bool,
}

impl Args {
    fn parse() -> Self {
        let mut a = Args {
            seed: 1,
            turns: 12,
            width: 800,
            height: 500,
            log: String::from("logs/run.jsonl"),
            rec: String::from("logs/run.rec.jsonl"),
            snapshot: None,
            inspect: None,
            settle: None,
            load: None,
            replay: None,
            repl: false,
            auto_expand: false,
        };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--seed" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        a.seed = v;
                    }
                }
                "--turns" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        a.turns = v;
                    }
                }
                "--width" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        a.width = v;
                    }
                }
                "--height" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        a.height = v;
                    }
                }
                "--log" => {
                    if let Some(v) = it.next() {
                        a.log = v;
                    }
                }
                "--rec" => {
                    if let Some(v) = it.next() {
                        a.rec = v;
                    }
                }
                "--snapshot" => a.snapshot = it.next(),
                "--inspect" => a.inspect = it.next().and_then(parse_xy),
                "--settle" => a.settle = it.next().and_then(parse_xy),
                "--load" => a.load = it.next(),
                "--replay" => a.replay = it.next(),
                "--repl" => a.repl = true,
                "--auto-expand" => a.auto_expand = true,
                other => eprintln!("argument ignoré : {other}"),
            }
        }
        a
    }
}

/// Driver de démo : pour chaque case de `nation` à >=1000 hab., essaime vers une
/// case de terre adjacente libre (déduplique les cibles ; une par source/tour).
fn auto_expansion(world: &World, nation: u16) -> Vec<Command> {
    use std::collections::HashSet;
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

/// Parse une coordonnée "x,y".
fn parse_xy(s: String) -> Option<(u32, u32)> {
    let (a, b) = s.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}
