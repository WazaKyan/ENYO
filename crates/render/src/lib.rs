//! Rendu **headless → PNG** d'ENYO. Pas de fenêtre : on produit des images que
//! l'on peut inspecter directement (idéal pour itérer sur l'esthétique).
//!
//! Deux vues : [`overview`] (carte entière, 1+ px/case) et [`region`] (zoom
//! pixel-art via un tileset, à venir).

use image::{Rgb, RgbImage};
use sim::tile::{Biome, Tile, TileKind};
use sim::World;

/// Niveau de la mer (doit suivre `worldgen::SEA_LEVEL`).
const SEA_LEVEL: f32 = 0.5;

/// Palette de couleurs distinctes par nation (teinte du territoire).
const NATION_COLORS: [[u8; 3]; 12] = [
    [206, 74, 74],   // rouge
    [74, 114, 206],  // bleu
    [86, 176, 96],   // vert
    [214, 182, 74],  // or
    [150, 92, 196],  // violet
    [224, 134, 60],  // orange
    [80, 184, 196],  // cyan
    [206, 96, 158],  // rose
    [150, 160, 70],  // olive
    [120, 130, 150], // ardoise
    [180, 120, 90],  // terre
    [110, 196, 150], // menthe
];

/// Carte entière : `scale` pixels par case. Biomes + relief + nations + villes.
pub fn overview(world: &World, scale: u32) -> RgbImage {
    let scale = scale.max(1);
    let mut img = RgbImage::new(world.width * scale, world.height * scale);
    for ty in 0..world.height {
        for tx in 0..world.width {
            let color = Rgb(tile_color(world.tile(tx, ty)));
            for dy in 0..scale {
                for dx in 0..scale {
                    img.put_pixel(tx * scale + dx, ty * scale + dy, color);
                }
            }
        }
    }
    img
}

/// Rend la carte et la sauvegarde en PNG.
pub fn save_overview(world: &World, scale: u32, path: &str) -> Result<(), String> {
    overview(world, scale)
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|e| e.to_string())
}

