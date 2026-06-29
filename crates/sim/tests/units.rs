//! Tests des unités militaires (S5) : recrutement (coût force+argent, caserne,
//! tech), mouvement (points selon terrain), et combat (bonus de défense du terrain,
//! malus d'attaque selon le type, destruction, déterminisme).

use proto::{Building, Command, Event, UnitKind};
use sim::nation::FER;
use sim::tile::TileKind;
use sim::unit::{unit_stats, Unit};
use sim::World;

fn idx(w: &World, x: u32, y: u32) -> usize {
    (y * w.width + x) as usize
}

/// Met une case en plaine « neutre » (sans relief, météo, ni végétation).
fn plain(w: &mut World, x: u32, y: u32) {
    let i = idx(w, x, y);
    let t = &mut w.tiles[i];
    t.kind = TileKind::Land;
    t.ruggedness = 0.0;
    t.vegetation = 0.0;
    t.precip_now = 0.0;
    t.temperature = 15.0;
    t.devastation = 0.0;
}

/// Nation possédant une caserne dotée en force et en argent, sur une plaine.
fn setup_barracks(w: &mut World, x: u32, y: u32, nation: u16) {
    plain(w, x, y);
    w.apply(Command::Settle {
        x,
        y,
        nation,
        population: 100,
    });
    let i = idx(w, x, y);
    w.tiles[i].building = Some(Building::Military);
    let ni = w.nations.iter().position(|n| n.id == nation).unwrap();
    w.nations[ni].money = 5000;
    w.nations[ni].manpower = 1000;
}

#[test]
fn create_unit_costs_money_and_force() {
    let mut w = World::new(1, 40, 30);
    setup_barracks(&mut w, 5, 5, 0);
    let money0 = w.nation(0).unwrap().money;
    let manpower0 = w.nation(0).unwrap().manpower;
    let ev = w.apply(Command::CreateUnit {
        x: 5,
        y: 5,
        nation: 0,
        kind: UnitKind::Infantry,
    });
    assert!(matches!(ev[0], Event::UnitCreated { .. }), "créée: {:?}", ev[0]);
    assert_eq!(w.units.len(), 1);
    let s = unit_stats(UnitKind::Infantry);
    assert_eq!(w.nation(0).unwrap().money, money0 - s.cost_money);
    assert_eq!(w.nation(0).unwrap().manpower, manpower0 - s.cost_force);
    assert_eq!(w.units[0].hp, s.max_hp);
}

#[test]
fn create_unit_requires_barracks() {
    let mut w = World::new(2, 40, 30);
    plain(&mut w, 5, 5);
    w.apply(Command::Settle {
        x: 5,
        y: 5,
        nation: 0,
        population: 100,
    });
    let ni = w.nations.iter().position(|n| n.id == 0).unwrap();
    w.nations[ni].money = 5000;
    // Pas de caserne -> rejet.
    let ev = w.apply(Command::CreateUnit {
        x: 5,
        y: 5,
        nation: 0,
        kind: UnitKind::Infantry,
    });
    assert!(matches!(ev[0], Event::CommandRejected { .. }));
}

#[test]
fn tech_gates_archer() {
    let mut w = World::new(3, 40, 30);
    setup_barracks(&mut w, 5, 5, 0);
    // Archer exige Fer >= 1.
    let ev = w.apply(Command::CreateUnit {
        x: 5,
        y: 5,
        nation: 0,
        kind: UnitKind::Archer,
    });
    assert!(
        matches!(ev[0], Event::CommandRejected { .. }),
        "archer sans tech -> rejet"
    );
    let ni = w.nations.iter().position(|n| n.id == 0).unwrap();
    w.nations[ni].tech[FER] = 1;
    let ev = w.apply(Command::CreateUnit {
        x: 5,
        y: 5,
        nation: 0,
        kind: UnitKind::Archer,
    });
    assert!(
        matches!(ev[0], Event::UnitCreated { .. }),
        "archer avec tech -> ok: {:?}",
        ev[0]
    );
}

