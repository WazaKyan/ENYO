//! Tests d'audit : le snapshot (serde) capture fidèlement l'état et constitue un
//! point de reprise valide pour le replay. On vérifie aussi que le monde évolue
//! et que les stats restent dans leurs bornes (pas de NaN/Inf, 0..1 respecté).

use std::collections::HashSet;

use proto::Command;
use sim::tile::Biome;
use sim::World;

#[test]
fn snapshot_roundtrip_preserves_state() {
    let mut w = World::new(99, 80, 60);
    for _ in 0..5 {
        w.apply(Command::Step);
    }
    let json = serde_json::to_string(&w).unwrap();
    let w2: World = serde_json::from_str(&json).unwrap();
    assert_eq!(
        w.checksum(),
        w2.checksum(),
        "le snapshot doit préserver l'état"
    );
    assert_eq!(w.width, w2.width);
    assert_eq!(w.tiles.len(), w2.tiles.len());
}

#[test]
fn snapshot_is_valid_resume_point() {
    let mut a = World::new(7, 80, 60);
    for _ in 0..5 {
        a.apply(Command::Step);
    }
    // Reprise depuis un snapshot (le RNG est inclus dans l'état sérialisé).
    let json = serde_json::to_string(&a).unwrap();
    let mut b: World = serde_json::from_str(&json).unwrap();
    for _ in 0..5 {
        a.apply(Command::Step);
        b.apply(Command::Step);
    }
    assert_eq!(
        a.checksum(),
        b.checksum(),
        "reprise depuis snapshot = même futur"
    );
}

#[test]
fn world_evolves_and_stays_finite() {
    let mut w = World::new(2024, 100, 70);
    let genesis = w.checksum();
    for _ in 0..12 {
        w.apply(Command::Step);
    }
    assert_ne!(genesis, w.checksum(), "le monde doit évoluer dans le temps");
    for t in &w.tiles {
        assert!(t.temperature.is_finite(), "température non finie");
        assert!(t.vegetation.is_finite(), "végétation non finie");
        assert!(t.precip_now.is_finite(), "précip non finie");
        assert!(
            (0.0..=1.0).contains(&t.vegetation),
            "végétation hors bornes: {}",
            t.vegetation
        );
        assert!(
            (0.0..=1.0).contains(&t.precip_now),
            "précip hors bornes: {}",
            t.precip_now
        );
    }
}

#[test]
fn world_has_diverse_biomes() {
    let w = World::new(123, 300, 200);
    let biomes: HashSet<Biome> = w.tiles.iter().map(|t| t.biome).collect();
    assert!(
        biomes.len() >= 4,
        "biomes trop peu variés: {}",
        biomes.len()
    );
}
