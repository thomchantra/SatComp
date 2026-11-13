//! Colour themes and the About / Appearance / Changelog pages, shared by the plug-ins.
//!
//! * `AppState`: scheme list (built-in `PALETTES` + user themes from `themes.json`), the theme-editor state and the
//!   About-page scroll state. Plug-ins keep one in their GUI state.
//! * `AppearanceUi`: a trait over `Widgets` whose provided methods draw the pages and run the editor (swatches, colour
//!   picker, on-screen keyboard). The plug-in supplies the few things that are its own: where the scheme / light flag /
//!   font are stored, the page rectangles, the About copy and the signal-flow drawing.
//!
//! Themes live in one file (`SatComp/themes.json` beside the preset folders), so a theme made in one plug-in shows in all.

use crate::canvas::{FONT_NAMES, PALETTES};
use crate::kit::Widgets;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// [BG rgb, UI rgb, ACCENT rgb] per mode, same layout as `PALETTES`.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct UserTheme {
    pub name: String,
    pub dark: [f32; 9],
    pub light: [f32; 9],
}

const SLOT_NAMES: [&str; 6] = ["DARK BG", "DARK UI", "DARK ACCENT", "LIGHT BG", "LIGHT UI", "LIGHT ACCENT"];
const SLOT_SHORT: [&str; 6] = ["DARK BG", "DARK UI", "DARK ACC", "LIGHT BG", "LIGHT UI", "LIGHT ACC"];

/// The folder that holds `themes.json`, `defaults.json` and the preset folders.
fn data_dir() -> Option<PathBuf> {
    // test / portable override
    if let Some(d) = std::env::var_os("SATCOMP_DATA_DIR") {
        return Some(PathBuf::from(d));
    }
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(any(target_os = "macos", windows)))]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"));
    base.map(|b| b.join("SatComp"))
}

fn themes_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("themes.json"))
}

fn defaults_path() -> Option<PathBuf> {
    data_dir().map(|d| d.join("defaults.json"))
}

/// The look a NEW plug-in instance starts with, shared by all the plug-ins (`SatComp/defaults.json`). A saved project keeps
/// its own look; this only seeds fresh instances. Absent file = factory look.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Defaults {
    /// scheme index (`PALETTES.len()` = a user theme, given by `theme`)
    pub pal: i32,
    /// `encode`d user theme when `pal` is a user theme, else empty
    pub theme: String,
    pub light: bool,
    pub face: i32,
    /// spectrum / scope fill (plug-ins without the switch ignore it)
    pub fill_solid: bool,
    /// window scale relative to each plug-in's base size
    pub scale: f32,
}

impl Defaults {
    /// Window size in logical pixels for a plug-in with this base size.
    pub fn size(&self, base: (f32, f32)) -> (u32, u32) {
        let s = self.scale.clamp(1.0, 2.5);
        ((base.0 * s).round() as u32, (base.1 * s).round() as u32)
    }

    /// Scheme index as stored in `palette-v2`, kept inside the table.
    pub fn pal_index(&self) -> i32 {
        self.pal.clamp(0, PALETTES.len() as i32)
    }

    pub fn face_index(&self) -> i32 {
        self.face.clamp(0, FONT_NAMES.len() as i32 - 1)
    }
}

pub fn load_defaults() -> Option<Defaults> {
    defaults_path().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok())
}

pub fn save_defaults(d: &Defaults) {
    if let Some(p) = defaults_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(d) {
            let _ = std::fs::write(p, s);
        }
    }
}

/// Back to the factory look for new instances.
pub fn clear_defaults() {
    if let Some(p) = defaults_path() {
        let _ = std::fs::remove_file(p);
    }
}

pub fn load_themes() -> Vec<UserTheme> {
    themes_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Vec<UserTheme>>(&s).ok())
        .unwrap_or_default()
}

fn save_themes(v: &[UserTheme]) {
    if let Some(p) = themes_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string_pretty(v) {
            let _ = std::fs::write(p, s);
        }
    }
}

fn hex6(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    format!("{:02X}{:02X}{:02X}", b(c[0]), b(c[1]), b(c[2]))
}

fn parse6(s: &str) -> Option<[f32; 3]> {
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([((v >> 16) & 255) as f32 / 255.0, ((v >> 8) & 255) as f32 / 255.0, (v & 255) as f32 / 255.0])
}

/// `name|` + 36 hex digits (dark BG UI ACCENT, light BG UI ACCENT)
pub fn encode(t: &UserTheme) -> String {
    let mut s = format!("{}|", t.name);
    for a in [&t.dark, &t.light] {
        for k in 0..3 {
            s.push_str(&hex6([a[k * 3], a[k * 3 + 1], a[k * 3 + 2]]));
        }
    }
    s
}

pub fn decode(s: &str) -> Option<UserTheme> {
    let (name, hex) = s.rsplit_once('|')?;
    if hex.len() != 36 || name.is_empty() {
        return None;
    }
    let mut c = [[0.0f32; 3]; 6];
    for (i, slot) in c.iter_mut().enumerate() {
        *slot = parse6(&hex[i * 6..i * 6 + 6])?;
    }
    Some(UserTheme { name: name.to_string(), dark: flat(&c[0..3]), light: flat(&c[3..6]) })
}

fn flat(c: &[[f32; 3]]) -> [f32; 9] {
    let mut a = [0.0; 9];
    for k in 0..3 {
        a[k * 3..k * 3 + 3].copy_from_slice(&c[k]);
    }
    a
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let (i, f) = (h.floor() as i32, h - h.floor());
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

pub fn rgb_to_hsv(c: [f32; 3]) -> (f32, f32, f32) {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    let d = mx - mn;
    let h = if d < 1e-6 {
        0.0
    } else if mx == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0) / 6.0
    } else if mx == c[1] {
        ((c[2] - c[0]) / d + 2.0) / 6.0
    } else {
        ((c[0] - c[1]) / d + 4.0) / 6.0
    };
    (h, if mx <= 0.0 { 0.0 } else { d / mx }, mx)
}

