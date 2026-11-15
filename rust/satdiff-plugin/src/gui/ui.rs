//! SatDiff editor (first pass, layout per docs/feedbacknotes/satdiffpass1.png): the SatComp sat module (plot, draw view, cells)
//! on the shared widget kit, the phase section, morph, tile, the vertical meters and the spectrum / scope viz. Immediate mode
//! with the same `act` / `press` drag logic as SatComp. Controls whose DSP is not built yet are live parameters with no sound.

use satcomp_ui::canvas::Canvas;
use satcomp_ui::kit::{Kit, Meta, Widgets};
use satcomp_ui::appearance::{self, AppState, AppearanceUi, CellGrid, Rect};
use crate::params::{SatDiffParams, SLOTS, TABLE};
use crate::presets::{self, Preset};
use crate::Shared;
use super::{H, W};
use satcomp_ui::spectrum::Spectrum;
use nih_plug::prelude::*;
use satcomp_dsp::shapers::{Curve, ShaperLut, CUSTOM, MAX_BEND, MIN_GAP, PT_CAP, SHAPER_NAMES};

/// Points the drawn curve may have (15 per half in the odd / even views).
const DRAW_PTS: usize = PT_CAP;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;
use std::sync::Arc;

enum MenuKind {
    Shape,
    Palette,
    Size,
    Preset,
    Fx,
    Os,
    Morph,
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

/// colour-fx types in the dropdown (only the diffusor exists so far)
const FX_NAMES: [&str; 4] = ["DIFFUSOR", "PHASER", "FLANGER", "RINGMOD"];
/// oscilloscope windows (ms). Under 500 ms the trace scrolls like a normal scope (newest at the top); from 500 ms it is a hard
/// wipe that waits for the next transient after each sweep (retrigger) so peaks land on the same row, and free-runs after one
/// window without one. Both gather the tap samples frame by frame, so any length works.
const VIZ_WIN_MS: [u32; 11] = [5, 10, 20, 40, 80, 160, 250, 500, 1000, 2000, 4000];

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

#[allow(dead_code)] // the horizontal transient scope is not part of the pass-1 layout; kept for later
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
    pub dummy_params: Arc<SatDiffParams>,
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
    // pass-1 layout state
    /// input meter (left of the plot): decaying levels, peak holds, clip lamp time
    ivl: f32,
    ivr: f32,
    ihl_l: f32,
    ihl_r: f32,
    ihl_l_t: f64,
    ihl_r_t: f64,
    iclip_t: f64,
    /// vertical viz: 0 spectrum, 1 oscilloscope; scope window step
    viz_mode: u8,
    viz_win: usize,
    spec: Spectrum,
    /// scope wipe: per row (min, max), the row being written, samples gathered for it, last tap head
    sc_rows: Vec<(f32, f32)>,
    sc_row: usize,
    sc_acc: f64,
    sc_cur: (f32, f32),
    sc_head: usize,
    /// retrigger: waiting for a transient, samples waited, slow level follower
    sc_wait: bool,
    sc_scroll: bool,
    sc_wait_n: f64,
    sc_env: f32,
    /// draw view: LINEAR / CURVE segment toggle (UI only for now) and the even-mirror tool (with `sym_mode`)
    /// curve handle being dragged (segment index in the working list) and whether it was grabbed on the mirrored ghost
    chd: usize,
    chg: bool,
    even: bool,
    // sat plot
    lut: ShaperLut,
    /// the other scene's table (the faint underlay in the plot)
    lut2: ShaperLut,
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
            self.ihl_l_t = now;
            self.ihl_r_t = now;
            self.iclip_t = -10.0;
            self.spec.rebase(now);
            self.gr_hold_t = now;
            self.sat_clip_t = -10.0;
            self.hold_until = now;
            self.scope_t = now;
        }
    }

    pub fn new(pal: usize, light: bool, face: usize, params: &SatDiffParams) -> Self {
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
        sp(30, 0, 0, 1.0);
        sp(34, 1, 0, 1.0);
        sp(35, 0, 0, 0.01);
        sp(37, 0, 0, 1.0);
        sp(40, 0, 0, 1.0);
        sp(41, 0, 0, 1.0);
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
            dummy_params: Arc::new(SatDiffParams::from_defaults(None)),
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
            ivl: 0.0,
            ivr: 0.0,
            ihl_l: 0.0,
            ihl_r: 0.0,
            ihl_l_t: 0.0,
            ihl_r_t: 0.0,
            iclip_t: -10.0,
            viz_mode: 1, // oscilloscope first; right-click flips to the spectrum
            viz_win: 8,
            sc_scroll: false,
            sc_wait: false,
            sc_wait_n: 0.0,
            sc_env: 0.0,
            spec: Spectrum::new(),
            sc_rows: Vec::new(),
            sc_row: 0,
            sc_acc: 0.0,
            sc_cur: (0.0, 0.0),
            sc_head: 0,
            chd: 0,
            chg: false,
            even: false,
            lut: ShaperLut::new(0),
            lut2: ShaperLut::new(0),
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
    pub fn sync_look(&mut self, params: &SatDiffParams) {
        let (pal, light) = initial_palette(params);
        self.pal = pal;
        self.light = light;
        self.face = (params.font.load(Ordering::Relaxed).max(0) as usize).min(satcomp_ui::canvas::FONT_NAMES.len() - 1);
        self.scope_cfg = params.scope.load(Ordering::Relaxed);
        self.restore_custom(params);
    }

    /// A project that used a user theme: resolve it from the themes file or the project copy.
    pub fn restore_custom(&mut self, params: &SatDiffParams) {
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
    pub params: &'a SatDiffParams,
    pub setter: &'a ParamSetter<'a>,
    pub shared: &'a Shared,
    pub dsp: satdiff_dsp::DiffParams,
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
        "satdiff"
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
        None // no manual yet (alpha)
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
    &["Waveshaping saturator with a drawable curve and phase tools (alpha)", "Phase rotator and diffusor are coming in the next builds."],
    &["Use tube with low drive for warm saturation, opt for destruction with tri-fold, or annihilate with a custom asymmetric waveshape.", "[Click] the top-right pencil icon to enter custom draw mode.", "[Click] the lower-right SAT value to auto-match the gain."],
    &["LIMIT is a true-peak output limiter. GAIN MATCH trims the output back to the input level."],
    &["Transient/GR meter: solid = limiter gain reduction."],
    &["[Double-click] slider resets the parameter to its default value", "[Click] GR meter cycles its range", "[Click] clip meter resets the readout", "[Left/Right-click] top-left icon cycles the colour theme", "[Double-click] top-left icon switches between dark and light mode"],
];

/// Changelog, newest first. Keep each entry to two short sentences or fewer.
const CHANGELOG: &[(&str, &[&str])] = &[
    ("v0.1", &["First build: the SatComp saturator without the compressor and EQ, OUT GAIN, GAIN MATCH and an optional true-peak limiter.", "The phase rotator and the diffusor follow."]),
];

/// Crate version for the header: "0.4" for 0.4.0, "0.4.1" for a patch release.
/// Palette index and light flag to start with (`palette-v2`; -1 = not set yet = the first scheme, dark).
pub fn initial_palette(p: &crate::params::SatDiffParams) -> (usize, bool) {
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

    // ------------------------------------------------------------------ scenes
    /// 1 while scene B is the one being edited (the A | B switch), else 0.
    fn ab(&self) -> i32 {
        (self.sget(42) + 0.5).floor() as i32
    }

    /// Parameter slot of the edited scene's shaper / tile: scene A's (1, 38) or scene B's (44, 45). Presets and the modified marker
    /// address both scenes by slot directly, only the editing widgets go through these.
    fn sc_shape(&self) -> usize {
        if self.ab() == 1 {
            44
        } else {
            1
        }
    }

    fn sc_tile(&self) -> usize {
        if self.ab() == 1 {
            45
        } else {
            38
        }
    }

    // ------------------------------------------------------------------ formatting
    fn fmt_slot(&self, n: usize, v: f32) -> String {
        match n {
            2 | 25 | 11 | 36 | 37 | 40 | 41 | 43 => format!("{:.0}%", v),
            30 => format!("{:+.0}\u{b0}", v),
            34 => {
                if v >= 10000.0 {
                    format!("{:.1}k", v * 0.001) // "20.0k": the cell is only 72 wide
                } else if v >= 1000.0 {
                    format!("{:.2}k", v * 0.001)
                } else {
                    format!("{:.0}Hz", v)
                }
            }
            35 => format!("{:.2}", v),
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
        let aw = 20.0;
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
        // scene B is edited / heard: the selector is drawn inverted (filled), everything on it in the background colour
        let inv = self.sget(42) > 0.5;
        self.setc(false);
        if inv {
            self.cv.fill(x, y, w, h);
            self.cv.setbg();
        } else {
            self.cv.boxr(x, y, w, h);
        }
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
    /// Limiter gain-reduction bookkeeping (the readouts and the LIMIT button show it; there is no GR bar in the layout).
    fn gr_update(&mut self) {
        let g = self.env.shared.gr_pk.swap(0.0, Ordering::Relaxed);
        self.gr_disp = g.max(self.gr_disp * 0.9);
        self.sat_lv = self.env.shared.sat_pk.swap(0.0, Ordering::Relaxed); // drained so it cannot saturate
    }

    /// Dry input level, vertical, hatched (every other row) so it reads apart from the solid output meter. Same ranges as the
    /// output meter (click the bars to cycle them); the IN lamp on top lights for 1 s after any sample over 0 dBFS.
    fn in_vu(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let sh = self.env.shared;
        let pa = sh.ipk[0].swap(0.0, Ordering::Relaxed);
        let pb = sh.ipk[1].swap(0.0, Ordering::Relaxed);
        if pa.max(pb) >= 1.0 {
            self.iclip_t = self.now;
        }
        let lamp = 18.0;
        let (by, bh) = (y + lamp + 1.0, h - lamp - 1.0);
        if self.hit(x, by, w, bh) && self.press && self.act == 0 {
            self.vu_ri = (self.vu_ri + 1) % VU_RANGES.len();
            self.press = false;
        }
        self.ivl = pa.max(self.ivl * 0.85);
        self.ivr = pb.max(self.ivr * 0.85);
        let now = self.now;
        if pa >= self.ihl_l {
            self.ihl_l = pa;
            self.ihl_l_t = now;
        } else if now - self.ihl_l_t > 1.0 {
            self.ihl_l *= 0.9;
        }
        if pb >= self.ihl_r {
            self.ihl_r = pb;
            self.ihl_r_t = now;
        } else if now - self.ihl_r_t > 1.0 {
            self.ihl_r *= 0.9;
        }
        let (rng, labels) = VU_RANGES[self.vu_ri];
        let bw = ((w - 3.0) * 0.5).floor();
        let lvf = |v: f32| ((8.6858896 * v.max(0.0000001).ln() + rng) / rng).clamp(0.0, 1.0);
        self.setc(false);
        self.cv.boxr(x, by, w, bh);
        for (i, (lv, hl)) in [(self.ivl, self.ihl_l), (self.ivr, self.ihl_r)].into_iter().enumerate() {
            let bx = x + 1.0 + i as f32 * (bw + 1.0);
            let fh = ((bh - 2.0) * lvf(lv)).floor();
            let mut ry = by + bh - 2.0;
            let top = by + bh - 1.0 - fh;
            while ry >= top {
                self.cv.fill(bx, ry, bw, 1.0); // hatch: every other row
                ry -= 2.0;
            }
            if hl > 0.0001 {
                self.cv.fill(bx, by + bh - 2.0 - ((bh - 3.0) * lvf(hl)).floor(), bw, 1.0);
            }
        }
        self.cv.set_font(1);
        for &db in labels {
            let ty = by + bh - 1.0 - ((bh - 2.0) * ((db as f32 + rng) / rng)).floor();
            self.setc(false);
            self.cv.fill(x + w, ty, 3.0, 1.0);
        }
        let clip = self.now - self.iclip_t < 1.0;
        self.setc(clip);
        if clip {
            self.cv.fill(x, y, w, lamp);
            self.cv.setbg();
        } else {
            self.cv.boxr(x, y, w, lamp);
        }
        self.cv.set_font(1);
        self.cv.textc(x, y + ((lamp - 8.0) * 0.5).floor(), w, "IN");
        self.setc(false);
    }

    /// Stereo output level, vertical: two bars with peak holds over a selectable range (click the bars to cycle 48 / 24 / 12 /
    /// 6 dB), small dB labels in the background colour on top of the bars, and a clip lamp at the top (accent after any over
    /// 0 dBFS; click it to reset the lamp and the peak readout). The true / sample peak readout is tracked here too.
    fn out_vu(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let sh = self.env.shared;
        let pa = sh.opk[0].swap(0.0, Ordering::Relaxed);
        let pb = sh.opk[1].swap(0.0, Ordering::Relaxed);
        let tp = pa.max(pb); // sample peak: SatDiff has no true-peak readout
        if sh.clip.swap(false, Ordering::Relaxed) {
            self.clip_lat = true;
            self.clip_t = self.now;
        }
        let lamp = 18.0;
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
            self.cv.setbg();
        } else {
            self.cv.boxr(x, y, w, lamp);
        }
        self.cv.set_font(1);
        self.cv.textc(x, y + ((lamp - 8.0) * 0.5).floor(), w, "OUT");
        self.setc(false);
    }

    // ------------------------------------------------------------------ sat plot
    /// ASYM: the current shaper's share of its even part (the part that carries DC and sub-audio pulses): one value per
    /// shaper, stored in the project / presets (not a host parameter). Drag, wheel, double-click = 100 %.
    fn asym_cell(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let shape = ((self.sget(self.sc_shape()) + 0.5).floor() as i32).clamp(0, CUSTOM) as usize;
        let cell = self.env.params.asym.clone();
        let get = || cell.lock().get(shape).copied().unwrap_or(100.0);
        let set = |v: f32| {
            let mut g = cell.lock();
            if g.len() <= shape {
                g.resize(SHAPER_NAMES.len(), 100.0);
            }
            g[shape] = v.clamp(0.0, 100.0);
        };
        let hb = self.hit(x, y, w, h);
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
        let on = self.act == ACT_ASYM;
        self.setc(on);
        self.cv.boxr(x, y, w, h);
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, "ASYM");
        self.cv.set_font(2);
        let v = get();
        self.cv.textc(x, y + (h * 0.4 + 1.0).floor(), w, &format!("{:.0}%", v));
        self.cv.fill(x + 2.0, y + h - 4.0, (((v * 0.01) * (w - 4.0)).floor()).max(1.0), 2.0);
        self.setc(false);
    }

    /// BIAS slider (top half) with the three tile icons under it (bottom half): OFF (diagonal), MIRROR (triangle), REPEAT
    /// (saw). Tile is stored per scene later; for now it is one host parameter and has no sound yet.
    fn bias_tile(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let hh = (h * 0.5).floor();
        let ha = self.hit(x, y, w, hh);
        if ha && self.press && self.act == 0 {
            self.begin_drag(3);
        }
        if ha && self.wheel != 0 {
            self.wheel_adj(3);
        }
        let act = self.act;
        self.setc(act == 3);
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
        self.setc(false);
        // tile segments: dimmed and inert on the periodic shapers (Tri Fold, Sine Fold, Wrap, Asym Fold already tile themselves)
        let (tile_slot, shape_slot) = (self.sc_tile(), self.sc_shape());
        let applies = satcomp_dsp::shapers::tile_applies((self.sget(shape_slot) + 0.5).floor() as i32);
        let keep_dk = self.dk;
        if !applies {
            self.set_dk(keep_dk * 0.35);
        }
        let sel = if applies { (self.sget(tile_slot) + 0.5).floor() as i32 } else { -1 };
        let (sy, sh) = (y + hh + 1.0, h - hh - 1.0);
        for i in 0..3i32 {
            let sx = x + (w * i as f32 / 3.0).floor();
            let sx2 = x + (w * (i + 1) as f32 / 3.0).floor();
            let sw = sx2 - sx;
            let hv = self.hit(sx, sy, sw, sh) && applies;
            if hv && self.press && self.act == 0 {
                self.click_set(tile_slot, i as f32);
                self.press = false;
            }
            let lit = sel == i || hv;
            if lit {
                self.setc(false);
                self.cv.fill(sx + if i > 0 { 1.0 } else { 0.0 }, sy, sw - if i > 0 { 1.0 } else { 0.0 } - if i < 2 { 0.0 } else { 1.0 }, sh - 1.0);
                self.cv.setbg();
            } else {
                self.setc(false);
            }
            if i > 0 && !lit {
                self.cv.fill(sx, sy, 1.0, sh - 1.0);
            }
            let (cx, t, b) = (sx + sw * 0.5, sy + 4.0, sy + sh - 6.0);
            match i {
                0 => self.cv.line(cx - 7.0, b, cx + 7.0, t),
                1 => {
                    self.cv.line(cx - 9.0, b, cx, t);
                    self.cv.line(cx, t, cx + 9.0, b);
                }
                _ => {
                    self.cv.line(cx - 9.0, b, cx - 2.0, t);
                    self.cv.line(cx - 2.0, t, cx - 2.0, b);
                    self.cv.line(cx - 2.0, b, cx + 5.0, t);
                    self.cv.line(cx + 5.0, t, cx + 5.0, b);
                }
            }
        }
        self.set_dk(keep_dk);
        self.setc(false);
    }

    /// Accent-filled cell (the MORPH block: DEPTH and SPEED); `on` false = outlined, background fill (depth 0).
    fn cell_acc(&mut self, n: usize, x: f32, y: f32, w: f32, h: f32, lbl: &str, on: bool, centred: bool) {
        let hv = self.hit(x, y, w, h);
        if hv && self.press && self.act == 0 {
            self.begin_drag(n);
        }
        if hv && self.wheel != 0 {
            self.wheel_adj(n);
        }
        let v = self.sget(n);
        let t = self.n2t(n, v);
        let isact = self.act == n as i32;
        let s = self.fmt(n, v);
        if on {
            self.setc(true);
            self.cv.fill(x, y, w, h);
            self.cv.setbg();
        } else {
            self.setc(isact);
            self.cv.boxr(x, y, w, h);
        }
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, lbl);
        self.cv.set_font(2);
        self.cv.textc(x, y + (h * 0.4 + 1.0).floor(), w, &s);
        if centred {
            // centre-tapped: the bar grows from the middle (left = scene A, right = scene B) and a notch marks the centre
            let (cx, half) = (x + 2.0 + ((w - 4.0) * 0.5).floor(), (w - 4.0) * 0.5);
            let len = ((t - 0.5).abs() * 2.0 * half).floor();
            if len >= 1.0 {
                self.cv.fill(if t < 0.5 { cx - len } else { cx }, y + h - 4.0, len, 2.0);
            }
            self.cv.fill(cx, y + h - 6.0, 1.0, 5.0);
        } else {
            self.cv.fill(x + 2.0, y + h - 4.0, ((t * (w - 4.0)).floor()).max(1.0), 2.0);
        }
        self.setc(false);
    }

    /// Button with a hover effect (filled while the pointer is over it); lit: 0 off, 1 UI colour, 2 accent.
    fn hbtn(&mut self, x: f32, y: f32, w: f32, h: f32, s: &str, lit: u8) -> bool {
        let hv = self.hit(x, y, w, h);
        let clicked = hv && self.press && self.act == 0;
        if lit > 0 || hv {
            self.setc(lit > 1);
            self.cv.fill(x, y, w, h);
            self.cv.setbg();
            if lit > 0 && hv {
                self.cv.boxr(x + 1.0, y + 1.0, w - 2.0, h - 2.0);
            }
        } else {
            self.setc(false);
            self.cv.boxr(x, y, w, h);
        }
        self.cv.set_font(1);
        self.cv.textc(x, y + ((h - 8.0) * 0.5).floor(), w, s);
        self.setc(false);
        clicked
    }

    /// Segmented toggle group (one box, shared edges): returns the index that was clicked.
    fn seg(&mut self, x: f32, y: f32, w: f32, h: f32, labels: &[&str], sel: usize) -> Option<usize> {
        let n = labels.len();
        let mut res = None;
        for (i, l) in labels.iter().enumerate() {
            let bx = x + (w * i as f32 / n as f32).floor();
            let bx2 = x + (w * (i + 1) as f32 / n as f32).floor();
            let wi = bx2 - bx + if i + 1 < n { 1.0 } else { 0.0 };
            if self.hbtn(bx, y, wi, h, l, (sel == i) as u8) {
                res = Some(i);
            }
        }
        res
    }

    /// MORPH block (top of the right column): direction toggle on top, DEPTH and SPEED below. No sound yet.
    fn morph_block(&mut self, x: f32, y: f32, w: f32) {
        // mode: 2 = MIX (a static A <-> B crossfade by DEPTH, the default), 0 = A > B and 1 = B > A (the detector moves it)
        let dir = ((self.sget(39) + 0.5).floor() as i32).clamp(0, 2);
        let mix = dir == 2;
        let depth_on = self.sget(40) > 0.0;
        // the three cells share 111 px, half of the plot's height
        let (th, dh, sh_, dy, sy) = (36.0, 34.0, 35.0, 39.0, 76.0);
        let hv = self.hit(x, y, w, th);
        if hv && self.press && self.act == 0 {
            self.click_set(39, [0.0, 1.0, 2.0][((dir + 1) % 3) as usize]); // MIX > A>B > B>A > MIX
            self.press = false;
        }
        if hv && self.rpress && self.act == 0 {
            // right click: the modes as a list
            let items = ["MIX  A <> B", "A > B  DETECT", "B > A  DETECT"].iter().map(|s| s.to_string()).collect();
            self.open_menu(MenuKind::Morph, x - 66.0, y + th, items, [1usize, 2, 0][dir as usize]);
            self.rpress = false;
        }
        if hv {
            self.setc(false);
            self.cv.fill(x, y, w, th);
            self.cv.setbg();
        } else {
            self.setc(false);
            self.cv.boxr(x, y, w, th);
        }
        self.cv.set_font(1);
        self.cv.textc(x, y + 4.0, w, if mix { "XFADE" } else { "MORPH" });
        let (l, r) = if dir == 1 { ("B", "A") } else { ("A", "B") };
        let cy = y + 20.0;
        self.cv.text(x + w * 0.5 - 14.0, cy - 4.0, l);
        if mix {
            // A <-> B: a line with a head at each end
            self.cv.line(x + w * 0.5 - 6.0, cy, x + w * 0.5 + 3.0, cy);
            self.cv.tri((x + w * 0.5 + 1.0, cy - 3.0), (x + w * 0.5 + 1.0, cy + 3.0), (x + w * 0.5 + 5.0, cy));
            self.cv.tri((x + w * 0.5 - 4.0, cy - 3.0), (x + w * 0.5 - 4.0, cy + 3.0), (x + w * 0.5 - 8.0, cy));
        } else {
            self.cv.line(x + w * 0.5 - 6.0, cy, x + w * 0.5 + 3.0, cy);
            self.cv.tri((x + w * 0.5 + 1.0, cy - 3.0), (x + w * 0.5 + 1.0, cy + 3.0), (x + w * 0.5 + 5.0, cy));
        }
        self.cv.text(x + w * 0.5 + 8.0, cy - 4.0, r);
        // where the blend leans right now: a thin line along the bottom edge growing from the centre, left = scene A, right = B
        // (nothing but the centre notch while the morph is off)
        let pos = self.env.shared.morph_pos.load(Ordering::Relaxed).clamp(0.0, 1.0);
        let (cx, half) = (x + 2.0 + ((w - 4.0) * 0.5).floor(), (w - 4.0) * 0.5);
        let len = if depth_on { ((pos - 0.5).abs() * 2.0 * half).floor() } else { 0.0 };
        if len >= 1.0 {
            self.cv.fill(if pos < 0.5 { cx - len } else { cx }, y + th - 4.0, len, 2.0);
        }
        self.cv.fill(cx, y + th - 6.0, 1.0, 4.0);
        self.setc(false);
        self.cell_acc(40, x, y + dy, w, dh, if mix { "MIX" } else { "DEPTH" }, depth_on, mix);
        if mix {
            // SPEED belongs to the detector: greyed out and inert while the crossfade is static
            let keep = self.dk;
            self.set_dk(keep * 0.35);
            self.mask_in();
            self.cell_acc(41, x, y + sy, w, sh_, "SPEED", false, false);
            self.unmask_in();
            self.set_dk(keep);
        } else {
            self.cell_acc(41, x, y + sy, w, sh_, "SPEED", depth_on, false);
        }
    }

    /// Full-width row: the fx enable square and the fx type dropdown (click or right-click anywhere on it). PRE-SAT | POST-SAT
    /// lives beside the viewport (`fx_pos_btns`).
    fn fx_row(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let on = self.sget(31) > 0.5;
        let byp = self.sget(26) > 0.5;
        self.set_dk(if byp { 0.5 } else { 1.0 });
        // enable square
        let sq = 12.0;
        self.sqind(x + 2.0, y + (h - sq) * 0.5, sq, on, false, false);
        self.sq_tog(31, x + 2.0, y + (h - sq) * 0.5, sq);
        // dropdown
        let (dx, dw) = (x + 22.0, w - 22.0);
        let ty = (self.sget(32) + 0.5).floor() as usize;
        let hv = self.hit(dx, y, dw, h);
        if hv && (self.press || self.rpress) && self.act == 0 {
            let items = FX_NAMES.iter().enumerate().map(|(i, n)| format!("{}{}", n, if i == 0 { "" } else { " (SOON)" })).collect();
            self.open_menu(MenuKind::Fx, dx, y + h, items, ty.min(FX_NAMES.len() - 1));
            self.press = false;
            self.rpress = false;
        }
        if hv {
            self.setc(false);
            self.cv.fill(dx, y, dw, h);
            self.cv.setbg();
        } else {
            self.setc(false);
            self.cv.boxr(dx, y, dw, h);
        }
        self.cv.set_font(2);
        self.cv.text(dx + 8.0, y + ((h - 8.0) * 0.5).floor(), FX_NAMES[ty.min(FX_NAMES.len() - 1)]);
        let (ax, ay) = (dx + dw - 14.0, y + h * 0.5);
        self.cv.tri((ax - 4.0, ay - 2.0), (ax + 4.0, ay - 2.0), (ax, ay + 2.0));
        self.setc(false);
        self.set_dk(1.0);
    }

    /// PRE-SAT over POST-SAT, two stacked buttons beside the fx viewport (the active one is drawn filled).
    fn fx_pos_btns(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let pos = (self.sget(33) + 0.5).floor() as usize;
        let bh = ((h - 2.0) * 0.5).floor();
        for (i, lab) in ["PRE-SAT", "POST-SAT"].iter().enumerate() {
            if self.hbtn(x, y + i as f32 * (bh + 2.0), w, bh, lab, (pos == i) as u8) {
                self.click_set(33, i as f32);
            }
        }
    }

    /// The fx viewport: a faint spectrum of the output underneath, and over it the group delay of the diffusor (the sum of its
    /// N second-order allpass stages) over 20 Hz..20 kHz. Every stage adds a bump at its own centre, so SPREAD fans the bumps
    /// out and STAGES raises them (the baseline ticks mark the centres). Other fx types have no view yet.
    fn fx_viz(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let on = self.sget(31) > 0.5;
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        let ty = (self.sget(32) + 0.5).floor();
        if ty > 0.0 {
            self.cv.set_font(1);
            self.cv.textc(x, y + (h * 0.5 - 4.0).floor(), w, "SOON");
            return;
        }
        let sh = self.env.shared;
        let sr = sh.sample_rate.load(Ordering::Relaxed).max(8000.0);
        let cols = (w - 2.0) as usize;
        let prev = self.dk;
        // faint spectrum underlay (the shared analyser; it only analyses every 30 ms, so the vertical viz sharing it costs nothing)
        {
            let now = self.now;
            let head = sh.tap.w.load(Ordering::Acquire);
            self.spec.update(now, head, |i| sh.tap.get(i));
            let db = self.spec.columns(cols, sr);
            self.set_dk(prev * 0.3);
            self.setc(false);
            for (i, v) in db.iter().enumerate() {
                let bar = (((v + 100.0) / 95.0).clamp(0.0, 1.0) * (h - 3.0)).floor();
                if bar > 0.0 {
                    self.cv.fill(x + 1.0 + i as f32, y + h - 1.0 - bar, 1.0, bar);
                }
            }
        }
        let (f0, q) = (self.sget(34) as f64, (self.sget(35) as f64).clamp(0.3, 8.0));
        let (pct, spread) = (self.sget(36) as f64, self.sget(43) as f64);
        let n = satdiff_dsp::diffuse::stages_from_pct(pct) as usize;
        let srd = sr as f64;
        let stage_gd = |fc: f64, om: f64| -> f64 {
            let w0 = 2.0 * std::f64::consts::PI * fc / srd;
            let alpha = w0.sin() / (2.0 * q);
            let a0 = 1.0 + alpha;
            let (a1, a2) = (-2.0 * w0.cos() / a0, (1.0 - alpha) / a0);
            let phase = |om: f64| -> f64 {
                // allpass: H = (a2 + a1 z^-1 + z^-2) / (1 + a1 z^-1 + a2 z^-2)
                let (c1, s1, c2, s2) = (om.cos(), om.sin(), (2.0 * om).cos(), (2.0 * om).sin());
                let (nr, ni) = (a2 + a1 * c1 + c2, -(a1 * s1 + s2));
                let (dr, di) = (1.0 + a1 * c1 + a2 * c2, -(a1 * s1 + a2 * s2));
                ni.atan2(nr) - di.atan2(dr)
            };
            let d = om * 0.002;
            let mut dp = phase(om + d) - phase(om - d);
            while dp > std::f64::consts::PI {
                dp -= 2.0 * std::f64::consts::PI;
            }
            while dp < -std::f64::consts::PI {
                dp += 2.0 * std::f64::consts::PI;
            }
            (-dp / (2.0 * d)).max(0.0)
        };
        let centres: Vec<f64> = (0..n).map(|k| satdiff_dsp::diffuse::stage_freq(f0, spread, k, n)).collect();
        let stacked = spread < 0.01 || n == 1;
        let gd: Vec<f64> = (0..cols)
            .map(|i| {
                let f = 20.0 * (i as f64 / cols as f64 * 6.9077553).exp();
                let om = 2.0 * std::f64::consts::PI * f / srd;
                if stacked {
                    stage_gd(centres[0], om) * n as f64
                } else {
                    centres.iter().map(|&fc| stage_gd(fc, om)).sum()
                }
            })
            .collect();
        let top = gd.iter().cloned().fold(1.0, f64::max);
        let amp = h - 9.0;
        self.set_dk(prev * if on { 1.0 } else { 0.5 });
        self.setc(false);
        let mut last: Option<(f32, f32)> = None;
        for (i, g) in gd.iter().enumerate() {
            let (px, py) = (x + 1.0 + i as f32, y + h - 4.0 - (amp * (*g / top) as f32).floor());
            if let Some((lx, ly)) = last {
                self.cv.line(lx, ly, px, py);
            }
            last = Some((px, py));
        }
        // one tick per stage centre on the bottom edge (stacked stages share a tick)
        let xi = |fc: f64| (((fc.max(20.0) / 20.0).ln() / 6.9077553 * cols as f64) as f32).clamp(0.0, cols as f32 - 1.0);
        for fc in &centres {
            self.cv.fill(x + 1.0 + xi(*fc).floor(), y + h - 3.0, 1.0, 2.0);
            if stacked {
                break;
            }
        }
        // marker at FREQ, on the curve
        let fi = xi(f0) as usize;
        let my = y + h - 4.0 - (amp * (gd[fi] / top) as f32).floor();
        self.setc(on);
        self.cv.fill(x + 1.0 + fi as f32 - 1.0, my - 1.0, 3.0, 3.0);
        self.set_dk(prev);
        self.setc(false);
    }

    /// Vertical viz: spectrum (low at the bottom, high at the top) or an oscilloscope wipe from top to bottom with a playhead.
    /// Right-click = mode menu, left-click = cycle the scope's time window (the spectrum has no click action).
    fn vviz(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let hv = self.hit(x, y, w, h);
        if hv && self.rpress && self.act == 0 {
            self.viz_mode = 1 - self.viz_mode; // right-click flips spectrum <-> scope (only two views)
            self.rpress = false;
        }
        if hv && self.press && self.act == 0 && self.viz_mode == 1 {
            self.viz_win = (self.viz_win + 1) % VIZ_WIN_MS.len();
            self.press = false;
        }
        let sh = self.env.shared;
        let tap = &sh.tap;
        let head = tap.w.load(Ordering::Acquire);
        let rows = (h - 2.0) as usize;
        let sr = sh.sample_rate.load(Ordering::Relaxed).max(8000.0);
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        if self.viz_mode == 0 {
            let now = self.now;
            self.spec.update(now, head, |i| tap.get(i));
            let cols = self.spec.columns(rows, sr);
            let solid = self.env.params.fft_solid.load(Ordering::Relaxed);
            let bar = |v: f32| (((v + 100.0) / 95.0).clamp(0.0, 1.0) * (w - 2.0)).floor();
            let mut prev: Option<f32> = None;
            for (i, v) in cols.iter().enumerate() {
                let ry = y + h - 2.0 - i as f32;
                let l = bar(*v);
                if solid {
                    if l > 0.0 {
                        self.cv.fill(x + 1.0, ry, l, 1.0);
                    }
                } else {
                    let (a, b) = match prev {
                        Some(p) => (p.min(l), p.max(l)),
                        None => (l, l),
                    };
                    self.cv.fill(x + 1.0 + a, ry, (b - a).max(1.0), 1.0);
                    prev = Some(l);
                }
            }
        } else {
            // gather the samples that arrived since the last frame into the wipe, one row per `win / rows` of time
            if self.sc_rows.len() != rows {
                self.sc_rows = vec![(0.0, 0.0); rows];
                self.sc_row = 0;
                self.sc_acc = 0.0;
                self.sc_cur = (0.0, 0.0);
                self.sc_head = head;
            }
            let scroll = VIZ_WIN_MS[self.viz_win] < 500;
            if scroll != self.sc_scroll {
                self.sc_scroll = scroll;
                self.sc_rows = vec![(0.0, 0.0); rows];
                self.sc_row = 0;
                self.sc_acc = 0.0;
                self.sc_cur = (0.0, 0.0);
                self.sc_wait = false;
            }
            let spr = (VIZ_WIN_MS[self.viz_win] as f64 * 0.001 * sr as f64 / rows as f64).max(1.0);
            let span = satcomp_dsp::TapRing::LEN - 256;
            let from = if head.wrapping_sub(self.sc_head) > span { head.wrapping_sub(span) } else { self.sc_head };
            let win_n = VIZ_WIN_MS[self.viz_win] as f64 * 0.001 * sr as f64;
            for k in 0..head.wrapping_sub(from) {
                let v = tap.get(from.wrapping_add(k));
                let av = v.abs();
                if scroll {
                    self.sc_cur.0 = self.sc_cur.0.min(v);
                    self.sc_cur.1 = self.sc_cur.1.max(v);
                    self.sc_acc += 1.0;
                    if self.sc_acc >= spr {
                        // scrolling: the newest row goes in at the top, the rest move down
                        self.sc_row = (self.sc_row + rows - 1) % rows;
                        let (r, c) = (self.sc_row, self.sc_cur);
                        self.sc_rows[r] = c;
                        self.sc_cur = (0.0, 0.0);
                        self.sc_acc -= spr;
                    }
                    continue;
                }
                if self.sc_wait {
                    // armed: start the next sweep on a rise well above the recent level, or after one window of nothing
                    let hit = av > 0.05 && av > self.sc_env * 2.2 + 0.02;
                    self.sc_env += (av - self.sc_env) * 0.0007;
                    self.sc_wait_n += 1.0;
                    if !hit && self.sc_wait_n < win_n {
                        continue;
                    }
                    self.sc_wait = false;
                    self.sc_row = 0;
                    self.sc_acc = 0.0;
                    self.sc_cur = (0.0, 0.0);
                } else {
                    self.sc_env += (av - self.sc_env) * 0.0007;
                }
                self.sc_cur.0 = self.sc_cur.0.min(v);
                self.sc_cur.1 = self.sc_cur.1.max(v);
                self.sc_acc += 1.0;
                if self.sc_acc >= spr {
                    let (r, c) = (self.sc_row, self.sc_cur);
                    self.sc_rows[r] = c;
                    self.sc_cur = (0.0, 0.0);
                    self.sc_acc -= spr;
                    self.sc_row += 1;
                    if self.sc_row >= rows {
                        self.sc_row = rows - 1;
                        self.sc_wait = true;
                        self.sc_wait_n = 0.0;
                    }
                }
            }
            self.sc_head = head;
            let (cx, hw) = (x + 1.0 + ((w - 2.0) * 0.5).floor(), (w - 2.0) * 0.5);
            for k in 0..rows {
                // scrolling: row k is `k` rows behind the newest; wipe: row k is where it was written
                let (lo, hi) = self.sc_rows[if scroll { (self.sc_row + k) % rows } else { k }];
                let ry = y + 1.0 + k as f32;
                let (a, b) = ((lo.clamp(-1.0, 1.0) * hw).floor(), (hi.clamp(-1.0, 1.0) * hw).ceil());
                self.cv.fill(cx + a, ry, (b - a).max(1.0), 1.0);
            }
            if !scroll {
                self.setc(true);
                self.cv.fill(x + 1.0, y + 1.0 + self.sc_row as f32, w - 2.0, 1.0);
                self.setc(false);
            }
        }
        if hv {
            // mode / window label on hover
            // two lines for the scope (the one-line label did not fit the 62 px viewport)
            let ms = VIZ_WIN_MS[self.viz_win];
            let (l1, l2) = if self.viz_mode == 0 {
                ("SPECTRUM".to_string(), None)
            } else if ms >= 1000 {
                ("SCOPE".to_string(), Some(format!("{}s", ms / 1000)))
            } else {
                ("SCOPE".to_string(), Some(format!("{}ms", ms)))
            };
            self.cv.setbg();
            self.cv.fill(x + 1.0, y + 1.0, w - 2.0, if l2.is_some() { 21.0 } else { 11.0 });
            self.setc(false);
            self.cv.set_font(1);
            self.cv.textc(x, y + 2.0, w, &l1);
            if let Some(l2) = l2 {
                self.cv.textc(x, y + 12.0, w, &l2);
            }
        }
        self.setc(false);
    }

    /// Secondary grid: the domain split 10 x 10, drawn as sparse, faint, dim dashes (the centre cross is drawn separately).
    fn grid10(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let prev = self.dk;
        self.set_dk(prev * 0.3);
        self.setc(false);
        for i in 1..10 {
            if i == 5 {
                continue;
            }
            let gx = (x + w * i as f32 / 10.0).floor();
            let gy = (y + h * i as f32 / 10.0).floor();
            let mut t = 0.0;
            while t < h {
                self.cv.fill(gx, y + t, 1.0, 2.0f32.min(h - t));
                t += 8.0;
            }
            let mut t = 0.0;
            while t < w {
                self.cv.fill(x + t, gy, 2.0f32.min(w - t), 1.0);
                t += 8.0;
            }
        }
        self.set_dk(prev);
    }

    fn draw_plot(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let sh = self.env.shared;
        let dsp = self.env.dsp;
        let sat_on = dsp.drive > 0.0 || dsp.bias != 0.0;
        let pre = (dsp.drive * 0.01 * 36.0 * 0.11512925).exp() as f32;
        let bias = (dsp.bias * 0.01) as f32;
        let (shape, curve) = dsp.scene_view(self.ab());
        self.lut.sync(shape, &curve);
        let (shape2, curve2) = dsp.scene_view(1 - self.ab());
        self.lut2.sync(shape2, &curve2);
        // static curve as heard: the shaper minus the part of its even half that ASYM removes
        let ak = dsp.asym[(shape.max(0) as usize).min(dsp.asym.len() - 1)] * 0.01;
        let eff = move |l: &ShaperLut, v: f64| {
            let r = 0.5 * (1.0 - ak);
            l.eval(v) * (1.0 - r) - r * l.eval(-v)
        };
        // pencil (top right): toggles the custom-curve draw view; entering selects the Custom shaper so edits are audible
        let pen = self.hit(x + w - 19.0, y + 3.0, 16.0, 16.0);
        if pen && self.press && self.act == 0 {
            self.curve_edit = !self.curve_edit;
            self.edit_seen = false;
            if self.curve_edit {
                let sl = self.sc_shape();
                self.click_set(sl, CUSTOM as f32);
                // symmetry is the default tool, unless the stored curve is lopsided: opening must not alter it
                self.sym_mode = self.env.params.curve_of(self.ab()).is_odd();
                self.even = false;
            }
            self.press = false;
        }
        if self.curve_edit {
            if (self.sget(self.sc_shape()) + 0.5).floor() as i32 == CUSTOM {
                self.edit_seen = true;
            } else if self.edit_seen {
                self.curve_edit = false; // another shaper was chosen
            }
        }
        let hv = self.hit(x, y, w, h) && !self.curve_edit;
        let tb = self.hit(x + w - 84.0, y + h - 14.0, 82.0, 12.0);
        if hv && self.press && self.act == 0 {
            if tb {
                self.env.shared.sat_req.store(true, Ordering::Relaxed); // measure 3 s, then write OUT GAIN
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
        if !self.curve_edit {
            self.grid10(x + 1.0, y + 1.0, w - 2.0, h - 2.0);
        }
        self.setc(self.act == 27);
        for i in 0..(w / 2.0).floor() as i32 {
            self.cv.fill(x + i as f32 * 2.0, y + (h / 2.0).floor(), 1.0, 1.0);
        }
        for i in 0..(h / 2.0).floor() as i32 {
            self.cv.fill(x + (w / 2.0).floor(), y + i as f32 * 2.0, 1.0, 1.0);
        }
        if self.curve_edit {
            self.tool_btn(x + w - 19.0, y + 3.0, 3, false);
        } else {
            self.tool_btn(x + w - 19.0, y + 3.0, 0, false);
        }
        if self.curve_edit {
            self.draw_curve_edit(x, y, w, h);
            return;
        }
        // the other scene as a faint underlay (same drive and bias), under the scene being edited
        if sat_on {
            let ak2 = dsp.asym[(shape2.max(0) as usize).min(dsp.asym.len() - 1)] * 0.01;
            let r2 = 0.5 * (1.0 - ak2);
            let prev = self.dk;
            self.set_dk(prev * 0.35);
            self.setc(false);
            let (mut lx, mut ly) = (0.0, 0.0);
            let un = (w as usize).max(64);
            for i in 0..=un {
                let vx = -1.0 + 2.0 * i as f32 / un as f32;
                let v = (vx * pre + bias) as f64;
                let vy = ((self.lut2.eval(v) * (1.0 - r2) - r2 * self.lut2.eval(-v)) as f32).clamp(-1.0, 1.0);
                let (qx, qy) = (x + (vx + 1.0) * 0.5 * (w - 1.0), y + (1.0 - (vy + 1.0) * 0.5) * (h - 1.0));
                if i > 0 {
                    self.cv.line(lx, ly, qx, qy);
                }
                lx = qx;
                ly = qy;
            }
            self.set_dk(prev);
        }
        let pk = sh.sat_vz.load(Ordering::Relaxed).min(1.0);
        // one point per pixel column: a tiled curve under drive folds / wraps many times across the plot
        let n = if curve.tile != 0 { (w as usize).max(64) } else { 64 };
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
        // IN GAIN marker on the left edge (-24..+24 dB, drag the plot to move it)
        let tot = self.sget(27);
        let ty = y + (1.0 - (tot.clamp(-24.0, 24.0) + 24.0) / 48.0) * (h - 3.0);
        self.setc(self.act == 27);
        self.cv.fill(x + 1.0, ty, 7.0, 2.0);
        if self.act == 27 {
            for i in 0..(w / 3.0).floor() as i32 {
                self.cv.fill(x + i as f32 * 3.0, ty, 1.0, 1.0);
            }
        }
        self.setc(false);
        self.cv.set_font(1);
        let s = format!("IN {:+.1}dB", tot);
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

    /// 16x16 square tool button with a hover fill, lit while its tool is active: 0 pencil (free), 1 odd (point mirror),
    /// 2 even (Y mirror), 3 close cross. The icons are drawn in the background colour on the fill, else in the UI colour.
    fn tool_btn(&mut self, x: f32, y: f32, kind: u8, lit: bool) {
        let hv = self.hit(x, y, 16.0, 16.0);
        self.setc(false);
        if lit || hv {
            self.cv.fill(x, y, 16.0, 16.0);
            self.cv.setbg();
        } else {
            self.cv.boxr(x, y, 16.0, 16.0);
        }
        let (ox, oy) = (x + 2.0, y + 2.0);
        let l = |s: &Self, a: f32, b: f32, c: f32, d: f32| s.cv.line(ox + a, oy + b, ox + c, oy + d);
        match kind {
            0 => {
                l(self, 9.0, 1.0, 11.0, 3.0);
                l(self, 9.0, 1.0, 1.0, 9.0);
                l(self, 11.0, 3.0, 3.0, 11.0);
                l(self, 1.0, 9.0, 0.0, 12.0);
                l(self, 3.0, 11.0, 0.0, 12.0);
            }
            1 => {
                for w in [(0.0, 11.0), (3.0, 11.0), (5.0, 9.0), (6.0, 6.0), (7.0, 3.0), (9.0, 1.0), (12.0, 1.0)].windows(2) {
                    l(self, w[0].0, w[0].1, w[1].0, w[1].1);
                }
                self.cv.fill(ox + 5.0, oy + 5.0, 2.0, 2.0);
            }
            2 => {
                for w in [(0.0, 2.0), (2.0, 7.0), (4.0, 10.0), (6.0, 11.0), (8.0, 10.0), (10.0, 7.0), (12.0, 2.0)].windows(2) {
                    l(self, w[0].0, w[0].1, w[1].0, w[1].1);
                }
                for k in [0.0, 4.0, 8.0] {
                    self.cv.fill(ox + 6.0, oy + k, 1.0, 2.0);
                }
            }
            _ => {
                l(self, 1.0, 1.0, 11.0, 11.0);
                l(self, 11.0, 1.0, 1.0, 11.0);
            }
        }
        self.setc(false);
    }

    /// Draw view of the custom curve (unity input, independent of drive/bias). Tools (top right, left of the close cross):
    /// symmetry (default) draws the right half and mirrors it through the origin, `f(-x) = -f(x)`, the pencil draws free.
    /// Left click empty space = add a point (and drag it), drag = move, double click = remove.
    /// The two end points are fixed in x and cannot be removed.
    fn draw_curve_edit(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let ab = self.ab();
        let mut c = self.env.params.curve_of(ab);
        let before = c;
        let mut lin = c.linear;
        // LINEAR | CURVE: straight segments, or the smooth monotone cubic
        if let Some(i) = self.seg(x + 3.0, y + 3.0, 104.0, 16.0, &["LINEAR", "CURVE"], if lin { 0 } else { 1 }) {
            lin = i == 0;
            c.linear = lin;
            self.press = false;
        }
        // tool icons left of the close cross: free draw (pencil), odd (point mirror), even (Y mirror)
        let (pen_x, sym_x, ev_x) = (x + w - 73.0, x + w - 55.0, x + w - 37.0);
        let (hp, hs, he) = (self.hit(pen_x, y + 3.0, 16.0, 16.0), self.hit(sym_x, y + 3.0, 16.0, 16.0), self.hit(ev_x, y + 3.0, 16.0, 16.0));
        if self.press && self.act == 0 {
            // the right half of the curve as drawn (x > 0): switching between the two mirrors reinterprets it
            let (half, half_b) = {
                let (h, b) = c.right_half();
                if h.is_empty() { (vec![(1.0, 1.0)], vec![0.0]) } else { (h, b) }
            };
            if hp && self.sym_mode {
                self.sym_mode = false;
                self.press = false;
            } else if hs && (!self.sym_mode || self.even) {
                // free -> odd keeps the odd part of the curve (what an odd curve can hold); even -> odd keeps the drawn half
                c = if self.sym_mode { Curve::from_half(&half, &half_b, false, lin) } else { Curve::from_half(&c.odd_half_n(DRAW_PTS), &[], false, lin) };
                self.sym_mode = true;
                self.even = false;
                self.press = false;
            } else if he && (!self.sym_mode || !self.even) {
                c = if self.sym_mode { Curve::from_half(&half, &half_b, true, lin) } else { Curve::from_half(&c.even_half_n(DRAW_PTS), &[], true, lin) };
                self.sym_mode = true;
                self.even = true;
                self.press = false;
            }
        }
        let sym = self.sym_mode;
        // y of the mirror image: odd = point reflection (-y), even = Y mirror (+y)
        let m: f32 = if self.even { 1.0 } else { -1.0 };
        // domain [-1,1]^2 is inset so end points and the icons never collide (symmetric margins keep the centre cross true)
        let (ex, ey, ew, eh) = (x + 22.0, y + 22.0, w - 44.0, h - 44.0);
        let (px, py) = (|vx: f32| ex + (vx + 1.0) * 0.5 * (ew - 1.0), |vy: f32| ey + (1.0 - (vy + 1.0) * 0.5) * (eh - 1.0));
        let to_v = |mx: f32, my: f32| (((mx - ex) / (ew - 1.0)) * 2.0 - 1.0, 1.0 - ((my - ey) / (eh - 1.0)) * 2.0);
        self.grid10(ex, ey, ew - 1.0, eh - 1.0);
        // working lists: the whole curve, or in symmetry mode its right half (x > 0, ending at x = 1); the left is derived.
        // `hb[i]`: free = the bend of the segment that starts at point i, symmetry = of the one that ends at it (0 = across the centre)
        let (mut hv, mut hb): (Vec<(f32, f32)>, Vec<f32>) = if sym {
            let (h, b) = c.right_half();
            if h.is_empty() { (vec![(1.0, 1.0)], vec![0.0]) } else { (h, b) }
        } else {
            (c.pts().to_vec(), c.bend[..c.n - 1].to_vec())
        };
        let cap = if sym { DRAW_PTS / 2 } else { DRAW_PTS };
        let interior = |i: usize, n: usize| if sym { i + 1 < n } else { i > 0 && i + 1 < n };
        let xlo = |hv: &Vec<(f32, f32)>, i: usize| if i == 0 { if sym { 0.0 } else { -1.0 } } else { hv[i - 1].0 };
        // curve handles: (index into hb, spot, y of the unbent curve there, the mirrored ghost's (spot, base y))
        type Hand = (usize, (f32, f32), f32, Option<((f32, f32), f32)>);
        let mut hands: Vec<Hand> = Vec::new();
        if sym {
            let nh = hv.len();
            let first = c.n.saturating_sub(nh);
            for j in 0..nh {
                if j == 0 && !self.even {
                    continue; // an odd curve passes through the origin: its middle segment cannot bend
                }
                let kf = if j == 0 { first.wrapping_sub(1) } else { first + j - 1 };
                if kf >= c.n - 1 {
                    continue;
                }
                let (xm, by) = c.handle_base(kf);
                let ghost = if j >= 1 && first >= j + 1 {
                    let kl = first - 1 - j;
                    let (gx, gb) = c.handle_base(kl);
                    Some(((gx, gb + c.bend[kl]), gb))
                } else {
                    None
                };
                hands.push((j, (xm, by + c.bend[kf]), by, ghost));
            }
        } else {
            for k in 0..c.n - 1 {
                let (xm, by) = c.handle_base(k);
                hands.push((k, (xm, by + c.bend[k]), by, None));
            }
        }
        // a point is picked on its own spot, or in symmetry mode on its mirror image
        let pick = |s: &Self, hv: &Vec<(f32, f32)>| -> Option<usize> {
            let mut best: Option<(usize, f32)> = None;
            for (i, &(vx, vy)) in hv.iter().enumerate() {
                let mut d = ((s.kit.mx - px(vx)).powi(2) + (s.kit.my - py(vy)).powi(2)).sqrt();
                if sym {
                    d = d.min(((s.kit.mx - px(-vx)).powi(2) + (s.kit.my - py(m * vy)).powi(2)).sqrt());
                }
                if d <= 6.0 && best.map_or(true, |b| d < b.1) {
                    best = Some((i, d));
                }
            }
            best.map(|b| b.0)
        };
        // a handle is picked on its own spot or on its ghost: (position in `hands`, grabbed on the ghost)
        let pick_hand = |s: &Self, hands: &Vec<Hand>| -> Option<(usize, bool)> {
            let mut best: Option<((usize, bool), f32)> = None;
            for (i, hd) in hands.iter().enumerate() {
                let d = ((s.kit.mx - px((hd.1).0)).powi(2) + (s.kit.my - py((hd.1).1)).powi(2)).sqrt();
                if d <= 5.0 && best.map_or(true, |b| d < b.1) {
                    best = Some(((i, false), d));
                }
                if let Some(((gx, gy), _)) = hd.3 {
                    let d = ((s.kit.mx - px(gx)).powi(2) + (s.kit.my - py(gy)).powi(2)).sqrt();
                    if d <= 5.0 && best.map_or(true, |b| d < b.1) {
                        best = Some(((i, true), d));
                    }
                }
            }
            best.map(|b| b.0)
        };
        let in_box = self.hit(x, y, w, h) && !self.hit(pen_x - 2.0, y, w - (pen_x - x) + 2.0, 22.0) && !self.hit(x, y, 110.0, 22.0);
        if in_box && self.press && self.act == 0 {
            let n = hv.len();
            if let Some(i) = pick(self, &hv) {
                if self.dbl(200 + i as i32) && interior(i, n) {
                    // removing a point merges its two segments (the merged one is straight again)
                    hv.remove(i);
                    if sym {
                        hb[i + 1] = 0.0;
                        hb.remove(i);
                    } else {
                        hb[i - 1] = 0.0;
                        hb.remove(i);
                    }
                    self.last_clk_n = 0;
                } else {
                    self.act = 160;
                    self.cpt = i;
                }
            } else if let Some((hi, ghost)) = pick_hand(self, &hands) {
                let idx = hands[hi].0;
                if self.dbl(300 + idx as i32) {
                    hb[idx] = 0.0; // double click = straighten the segment
                    self.last_clk_n = 0;
                } else {
                    self.act = 161;
                    self.chd = hi;
                    self.chg = ghost;
                }
            } else if n < cap {
                let (mut vx, mut vy) = to_v(self.mx, self.my);
                if sym && vx < 0.0 {
                    vx = -vx;
                    vy *= m;
                }
                let at = if sym { hv.iter().position(|p| p.0 > vx) } else { (1..n).find(|&k| hv[k].0 > vx) }.unwrap_or(n - 1);
                let room = vx - xlo(&hv, at) >= MIN_GAP && hv[at].0 - vx >= MIN_GAP;
                if room {
                    hv.insert(at, (vx, vy.clamp(-1.0, 1.0)));
                    // the new point splits a segment: both halves start straight
                    if sym {
                        hb[at] = 0.0;
                        hb.insert(at + 1, 0.0);
                    } else {
                        hb[at - 1] = 0.0;
                        hb.insert(at, 0.0);
                    }
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
                    vy *= m;
                }
            }
            let n = hv.len();
            if interior(i, n) {
                hv[i].0 = vx.clamp(xlo(&hv, i) + MIN_GAP, hv[i + 1].0 - MIN_GAP);
            }
            hv[i].1 = vy.clamp(-1.0, 1.0);
        }
        if self.act == 161 && self.down && self.chd < hands.len() {
            // the handle follows the pointer vertically: its bend is the distance from the unbent curve
            let (_, vy) = to_v(self.mx, self.my);
            let hd = &hands[self.chd];
            let b = match (self.chg, hd.3) {
                (true, Some((_, gbase))) => m * (vy.clamp(-1.0, 1.0) - gbase),
                _ => vy.clamp(-1.0, 1.0) - hd.2,
            };
            hb[hd.0] = b.clamp(-MAX_BEND, MAX_BEND);
        }
        c = if sym { Curve::from_half(&hv, &hb, self.even, lin) } else { Curve::from_points_bends(&hv, &hb, lin) };
        if c != before {
            self.env.params.store_curve(ab, &c);
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
            // the derived half of a mirrored curve is an accent dash; the half you draw is solid
            if sym && vx <= 0.0 {
                self.setc(true);
                if i % 2 == 0 {
                    self.cv.line(lx, ly, qx, qy);
                }
            } else {
                self.setc(false);
                self.cv.line(lx, ly, qx, qy);
            }
            lx = qx;
            ly = qy;
        }
        self.setc(false);
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
                let (gx, gy) = (px(-vx).round(), py(m * vy).round());
                let pd = self.dk;
                self.set_dk(pd * 0.6);
                self.cv.boxr(gx - 2.0, gy - 2.0, 5.0, 5.0);
                self.set_dk(pd);
            }
        }
        // curve handles: hollow circles on the curve, in the middle of each segment (the hovered / dragged one in the accent)
        let hot_h = if self.act == 161 { Some(self.chd) } else if hot.is_none() { pick_hand(self, &hands).map(|p| p.0) } else { None };
        // the handles come from the curve as it was before this frame's edit: redraw from the new one so they never lag the line
        let mut now: Vec<(usize, (f32, f32))> = Vec::new();
        if sym {
            let nh = hv.len();
            let first = c.n.saturating_sub(nh);
            for j in 0..nh {
                if (j == 0 && !self.even) || (if j == 0 { first.wrapping_sub(1) } else { first + j - 1 }) >= c.n - 1 {
                    continue;
                }
                let kf = if j == 0 { first - 1 } else { first + j - 1 };
                let (xm, by) = c.handle_base(kf);
                now.push((j, (xm, by + c.bend[kf])));
            }
        } else {
            for k in 0..c.n - 1 {
                let (xm, by) = c.handle_base(k);
                now.push((k, (xm, by + c.bend[k])));
            }
        }
        for (ni, (idx, (hx, hy))) in now.iter().enumerate() {
            let on = hot_h.map_or(false, |hh| hands.get(hh).map_or(false, |hd| hd.0 == *idx)) && ni < now.len();
            let (qx, qy) = (px(*hx).round(), py(*hy).round());
            self.setc(on);
            self.cv.disc(qx, qy, 3.0);
            self.cv.setbg();
            self.cv.disc(qx, qy, 1.6);
            if sym {
                if let Some(Some(((gx, gy), _))) = hands.iter().find(|hd| hd.0 == *idx).map(|hd| hd.3) {
                    // keep the ghost in step with the (possibly just edited) bend: it sits on the mirrored curve
                    let _ = (gx, gy);
                    let (gx, gy) = (px(-*hx).round(), py(c.eval(-(*hx) as f64) as f32).round());
                    let pd = self.dk;
                    self.set_dk(pd * 0.6);
                    self.setc(false);
                    self.cv.disc(gx, gy, 3.0);
                    self.cv.setbg();
                    self.cv.disc(gx, gy, 1.6);
                    self.set_dk(pd);
                }
            }
        }
        self.setc(false);
        // tools: pencil (free draw), odd, even
        self.tool_btn(pen_x, y + 3.0, 0, !sym);
        self.tool_btn(sym_x, y + 3.0, 1, sym && !self.even);
        self.tool_btn(ev_x, y + 3.0, 2, sym && self.even);
        self.cv.set_font(1);
        let _ = (hp, hs, he);
        let tool = if !sym { "FREE" } else if self.even { "EVEN" } else { "ODD" };
        self.cv.text(x + 3.0, y + h - 11.0, &format!("DRAW {tool}"));
        self.cv.textr(x, y + h - 11.0, w - 3.0, &format!("{}/{} PTS", c.n, DRAW_PTS));
    }

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
        let x = m.x.min(W - mw).max(0.0);
        let y = m.y.min(H - mh).max(0.0);
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
                                MenuKind::Fx => 4,
                                MenuKind::Os => 6,
                                MenuKind::Morph => 7,
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
                    0 => {
                        let sl = self.sc_shape();
                        self.click_set(sl, id as f32)
                    }
                    1 if id >= 1000 => self.set_light(!self.light),
                    1 => self.set_pal(id as usize),
                    2 => self.request_scale(SIZES[(id as usize).min(SIZES.len() - 1)]),
                    4 => {
                        if id == 0 {
                            self.click_set(32, 0.0); // only the diffusor exists so far
                        }
                    }
                    6 => self.click_set(7, id as f32),
                    7 => self.click_set(39, [2.0, 0.0, 1.0][(id as usize).min(2)]),
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
        self.env.params.store_curve(0, &Curve::from_parts(&p.curve, &p.curve_ext));
        self.env.params.store_curve(1, &Curve::from_parts(&p.curve_b, &p.curve_ext_b));
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
        Curve::from_parts(&b.curve, &b.curve_ext) != self.env.params.curve_snapshot() || Curve::from_parts(&b.curve_b, &b.curve_ext_b) != self.env.params.curve_snapshot_b() || satcomp_dsp::shapers::asym_from(&b.asym) != self.env.params.asym_snapshot()
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
            p.curve_ext = c.ext();
        }
        let cb = self.env.params.curve_snapshot_b();
        if cb != Curve::linear() {
            p.curve_b = cb.pts().to_vec();
            p.curve_ext_b = cb.ext();
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

    /// Preset bar: save icon | name dropdown | prev next (INIT lives in the preset list).
    fn preset_bar(&mut self, y: f32, h: f32) {
        // save icon
        if self.hbtn(10.0, y, 28.0, h, "", 0) {
            self.pending = 2;
        }
        let hv = self.hit(10.0, y, 28.0, h);
        if hv {
            self.cv.setbg();
        } else {
            self.setc(false);
        }
        self.cv.boxr(18.0, y + 3.0, 11.0, 11.0);
        self.cv.fill(21.0, y + 3.0, 5.0, 4.0);
        self.cv.boxr(20.0, y + 8.0, 7.0, 6.0);
        self.setc(false);
        // name -> dropdown
        let (nx, nw) = (38.0, 154.0);
        let name = self.preset_name();
        let shown = if name.is_empty() || name == "INIT" { "-- INIT --".to_string() } else { name };
        let hv = self.hit(nx, y, nw, h);
        if hv && self.press && self.act == 0 {
            self.open_preset_menu(nx, y + h);
            self.press = false;
        }
        self.setc(false);
        self.cv.boxr(nx, y, nw, h);
        self.cv.set_font(2);
        let modified = self.is_modified();
        let mut t = shown;
        while self.cv.text_w(&t) > if modified { nw - 18.0 } else { nw - 8.0 } && t.chars().count() > 3 {
            t.pop();
        }
        if modified {
            t = format!("*{t}*");
        }
        self.cv.textc(nx, y + ((h - 8.0) * 0.5).floor(), nw, &t);
        // prev / next
        for (i, dir) in [(0, -1), (1, 1)] {
            let (x, bw) = if i == 0 { (192.0, 22.0) } else { (214.0, 22.0) };
            if self.hbtn(x, y, bw, h, "", 0) {
                self.step_preset(dir);
            }
            if self.hit(x, y, bw, h) {
                self.cv.setbg();
            } else {
                self.setc(false);
            }
            let (cx, cy) = (x + bw * 0.5, y + h * 0.5);
            if dir < 0 {
                self.cv.tri((cx + 2.0, cy - 3.0), (cx + 2.0, cy + 3.0), (cx - 2.0, cy));
            } else {
                self.cv.tri((cx - 2.0, cy - 3.0), (cx - 2.0, cy + 3.0), (cx + 2.0, cy));
            }
            self.setc(false);
        }
        self.setc(false);
    }

    /// Oversampling dropdown (top right of the preset row): 1x 2x 4x 8x of the saturator.
    fn os_dropdown(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let os = ((self.sget(7) + 0.5).floor() as i32).clamp(0, 3);
        let hv = self.hit(x, y, w, h);
        if hv && self.press && self.act == 0 {
            let items = (0..4).map(|i| format!("{}x OVERSAMPLE", 1 << i)).collect();
            self.open_menu(MenuKind::Os, x - 40.0, y + h, items, os as usize);
            self.press = false;
        }
        if hv && self.wheel != 0 {
            self.click_set(7, (os + if self.wheel > 0 { -1 } else { 1 }).clamp(0, 3) as f32);
        }
        if hv {
            self.setc(false);
            self.cv.fill(x, y, w, h);
            self.cv.setbg();
        } else {
            self.setc(false);
            self.cv.boxr(x, y, w, h);
        }
        self.cv.set_font(2);
        // TILE on the drawn curve runs at x4 at least: show what is actually running
        let forced = self.env.dsp.tile_active();
        self.cv.textc(x, y + ((h - 8.0) * 0.5).floor(), w, &format!("{}x{}", 1 << if forced { os.max(2) } else { os }, if forced && os < 2 { "*" } else { "" }));
        self.setc(false);
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
        let lim_en = self.sget(24) > 0.5;
        let names = ["IN", "SAT", "OUT GAIN", "LIM", "OUT"];
        let bw = [30.0, 40.0, 66.0, 40.0, 34.0];
        let lit = [false, sat_on, false, lim_en && gr > 0.1, false];
        let n = names.len();
        let gap = (w - 24.0 - bw.iter().sum::<f32>()) / (n - 1) as f32;
        let (cy, bh) = (y + 50.0, 22.0);
        let mut bx = [0.0f32; 5];
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
        // a finished sat level match writes OUT GAIN (the audio thread cannot set parameters)
        if sh.sat_result_new.swap(false, Ordering::Acquire) {
            let v = sh.sat_result.load(Ordering::Relaxed);
            self.click_set(22, v);
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
        let lim_on = self.sget(24) > 0.5;
        self.gr_update();

        // ---------- header row: logo, title, size, BYPASS
        self.set_dk(1.0);
        self.logo(13.0, 5.0);
        self.setc(false);
        self.cv.set_font(2);
        let title = format!("SATDIFF {}", version_short());
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
        if self.hbtn(192.0, 6.0, 46.0, 20.0, &label, 0) {
            let cur = SIZES.iter().position(|v| (v - self.cur_scale()).abs() < 0.13).unwrap_or(0);
            let items = SIZES.iter().map(|v| format!("{:.0}%", v * 100.0)).collect();
            self.open_menu(MenuKind::Size, 192.0, 26.0, items, cur);
        }
        if self.hbtn(244.0, 6.0, 62.0, 20.0, "BYPASS", byp as u8) {
            self.click_set(26, if byp { 0.0 } else { 1.0 });
        }

        // ---------- preset row (the About tabs take its place while the About page is open)
        let ab = self.app.about > 0;
        if !ab {
            self.preset_bar(32.0, 18.0);
            self.os_dropdown(244.0, 32.0, 62.0, 18.0);
        }
        self.set_dk(if byp { 0.5 } else { 1.0 });
        if ab {
            self.draw_about(Rect::new(10.0, 54.0, 296.0, 438.0), byp);
        }

        if !ab {
            let bd = if byp { 0.5 } else { 1.0 };
            // ---------- scene row: A | B, shape selector, LIMIT
            self.set_dk(bd);
            let abi = (self.sget(42) + 0.5).floor() as usize;
            if let Some(i) = self.seg(10.0, 54.0, 52.0, 18.0, &["A", "B"], abi) {
                self.click_set(42, i as f32);
                self.press = false;
            }
            let sl = self.sc_shape();
            self.selector(sl, 70.0, 54.0, 166.0, 18.0);
            let gr = self.rd_gr;
            let lim_lab = if lim_on && gr > 0.1 { format!("-{:.1}", gr) } else { "LIMIT".to_string() };
            if self.hbtn(244.0, 54.0, 62.0, 18.0, &lim_lab, lim_on as u8) {
                self.click_set(24, if lim_on { 0.0 } else { 1.0 });
            }

            // ---------- plot (the sat plot / draw view); right column: MORPH, then the viz down to the plot's bottom
            self.draw_plot(10.0, 76.0, 226.0, 226.0);
            self.morph_block(244.0, 76.0, 62.0);
            self.vviz(244.0, 191.0, 62.0, 111.0);

            // ---------- sat cells: three columns of 72 (just wide enough for the longest label + value pair)
            if editing {
                self.theme_cells(CellGrid { x0: 10.0, y0: 308.0, cw: 72.0, ch: 38.0, px: 77.0, py: 42.0 });
            } else {
                self.cell(2, 10.0, 308.0, 72.0, 38.0, "DRIVE");
                self.asym_cell(87.0, 308.0, 72.0, 38.0);
                self.cell2(4, 5, 164.0, 308.0, 72.0, 38.0, "HPF", "LPF");
                self.bias_tile(10.0, 350.0, 149.0, 38.0);
                self.cell(6, 164.0, 350.0, 72.0, 38.0, "TONE");
            }

            // ---------- colour section: fx row (enable + type) with PHASE beside it, viewport + PRE-SAT / POST-SAT + IN GAIN, then a
            // grid of three slots (a dual cell or one normal cell each) with COLOR MIX, SAT MIX and OUT GAIN
            self.fx_row(10.0, 392.0, 149.0, 18.0);
            let fx_on = self.sget(31) > 0.5;
            self.set_dk(if fx_on { bd } else { bd * 0.5 });
            self.fx_viz(10.0, 414.0, 72.0, 38.0);
            self.fx_pos_btns(87.0, 414.0, 72.0, 38.0);
            self.set_dk(bd);
            if !editing {
                // PHASE is a small slider row next to the dropdown; the cell it freed holds IN GAIN (a normal cell: the inverted fill
                // belongs to OUT GAIN alone)
                self.cell1(30, 164.0, 392.0, 72.0, 18.0, "PHASE");
                self.cell(27, 164.0, 414.0, 72.0, 38.0, "IN GAIN");
                self.set_dk(if fx_on { bd } else { bd * 0.5 });
                // three slots (a dual cell or one normal cell each), filled in order; with more than three parameters the first ones
                // pair up. The diffusor has four: FREQ | Q, STAGES, SPREAD
                let slots = [(10.0, 456.0), (87.0, 456.0), (10.0, 498.0)];
                let params: [(usize, &str); 4] = [(34, "FREQ"), (35, "Q"), (36, "STAGES"), (43, "SPREAD")];
                let mut si = 0;
                let mut pi = 0;
                while pi < params.len() && si < slots.len() {
                    let (x, y) = slots[si];
                    if params.len() - pi > slots.len() - si {
                        let (a, b) = (params[pi], params[pi + 1]);
                        self.cell2(a.0, b.0, x, y, 72.0, 38.0, a.1, b.1);
                        pi += 2;
                    } else {
                        self.cell(params[pi].0, x, y, 72.0, 38.0, params[pi].1);
                        pi += 1;
                    }
                    si += 1;
                }
                self.cell(37, 87.0, 498.0, 72.0, 38.0, "COLOR MIX");
                self.set_dk(bd);
                self.cell(25, 164.0, 456.0, 72.0, 38.0, "SAT MIX");
                self.cell_o(22, 164.0, 498.0, 72.0, 38.0, "OUT GAIN");
            }

            // ---------- meters (input hatched, output solid) down the right column beside the cell rows
            self.in_vu(244.0, 308.0, 27.0, 228.0);
            self.out_vu(279.0, 308.0, 27.0, 228.0);
        }
        self.set_dk(1.0);

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
            self.theme_picker(Rect::new(10.0, 76.0, 296.0, 226.0));
            if osk {
                self.unmask_in();
                self.theme_osk(Rect::new(10.0, 76.0, 296.0, 226.0));
            }
        }
    }
}
