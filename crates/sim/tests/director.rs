//! Tests des leviers du Directeur (S7, côté mécanique) : calamité (famine),
//! aubaine (salut), et nudge d'opinion.

use proto::{Command, Event};
use sim::tile::TileKind;
use sim::World;

fn productive(w: &World) -> (u32, u32) {
    for y in 0..w.height {
        for x in 0..w.width {
            if w.tile(x, y).kind == TileKind::Land && w.capacity_at(x, y) > 300.0 {
                return (x, y);
            }
        }
    }
    panic!("aucune terre productive");
}

#[test]
fn blight_reduces_capacity() {
    let mut w = World::new(10, 80, 60);
    let (x, y) = productive(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 1000,
    });
    let before = w.capacity_at(x, y);
    let ev = w.apply(Command::DirectorBlight { x, y, amount: 60 });
    assert!(matches!(ev[0], Event::Blighted { .. }));
    assert!(
        w.capacity_at(x, y) < before,
        "la calamité doit réduire la capacité ({} -> {})",
        before,
        w.capacity_at(x, y)
    );
    assert!(w.tile(x, y).devastation > 0.0);
}

#[test]
fn windfall_heals_devastation() {
    let mut w = World::new(10, 80, 60);
    let (x, y) = productive(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 1000,
    });
    w.apply(Command::DirectorBlight { x, y, amount: 80 });
    let dev = w.tile(x, y).devastation;
    assert!(dev > 0.0);
    let ev = w.apply(Command::DirectorWindfall { x, y, amount: 80 });
    assert!(matches!(ev[0], Event::Windfall { .. }));
    assert!(w.tile(x, y).devastation < dev, "l'aubaine doit soigner");
}

#[test]
fn director_grievance_nudges_opinion() {
    let mut w = World::new(10, 80, 60);
    let ev = w.apply(Command::DirectorGrievance {
        from: 1,
        to: 0,
        amount: 10,
    });
    assert!(matches!(ev[0], Event::OpinionNudged { .. }));
    assert!(w.diplomacy.grievance(1, 0) > 0.0);
}
