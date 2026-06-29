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

/// Trois cases de terre PRODUCTIVES alignées horizontalement (A, B, C).
fn three_land_in_row(w: &World) -> [(u32, u32); 3] {
    for y in 0..w.height {
        for x in 0..w.width - 2 {
            if [0u32, 1, 2].iter().all(|&d| {
                w.tile(x + d, y).kind == TileKind::Land && w.capacity_at(x + d, y) > 300.0
            }) {
                return [(x, y), (x + 1, y), (x + 2, y)];
            }
        }
    }
    panic!("pas de rangée de 3 terres productives");
}

#[test]
fn infrastructure_pools_population() {
    use sim::connect::Networks;
    let mut w = World::new(11, 80, 60);
    let [a, b, c] = three_land_in_row(&w);
    for (p, pop) in [(a, 800u32), (b, 100), (c, 900)] {
        w.apply(Command::Settle {
            x: p.0,
            y: p.1,
            nation: 0,
            population: pop,
        });
    }
    // Industrie sur A pour accumuler les matériaux nécessaires à l'infra (20).
    w.apply(Command::Build {
        x: a.0,
        y: a.1,
        nation: 0,
        building: Building::Industry,
    });
    for _ in 0..30 {
        w.apply(Command::Step);
    }
    let c_idx = (c.1 * w.width + c.0) as usize;
    // Sans infra : C ne voit que son voisinage (C + B), pas A (non adjacente).
    let before = Networks::build(&w.tiles, w.width, w.height).connected_pop(&w.tiles, c_idx, 0);
    // Infra en B : C est reliée au réseau qui dessert A et C → pop de A mise en commun.
    let (nmon, nmat) = {
        let n = w.nation(0).unwrap();
        (n.money, n.materials)
    };
    let ev = w.apply(Command::Build {
        x: b.0,
        y: b.1,
        nation: 0,
        building: Building::Infrastructure,
    });
    assert!(
        matches!(ev[0], Event::Built { .. }),
        "infra bâtie (argent {nmon}, mat {nmat}, ev {:?})",
        ev[0]
    );
    let after = Networks::build(&w.tiles, w.width, w.height).connected_pop(&w.tiles, c_idx, 0);
    assert!(
        after > before,
        "l'infra met en commun la pop de la region ({before} -> {after})"
    );
}

#[test]
fn commerce_makes_money_and_housing_from_materials() {
    let mut w = World::new(11, 80, 60);
    let [a, b, _] = three_land_in_row(&w);
    w.apply(Command::Settle {
        x: a.0,
        y: a.1,
        nation: 0,
        population: 1500,
    });
    w.apply(Command::Settle {
        x: b.0,
        y: b.1,
        nation: 0,
        population: 1500,
    });
    // Industrie sur A -> accumule des matériaux.
    w.apply(Command::Build {
        x: a.0,
        y: a.1,
        nation: 0,
        building: Building::Industry,
    });
    for _ in 0..30 {
        w.apply(Command::Step);
    }
    assert!(
        w.nation(0).unwrap().materials >= 20,
        "l'industrie a accumulé des matériaux (got {})",
        w.nation(0).unwrap().materials
    );
    // Commerce sur B (coûte 120 argent + 20 matériaux).
    let ev = w.apply(Command::Build {
        x: b.0,
        y: b.1,
        nation: 0,
        building: Building::Commerce,
    });
    assert!(matches!(ev[0], Event::Built { .. }), "commerce bâti");
    let money0 = w.nation(0).unwrap().money;
    let house0 = w.nation(0).unwrap().housing;
    for _ in 0..15 {
        w.apply(Command::Step);
    }
    let n = w.nation(0).unwrap();
    assert!(
        n.money > money0,
        "le commerce génère de l'argent ({} -> {})",
        money0,
        n.money
    );
    assert!(n.housing > house0, "le commerce génère de l'habitation");
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