/// black or white, whichever reads on `c`
fn contrast(c: [f32; 3]) -> [f32; 3] {
    if 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2] > 0.5 {
        [0.0; 3]
    } else {
        [1.0; 3]
    }
}

/// Text cursor blink.
pub fn blink(t: f64) -> bool {
    ((t * 4.0).floor() as i64) & 1 == 1
}

pub struct Picker {
    slot: usize,
    old: [f32; 3],
    h: f32,
    s: f32,
    v: f32,
    /// 0 none, 1 dragging the SV square, 2 dragging the hue strip
    drag: u8,
}

/// On-screen keyboard state: 0 = theme name, 1 = hex colour.
pub struct Osk {
    kind: u8,
    text: String,
}

pub struct ThemeEdit {
    name: String,
    /// dark BG, UI, ACCENT, then light BG, UI, ACCENT
    col: [[f32; 3]; 6],
    preview_light: bool,
    /// user theme being edited (None = new)
    index: Option<usize>,
    picker: Option<Picker>,
    pub osk: Option<Osk>,
}

/// One line of the scrolling About / Changelog text. Head = version title (+ rule), takes the next Pad row too.
pub enum Row {
    Text(f32, String),
    Head(String),
    Pad,
}

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

/// Where the theme-editor cells go: a 4 x 3 grid of `cw` x `ch` cells, `px` / `py` apart, from (`x0`, `y0`).
#[derive(Clone, Copy)]
pub struct CellGrid {
    pub x0: f32,
    pub y0: f32,
    pub cw: f32,
    pub ch: f32,
    pub px: f32,
    pub py: f32,
}

/// Input captured before the theme editor masks it, restored for the editor's own widgets.
pub type RealInput = (bool, bool, bool, i32, f32, f32);

pub struct AppState {
    /// About page: 0 closed, 1 about, 2 changelog, 3 appearance
    pub about: u8,
    /// first visible row of the About / Changelog text box
    pub about_scroll: usize,
    /// first visible scheme row in the Appearance list
    pub ap_scroll: usize,
    del_arm: f64,
    /// feedback line on the Preferences page: (time it was set, text)
    pub note: (f64, &'static str),
    pub themes: Vec<UserTheme>,
    pub edit: Option<ThemeEdit>,
    pub real_in: RealInput,
}

impl AppState {
    pub fn new() -> Self {
        Self { about: 0, about_scroll: 0, ap_scroll: 0, del_arm: -10.0, note: (-10.0, ""), themes: load_themes(), edit: None, real_in: (false, false, false, 0, -1000.0, -1000.0) }
    }

    /// The egui clock restarted (editor reopened): re-base stored timestamps.
    pub fn rebase(&mut self) {
        self.del_arm = -10.0;
    }

    pub fn n_schemes(&self) -> usize {
        PALETTES.len() + self.themes.len()
    }

    pub fn scheme_name(&self, i: usize) -> String {
        match i.checked_sub(PALETTES.len()) {
            Some(u) => self.themes.get(u).map(|t| t.name.clone()).unwrap_or_default(),
            None => PALETTES[i].0.to_string(),
        }
    }

    pub fn scheme_cols(&self, i: usize, light: bool) -> [f32; 9] {
        match i.checked_sub(PALETTES.len()) {
            Some(u) => self.themes.get(u).map(|t| if light { t.light } else { t.dark }).unwrap_or(PALETTES[0].1),
            None => {
                let e = &PALETTES[i.min(PALETTES.len() - 1)];
                if light { e.2 } else { e.1 }
            }
        }
    }

    /// Colours the canvas draws with: the editor preview while editing, else the active scheme.
    pub fn colors(&self, pal: usize, light: bool) -> [f32; 9] {
        match &self.edit {
            Some(e) => {
                let o = if e.preview_light { 3 } else { 0 };
                flat(&e.col[o..o + 3])
            }
            None => self.scheme_cols(pal, light),
        }
    }

    /// A project that used a user theme: find it in the themes file by name, or restore it from the project copy.
    /// `saved` is the project's `custom-theme` string. Returns the scheme index to use.
    pub fn restore_custom(&mut self, pal: usize, saved: &str) -> usize {
        if pal < PALETTES.len() {
            return pal;
        }
        match decode(saved) {
            Some(t) => match self.themes.iter().position(|u| u.name == t.name) {
                Some(i) => PALETTES.len() + i,
                None => {
                    self.themes.push(t);
                    PALETTES.len() + self.themes.len() - 1
                }
            },
            None => 0,
        }
    }

    pub fn editing(&self) -> bool {
        self.edit.is_some()
    }

    pub fn osk_open(&self) -> bool {
        self.edit.as_ref().map(|e| e.osk.is_some()).unwrap_or(false)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything the pages need from the plug-in. Implemented on the plug-in's per-frame GUI type next to `Widgets`.
pub trait AppearanceUi<'c>: Widgets<'c> {
    fn app(&mut self) -> &mut AppState;
    fn app_ref(&self) -> &AppState;
    fn pal(&self) -> usize;
    fn light(&self) -> bool;
    fn face(&self) -> usize;
    /// Store the scheme (state, `palette-v2` and, for a user theme, the project copy `encoded`), then re-colour the canvas.
    fn commit_scheme(&mut self, pal: usize, encoded: String);
    fn commit_light(&mut self, light: bool);
    fn commit_face(&mut self, face: usize);
    /// Current window scale (1.0 = the plug-in's base size) and a request to change it.
    fn scale_now(&self) -> f32;
    fn set_scale(&mut self, scale: f32);
    /// The scale a fresh instance opens at when no defaults file exists.
    fn factory_scale(&self) -> f32;
    /// Factory spectrum / scope fill (for plug-ins that have the switch).
    fn factory_fill_solid(&self) -> bool {
        false
    }
    /// Spectrum fill style (`Some(solid)`), for plug-ins that draw a spectrum; `None` hides the switch on the Appearance page.
    fn fft_solid(&self) -> Option<bool> {
        None
    }
    fn commit_fft_solid(&mut self, _solid: bool) {}
    /// Label of that switch (plug-ins that draw a scope rather than a spectrum can say so).
    fn fill_label(&self) -> &'static str {
        "SPECTRUM FILL"
    }

    /// About page copy: paragraphs of source lines.
    fn about_paras(&self) -> &'static [&'static [&'static str]];
    /// Changelog, newest first.
    fn changelog(&self) -> &'static [(&'static str, &'static [&'static str])];
    /// The signal-flow diagram at the top of the About tab (height `flow_h`).
    fn draw_flow(&mut self, r: Rect, byp: bool);
    fn flow_h(&self) -> f32 {
        114.0
    }