#[test]
fn unit_moves_then_runs_out_of_points() {
    let mut w = World::new(4, 40, 30);
    for x in 5..16 {
        plain(&mut w, x, 5);
    }
    setup_barracks(&mut w, 5, 5, 0);
    w.apply(Command::CreateUnit {
        x: 5,
        y: 5,
        nation: 0,
        kind: UnitKind::Infantry,
    });
    let id = w.units[0].id;
    // Infanterie : 30 points, plaine = 10/case -> 3 cases (jusqu'à x=8).
    let ev = w.apply(Command::MoveUnit {
        unit: id,
        to_x: 8,
        to_y: 5,
    });
    assert!(matches!(ev[0], Event::UnitMoved { .. }), "déplacement: {:?}", ev[0]);
    assert_eq!((w.units[0].x, w.units[0].y), (8, 5));
    // Plus de points ce tour -> rejet.
    let ev = w.apply(Command::MoveUnit {
        unit: id,
        to_x: 9,
        to_y: 5,
    });
    assert!(matches!(ev[0], Event::CommandRejected { .. }), "épuisé -> rejet");
    // Après un mois, les points se rechargent -> on peut repartir.
    w.apply(Command::Step);
    let ev = w.apply(Command::MoveUnit {
        unit: id,
        to_x: 9,
        to_y: 5,
    });
    assert!(matches!(ev[0], Event::UnitMoved { .. }), "rechargé: {:?}", ev[0]);
}

/// Dégâts d'une attaque infligés au défenseur, pour un terrain défenseur (veg, rug)
/// et un terrain attaquant (attacker_veg) donnés.
fn attack_damage(attacker: UnitKind, attacker_veg: f32, def_veg: f32, def_rug: f32) -> i32 {
    let mut w = World::new(5, 40, 30);
    plain(&mut w, 5, 5);
    plain(&mut w, 6, 5);
    let a = idx(&w, 5, 5);
    w.tiles[a].vegetation = attacker_veg;
    let d = idx(&w, 6, 5);
    w.tiles[d].vegetation = def_veg;
    w.tiles[d].ruggedness = def_rug;
    w.units.push(Unit {
        id: 1,
        owner: 0,
        kind: attacker,
        x: 5,
        y: 5,
        hp: 300,
        moves_left: 30,
    });
    w.units.push(Unit {
        id: 2,
        owner: 1,
        kind: UnitKind::Infantry,
        x: 6,
        y: 5,
        hp: 300,
        moves_left: 30,
    });
    w.apply(Command::DeclareWar {
        nation: 0,
        target: 1,
    });
    let ev = w.apply(Command::AttackUnit { unit: 1, x: 6, y: 5 });
    match &ev[0] {
        Event::UnitAttacked { damage, .. } => *damage,
        o => panic!("attendu UnitAttacked, eu {o:?}"),
    }
}

#[test]
fn terrain_defense_reduces_damage() {
    let open = attack_damage(UnitKind::Infantry, 0.0, 0.0, 0.0);
    let forest = attack_damage(UnitKind::Infantry, 0.0, 1.0, 0.0);
    let hills = attack_damage(UnitKind::Infantry, 0.0, 0.0, 1.0);
    assert!(forest < open, "la végétation protège le défenseur ({open} -> {forest})");
    assert!(hills < open, "le relief protège le défenseur ({open} -> {hills})");
}

#[test]
fn archer_has_forest_attack_malus() {
    // Des archers qui tirent DEPUIS une forêt ont un malus d'attaque.
    let open = attack_damage(UnitKind::Archer, 0.0, 0.0, 0.0);
    let from_forest = attack_damage(UnitKind::Archer, 1.0, 0.0, 0.0);
    assert!(
        from_forest < open,
        "archers en forêt : malus d'attaque ({open} -> {from_forest})"
    );
}

