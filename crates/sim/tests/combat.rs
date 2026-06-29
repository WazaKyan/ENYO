//! Tests du conflit (S6 diplomatie) : friction de frontière déterministe et grief
//! né d'une expansion contestée. (Le combat passe désormais par les unités, S5 —
//! voir `tests/units.rs` et `tests/war.rs`.)

use proto::{Command, Event};
use sim::tile::TileKind;
use sim::World;

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

/// Deux cases de terre adjacentes.
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
fn border_friction_is_deterministic() {
    // Deux nations voisines : la friction de frontière génère du grief chaque
    // tour. Le résultat doit être identique d'une exécution à l'autre.
    let run = || {
        let mut w = World::new(15, 100, 70);
        let ((ax, ay), (bx, by)) = land_pair(&w);
        w.apply(Command::Settle {
            x: ax,
            y: ay,
            nation: 0,
            population: 500,
        });
        w.apply(Command::Settle {
            x: bx,
            y: by,
            nation: 1,
            population: 500,
        });
        for _ in 0..10 {
            w.apply(Command::Step);
        }
        (w.checksum(), w.diplomacy.grievance(0, 1))
    };
    let (c1, g1) = run();
    let (c2, g2) = run();
    assert_eq!(c1, c2, "la friction de frontière doit être déterministe");
    assert!(g1 > 0.0, "du grief doit être né de la frontière");
    assert_eq!(g1, g2);
}

#[test]
fn contested_swarm_raises_grievance() {
    let mut w = World::new(10, 100, 70);
    let ((ax, ay), (bx, by)) = land_pair(&w);
    w.apply(Command::Settle {
        x: ax,
        y: ay,
        nation: 0,
        population: 2000,
    });
    w.apply(Command::Settle {
        x: bx,
        y: by,
        nation: 1,
        population: 100,
    });
    let ev = w.apply(Command::Swarm {
        from_x: ax,
        from_y: ay,
        to_x: bx,
        to_y: by,
    });
    assert!(matches!(ev[0], Event::GrievanceRaised { .. }));
    assert!(w.diplomacy.grievance(0, 1) > 0.0);
}
