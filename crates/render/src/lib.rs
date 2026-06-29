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

    // Marqueurs de villes : repérables même à petite échelle.
    let m = (scale + 1).max(3);
    for ty in 0..world.height {
        for tx in 0..world.width {
            let t = world.tile(tx, ty);
            if let Some(o) = t.owner {
                let urban = t.development.max(t.population / 4000.0);
                if urban > 0.3 {
                    let nc = NATION_COLORS[o as usize % NATION_COLORS.len()];
                    let bright = lerp(nc, [255, 250, 230], 0.3);
                    let (px0, py0) = (tx * scale, ty * scale);
                    fill_block(&mut img, px0, py0, m, m, [20, 18, 16]);
                    fill_block(
                        &mut img,
                        px0 + 1,
                        py0 + 1,
                        m.saturating_sub(2),
                        m.saturating_sub(2),
                        bright,
                    );
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

/// Ancre le coin du viewport en centrant la caméra, borné à la carte.
fn clamp_cam(cam: u32, span: u32, max: u32) -> u32 {
    cam.saturating_sub(span / 2).min(max.saturating_sub(span))
}

/// (x0, y0, cols, rows, px) du viewport — source unique partagée par le rendu
/// ET le mapping souris→case (l'UI en a besoin pour les clics).
pub fn viewport_rect(
    world: &World,
    cam_x: u32,
    cam_y: u32,
    px: u32,
    win_w: u32,
    win_h: u32,
) -> (u32, u32, u32, u32, u32) {
    let px = px.clamp(4, 64);
    let cols = (win_w / px).max(1);
    let rows = (win_h / px).max(1);
    let x0 = clamp_cam(cam_x, cols, world.width);
    let y0 = clamp_cam(cam_y, rows, world.height);
    (x0, y0, cols, rows, px)
}

/// Viewport → buffer ARGB (`0x00RRGGBB`) de `win_w`×`win_h`, pour minifb.
/// La caméra (en cases) est centrée ; le pixel fenêtre == le pixel PNG.
pub fn viewport_argb(
    world: &World,
    cam_x: u32,
    cam_y: u32,
    px: u32,
    win_w: u32,
    win_h: u32,
) -> Vec<u32> {
    let (x0, y0, cols, rows, px) = viewport_rect(world, cam_x, cam_y, px, win_w, win_h);
    let img = region(world, x0, y0, cols, rows, px);
    let mut buf = vec![0u32; (win_w * win_h) as usize];
    let iw = img.width().min(win_w);
    let ih = img.height().min(win_h);
    for y in 0..ih {
        for x in 0..iw {
            let p = img.get_pixel(x, y).0;
            buf[(y * win_w + x) as usize] =
                ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | p[2] as u32;
        }
    }
    buf
}

/// Même viewport, sauvegardé en PNG (voie agent : fenêtre == PNG).
pub fn viewport_png(
    world: &World,
    cam_x: u32,
    cam_y: u32,
    px: u32,
    win_w: u32,
    win_h: u32,
    path: &str,
) -> Result<(), String> {
    let (x0, y0, cols, rows, px) = viewport_rect(world, cam_x, cam_y, px, win_w, win_h);
    region(world, x0, y0, cols, rows, px)
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
            let bx = rx * px;
            let by = ry * px;
            draw_tile(&mut img, bx, by, px, t);
            if let Some(o) = t.owner {
                let nc = NATION_COLORS[o as usize % NATION_COLORS.len()];
                tint_block(&mut img, bx, by, px, nc, 0.5);
            }
            if t.devastation > 0.04 {
                tint_block(
                    &mut img,
                    bx,
                    by,
                    px,
                    [70, 22, 22],
                    (t.devastation * 0.7).min(0.7),
                );
            }
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

/// Boîte englobante des cases d'UNE nation (+ marge).
pub fn nation_bbox(world: &World, nation: u16, pad: u32) -> Option<(u32, u32, u32, u32)> {
    let (mut minx, mut miny, mut maxx, mut maxy) = (u32::MAX, u32::MAX, 0u32, 0u32);
    let mut any = false;
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.owner == Some(nation) {
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

/// Rend la région d'UNE nation (zoom serré) et la sauvegarde en PNG.
pub fn save_region(world: &World, nation: u16, px: u32, path: &str) -> Result<(u32, u32), String> {
    let (x0, y0, w, h) = nation_bbox(world, nation, 6).ok_or("nation introuvable")?;
    region(world, x0, y0, w, h, px.max(1))
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok((w * px.max(1), h * px.max(1)))
}

/// Hash déterministe d'un pixel (texture stable d'un rendu à l'autre).
fn hash2(x: u32, y: u32) -> u32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B1) ^ (y as u64).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h as u32
}

/// Dessine la tuile pixel-art d'une case : fond ditheré + motifs (arbres, dunes,
/// touffes d'herbe, pics de montagne, vaguelettes).
fn draw_tile(img: &mut RgbImage, bx: u32, by: u32, px: u32, t: &Tile) {
    let base = base_color(t);
    let lo = scale_color(base, 0.88);
    let hi = scale_color(base, 1.08);

    // Fond ditheré.
    for yy in 0..px {
        for xx in 0..px {
            let n = hash2(bx + xx, by + yy) % 100;
            let c = if t.kind == TileKind::Ocean {
                if (yy + xx / 3) % 4 == 0 {
                    hi
                } else if n < 16 {
                    lo
                } else {
                    base
                }
            } else if n < 20 {
                lo
            } else if n > 84 {
                hi
            } else {
                base
            };
            put(img, bx + xx, by + yy, c);
        }
    }

    if t.kind == TileKind::Ocean {
        return;
    }
    // Montagne : un pic remplace les motifs de biome.
    if t.elevation > 0.8 {
        draw_peak(img, bx, by, px, t.elevation);
        return;
    }
    match t.biome {
        Biome::Boreal | Biome::TemperateForest | Biome::TropicalForest => {
            let dark = scale_color(base, 0.62);
            let light = scale_color(base, 1.2);
            for k in 0..(2 + px / 6) {
                let h = hash2(bx + k * 7 + 1, by + k * 13 + 1);
                let ox = bx + h % px.saturating_sub(2).max(1);
                let oy = by + (h >> 9) % px.saturating_sub(3).max(1);
                draw_tree(img, ox, oy, dark, light);
            }
        }
        Biome::Desert | Biome::Savanna => {
            let light = scale_color(base, 1.18);
            for k in 0..2 {
                let h = hash2(bx + k * 11 + 3, by + k * 5 + 3);
                let ox = bx + h % px.saturating_sub(3).max(1);
                let oy = by + (h >> 9) % px.max(1);
                put(img, ox, oy, light);
                put(img, ox + 1, oy, light);
                put(img, ox + 2, oy, light);
            }
        }
        Biome::Grassland => {
            let dark = scale_color(base, 0.7);
            for k in 0..3 {
                let h = hash2(bx + k * 9 + 2, by + k * 6 + 2);
                let ox = bx + h % px.max(1);
                let oy = by + (h >> 9) % px.saturating_sub(1).max(1);
                put(img, ox, oy, dark);
                put(img, ox, oy + 1, dark);
            }
        }
        _ => {}
    }
}

/// Pose un pixel (borné à l'image).
fn put(img: &mut RgbImage, x: u32, y: u32, c: [u8; 3]) {
    if x < img.width() && y < img.height() {
        img.put_pixel(x, y, Rgb(c));
    }
}

/// Petit arbre pixel-art : canopée 2×2 + tronc.
fn draw_tree(img: &mut RgbImage, ox: u32, oy: u32, dark: [u8; 3], light: [u8; 3]) {
    put(img, ox, oy, dark);
    put(img, ox + 1, oy, light);
    put(img, ox, oy + 1, dark);
    put(img, ox + 1, oy + 1, dark);
    put(img, ox + 1, oy + 2, [70, 46, 28]); // tronc
}

/// Pic de montagne : triangle clair, neige au sommet en haute altitude.
fn draw_peak(img: &mut RgbImage, bx: u32, by: u32, px: u32, elevation: f32) {
    let rock = [124, 116, 108];
    let lightr = [176, 168, 158];
    let snow = [238, 240, 244];
    let cx = bx + px / 2;
    let rows = (px / 2).max(2);
    let top = by + px.saturating_sub(rows);
    for r in 0..rows {
        let y = top + r;
        for d in 0..=r {
            let shade = if r < 2 {
                if elevation > 0.88 {
                    snow
                } else {
                    lightr
                }
            } else if d == r {
                lightr
            } else {
                rock
            };
            put(img, cx.saturating_sub(d), y, shade);
            put(img, cx + d, y, shade);
        }
    }
}

/// Mélange tout le bloc vers une couleur (teinte de nation, dévastation…).
fn tint_block(img: &mut RgbImage, bx: u32, by: u32, px: u32, color: [u8; 3], t: f32) {
    for y in by..(by + px).min(img.height()) {
        for x in bx..(bx + px).min(img.width()) {
            let p = img.get_pixel(x, y).0;
            img.put_pixel(x, y, Rgb(lerp(p, color, t)));
        }
    }
}

/// Tuile d'exemple (pour la planche de tileset).
fn sample(kind: TileKind, biome: Biome, elevation: f32) -> Tile {
    Tile {
        kind,
        elevation,
        ruggedness: 0.3,
        mean_temperature: 15.0,
        precipitation: 0.5,
        biome,
        vegetation: 0.5,
        soil_fertility: 0.5,
        wildlife: 0.3,
        marine_life: 0.3,
        temperature: 15.0,
        precip_now: 0.5,
        owner: None,
        population: 0.0,
        development: 0.0,
        devastation: 0.0,
        force: 0.0,
    }
}

/// Planche du tileset (asset visuel) : chaque biome/type en patch texturé.
pub fn tileset_sheet(px: u32) -> RgbImage {
    use Biome::*;
    use TileKind::{Land, Ocean as Sea};
    let samples = [
        sample(Sea, Ocean, 0.15),
        sample(Sea, Ocean, 0.45),
        sample(Land, Grassland, 0.6),
        sample(Land, TemperateForest, 0.6),
        sample(Land, TropicalForest, 0.55),
        sample(Land, Boreal, 0.6),
        sample(Land, Savanna, 0.6),
        sample(Land, Desert, 0.6),
        sample(Land, Tundra, 0.6),
        sample(Land, Ice, 0.6),
        sample(Land, Grassland, 0.85), // roche (altitude)
        sample(Land, Grassland, 0.95), // neige (altitude)
    ];
    let px = px.max(4);
    let patch = 4u32;
    let cols = 6u32;
    let cell = patch * px;
    let gap = 3u32;
    let rows = (samples.len() as u32).div_ceil(cols);
    let mut img = RgbImage::new(cols * (cell + gap) + gap, rows * (cell + gap) + gap);
    for p in img.pixels_mut() {
        *p = Rgb([18, 18, 22]);
    }
    for (i, t) in samples.iter().enumerate() {
        let cx = (i as u32 % cols) * (cell + gap) + gap;
        let cy = (i as u32 / cols) * (cell + gap) + gap;
        for ty in 0..patch {
            for tx in 0..patch {
                draw_tile(&mut img, cx + tx * px, cy + ty * px, px, t);
            }
        }
    }
    img
}

/// Rend la planche de tileset et la sauvegarde en PNG.
pub fn save_tileset(px: u32, path: &str) -> Result<(), String> {
    tileset_sheet(px)
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|e| e.to_string())
}

/// Encode une suite de frames en **GIF animé** (boucle infinie) — pour regarder
/// la partie évoluer.
pub fn save_gif(frames: &[RgbImage], path: &str, delay_ms: u32) -> Result<(), String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame};
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = GifEncoder::new_with_speed(file, 10);
    enc.set_repeat(Repeat::Infinite)
        .map_err(|e| e.to_string())?;
    for f in frames {
        let rgba = image::DynamicImage::ImageRgb8(f.clone()).into_rgba8();
        let frame = Frame::from_parts(rgba, 0, 0, Delay::from_numer_denom_ms(delay_ms, 1));
        enc.encode_frame(frame).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Planche-contact : toutes les frames en grille (toute l'évolution en 1 image).
pub fn contact_sheet(frames: &[RgbImage], cols: u32) -> RgbImage {
    if frames.is_empty() {
        return RgbImage::new(1, 1);
    }
    let (fw, fh) = (frames[0].width(), frames[0].height());
    let cols = cols.max(1);
    let gap = 4u32;
    let rows = (frames.len() as u32).div_ceil(cols);
    let mut sheet = RgbImage::new(cols * (fw + gap) + gap, rows * (fh + gap) + gap);
    for p in sheet.pixels_mut() {
        *p = Rgb([18, 18, 22]);
    }
    for (i, f) in frames.iter().enumerate() {
        let ox = (i as u32 % cols) * (fw + gap) + gap;
        let oy = (i as u32 / cols) * (fh + gap) + gap;
        for y in 0..fh.min(f.height()) {
            for x in 0..fw.min(f.width()) {
                sheet.put_pixel(ox + x, oy + y, *f.get_pixel(x, y));
            }
        }
    }
    sheet
}

/// Rend la planche-contact et la sauvegarde en PNG.
pub fn save_contact(frames: &[RgbImage], cols: u32, path: &str) -> Result<(), String> {
    contact_sheet(frames, cols)
        .save_with_format(path, image::ImageFormat::Png)
        .map_err(|e| e.to_string())
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
        return lerp([20, 40, 80], [52, 108, 164], shallow);
    }

    // Terre : montagnes au-dessus d'un seuil d'altitude, sinon biome.
    if t.elevation > 0.9 {
        return [234, 238, 242]; // neige
    }
    if t.elevation > 0.8 {
        return lerp([104, 98, 92], [148, 142, 134], (t.elevation - 0.8) / 0.1); // roche
    }

    let c = biome_color(t.biome);
    // Ombrage solaire léger (évite le délavage).
    let above = ((t.elevation - SEA_LEVEL) / (1.0 - SEA_LEVEL)).clamp(0.0, 1.0);
    scale_color(c, 0.94 + 0.14 * above)
}

/// Palette de biomes (tons terreux/fantasy).
fn biome_color(b: Biome) -> [u8; 3] {
    match b {
        Biome::Ocean => [38, 78, 132],
        Biome::Ice => [216, 228, 236],
        Biome::Tundra => [134, 148, 130],
        Biome::Boreal => [40, 82, 60],
        Biome::Grassland => [104, 158, 72],
        Biome::Desert => [216, 196, 128],
        Biome::Savanna => [182, 162, 84],
        Biome::TemperateForest => [54, 112, 66],
        Biome::TropicalForest => [32, 98, 54],
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