#[test]
fn attack_requires_war() {
    let mut w = World::new(6, 40, 30);
    plain(&mut w, 5, 5);
    plain(&mut w, 6, 5);
    w.units.push(Unit {
        id: 1,
        owner: 0,
        kind: UnitKind::Infantry,
        x: 5,
        y: 5,
        hp: 100,
        moves_left: 30,
    });
    w.units.push(Unit {
        id: 2,
        owner: 1,
        kind: UnitKind::Infantry,
        x: 6,
        y: 5,
        hp: 100,
        moves_left: 30,
    });
    let ev = w.apply(Command::AttackUnit { unit: 1, x: 6, y: 5 });
    assert!(matches!(ev[0], Event::CommandRejected { .. }), "pas en guerre -> rejet");
}

#[test]
fn unit_destroyed_at_zero_hp() {
    let mut w = World::new(7, 40, 30);
    plain(&mut w, 5, 5);
    plain(&mut w, 6, 5);
    w.units.push(Unit {
        id: 1,
        owner: 0,
        kind: UnitKind::Cavalry,
        x: 5,
        y: 5,
        hp: 300,
        moves_left: 55,
    });
    w.units.push(Unit {
        id: 2,
        owner: 1,
        kind: UnitKind::Infantry,
        x: 6,
        y: 5,
        hp: 10, // fragile -> détruit en un coup
        moves_left: 30,
    });
    w.apply(Command::DeclareWar {
        nation: 0,
        target: 1,
    });
    let ev = w.apply(Command::AttackUnit { unit: 1, x: 6, y: 5 });
    assert!(
        ev.iter().any(|e| matches!(e, Event::UnitDestroyed { unit: 2, .. })),
        "le défenseur doit être détruit: {ev:?}"
    );
    assert!(w.units.iter().all(|u| u.id != 2), "défenseur retiré du monde");
    assert!(w.units.iter().any(|u| u.id == 1), "l'attaquant survit (pas de riposte d'un mort)");
}

#[test]
fn unit_regenerates_on_home_territory_only() {
    // Sur son territoire national : régénère en consommant du manpower.
    let mut w = World::new(33, 40, 30);
    plain(&mut w, 5, 5);
    w.apply(Command::Settle {
        x: 5,
        y: 5,
        nation: 0,
        population: 100,
    });
    let ni = w.nations.iter().position(|n| n.id == 0).unwrap();
    w.nations[ni].manpower = 100;
    w.units.push(Unit {
        id: 1,
        owner: 0,
        kind: UnitKind::Infantry,
        x: 5,
        y: 5,
        hp: 50,
        moves_left: 30,
    });
    let mp0 = w.nation(0).unwrap().manpower;
    w.apply(Command::Step);
    assert!(w.units[0].hp > 50, "régénère chez soi (50 -> {})", w.units[0].hp);
    assert!(
        w.nation(0).unwrap().manpower < mp0,
        "la régénération consomme du manpower"
    );

    // Sur une case neutre (non possédée) : aucune régénération.
    let mut w2 = World::new(34, 40, 30);
    plain(&mut w2, 7, 7);
    w2.units.push(Unit {
        id: 1,
        owner: 0,
        kind: UnitKind::Infantry,
        x: 7,
        y: 7,
        hp: 50,
        moves_left: 30,
    });
    w2.apply(Command::Step);
    assert_eq!(w2.units[0].hp, 50, "pas de régénération en terre neutre");
}

#[test]
fn units_are_deterministic() {
    let run = || {
        let mut w = World::new(9, 50, 40);
        for x in 5..20 {
            plain(&mut w, x, 7);
        }
        setup_barracks(&mut w, 5, 7, 0);
        w.apply(Command::CreateUnit {
            x: 5,
            y: 7,
            nation: 0,
            kind: UnitKind::Infantry,
        });
        let id = w.units[0].id;
        w.apply(Command::MoveUnit {
            unit: id,
            to_x: 7,
            to_y: 7,
        });
        for _ in 0..5 {
            w.apply(Command::Step);
        }
        w.apply(Command::MoveUnit {
            unit: id,
            to_x: 9,
            to_y: 7,
        });
        w.checksum()
    };
    assert_eq!(run(), run(), "le système d'unités doit être déterministe");
}
