//! Tests de l'économie interne (S8, E1) : ressources, construction, production
//! d'industrie (matériaux ∝ stats × main-d'œuvre), pollution, et coûts.

use proto::{Building, Command, Event};
use sim::nation::STARTING_MONEY;
use sim::tile::TileKind;
use sim::World;

/// Première terre productive (capacité élevée → bonne main-d'œuvre potentielle).
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
fn influence_accrues_each_month() {
    let mut w = World::new(7, 80, 60);
    let (x, y) = productive(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 800,
    });
    assert_eq!(w.nation(0).unwrap().influence, 0);
    w.apply(Command::Step);
    w.apply(Command::Step);
    assert_eq!(w.nation(0).unwrap().influence, 2, "influence +1/mois");
}

#[test]
fn build_costs_money_and_industry_produces_materials() {
    let mut w = World::new(7, 80, 60);
    let (x, y) = productive(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 1500, // main-d'œuvre pleine
    });

    // Construire l'industrie : coûte de l'argent, pose le bâtiment.
    let ev = w.apply(Command::Build {
        x,
        y,
        nation: 0,
        building: Building::Industry,
    });
    assert!(matches!(ev[0], Event::Built { .. }), "industrie bâtie");
    assert_eq!(w.tile(x, y).building, Some(Building::Industry));
    assert!(
        w.nation(0).unwrap().money < STARTING_MONEY,
        "la construction coûte de l'argent"
    );
    assert_eq!(w.nation(0).unwrap().materials, 0);

    // Laisser produire quelques mois.
    let dev0 = w.tile(x, y).devastation;
    for _ in 0..5 {
        w.apply(Command::Step);
    }
    assert!(
        w.nation(0).unwrap().materials > 0,
        "l'industrie doit produire des matériaux (got {})",
        w.nation(0).unwrap().materials
    );
    assert!(
        w.tile(x, y).devastation > dev0,
        "l'industrie pollue (dévastation croît)"
    );
}

#[test]
fn build_rejected_when_unaffordable_or_invalid() {
    let mut w = World::new(7, 80, 60);
    let (x, y) = productive(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 500,
    });

    // Case non possédée -> rejet.
    let other = if x + 2 < w.width { (x + 2, y) } else { (x - 2, y) };
    let ev = w.apply(Command::Build {
        x: other.0,
        y: other.1,
        nation: 0,
        building: Building::Industry,
    });
    assert!(matches!(ev[0], Event::CommandRejected { .. }));

    // Déjà bâtie -> rejet.
    w.apply(Command::Build {
        x,
        y,
        nation: 0,
        building: Building::Industry,
    });
    let ev = w.apply(Command::Build {
        x,
        y,
        nation: 0,
        building: Building::Commerce,
    });
    assert!(matches!(ev[0], Event::CommandRejected { .. }));

    // Argent insuffisant -> rejet (on vide la bourse via des constructions).
    // STARTING_MONEY=500 ; industrie=100. On dépense jusqu'à ne plus pouvoir.
    let mut nw = World::new(8, 80, 60);
    let (px, py) = productive(&nw);
    nw.apply(Command::Settle {
        x: px,
        y: py,
        nation: 0,
        population: 500,
    });
    // Vider l'argent : 5 industries = 500 ; la 1re sur (px,py), les autres rejetées
    // (cases non possédées) — on force plutôt le rejet pour fonds insuffisants en
    // construisant sur la seule case possédée puis en re-tentant (déjà bâtie).
    // Ici on vérifie surtout le rejet "ressources insuffisantes" via un coût élevé.
    // (Le commerce coûte matériaux qu'on n'a pas.)
    let ev = nw.apply(Command::Build {
        x: px,
        y: py,
        nation: 0,
        building: Building::Commerce, // coûte 20 matériaux, on en a 0
    });
    assert!(
        matches!(ev[0], Event::CommandRejected { .. }),
        "commerce sans matériaux -> rejet"
    );
}
