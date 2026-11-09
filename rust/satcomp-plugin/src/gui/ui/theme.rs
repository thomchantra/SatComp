//! User colour themes: storage (`themes.json` next to the Presets folder), the theme editor page (the INIT page drawn as a
//! static dummy, with the comp-settings area turned into the editor) and the colour picker overlay.

use super::*;
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

/// Native single-line text prompt (blocking: call on a worker thread). REAPER swallows typing in plug-in windows as hotkeys,
/// so text is entered in an OS dialog instead. None = cancelled / unavailable.
fn text_prompt(title: &str, msg: &str, default: &str) -> Option<String> {
    use std::process::Command;
    #[cfg(target_os = "macos")]
    let out = Command::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "tell application \"System Events\"",
            "-e",
            "activate",
            "-e",
            "set r to display dialog (item 1 of argv) default answer (item 2 of argv) with title (item 3 of argv) buttons {\"Cancel\", \"OK\"} default button \"OK\"",
            "-e",
            "end tell",
            "-e",
            "return text returned of r",
            "-e",
            "end run",
            msg,
            default,
            title,
        ])
        .output()
        .ok()?;
    #[cfg(windows)]
    let out = {
        use std::os::windows::process::CommandExt;
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "& { param($m,$t,$d) Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::InputBox($m,$t,$d) }",
                msg,
                title,
                default,
            ])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .output()
            .ok()?
    };
    #[cfg(not(any(target_os = "macos", windows)))]
    let out = Command::new("zenity").args(["--entry", "--title", title, "--text", msg, "--entry-text", default]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim_end_matches(['\r', '\n']).to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn themes_path() -> Option<PathBuf> {
    presets::user_dir().and_then(|d| d.parent().map(|p| p.join("themes.json")))
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

/// `name|` + 18 hex digits (dark BG UI ACCENT, light BG UI ACCENT)
pub fn encode(t: &UserTheme) -> String {
    let mut s = format!("{}|", t.name);
    for a in [&t.dark, &t.light] {
        for k in 0..3 {
            s.push_str(&hex6([a[k * 3], a[k * 3 + 1], a[k * 3 + 2]]));
        }
    }
    s
}

fn decode(s: &str) -> Option<UserTheme> {
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

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
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

fn rgb_to_hsv(c: [f32; 3]) -> (f32, f32, f32) {
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

pub struct Picker {
    slot: usize,
    old: [f32; 3],
    h: f32,
    s: f32,
    v: f32,
    /// 0 none, 1 dragging the SV square, 2 dragging the hue strip
    drag: u8,
}

pub struct ThemeEdit {
    name: String,
    /// dark BG, UI, ACCENT, then light BG, UI, ACCENT
    col: [[f32; 3]; 6],
    preview_light: bool,
    /// user theme being edited (None = new)
    index: Option<usize>,
    picker: Option<Picker>,
}

impl State {
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
    pub fn colors(&self) -> [f32; 9] {
        match &self.edit {
            Some(e) => {
                let o = if e.preview_light { 3 } else { 0 };
                flat(&e.col[o..o + 3])
            }
            None => self.scheme_cols(self.pal, self.light),
        }
    }

    /// A project that used a user theme: find it in the themes file by name, or restore it from the project copy.
    pub fn restore_custom(&mut self, params: &SatCompParams) {
        if self.pal < PALETTES.len() {
            return;
        }
        let saved = decode(&params.custom_theme.lock());
        self.pal = match saved {
            Some(t) => match self.themes.iter().position(|u| u.name == t.name) {
                Some(i) => PALETTES.len() + i,
                None => {
                    self.themes.push(t);
                    PALETTES.len() + self.themes.len() - 1
                }
            },
            None => 0,
        };
        params.palette_v2.store(self.pal as i32, Ordering::Relaxed);
    }

    /// Constant meter / readout values for the dummy page (set every frame, the draw code then settles on them).
    pub fn fill_dummy(&self) {
        let d = &self.dummy_shared;
        d.gr_pk.store(3.5, Ordering::Relaxed);
        d.opk[0].store(1.0, Ordering::Relaxed);
        d.opk[1].store(0.5, Ordering::Relaxed);
        d.clip.store(true, Ordering::Relaxed);
        d.vz_lv.store(0.3, Ordering::Relaxed);
        d.mn_lv.store(0.5, Ordering::Relaxed);
        d.fms.store(0.05, Ordering::Relaxed);
        d.diff_disp.store(2.0, Ordering::Relaxed);
        d.sat_db.store(2.0, Ordering::Relaxed);
        d.sat_vz.store(0.3, Ordering::Relaxed);
    }
}

impl<'a> Gui<'a> {
    pub(super) fn mask_in(&mut self) {
        self.press = false;
        self.down = false;
        self.rpress = false;
        self.wheel = 0;
        self.mx = -1000.0;
        self.my = -1000.0;
    }

    pub(super) fn unmask_in(&mut self) {
        let r = self.real_in;
        self.press = r.0;
        self.down = r.1;
        self.rpress = r.2;
        self.wheel = r.3;
        self.mx = r.4;
        self.my = r.5;
    }

    fn start_prompt(&mut self, kind: u8, msg: &str, default: &str) {
        if self.prompt.is_some() {
            return;
        }
        let slot: Arc<parking_lot::Mutex<Option<Option<String>>>> = Arc::new(parking_lot::Mutex::new(None));
        self.prompt = Some((kind, slot.clone()));
        let (msg, default) = (msg.to_string(), default.to_string());
        std::thread::spawn(move || {
            let r = text_prompt("SatComp", &msg, &default);
            *slot.lock() = Some(r);
        });
    }

    /// Apply a finished native text prompt (theme name or hex colour).
    pub(super) fn poll_prompt(&mut self) {
        let Some((kind, slot)) = self.prompt.clone() else { return };
        let Some(r) = slot.lock().take() else { return };
        self.prompt = None;
        let (Some(text), Some(e)) = (r, self.edit.as_mut()) else { return };
        if kind == 0 {
            e.name = text.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_')).take(16).collect();
        } else if let (Some(c), Some(p)) = (parse6(text.trim().trim_start_matches('#')), e.picker.as_mut()) {
            let (h, s, v) = rgb_to_hsv(c);
            p.h = if s > 0.0 { h } else { p.h };
            p.s = s;
            p.v = v;
        }
    }

    pub(super) fn begin_edit(&mut self, index: Option<usize>) {
        let (name, d, l) = match index.and_then(|i| self.themes.get(i)) {
            Some(t) => (t.name.clone(), t.dark, t.light),
            None => (String::new(), self.scheme_cols(self.pal, false), self.scheme_cols(self.pal, true)),
        };
        let tri = |a: [f32; 9], k: usize| [a[k * 3], a[k * 3 + 1], a[k * 3 + 2]];
        self.edit = Some(ThemeEdit {
            name,
            col: [tri(d, 0), tri(d, 1), tri(d, 2), tri(l, 0), tri(l, 1), tri(l, 2)],
            preview_light: self.light,
            index,
            picker: None,
        });
        self.about = 0;
    }

    pub(super) fn delete_user(&mut self, i: usize) {
        if i >= self.themes.len() {
            return;
        }
        self.themes.remove(i);
        save_themes(&self.themes);
        let cur = self.pal;
        let gone = PALETTES.len() + i;
        self.set_pal(if cur == gone { 0 } else if cur > gone { cur - 1 } else { cur });
    }

    fn save_edit(&mut self) {
        let Some(e) = self.edit.take() else { return };
        let mut name = e.name.trim().to_string();
        if name.is_empty() {
            name = format!("Custom {}", self.themes.len() + 1);
        }
        let taken = |s: &State, n: &str, skip: Option<usize>| {
            PALETTES.iter().any(|p| p.0.eq_ignore_ascii_case(n))
                || s.themes.iter().enumerate().any(|(i, u)| Some(i) != skip && u.name.eq_ignore_ascii_case(n))
        };
        let base = name.clone();
        let mut k = 2;
        while taken(self, &name, e.index) {
            name = format!("{base} {k}");
            k += 1;
        }
        let t = UserTheme { name, dark: flat(&e.col[0..3]), light: flat(&e.col[3..6]) };
        let idx = match e.index.filter(|&i| i < self.themes.len()) {
            Some(i) => {
                self.themes[i] = t;
                i
            }
            None => {
                self.themes.push(t);
                self.themes.len() - 1
            }
        };
        save_themes(&self.themes);
        self.set_pal(PALETTES.len() + idx);
        self.about = 3;
        self.ap_scroll = (PALETTES.len() + idx).saturating_sub(4);
    }

    fn cancel_edit(&mut self) {
        self.edit = None;
        self.about = 3;
    }

    /// One 62x38 editor cell: label top left, centred value. Returns true when clicked.
    fn tc_box(&mut self, x: f32, y: f32, w: f32, h: f32, lbl: &str, val: &str, lit: bool, hot: bool) -> bool {
        let clicked = self.hit(x, y, w, h) && self.press;
        self.setc(hot);
        if lit {
            self.cv.fill(x, y, w, h);
            self.cv.setbg();
        } else {
            self.cv.boxr(x, y, w, h);
        }
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, lbl);
        self.cv.set_font(2);
        self.cv.textc(x, y + (h * 0.4 + 1.0).floor(), w, val);
        self.setc(false);
        clicked
    }

    fn tc_swatch(&mut self, x: f32, y: f32, slot: usize, interactive: bool) {
        let (w, h) = (62.0, 38.0);
        let Some(e) = &self.edit else { return };
        let col = e.col[slot];
        let hot = e.picker.as_ref().map(|p| p.slot == slot).unwrap_or(false);
        let clicked = interactive && self.hit(x, y, w, h) && self.press;
        self.setc(hot);
        self.cv.boxr(x, y, w, h);
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, SLOT_SHORT[slot]);
        self.setc(false);
        self.cv.boxr(x + 3.0, y + 15.0, w - 6.0, h - 18.0);
        self.cv.setrgb(col);
        self.cv.fill(x + 4.0, y + 16.0, w - 8.0, h - 20.0);
        self.cv.setrgb(contrast(col));
        self.cv.textc(x + 3.0, y + 20.0, w - 6.0, &format!("#{}", hex6(col)));
        self.setc(false);
        if clicked {
            self.press = false;
            let (h_, s_, v_) = rgb_to_hsv(col);
            if let Some(e) = &mut self.edit {
                e.preview_light = slot >= 3;
                e.picker = Some(Picker { slot, old: col, h: h_, s: s_, v: v_, drag: 0 });
            }
        }
    }

    /// The comp-settings area of the dummy page: theme name, dark and light swatches, preview switch, save / cancel.
    pub(super) fn theme_cells(&mut self) {
        let interactive = self.edit.as_ref().map(|e| e.picker.is_none()).unwrap_or(false);
        let keep = self.real_in;
        if interactive {
            self.unmask_in();
        }
        let xs = [152.0, 220.0, 288.0, 356.0];
        let ys = [196.0, 240.0, 284.0];
        for k in 0..3 {
            self.tc_swatch(xs[k], ys[0], k, interactive);
            self.tc_swatch(xs[k], ys[1], 3 + k, interactive);
        }
        let Some(e) = &self.edit else { return };
        let pl = e.preview_light;
        let name = e.name.clone();

        if self.tc_box(xs[3], ys[0], 62.0, 38.0, "PREVIEW", if pl { "LIGHT" } else { "DARK" }, false, false) && interactive {
            self.press = false;
            if let Some(e) = &mut self.edit {
                e.preview_light = !e.preview_light;
            }
        }
        if self.tc_box(xs[3], ys[1], 62.0, 38.0, "AUTO", "LIGHT", false, false) && interactive {
            self.press = false;
            if let Some(e) = &mut self.edit {
                // light = dark with BG and UI swapped, same accent
                e.col[3] = e.col[1];
                e.col[4] = e.col[0];
                e.col[5] = e.col[2];
            }
        }

        // name field (two cells wide)
        let shown = if name.is_empty() { "Custom".to_string() } else { name.clone() };
        if self.tc_box(xs[0], ys[2], 130.0, 38.0, "THEME NAME (CLICK)", &shown, false, false) && interactive {
            self.press = false;
            self.start_prompt(0, "Theme name", &name);
        }
        if self.tc_box(xs[2], ys[2], 62.0, 38.0, "", "SAVE", true, false) && interactive {
            self.press = false;
            self.save_edit();
        } else if self.edit.is_some() && self.tc_box(xs[3], ys[2], 62.0, 38.0, "", "CANCEL", false, false) && interactive {
            self.press = false;
            self.cancel_edit();
        }
        if interactive {
            self.real_in = keep;
            self.mask_in();
        }
    }

    /// Colour picker overlay (over the compressor plot): saturation / value square, hue strip, hex field, OK / CANCEL.
    /// Edits the slot live, so the whole dummy page re-colours while you drag.
    pub(super) fn theme_picker(&mut self) {
        let Some(e) = &self.edit else { return };
        let Some(p) = &e.picker else { return };
        let slot = p.slot;
        let (x, y, w, h) = (152.0, 56.0, 266.0, 128.0);
        self.setc(false);
        self.cv.setbg();
        self.cv.fill(x, y, w, h);
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        self.cv.set_font(1);
        self.cv.text(x + 8.0, y + 6.0, &format!("{}  (live preview)", SLOT_NAMES[slot]));

        let (sx, sy, sw, sh_) = (x + 8.0, y + 20.0, 120.0, 100.0);
        let (hx, hw) = (sx + sw + 8.0, 14.0);
        let (mx, my, down, press) = (self.mx, self.my, self.down, self.press);
        let mut close_ok = false;
        let mut close_cancel = false;
        let hit = |ax: f32, ay: f32, aw: f32, ah: f32| mx >= ax && mx < ax + aw && my >= ay && my < ay + ah;
        let mut ask_hex = false;
        let idle = self.prompt.is_none();

        let Some(e) = &mut self.edit else { return };
        let Some(p) = &mut e.picker else { return };
        if press && idle {
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
                self.cv.setrgb(hsv_to_rgb(ph, s_, v_));
                self.cv.fill(sx + ix as f32 * cell, sy + iy as f32 * cell, cell, cell);
            }
        }
        for iy in 0..(sh_ / cell) as i32 {
            self.cv.setrgb(hsv_to_rgb((iy as f32 + 0.5) * cell / sh_, 1.0, 1.0));
            self.cv.fill(hx, sy + iy as f32 * cell, hw, cell);
        }
        self.setc(false);
        self.cv.boxr(sx - 1.0, sy - 1.0, sw + 2.0, sh_ + 2.0);
        self.cv.boxr(hx - 1.0, sy - 1.0, hw + 2.0, sh_ + 2.0);
        // markers: a ring on the square, a bar on the strip (contrast against what is under them)
        let (cx, cy) = (sx + ps * (sw - 1.0), sy + (1.0 - pv) * (sh_ - 1.0));
        self.cv.setrgb(contrast(cur));
        self.cv.boxr((cx - 3.0).floor(), (cy - 3.0).floor(), 7.0, 7.0);
        self.cv.setrgb(contrast(hsv_to_rgb(ph, 1.0, 1.0)));
        self.cv.fill(hx, (sy + ph * (sh_ - 1.0)).floor() - 1.0, hw, 3.0);

        // old / new, hex, RGB, buttons
        let rx = hx + hw + 8.0;
        self.setc(false);
        self.cv.text(rx, sy, "OLD");
        self.cv.text(rx + 56.0, sy, "NEW");
        self.cv.setrgb(old);
        self.cv.fill(rx, sy + 10.0, 50.0, 22.0);
        self.cv.setrgb(cur);
        self.cv.fill(rx + 56.0, sy + 10.0, 50.0, 22.0);
        self.setc(false);
        self.cv.boxr(rx - 1.0, sy + 9.0, 52.0, 24.0);
        self.cv.boxr(rx + 55.0, sy + 9.0, 52.0, 24.0);
        self.setc(false);
        self.cv.boxr(rx, sy + 42.0, 106.0, 16.0);
        self.cv.text(rx + 4.0, sy + 46.0, &format!("#{hexshow}  EDIT"));
        self.setc(false);
        let b = |v: f32| (v * 255.0 + 0.5) as u8;
        self.cv.text(rx, sy + 66.0, &format!("R{:03} G{:03} B{:03}", b(cur[0]), b(cur[1]), b(cur[2])));
        let ok = self.btn(rx, sy + 84.0, 50.0, 16.0, "OK", 1);
        let cn = self.btn(rx + 56.0, sy + 84.0, 50.0, 16.0, "CANCEL", 0);
        if ask_hex {
            self.start_prompt(1, "Hex colour (RRGGBB)", &hexshow);
        }
        if ok && press {
            close_ok = true;
        }
        if cn && press {
            close_cancel = true;
        }
        if close_cancel {
            if let Some(e) = &mut self.edit {
                e.col[slot] = old;
                e.picker = None;
            }
        } else if close_ok {
            if let Some(e) = &mut self.edit {
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
        let t = UserTheme { name: "My Theme".into(), dark: [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0], light: [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.2, 0.4, 1.0] };
        let r = decode(&encode(&t)).unwrap();
        assert_eq!(r.name, "My Theme");
        for k in 0..9 {
            assert!((r.dark[k] - t.dark[k]).abs() < 0.003 && (r.light[k] - t.light[k]).abs() < 0.003);
        }
        assert!(decode("x|zz").is_none());
        let c = [0.2, 0.6, 0.9];
        let (h, s, v) = rgb_to_hsv(c);
        let b = hsv_to_rgb(h, s, v);
        assert!((0..3).all(|i| (b[i] - c[i]).abs() < 1e-5));
    }
}
