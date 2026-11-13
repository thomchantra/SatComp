//! SatLim editor: the SatComp sat module (plot, draw view, cells) on the shared widget kit, the transient scope, the
//! limiter column and the meters. Immediate mode with the same `act` / `press` drag logic as SatComp.

use satcomp_ui::canvas::Canvas;
use satcomp_ui::kit::{Kit, Meta, Widgets};
use satcomp_ui::appearance::{self, AppState, AppearanceUi, CellGrid, Rect};
use crate::params::{SatLimParams, SLOTS, TABLE};
use crate::presets::{self, Preset};
use crate::Shared;
use super::{H, W};
use nih_plug::prelude::*;
use satcomp_dsp::shapers::{Curve, ShaperLut, CUSTOM, MAX_PTS, MIN_GAP, SHAPER_NAMES};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;
use std::sync::Arc;

enum MenuKind {
    Shape,
    Palette,
    Size,
    Preset,
}

/// kind: 0 selectable item, 1 header (dim, not selectable), 2 separator
#[derive(Clone)]
struct MItem {
    label: String,
    kind: u8,
    id: i32,
}

/// Output VU ranges: (dB span, dB values labelled), cycled by clicking the bars.
const VU_RANGES: [(f32, &[i32]); 4] = [(48.0, &[-6, -12, -24, -36]), (24.0, &[-3, -6, -12, -18]), (12.0, &[-2, -4, -6, -9]), (6.0, &[-1, -2, -3, -4, -5])];

/// ASYM slider drag (viewport-style act id: released with the mouse)
const ACT_ASYM: i32 = 145;
const MENU_ROWS: usize = 18;
const MENU_ROW_H: f32 = 14.0;

/// Window scale steps (percent of the W x H base size).
const SIZES: [f32; 6] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5];

struct Menu {
    kind: MenuKind,
    x: f32,
    y: f32,
    items: Vec<MItem>,
    checked: i32,
    scroll: usize,
}

/// Outcome of a file dialog on its worker thread.
pub enum Dlg {
    Pending,
    Cancelled,
    Loaded(Preset),
    Saved(String),
}

mod scope;

pub struct State {
    /// widget interaction state (satcomp-ui); `State` derefs to it
    pub kit: Kit,
    /// packed scope settings (see scope.rs)
    scope_cfg: i32,
    /// scope viewport drag start: (mouse x, mouse y, window step, ceiling step)
    scope_drag: (f32, f32, usize, usize),
    /// eased read position in the scope ring (buckets) and the time of the last scope frame
    scope_pos: f64,
    scope_t: f64,
    /// ASYM drag anchor: (mouse y, value 0..1) at the press
    asym_drag: (f32, f32),
    /// colour themes, theme editor and About-page state (satcomp-ui)
    pub app: AppState,
    pub dummy_params: Arc<SatLimParams>,
    pub dummy_shared: Arc<Shared>,
    pub pal: usize,
    pub light: bool,
    /// UI font index (FONT_NAMES)
    pub face: usize,
    /// time of a logo click waiting out the double-click window (then it cycles the colour scheme)
    logo_pend: f64,
    // interaction
    vt1: usize,
    vt2: usize,
    menu: Option<Menu>,
    // per-frame input (logical units)
    // meters
    gr_disp: f32,
    /// the clipper's reduction (dB): decaying display value, and its readout copy
    clip_disp: f32,
    rd_clip: f32,
    gr_hold: f32,
    gr_hold_t: f64,
    gr_ri: usize,
    ovl: f32,
    ovr: f32,
    hl_l: f32,
    hl_r: f32,
    hl_l_t: f64,
    hl_r_t: f64,
    opk_max: f32,
    clip_lat: bool,
    clip_t: f64,
    /// output VU range index (VU_RANGES)
    vu_ri: usize,
    /// post-saturator level meter: decaying level and the last time it clipped (accent, held 1 s)
    sat_lv: f32,
    sat_clip_t: f64,
    /// limiter gain reduction (dB): decaying display value
    // readouts (refresh ~8x/s)
    rd_last: f64,
    rd_tick: bool,
    rd_gr: f32,
    rd_pk: f32,
    rd_rms: f32,
    rd_sat: f32,
    // sat plot
    lut: ShaperLut,
    /// custom-curve draw mode (pencil) and the point being dragged
    curve_edit: bool,
    /// draw view tool: true = symmetry (odd curve), false = free draw
    sym_mode: bool,
    /// shape was observed as Custom since the draw view opened (host writes land a frame or more later)
    edit_seen: bool,
    cpt: usize,
    // window resize grip
    resizing: bool,
    resize_off: (f32, f32),
    pub ptr_pt: (f32, f32),
    /// egui screen size in points (what the host window has actually become so far)
    pub win_pt: (f32, f32),
    // presets
    factory: Vec<Preset>,
    user: Vec<(String, std::path::PathBuf)>,
    /// 1 = load dialog, 2 = save dialog (run after the frame is drawn, they block)
    pending: u8,
    /// result slot of a file dialog running on a worker thread (never block the UI/host thread in a modal)
    dlg: Option<Arc<parking_lot::Mutex<Dlg>>>,
    /// preset the edit marker compares against (cached by name), and the post-load settle time
    base: Option<(String, Preset)>,
    hold_until: f64,
}

impl State {
    /// Per-frame input snapshot (positions in logical canvas units). Computes press edges like the JSFX.
    pub fn set_input(&mut self, mx: f32, my: f32, down: bool, rdown: bool, scroll: f32, shift: bool, now: f64) {
        // The egui clock restarts from 0 whenever the editor window is reopened, but this State lives on. Re-base every
        // stored timestamp, otherwise `now - last` stays negative and the ~8 Hz readouts (PK, RMS, GR) stay frozen.
        if self.kit.set_input(mx, my, down, rdown, scroll, shift, now) {
            self.rd_last = now;
            self.hl_l_t = now;
            self.hl_r_t = now;
            self.gr_hold_t = now;
            self.sat_clip_t = -10.0;
            self.hold_until = now;
            self.scope_t = now;
        }
    }

    pub fn new(pal: usize, light: bool, face: usize, params: &SatLimParams) -> Self {
        let mut kit = Kit::new(SLOTS);
        let meta = &mut kit.meta;
        for &(n, _, mn, mx, df) in TABLE {
            meta[n] = Meta { min: mn, max: mx, def: df, mode: 0, snap: 0, step: 1.0 };
        }
        let mut sp = |n: usize, mode: u8, snap: u8, step: f32| {
            meta[n].mode = mode;
            meta[n].snap = snap;
            meta[n].step = step;
        };
        sp(2, 0, 0, 0.5);
        sp(3, 0, 0, 1.0);
        sp(4, 1, 1, 1.0);
        sp(5, 1, 2, 10.0);
        sp(6, 1, 2, 10.0);
        sp(8, 0, 0, 0.1);
        sp(9, 0, 0, 0.1);
        sp(10, 0, 0, 1.0);
        sp(11, 0, 0, 1.0);
        sp(22, 0, 0, 0.1);
        sp(25, 0, 0, 1.0);
        sp(27, 0, 0, 0.1);
        let mut st = Self {
            kit,
            pal,
            light,
            face,
            scope_cfg: params.scope.load(Ordering::Relaxed),
            scope_drag: (0.0, 0.0, 0, 0),
            scope_pos: 0.0,
            scope_t: 0.0,
            asym_drag: (0.0, 0.0),
            app: AppState::new(),
            dummy_params: Arc::new(SatLimParams::from_defaults(None)),
            dummy_shared: Arc::new(Shared::new()),
            logo_pend: -1.0,
            vt1: 0,
            vt2: 0,
            menu: None,
            gr_disp: 0.0,
            clip_disp: 0.0,
            rd_clip: 0.0,
            gr_hold: 0.0,
            gr_hold_t: 0.0,
            gr_ri: 0,
            ovl: 0.0,
            ovr: 0.0,
            hl_l: 0.0,
            hl_r: 0.0,
            hl_l_t: 0.0,
            hl_r_t: 0.0,
            opk_max: 0.0,
            clip_lat: false,
            clip_t: -10.0,
            vu_ri: 0,
            sat_lv: 0.0,
            sat_clip_t: -10.0,
            rd_last: 0.0,
            rd_tick: false,
            rd_gr: 0.0,
            rd_pk: 0.0,
            rd_rms: 0.0,
            rd_sat: 0.0,
            lut: ShaperLut::new(0),
            curve_edit: false,
            sym_mode: true,
            edit_seen: false,
            cpt: 0,
            resizing: false,
            resize_off: (0.0, 0.0),
            ptr_pt: (0.0, 0.0),
            win_pt: (W, H),
            factory: presets::factory(),
            user: presets::scan_user(),
            pending: 0,
            dlg: None,
            base: None,
            hold_until: 0.0,
        };
        st.restore_custom(params);
        st
    }
}

impl State {
    /// Colours the canvas draws with: the editor preview while editing, else the active scheme.
    pub fn colors(&self) -> [f32; 9] {
        self.app.colors(self.pal, self.light)
    }

    /// nih-plug builds the editor (and so this state) when the plug-in is created, BEFORE the host restores the project's
    /// saved parameters. Re-read the saved look every time the window opens, otherwise a project reopens on the defaults.
    pub fn sync_look(&mut self, params: &SatLimParams) {
        let (pal, light) = initial_palette(params);
        self.pal = pal;
        self.light = light;
        self.face = (params.font.load(Ordering::Relaxed).max(0) as usize).min(satcomp_ui::canvas::FONT_NAMES.len() - 1);
        self.scope_cfg = params.scope.load(Ordering::Relaxed);
        self.restore_custom(params);
    }

    /// A project that used a user theme: resolve it from the themes file or the project copy.
    pub fn restore_custom(&mut self, params: &SatLimParams) {
        let pal = self.app.restore_custom(self.pal, &params.custom_theme.lock());
        if pal != self.pal || pal >= satcomp_ui::canvas::PALETTES.len() {
            self.pal = pal;
            params.palette_v2.store(pal as i32, Ordering::Relaxed);
        }
    }

