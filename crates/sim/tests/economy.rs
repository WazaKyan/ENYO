//! Tests de l'économie interne (S8, E1) : ressources, construction, production
//! d'industrie (matériaux ∝ stats × main-d'œuvre), pollution, et coûts.

use proto::{Building, Command, Event};
use sim::nation::{STARTING_INFLUENCE, STARTING_MONEY};
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
    // (L'habitation produite par le commerce sert désormais à FONDER des villes ;
    //  cf. `city_grows_population`.)
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
    assert_eq!(w.nation(0).unwrap().influence, STARTING_INFLUENCE);
    w.apply(Command::Step);
    w.apply(Command::Step);
    assert_eq!(
        w.nation(0).unwrap().influence,
        STARTING_INFLUENCE + 2,
        "influence +1/mois"
    );
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

/// Gain de savoir sur 10 mois quand on pose industrie + (commerce?) + université
/// sur 3 cases alignées et peuplées. La base densité est identique dans les deux
/// cas → la différence isole la production de l'université (qui exige un commerce).
fn science_gain(with_commerce: bool) -> f32 {
    let mut w = World::new(13, 80, 60);
    let [a, b, c] = three_land_in_row(&w);
    for p in [a, b, c] {
        w.apply(Command::Settle {
            x: p.0,
            y: p.1,
            nation: 0,
            population: 1500,
        });
    }
    // Industrie sur A : accumuler les matériaux (commerce 20 + université 30).
    w.apply(Command::Build {
        x: a.0,
        y: a.1,
        nation: 0,
        building: Building::Industry,
    });
    for _ in 0..40 {
        w.apply(Command::Step);
    }
    if with_commerce {
        let ev = w.apply(Command::Build {
            x: b.0,
            y: b.1,
            nation: 0,
            building: Building::Commerce,
        });
        assert!(matches!(ev[0], Event::Built { .. }), "commerce bâti");
    }
    let ev = w.apply(Command::Build {
        x: c.0,
        y: c.1,
        nation: 0,
        building: Building::Education,
    });
    assert!(matches!(ev[0], Event::Built { .. }), "université bâtie");
    let k0 = w.nation(0).unwrap().knowledge;
    for _ in 0..10 {
        w.apply(Command::Step);
    }
    w.nation(0).unwrap().knowledge - k0
}

#[test]
fn education_makes_science_only_with_commerce() {
    let with = science_gain(true);
    let without = science_gain(false);
    assert!(
        with > without,
        "l'université connectée à un commerce produit plus de science ({without} -> {with})"
    );
}

#[test]
fn military_recruits_soldiers_for_upkeep() {
    let mut w = World::new(15, 80, 60);
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
    // Industrie sur B pour les matériaux de la caserne (40).
    w.apply(Command::Build {
        x: b.0,
        y: b.1,
        nation: 0,
        building: Building::Industry,
    });
    for _ in 0..40 {
        w.apply(Command::Step);
    }
    // Caserne sur A (120 argent + 40 matériaux).
    let ev = w.apply(Command::Build {
        x: a.0,
        y: a.1,
        nation: 0,
        building: Building::Military,
    });
    assert!(matches!(ev[0], Event::Built { .. }), "caserne bâtie");
    let f0 = w.nation(0).unwrap().manpower;
    let m0 = w.nation(0).unwrap().money;
    for _ in 0..5 {
        w.apply(Command::Step);
    }
    assert!(
        w.nation(0).unwrap().manpower > f0,
        "la caserne produit du manpower ({} -> {})",
        f0,
        w.nation(0).unwrap().manpower
    );
    assert!(
        w.nation(0).unwrap().money < m0,
        "la caserne coûte un entretien mensuel"
    );
}

/// Case à capacité ≥ 1500 (pop stable ≥ 1000 pour essaimer) avec un voisin terre.
fn high_cap_with_neighbor(w: &World) -> ((u32, u32), (u32, u32)) {
    for y in 0..w.height {
        for x in 0..w.width - 1 {
            if w.capacity_at(x, y) >= 1500.0 && w.tile(x + 1, y).kind == TileKind::Land {
                return ((x, y), (x + 1, y));
            }
        }
    }
    panic!("pas de case à haute capacité avec voisin terre");
}

