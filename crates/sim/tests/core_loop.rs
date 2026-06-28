//! Tests de la boucle cœur (Phase 2) : implantation, croissance, essaimage,
//! rejets audités, et déterminisme en présence de commandes joueur.

use proto::{Command, Event};
use sim::tile::TileKind;
use sim::World;

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

fn first_water(w: &World) -> (u32, u32) {
    for y in 0..w.height {
        for x in 0..w.width {
            if w.tile(x, y).kind == TileKind::Ocean {
                return (x, y);
            }
        }
    }
    panic!("aucune eau");
}

/// Un voisin terrestre (4-connexité, X enroulé) d'une case, s'il existe.
fn adjacent_land(w: &World, x: u32, y: u32) -> Option<(u32, u32)> {
    let cands = [
        ((x + 1) % w.width, y),
        ((x + w.width - 1) % w.width, y),
        (x, y + 1),
        (x, y.wrapping_sub(1)),
    ];
    for (nx, ny) in cands {
        if (nx, ny) != (x, y) && ny < w.height && w.tile(nx, ny).kind == TileKind::Land {
            return Some((nx, ny));
        }
    }
    None
}

/// Une case de terre qui a un voisin terrestre.
fn land_pair(w: &World) -> ((u32, u32), (u32, u32)) {
    for y in 0..w.height {
        for x in 0..w.width {
            if w.tile(x, y).kind == TileKind::Land {
                if let Some(n) = adjacent_land(w, x, y) {
                    return ((x, y), n);
                }
            }
        }
    }
    panic!("aucune paire de terres adjacentes");
}

#[test]
fn settle_then_population_grows() {
    let mut w = World::new(10, 120, 80);
    let (x, y) = first_land(&w);
    let ev = w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 200,
    });
    assert!(matches!(ev[0], Event::Settled { .. }));
    let p0 = w.tile(x, y).population;
    for _ in 0..24 {
        w.apply(Command::Step);
    }
    let p1 = w.tile(x, y).population;
    assert!(p1 > p0, "la population doit croître: {p0} -> {p1}");
    assert!(w.nation(0).is_some());
}

#[test]
fn settle_on_water_is_rejected() {
    let mut w = World::new(10, 120, 80);
    let (x, y) = first_water(&w);
    let ev = w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 100,
    });
    assert!(matches!(ev[0], Event::CommandRejected { .. }));
}

#[test]
fn swarm_requires_threshold_then_succeeds() {
    let mut w = World::new(10, 120, 80);
    let ((x, y), near) = land_pair(&w);

    // Sous 1000 : rejet.
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 200,
    });
    let ev = w.apply(Command::Swarm {
        from_x: x,
        from_y: y,
        to_x: near.0,
        to_y: near.1,
    });
    assert!(
        matches!(ev[0], Event::CommandRejected { .. }),
        "doit refuser sous 1000"
    );

    // Au-dessus de 1000 et à portée d'une case adjacente : succès.
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 2000,
    });
    let ev = w.apply(Command::Swarm {
        from_x: x,
        from_y: y,
        to_x: near.0,
        to_y: near.1,
    });
    assert!(matches!(ev[0], Event::Swarmed { .. }), "doit essaimer");
    assert_eq!(w.tile(near.0, near.1).owner, Some(0));
}

#[test]
fn determinism_with_commands() {
    let run = || {
        let mut w = World::new(5, 100, 70);
        let (x, y) = first_land(&w);
        let mut log = vec![w.genesis_event()];
        log.extend(w.apply(Command::Settle {
            x,
            y,
            nation: 0,
            population: 500,
        }));
        for _ in 0..15 {
            log.extend(w.apply(Command::Step));
        }
        log
    };
    assert_eq!(run(), run(), "même scénario => même journal");
}
