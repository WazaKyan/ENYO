//! Harness CLI : pilote la simulation sans interface graphique.
//!
//! Usage : `harness [--seed N] [--turns N] [--width N] [--height N]
//!                  [--log chemin.jsonl] [--snapshot chemin.json] [--inspect x,y]`
//!
//! Outil principal d'exécution, de traçage et d'audit. Écrit un journal JSONL
//! (un événement par ligne) ; chaque événement porte un checksum du monde.

use std::io::Write;

use proto::Command;
use sim::World;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    tracing::info!(
        seed = args.seed,
        width = args.width,
        height = args.height,
        turns = args.turns,
        "génération du monde"
    );

    let mut world = World::new(args.seed, args.width, args.height);

    // Prépare le journal d'événements (audit).
    if let Some(parent) = std::path::Path::new(&args.log).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).expect("création du dossier de log");
        }
    }
    let mut log_file = std::fs::File::create(&args.log).expect("création du fichier de log");
    let write_event = |file: &mut std::fs::File, ev: &proto::Event| {
        let line = serde_json::to_string(ev).expect("sérialisation de l'événement");
        writeln!(file, "{line}").expect("écriture du log");
    };

    // Genèse.
    let genesis = world.genesis_event();
    write_event(&mut log_file, &genesis);
    tracing::info!(?genesis, "monde généré");

    // Tours.
    for _ in 0..args.turns {
        for ev in world.apply(Command::Step) {
            write_event(&mut log_file, &ev);
        }
    }

    // Snapshot complet (audit profond), à la demande.
    if let Some(path) = &args.snapshot {
        let json = serde_json::to_string(&world).expect("sérialisation du monde");
        std::fs::write(path, json).expect("écriture du snapshot");
        tracing::info!(snapshot = %path, "snapshot écrit");
    }

    // Inspection d'une case, à la demande.
    if let Some((x, y)) = args.inspect {
        if x < world.width && y < world.height {
            println!("Case ({x},{y}) = {:#?}", world.tile(x, y));
        } else {
            eprintln!("inspect hors limites ({x},{y})");
        }
    }

    tracing::info!(turn = world.turn, log = %args.log, "simulation terminée");
    println!(
        "OK — monde {}x{} (terre {} / océan {}), {} tours simulés, journal: {}",
        world.width, world.height, world.land_tiles, world.ocean_tiles, world.turn, args.log
    );
}

/// Arguments de la ligne de commande.
struct Args {
    seed: u64,
    turns: usize,
    width: u32,
    height: u32,
    log: String,
    snapshot: Option<String>,
    inspect: Option<(u32, u32)>,
}

impl Args {
    fn parse() -> Self {
        let mut seed = 1u64;
        let mut turns = 12usize;
        let mut width = 800u32;
        let mut height = 500u32;
        let mut log = String::from("logs/run.jsonl");
        let mut snapshot = None;
        let mut inspect = None;

        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--seed" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        seed = v;
                    }
                }
                "--turns" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        turns = v;
                    }
                }
                "--width" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        width = v;
                    }
                }
                "--height" => {
                    if let Some(v) = it.next().and_then(|v| v.parse().ok()) {
                        height = v;
                    }
                }
                "--log" => {
                    if let Some(v) = it.next() {
                        log = v;
                    }
                }
                "--snapshot" => snapshot = it.next(),
                "--inspect" => inspect = it.next().and_then(parse_xy),
                other => eprintln!("argument ignoré : {other}"),
            }
        }

        Args {
            seed,
            turns,
            width,
            height,
            log,
            snapshot,
            inspect,
        }
    }
}

/// Parse une coordonnée "x,y".
fn parse_xy(s: String) -> Option<(u32, u32)> {
    let (a, b) = s.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}
