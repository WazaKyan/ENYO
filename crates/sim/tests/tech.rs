//! Tests du système technologie (S3) : la recherche dépense le savoir, monte un
//! palier, et ses effets sont réels (Terroir augmente la capacité). Le savoir
//! s'accumule au fil du temps.

use proto::{Command, Event};
use sim::nation::{ESSOR, TERROIR};
use sim::tile::TileKind;
use sim::World;

fn productive_land(w: &World) -> (u32, u32) {
    for y in 0..w.height {
        for x in 0..w.width {
            if w.tile(x, y).kind == TileKind::Land && w.capacity_at(x, y) > 200.0 {
                return (x, y);
            }
        }
    }
    panic!("aucune terre productive");
}

#[test]
fn research_raises_tier_and_capacity() {
    let mut w = World::new(10, 80, 60);
    let (x, y) = productive_land(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 1000,
    });

    // Savoir donné directement pour tester la COMMANDE (pas l'accumulation).
    let ni = w.nations.iter().position(|n| n.id == 0).unwrap();
    w.nations[ni].knowledge = 1000.0;

    let cap_before = w.capacity_at(x, y);
    let ev = w.apply(Command::Research {
        nation: 0,
        branch: TERROIR as u8,
    });
    assert!(matches!(ev[0], Event::Researched { .. }));
    assert_eq!(w.nation(0).unwrap().tech[TERROIR], 1);
    assert!(
        w.capacity_at(x, y) > cap_before,
        "Terroir doit augmenter la capacité ({} -> {})",
        cap_before,
        w.capacity_at(x, y)
    );
}

#[test]
fn research_without_knowledge_is_rejected() {
    let mut w = World::new(10, 80, 60);
    let (x, y) = productive_land(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 100,
    });
    let ev = w.apply(Command::Research {
        nation: 0,
        branch: ESSOR as u8,
    });
    assert!(matches!(ev[0], Event::CommandRejected { .. }));
}

#[test]
fn knowledge_accrues_over_time() {
    let mut w = World::new(11, 80, 60);
    let (x, y) = productive_land(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 2000,
    });
    for _ in 0..60 {
        w.apply(Command::Step);
    }
    assert!(
        w.nation(0).unwrap().knowledge > 0.0,
        "le savoir doit s'accumuler avec une population développée"
    );
}
