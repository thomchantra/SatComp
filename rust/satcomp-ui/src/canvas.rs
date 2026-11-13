//! Drawing primitives on the 560x336 logical canvas (port of the JSFX Rfill/Rbox/Rtext/Rline helpers).
//! Edges are snapped to whole device pixels so borders stay uniform, like the JSFX.

use nih_plug_egui::egui::{self, Align2, Color32, FontFamily, FontId, Painter, Pos2, Shape, Stroke};

pub const W: f32 = 560.0;
pub const H: f32 = 336.0;

/// egui puts glyphs lower in the line box than REAPER's text does (JSFX `gfx_y` is close to the cap top).
/// Fraction of the font size to lift text by; tune against a JSFX screenshot.
pub const TEXT_LIFT_EM: f32 = 0.28;

/// The JSFX font sizes (12/14/18 px) render about 14% smaller in REAPER than the same em size in egui
/// (measured from side-by-side screenshots), so scale to match and keep the labels inside their cells.
pub const FONT_SCALE: f32 = 0.86;

/// Colour schemes: (name, dark [BG rgb, UI rgb, ACCENT rgb], light [same]). Generated from docs/colorscheme.md (itself from
/// docs/colorthemesV2.svg). The index is persisted (`palette-v2`): append new schemes, do not reorder.
pub const PALETTES: [(&str, [f32; 9], [f32; 9]); 20] = [
    ("Mono", [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0], [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
    ("Furnace", [0.2549, 0.0275, 0.0275, 1.0, 0.0, 0.251, 1.0, 0.4, 0.0], [1.0, 0.0, 0.251, 0.2549, 0.0275, 0.0275, 1.0, 0.4, 0.0]),
    ("Barracuda", [0.0784, 0.2196, 0.1961, 0.6431, 1.0, 0.9647, 0.0, 1.0, 0.898], [0.6431, 1.0, 0.9647, 0.0784, 0.2196, 0.1961, 0.0, 0.4745, 0.4275]),
    ("Cadmium", [0.3137, 0.3137, 0.3137, 1.0, 1.0, 1.0, 1.0, 0.451, 0.0], [1.0, 1.0, 1.0, 0.3137, 0.3137, 0.3137, 1.0, 0.451, 0.0]),
    ("Ultramarine", [0.0, 0.102, 0.549, 1.0, 1.0, 1.0, 0.149, 0.3059, 1.0], [1.0, 1.0, 1.0, 0.0, 0.102, 0.549, 0.149, 0.3059, 1.0]),
    ("Azure", [0.1686, 0.1961, 0.2353, 0.8157, 0.8588, 0.8941, 0.0, 0.8, 1.0], [0.8157, 0.8588, 0.8941, 0.1686, 0.1961, 0.2353, 0.0, 0.8, 1.0]),
    ("Magenta", [0.3098, 0.0, 0.4314, 1.0, 0.4196, 0.8471, 1.0, 0.0, 0.6824], [1.0, 0.4196, 0.8471, 0.3098, 0.0, 0.4314, 1.0, 0.0, 0.6824]),
    ("Bananas", [0.0, 0.0, 0.0, 1.0, 0.851, 0.0, 0.0, 0.5843, 1.0], [1.0, 0.851, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5843, 1.0]),
    ("Genesis", [0.1961, 0.0, 0.6588, 0.8353, 1.0, 0.0, 1.0, 0.298, 0.298], [0.8353, 1.0, 0.0, 0.298, 0.0, 1.0, 1.0, 0.298, 0.298]),
    ("Uranium", [0.0235, 0.2314, 0.0275, 0.7216, 0.9647, 0.5059, 0.5255, 0.9882, 0.1216], [0.7216, 0.9647, 0.5059, 0.0235, 0.2314, 0.0275, 0.3765, 0.7647, 0.0353]),
    ("Midnight", [0.1412, 0.0039, 0.1961, 0.9255, 0.2471, 1.0, 0.9647, 1.0, 0.0], [0.9255, 0.2471, 1.0, 0.1412, 0.0039, 0.1961, 0.9647, 1.0, 0.0]),
    ("Crimson", [0.0, 0.0, 0.0, 0.8471, 0.0, 0.0, 0.0, 1.0, 1.0], [0.8471, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0]),
    ("Cyberware", [0.0, 0.0, 0.0, 0.0, 1.0, 0.8157, 0.9176, 0.0, 1.0], [0.0, 1.0, 0.8157, 0.0, 0.0, 0.0, 0.9176, 0.0, 1.0]),
    ("Ashen", [0.1804, 0.1804, 0.1804, 0.6431, 0.6431, 0.6431, 0.8196, 0.8196, 0.8196], [0.6431, 0.6431, 0.6431, 0.1804, 0.1804, 0.1804, 0.8196, 0.8196, 0.8196]),
    ("Sailor", [0.0667, 0.1843, 0.4549, 0.7059, 0.8314, 1.0, 0.6706, 0.1961, 0.2039], [0.7059, 0.8314, 1.0, 0.0667, 0.1843, 0.4549, 0.6706, 0.1961, 0.2039]),
    ("Sonar", [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
    ("Apricot", [0.2902, 0.2392, 0.2, 1.0, 0.7961, 0.5765, 1.0, 0.451, 0.0], [1.0, 0.7961, 0.5765, 0.2902, 0.2392, 0.2, 1.0, 0.451, 0.0]),
    ("Berry", [0.4627, 0.1176, 0.2745, 1.0, 0.5961, 0.8196, 1.0, 0.298, 0.298], [1.0, 0.5961, 0.8196, 0.4627, 0.1176, 0.2745, 1.0, 0.298, 0.298]),
    ("Night Rider", [0.0392, 0.0, 0.2706, 1.0, 0.2627, 0.7529, 0.298, 0.9529, 1.0], [1.0, 0.2627, 0.7529, 0.0392, 0.0, 0.2706, 0.298, 0.9529, 1.0]),
    ("Suika", [0.6588, 0.0157, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 0.4275], [1.0, 1.0, 1.0, 0.6588, 0.0157, 0.0, 0.298, 1.0, 0.3882]),
];

/// Selectable UI fonts (index persisted as `ui-font`: append, do not reorder). All are SIL OFL 1.1.
pub const FONT_NAMES: [&str; 11] = ["Fira Code", "JetBrains Mono", "Geist Mono", "Roboto Mono", "Oxygen Mono", "Doto", "DM Mono", "Victor Mono", "Anonymous Pro", "Inconsolata", "Courier Prime"];

/// Per-typeface tuning, SAME ORDER as `FONT_NAMES`: `(size multiplier, vertical shift)`.
/// * size multiplier: 1.0 = unchanged. Fonts differ in cap height (Victor Mono is tall, Anonymous Pro small), so each is
///   scaled to land near Fira Code's cap height (about 0.71 em). Raise it if a font looks small, lower it if it looks big.
/// * vertical shift: in em of the scaled font; positive moves the text DOWN, negative UP (1 unit of the shift is the font
///   size, so 0.05 is about half a pixel at 100%).
pub const FONT_TUNE: [(f32, f32); 11] = [
    (1.00, 0.0), // Fira Code
    (0.97, 0.0), // JetBrains Mono
    (1.00, 0.0), // Geist Mono
    (1.00, 0.0), // Roboto Mono
    (0.97, 0.0), // Oxygen Mono
    (1.00, 0.0), // Doto
    (1.01, 0.0), // DM Mono
    (0.89, 0.0), // Victor Mono
    (1.11, 0.0), // Anonymous Pro
    (1.14, 0.0), // Inconsolata (small capitals, narrow: scaled up to Fira's cap height)
    (1.15, 0.0), // Courier Prime (smaller still, but wide: held at 1.15 so labels keep fitting their boxes)
];

pub fn font_family(i: usize) -> FontFamily {
    FontFamily::Name(format!("f{i}").into())
}

pub struct Canvas<'a> {
    pub p: &'a Painter,
    /// pixels_per_point
    pub ppp: f32,
    /// origin and scale: logical unit -> points
    pub ox: f32,
    pub oy: f32,
    pub s: f32,
    pub bk: [f32; 3],
    pub fg: [f32; 3],
    pub ac: [f32; 3],
    /// 1 = normal, 0.5 = dimmed (modules dimmed while Bypass is on)
    pub dk: f32,
    pub col: Color32,
    pub font: u8,
    /// index into FONT_NAMES
    pub face: usize,
}

fn rgb(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0 + 0.5) as u8, (c[1] * 255.0 + 0.5) as u8, (c[2] * 255.0 + 0.5) as u8)
}

impl<'a> Canvas<'a> {
    pub fn new(p: &'a Painter, ppp: f32, ox: f32, oy: f32, s: f32, a: [f32; 9], face: usize) -> Self {
        let mut c = Self {
            p,
            ppp,
            ox,
            oy,
            s,
            bk: [a[0], a[1], a[2]],
            fg: [a[3], a[4], a[5]],
            ac: [a[6], a[7], a[8]],
            dk: 1.0,
            col: Color32::WHITE,
            font: 1,
            face: face.min(FONT_NAMES.len() - 1),
        };
        c.setc(false);
        c
    }

    pub fn bg_color(&self) -> Color32 {
        rgb(self.bk)
    }

    /// Foreground (red = accent) at the current dim level.
    pub fn setc(&mut self, red: bool) {
        let t = if red { self.ac } else { self.fg };
        let d = self.dk;
        self.col = rgb([
            self.bk[0] + (t[0] - self.bk[0]) * d,
            self.bk[1] + (t[1] - self.bk[1]) * d,
            self.bk[2] + (t[2] - self.bk[2]) * d,
        ]);
    }

    /// Set the drawing colour from a palette entry (r, g, b in 0..1), ignoring the dim level.
    pub fn setrgb(&mut self, c: [f32; 3]) {
        self.col = rgb(c);
    }

    /// Text colour on filled (lit) widgets = background colour.
    pub fn setbg(&mut self) {
        self.col = rgb(self.bk);
    }

    fn snap(&self, v: f32) -> f32 {
        (v * self.ppp).round() / self.ppp
    }

    pub fn fill(&self, x: f32, y: f32, w: f32, h: f32) {
        let x0 = self.snap(self.ox + x * self.s);
        let y0 = self.snap(self.oy + y * self.s);
        let x1 = self.snap(self.ox + (x + w) * self.s).max(x0 + 1.0 / self.ppp);
        let y1 = self.snap(self.oy + (y + h) * self.s).max(y0 + 1.0 / self.ppp);
        self.p.rect_filled(egui::Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1)), 0.0, self.col);
    }

    pub fn boxr(&self, x: f32, y: f32, w: f32, h: f32) {
        self.fill(x, y, w, 1.0);
        self.fill(x, y + h - 1.0, w, 1.0);
        self.fill(x, y, 1.0, h);
        self.fill(x + w - 1.0, y, 1.0, h);
    }

    fn fid(&self) -> FontId {
        let sz = match self.font {
            1 => 12.0,
            2 => 14.0,
            _ => 18.0,
        } * self.s * FONT_SCALE * FONT_TUNE[self.face].0;
        FontId::new(sz.max(5.0), font_family(self.face))
    }

    pub fn set_font(&mut self, f: u8) {
        self.font = f;
    }

    pub fn text_w(&self, s: &str) -> f32 {
        let g = self.p.layout_no_wrap(s.to_string(), self.fid(), self.col);
        g.size().x / self.s
    }

    pub fn text(&self, x: f32, y: f32, s: &str) {
        let lift = (TEXT_LIFT_EM - FONT_TUNE[self.face].1) * self.fid().size;
        self.p.text(
            Pos2::new(self.snap(self.ox + x * self.s), self.snap(self.oy + y * self.s - lift)),
            Align2::LEFT_TOP,
            s,
            self.fid(),
            self.col,
        );
    }

    pub fn textc(&self, x: f32, y: f32, w: f32, s: &str) {
        let tw = self.text_w(s);
        self.text(x + (w - tw) * 0.5, y, s);
    }

    pub fn textr(&self, x: f32, y: f32, w: f32, s: &str) {
        let tw = self.text_w(s);
        self.text(x + w - tw, y, s);
    }

    pub fn line(&self, x1: f32, y1: f32, x2: f32, y2: f32) {
        let a = Pos2::new(self.ox + x1 * self.s, self.oy + y1 * self.s);
        let b = Pos2::new(self.ox + x2 * self.s, self.oy + y2 * self.s);
        self.p.line_segment([a, b], Stroke::new(self.s.max(1.0 / self.ppp), self.col));
    }

    pub fn tri(&self, a: (f32, f32), b: (f32, f32), c: (f32, f32)) {
        let pt = |p: (f32, f32)| Pos2::new(self.ox + p.0 * self.s, self.oy + p.1 * self.s);
        self.p.add(Shape::convex_polygon(vec![pt(a), pt(b), pt(c)], self.col, Stroke::NONE));
    }

    pub fn disc(&self, x: f32, y: f32, r: f32) {
        self.p.circle_filled(Pos2::new(self.ox + x * self.s, self.oy + y * self.s), r * self.s, self.col);
    }
}
