//! Tests de la prise de territoire (S5/S6) : occupation collante (hachures →
//! score de guerre), capitulation à > 75 % de la valeur ennemie (annexion des
//! cases occupées + paix imposée), et valeur des cases (vide/bâtiment/ville).

use proto::{Building, Command, Event, UnitKind};
use sim::tile::TileKind;
use sim::unit::Unit;
use sim::World;

fn idx(w: &World, x: u32, y: u32) -> usize {
    (y * w.width + x) as usize
}

/// Plaine neutre (sans relief/météo) — coût de déplacement standard.
fn plain(w: &mut World, x: u32, y: u32) {
    let i = idx(w, x, y);
    let t = &mut w.tiles[i];
    t.kind = TileKind::Land;
    t.ruggedness = 0.0;
    t.precip_now = 0.0;
    t.temperature = 15.0;
}

/// Donne `count` cases de plaine (à partir de x0, ligne y) à `nation`.
fn give_row(w: &mut World, x0: u32, count: u32, y: u32, nation: u16) {
    for x in x0..x0 + count {
        plain(w, x, y);
        let i = idx(w, x, y);
        w.tiles[i].owner = Some(nation);
    }
}

#[test]
fn tile_value_via_nation_value() {
    let mut w = World::new(1, 40, 30);
    give_row(&mut w, 5, 3, 5, 1); // 3 cases vides -> 3
    assert_eq!(w.nation_value(1), 3);
    let i = idx(&w, 5, 5);
    w.tiles[i].building = Some(Building::Industry); // +5 -1 = +4
    assert_eq!(w.nation_value(1), 3 - 1 + 5);
    let j = idx(&w, 6, 5);
    w.tiles[j].building = Some(Building::City); // +10 -1 = +9
    assert_eq!(w.nation_value(1), 7 + 9);
}

#[test]
fn occupation_sets_score_without_capitulation() {
    let mut w = World::new(2, 40, 30);
    // Ennemi (nation 1) : 5 cases vides (valeur 5) -> seuil capitulation > 3.75.
    give_row(&mut w, 5, 5, 5, 1);
    // Unité du joueur (nation 0) sur une plaine adjacente.
    plain(&mut w, 4, 5);
    w.units.push(Unit {
        id: 1,
        owner: 0,
        kind: UnitKind::Infantry,
        x: 4,
        y: 5,
        hp: 100,
        moves_left: 99,
    });
    w.apply(Command::DeclareWar {
        nation: 0,
        target: 1,
    });
    // Occupe UNE case ennemie (valeur 1).
    let ev = w.apply(Command::MoveUnit {
        unit: 1,
        to_x: 5,
        to_y: 5,
    });
    assert!(matches!(ev[0], Event::UnitMoved { .. }), "déplacement: {:?}", ev[0]);
    assert_eq!(w.tile(5, 5).occupier, Some(0), "case hachurée (occupée)");
    assert_eq!(w.tile(5, 5).owner, Some(1), "mais toujours à l'ennemi");
    assert_eq!(w.war_score(0, 1), 1);
    // Un tour : pas assez (1 < 3.75) -> pas de capitulation, case non annexée.
    let ev = w.apply(Command::Step);
    assert!(!ev.iter().any(|e| matches!(e, Event::Capitulation { .. })));
    assert_eq!(w.tile(5, 5).owner, Some(1), "non annexée");
    assert!(w.diplomacy.at_war(0, 1), "toujours en guerre");
}

#[test]
fn occupying_over_75_percent_forces_capitulation() {
    let mut w = World::new(3, 40, 30);
    // Ennemi : 1 seule case (valeur 1) -> occuper cette case (1 > 0.75) capitule.
    plain(&mut w, 7, 5);
    let e = idx(&w, 7, 5);
    w.tiles[e].owner = Some(1);
    // Chemin de plaine pour atteindre (7,5).
    for x in 4..8 {
        plain(&mut w, x, 5);
    }
    w.units.push(Unit {
        id: 1,
        owner: 0,
        kind: UnitKind::Cavalry,
        x: 4,
        y: 5,
        hp: 120,
        moves_left: 99,
    });
    w.apply(Command::DeclareWar {
        nation: 0,
        target: 1,
    });
    w.apply(Command::MoveUnit {
        unit: 1,
        to_x: 7,
        to_y: 5,
    });
    assert_eq!(w.war_score(0, 1), 1);
    let ev = w.apply(Command::Step);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::Capitulation { winner: 0, loser: 1, .. })),
        "capitulation attendue: {ev:?}"
    );
    assert_eq!(w.tile(7, 5).owner, Some(0), "case annexée par le vainqueur");
    assert_eq!(w.tile(7, 5).occupier, None, "occupation effacée");
    assert!(!w.diplomacy.at_war(0, 1), "paix imposée");
}

#[test]
fn peace_clears_occupation_without_annexation() {
    let mut w = World::new(4, 40, 30);
    give_row(&mut w, 5, 5, 5, 1);
    plain(&mut w, 4, 5);
    w.units.push(Unit {
        id: 1,
        owner: 0,
        kind: UnitKind::Infantry,
        x: 4,
        y: 5,
        hp: 100,
        moves_left: 99,
    });
    w.apply(Command::DeclareWar {
        nation: 0,
        target: 1,
    });
    w.apply(Command::MoveUnit {
        unit: 1,
        to_x: 5,
        to_y: 5,
    });
    assert_eq!(w.tile(5, 5).occupier, Some(0));
    w.apply(Command::MakePeace {
        nation: 0,
        target: 1,
    });
    assert_eq!(w.tile(5, 5).occupier, None, "la paix efface l'occupation");
    assert_eq!(w.tile(5, 5).owner, Some(1), "rien n'est annexé sans victoire");
}

#[test]
fn war_is_deterministic() {
    let run = || {
        let mut w = World::new(5, 50, 40);
        plain(&mut w, 9, 7);
        let e = idx(&w, 9, 7);
        w.tiles[e].owner = Some(1);
        for x in 5..10 {
            plain(&mut w, x, 7);
        }
        w.units.push(Unit {
            id: 1,
            owner: 0,
            kind: UnitKind::Cavalry,
            x: 5,
            y: 7,
            hp: 120,
            moves_left: 99,
        });
        w.apply(Command::DeclareWar {
            nation: 0,
            target: 1,
        });
        w.apply(Command::MoveUnit {
            unit: 1,
            to_x: 9,
            to_y: 7,
        });
        for _ in 0..3 {
            w.apply(Command::Step);
        }
        w.checksum()
    };
    assert_eq!(run(), run(), "la guerre (occupation/capitulation) doit être déterministe");
}