/// Boîte englobante des cases possédées (+ marge), bornée à la carte.
pub fn nations_bbox(world: &World, pad: u32) -> Option<(u32, u32, u32, u32)> {
    let (mut minx, mut miny, mut maxx, mut maxy) = (u32::MAX, u32::MAX, 0u32, 0u32);
    let mut any = false;
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.owner.is_some() {
            any = true;
            let x = idx as u32 % world.width;
            let y = idx as u32 / world.width;
            minx = minx.min(x);
            miny = miny.min(y);
            maxx = maxx.max(x);
            maxy = maxy.max(y);
        }
    }
    if !any {
        return None;
    }
    let x0 = minx.saturating_sub(pad);
    let y0 = miny.saturating_sub(pad);
    let x1 = (maxx + pad).min(world.width - 1);
    let y1 = (maxy + pad).min(world.height - 1);
    Some((x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// Vue zoomée d'une région : terrain + territoires + frontières + villes + armées.
pub fn region(world: &World, x0: u32, y0: u32, w: u32, h: u32, px: u32) -> RgbImage {
    let px = px.max(1);
    let mut img = RgbImage::new(w * px, h * px);
    for ry in 0..h {
        for rx in 0..w {
            let tx = x0 + rx;
            let ty = y0 + ry;
            if tx >= world.width || ty >= world.height {
                continue;
            }
            let t = world.tile(tx, ty);
            let mut c = base_color(t);
            if let Some(o) = t.owner {
                c = lerp(c, NATION_COLORS[o as usize % NATION_COLORS.len()], 0.6);
            }
            if t.devastation > 0.04 {
                c = lerp(c, [70, 22, 22], (t.devastation * 0.7).min(0.7));
            }
            let bx = rx * px;
            let by = ry * px;
            fill_block(&mut img, bx, by, px, px, c);

            if let Some(o) = t.owner {
                draw_borders(&mut img, world, tx, ty, o, (bx, by), px);
            }
            let urban = t.development.max(t.population / 4000.0);
            if t.owner.is_some() && urban > 0.3 {
                draw_marker(&mut img, bx, by, px, [245, 232, 150]);
            }
            if t.force > 150.0 {
                let s = (px / 3).max(1);
                fill_block(&mut img, bx, by, s, s, [220, 40, 40]);
            }
        }
    }
    img
}

/// Rend la région englobant les nations et la sauvegarde en PNG.
pub fn save_region(world: &World, px: u32, path: &str) -> Result<(u32, u32), String> {
    let (x0, y0, w, h) = nations_bbox(world, 8).ok_or("aucune nation à cadrer")?;
    region(world, x0, y0, w, h, px.max(1))
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok((w * px.max(1), h * px.max(1)))
}

fn fill_block(img: &mut RgbImage, bx: u32, by: u32, w: u32, h: u32, c: [u8; 3]) {
    for y in by..(by + h).min(img.height()) {
        for x in bx..(bx + w).min(img.width()) {
            img.put_pixel(x, y, Rgb(c));
        }
    }
}

/// Trace un bord sombre sur les côtés de la case qui touchent une autre nation.
fn draw_borders(
    img: &mut RgbImage,
    world: &World,
    tx: u32,
    ty: u32,
    owner: u16,
    origin: (u32, u32),
    px: u32,
) {
    let (bx, by) = origin;
    let edge = [22, 22, 28];
    let differ = |nx: i64, ny: i64| -> bool {
        if ny < 0 || ny >= world.height as i64 {
            return false;
        }
        let xx = nx.rem_euclid(world.width as i64) as u32;
        world.tile(xx, ny as u32).owner != Some(owner)
    };
    let (x, y) = (tx as i64, ty as i64);
    if differ(x, y - 1) {
        fill_block(img, bx, by, px, 1, edge);
    }
    if differ(x, y + 1) {
        fill_block(img, bx, by + px.saturating_sub(1), px, 1, edge);
    }
    if differ(x - 1, y) {
        fill_block(img, bx, by, 1, px, edge);
    }
    if differ(x + 1, y) {
        fill_block(img, bx + px.saturating_sub(1), by, 1, px, edge);
    }
}

/// Marqueur central (ville) : carré clair cerné de sombre.
fn draw_marker(img: &mut RgbImage, bx: u32, by: u32, px: u32, c: [u8; 3]) {
    let s = (px / 2).max(2);
    let off = (px - s) / 2;
    fill_block(img, bx + off, by + off, s, s, [30, 26, 20]);
    if s > 2 {
        fill_block(img, bx + off + 1, by + off + 1, s - 2, s - 2, c);
    }
}

/// Couleur finale d'une case : biome → relief → territoire → ville → dévastation.
fn tile_color(t: &Tile) -> [u8; 3] {
    let mut c = base_color(t);

    // Territoire d'une nation : on teinte vers sa couleur.
    if let Some(o) = t.owner {
        let nc = NATION_COLORS[o as usize % NATION_COLORS.len()];
        c = lerp(c, nc, 0.45);
        // Ville : forte densité/développement → éclaircit (lumières).
        let urban = (t.development.max(t.population / 4000.0)).clamp(0.0, 1.0);
        if urban > 0.25 {
            c = lerp(c, [245, 240, 210], (urban * 0.6).min(0.55));
        }
    }

    // Dévastation : assombrit et vire au rouge sombre.
    if t.devastation > 0.04 {
        c = lerp(c, [70, 22, 22], (t.devastation * 0.7).min(0.7));
    }
    c
}

/// Couleur de base (biome + relief), sans couche anthropique.
fn base_color(t: &Tile) -> [u8; 3] {
    if t.kind == TileKind::Ocean {
        // Profond → clair selon l'altitude (0 = abysses, 0.5 = côte).
        let shallow = (t.elevation / SEA_LEVEL).clamp(0.0, 1.0);
        return lerp([24, 44, 84], [58, 104, 158], shallow);
    }

    // Terre : montagnes au-dessus d'un seuil d'altitude, sinon biome.
    if t.elevation > 0.9 {
        return [236, 238, 240]; // neige
    }
    if t.elevation > 0.8 {
        return lerp([108, 100, 92], [150, 144, 138], (t.elevation - 0.8) / 0.1);
        // roche
    }

    let c = biome_color(t.biome);
    // Léger ombrage solaire selon l'altitude (plus haut = plus clair).
    let shade = 0.9 + 0.35 * ((t.elevation - SEA_LEVEL) / (1.0 - SEA_LEVEL)).clamp(0.0, 1.0);
    scale_color(c, shade)
}

/// Palette de biomes (tons terreux/fantasy).
fn biome_color(b: Biome) -> [u8; 3] {
    match b {
        Biome::Ocean => [40, 80, 130],
        Biome::Ice => [226, 234, 240],
        Biome::Tundra => [156, 160, 146],
        Biome::Boreal => [48, 86, 66],
        Biome::Grassland => [126, 158, 90],
        Biome::Desert => [212, 194, 134],
        Biome::Savanna => [178, 166, 100],
        Biome::TemperateForest => [74, 122, 78],
        Biome::TropicalForest => [42, 100, 62],
    }
}

fn lerp(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
    ]
}

fn scale_color(c: [u8; 3], f: f32) -> [u8; 3] {
    [
        (c[0] as f32 * f).clamp(0.0, 255.0) as u8,
        (c[1] as f32 * f).clamp(0.0, 255.0) as u8,
        (c[2] as f32 * f).clamp(0.0, 255.0) as u8,
    ]
}
