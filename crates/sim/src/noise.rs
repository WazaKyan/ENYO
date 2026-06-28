//! Bruit de valeur déterministe avec **enroulement (wrap) sur X** — indispensable
//! pour un monde cylindrique (bord est relié au bord ouest, cf. `PLAN.md`).
//!
//! Pas de dépendance, arithmétique simple : reproductible sur une même machine.

/// Hash déterministe d'un point du réseau (lattice) -> f32 dans [0,1).
fn lattice(seed: u64, xi: i64, yi: i64) -> f32 {
    let mut h = seed;
    h ^= (xi as u64).wrapping_mul(0xA076_1D64_78BD_642F);
    h ^= (yi as u64).wrapping_mul(0xE703_7ED1_A0B4_28DB);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    ((h >> 11) as f64 / (1u64 << 53) as f64) as f32
}

/// Lissage de Hermite (smoothstep) pour des transitions douces.
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Bruit de valeur bilinéaire ; X s'enroule sur `gx` cellules, Y est borné.
fn value_noise(seed: u64, u: f32, v: f32, gx: i64, gy: i64) -> f32 {
    let fx = u * gx as f32;
    let fy = v * gy as f32;
    let x0 = fx.floor() as i64;
    let y0 = fy.floor() as i64;
    let tx = smoothstep(fx - x0 as f32);
    let ty = smoothstep(fy - y0 as f32);

    // Enroulement sur X (modulo), bornage sur Y.
    let x0w = x0.rem_euclid(gx);
    let x1w = (x0 + 1).rem_euclid(gx);
    let y0c = y0.clamp(0, gy);
    let y1c = (y0 + 1).clamp(0, gy);

    let v00 = lattice(seed, x0w, y0c);
    let v10 = lattice(seed, x1w, y0c);
    let v01 = lattice(seed, x0w, y1c);
    let v11 = lattice(seed, x1w, y1c);

    let a = v00 + (v10 - v00) * tx;
    let b = v01 + (v11 - v01) * tx;
    a + (b - a) * ty
}

/// fBm (somme d'octaves) dans [0,1), enroulé sur X.
///
/// `base_x` / `base_y` = nombre de cellules de la première octave ; chaque octave
/// double la fréquence et divise l'amplitude par deux.
pub fn fbm(seed: u64, u: f32, v: f32, octaves: u32, base_x: i64, base_y: i64) -> f32 {
    let mut amp = 1.0f32;
    let mut sum = 0.0f32;
    let mut norm = 0.0f32;
    for o in 0..octaves {
        let gx = base_x << o;
        let gy = base_y << o;
        let s = seed ^ 0x9E37_79B9_7F4A_7C15u64.wrapping_mul(o as u64 + 1);
        sum += amp * value_noise(s, u, v, gx, gy);
        norm += amp;
        amp *= 0.5;
    }
    sum / norm
}