    // ------------------------------------------------------------------ small helpers
    fn set_pal(&mut self, p: usize) {
        let p = p.min(self.app_ref().n_schemes() - 1);
        let enc = match p.checked_sub(PALETTES.len()) {
            Some(i) => encode(&self.app_ref().themes[i]),
            None => String::new(),
        };
        self.commit_scheme(p, enc);
    }

    fn set_light(&mut self, l: bool) {
        self.commit_light(l);
    }

    /// Save this instance's look (scheme, mode, font, fill, window scale) as the default for new instances of every plug-in.
    fn save_as_default(&mut self) {
        let pal = self.pal();
        let user = pal >= PALETTES.len();
        let theme = if user { self.app_ref().themes.get(pal - PALETTES.len()).map(encode).unwrap_or_default() } else { String::new() };
        let d = Defaults {
            pal: if user && theme.is_empty() { 0 } else { pal.min(PALETTES.len()) as i32 },
            theme,
            light: self.light(),
            face: self.face() as i32,
            fill_solid: self.fft_solid().unwrap_or_else(|| self.factory_fill_solid()),
            scale: self.scale_now(),
        };
        save_defaults(&d);
        let now = self.kit_ref().now;
        self.app().note = (now, "Saved as the default for new instances");
    }

    /// Clear the defaults file and put this instance back on the factory look too.
    fn reset_to_factory(&mut self) {
        clear_defaults();
        self.set_pal(0);
        self.set_light(false);
        self.commit_face(0);
        if self.fft_solid().is_some() {
            let f = self.factory_fill_solid();
            self.commit_fft_solid(f);
        }
        let f = self.factory_scale();
        self.set_scale(f);
        let now = self.kit_ref().now;
        self.app().note = (now, "Back to the factory look");
    }

    fn mask_in(&mut self) {
        let k = self.kit();
        k.press = false;
        k.down = false;
        k.rpress = false;
        k.wheel = 0;
        k.mx = -1000.0;
        k.my = -1000.0;
    }

    fn unmask_in(&mut self) {
        let r = self.app_ref().real_in;
        let k = self.kit();
        k.press = r.0;
        k.down = r.1;
        k.rpress = r.2;
        k.wheel = r.3;
        k.mx = r.4;
        k.my = r.5;
    }

    /// Call at the start of every frame: while the editor is open the real input is saved and masked.
    fn begin_frame_editing(&mut self) {
        let k = self.kit_ref();
        let r = (k.press, k.down, k.rpress, k.wheel, k.mx, k.my);
        self.app().real_in = r;
        if self.app_ref().editing() {
            self.mask_in();
            self.kit().act = 0;
            self.app().about = 0;
        }
    }

