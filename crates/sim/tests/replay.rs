//! Test de déterminisme : rejouer la même graine + les mêmes commandes doit
//! produire exactement le même journal d'événements. C'est le filet de sécurité
//! qui rend tout le reste testable sans interface graphique.

use proto::{Command, Event};
use sim::World;

/// Joue `turns` tours sur un monde neuf de graine `seed` et renvoie le journal.
fn run(seed: u64, turns: usize) -> Vec<Event> {
    let mut world = World::new(seed);
    let mut log = Vec::new();
    for _ in 0..turns {
        log.extend(world.apply(Command::Step));
    }
    log
}

#[test]
fn replay_is_deterministic() {
    let a = run(12_345, 500);
    let b = run(12_345, 500);
    assert_eq!(a, b, "même graine + mêmes commandes => même journal");
    assert_eq!(a.len(), 500);
}

#[test]
fn different_seeds_diverge() {
    let a = run(1, 100);
    let b = run(2, 100);
    assert_ne!(a, b, "des graines différentes doivent diverger");
}
