//! Tests de génération du monde : déterminisme, présence terre/océan, et
//! continuité du raccord est-ouest (monde cylindrique).

use sim::World;

#[test]
fn generation_is_deterministic() {
    let a = World::new(2024, 200, 120);
    let b = World::new(2024, 200, 120);
    assert_eq!(a.checksum(), b.checksum());
    assert_eq!(a.land_tiles, b.land_tiles);
    assert_eq!(a.ocean_tiles, b.ocean_tiles);
}

#[test]
fn different_seeds_differ() {
    let a = World::new(1, 200, 120);
    let b = World::new(2, 200, 120);
    assert_ne!(a.checksum(), b.checksum());
}

#[test]
fn has_land_and_ocean() {
    let w = World::new(7, 200, 120);
    assert_eq!(w.land_tiles + w.ocean_tiles, 200 * 120);
    assert!(w.land_tiles > 0, "il doit y avoir de la terre");
    assert!(w.ocean_tiles > 0, "il doit y avoir de l'océan");
}

#[test]
fn x_wraps_seamlessly() {
    // Sur un cylindre, la colonne 0 et la colonne width-1 sont voisines :
    // l'altitude doit y être continue (le bruit s'enroule sur X).
    let w = World::new(42, 360, 180);
    let mut max_jump = 0.0f32;
    for y in 0..w.height {
        let left = w.tile(0, y).elevation;
        let right = w.tile(w.width - 1, y).elevation;
        max_jump = max_jump.max((left - right).abs());
    }
    assert!(max_jump < 0.25, "discontinuité au raccord est-ouest: {max_jump}");
}
