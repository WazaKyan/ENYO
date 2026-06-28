//! Tests de la logique du Directeur (Indice de Drame + leviers) : il met la
//! pression sur un joueur dominant et secourt un joueur en difficulté.

use ai::direct;
use proto::Command;
use sim::tile::TileKind;
use sim::World;

fn two_lands(w: &World) -> ((u32, u32), (u32, u32)) {
    let mut found = Vec::new();
    'outer: for y in 0..w.height {
        for x in 0..w.width {
            if w.tile(x, y).kind == TileKind::Land && w.capacity_at(x, y) > 300.0 {
                found.push((x, y));
                if found.len() == 2 {
                    break 'outer;
                }
            }
        }
    }
    (found[0], found[1])
}

#[test]
fn director_pressures_a_dominant_player() {
    let mut w = World::new(20, 120, 80);
    let (a, b) = two_lands(&w);
    w.apply(Command::Settle {
        x: a.0,
        y: a.1,
        nation: 0,
        population: 5000,
    });
    w.apply(Command::Settle {
        x: b.0,
        y: b.1,
        nation: 1,
        population: 100,
    });
    let cmds = direct(&w, 0);
    assert!(
        cmds.iter()
            .any(|c| matches!(c, Command::DirectorGrievance { to: 0, .. })),
        "le Directeur doit attiser une coalition contre le joueur dominant"
    );
}

#[test]
fn director_relieves_a_struggling_player() {
    let mut w = World::new(20, 120, 80);
    let (a, b) = two_lands(&w);
    w.apply(Command::Settle {
        x: a.0,
        y: a.1,
        nation: 0,
        population: 100,
    });
    w.apply(Command::Settle {
        x: b.0,
        y: b.1,
        nation: 1,
        population: 5000,
    });
    let cmds = direct(&w, 0);
    assert!(
        cmds.iter()
            .any(|c| matches!(c, Command::DirectorWindfall { .. })),
        "le Directeur doit secourir le joueur en difficulté"
    );
}