    /// Constant meter / readout values for the dummy page (set every frame, the draw code then settles on them).
    pub fn fill_dummy(&self) {
        let d = &self.dummy_shared;
        d.gr_pk.store(3.5, Ordering::Relaxed);
        d.opk[0].store(1.0, Ordering::Relaxed);
        d.opk[1].store(0.5, Ordering::Relaxed);
        d.clip.store(true, Ordering::Relaxed);
        d.fms.store(0.05, Ordering::Relaxed);
        d.sat_db.store(2.0, Ordering::Relaxed);
        d.sat_vz.store(0.3, Ordering::Relaxed);
        d.sat_pk.store(0.4, Ordering::Relaxed);
        d.tpk.store(0.9, Ordering::Relaxed);
        d.clip_gr_pk.store(1.0, Ordering::Relaxed);
        // an active-looking scope: drum hits every half second (alternating strong / softer) with some of each hit taken off
        // by the clipper and the limiter. Filled once; the ring then never moves, so the picture stands still.
        if d.scope.head() == 0 {
            for i in 0..1600u32 {
                let phase = (i % 250) as f32;
                let amp = if (i / 250) % 2 == 0 { 1.0 } else { 0.7 };
                let pre = 0.08 + 1.2 * amp * (-phase / 30.0).exp() + 0.04 * ((i as f32) * 0.9).sin().abs();
                let handed = pre.min(1.1);
                let out = handed.min(0.85);
                d.scope.push3(out, handed, pre);
            }
        }
    }
}

impl Deref for State {
    type Target = Kit;
    fn deref(&self) -> &Kit {
        &self.kit
    }
}
impl DerefMut for State {
    fn deref_mut(&mut self) -> &mut Kit {
        &mut self.kit
    }
}

pub struct Env<'a> {
    pub params: &'a SatLimParams,
    pub setter: &'a ParamSetter<'a>,
    pub shared: &'a Shared,
    pub dsp: satlim_dsp::LimParams,
}

pub struct Gui<'a> {
    pub s: &'a mut State,
    pub cv: Canvas<'a>,
    pub env: Env<'a>,
}

impl<'a> Widgets<'a> for Gui<'a> {
    fn kit(&mut self) -> &mut Kit {
        &mut self.s.kit
    }
    fn kit_ref(&self) -> &Kit {
        &self.s.kit
    }
    fn cv(&mut self) -> &mut Canvas<'a> {
        &mut self.cv
    }
    fn param_ptr(&self, n: usize) -> Option<ParamPtr> {
        self.env.params.ptr(n)
    }
    fn host(&self) -> &dyn GuiContext {
        self.env.setter.raw_context
    }
    fn masked(&self) -> bool {
        self.s.app.edit.is_some()
    }
    fn fmt(&self, n: usize, v: f32) -> String {
        self.fmt_slot(n, v)
    }
}

impl<'a> AppearanceUi<'a> for Gui<'a> {
    fn app(&mut self) -> &mut AppState {
        &mut self.s.app
    }
    fn app_ref(&self) -> &AppState {
        &self.s.app
    }
    fn pal(&self) -> usize {
        self.s.pal
    }
    fn light(&self) -> bool {
        self.s.light
    }
    fn face(&self) -> usize {
        self.s.face
    }
    fn commit_scheme(&mut self, pal: usize, encoded: String) {
        self.s.pal = pal;
        self.env.params.palette_v2.store(pal as i32, Ordering::Relaxed);
        // a user theme is also stored in the project, so it survives a machine without the themes file
        *self.env.params.custom_theme.lock() = encoded;
        self.apply_pal();
    }
    fn commit_light(&mut self, light: bool) {
        self.s.light = light;
        self.env.params.light.store(light, Ordering::Relaxed);
        self.apply_pal();
    }
    fn plugin_key(&self) -> &'static str {
        "satlim"
    }
    fn presets_dir(&self) -> Option<std::path::PathBuf> {
        crate::presets::user_dir()
    }
    fn scale_now(&self) -> f32 {
        self.cur_scale()
    }
    fn set_scale(&mut self, scale: f32) {
        self.request_scale(scale);
    }
    fn factory_scale(&self) -> f32 {
        1.0
    }
    fn factory_fill_solid(&self) -> bool {
        true
    }
    fn commit_face(&mut self, face: usize) {
        self.s.face = face;
        self.env.params.font.store(face as i32, Ordering::Relaxed);
        self.cv.face = face;
    }
    fn fft_solid(&self) -> Option<bool> {
        Some(self.env.params.fft_solid.load(Ordering::Relaxed))
    }
    fn commit_fft_solid(&mut self, solid: bool) {
        self.env.params.fft_solid.store(solid, Ordering::Relaxed);
    }
    fn fill_label(&self) -> &'static str {
        "SCOPE FILL"
    }
    fn about_paras(&self) -> &'static [&'static [&'static str]] {
        ABOUT_TEXT
    }
    fn changelog(&self) -> &'static [(&'static str, &'static [&'static str])] {
        CHANGELOG
    }
    fn manual_pdf(&self) -> Option<(&'static str, &'static [u8])> {
        // the file must exist for the crate's current version (docs/manual/build.py), or the plug-in does not compile
        Some((
            concat!("SatLim-Manual-v", env!("CARGO_PKG_VERSION"), ".pdf"),
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/manual/SatLim-Manual-v", env!("CARGO_PKG_VERSION"), ".pdf")),
        ))
    }
    fn draw_flow(&mut self, r: Rect, byp: bool) {
        self.draw_flow_at(r.x, r.y, r.w, r.h, byp);
    }
    fn about_scroll_drag_start(&mut self) {
        self.vt1 = 0;
        self.vt2 = 0;
    }
}

impl<'a> Deref for Gui<'a> {
    type Target = State;
    fn deref(&self) -> &State {
        self.s
    }
}
impl<'a> DerefMut for Gui<'a> {
    fn deref_mut(&mut self) -> &mut State {
        self.s
    }
}

/// About page copy (docs/abouttextcopy.md). Each inner slice is one paragraph, each string one source line.
const ABOUT_TEXT: &[&[&str]] = &[
    &["A 2-stage clipper/limiter with waveshaping saturator", "When you need to nuke it from orbit B)"],
    &["Use tube with low drive for warm saturation, opt for destruction with tri-fold, or annihilate with a custom asymmetric waveshape.", "[Click] the top-right pencil icon to enter custom draw mode.", "[Click] the lower-right SAT value to auto-match the gain."],
    &["Turn on lookahead for brickwall, true peak limiting. Lowering the threshold chews the input signal deeper. Clip exposes and bricks more transients in the mix."],
    &["Balance threshold (limiter) and clip (clipper) to dial in the desired loudness and character."],
    &["A shorter release gives a denser, punchier result while introducing distortion artifacts, a longer release is smoother with lower distortion."],
    &["Transient/GR meter: solid = limiter gain reduction, striped = clipper gain reduction."],
    &["[Double-click] slider resets the parameter to its default value", "[Click] GR meter cycles its range", "[Click] clip meter resets the readout", "[Left/Right-click] top-left icon cycles the colour theme", "[Double-click] top-left icon switches between dark and light mode"],
];

/// Changelog, newest first. Keep each entry to two short sentences or fewer.
const CHANGELOG: &[(&str, &[&str])] = &[
    ("v0.1", &["First version: the SatComp saturator, an oversampled clipper (CLIP, x1 to x8) and a true-peak brickwall limiter with a transient scope.", "L.AHEAD off gives a zero-latency limiter. OUT GAIN sits after the limiter on purpose.", "The limiter and the TRU.PK readout detect true peaks at 8x, with L.AHEAD on or off, so inter-sample peaks of a clipped or folded signal are caught. Bypass is a plain bypass."]),
];

/// Crate version for the header: "0.4" for 0.4.0, "0.4.1" for a patch release.
/// Palette index and light flag to start with (`palette-v2`; -1 = not set yet = the first scheme, dark).
pub fn initial_palette(p: &crate::params::SatLimParams) -> (usize, bool) {
    let v2 = p.palette_v2.load(Ordering::Relaxed);
    if v2 >= 0 {
        return (v2 as usize, p.light.load(Ordering::Relaxed)); // >= 20 = a user theme, resolved in State::restore_custom
    }
    (0, false)
}

fn version_short() -> String {
    if cfg!(test) {
        return "TEST".into(); // keeps the UI golden independent of release version bumps
    }
    let v = env!("CARGO_PKG_VERSION");
    v.strip_suffix(".0").unwrap_or(v).to_string()
}


impl<'a> Gui<'a> {
    /// Push the active scheme's colours into the canvas.
    fn apply_pal(&mut self) {
        let a = self.app.scheme_cols(self.pal, self.light);
        self.cv.bk = [a[0], a[1], a[2]];
        self.cv.fg = [a[3], a[4], a[5]];
        self.cv.ac = [a[6], a[7], a[8]];
    }

    // ------------------------------------------------------------------ params (slider-number addressing)
    fn refresh_dsp(&mut self) {
        self.env.dsp = self.env.params.to_dsp();
    }

    // ------------------------------------------------------------------ formatting
    fn fmt_slot(&self, n: usize, v: f32) -> String {
        match n {
            2 | 25 | 11 => format!("{:.0}%", v),
            3 => format!("{:+.0}%", v),
            4 => {
                if v <= 20.5 {
                    "OFF".into()
                } else {
                    format!("{:.0}Hz", v)
                }
            }
            5 | 6 => {
                if v >= 19900.0 {
                    "OFF".into()
                } else if n != 6 && v < 1000.0 {
                    format!("{:.0}Hz", v)
                } else {
                    format!("{:.1}k", v * 0.001)
                }
            }
            8 | 9 => format!("{:.1}dB", v),
            10 => format!("{:.0}ms", v),
            22 | 27 => format!("{:+.1}dB", v),
            _ => format!("{:.1}", v),
        }
    }

    // ------------------------------------------------------------------ widgets
    fn open_menu(&mut self, kind: MenuKind, x: f32, y: f32, items: Vec<String>, checked: usize) {
        let items = items.into_iter().enumerate().map(|(i, label)| MItem { label, kind: 0, id: i as i32 }).collect();
        self.menu = Some(Menu { kind, x, y, items, checked: checked as i32, scroll: 0 });
    }

