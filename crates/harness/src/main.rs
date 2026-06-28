//! Harness CLI : pilote la simulation sans interface graphique.
//!
//! Usage : `harness [--seed N] [--turns N] [--log chemin.jsonl]`
//!
//! C'est l'outil principal pour exécuter, tracer et tester le jeu. Il écrit un
//! journal d'événements JSONL (un événement par ligne), auditable et rejouable.

use std::io::Write;

use proto::Command;
use sim::World;
use tracing_subscriber::EnvFilter;

fn main() {
    // Logs humains ; niveau réglable via la variable d'env RUST_LOG (défaut "info").
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let args = Args::parse();
    tracing::info!(seed = args.seed, turns = args.turns, "démarrage de la simulation");

    let mut world = World::new(args.seed);

    // Crée le dossier du log si besoin.
    if let Some(parent) = std::path::Path::new(&args.log).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).expect("création du dossier de log");
        }
    }
    let mut log_file = std::fs::File::create(&args.log).expect("création du fichier de log");

    for _ in 0..args.turns {
        for event in world.apply(Command::Step) {
            let line = serde_json::to_string(&event).expect("sérialisation de l'événement");
            writeln!(log_file, "{line}").expect("écriture du log");
        }
    }

    tracing::info!(turn = world.turn, log = %args.log, "simulation terminée");
    println!(
        "OK — {} tours simulés, journal écrit dans {}",
        world.turn, args.log
    );
}

/// Arguments de la ligne de commande.
struct Args {
    seed: u64,
    turns: usize,
    log: String,
}

impl Args {
    /// Parse les arguments ; valeurs par défaut si absents.
    fn parse() -> Self {
        let mut seed = 1u64;
        let mut turns = 100usize;
        let mut log = String::from("logs/run.jsonl");

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
                "--log" => {
                    if let Some(v) = it.next() {
                        log = v;
                    }
                }
                other => eprintln!("argument ignoré : {other}"),
            }
        }
        Args { seed, turns, log }
    }
}
