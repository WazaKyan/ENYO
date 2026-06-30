//! Tests de l'arbre de recherche (S3) : la recherche dépense le savoir, débloque une
//! techno, et ses effets sont réels (Maçonnerie augmente la capacité). Les prérequis
//! verrouillent les techs avancées. Le savoir s'accumule au fil du temps.

use proto::{Command, Event};
use sim::tile::TileKind;
use sim::World;

// Ids de l'arbre (cf. `sim::tech::TREE`).
const AGRICULTURE: u16 = 0;
const MACONNERIE: u16 = 1;
const AQUEDUCS: u16 = 6; // prérequis : Maçonnerie

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
fn research_unlocks_tech_and_raises_capacity() {
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
        tech: MACONNERIE,
    });
    assert!(matches!(ev[0], Event::Researched { tech: MACONNERIE, .. }));
    assert!(sim::tech::is_researched(w.nation(0).unwrap().techs, MACONNERIE));
    assert!(
        w.capacity_at(x, y) > cap_before,
        "Maçonnerie doit augmenter la capacité ({} -> {})",
        cap_before,
        w.capacity_at(x, y)
    );
    // Re-rechercher la même tech est rejeté (déjà acquise).
    let ev = w.apply(Command::Research {
        nation: 0,
        tech: MACONNERIE,
    });
    assert!(matches!(ev[0], Event::CommandRejected { .. }));
}

#[test]
fn prereqs_gate_advanced_tech() {
    let mut w = World::new(12, 80, 60);
    let (x, y) = productive_land(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 100,
    });
    let ni = w.nations.iter().position(|n| n.id == 0).unwrap();
    w.nations[ni].knowledge = 1000.0;
    // Aqueducs exige Maçonnerie : rejeté tant qu'on ne l'a pas.
    let ev = w.apply(Command::Research {
        nation: 0,
        tech: AQUEDUCS,
    });
    assert!(
        matches!(ev[0], Event::CommandRejected { .. }),
        "Aqueducs verrouillé sans Maçonnerie: {:?}",
        ev[0]
    );
    // Avec Maçonnerie d'abord, puis Aqueducs : accepté.
    w.apply(Command::Research {
        nation: 0,
        tech: MACONNERIE,
    });
    let ev = w.apply(Command::Research {
        nation: 0,
        tech: AQUEDUCS,
    });
    assert!(matches!(ev[0], Event::Researched { tech: AQUEDUCS, .. }));
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
        tech: AGRICULTURE,
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