#[test]
fn swarm_costs_influence() {
    let mut w = World::new(17, 80, 60);
    let (a, b) = high_cap_with_neighbor(&w);
    w.apply(Command::Settle {
        x: a.0,
        y: a.1,
        nation: 0,
        population: 2000,
    });
    // L'influence de départ (STARTING_INFLUENCE) permet de s'étendre d'emblée.
    let infl0 = w.nation(0).unwrap().influence;
    assert!(infl0 >= sim::SWARM_INFLUENCE, "influence de départ suffisante");
    let ev = w.apply(Command::Swarm {
        from_x: a.0,
        from_y: a.1,
        to_x: b.0,
        to_y: b.1,
    });
    assert!(matches!(ev[0], Event::Swarmed { .. }), "expansion réussie: {:?}", ev[0]);
    assert_eq!(
        w.nation(0).unwrap().influence,
        infl0 - sim::SWARM_INFLUENCE,
        "l'expansion consomme de l'influence"
    );
}

#[test]
fn city_grows_population() {
    // Refonte « villes uniquement » : seule une case VILLE engendre de la
    // population ; une case possédée SANS ville garde sa population (main-d'œuvre).
    let mut w = World::new(19, 80, 60);
    let [a, b, _] = three_land_in_row(&w);
    for p in [a, b] {
        w.apply(Command::Settle {
            x: p.0,
            y: p.1,
            nation: 0,
            population: 200,
        });
    }
    // Ville sur A seulement (coûte 100 argent + 50 habitation ; départ : 60 hab).
    let ev = w.apply(Command::Build {
        x: a.0,
        y: a.1,
        nation: 0,
        building: Building::City,
    });
    assert!(matches!(ev[0], Event::Built { .. }), "ville fondée sur A");
    for _ in 0..20 {
        w.apply(Command::Step);
    }
    assert!(
        w.tile(a.0, a.1).population > 200.0,
        "la ville doit croître (got {})",
        w.tile(a.0, a.1).population
    );
    assert!(
        (w.tile(b.0, b.1).population - 200.0).abs() < 1.0,
        "une case sans ville ne croît pas (got {})",
        w.tile(b.0, b.1).population
    );
}

#[test]
fn unfed_dense_city_starves() {
    // Famine : la population au-delà de la subsistance (1500/case) qui n'est pas
    // nourrie décline. Sans ferme, une case dense reflue vers la subsistance.
    let mut w = World::new(29, 80, 60);
    let (x, y) = productive(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 3000, // bien au-dessus de la subsistance
    });
    let p0 = w.tile(x, y).population;
    for _ in 0..8 {
        w.apply(Command::Step);
    }
    let p1 = w.tile(x, y).population;
    assert!(p1 < p0, "sans nourriture, la population dense décline ({p0} -> {p1})");
    assert!(
        p1 > 1000.0,
        "la famine reflue vers la subsistance, sans tout effacer (got {p1})"
    );
}

#[test]
fn farm_produces_food() {
    let mut w = World::new(23, 80, 60);
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
    // Industrie sur B pour les matériaux de la ferme (15).
    w.apply(Command::Build {
        x: b.0,
        y: b.1,
        nation: 0,
        building: Building::Industry,
    });
    for _ in 0..15 {
        w.apply(Command::Step);
    }
    let ev = w.apply(Command::Build {
        x: a.0,
        y: a.1,
        nation: 0,
        building: Building::Farm,
    });
    assert!(matches!(ev[0], Event::Built { .. }), "ferme bâtie");
    let f0 = w.nation(0).unwrap().food;
    for _ in 0..10 {
        w.apply(Command::Step);
    }
    assert!(
        w.nation(0).unwrap().food > f0,
        "la ferme produit de la nourriture ({} -> {})",
        f0,
        w.nation(0).unwrap().food
    );
}

#[test]
fn demolish_refunds_scaled_and_allows_rebuild() {
    let mut w = World::new(31, 80, 60);
    let (x, y) = productive(&w);
    w.apply(Command::Settle {
        x,
        y,
        nation: 0,
        population: 500,
    });
    w.apply(Command::Build {
        x,
        y,
        nation: 0,
        building: Building::Industry, // coût 100 argent
    });
    // Dévaste la case à 50 % -> remboursement = 50 % × (1 − 0.5) × 100 = 25.
    let i = (y * w.width + x) as usize;
    w.tiles[i].devastation = 0.5;
    let money = w.nation(0).unwrap().money;
    let ev = w.apply(Command::Demolish { x, y, nation: 0 });
    assert!(matches!(ev[0], Event::Demolished { .. }), "démoli: {:?}", ev[0]);
    assert_eq!(w.tile(x, y).building, None, "case vidée");
    assert_eq!(
        w.nation(0).unwrap().money,
        money + 25,
        "remboursement réduit par la dévastation"
    );
    // On peut reconstruire autre chose à la place.
    let ev = w.apply(Command::Build {
        x,
        y,
        nation: 0,
        building: Building::City,
    });
    assert!(matches!(ev[0], Event::Built { .. }), "reconstruction: {:?}", ev[0]);
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
