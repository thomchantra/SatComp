//! Drawing primitives on the 560x336 logical canvas (port of the JSFX Rfill/Rbox/Rtext/Rline helpers).
//! Edges are snapped to whole device pixels so borders stay uniform, like the JSFX.

use nih_plug_egui::egui::{self, Align2, Color32, FontFamily, FontId, Painter, Pos2, Shape, Stroke};

pub const W: f32 = 560.0;
pub const H: f32 = 336.0;

/// egui puts glyphs lower in the line box than REAPER's text does (JSFX `gfx_y` is close to the cap top).
/// Fraction of the font size to lift text by; tune against a JSFX screenshot.
pub const TEXT_LIFT_EM: f32 = 0.28;

/// Colour schemes (BG rgb, UI rgb, ACCENT rgb), from docs/colorscheme.md. Index 0/1 = Black/White.
pub const PALETTES: [(&str, [f32; 9]); 10] = [
    ("Black", [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0]),
    ("White", [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
    ("Teal", [0.0784, 0.2196, 0.1961, 0.6431, 1.0, 0.9647, 0.0, 1.0, 0.8980]),
    ("Cadmium", [0.3137, 0.3137, 0.3137, 1.0, 1.0, 1.0, 1.0, 0.4510, 0.0]),
    ("Ultramarine", [0.0, 0.1020, 0.5490, 1.0, 1.0, 1.0, 0.1490, 0.3059, 1.0]),
    ("Azure", [0.1686, 0.1961, 0.2353, 0.8157, 0.8588, 0.8941, 0.0, 0.8, 1.0]),
    ("Magenta", [0.3098, 0.0, 0.4314, 1.0, 0.4196, 0.8471, 1.0, 0.0, 0.6824]),
    ("Sulfur", [0.0, 0.0, 0.0, 1.0, 0.9333, 0.0, 1.0, 0.9333, 0.0]),
    ("Uranium", [0.0235, 0.2314, 0.0275, 0.7216, 0.9647, 0.5059, 0.5255, 0.9882, 0.1216]),
    ("Midnight", [0.1412, 0.0039, 0.1961, 0.9255, 0.2471, 1.0, 0.9647, 1.0, 0.0]),
];

pub fn fira() -> FontFamily {
    FontFamily::Name("fira".into())
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
}

fn rgb(c: [f32; 3]) -> Color32 {
    Color32::from_rgb((c[0] * 255.0 + 0.5) as u8, (c[1] * 255.0 + 0.5) as u8, (c[2] * 255.0 + 0.5) as u8)
}

impl<'a> Canvas<'a> {
    pub fn new(p: &'a Painter, ppp: f32, ox: f32, oy: f32, s: f32, pal: usize) -> Self {
        let a = PALETTES[pal.min(9)].1;
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
        } * self.s;
        FontId::new(sz.max(6.0), fira())
    }

    pub fn set_font(&mut self, f: u8) {
        self.font = f;
    }

    pub fn text_w(&self, s: &str) -> f32 {
        let g = self.p.layout_no_wrap(s.to_string(), self.fid(), self.col);
        g.size().x / self.s
    }

    pub fn text(&self, x: f32, y: f32, s: &str) {
        let lift = TEXT_LIFT_EM * self.fid().size;
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
