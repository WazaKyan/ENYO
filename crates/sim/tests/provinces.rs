//! Tests des provinces émergentes (S4) : cases connexes = une province ; cases
//! éloignées = provinces distinctes.

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
fn connected_tiles_form_one_province() {
    let mut w = World::new(10, 120, 80);
    let ((x, y), near) = land_pair(&w);

    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 2000,
    });
    assert_eq!(w.provinces().len(), 1);
    assert_eq!(w.provinces()[0].tiles, 1);

    let ev = w.apply(Command::Swarm {
        from_x: x,
        from_y: y,
        to_x: near.0,
        to_y: near.1,
    });
    assert!(matches!(ev[0], Event::Swarmed { .. }));

    let provs = w.provinces();
    assert_eq!(provs.len(), 1, "deux cases adjacentes => une province");
    assert_eq!(provs[0].tiles, 2);
    assert_eq!(provs[0].owner, 0);
}

#[test]
fn distant_settlements_form_separate_provinces() {
    let mut w = World::new(10, 200, 120);
    let (x, y) = first_land(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 100,
    });

    // Cherche une terre nettement éloignée (non connexe).
    let mut far = None;
    'outer: for yy in (0..w.height).rev() {
        for xx in (0..w.width).rev() {
            if w.tile(xx, yy).kind == TileKind::Land {
                let d = (xx as i64 - x as i64).abs() + (yy as i64 - y as i64).abs();
                if d > 10 {
                    far = Some((xx, yy));
                    break 'outer;
                }
            }
        }
    }
    let (fx, fy) = far.expect("terre lointaine");
    w.apply(Command::Settle {
        x: fx,
        y: fy,
        nation: 0,
        population: 100,
    });

    assert_eq!(
        w.provinces().len(),
        2,
        "deux implantations éloignées => deux provinces"
    );
}