    fn wrap(&mut self, s: &str, w: f32) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = String::new();
        for word in s.split(' ') {
            let t = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if !cur.is_empty() && self.cv().text_w(&t) > w {
                out.push(std::mem::take(&mut cur));
                cur = word.to_string();
            } else {
                cur = t;
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    }

    // ------------------------------------------------------------------ pages
    /// About / Appearance / Changelog inside `r` (tab row 22 units above it).
    fn draw_about(&mut self, r: Rect, byp: bool) {
        let ty = r.y - 22.0;
        // a narrow page (SatLim) fits three abutting tabs and a single cross icon, with no spacing; wider pages keep the labelled button
        let narrow = r.w < 360.0;
        let tw = ((r.w - 16.0) / 3.0).floor();
        for (i, (l, id)) in [("ABOUT", 1u8), ("PREFERENCES", 3u8), ("CHANGELOG", 2u8)].into_iter().enumerate() {
            let (dx, w) = if narrow { (i as f32 * tw, tw + 1.0) } else { (i as f32 * 115.0, 114.0) };
            if self.btn(r.x + dx, ty, w, 16.0, l, (self.app_ref().about == id) as u8) {
                let pal = self.pal();
                let a = self.app();
                a.about = id;
                a.about_scroll = 0;
                if id == 3 {
                    a.ap_scroll = pal.saturating_sub(2);
                }
            }
        }
        let (cx0, cw0) = if narrow { (r.x + 3.0 * tw, r.w - 3.0 * tw) } else { (r.x + r.w - 62.0, 62.0) };
        if self.btn(cx0, ty, cw0, 16.0, if narrow { "" } else { "X CLOSE" }, 0) {
            let a = self.app();
            a.about = 0;
            a.about_scroll = 0;
        }
        if narrow {
            self.setc(false);
            let (mx, my) = (cx0 + (cw0 * 0.5).floor(), ty + 8.0);
            self.cv().line(mx - 3.0, my - 3.0, mx + 3.0, my + 3.0);
            self.cv().line(mx - 3.0, my + 3.0, mx + 3.0, my - 3.0);
        }
        self.setc(false);
        self.cv().set_font(1);
        let page = self.app_ref().about;
        if page == 3 {
            self.draw_appearance(r);
            return;
        }
        if page == 2 {
            self.cv().boxr(r.x, r.y, r.w, r.h);
            let mut rows = Vec::new();
            for (ver, lines) in self.changelog() {
                rows.push(Row::Head(ver.to_string()));
                rows.push(Row::Pad);
                for l in *lines {
                    for (i, t) in self.wrap(l, r.w - 30.0).into_iter().enumerate() {
                        rows.push(Row::Text(if i == 0 { 0.0 } else { 10.0 }, if i == 0 { format!("- {t}") } else { t }));
                    }
                }
                rows.push(Row::Pad);
            }
            self.scroll_rows(r, &rows);
            return;
        }
        let fh = self.flow_h();
        self.draw_flow(Rect::new(r.x, r.y, r.w, fh), byp);
        self.setc(false);
        let (by, bh) = (r.y + fh + 6.0, r.h - fh - 6.0);
        self.cv().boxr(r.x, by, r.w, bh);
        let mut rows = Vec::new();
        for para in self.about_paras() {
            for l in *para {
                for t in self.wrap(l, r.w - 30.0) {
                    rows.push(Row::Text(0.0, t));
                }
            }
            rows.push(Row::Pad);
        }
        self.scroll_rows(Rect::new(r.x, by, r.w, bh - 17.0), &rows);
        self.setc(false);
        let face = self.face();
        self.cv().text(r.x + 6.0, r.y + r.h - 17.0, &format!("Font: {} (SIL OFL 1.1)", FONT_NAMES[face]));
        self.cv().textr(r.x, r.y + r.h - 17.0, r.w - 6.0, "thomchantra");
    }

    /// Appearance tab: colour mode switch, scrollable scheme list with swatches, font choice. Everything applies at once.
    fn draw_appearance(&mut self, r: Rect) {
        const RH: f32 = 20.0;
        // a narrow page puts the fill switch on a row of its own under COLOR MODE (everything below moves down)
        let narrow = r.w < 460.0 && self.fft_solid().is_some();
        let hx = if narrow { 22.0 } else { 0.0 };
        let rows = ((r.h - 266.0 - hx) / RH).floor().max(0.0) as usize + 5;
        let extra = (rows - 5) as f32 * RH;
        let (lx, ly, lw) = (r.x + 8.0, r.y + 42.0 + hx, r.w - 16.0);
        self.cv().boxr(r.x, r.y, r.w, r.h);
        self.setc(false);
        self.cv().set_font(1);
        self.cv().text(lx, r.y + 8.0, "COLOR MODE");
        let light = self.light();
        for (dx, l, lt) in [(100.0, "DARK", false), (166.0, "LIGHT", true)] {
            if self.btn(r.x + dx, r.y + 4.0, 64.0, 16.0, l, (light == lt) as u8) {
                self.set_light(lt);
            }
        }
        if let Some(solid) = self.fft_solid() {
            self.setc(false);
            let (ly0, lx0, bx0) = if narrow { (r.y + 26.0, lx, r.x + 100.0) } else { (r.y + 4.0, r.x + 240.0, r.x + 330.0) };
            let label = self.fill_label();
            self.cv().text(lx0, ly0 + 4.0, label);
            for (dx, l, sd) in [(0.0, "OUTLINE", false), (74.0, "SOLID", true)] {
                if self.btn(bx0 + dx, ly0, if sd { 56.0 } else { 72.0 }, 16.0, l, (solid == sd) as u8) {
                    self.commit_fft_solid(sd);
                }
            }
        }

        // ---- scheme list
        self.setc(false);
        self.cv().text(lx, r.y + 30.0 + hx, "COLOR SCHEME");
        let pal = self.pal();
        let user_sel = pal >= PALETTES.len();
        let bx = r.x + r.w - 170.0;
        if self.btn(bx, r.y + 26.0 + hx, 82.0, 14.0, "NEW THEME", 0) {
            self.begin_edit(None);
            return;
        }
        let prev_dk = self.kit_ref().dk;
        if !user_sel {
            self.set_dk(0.5); // EDIT / DELETE only apply to a user theme
        }
        let now = self.kit_ref().now;
        let armed = now - self.app_ref().del_arm < 3.0;
        let edit_hit = self.btn(bx + 84.0, r.y + 26.0 + hx, 38.0, 14.0, "EDIT", 0);
        let del_hit = self.btn(bx + 124.0, r.y + 26.0 + hx, 38.0, 14.0, if armed && user_sel { "SURE?" } else { "DEL" }, (armed && user_sel) as u8 * 2);
        self.set_dk(prev_dk);
        if user_sel {
            if edit_hit {
                self.begin_edit(Some(pal - PALETTES.len()));
                return;
            }
            if del_hit {
                if armed {
                    self.delete_user(pal - PALETTES.len());
                    self.app().del_arm = -10.0;
                } else {
                    self.app().del_arm = now;
                }
            }
        }
        let lh = rows as f32 * RH + 2.0;
        let n = self.app_ref().n_schemes();
        let max = n.saturating_sub(rows);
        let wheel = self.kit_ref().wheel;
        if self.hit(lx, ly, lw, lh) && wheel != 0 {
            let a = self.app();
            a.ap_scroll = (a.ap_scroll as i32 - wheel).clamp(0, max as i32) as usize;
        }
        let (tx, ty, th) = (lx + lw - 9.0, ly + 3.0, lh - 6.0);
        let thumb = (th * rows as f32 / n as f32).floor().max(14.0).min(th);
        if self.hit(tx - 3.0, ty, 11.0, th) && self.kit_ref().press && self.kit_ref().act == 0 {
            let k = self.kit();
            k.act = 141;
            k.press = false;
        }
        if self.kit_ref().act == 141 && self.kit_ref().down {
            let f = ((self.kit_ref().my - ty - thumb * 0.5) / (th - thumb).max(1.0)).clamp(0.0, 1.0);
            self.app().ap_scroll = (f * max as f32 + 0.5).floor() as usize;
        }
        let a = self.app();
        a.ap_scroll = a.ap_scroll.min(max);
        let scroll = a.ap_scroll;
        self.setc(false);
        self.cv().boxr(lx, ly, lw, lh);
        let rw = lw - 2.0 - 11.0;
        for i in 0..rows.min(n) {
            let idx = scroll + i;
            let y = ly + 1.0 + i as f32 * RH;
            let sel = idx == self.pal(); // re-read: a click earlier in this list changes it for the rows below
            if self.hit(lx + 1.0, y, rw, RH) && self.kit_ref().press && self.kit_ref().act == 0 {
                self.set_pal(idx);
                self.kit().press = false;
            }
            if sel {
                self.setc(false);
                self.cv().fill(lx + 1.0, y, rw, RH);
                self.cv().setbg();
            } else {
                self.setc(false);
            }
            let nm = self.app_ref().scheme_name(idx);
            self.cv().text(lx + 9.0, y + 6.0, &nm);
            let a = self.app_ref().scheme_cols(idx, light);
            for k in 0..3 {
                let (cx, cy) = (lx + rw - 3.0 - (3 - k) as f32 * 22.0 + 4.0, y + 3.0);
                if sel {
                    self.cv().setbg();
                } else {
                    self.setc(false);
                }
                self.cv().fill(cx, cy, 20.0, RH - 6.0);
                self.cv().setrgb([a[k * 3], a[k * 3 + 1], a[k * 3 + 2]]);
                self.cv().fill(cx + 1.0, cy + 1.0, 18.0, RH - 8.0);
            }
        }
        self.setc(false);
        self.cv().fill(tx + 3.0, ty, 1.0, th); // track
        let top = ty + (th - thumb) * scroll as f32 / max.max(1) as f32;
        let act141 = self.kit_ref().act == 141;
        self.setc(act141);
        self.cv().fill(tx + 1.0, top.floor(), 5.0, thumb);
        self.setc(false);

        // ---- font
        let fy = r.y + 152.0 + extra + hx;
        self.cv().text(lx, fy, "FONT");
        let face = self.face();
        let fw = ((lw - 8.0) * 0.5).floor();
        for (i, name) in FONT_NAMES.iter().enumerate() {
            let (x, y, w, h) = (lx + (i % 2) as f32 * (fw + 8.0), fy + 12.0 + (i / 2) as f32 * 24.0, fw, 20.0);
            let on = face == i;
            if self.hit(x, y, w, h) && self.kit_ref().press && self.kit_ref().act == 0 {
                self.commit_face(i.min(FONT_NAMES.len() - 1));
                self.kit().press = false;
            }
            self.setc(false);
            if on {
                self.cv().fill(x, y, w, h);
                self.cv().setbg();
            } else {
                self.cv().boxr(x, y, w, h);
            }
            let keep = self.cv().face;
            self.cv().face = i; // each name is drawn in its own typeface
            self.cv().textc(x, y + 6.0, w, name);
            self.cv().face = keep;
        }
        self.setc(false);
        let bw = ((lw - 8.0) * 0.5).floor();
        let by = fy + 86.0;
        if self.btn(lx, by, bw, 14.0, "SET AS DEFAULT", 0) {
            self.save_as_default();
        }
        if self.btn(lx + bw + 8.0, by, bw, 14.0, "RESET TO FACTORY", 0) {
            self.reset_to_factory();
        }
        self.setc(false);
        let (t0, note) = self.app_ref().note;
        if self.kit_ref().now - t0 < 4.0 {
            self.cv().text(lx, by + 17.0, note);
        } else {
            self.cv().text(lx, by + 17.0, "Saved with the project. Fonts: SIL OFL 1.1");
        }
    }

    /// Text rows (11 units each) in a box whose frame the caller has drawn. Mouse wheel scrolls 3 rows per notch; when the
    /// text overflows a thumb on the right edge shows the position and can be dragged. Rows are culled whole (no clipping).
    fn scroll_rows(&mut self, r: Rect, rows: &[Row]) {
        let Rect { x, y, w, h } = r;
        let vis = (((h - 10.0) / 11.0).floor() as usize).max(1);
        let max = rows.len().saturating_sub(vis);
        let wheel = self.kit_ref().wheel;
        if self.hit(x, y, w, h) && wheel != 0 {
            let a = self.app();
            a.about_scroll = (a.about_scroll as i32 - wheel * 3).clamp(0, max as i32) as usize;
        }
        let (tx, ty, th) = (x + w - 8.0, y + 4.0, h - 8.0);
        let thumb = (th * vis as f32 / rows.len().max(1) as f32).floor().max(14.0).min(th);
        if max > 0 {
            if self.hit(tx - 3.0, ty, 10.0, th) && self.kit_ref().press && self.kit_ref().act == 0 {
                let k = self.kit();
                k.act = 140;
                k.press = false;
                self.about_scroll_drag_start();
            }
            if self.kit_ref().act == 140 && self.kit_ref().down {
                let f = ((self.kit_ref().my - ty - thumb * 0.5) / (th - thumb).max(1.0)).clamp(0.0, 1.0);
                self.app().about_scroll = (f * max as f32 + 0.5).floor() as usize;
            }
        }
        let a = self.app();
        a.about_scroll = a.about_scroll.min(max);
        let scroll = a.about_scroll;
        self.setc(false);
        for (i, row) in rows.iter().skip(scroll).take(vis).enumerate() {
            let yy = y + 7.0 + i as f32 * 11.0;
            match row {
                Row::Text(ind, t) => self.cv().text(x + 8.0 + ind, yy, t),
                Row::Head(t) => {
                    self.cv().text(x + 8.0, yy, t);
                    if i + 1 < vis {
                        self.cv().fill(x + 8.0, yy + 13.0, w - 16.0 - if max > 0 { 8.0 } else { 0.0 }, 1.0);
                    }
                }
                Row::Pad => {}
            }
        }
        if max > 0 {
            self.cv().fill(tx + 3.0, ty, 1.0, th); // track
            let top = ty + (th - thumb) * scroll as f32 / max as f32;
            let act140 = self.kit_ref().act == 140;
            self.setc(act140);
            self.cv().fill(tx + 1.0, top.floor(), 5.0, thumb);
            self.setc(false);
        }
    }

    /// Hook: a scrollbar drag started (plug-ins that track other viewport drags clear them here).
    fn about_scroll_drag_start(&mut self) {}

    // ------------------------------------------------------------------ theme editor
    fn begin_edit(&mut self, index: Option<usize>) {
        let (pal, light) = (self.pal(), self.light());
        let (name, d, l) = match index.and_then(|i| self.app_ref().themes.get(i)) {
            Some(t) => (t.name.clone(), t.dark, t.light),
            None => (String::new(), self.app_ref().scheme_cols(pal, false), self.app_ref().scheme_cols(pal, true)),
        };
        let tri = |a: [f32; 9], k: usize| [a[k * 3], a[k * 3 + 1], a[k * 3 + 2]];
        let a = self.app();
        a.edit = Some(ThemeEdit {
            name,
            col: [tri(d, 0), tri(d, 1), tri(d, 2), tri(l, 0), tri(l, 1), tri(l, 2)],
            preview_light: light,
            index,
            picker: None,
            osk: None,
        });
        a.about = 0;
    }

    fn delete_user(&mut self, i: usize) {
        if i >= self.app_ref().themes.len() {
            return;
        }
        let cur = self.pal();
        let a = self.app();
        a.themes.remove(i);
        save_themes(&a.themes);
        let gone = PALETTES.len() + i;
        self.set_pal(if cur == gone { 0 } else if cur > gone { cur - 1 } else { cur });
    }

    fn save_edit(&mut self) {
        let Some(e) = self.app().edit.take() else { return };
        let mut name = e.name.trim().to_string();
        if name.is_empty() {
            name = format!("Custom {}", self.app_ref().themes.len() + 1);
        }
        let taken = |a: &AppState, n: &str, skip: Option<usize>| {
            PALETTES.iter().any(|p| p.0.eq_ignore_ascii_case(n))
                || a.themes.iter().enumerate().any(|(i, u)| Some(i) != skip && u.name.eq_ignore_ascii_case(n))
        };
        let base = name.clone();
        let mut k = 2;
        while taken(self.app_ref(), &name, e.index) {
            name = format!("{base} {k}");
            k += 1;
        }
        let t = UserTheme { name, dark: flat(&e.col[0..3]), light: flat(&e.col[3..6]) };
        let a = self.app();
        let idx = match e.index.filter(|&i| i < a.themes.len()) {
            Some(i) => {
                a.themes[i] = t;
                i
            }
            None => {
                a.themes.push(t);
                a.themes.len() - 1
            }
        };
        save_themes(&a.themes);
        self.set_pal(PALETTES.len() + idx);
        let a = self.app();
        a.about = 3;
        a.ap_scroll = (PALETTES.len() + idx).saturating_sub(4);
    }

    fn cancel_edit(&mut self) {
        let a = self.app();
        a.edit = None;
        a.about = 3;
    }

    /// One editor cell: label top left, centred value. Returns true when clicked.
    fn tc_box(&mut self, x: f32, y: f32, w: f32, h: f32, lbl: &str, val: &str, lit: bool, hot: bool) -> bool {
        let clicked = self.hit(x, y, w, h) && self.kit_ref().press;
        self.setc(hot);
        if lit {
            self.cv().fill(x, y, w, h);
            self.cv().setbg();
        } else {
            self.cv().boxr(x, y, w, h);
        }
        self.cv().set_font(1);
        self.cv().text(x + 3.0, y + 3.0, lbl);
        self.cv().set_font(2);
        self.cv().textc(x, y + (h * 0.4 + 1.0).floor(), w, val);
        self.setc(false);
        clicked
    }

    /// Large check mark (save) or X (cancel) button.
    fn tc_icon(&mut self, x: f32, y: f32, w: f32, h: f32, check: bool, lit: bool) -> bool {
        let clicked = self.hit(x, y, w, h) && self.kit_ref().press;
        self.setc(false);
        if lit {
            self.cv().fill(x, y, w, h);
            self.cv().setbg();
        } else {
            self.cv().boxr(x, y, w, h);
        }
        let (cx, cy) = ((x + w * 0.5).floor(), (y + h * 0.5).floor());
        for d in 0..4 {
            let o = d as f32 - 1.5;
            if check {
                self.cv().line(cx - 11.0 + o, cy + 1.0, cx - 4.0 + o, cy + 8.0);
                self.cv().line(cx - 4.0 + o, cy + 8.0, cx + 11.0 + o, cy - 9.0);
            } else {
                self.cv().line(cx - 9.0 + o, cy - 9.0, cx + 9.0 + o, cy + 9.0);
                self.cv().line(cx - 9.0 + o, cy + 9.0, cx + 9.0 + o, cy - 9.0);
            }
        }
        self.setc(false);
        clicked
    }

    fn tc_swatch(&mut self, x: f32, y: f32, w: f32, h: f32, slot: usize, interactive: bool) {
        let Some(e) = &self.app_ref().edit else { return };
        let col = e.col[slot];
        let hot = e.picker.as_ref().map(|p| p.slot == slot).unwrap_or(false);
        let clicked = interactive && self.hit(x, y, w, h) && self.kit_ref().press;
        self.setc(hot);
        self.cv().boxr(x, y, w, h);
        self.cv().set_font(1);
        self.cv().text(x + 3.0, y + 3.0, SLOT_SHORT[slot]);
        self.setc(false);
        self.cv().boxr(x + 3.0, y + 15.0, w - 6.0, h - 18.0);
        self.cv().setrgb(col);
        self.cv().fill(x + 4.0, y + 16.0, w - 8.0, h - 20.0);
        self.cv().setrgb(contrast(col));
        self.cv().textc(x + 3.0, y + 20.0, w - 6.0, &format!("#{}", hex6(col)));
        self.setc(false);
        if clicked {
            self.kit().press = false;
            let (h_, s_, v_) = rgb_to_hsv(col);
            if let Some(e) = &mut self.app().edit {
                e.preview_light = slot >= 3;
                e.picker = Some(Picker { slot, old: col, h: h_, s: s_, v: v_, drag: 0 });
            }
        }
    }

    /// The editor's cell group on the dummy page: theme name, dark and light swatches, preview switch, save / cancel.
    fn theme_cells(&mut self, g: CellGrid) {
        let interactive = self.app_ref().edit.as_ref().map(|e| e.picker.is_none() && e.osk.is_none()).unwrap_or(false);
        let keep = self.app_ref().real_in;
        if interactive {
            self.unmask_in();
        }
        let xs = [g.x0, g.x0 + g.px, g.x0 + 2.0 * g.px, g.x0 + 3.0 * g.px];
        let ys = [g.y0, g.y0 + g.py, g.y0 + 2.0 * g.py];
        for k in 0..3 {
            self.tc_swatch(xs[k], ys[0], g.cw, g.ch, k, interactive);
            self.tc_swatch(xs[k], ys[1], g.cw, g.ch, 3 + k, interactive);
        }
        let Some(e) = &self.app_ref().edit else { return };
        let pl = e.preview_light;
        let name = e.name.clone();

        if self.tc_box(xs[3], ys[0], g.cw, g.ch, "PREVIEW", if pl { "LIGHT" } else { "DARK" }, false, false) && interactive {
            self.kit().press = false;
            if let Some(e) = &mut self.app().edit {
                e.preview_light = !e.preview_light;
            }
        }
        if self.tc_box(xs[3], ys[1], g.cw, g.ch, "AUTO", "LIGHT", false, false) && interactive {
            self.kit().press = false;
            if let Some(e) = &mut self.app().edit {
                // light = dark with BG and UI swapped, same accent
                e.col[3] = e.col[1];
                e.col[4] = e.col[0];
                e.col[5] = e.col[2];
            }
        }

        // name field (two cells wide)
        let shown = if name.is_empty() { "Custom".to_string() } else { name.clone() };
        if self.tc_box(xs[0], ys[2], g.px + g.cw, g.ch, "THEME NAME", &shown, false, false) && interactive {
            self.kit().press = false;
            if let Some(e) = &mut self.app().edit {
                e.osk = Some(Osk { kind: 0, text: name.clone() });
            }
        }
        if self.tc_icon(xs[2], ys[2], g.cw, g.ch, true, true) && interactive {
            self.kit().press = false;
            self.save_edit();
        } else if self.app_ref().edit.is_some() && self.tc_icon(xs[3], ys[2], g.cw, g.ch, false, false) && interactive {
            self.kit().press = false;
            self.cancel_edit();
        }
        if interactive {
            self.app().real_in = keep;
            self.mask_in();
        }
    }

    /// On-screen keyboard over `r` (REAPER swallows typing in plug-in windows as hotkeys). Capital letters and digits only;
    /// in hex mode just 0-9 and A-F are enabled.
    fn theme_osk(&mut self, r: Rect) {
        let Some(e) = &self.app_ref().edit else { return };
        let Some(o) = &e.osk else { return };
        let (kind, mut text) = (o.kind, o.text.clone());
        let Rect { x, y, w, h } = r;
        let max = if kind == 0 { 16 } else { 6 };
        self.setc(false);
        self.cv().setbg();
        self.cv().fill(x, y, w, h);
        self.setc(false);
        self.cv().boxr(x, y, w, h);
        self.cv().set_font(1);
        self.cv().text(x + 8.0, y + 5.0, if kind == 0 { "THEME NAME" } else { "HEX COLOUR" });
        self.cv().boxr(x + 8.0, y + 16.0, w - 16.0, 14.0);
        let caret = if blink(self.kit_ref().now) { "_" } else { " " };
        let shown = if kind == 1 { format!("#{text}{caret}") } else { format!("{text}{caret}") };
        self.cv().text(x + 12.0, y + 19.0, &shown);
        let (pitch, kh) = (26.0, 15.0);
        let x0 = x + ((w - (10.0 * pitch - 2.0)) * 0.5).floor();
        let prev_dk = self.kit_ref().dk;
        let mut close = false;
        let mut accept = false;
        let rows = ["1234567890", "QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"];
        let offs = [0.0, 0.0, 0.5, 1.0];
        for (ri, row) in rows.iter().enumerate() {
            let ky = y + 34.0 + ri as f32 * 17.0;
            for (i, c) in row.chars().enumerate() {
                let kx = x0 + (offs[ri] + i as f32) * pitch;
                let ok = kind == 0 || c.is_ascii_hexdigit();
                self.set_dk(if ok { prev_dk } else { 0.35 });
                let s = c.to_string();
                if self.btn(kx, ky, pitch - 2.0, kh, &s, 0) && ok && text.chars().count() < max {
                    text.push(c);
                }
            }
        }
        self.set_dk(prev_dk);
        let ky = y + 34.0 + 4.0 * 17.0;
        if self.btn(x0 + 8.0 * pitch, y + 34.0 + 3.0 * 17.0, 2.0 * pitch - 2.0, kh, "DEL", 0) {
            text.pop();
        }
        self.set_dk(if kind == 0 { prev_dk } else { 0.35 });
        if self.btn(x0, ky, 4.0 * pitch - 2.0, kh, "SPACE", 0) && kind == 0 && text.chars().count() < max {
            text.push(' ');
        }
        self.set_dk(prev_dk);
        if self.btn(x0 + 4.0 * pitch, ky, 3.0 * pitch - 2.0, kh, "OK", 1) {
            accept = true;
        }
        if self.btn(x0 + 7.0 * pitch, ky, 3.0 * pitch - 2.0, kh, "CANCEL", 0) {
            close = true;
        }
        if accept {
            if kind == 0 {
                if let Some(e) = &mut self.app().edit {
                    e.name = text.trim().to_string();
                    e.osk = None;
                }
                return;
            }
            if let (Some(c), Some(e)) = (parse6(&text), self.app().edit.as_mut()) {
                if let Some(p) = e.picker.as_mut() {
                    let (h_, s_, v_) = rgb_to_hsv(c);
                    p.h = if s_ > 0.0 { h_ } else { p.h };
                    p.s = s_;
                    p.v = v_;
                }
                e.osk = None;
            }
            return;
        }
        if let Some(e) = &mut self.app().edit {
            if close {
                e.osk = None;
            } else if let Some(o) = &mut e.osk {
                o.text = text;
            }
        }
    }

    /// Colour picker overlay over `r` (266 x 128): saturation / value square, hue strip, hex field, OK / CANCEL. Edits the
    /// slot live, so the whole dummy page re-colours while you drag.
    fn theme_picker(&mut self, r: Rect) {
        let Some(e) = &self.app_ref().edit else { return };
        let Some(p) = &e.picker else { return };
        let slot = p.slot;
        let Rect { x, y, w, h } = r;
        self.setc(false);
        self.cv().setbg();
        self.cv().fill(x, y, w, h);
        self.setc(false);
        self.cv().boxr(x, y, w, h);
        self.cv().set_font(1);
        self.cv().text(x + 8.0, y + 6.0, &format!("{}  (live preview)", SLOT_NAMES[slot]));

        let (sx, sy, sw, sh_) = (x + 8.0, y + 20.0, 120.0, 100.0);
        let (hx, hw) = (sx + sw + 8.0, 14.0);
        let (mx, my, down, press) = (self.kit_ref().mx, self.kit_ref().my, self.kit_ref().down, self.kit_ref().press);
        let mut close_ok = false;
        let mut close_cancel = false;
        let hit = |ax: f32, ay: f32, aw: f32, ah: f32| mx >= ax && mx < ax + aw && my >= ay && my < ay + ah;
        let mut ask_hex = false;

        let Some(e) = &mut self.app().edit else { return };
        let Some(p) = &mut e.picker else { return };
        if press {
            if hit(sx, sy, sw, sh_) {
                p.drag = 1;
            } else if hit(hx, sy, hw, sh_) {
                p.drag = 2;
            } else if hit(hx + hw + 8.0, sy + 42.0, 106.0, 16.0) {
                ask_hex = true;
            }
        }
        if !down {
            p.drag = 0;
        }
        if p.drag == 1 {
            p.s = ((mx - sx) / (sw - 1.0)).clamp(0.0, 1.0);
            p.v = (1.0 - (my - sy) / (sh_ - 1.0)).clamp(0.0, 1.0);
        } else if p.drag == 2 {
            p.h = ((my - sy) / (sh_ - 1.0)).clamp(0.0, 1.0);
        }
        let cur = hsv_to_rgb(p.h, p.s, p.v);
        let ph = p.h;
        let (ps, pv) = (p.s, p.v);
        let old = p.old;
        let hexshow = hex6(cur);
        e.col[slot] = cur;

        // SV square (4 px cells) and hue strip
        let cell = 4.0;
        for iy in 0..(sh_ / cell) as i32 {
            for ix in 0..(sw / cell) as i32 {
                let s_ = (ix as f32 + 0.5) * cell / sw;
                let v_ = 1.0 - (iy as f32 + 0.5) * cell / sh_;
                self.cv().setrgb(hsv_to_rgb(ph, s_, v_));
                self.cv().fill(sx + ix as f32 * cell, sy + iy as f32 * cell, cell, cell);
            }
        }
        for iy in 0..(sh_ / cell) as i32 {
            self.cv().setrgb(hsv_to_rgb((iy as f32 + 0.5) * cell / sh_, 1.0, 1.0));
            self.cv().fill(hx, sy + iy as f32 * cell, hw, cell);
        }
        self.setc(false);
        self.cv().boxr(sx - 1.0, sy - 1.0, sw + 2.0, sh_ + 2.0);
        self.cv().boxr(hx - 1.0, sy - 1.0, hw + 2.0, sh_ + 2.0);
        // markers: a ring on the square, a bar on the strip (contrast against what is under them)
        let (cx, cy) = (sx + ps * (sw - 1.0), sy + (1.0 - pv) * (sh_ - 1.0));
        self.cv().setrgb(contrast(cur));
        self.cv().boxr((cx - 3.0).floor(), (cy - 3.0).floor(), 7.0, 7.0);
        self.cv().setrgb(contrast(hsv_to_rgb(ph, 1.0, 1.0)));
        self.cv().fill(hx, (sy + ph * (sh_ - 1.0)).floor() - 1.0, hw, 3.0);

        // old / new, hex, RGB, buttons
        let rx = hx + hw + 8.0;
        self.setc(false);
        self.cv().text(rx, sy, "OLD");
        self.cv().text(rx + 56.0, sy, "NEW");
        self.cv().setrgb(old);
        self.cv().fill(rx, sy + 10.0, 50.0, 22.0);
        self.cv().setrgb(cur);
        self.cv().fill(rx + 56.0, sy + 10.0, 50.0, 22.0);
        self.setc(false);
        self.cv().boxr(rx - 1.0, sy + 9.0, 52.0, 24.0);
        self.cv().boxr(rx + 55.0, sy + 9.0, 52.0, 24.0);
        self.setc(false);
        self.cv().boxr(rx, sy + 42.0, 106.0, 16.0);
        self.cv().text(rx + 4.0, sy + 46.0, &format!("#{hexshow}  EDIT"));
        self.setc(false);
        let b = |v: f32| (v * 255.0 + 0.5) as u8;
        self.cv().text(rx, sy + 66.0, &format!("R{:03} G{:03} B{:03}", b(cur[0]), b(cur[1]), b(cur[2])));
        let ok = self.btn(rx, sy + 84.0, 50.0, 16.0, "OK", 1);
        let cn = self.btn(rx + 56.0, sy + 84.0, 50.0, 16.0, "CANCEL", 0);
        if ask_hex {
            if let Some(e) = &mut self.app().edit {
                e.osk = Some(Osk { kind: 1, text: hexshow.clone() });
            }
        }
        if ok && press {
            close_ok = true;
        }
        if cn && press {
            close_cancel = true;
        }
        if close_cancel {
            if let Some(e) = &mut self.app().edit {
                e.col[slot] = old;
                e.picker = None;
            }
        } else if close_ok {
            if let Some(e) = &mut self.app().edit {
                e.picker = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_encoding_roundtrips_and_hsv_inverts() {
        let t = UserTheme { name: "Test|odd".into(), dark: [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0], light: [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0] };
        let d = decode(&encode(&t)).unwrap();
        assert_eq!(d.name, "Test|odd");
        for k in 0..9 {
            assert!((d.dark[k] - t.dark[k]).abs() < 1.0 / 255.0 && (d.light[k] - t.light[k]).abs() < 1.0 / 255.0);
        }
        for c in [[0.2, 0.5, 0.9], [1.0, 0.0, 0.0], [0.3, 0.3, 0.3]] {
            let (h, s, v) = rgb_to_hsv(c);
            let r = hsv_to_rgb(h, s, v);
            for k in 0..3 {
                assert!((r[k] - c[k]).abs() < 1e-4);
            }
        }
        assert!(decode("nobar").is_none() && decode("x|123").is_none());
    }
}
