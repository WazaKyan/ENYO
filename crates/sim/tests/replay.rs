//! Test de déterminisme du déroulé : même graine + mêmes commandes ⇒ même journal
//! (checksums inclus). C'est le filet de sécurité qui rend tout testable sans UI.

use proto::{Command, Event};
use sim::World;

/// Génère un monde et joue `turns` tours ; renvoie le journal (genèse + tours).
fn run(seed: u64, turns: usize) -> Vec<Event> {
    let mut world = World::new(seed, 120, 80);
    let mut log = vec![world.genesis_event()];
    for _ in 0..turns {
        log.extend(world.apply(Command::Step));
    }
    log
}

#[test]
fn replay_is_deterministic() {
    let a = run(12_345, 24);
    let b = run(12_345, 24);
    assert_eq!(a, b, "même graine + mêmes commandes => même journal");
    assert_eq!(a.len(), 25, "1 genèse + 24 tours");
}

#[test]
fn different_seeds_diverge() {
    let a = run(1, 12);
    let b = run(2, 12);
    assert_ne!(a, b);
}
