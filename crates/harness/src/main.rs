//! Harness CLI : pilote la simulation sans interface graphique. Outil principal
//! d'exécution, de traçage et d'**audit**.
//!
//! Modes :
//! - normal : génère un monde, joue des tours, écrit le journal d'événements ET
//!   un **enregistrement rejouable** des commandes.
//! - `--nations N` : bac à sable — N nations IA s'implantent et s'étendent.
//! - `--settle x,y [--auto-expand]` : démo une nation (joueur 0).
//! - `--replay f.rec.jsonl` : rejoue un enregistrement et vérifie le déterminisme.
//! - `--load f.json` : reprend depuis un snapshot puis joue `--turns` tours.
//! - `--repl` : console interactive pour piloter la sim à la main.
//!
//! Options : `--seed N --turns N --width N --height N --log f --rec f
//!            --snapshot f --inspect x,y`

use std::fs::File;
use std::io::{self, BufRead, Write};

use persist::{Header, Recorder};
use proto::{Command, Event};
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

    // Acteurs IA : --nations N (bac à sable) OU une nation 0 si --settle + --auto-expand.
    let actors: Vec<u16> = if args.nations > 0 {
        for cmd in ai::spawn_nations(&world, args.nations) {
            run_command(&mut world, &mut rec, &mut log, cmd);
        }
        (0..args.nations).collect()
    } else if let Some((x, y)) = args.settle {
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
        if args.auto_expand {
            vec![0]
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    for _ in 0..args.turns {
        run_command(&mut world, &mut rec, &mut log, Command::Step);
        for &nid in &actors {
            for cmd in ai::plan(&world, nid) {
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
    print_summary(&world, &actors, args.settle.is_some());
}

/// Résumé final par nation.
fn print_summary(world: &World, actors: &[u16], settled: bool) {
    if actors.is_empty() && !settled {
        return;
    }
    let provinces = world.provinces();
    let ids: Vec<u16> = if actors.is_empty() {
        vec![0]
    } else {
        actors.to_vec()
    };
    println!("{} nation(s) :", ids.len());
    for nid in ids {
        let (pop, tiles) = world.nation_stats(nid);
        let provs = provinces.iter().filter(|p| p.owner == nid).count();
        let tech = world.nation(nid).map(|n| n.tech).unwrap_or_default();
        println!("  nation {nid} : {pop:.0} hab, {tiles} cases, {provs} prov., tech {tech:?}");
    }
    let wars = world.diplomacy.wars();
    if !wars.is_empty() {
        println!("  guerres : {wars:?}");
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
}

/// Console interactive pour piloter la simulation à la main.
fn run_repl(world: &mut World) {
    println!(
        "REPL ENYO. Commandes : step [n] | settle x y [pop] | swarm fx fy tx ty | \
         research nat br | mobilize x y amt | march fx fy tx ty | war a b | peace a b | \
         inspect x y | capacity x y | nation id | checksum | quit"
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
            ["mobilize", x, y, amt] => match (u(x), u(y), u(amt)) {
                (Some(x), Some(y), Some(a)) => emit(world.apply(Command::Mobilize {
                    x,
                    y,
                    nation: 0,
                    amount: a,
                })),
                _ => println!("arguments invalides"),
            },
            ["march", a, b, c, d] => match (u(a), u(b), u(c), u(d)) {
                (Some(fx), Some(fy), Some(tx), Some(ty)) => emit(world.apply(Command::March {
                    from_x: fx,
                    from_y: fy,
                    to_x: tx,
                    to_y: ty,
                })),
                _ => println!("coordonnées invalides"),
            },
            ["war", a, b] => match (u(a), u(b)) {
                (Some(a), Some(b)) => emit(world.apply(Command::DeclareWar {
                    nation: a as u16,
                    target: b as u16,
                })),
                _ => println!("arguments invalides"),
            },
            ["peace", a, b] => match (u(a), u(b)) {
                (Some(a), Some(b)) => emit(world.apply(Command::MakePeace {
                    nation: a as u16,
                    target: b as u16,
                })),
                _ => println!("arguments invalides"),
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
    nations: u16,
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
            nations: 0,
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
                "--nations" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        a.nations = v;
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

/// Parse une coordonnée "x,y".
fn parse_xy(s: String) -> Option<(u32, u32)> {
    let (a, b) = s.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}