    /// shape selector: [<] NAME [>]
    fn selector(&mut self, n: usize, x: f32, y: f32, w: f32, h: f32) {
        let aw = 16.0;
        let cy = y + h * 0.5;
        let hv = self.hit(x, y, w, h);
        let mut sel = (self.sget(n) + 0.5).floor() as i32;
        if hv && self.wheel != 0 {
            self.click_set(n, (sel + if self.wheel > 0 { -1 } else { 1 }).clamp(0, CUSTOM) as f32);
        }
        if hv && self.press && self.act == 0 {
            if self.mx < x + aw {
                self.click_set(n, if sel <= 0 { CUSTOM as f32 } else { (sel - 1) as f32 });
            } else if self.mx >= x + w - aw {
                self.click_set(n, if sel >= CUSTOM { 0.0 } else { (sel + 1) as f32 });
            } else {
                let items = SHAPER_NAMES.iter().map(|s| s.to_string()).collect();
                self.open_menu(MenuKind::Shape, x, y + h, items, sel as usize);
            }
            self.press = false;
        }
        sel = (self.sget(n) + 0.5).floor() as i32;
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        self.cv.fill(x + aw, y, 1.0, h);
        self.cv.fill(x + w - aw - 1.0, y, 1.0, h);
        self.cv.tri((x + aw * 0.5 + 2.0, cy - 3.0), (x + aw * 0.5 + 2.0, cy + 3.0), (x + aw * 0.5 - 2.0, cy));
        self.cv.tri((x + w - aw * 0.5 - 2.0, cy - 3.0), (x + w - aw * 0.5 - 2.0, cy + 3.0), (x + w - aw * 0.5 + 2.0, cy));
        self.cv.set_font(2);
        let name = SHAPER_NAMES[sel.clamp(0, CUSTOM) as usize].to_uppercase();
        self.cv.textc(x + aw, y + ((h - 8.0) * 0.5).floor(), w - 2.0 * aw, &name);
    }


    /// header logo: left click = next colour scheme, right click = scheme dropdown
    fn logo(&mut self, x: f32, y: f32) {
        let hv = self.hit(x - 3.0, y - 3.0, 27.0, 27.0);
        if hv && self.press && self.act == 0 {
            // single click = next scheme (after the double-click window), double click = light/dark
            if self.logo_pend >= 0.0 && self.now - self.logo_pend < 0.3 {
                self.logo_pend = -1.0;
                self.set_light(!self.light);
            } else {
                self.logo_pend = self.now;
            }
            self.press = false;
        }
        if self.logo_pend >= 0.0 && self.now - self.logo_pend >= 0.3 {
            self.logo_pend = -1.0;
            let n = self.app.n_schemes();
            self.set_pal((self.pal + 1) % n);
        }
        if hv && self.rpress {
            self.logo_pend = -1.0;
            let mut items = vec![MItem { label: format!("MODE: {}", if self.light { "LIGHT" } else { "DARK" }), kind: 0, id: 1000 }];
            items.push(MItem { label: String::new(), kind: 2, id: -1 });
            items.extend((0..self.app.n_schemes()).map(|i| MItem { label: self.app.scheme_name(i), kind: 0, id: i as i32 }));
            self.menu = Some(Menu { kind: MenuKind::Palette, x, y: y + 22.0, items, checked: self.pal as i32, scroll: 0 });
        }
        // plug-in icon in a square box: fill = BG, stroke = UI (so light mode inverts it with the scheme)
        self.setc(false);
        // hovering inverts the icon (a plate in the UI colour, or back to the outline in light mode): it is clickable
        if self.light != hv {
            // light mode inverts the icon: UI-coloured plate, BG-coloured glyph
            self.cv.fill(x - 3.0, y - 3.0, 27.0, 27.0);
            self.cv.setbg();
        } else {
            self.cv.boxr(x - 3.0, y - 3.0, 27.0, 27.0);
            self.setc(false);
        }
        self.cv.boxr(x + 6.0, y + 6.0, 9.0, 9.0);
        let l = |c: &Canvas, a: f32, b: f32, cc: f32, d: f32| c.line(x + a, y + b, x + cc, y + d);
        // four corner brackets round the inner square
        for (sx, sy) in [(0.5f32, 0.5f32), (20.5, 0.5), (0.5, 20.5), (20.5, 20.5)] {
            let (dx, dy) = (if sx < 10.0 { 1.0 } else { -1.0 }, if sy < 10.0 { 1.0 } else { -1.0 });
            l(&self.cv, sx, sy, sx + 7.0 * dx, sy);
            l(&self.cv, sx, sy, sx, sy + 7.0 * dy);
        }
    }


    // ------------------------------------------------------------------ meters
    fn draw_meter(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let g = self.env.shared.gr_pk.swap(0.0, Ordering::Relaxed);
        let cg = self.env.shared.clip_gr_pk.swap(0.0, Ordering::Relaxed);
        self.gr_disp = g.max(self.gr_disp * 0.9);
        self.clip_disp = cg.max(self.clip_disp * 0.9);
        // the bar is the total reduction: the limiter's part as solid accent from the top, the clipper's hatched right under it
        let total = self.gr_disp + self.clip_disp;
        if g + cg >= self.gr_hold {
            self.gr_hold = g + cg;
            self.gr_hold_t = self.now;
        } else if self.now - self.gr_hold_t > 1.0 {
            self.gr_hold = (self.gr_hold - 0.4).max(0.0);
        }
        if self.hit(x, y, w, h) && self.press && self.act == 0 {
            self.gr_ri = if self.gr_ri >= 3 { 0 } else { self.gr_ri + 1 };
            self.press = false;
        }
        let rng = |i: usize| [3.0f32, 6.0, 12.0, 18.0][i.min(3)];
        while self.gr_ri < 3 && total > rng(self.gr_ri) {
            self.gr_ri += 1;
        }
        let rg = rng(self.gr_ri);
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        self.setc(true);
        let lim_h = ((h - 2.0) * (self.gr_disp / rg).min(1.0)).floor();
        let all_h = ((h - 2.0) * (total / rg).min(1.0)).floor();
        self.cv.fill(x + 1.0, y + 1.0, w - 2.0, lim_h);
        let mut ry = y + 1.0 + lim_h;
        while ry < y + 1.0 + all_h {
            self.cv.fill(x + 1.0, ry, w - 2.0, 1.0); // hatched: every other row
            ry += 2.0;
        }
        self.setc(false);
        let hp = y + 1.0 + ((h - 3.0) * (self.gr_hold / rg).min(1.0)).floor();
        self.cv.fill(x + 1.0, hp, w - 2.0, 1.0);
        self.cv.set_font(1);
        // total GR as a number in a window at the top of the bar (positive, like the bar)
        self.cv.setbg();
        self.cv.fill(x + 1.0, y + 1.0, w - 2.0, 11.0);
        self.setc(false);
        self.cv.fill(x + 1.0, y + 12.0, w - 2.0, 1.0);
        let gs = format!("{:.1}", self.rd_gr + self.rd_clip);
        self.cv.textc(x, y + 3.0, w, &gs);
        let mj = if rg == 3.0 { 1.0 } else if rg == 6.0 { 2.0 } else if rg == 12.0 { 3.0 } else { 6.0 };
        let stp = if rg <= 6.0 { 0.5 } else { 1.0 };
        let cnt = (rg / stp) as i32;
        for j in 1..=cnt {
            let v = j as f32 * stp;
            let ty = y + 1.0 + (h - 2.0) * (v / rg);
            let major = v == v.floor() && (v.floor() % mj) == 0.0;
            if major {
                self.cv.fill(x + w - 5.0, ty, 4.0, 1.0);
                let s = format!("{}", v as i32);
                self.cv.text(x + 2.0, (ty - 4.0).min(y + h - 11.0), &s);
            } else {
                self.cv.fill(x + w - 3.0, ty, 2.0, 1.0);
            }
        }
    }

    /// Post-saturator level (summed channels), segmented (-48..0 dB). The clip lamp is a block at the top of the bar, the
    /// height of the GR value window, in the accent colour while the saturator output is over 0 dBFS and for 1 s after.
    fn sat_vu(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let pk = self.env.shared.sat_pk.swap(0.0, Ordering::Relaxed);
        self.sat_lv = pk.max(self.sat_lv * 0.85);
        if pk >= 1.0 {
            self.sat_clip_t = self.now;
        }
        let clip = self.now - self.sat_clip_t < 1.0;
        let lamp = 12.0;
        let (by, bh) = (y + lamp + 1.0, h - lamp - 1.0);
        let lvf = |v: f32| ((8.6858896 * v.max(0.0000001).ln() + 48.0) / 48.0).clamp(0.0, 1.0);
        self.setc(false);
        self.cv.boxr(x, by, w, bh);
        // 2 px segments, 1 px gaps, filled from the bottom
        let segs = ((bh - 2.0) / 3.0).floor() as i32;
        let on = (segs as f32 * lvf(self.sat_lv)).ceil() as i32;
        for i in 0..on.min(segs) {
            self.cv.fill(x + 1.0, by + bh - 1.0 - 2.0 - i as f32 * 3.0, w - 2.0, 2.0);
        }
        self.setc(clip);
        if clip {
            self.cv.fill(x, y, w, lamp);
        } else {
            self.cv.boxr(x, y, w, lamp);
        }
        self.setc(false);
    }

