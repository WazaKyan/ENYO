//! Tests de l'infrastructure d'audit : un enregistrement de commandes se relit à
//! l'identique et, rejoué, **reproduit exactement** l'état (checksum). Les
//! snapshots font un aller-retour fidèle.

use persist::{load_snapshot, read_recording, replay, save_snapshot, Header, Recorder};
use proto::Command;
use sim::tile::TileKind;
use sim::World;

fn tmp(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("enyo_persist_test_{name}"));
    p
}

fn first_land(w: &World) -> (u32, u32) {
    for y in 0..w.height {
        for x in 0..w.width {
            if w.tile(x, y).kind == TileKind::Land {
                return (x, y);
            }
        }
    }
    panic!("aucune terre");
}

#[test]
fn recording_roundtrips_and_replay_reproduces_state() {
    let header = Header {
        seed: 77,
        width: 100,
        height: 70,
    };

    // Monde de référence + commandes.
    let mut reference = World::new(header.seed, header.width, header.height);
    let (lx, ly) = first_land(&reference);
    let commands = vec![
        Command::Settle {
            x: lx,
            y: ly,
            nation: 0,
            population: 500,
        },
        Command::Step,
        Command::Step,
        Command::Step,
        Command::Step,
        Command::Step,
    ];

    let path = tmp("rec.jsonl");
    {
        let mut rec = Recorder::create(&path, &header).unwrap();
        for c in &commands {
            rec.record(c).unwrap();
            reference.apply(c.clone());
        }
    }
    let reference_checksum = reference.checksum();

    // Relecture + replay.
    let (h2, cmds2) = read_recording(&path).unwrap();
    assert_eq!(h2, header, "l'en-tête doit être préservé");
    assert_eq!(cmds2, commands, "les commandes doivent être préservées");

    let (replayed, _events) = replay(&h2, &cmds2);
    assert_eq!(
        replayed.checksum(),
        reference_checksum,
        "le replay doit reproduire l'état EXACT"
    );

    std::fs::remove_file(&path).ok();
}

#[test]
fn snapshot_roundtrips_on_disk() {
    let mut world = World::new(5, 90, 60);
    for _ in 0..3 {
        world.apply(Command::Step);
    }
    let path = tmp("snap.json");
    save_snapshot(&world, &path).unwrap();
    let loaded = load_snapshot(&path).unwrap();
    assert_eq!(loaded.checksum(), world.checksum());
    std::fs::remove_file(&path).ok();
}
