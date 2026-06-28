//! Tests de l'IA baseline : elle propose des essaimages pour une nation établie,
//! place le bon nombre de nations, et un bac à sable multi-nations est déterministe.

use ai::{plan, spawn_nations};
use proto::Command;
use sim::tile::TileKind;
use sim::World;

fn productive(w: &World) -> (u32, u32) {
    for y in 0..w.height {
        for x in 0..w.width {
            if w.tile(x, y).kind == TileKind::Land && w.capacity_at(x, y) > 400.0 {
                return (x, y);
            }
        }
    }
    panic!("aucune terre productive");
}

#[test]
fn plan_expands_grown_nation() {
    let mut w = World::new(20, 100, 70);
    let (x, y) = productive(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 2000,
    });
    let cmds = plan(&w, 0);
    assert!(
        cmds.iter().any(|c| matches!(c, Command::Swarm { .. })),
        "l'IA doit proposer un essaimage pour une case >=1000"
    );
}

#[test]
fn spawn_places_requested_nations() {
    let w = World::new(7, 200, 140);
    let settles = spawn_nations(&w, 4)
        .iter()
        .filter(|c| matches!(c, Command::Settle { .. }))
        .count();
    assert_eq!(settles, 4);
}

#[test]
fn multi_nation_sandbox_is_deterministic() {
    let run = || {
        let mut w = World::new(9, 120, 90);
        let mut log = vec![w.genesis_event()];
        for cmd in spawn_nations(&w, 3) {
            log.extend(w.apply(cmd));
        }
        for _ in 0..25 {
            log.extend(w.apply(Command::Step));
            for nid in 0..3u16 {
                for cmd in plan(&w, nid) {
                    log.extend(w.apply(cmd));
                }
            }
        }
        (w.checksum(), log.len())
    };
    assert_eq!(
        run(),
        run(),
        "le bac à sable multi-nations doit être déterministe"
    );
}