    /// Stereo output level, vertical: two bars with peak holds over a selectable range (click the bars to cycle 48 / 24 / 12 /
    /// 6 dB), small dB labels in the background colour on top of the bars, and a clip lamp at the top (accent after any over
    /// 0 dBFS; click it to reset the lamp and the peak readout). The true / sample peak readout is tracked here too.
    fn out_vu(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let sh = self.env.shared;
        let pa = sh.opk[0].swap(0.0, Ordering::Relaxed);
        let pb = sh.opk[1].swap(0.0, Ordering::Relaxed);
        let tp = sh.tpk.swap(0.0, Ordering::Relaxed);
        if sh.clip.swap(false, Ordering::Relaxed) {
            self.clip_lat = true;
            self.clip_t = self.now;
        }
        let lamp = 12.0;
        let (by, bh) = (y + lamp + 1.0, h - lamp - 1.0);
        if self.hit(x, y, w, lamp) && self.press && self.act == 0 {
            self.opk_max = 0.0;
            self.clip_lat = false;
            self.hl_l = 0.0;
            self.hl_r = 0.0;
            self.press = false;
        }
        if self.hit(x, by, w, bh) && self.press && self.act == 0 {
            self.vu_ri = (self.vu_ri + 1) % VU_RANGES.len();
            self.press = false;
        }
        self.ovl = pa.max(self.ovl * 0.85);
        self.ovr = pb.max(self.ovr * 0.85);
        let now = self.now;
        if pa >= self.hl_l {
            self.hl_l = pa;
            self.hl_l_t = now;
        } else if now - self.hl_l_t > 1.0 {
            self.hl_l *= 0.9;
        }
        if pb >= self.hl_r {
            self.hl_r = pb;
            self.hl_r_t = now;
        } else if now - self.hl_r_t > 1.0 {
            self.hl_r *= 0.9;
        }
        self.opk_max = self.opk_max.max(tp);
        let (rng, labels) = VU_RANGES[self.vu_ri];
        let bw = ((w - 3.0) * 0.5).floor();
        let lvf = |v: f32| ((8.6858896 * v.max(0.0000001).ln() + rng) / rng).clamp(0.0, 1.0);
        self.setc(false);
        self.cv.boxr(x, by, w, bh);
        for (i, (lv, hl)) in [(self.ovl, self.hl_l), (self.ovr, self.hl_r)].into_iter().enumerate() {
            let bx = x + 1.0 + i as f32 * (bw + 1.0);
            let fh = ((bh - 2.0) * lvf(lv)).floor();
            if fh > 0.0 {
                self.cv.fill(bx, by + bh - 1.0 - fh, bw, fh);
            }
            if hl > 0.0001 {
                self.cv.fill(bx, by + bh - 2.0 - ((bh - 3.0) * lvf(hl)).floor(), bw, 1.0);
            }
        }
        // dB labels (no sign) in the background colour on top of the bars, ticks beside them
        self.cv.set_font(1);
        for &db in labels {
            let ty = by + bh - 1.0 - ((bh - 2.0) * ((db as f32 + rng) / rng)).floor();
            self.setc(false);
            self.cv.fill(x + w, ty, 3.0, 1.0);
            self.cv.setbg();
            self.cv.text(x + 2.0, (ty - 4.0).min(by + bh - 11.0), &format!("{}", -db));
        }
        let clip = self.clip_lat;
        self.setc(clip);
        if clip {
            self.cv.fill(x, y, w, lamp);
        } else {
            self.cv.boxr(x, y, w, lamp);
        }
        self.setc(false);
    }

    // ------------------------------------------------------------------ sat plot
    /// BIAS over ASYM as one stacked group (like HP / LP). ASYM is the current shaper's share of its even part (the part that
    /// carries DC and sub-audio pulses): one value per shaper, stored in the project / presets (not a host parameter).
    /// Drag, wheel, double-click = 100 %.
    fn bias_asym(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let hh = (h * 0.5).floor();
        let shape = ((self.sget(1) + 0.5).floor() as i32).clamp(0, CUSTOM) as usize;
        let cell = self.env.params.asym.clone();
        let get = || satcomp_dsp::shapers::asym_from(&cell.lock())[shape] as f32;
        let set = |v: f32| {
            let mut g = cell.lock();
            if g.len() < SHAPER_NAMES.len() {
                g.resize(SHAPER_NAMES.len(), 100.0);
            }
            g[shape] = v.clamp(0.0, 100.0);
        };
        // top half: BIAS (an ordinary slider)
        let ha = self.hit(x, y, w, hh);
        if ha && self.press && self.act == 0 {
            self.begin_drag(3);
        }
        if ha && self.wheel != 0 {
            self.wheel_adj(3);
        }
        // bottom half: ASYM
        let hb = self.hit(x, y + hh, w, h - hh);
        if hb && self.press && self.act == 0 {
            if self.dbl(170) {
                set(100.0);
            } else {
                self.act = ACT_ASYM;
                self.asym_drag = (self.my, get() * 0.01);
            }
            self.press = false;
        }
        if hb && self.wheel != 0 {
            let d = if self.wheel > 0 { 1.0 } else { -1.0 } * if self.shift { 0.5 } else { 3.0 };
            set((get() + d).round());
        }
        if self.act == ACT_ASYM && self.down {
            let k = if self.shift { 0.0008 } else { 0.006 };
            let t = (self.asym_drag.1 + (self.asym_drag.0 - self.my) * k).clamp(0.0, 1.0);
            set((t * 100.0).round());
        }
        let act = self.act;
        self.setc(act == 3 || act == ACT_ASYM);
        self.cv.boxr(x, y, w, h);
        self.setc(false);
        self.cv.fill(x, y + hh, w, 1.0);
        self.cv.set_font(1);
        self.setc(act == 3);
        let vb = self.sget(3);
        let sb = self.fmt(3, vb);
        let tb = self.n2t(3, vb);
        self.cv.text(x + 3.0, y + 3.0, "BIAS");
        self.cv.textr(x, y + 3.0, w - 3.0, &sb);
        self.cv.fill(x + 2.0, y + hh - 3.0, ((tb * (w - 4.0)).floor()).max(1.0), 1.0);
        self.setc(act == ACT_ASYM);
        let v = get();
        self.cv.text(x + 3.0, y + hh + 3.0, "ASYM");
        self.cv.textr(x, y + hh + 3.0, w - 3.0, &format!("{:.0}%", v));
        self.cv.fill(x + 2.0, y + h - 4.0, (((v * 0.01) * (w - 4.0)).floor()).max(1.0), 2.0);
        self.setc(false);
    }

    fn draw_plot(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let sh = self.env.shared;
        let dsp = self.env.dsp;
        let sat_on = dsp.drive > 0.0 || dsp.bias != 0.0;
        let pre = (dsp.drive * 0.01 * 36.0 * 0.11512925).exp() as f32;
        let bias = (dsp.bias * 0.01) as f32;
        self.lut.sync(dsp.shape, &dsp.curve);
        // static curve as heard: the shaper minus the part of its even half that ASYM removes
        let ak = dsp.asym[(dsp.shape.max(0) as usize).min(dsp.asym.len() - 1)] * 0.01;
        let eff = move |l: &ShaperLut, v: f64| {
            let r = 0.5 * (1.0 - ak);
            l.eval(v) * (1.0 - r) - r * l.eval(-v)
        };
        // pencil (top right): toggles the custom-curve draw view; entering selects the Custom shaper so edits are audible
        let pen = self.hit(x + w - 14.0, y + 2.0, 12.0, 12.0);
        if pen && self.press && self.act == 0 {
            self.curve_edit = !self.curve_edit;
            self.edit_seen = false;
            if self.curve_edit {
                self.click_set(1, CUSTOM as f32);
                // symmetry is the default tool, unless the stored curve is lopsided: opening must not alter it
                self.sym_mode = self.env.params.curve_snapshot().is_odd();
            }
            self.press = false;
        }
        if self.curve_edit {
            if (self.sget(1) + 0.5).floor() as i32 == CUSTOM {
                self.edit_seen = true;
            } else if self.edit_seen {
                self.curve_edit = false; // another shaper was chosen
            }
        }
        let hv = self.hit(x, y, w, h) && !self.curve_edit;
        let tb = self.hit(x + w - 84.0, y + h - 14.0, 82.0, 12.0);
        if hv && self.press && self.act == 0 {
            if tb {
                self.env.shared.sat_req.store(true, Ordering::Relaxed); // measure 3 s, then write SAT GAIN
                self.press = false;
            } else {
                self.begin_drag(27);
            }
        }
        if hv && self.wheel != 0 {
            self.wheel_adj(27);
        }
        self.setc(self.act == 27);
        self.cv.boxr(x, y, w, h);
        for i in 0..(w / 2.0).floor() as i32 {
            self.cv.fill(x + i as f32 * 2.0, y + (h / 2.0).floor(), 1.0, 1.0);
        }
        for i in 0..(h / 2.0).floor() as i32 {
            self.cv.fill(x + (w / 2.0).floor(), y + i as f32 * 2.0, 1.0, 1.0);
        }
        if self.curve_edit {
            self.close_glyph(x + w - 14.0, y + 2.0, pen);
        } else {
            self.pencil_glyph(x + w - 14.0, y + 2.0, false, pen);
        }
        if self.curve_edit {
            self.draw_curve_edit(x, y, w, h);
            return;
        }
        let pk = sh.sat_vz.load(Ordering::Relaxed).min(1.0);
        let n = 64;
        let (mut lx, mut ly) = (0.0, 0.0);
        for i in 0..=n {
            let vx = -1.0 + 2.0 * i as f32 / n as f32;
            let mut vy = if sat_on { eff(&self.lut, (vx * pre + bias) as f64) as f32 } else { vx };
            vy = vy.clamp(-1.0, 1.0);
            let px = x + (vx + 1.0) * 0.5 * (w - 1.0);
            let py = y + (1.0 - (vy + 1.0) * 0.5) * (h - 1.0);
            self.setc(sat_on && pk > 0.002 && vx.abs() <= pk);
            if i > 0 {
                self.cv.line(lx, ly, px, py);
            }
            lx = px;
            ly = py;
        }
        if sat_on && pk > 0.002 {
            self.setc(true);
            let vy = (eff(&self.lut, (pk * pre + bias) as f64) as f32).clamp(-1.0, 1.0);
            self.cv.fill(x + (pk + 1.0) * 0.5 * (w - 1.0) - 1.0, y + (1.0 - (vy + 1.0) * 0.5) * (h - 1.0) - 1.0, 3.0, 3.0);
            let vy = (eff(&self.lut, (-pk * pre + bias) as f64) as f32).clamp(-1.0, 1.0);
            self.cv.fill(x + (1.0 - pk) * 0.5 * (w - 1.0) - 1.0, y + (1.0 - (vy + 1.0) * 0.5) * (h - 1.0) - 1.0, 3.0, 3.0);
            self.setc(false);
        }
        // SAT OUT level marker on the left edge (-24..+12 dB)
        let tot = self.sget(27);
        let ty = y + (1.0 - (tot.clamp(-24.0, 12.0) + 24.0) / 36.0) * (h - 3.0);
        self.setc(self.act == 27);
        self.cv.fill(x + 1.0, ty, 7.0, 2.0);
        if self.act == 27 {
            for i in 0..(w / 3.0).floor() as i32 {
                self.cv.fill(x + i as f32 * 3.0, ty, 1.0, 1.0);
            }
        }
        self.setc(false);
        self.cv.set_font(1);
        let s = format!("OUT {:+.1}dB", tot);
        self.cv.text(x + 3.0, y + 3.0, &s);
        let s = if self.env.shared.sat_meas.load(Ordering::Relaxed) {
            self.setc(appearance::blink(self.now));
            format!("MEAS {:.1}s", self.env.shared.sat_mt.load(Ordering::Relaxed))
        } else {
            format!("SAT {:+.1}dB", self.rd_sat)
        };
        self.cv.textr(x, y + h - 14.0, w - 3.0, &s);
        self.setc(false);
    }

