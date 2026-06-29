//! Test d'intégration : l'IA **fait la guerre avec des unités** de bout en bout —
//! elle recrute à sa caserne, marche vers l'ennemi, l'occupe, et le fait capituler
//! (annexion). Scénario sur une bande de terre continue (pas de coupure maritime).

use ai::plan;
use proto::{Building, Command};
use sim::nation::FER;
use sim::tile::TileKind;
use sim::World;

fn idx(w: &World, x: u32, y: u32) -> usize {
    (y * w.width + x) as usize
}

fn land(w: &mut World, x: u32, y: u32) {
    let i = idx(w, x, y);
    let t = &mut w.tiles[i];
    t.kind = TileKind::Land;
    t.ruggedness = 0.0;
    t.precip_now = 0.0;
    t.temperature = 15.0;
}

#[test]
fn ai_wages_and_wins_a_war() {
    let mut w = World::new(1, 40, 8);
    let y = 4;
    // Bande de terre continue sur TOUTE la largeur (cylindre) reliant les nations
    // dans les deux sens — l'IA prendra le plus court chemin (enroulement X).
    for x in 0..w.width {
        land(&mut w, x, y);
    }
    // Nation 0 (attaquante) : ville + caserne + industrie.
    w.apply(Command::Settle { x: 3, y, nation: 0, population: 3000 });
    w.apply(Command::Settle { x: 4, y, nation: 0, population: 2000 });
    w.apply(Command::Settle { x: 5, y, nation: 0, population: 1000 });
    let i = idx(&w, 3, y);
    w.tiles[i].building = Some(Building::City);
    let i = idx(&w, 4, y);
    w.tiles[i].building = Some(Building::Military);
    let i = idx(&w, 5, y);
    w.tiles[i].building = Some(Building::Industry);
    // Ressources + tech militaire pour recruter.
    let n0 = w.nations.iter().position(|n| n.id == 0).unwrap();
    w.nations[n0].money = 5000;
    w.nations[n0].materials = 500;
    w.nations[n0].manpower = 500;
    w.nations[n0].tech[FER] = 1;
    // Nation 1 (défenseure) : deux cases nues, loin sur la bande.
    w.apply(Command::Settle { x: 25, y, nation: 1, population: 100 });
    w.apply(Command::Settle { x: 26, y, nation: 1, population: 100 });

    assert_eq!(w.nation_stats(1).1, 2, "l'ennemi part avec 2 cases");
    w.apply(Command::DeclareWar { nation: 0, target: 1 });

    // L'IA joue la nation 0 chaque mois : elle recrute, marche vers l'ennemi,
    // l'occupe, et finit par le faire capituler (annexion → 0 case restante).
    let mut won = false;
    for _ in 0..150 {
        for c in plan(&w, 0) {
            w.apply(c);
        }
        w.apply(Command::Step);
        if w.nation_stats(1).1 == 0 {
            won = true;
            break;
        }
    }
    assert!(
        won,
        "l'IA doit conquérir l'ennemi par occupation (restant : {} cases)",
        w.nation_stats(1).1
    );
}
