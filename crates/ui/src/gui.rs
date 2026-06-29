//! GUI immédiate dessinée directement dans le framebuffer ARGB (0x00RRGGBB).
//! Police bitmap 8×8 (`font8x8`, données const pures) — aucune dépendance lourde,
//! rendu déterministe, lisible aussi bien dans la fenêtre que sur une capture PNG.

use font8x8::legacy::BASIC_LEGACY;

// Palette de l'interface (sombre, lisible, cohérente avec la carte).
pub const BG: u32 = 0x0E1116;
pub const PANEL: u32 = 0x121A24;
pub const PANEL_HI: u32 = 0x1B2735;
pub const ACCENT: u32 = 0x3A6EA5;
pub const ACCENT_HI: u32 = 0x5C92C7;
pub const BORDER: u32 = 0x36465A;
pub const TEXT: u32 = 0xEAF0F4;
pub const TEXT_DIM: u32 = 0x90A1B2;
pub const GOOD: u32 = 0x57A36C;
pub const WARN: u32 = 0xC2613F;

/// Surface de dessin : un buffer ARGB de taille `w*h`.
pub struct Canvas<'a> {
    pub buf: &'a mut [u32],
    pub w: i32,
    pub h: i32,
}

impl<'a> Canvas<'a> {
    pub fn new(buf: &'a mut [u32], w: usize, h: usize) -> Self {
        Self {
            buf,
            w: w as i32,
            h: h as i32,
        }
    }

    /// Rectangle plein opaque.
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        for yy in y.max(0)..(y + h).min(self.h) {
            let row = yy * self.w;
            for xx in x.max(0)..(x + w).min(self.w) {
                self.buf[(row + xx) as usize] = color;
            }
        }
    }

    /// Rectangle semi-transparent (voile) : mélange `color` sur le fond, `alpha` 0..255.
    pub fn blend_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32, alpha: u32) {
        let (cr, cg, cb) = ((color >> 16) & 255, (color >> 8) & 255, color & 255);
        let a = alpha.min(255);
        for yy in y.max(0)..(y + h).min(self.h) {
            let row = yy * self.w;
            for xx in x.max(0)..(x + w).min(self.w) {
                let idx = (row + xx) as usize;
                let bg = self.buf[idx];
                let (br, bgn, bb) = ((bg >> 16) & 255, (bg >> 8) & 255, bg & 255);
                let r = (cr * a + br * (255 - a)) / 255;
                let g = (cg * a + bgn * (255 - a)) / 255;
                let b = (cb * a + bb * (255 - a)) / 255;
                self.buf[idx] = (r << 16) | (g << 8) | b;
            }
        }
    }

    /// Contour d'un rectangle (1 px).
    pub fn rect_outline(&mut self, x: i32, y: i32, w: i32, h: i32, color: u32) {
        self.fill_rect(x, y, w, 1, color);
        self.fill_rect(x, y + h - 1, w, 1, color);
        self.fill_rect(x, y, 1, h, color);
        self.fill_rect(x + w - 1, y, 1, h, color);
    }

    /// Un caractère ASCII (8×8) mis à l'échelle `scale`.
    pub fn glyph(&mut self, x: i32, y: i32, c: char, scale: i32, color: u32) {
        let idx = ascii_fold(c) as usize;
        if idx >= 128 {
            return;
        }
        let g = BASIC_LEGACY[idx];
        for (row, bits) in g.iter().enumerate() {
            for col in 0..8 {
                // font8x8 : bit de poids faible = colonne de gauche.
                if (bits >> col) & 1 == 1 {
                    self.fill_rect(x + col * scale, y + row as i32 * scale, scale, scale, color);
                }
            }
        }
    }

    /// Texte (gauche).
    pub fn text(&mut self, x: i32, y: i32, s: &str, scale: i32, color: u32) {
        let mut cx = x;
        for c in s.chars() {
            self.glyph(cx, y, c, scale, color);
            cx += 8 * scale;
        }
    }

    /// Texte centré horizontalement sur `cx`.
    pub fn text_centered(&mut self, cx: i32, y: i32, s: &str, scale: i32, color: u32) {
        self.text(cx - text_w(s, scale) / 2, y, s, scale, color);
    }
}

/// Largeur en pixels d'une chaîne à l'échelle donnée.
pub fn text_w(s: &str, scale: i32) -> i32 {
    s.chars().count() as i32 * 8 * scale
}

/// Replie les accents (et quelques symboles) vers l'ASCII couvert par la police.
fn ascii_fold(c: char) -> char {
    match c {
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'à' | 'â' | 'ä' => 'a',
        'î' | 'ï' => 'i',
        'ô' | 'ö' => 'o',
        'û' | 'ü' | 'ù' => 'u',
        'ç' => 'c',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'À' | 'Â' => 'A',
        'Ô' => 'O',
        'Î' => 'I',
        'Ç' => 'C',
        '—' | '–' => '-',
        '→' => '>',
        '×' => 'x',
        '’' => '\'',
        '°' => 'o',
        _ => c,
    }
}

/// Un bouton rectangulaire cliquable (immédiat : recréé chaque frame).
pub struct Button {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub label: String,
}

impl Button {
    pub fn new(x: i32, y: i32, w: i32, h: i32, label: impl Into<String>) -> Self {
        Self {
            x,
            y,
            w,
            h,
            label: label.into(),
        }
    }

    pub fn hit(&self, mx: i32, my: i32) -> bool {
        mx >= self.x && mx < self.x + self.w && my >= self.y && my < self.y + self.h
    }

    /// Dessine le bouton. `hover` = survolé, `active` = état sélectionné (outil courant…).
    pub fn draw(&self, c: &mut Canvas, hover: bool, active: bool) {
        let bg = if active {
            ACCENT
        } else if hover {
            PANEL_HI
        } else {
            PANEL
        };
        let border = if active || hover { ACCENT_HI } else { BORDER };
        c.fill_rect(self.x, self.y, self.w, self.h, bg);
        c.rect_outline(self.x, self.y, self.w, self.h, border);
        let scale = ((self.h - 8) / 8).clamp(1, 3);
        let tw = text_w(&self.label, scale);
        c.text(
            self.x + (self.w - tw) / 2,
            self.y + (self.h - 8 * scale) / 2,
            &self.label,
            scale,
            TEXT,
        );
    }
}