    /// 12x12 pencil icon; lit while the draw view is open, bright on hover.
    fn pencil_glyph(&mut self, x: f32, y: f32, on: bool, hover: bool) {
        let prev = self.dk; // keep the bypass dimming of the surrounding module
        self.set_dk(prev * if on || hover { 1.0 } else { 0.6 });
        self.setc(on);
        let l = |s: &Self, a: f32, b: f32, c: f32, d: f32| s.cv.line(x + a, y + b, x + c, y + d);
        l(self, 8.0, 1.0, 10.0, 3.0); // eraser cap
        l(self, 8.0, 1.0, 1.0, 8.0); // body
        l(self, 10.0, 3.0, 3.0, 10.0);
        l(self, 1.0, 8.0, 0.0, 11.0); // tip
        l(self, 3.0, 10.0, 0.0, 11.0);
        self.set_dk(prev);
        self.setc(false);
    }

    /// Draw view of the custom curve (unity input, independent of drive/bias). Tools (top right, left of the close cross):
    /// symmetry (default) draws the right half and mirrors it through the origin, `f(-x) = -f(x)`, the pencil draws free.
    /// Left click empty space = add a point (and drag it), drag = move, double click = remove.
    /// The two end points are fixed in x and cannot be removed.
    fn draw_curve_edit(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let mut c = self.env.params.curve_snapshot();
        let before = c;
        // tool icons left of the close cross: free draw (pencil), symmetry
        let (pen_x, sym_x) = (x + w - 28.0, x + w - 42.0);
        let (hp, hs) = (self.hit(pen_x, y + 2.0, 12.0, 12.0), self.hit(sym_x, y + 2.0, 12.0, 12.0));
        if self.press && self.act == 0 {
            if hp && self.sym_mode {
                self.sym_mode = false;
                self.press = false;
            } else if hs && !self.sym_mode {
                // free -> symmetric keeps the odd part of the curve (what a symmetric curve can hold)
                c = Curve::from_odd_half(&c.odd_half());
                self.sym_mode = true;
                self.press = false;
            }
        }
        let sym = self.sym_mode;
        // domain [-1,1]^2 is inset so end points and the icons never collide (symmetric margins keep the centre cross true)
        let (ex, ey, ew, eh) = (x + 9.0, y + 14.0, w - 18.0, h - 28.0);
        let (px, py) = (|vx: f32| ex + (vx + 1.0) * 0.5 * (ew - 1.0), |vy: f32| ey + (1.0 - (vy + 1.0) * 0.5) * (eh - 1.0));
        let to_v = |mx: f32, my: f32| (((mx - ex) / (ew - 1.0)) * 2.0 - 1.0, 1.0 - ((my - ey) / (eh - 1.0)) * 2.0);
        // working list: the whole curve, or in symmetry mode its right half (x > 0, ending at x = 1); the left is derived
        let mut hv: Vec<(f32, f32)> = if sym { c.pts().iter().cloned().filter(|p| p.0 > 0.0).collect() } else { c.pts().to_vec() };
        if sym && hv.is_empty() {
            hv.push((1.0, 1.0));
        }
        let cap = if sym { MAX_PTS / 2 } else { MAX_PTS };
        let interior = |i: usize, n: usize| if sym { i + 1 < n } else { i > 0 && i + 1 < n };
        let xlo = |hv: &Vec<(f32, f32)>, i: usize| if i == 0 { if sym { 0.0 } else { -1.0 } } else { hv[i - 1].0 };
        // a point is picked on its own spot, or in symmetry mode on its mirror image
        let pick = |s: &Self, hv: &Vec<(f32, f32)>| -> Option<usize> {
            let mut best: Option<(usize, f32)> = None;
            for (i, &(vx, vy)) in hv.iter().enumerate() {
                let mut d = ((s.kit.mx - px(vx)).powi(2) + (s.kit.my - py(vy)).powi(2)).sqrt();
                if sym {
                    d = d.min(((s.kit.mx - px(-vx)).powi(2) + (s.kit.my - py(-vy)).powi(2)).sqrt());
                }
                if d <= 6.0 && best.map_or(true, |b| d < b.1) {
                    best = Some((i, d));
                }
            }
            best.map(|b| b.0)
        };
        let in_box = self.hit(x, y, w, h) && !self.hit(sym_x, y + 2.0, 56.0, 12.0);
        if in_box && self.press && self.act == 0 {
            let n = hv.len();
            if let Some(i) = pick(self, &hv) {
                if self.dbl(200 + i as i32) && interior(i, n) {
                    hv.remove(i);
                    self.last_clk_n = 0;
                } else {
                    self.act = 160;
                    self.cpt = i;
                }
            } else if n < cap {
                let (mut vx, mut vy) = to_v(self.mx, self.my);
                if sym && vx < 0.0 {
                    vx = -vx;
                    vy = -vy;
                }
                let at = if sym { hv.iter().position(|p| p.0 > vx) } else { (1..n).find(|&k| hv[k].0 > vx) }.unwrap_or(n - 1);
                let room = vx - xlo(&hv, at) >= MIN_GAP && hv[at].0 - vx >= MIN_GAP;
                if room {
                    hv.insert(at, (vx, vy.clamp(-1.0, 1.0)));
                    self.act = 160;
                    self.cpt = at;
                    self.last_clk_n = 0; // a creating click never counts towards a double click
                }
            }
            self.vt1 = 0;
            self.vt2 = 0;
            self.press = false;
        }
        if self.act == 160 && self.down && self.cpt < hv.len() {
            let (mut vx, mut vy) = to_v(self.mx, self.my);
            let i = self.cpt;
            if sym {
                // dragging the ghost half moves the real point the other way round
                if vx < 0.0 {
                    vx = -vx;
                    vy = -vy;
                }
            }
            let n = hv.len();
            if interior(i, n) {
                hv[i].0 = vx.clamp(xlo(&hv, i) + MIN_GAP, hv[i + 1].0 - MIN_GAP);
            }
            hv[i].1 = vy.clamp(-1.0, 1.0);
        }
        c = if sym { Curve::from_odd_half(&hv) } else { Curve::from_points(&hv) };
        if c != before {
            *self.env.params.custom_curve.lock() = c.pts().to_vec();
        }
        // identity reference (dotted), the curve, then the points
        let prev = self.dk;
        self.set_dk(prev * 0.5);
        self.setc(false);
        for i in 0..(ew / 3.0).floor() as i32 {
            let vx = -1.0 + 2.0 * i as f32 / (ew / 3.0).floor();
            self.cv.fill(px(vx), py(vx), 1.0, 1.0); // identity
            self.cv.fill(px(vx), py(1.0), 1.0, 1.0); // domain frame (+-1)
            self.cv.fill(px(vx), py(-1.0), 1.0, 1.0);
            self.cv.fill(px(1.0), py(vx), 1.0, 1.0);
            self.cv.fill(px(-1.0), py(vx), 1.0, 1.0);
        }
        self.set_dk(prev);
        let n = 96;
        let (mut lx, mut ly) = (px(-1.0), py(c.eval(-1.0) as f32));
        for i in 1..=n {
            let vx = -1.0 + 2.0 * i as f32 / n as f32;
            let (qx, qy) = (px(vx), py(c.eval(vx as f64) as f32));
            self.cv.line(lx, ly, qx, qy);
            lx = qx;
            ly = qy;
        }
        let hot = if self.act == 160 { Some(self.cpt) } else { pick(self, &hv) };
        let nh = hv.len();
        for (i, &(vx, vy)) in hv.iter().enumerate() {
            let (qx, qy) = (px(vx).round(), py(vy).round());
            self.setc(hot == Some(i));
            let end = if sym { i + 1 == nh } else { i == 0 || i + 1 == nh };
            if end {
                self.cv.boxr(qx - 2.0, qy - 2.0, 5.0, 5.0); // y-only end points
            } else {
                self.cv.fill(qx - 2.0, qy - 2.0, 5.0, 5.0);
            }
            if sym {
                // the mirrored partner: a dim outline you can grab just the same
                let (gx, gy) = (px(-vx).round(), py(-vy).round());
                let pd = self.dk;
                self.set_dk(pd * 0.6);
                self.cv.boxr(gx - 2.0, gy - 2.0, 5.0, 5.0);
                self.set_dk(pd);
            }
        }
        self.setc(false);
        // tools: pencil (free draw), symmetry
        self.pencil_glyph(pen_x, y + 2.0, !sym, hp);
        self.sym_glyph(sym_x, y + 2.0, sym, hs);
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, "DRAW");
        self.cv.textr(x, y + h - 11.0, w - 3.0, &format!("{}/{} PTS", c.n, MAX_PTS));
    }

    // ------------------------------------------------------------------ comp plot


    // ------------------------------------------------------------------ EQ page









    // ------------------------------------------------------------------ window size
    fn cur_scale(&self) -> f32 {
        self.env.params.editor_state.size().0 as f32 / W
    }

    /// Ask the host to resize the editor to `scale` x the W x H base size (aspect locked).
    fn request_scale(&mut self, scale: f32) {
        let sc = scale.clamp(SIZES[0], SIZES[SIZES.len() - 1]);
        if (sc - self.cur_scale()).abs() > 0.001 {
            self.env.params.editor_state.set_requested_size(((W * sc).round() as u32, (H * sc).round() as u32));
        }
    }

    /// Bottom-right grip: drag = aspect-locked resize in 25% steps, right-click = size menu.
    fn resize_grip(&mut self) {
        let (gx, gy, gw, gh) = (W - 16.0, H - 16.0, 16.0, 16.0);
        let hv = self.hit(gx, gy, gw, gh);
        if hv && self.press && self.act == 0 {
            self.resizing = true;
            // distance from the pointer to the window's bottom-right corner, in window points
            self.resize_off = (self.win_pt.0 - self.ptr_pt.0, self.win_pt.1 - self.ptr_pt.1);
            self.press = false;
        }
        if hv && self.rpress {
            let items = SIZES.iter().map(|v| format!("{:.0}%", v * 100.0)).collect();
            let cur = SIZES.iter().position(|v| (v - self.cur_scale()).abs() < 0.13).unwrap_or(0);
            self.open_menu(MenuKind::Size, gx - 40.0, gy - 90.0, items, cur);
        }
        if self.resizing {
            if self.down {
                // Absolute target (pointer + fixed corner offset), independent of the lagging render
                // scale. Only issue a new request once the host window has caught up with the last one,
                // otherwise stale pointer/size pairs make the target overshoot and the window loops.
                let (cw, _) = self.env.params.editor_state.size();
                let caught_up = (self.win_pt.0 - cw as f32).abs() < 1.5;
                if caught_up {
                    let w = (self.ptr_pt.0 + self.resize_off.0) / W;
                    let h = (self.ptr_pt.1 + self.resize_off.1) / H;
                    self.request_scale((w.min(h) * 4.0).round() / 4.0);
                }
            } else {
                self.resizing = false;
            }
        }
        self.set_dk(1.0);
        self.setc(self.resizing);
        for o in [4.0, 8.0, 12.0] {
            self.cv.line(W - 1.0, H - 1.0 - o, W - 1.0 - o, H - 1.0);
        }
        self.setc(false);
    }

    // ------------------------------------------------------------------ popup menu (replaces gfx_showmenu)
    /// (x, y, w, h, visible rows) of the open menu, clamped to the canvas.
    fn menu_geom(&self) -> Option<(f32, f32, f32, f32, usize)> {
        let m = self.menu.as_ref()?;
        let tw = m.items.iter().map(|i| self.cv.text_w(&i.label)).fold(0.0, f32::max);
        let rows = m.items.len().min(MENU_ROWS);
        let (mw, mh) = (tw + 26.0, MENU_ROW_H * rows as f32 + 2.0);
        let x = m.x.min(560.0 - mw).max(0.0);
        let y = m.y.min(336.0 - mh).max(0.0);
        Some((x, y, mw, mh, rows))
    }

    fn menu_frame(&mut self) {
        let Some((x, y, mw, mh, rows)) = self.menu_geom() else { return };
        let inside = self.hit(x, y, mw, mh);
        let row = ((self.my - y - 1.0) / MENU_ROW_H).floor() as i32;
        let mut chosen: Option<(i32, u8)> = None;
        let mut close = false;
        let (wheel, press, rpress) = (self.wheel, self.press, self.rpress);
        if let Some(m) = self.menu.as_mut() {
            if wheel != 0 && m.items.len() > rows {
                let max = m.items.len() - rows;
                m.scroll = (m.scroll as i32 - wheel).clamp(0, max as i32) as usize;
            }
            if press {
                if inside && row >= 0 && (row as usize) < rows {
                    if let Some(it) = m.items.get(m.scroll + row as usize) {
                        if it.kind == 0 {
                            chosen = Some((it.id, match m.kind {
                                MenuKind::Shape => 0,
                                MenuKind::Palette => 1,
                                MenuKind::Size => 2,
                                MenuKind::Preset => 3,
                            }));
                        } else {
                            return self.eat_menu_input();
                        }
                    }
                }
                close = true;
            }
            if rpress {
                close = true;
            }
        }
        if close || chosen.is_some() {
            self.menu = None;
            if let Some((id, kind)) = chosen {
                match kind {
                    0 => self.click_set(1, id as f32),
                    1 if id >= 1000 => self.set_light(!self.light),
                    1 => self.set_pal(id as usize),
                    2 => self.request_scale(SIZES[(id as usize).min(SIZES.len() - 1)]),
                    _ => self.preset_action(id),
                }
            }
        }
        self.eat_menu_input();
    }

    fn eat_menu_input(&mut self) {
        self.press = false;
        self.rpress = false;
        self.wheel = 0;
    }

    fn menu_draw(&mut self) {
        let Some((x, y, mw, mh, rows)) = self.menu_geom() else { return };
        let Some(m) = self.menu.as_ref() else { return };
        let items: Vec<MItem> = m.items.iter().skip(m.scroll).take(rows).cloned().collect();
        let checked = m.checked;
        let (more_up, more_down) = (m.scroll > 0, m.scroll + rows < m.items.len());
        self.cv.dk = 1.0;
        self.cv.setbg();
        self.cv.fill(x, y, mw, mh);
        self.setc(false);
        self.cv.boxr(x, y, mw, mh);
        self.cv.set_font(1);
        let hov = ((self.my - y - 1.0) / MENU_ROW_H).floor() as i32;
        for (i, it) in items.iter().enumerate() {
            let iy = y + 1.0 + i as f32 * MENU_ROW_H;
            match it.kind {
                2 => {
                    self.setc(false);
                    self.cv.fill(x + 4.0, iy + 6.0, mw - 8.0, 1.0);
                }
                1 => {
                    self.cv.dk = 0.5;
                    self.setc(false);
                    self.cv.text(x + 6.0, iy + 1.0, &it.label);
                    self.cv.dk = 1.0;
                }
                _ => {
                    let hot = self.hit(x, y, mw, mh) && hov == i as i32;
                    self.setc(false);
                    if hot {
                        self.cv.fill(x + 1.0, iy, mw - 2.0, MENU_ROW_H);
                        self.cv.setbg();
                    }
                    if it.id == checked {
                        self.cv.fill(x + 5.0, iy + 5.0, 4.0, 4.0);
                    }
                    self.cv.text(x + 14.0, iy + 1.0, &it.label);
                }
            }
        }
        self.setc(false);
        if more_up {
            self.cv.tri((x + mw - 10.0, y + 7.0), (x + mw - 4.0, y + 7.0), (x + mw - 7.0, y + 3.0));
        }
        if more_down {
            self.cv.tri((x + mw - 10.0, y + mh - 7.0), (x + mw - 4.0, y + mh - 7.0), (x + mw - 7.0, y + mh - 3.0));
        }
    }

    // ------------------------------------------------------------------ presets
    fn preset_name(&self) -> String {
        self.env.params.preset_name.lock().clone()
    }

    fn set_preset_name(&mut self, n: &str) {
        *self.env.params.preset_name.lock() = n.to_string();
    }

    /// Write every stored parameter of `p` (missing ids -> default). Bypass / Gain Match are left alone.
    fn apply_preset(&mut self, p: &Preset) {
        for &(n, id, mn, mx, df) in TABLE {
            if presets::EXCLUDED.contains(&id) {
                continue;
            }
            let v = p.params.get(id).copied().unwrap_or(df).clamp(mn, mx);
            self.click_set(n, v);
        }
        // absent curve = linear (deterministic: loading a preset never keeps the previous preset's curve)
        let c = Curve::from_points(&p.curve);
        *self.env.params.custom_curve.lock() = c.pts().to_vec();
        *self.env.params.asym.lock() = satcomp_dsp::shapers::asym_from(&p.asym).iter().map(|&v| v as f32).collect();
        self.set_preset_name(&p.name);
        self.base = None;
        self.hold_until = self.now + 0.6;
    }

    /// True when the live parameters differ from the loaded preset (drives the *NAME* marker).
    fn is_modified(&mut self) -> bool {
        if self.now < self.hold_until {
            return false; // host parameter writes land a few frames late after a load
        }
        let cur = self.preset_name();
        let key = if cur.is_empty() { "INIT".to_string() } else { cur };
        if self.base.as_ref().map(|b| b.0 != key).unwrap_or(true) {
            let p = if key == "INIT" && self.factory.iter().all(|f| f.name != "INIT") {
                Some(Preset::init())
            } else {
                self.preset_index().and_then(|i| self.preset_at(i))
            };
            self.base = p.map(|p| (key, p));
        }
        let Some((_, b)) = &self.base else { return false };
        for &(n, id, mn, mx, df) in TABLE {
            if presets::EXCLUDED.contains(&id) {
                continue;
            }
            let want = b.params.get(id).copied().unwrap_or(df).clamp(mn, mx);
            let have = self.sget(n);
            if (want - have).abs() > 2e-4 * want.abs().max(have.abs()) + 1e-4 {
                return true;
            }
        }
        Curve::from_points(&b.curve) != self.env.params.curve_snapshot() || satcomp_dsp::shapers::asym_from(&b.asym) != self.env.params.asym_snapshot()
    }

    fn capture(&self, name: &str) -> Preset {
        let params = TABLE
            .iter()
            .filter(|e| !presets::EXCLUDED.contains(&e.1))
            .map(|e| (e.1.to_string(), self.sget(e.0)))
            .collect();
        let mut p = Preset::new(name, params);
        let c = self.env.params.curve_snapshot();
        if c != Curve::linear() {
            p.curve = c.pts().to_vec();
        }
        let a = self.env.params.asym_snapshot();
        if a.iter().any(|&v| v != 100.0) {
            p.asym = a.iter().map(|&v| v as f32).collect();
        }
        p
    }

    fn preset_count(&self) -> usize {
        self.factory.len() + self.user.len()
    }

    fn preset_at(&self, i: usize) -> Option<Preset> {
        if i < self.factory.len() {
            Some(self.factory[i].clone())
        } else {
            let (name, path) = self.user.get(i - self.factory.len())?;
            presets::load_file(path).map(|mut p| {
                p.name = name.clone();
                p
            })
        }
    }

    fn preset_index(&self) -> Option<usize> {
        let cur = self.preset_name();
        let cur = if cur.is_empty() { "INIT".to_string() } else { cur };
        self.factory
            .iter()
            .position(|p| p.name == cur)
            .or_else(|| self.user.iter().position(|u| u.0 == cur).map(|i| i + self.factory.len()))
    }

    fn step_preset(&mut self, dir: i32) {
        self.user = presets::scan_user();
        let n = self.preset_count() as i32;
        let next = match self.preset_index() {
            Some(i) => (i as i32 + dir).rem_euclid(n),
            None => if dir > 0 { 0 } else { n - 1 },
        };
        if let Some(p) = self.preset_at(next as usize) {
            self.apply_preset(&p);
        }
    }

    fn open_preset_menu(&mut self, x: f32, y: f32) {
        self.user = presets::scan_user();
        let mut items = vec![
            MItem { label: "Initialize preset".into(), kind: 0, id: 1 },
            MItem { label: "Load preset...".into(), kind: 0, id: 0 },
            MItem { label: String::new(), kind: 2, id: -1 },
            MItem { label: "FACTORY".into(), kind: 1, id: -1 },
        ];
        // factory[0] is the built-in INIT: it is the "Initialize preset" entry above, so not listed twice
        for (i, p) in self.factory.iter().enumerate().skip(1) {
            items.push(MItem { label: p.name.clone(), kind: 0, id: 1000 + i as i32 });
        }
        if !self.user.is_empty() {
            items.push(MItem { label: String::new(), kind: 2, id: -1 });
            items.push(MItem { label: "USER".into(), kind: 1, id: -1 });
            for (i, u) in self.user.iter().enumerate() {
                items.push(MItem { label: u.0.clone(), kind: 0, id: 2000 + i as i32 });
            }
        }
        let checked = match self.preset_index() {
            Some(0) => 1,
            Some(i) if i < self.factory.len() => 1000 + i as i32,
            Some(i) => 2000 + (i - self.factory.len()) as i32,
            None => -1,
        };
        self.menu = Some(Menu { kind: MenuKind::Preset, x, y, items, checked, scroll: 0 });
    }

    fn preset_action(&mut self, id: i32) {
        match id {
            0 => self.pending = 1,
            1 => self.apply_preset(&Preset::init()),
            1000..=1999 => {
                if let Some(p) = self.preset_at((id - 1000) as usize) {
                    self.apply_preset(&p);
                }
            }
            2000.. => {
                if let Some(p) = self.preset_at(self.factory.len() + (id - 2000) as usize) {
                    self.apply_preset(&p);
                }
            }
            _ => {}
        }
    }

    /// File dialogs run on a worker thread: a modal NSSavePanel started inside our frame callback re-enters
    /// the host window's event handling (resign-key) while the editor state is borrowed, which aborted REAPER.
    /// rfd dispatches the panel to the main thread itself; the result is picked up by `poll_dialog`.
    fn run_pending(&mut self) {
        let what = std::mem::take(&mut self.pending);
        if what == 0 || self.dlg.is_some() {
            return;
        }
        let slot = Arc::new(parking_lot::Mutex::new(Dlg::Pending));
        self.dlg = Some(slot.clone());
        match what {
            1 => {
                std::thread::spawn(move || {
                    let r = presets::open_dialog().and_then(|path| {
                        presets::load_file(&path).map(|mut p| {
                            if let Some(stem) = path.file_stem() {
                                p.name = stem.to_string_lossy().into_owned();
                            }
                            p
                        })
                    });
                    *slot.lock() = r.map_or(Dlg::Cancelled, Dlg::Loaded);
                });
            }
            2 => {
                let cur = self.preset_name();
                let def = if cur.is_empty() || cur == "INIT" { "My preset".to_string() } else { cur };
                let snapshot = self.capture(&def); // parameters as of the click
                std::thread::spawn(move || {
                    let r = presets::save_dialog(&def).and_then(|path| {
                        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(def);
                        let mut p = snapshot;
                        p.name = name.clone();
                        presets::save_file(&path, &p).ok().map(|_| name)
                    });
                    *slot.lock() = r.map_or(Dlg::Cancelled, Dlg::Saved);
                });
            }
            _ => self.dlg = None,
        }
    }

    /// Apply the outcome of a finished file dialog (called every frame).
    fn poll_dialog(&mut self) {
        let Some(slot) = self.dlg.clone() else { return };
        let r = std::mem::replace(&mut *slot.lock(), Dlg::Pending);
        match r {
            Dlg::Pending => return,
            Dlg::Loaded(p) => self.apply_preset(&p),
            Dlg::Saved(name) => {
                self.set_preset_name(&name);
                self.base = None;
                self.user = presets::scan_user();
            }
            Dlg::Cancelled => {}
        }
        self.dlg = None;
    }

    /// Header preset bar (save icon | name | prev next) and the window-scale button.
    fn preset_bar(&mut self) {
        let (y, h) = (32.0, 16.0); // the logo box ends at 29: a 3 unit gap above, 4 to the scope below
        // save icon
        if self.btn(10.0, y, 26.0, h, "", 0) {
            self.pending = 2;
        }
        self.setc(false);
        self.cv.boxr(18.0, y + 3.0, 10.0, 10.0);
        self.cv.fill(21.0, y + 3.0, 4.0, 3.0);
        self.cv.boxr(20.0, y + 8.0, 6.0, 5.0);
        // name -> dropdown
        let name = self.preset_name();
        let shown = if name.is_empty() || name == "INIT" { "-- INIT --".to_string() } else { name };
        let hv = self.hit(36.0, y, 244.0, h);
        if hv && self.press && self.act == 0 {
            self.open_preset_menu(36.0, y + h);
            self.press = false;
        }
        self.setc(false);
        self.cv.boxr(36.0, y, 244.0, h);
        self.cv.set_font(2);
        let modified = self.is_modified();
        let mut t = shown;
        while self.cv.text_w(&t) > if modified { 226.0 } else { 238.0 } && t.chars().count() > 3 {
            t.pop();
        }
        if modified {
            t = format!("*{t}*");
        }
        self.cv.textc(36.0, y + ((h - 8.0) * 0.5).floor(), 244.0, &t);
        // prev / next
        for (i, dir) in [(0, -1), (1, 1)] {
            let x = 280.0 + i as f32 * 25.0;
            if self.btn(x, y, 25.0, h, "", 0) {
                self.step_preset(dir);
            }
            self.setc(false);
            let (cx, cy) = (x + 12.5, y + h * 0.5);
            if dir < 0 {
                self.cv.tri((cx + 2.0, cy - 3.0), (cx + 2.0, cy + 3.0), (cx - 2.0, cy));
            } else {
                self.cv.tri((cx - 2.0, cy - 3.0), (cx - 2.0, cy + 3.0), (cx + 2.0, cy));
            }
        }
    }

    // ------------------------------------------------------------------ about page
    /// Signal-flow diagram: IN > SAT > CLIP > LIM > OUT GAIN > OUT. Dots move along the path; their size follows the live
    /// output level (the stages before the limiter add the gain reduction back, so they read hotter).
    fn draw_flow_at(&mut self, x: f32, y: f32, w: f32, h: f32, byp: bool) {
        let prev = self.dk;
        let sh = self.env.shared;
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        self.cv.set_font(1);
        self.cv.text(x + 6.0, y + 5.0, "SIGNAL FLOW");
        let lvf = |v: f32| ((8.6858896 * v.max(1e-7).ln() + 48.0) / 48.0).clamp(0.0, 1.0);
        let lo = lvf(self.ovl.max(self.ovr));
        let gr = self.gr_disp;
        let lp = (lo + gr / 48.0).min(1.0);
        let sig = lo > 0.01;
        let sat_on = sh.sat_vz.load(Ordering::Relaxed) > 0.02;
        let clip_on = self.sget(11) > 0.5;
        let names = ["IN", "SAT", "CLIP", "LIM", "OUT GAIN", "OUT"];
        let bw = [26.0, 34.0, 38.0, 34.0, 58.0, 30.0];
        let lit = [false, sat_on, clip_on, gr > 0.1, false, false];
        let n = names.len();
        let gap = (w - 24.0 - bw.iter().sum::<f32>()) / (n - 1) as f32;
        let (cy, bh) = (y + 50.0, 22.0);
        let mut bx = [0.0f32; 6];
        let mut cx = x + 12.0;
        for i in 0..n {
            bx[i] = cx;
            cx += bw[i] + gap;
        }
        let phase = ((self.now * 26.0) as f32) % 6.0;
        let dot = |c: &Canvas, px: f32, py: f32, lv: f32| {
            if sig && lv > 0.02 {
                let s = (1.0 + lv * 2.99).floor();
                c.fill((px - s * 0.5).floor(), (py - s * 0.5).floor() + 0.5, s, s);
            }
        };
        self.setc(false);
        for i in 0..n - 1 {
            let (g0, g1) = (bx[i] + bw[i], bx[i + 1]);
            self.cv.fill(g0, cy, g1 - g0 - 3.0, 1.0);
            self.cv.tri((g1 - 4.0, cy - 3.0), (g1 - 4.0, cy + 4.0), (g1, cy + 0.5));
            let lv = if i < 3 { lp } else { lo };
            let mut px = g0 + phase;
            while px < g1 - 4.0 {
                dot(&self.cv, px, cy, lv);
                px += 6.0;
            }
        }
        for i in 0..n {
            let (bxl, byt) = (bx[i], cy - bh * 0.5 + 0.5);
            if lit[i] {
                self.setc(false);
                self.cv.fill(bxl, byt, bw[i], bh);
                self.cv.setbg();
            } else {
                self.setc(false);
                self.cv.fill(bxl + 1.0, byt + 1.0, bw[i] - 2.0, bh - 2.0); // clear the line under the box
                self.cv.setbg();
                self.cv.fill(bxl + 1.0, byt + 1.0, bw[i] - 2.0, bh - 2.0);
                self.setc(false);
                self.cv.boxr(bxl, byt, bw[i], bh);
            }
            self.cv.textc(bxl, byt + 7.0, bw[i], names[i]);
            self.setc(false);
        }
        self.set_dk(prev);
        let s = format!("GR {:.1}", gr);
        self.cv.textc(bx[3] - 10.0, cy - bh * 0.5 - 14.0, bw[3] + 20.0, &s);
        self.cv.text(x + 6.0, y + h - 15.0, if sig { "DOT SIZE = LEVEL" } else { "PLAY AUDIO TO SEE THE FLOW" });
        let _ = byp;
    }

    // ------------------------------------------------------------------ main frame
    pub fn frame(&mut self) {
        let sh = self.env.shared;
        // Theme editor: every widget draws as a dummy with the input masked; only the editor cells and the picker get it.
        let editing = self.app.editing();
        self.begin_frame_editing();
        if editing {
            self.logo_pend = -1.0;
            self.curve_edit = false;
            self.menu = None;
        }
        // readouts refresh ~8x/s
        self.rd_tick = self.now - self.rd_last >= 0.12;
        if self.rd_tick {
            self.rd_last = self.now;
            self.rd_gr = self.gr_disp;
            self.rd_clip = self.clip_disp;
            self.rd_pk = self.opk_max;
            self.rd_rms = sh.fms.load(Ordering::Relaxed);
            self.rd_sat = sh.sat_db.load(Ordering::Relaxed);
        }
        // a finished sat level match writes SAT GAIN (the audio thread cannot set parameters)
        if sh.sat_result_new.swap(false, Ordering::Acquire) {
            let v = sh.sat_result.load(Ordering::Relaxed);
            self.click_set(27, v);
        }
        self.refresh_dsp();
        self.menu_frame();

        // drag update (single active widget). act 1..99 = slider number (vertical drag); act >= 100 = viewport drags
        if self.act != 0 && self.act < 100 {
            let a = self.act as usize;
            if self.down {
                let t_new = (self.act_t + (self.last_my - self.my) * if self.shift { 0.0008 } else { 0.006 }).clamp(0.0, 1.0);
                self.act_t = t_new;
                self.commit(a, t_new);
            } else {
                self.end_touch(a);
                self.act = 0;
            }
        }
        if self.act >= 100 && !self.down {
            let (v1, v2) = (self.vt1, self.vt2);
            if v1 > 0 {
                self.end_touch(v1);
            }
            if v2 > 0 {
                self.end_touch(v2);
            }
            self.act = 0;
        }
        self.last_my = self.my;

        let byp = self.sget(26) > 0.5;
        let look = self.sget(12) > 0.5;

        // ---------- header row: logo, title, size, DELTA, BYPASS
        self.set_dk(1.0);
        self.logo(13.0, 5.0);
        self.setc(false);
        self.cv.set_font(2);
        let title = format!("SATLIM {}", version_short());
        let tw = self.cv.text_w(&title);
        if self.hit(42.0, 5.0, tw + 4.0, 21.0) && self.press && self.act == 0 {
            self.app.about = if self.app.about == 0 { 1 } else { 0 };
            self.app.about_scroll = 0;
            self.press = false;
        }
        // hovering highlights the title (a UI-coloured rectangle behind it, the text in the background colour): it opens About
        if self.hit(42.0, 5.0, tw + 4.0, 21.0) {
            self.cv.fill(42.0, 8.0, tw + 4.0, 15.0);
            self.cv.setbg();
        }
        self.cv.text(44.0, 12.0, &title);
        self.setc(false);
        let label = format!("{:.0}%", self.cur_scale() * 100.0);
        if self.btn(176.0, 7.0, 42.0, 17.0, &label, 0) {
            let cur = SIZES.iter().position(|v| (v - self.cur_scale()).abs() < 0.13).unwrap_or(0);
            let items = SIZES.iter().map(|v| format!("{:.0}%", v * 100.0)).collect();
            self.open_menu(MenuKind::Size, 176.0, 24.0, items, cur);
        }
        let delta = sh.delta_on.load(Ordering::Relaxed);
        if self.btn(222.0, 7.0, 50.0, 17.0, "DELTA", if delta { 2 } else { 0 }) {
            sh.delta_on.store(!delta, Ordering::Relaxed);
        }
        if self.btn(274.0, 7.0, 56.0, 17.0, "BYPASS", byp as u8) {
            self.click_set(26, if byp { 0.0 } else { 1.0 });
        }

        // ---------- preset row (the About tabs take its place while the About page is open)
        let ab = self.app.about > 0;
        if !ab {
            self.preset_bar();
        }
        self.set_dk(if byp { 0.5 } else { 1.0 });
        if ab {
            self.draw_about(Rect::new(10.0, 54.0, 320.0, 438.0), byp);
        }

        if !ab {
            // ---------- transient scope (what the clipper and limiter removed stacks on the output in the accent colour)
            self.draw_scope(10.0, 52.0, 320.0, 112.0);
            self.scope_row(10.0, 163.0, 320.0);

            // ---------- SAT module
            self.selector(1, 10.0, 188.0, 130.0, 16.0);
            self.draw_plot(10.0, 208.0, 130.0, 156.0);
            // the theme editor's dummy page keeps the plot, the meters (shorter) and the limiter readouts; its cell group
            // takes the place of the sat cells
            let mh = if editing { 146.0 } else { 282.0 };
            if editing {
                self.theme_cells(CellGrid { x0: 10.0, y0: 368.0, cw: 62.0, ch: 38.0, px: 68.0, py: 42.0 });
            } else {
                self.cell(2, 10.0, 368.0, 62.0, 38.0, "DRIVE");
                self.bias_asym(78.0, 368.0, 62.0, 38.0);
                self.cell2(4, 5, 10.0, 410.0, 62.0, 38.0, "HP", "LP");
                self.cell(6, 78.0, 410.0, 62.0, 38.0, "TONE");
                self.cell(25, 10.0, 452.0, 62.0, 38.0, "SAT MIX");
                self.cell_o(27, 78.0, 452.0, 62.0, 38.0, "SAT GAIN");
            }
            // sat out, GR and output meters
            self.sat_vu(146.0, 208.0, 20.0, mh);
            self.draw_meter(170.0, 208.0, 24.0, mh);
            self.out_vu(198.0, 208.0, 26.0, mh);
        }

        // ---------- limiter column: oversampling, readouts, L.AHEAD, cells (the thin About layout hides them)
        self.set_dk(1.0);
        let (cx, cw) = (232.0, 98.0);
        if !ab {
            // OVERSAMPL sits on the shape selector's row, centred over the meter bars; x1..x8 share that row at the column width
            self.cv.set_font(1);
            self.set_dk(if byp { 0.5 } else { 1.0 }); // full UI colour like the other components, dimmed on bypass like them
            self.setc(false);
            self.cv.textc(146.0, 192.0, 78.0, "OVERSAMPLE");
            self.set_dk(if byp { 0.5 } else { 1.0 });
            let os_i = (self.sget(7) + 0.5).floor() as i32;
            for i in 0..4 {
                let l = ["x1", "x2", "x4", "x8"][i as usize];
                let (bx, bx2) = (cx + (cw * i as f32 / 4.0).floor(), cx + (cw * (i + 1) as f32 / 4.0).floor());
                if self.btn(bx, 188.0, bx2 - bx + 1.0, 16.0, l, (os_i == i) as u8) {
                    self.click_set(7, i as f32);
                }
            }
            self.set_dk(1.0);
            self.cv.set_font(1);
            // GR (the limiter) and CLIP (the clipper) stay in the UI colour at all times; only the true peak turns accent on a clip
            self.setc(false);
            self.cv.text(cx, 214.0, "GR");
            let s1 = if byp { "-----dB".to_string() } else { format!("{:04.1}dB", self.rd_gr) };
            self.cv.textr(cx, 214.0, cw, &s1);
            self.cv.text(cx, 225.0, "CLIP");
            let s1 = if byp { "-----dB".to_string() } else { format!("{:04.1}dB", self.rd_clip) };
            self.cv.textr(cx, 225.0, cw, &s1);
            self.setc(self.rd_pk > 1.0);
            self.cv.text(cx, 236.0, "TRU.PK");
            let s1 = if self.rd_pk > 0.00001 { format!("{:+05.1}dB", 8.6858896 * self.rd_pk.ln()) } else { "-----dB".into() };
            self.cv.textr(cx, 236.0, cw, &s1);
            self.setc(false);
            self.cv.text(cx, 247.0, "RMS");
            let s1 = if self.rd_rms > 0.000000001 { format!("{:+05.1}dB", 4.3429448 * self.rd_rms.ln()) } else { "-----dB".into() };
            self.cv.textr(cx, 247.0, cw, &s1);

            self.set_dk(if byp { 0.5 } else { 1.0 });
            if self.btn(cx, 262.0, cw, 16.0, if look { "L.AHEAD  ON" } else { "L.AHEAD OFF" }, look as u8) {
                self.click_set(12, if look { 0.0 } else { 1.0 });
            }
            self.cell(8, cx, 284.0, cw, 38.0, "CEILING");
            if !editing {
                self.cell(9, cx, 326.0, cw, 38.0, "THRESHOLD");
                self.cell(11, cx, 368.0, cw, 38.0, "CLIP");
                self.cell(10, cx, 410.0, cw, 38.0, "RELEASE");
                self.cell_o(22, cx, 452.0, cw, 38.0, "OUT GAIN");
            }
        }

        self.resize_grip();
        self.menu_draw();
        self.poll_dialog();
        self.run_pending();
        if editing {
            self.unmask_in();
            let osk = self.app.osk_open();
            if osk {
                self.mask_in(); // picker stays visible underneath but inert
            }
            self.theme_picker(Rect::new(10.0, 52.0, 320.0, 129.0));
            if osk {
                self.unmask_in();
                self.theme_osk(Rect::new(10.0, 52.0, 320.0, 129.0));
            }
        }
    }
}
