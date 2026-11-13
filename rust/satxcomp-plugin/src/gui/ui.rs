//! SatXComp pages: header, tab row, spectrum / crossover viewport, the four band tabs with their saturator and compressor
//! cells, and the output column. Widgets (cells, buttons, drag logic) come from `satcomp_ui::kit`.

use crate::params::{SatXCompParams, SLOTS, TABLE};
use crate::presets::{self, Preset};
use crate::Shared;
use nih_plug::prelude::*;
use satcomp_dsp::biquad::{eq_coef, mag_db};
use satcomp_dsp::shapers::{Curve, ShaperLut, CUSTOM, MAX_PTS, MIN_GAP, SHAPER_NAMES};
use satcomp_ui::appearance::{self, AppState, AppearanceUi, CellGrid, Rect};
use satcomp_ui::canvas::{Canvas, PALETTES};
use satcomp_ui::kit::{Kit, Meta, Widgets};
use satcomp_ui::spectrum::{freq_to_x, x_to_freq, Spectrum};
use satxcomp_dsp::{eq_kind, XParams, EQ_KINDS, NB};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// First UI slot of band `b` (see tools/gen_xparams.py).
pub const fn band_base(b: usize) -> usize {
    10 + 24 * b
}
// offsets inside a band
const ON: usize = 0;
const SOLO: usize = 1;
const DELTA: usize = 2;
const SHAPE: usize = 3;
const DRIVE: usize = 4;
const BIAS: usize = 5;
const TONE: usize = 6;
const SAT_MIX: usize = 7;
const SAT_GAIN: usize = 8;
const THRESH: usize = 9;
const RATIO: usize = 10;
const ATTACK: usize = 11;
const RELEASE: usize = 12;
const SPEED: usize = 13;
const KNEE: usize = 14;
const KNEE_SHAPE: usize = 15;
const DETECTOR: usize = 16;
const FF_FB: usize = 17;
const GR_DIST: usize = 18;
const COMP_MIX: usize = 19;
const COMP_GAIN: usize = 20;
const HP: usize = 21;
const LP: usize = 22;
/// upward compression switch (offset inside the band); its RANGE lives at slot `range_slot(b)`
const UP: usize = 23;
const RANGE_BASE: usize = 160;
const fn range_slot(b: usize) -> usize {
    RANGE_BASE + b
}

/// first UI slot of EQ band `i`: on, type, freq, q, gain
const fn eq_base(i: usize) -> usize {
    120 + 5 * i
}
const EQ_TILT: usize = 145;
const EQ_PIVOT: usize = 146;
const EQ_MIX: usize = 147;
const EQ_GAIN: usize = 148;
const EQ_BYPASS: usize = 149;
/// crossover frequency slots (1, 2, 9) and slope slots, by split
const XOV: [usize; 3] = [1, 2, 9];
const XOV_SLOPE: [usize; 3] = [106, 107, 108];
const SC_ON: usize = 110;
const SC_GAIN: usize = 111;
const SC_HP: usize = 112;
const SC_LP: usize = 113;
const SC_MIX: usize = 114;
const LIM_ON: usize = 150;
/// height of the sidechain control row under the spectrum
const SC_ROW_H: f32 = 17.0;
// ---- layout (logical units; the window is W x H = 680 x 480)
/// viewport (spectrum / crossover / EQ curve)
const VX: f32 = 10.0;
const VY: f32 = 34.0;
const VW: f32 = 514.0;
const VH: f32 = 158.0;
/// right-hand column (levels, crossover sliders, output module)
const RX: f32 = 534.0;
const RW: f32 = 120.0;
/// row of band tabs / EQ band toggles under the viewport
const BAND_Y: f32 = VY + VH + 6.0;
/// first row of cells; four rows, CELL_P apart
const CELLS_Y: f32 = BAND_Y + 22.0;
const CELL_H: f32 = 38.0;
const CELL_P: f32 = 44.0;
/// band module cells (saturator / compressor)
const CELL_W: f32 = 74.0;
/// EQ page columns: five bands plus the pivot column, across the viewport width
const EQ_W: f32 = 80.0;
const EQ_P: f32 = 86.0;
const EQ_PW: f32 = VW - 5.0 * EQ_P;
/// window scale steps (x the 664 x 400 base)
const SIZES: [f32; 6] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5];
/// `kit.act` values: 1..VIEW_ACT are parameter slots (vertical drag, slots run to 150); VIEW_ACT and up are viewport drags
const VIEW_ACT: i32 = 1000;
const ACT_X1: i32 = VIEW_ACT + 1;
const ACT_X2: i32 = VIEW_ACT + 2;
/// third crossover line (the EQ nodes take VIEW_ACT + 3..7)
const ACT_X3: i32 = VIEW_ACT + 8;
/// EQ node `b` is ACT_NODE + b
const ACT_NODE: i32 = VIEW_ACT + 3;
/// EQ viewport: drag the dashed pivot line / drag empty space sideways for tilt
const ACT_PIVOT: i32 = VIEW_ACT + 10;
const ACT_TILT: i32 = VIEW_ACT + 11;
/// custom-curve point drag in the draw view
const ACT_CURVE: i32 = VIEW_ACT + 20;
/// band handle drag in the spectrum (threshold); band b is ACT_THR + b
const ACT_THR: i32 = VIEW_ACT + 30;
/// ASYM slider drag (one slider, follows the selected band)
const ACT_ASYM: i32 = VIEW_ACT + 40;
/// waveform scope: view drag, then the WIN and CEIL sliders (ACT_SCOPE + 1 / + 2)
const ACT_SCOPE: i32 = VIEW_ACT + 50;
mod scope;
const MENU_ROWS: usize = 18;
const MENU_ROW_H: f32 = 14.0;

/// popup list (shaper dropdown): plain label rows, one checked
#[derive(Clone, Copy)]
enum MenuKind {
    /// shaper list of band `b`
    Shape(usize),
    Size,
    Preset,
    /// colour scheme list (the icon's right-click menu)
    Palette,
    /// crossover slope list of split `k`
    Slope(usize),
}

/// kind: 0 selectable item, 1 header (dim, not selectable), 2 separator
#[derive(Clone)]
struct MItem {
    label: String,
    kind: u8,
    id: i32,
}

struct Menu {
    kind: MenuKind,
    x: f32,
    y: f32,
    items: Vec<MItem>,
    /// id of the checked item
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

const KIND_NAMES: [&str; 8] = ["HPF", "LPF", "LO SHELF", "HI SHELF", "BAND", "ALLPASS", "BANDPASS", "NOTCH"];

const BAND_NAMES: [&str; NB] = ["LOW", "ML", "MH", "HI"];
const BAND_LETTERS: [&str; NB] = ["LO", "ML", "MH", "HI"];

pub struct State {
    pub kit: Kit,
    /// colour themes, theme editor and About-page state (satcomp-ui)
    pub app: AppState,
    pub dummy_params: Arc<SatXCompParams>,
    pub dummy_shared: Arc<Shared>,
    /// time of an icon click waiting out the double-click window (then it cycles the colour scheme)
    logo_pend: f64,
    pub pal: usize,
    pub light: bool,
    pub face: usize,
    spec_out: Spectrum,
    spec_dry: Spectrum,
    gr_disp: [f32; NB],
    gr_hold: [f32; NB],
    gr_hold_t: [f64; NB],
    gr_ri: usize,
    ovl: f32,
    ovr: f32,
    hl_l: f32,
    hl_r: f32,
    hl_l_t: f64,
    hl_r_t: f64,
    opk_max: f32,
    clip_lat: bool,
    rd_last: f64,
    rd_pk: f32,
    /// per band: loudness change across the saturator (readout in the plot corner, refreshed ~8x/s)
    rd_sat: [f32; 4],
    rd_rms: f32,
    rd_diff: f32,
    lut: ShaperLut,
    eq_page: bool,
    eq_sel: usize,
    /// custom-curve draw view of the selected band (pencil), and the point being dragged
    curve_edit: bool,
    /// draw view tool: true = symmetry (odd: the right half is drawn, the left follows as f(-x) = -f(x)), false = free draw
    sym_mode: bool,
    /// waveform scope settings, packed (see scope.rs); view-drag anchor (mx, my, window step, ceiling step)
    scope_cfg: i32,
    /// steady scope scrolling: eased read position (bucket index) and the time of the last frame
    scope_pos: f64,
    scope_t: f64,
    scope_drag: (f32, f32, usize, usize),
    /// ASYM drag anchor: (mouse y, value 0..1) at the press
    asym_drag: (f32, f32),
    edit_seen: bool,
    cpt: usize,
    menu: Option<Menu>,
    /// presets: factory bank, user folder listing, the loaded preset as reference for the modified marker, pending dialog
    factory: Vec<Preset>,
    user: Vec<(String, std::path::PathBuf)>,
    base: Option<(String, Preset)>,
    hold_until: f64,
    /// tests: a preset to apply / a capture to take at the start of the next frame (the file dialogs are not testable)
    #[cfg(test)]
    pub test_apply: Option<Preset>,
    #[cfg(test)]
    pub test_want_capture: bool,
    #[cfg(test)]
    pub test_capture: Option<Preset>,
    /// what the preset bar last showed (the *NAME* marker)
    shown_modified: bool,
    pending: u8,
    dlg: Option<Arc<parking_lot::Mutex<Dlg>>>,
    /// gain-reduction readouts refresh slowly (peak over the last interval) so the numbers stay readable
    gr_txt: [f32; NB],
    /// upward compressor lift per band: display value, hold, hold time and the number window (like the gr_* ones)
    gl_disp: [f32; NB],
    gl_hold: [f32; NB],
    gl_hold_t: [f64; NB],
    gl_txt: [f32; NB],
    gl_txt_pk: [f32; NB],
    gr_txt_pk: [f32; NB],
    gr_txt_t: f64,
    /// output limiter reduction: decaying peak (flow block) and a 4 Hz text value
    lim_disp: f32,
    lim_txt: f32,
    lim_txt_pk: f32,
    /// per band post-saturator meter: decaying level, hold, and the last time it clipped
    vu: [f32; NB],
    vu_hold: [f32; NB],
    vu_hold_t: [f64; NB],
    vu_clip_t: [f64; NB],
    /// window-size grip drag and the pointer / window in host points (set by run_frame)
    resizing: bool,
    resize_off: (f32, f32),
    pub ptr_pt: (f32, f32),
    pub win_pt: (f32, f32),
    /// EQ viewport drags: tilt accumulator, last pointer x, pivot grab offset
    tacc: f32,
    tlast: f32,
    gdx: f32,
    rd_eq_diff: f32,
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

fn version_short() -> String {
    if cfg!(test) {
        return "TEST".into();
    }
    let v = env!("CARGO_PKG_VERSION");
    v.strip_suffix(".0").unwrap_or(v).to_string()
}

impl State {
    pub fn new(params: &SatXCompParams) -> Self {
        let mut kit = Kit::new(SLOTS);
        for &(n, _, mn, mx, df) in TABLE {
            kit.meta[n] = Meta { min: mn, max: mx, def: df, mode: 0, snap: 0, step: 1.0 };
        }
        let mut sp = |n: usize, mode: u8, snap: u8, step: f32| {
            kit.meta[n].mode = mode;
            kit.meta[n].snap = snap;
            kit.meta[n].step = step;
        };
        for n in XOV {
            sp(n, 1, 0, 1.0);
        }
        sp(4, 0, 0, 0.1);
        sp(6, 0, 0, 0.1);
        for b in 0..NB {
            let o = band_base(b);
            sp(o + DRIVE, 0, 0, 0.5);
            sp(o + BIAS, 0, 0, 1.0);
            sp(o + TONE, 1, 2, 10.0);
            sp(o + HP, 1, 1, 1.0);
            sp(o + LP, 1, 2, 10.0);
            sp(o + SAT_GAIN, 0, 0, 0.1);
            sp(o + THRESH, 0, 0, 0.1);
            sp(o + RATIO, 1, 2, 0.1);
            sp(o + ATTACK, 1, 0, 0.01);
            sp(o + RELEASE, 1, 2, 1.0);
            sp(o + KNEE, 0, 0, 0.1);
            sp(o + COMP_GAIN, 0, 0, 0.1);
            sp(range_slot(b), 0, 0, 0.5);
        }
        for i in 0..5 {
            let o = eq_base(i);
            sp(o + 2, 1, 0, 1.0);
            sp(o + 3, 1, 0, 0.01);
            sp(o + 4, 0, 0, 0.1);
        }
        sp(SC_GAIN, 0, 0, 0.1);
        sp(SC_HP, 1, 1, 1.0);
        sp(SC_LP, 1, 2, 10.0);
        sp(SC_MIX, 0, 0, 1.0);
        sp(EQ_TILT, 0, 0, 0.1);
        sp(EQ_PIVOT, 1, 0, 1.0);
        sp(EQ_GAIN, 0, 0, 0.1);
        let pal = params.palette_v2.load(Ordering::Relaxed).max(0) as usize;
        let mut app = AppState::new();
        let pal = app.restore_custom(pal, &params.custom_theme.lock());
        params.palette_v2.store(pal as i32, Ordering::Relaxed);
        Self {
            kit,
            app,
            dummy_params: Arc::new(SatXCompParams::from_defaults(None)),
            dummy_shared: Arc::new(Shared::new()),
            logo_pend: -1.0,
            pal,
            light: params.light.load(Ordering::Relaxed),
            face: (params.font.load(Ordering::Relaxed).max(0) as usize).min(satcomp_ui::canvas::FONT_NAMES.len() - 1),
            spec_out: Spectrum::new(),
            spec_dry: Spectrum::new(),
            gr_disp: [0.0; NB],
            gr_hold: [0.0; NB],
            gr_hold_t: [0.0; NB],
            gr_ri: 0,
            ovl: 0.0,
            ovr: 0.0,
            hl_l: 0.0,
            hl_r: 0.0,
            hl_l_t: 0.0,
            hl_r_t: 0.0,
            opk_max: 0.0,
            clip_lat: false,
            rd_last: 0.0,
            rd_pk: 0.0,
            rd_sat: [0.0; 4],
            rd_rms: 0.0,
            rd_diff: 0.0,
            lut: ShaperLut::new(0),
            eq_page: false,
            eq_sel: 0,
            curve_edit: false,
            sym_mode: true,
            scope_cfg: params.scope.load(Ordering::Relaxed),
            scope_pos: 0.0,
            scope_t: 0.0,
            scope_drag: (0.0, 0.0, 0, 0),
            asym_drag: (0.0, 0.0),
            edit_seen: false,
            cpt: 0,
            menu: None,
            factory: presets::factory(),
            user: presets::scan_user(),
            base: None,
            hold_until: 0.0,
            #[cfg(test)]
            test_apply: None,
            #[cfg(test)]
            test_want_capture: false,
            #[cfg(test)]
            test_capture: None,
            shown_modified: false,
            pending: 0,
            dlg: None,
            gr_txt: [0.0; NB],
            gl_disp: [0.0; NB],
            gl_hold: [0.0; NB],
            gl_hold_t: [0.0; NB],
            gl_txt: [0.0; NB],
            gl_txt_pk: [0.0; NB],
            gr_txt_pk: [0.0; NB],
            gr_txt_t: 0.0,
            lim_disp: 0.0,
            lim_txt: 0.0,
            lim_txt_pk: 0.0,
            vu: [0.0; NB],
            vu_hold: [0.0; NB],
            vu_hold_t: [0.0; NB],
            vu_clip_t: [-10.0; NB],
            resizing: false,
            resize_off: (0.0, 0.0),
            ptr_pt: (0.0, 0.0),
            win_pt: (super::W, super::H),
            tacc: 0.0,
            tlast: 0.0,
            gdx: 0.0,
            rd_eq_diff: 0.0,
        }
    }

    /// the preset bar's modified marker (tests)
    #[cfg(test)]
    pub fn modified_marker(&self) -> bool {
        self.shown_modified
    }

    /// last published GR number of band `b` (tests)
    #[cfg(test)]
    pub fn gr_text(&self, b: usize) -> f32 {
        self.gr_txt[b]
    }

    pub fn set_input(&mut self, mx: f32, my: f32, down: bool, rdown: bool, scroll: f32, shift: bool, now: f64) {
        if self.kit.set_input(mx, my, down, rdown, scroll, shift, now) {
            // the egui clock restarts when the editor window is reopened: re-base every stored timestamp
            self.rd_last = now;
            self.scope_t = now;
            self.hl_l_t = now;
            self.hl_r_t = now;
            self.gr_hold_t = [now; NB];
            self.gl_hold_t = [now; NB];
            self.gr_txt_t = now;
            self.hold_until = now;
            self.vu_hold_t = [now; NB];
            self.vu_clip_t = [-10.0; NB];
            self.spec_out.rebase(now);
            self.spec_dry.rebase(now);
            self.app.rebase();
            self.logo_pend = -1.0;
        }
    }

    /// nih-plug builds the editor (and so this state) when the plug-in is created, BEFORE the host restores the project's
    /// saved parameters. Re-read the saved look every time the window opens, otherwise a project reopens on the defaults.
    pub fn sync_look(&mut self, params: &SatXCompParams) {
        let pal = params.palette_v2.load(Ordering::Relaxed).max(0) as usize;
        self.pal = self.app.restore_custom(pal, &params.custom_theme.lock());
        params.palette_v2.store(self.pal as i32, Ordering::Relaxed);
        self.light = params.light.load(Ordering::Relaxed);
        self.face = (params.font.load(Ordering::Relaxed).max(0) as usize).min(satcomp_ui::canvas::FONT_NAMES.len() - 1);
        self.scope_cfg = params.scope.load(Ordering::Relaxed);
    }

    /// Colours the canvas draws with: the editor preview while editing, else the active scheme.
    pub fn colors(&self) -> [f32; 9] {
        self.app.colors(self.pal, self.light)
    }

    /// Constant meter / readout values and a synthetic spectrum for the theme editor's dummy page.
    pub fn fill_dummy(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        let d = &self.dummy_shared;
        d.opk[0].store(1.0, Relaxed);
        d.opk[1].store(0.5, Relaxed);
        d.clip.store(true, Relaxed);
        for b in 0..NB {
            d.gr_pk[b].store(2.0 + b as f32 * 2.0, Relaxed);
            d.sat_vz[b].store(0.3, Relaxed);
            d.sat_out_pk[b].store(0.3 + 0.35 * b as f32, Relaxed);
        }
        d.fms.store(0.05, Relaxed);
        d.diff_disp.store(2.0, Relaxed);
        d.eq_diff.store(1.0, Relaxed);
        // out: a pink-ish stack of sines; dry: the same, louder in the highs (the gap between them shows both accents)
        let w0 = d.tap_out.w.load(Relaxed);
        for i in 0..satcomp_dsp::TapRing::LEN {
            let t = (i + w0) as f32 * 0.001;
            let lo = 0.3 * (t * 211.0).sin() + 0.2 * (t * 1733.0).sin();
            let hi = 0.1 * (t * 7919.0).sin() + 0.05 * (t * 13007.0).sin();
            d.tap_out.buf[i].store((lo + hi * 0.5).to_bits(), Relaxed);
            d.tap_dry.buf[i].store((lo * 0.8 + hi * 2.0).to_bits(), Relaxed);
        }
        d.tap_out.w.store(satcomp_dsp::TapRing::LEN * 3, Relaxed);
        d.tap_dry.w.store(satcomp_dsp::TapRing::LEN * 3, Relaxed);
    }
}

pub struct Env<'a> {
    pub params: &'a SatXCompParams,
    pub setter: &'a ParamSetter<'a>,
    pub shared: &'a Shared,
    pub dsp: XParams,
}

pub struct Gui<'a> {
    pub s: &'a mut State,
    pub cv: Canvas<'a>,
    pub env: Env<'a>,
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
        fmt_slot(n, v)
    }
}

impl<'a> Gui<'a> {
    /// Push the active scheme's colours into the canvas.
    fn apply_pal(&mut self) {
        let a = self.app.scheme_cols(self.pal, self.light);
        self.cv.bk = [a[0], a[1], a[2]];
        self.cv.fg = [a[3], a[4], a[5]];
        self.cv.ac = [a[6], a[7], a[8]];
    }
}

/// About page copy (docs/abouttextcopy.md). Each inner slice is one paragraph, each string one source line.
const ABOUT_TEXT: &[&[&str]] = &[
    &["A 4-band compressor with waveshaping saturator and 5-band EQ", "For surgical baking intentions :3"],
    &["Set crossover range with X-OVR sliders with 12dB, 24dB, 48dB, and 96dB slope options, then drive each band to taste."],
    &["Use sidechain mode for dynamic EQ style ducking, then cook hard with waveshaping distortion.", "Left-click on the top-right pencil icon to enter custom draw mode."],
    &["Refine tone with post-compressor 5-band EQ, or sculpt with notch, bandpass and allpass with dry signal blending."],
    &["Double-click slider to reset parameter to its default value", "Left/Right-click top-left icon for more color ways", "Double-click top-left icon to switch between dark and light mode"],
];

/// Changelog, newest first. Keep each entry to two short sentences or fewer.
const CHANGELOG: &[(&str, &[&str])] = &[("v0.1", &["First release: four bands (LO, ML, MH, HI) with three 12 to 96 dB crossovers, a waveshaper and compressor per band, sidechain with solo, a 5-band post EQ and presets.", "Colour schemes with light mode, custom themes, fonts and the spectrum fill (outline or solid) under About > Appearance.", "Saturator per band: DRIVE and BIAS side by side, ASYM over TONE and HP over LP as stacked sliders.", "Each band handle in the spectrum carries a small bar for the band's level change (left = quieter, right = louder).", "ASYM next to DRIVE keeps lopsided waveshaper curves from dragging DC and sub-bass pulses into the signal. A true-peak output limiter (LIM, on by default) catches rogue peaks; its reduction shows in the LIM readout. The icon in the viewport corner opens a waveform scope (transient meter): half or full wave, with the gain reduction stacked in the accent colour; drag the view to change the window and ceiling."])];

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
    fn fft_solid(&self) -> Option<bool> {
        Some(self.env.params.fft_solid.load(Ordering::Relaxed))
    }
    fn commit_fft_solid(&mut self, solid: bool) {
        self.env.params.fft_solid.store(solid, Ordering::Relaxed);
    }
    fn plugin_key(&self) -> &'static str {
        "satxcomp"
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
        1.25
    }
    fn commit_face(&mut self, face: usize) {
        self.s.face = face;
        self.env.params.font.store(face as i32, Ordering::Relaxed);
        self.cv.face = face;
    }
    fn about_paras(&self) -> &'static [&'static [&'static str]] {
        ABOUT_TEXT
    }
    fn changelog(&self) -> &'static [(&'static str, &'static [&'static str])] {
        CHANGELOG
    }
    fn draw_flow(&mut self, r: Rect, byp: bool) {
        self.draw_flow_at(r, byp);
    }
    fn flow_h(&self) -> f32 {
        150.0
    }
    fn about_scroll_drag_start(&mut self) {}
}

fn hz(v: f32) -> String {
    if v >= 10000.0 {
        format!("{:.0}kHz", v * 0.001)
    } else if v >= 1000.0 {
        format!("{:.1}kHz", v * 0.001)
    } else {
        format!("{:.0}Hz", v)
    }
}

fn fmt_slot(n: usize, v: f32) -> String {
    match n {
        SC_GAIN => return format!("{:+.1}dB", v),
        SC_HP | SC_LP => return hz(v),
        SC_MIX => return format!("{:.0}%", v),
        _ => {}
    }
    if (RANGE_BASE..RANGE_BASE + NB).contains(&n) {
        return format!("{:.0}dB", v);
    }
    if XOV_SLOPE.contains(&n) {
        return format!("{}dB", [12, 24, 48, 96][(v + 0.5).floor().clamp(0.0, 3.0) as usize]);
    }
    if (10..10 + 24 * NB).contains(&n) {
        return match (n - 10) % 24 {
            DRIVE | SAT_MIX | SPEED | KNEE_SHAPE | DETECTOR | FF_FB | GR_DIST | COMP_MIX => format!("{:.0}%", v),
            BIAS => format!("{:+.0}%", v),
            TONE | LP => {
                if v >= 19900.0 {
                    "OFF".into()
                } else if v >= 1000.0 {
                    format!("{:.1}k", v * 0.001)
                } else {
                    format!("{:.0}", v)
                }
            }
            HP => {
                if v <= 20.5 {
                    "OFF".into()
                } else if v >= 1000.0 {
                    format!("{:.1}k", v * 0.001)
                } else {
                    format!("{:.0}", v)
                }
            }
            SAT_GAIN | COMP_GAIN => format!("{:+.1}dB", v),
            THRESH | KNEE => format!("{:.1}dB", v),
            RATIO => {
                if v >= 19.9 {
                    "INF".into()
                } else {
                    format!("{:.1}:1", v)
                }
            }
            ATTACK => {
                if v < 1.0 {
                    format!("{:.2}ms", v)
                } else {
                    format!("{:.1}ms", v)
                }
            }
            RELEASE => {
                if v >= 1990.0 {
                    "AUTO".into()
                } else if v >= 1000.0 {
                    format!("{:.2}s", v * 0.001)
                } else {
                    format!("{:.0}ms", v)
                }
            }
            _ => format!("{:.1}", v),
        };
    }
    if (120..145).contains(&n) {
        return match (n - 120) % 5 {
            2 => hz(v),
            3 => format!("{:.2}", v),
            4 => {
                if v.abs() < 0.05 {
                    "0.0dB".into()
                } else {
                    format!("{:+.1}dB", v)
                }
            }
            _ => format!("{:.0}", v),
        };
    }
    match n {
        EQ_TILT | EQ_GAIN => format!("{:+.1}dB", v),
        EQ_PIVOT => hz(v),
        EQ_MIX => format!("{:.0}%", v),
        1 | 2 | 9 => hz(v),
        4 => {
            if v < 0.05 {
                "OFF".into()
            } else {
                format!("{:.1}ms", v)
            }
        }
        5 => format!("{:.0}%", v),
        6 => format!("{:+.1}dB", v),
        _ => format!("{:.1}", v),
    }
}

impl<'a> Gui<'a> {
    fn sel(&self) -> usize {
        (self.env.params.sel_band.load(Ordering::Relaxed).max(0) as usize).min(NB - 1)
    }

    // ------------------------------------------------------------------ window size
    fn cur_scale(&self) -> f32 {
        self.env.params.editor_state.size().0 as f32 / super::W
    }

    /// Ask the host to resize the editor to `scale` x the base size (aspect locked).
    fn request_scale(&mut self, scale: f32) {
        let sc = scale.clamp(SIZES[0], SIZES[SIZES.len() - 1]);
        if (sc - self.cur_scale()).abs() > 0.001 {
            self.env.params.editor_state.set_requested_size(((super::W * sc).round() as u32, (super::H * sc).round() as u32));
        }
    }

    fn open_size_menu(&mut self, x: f32, y: f32) {
        let items = SIZES.iter().enumerate().map(|(i, v)| MItem { label: format!("{:.0}%", v * 100.0), kind: 0, id: i as i32 }).collect();
        let cur = SIZES.iter().position(|v| (v - self.cur_scale()).abs() < 0.13).unwrap_or(0);
        self.menu = Some(Menu { kind: MenuKind::Size, x, y, items, checked: cur as i32, scroll: 0 });
    }

    /// Bottom-right grip: drag = aspect-locked resize in 25% steps, right-click = size menu.
    fn resize_grip(&mut self) {
        let (gx, gy, gw, gh) = (super::W - 16.0, super::H - 16.0, 16.0, 16.0);
        let hv = self.hit(gx, gy, gw, gh);
        if hv && self.kit.press && self.kit.act == 0 {
            self.resizing = true;
            // distance from the pointer to the window's bottom-right corner, in window points
            self.resize_off = (self.win_pt.0 - self.ptr_pt.0, self.win_pt.1 - self.ptr_pt.1);
            self.kit.press = false;
        }
        if hv && self.kit.rpress {
            self.open_size_menu(gx - 40.0, gy - 90.0);
            self.kit.rpress = false;
        }
        if self.resizing {
            if self.kit.down {
                // Absolute target (pointer + fixed corner offset), independent of the lagging render scale. Only issue a new
                // request once the host window has caught up with the last one, otherwise stale pointer/size pairs make the
                // target overshoot and the window loops.
                let (cw, _) = self.env.params.editor_state.size();
                if (self.win_pt.0 - cw as f32).abs() < 1.5 {
                    let w = (self.ptr_pt.0 + self.resize_off.0) / super::W;
                    let h = (self.ptr_pt.1 + self.resize_off.1) / super::H;
                    self.request_scale((w.min(h) * 4.0).round() / 4.0);
                }
            } else {
                self.resizing = false;
            }
        }
        self.set_dk(1.0);
        self.setc(self.resizing);
        for o in [4.0, 8.0, 12.0] {
            self.cv.line(super::W - 1.0, super::H - 1.0 - o, super::W - 1.0 - o, super::H - 1.0);
        }
        self.setc(false);
    }

    // ------------------------------------------------------------------ presets
    fn preset_name(&self) -> String {
        self.env.params.preset_name.lock().clone()
    }

    fn set_preset_name(&mut self, n: &str) {
        *self.env.params.preset_name.lock() = n.to_string();
    }

    /// Write every stored parameter of `p` (missing ids -> default) and the three curves (missing = linear, so a preset never
    /// keeps the previous one's curve). Bypass / Gain Match are left alone, and so is the sidechain switch unless the preset
    /// carries it (`presets::value_for`).
    fn apply_preset(&mut self, p: &Preset) {
        for &(n, id, mn, mx, df) in TABLE {
            let Some(v) = presets::value_for(p, id, df) else { continue };
            self.click_set(n, v.clamp(mn, mx));
        }
        for b in 0..NB {
            let c = Curve::from_points(p.curve(b));
            self.curve_cell(b).lock().clone_from(&c.pts().to_vec());
            let a = crate::params::asym_from(p.asym(b));
            *self.env.params.asym_cell(b).lock() = a.iter().map(|&v| v as f32).collect();
        }
        self.set_preset_name(&p.name);
        self.base = None;
        self.hold_until = self.kit.now + 0.6;
    }

    fn curve_cell(&self, b: usize) -> &Arc<parking_lot::Mutex<Vec<(f32, f32)>>> {
        match b {
            0 => &self.env.params.curve_lo,
            1 => &self.env.params.curve_mid,
            2 => &self.env.params.curve_mh,
            _ => &self.env.params.curve_hi,
        }
    }

    /// True when the live parameters differ from the loaded preset (drives the *NAME* marker).
    fn is_modified(&mut self) -> bool {
        if self.kit.now < self.hold_until {
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
            let Some(want) = presets::value_for(b, id, df) else { continue };
            let want = want.clamp(mn, mx);
            let have = self.sget(n);
            if (want - have).abs() > 2e-4 * want.abs().max(have.abs()) + 1e-4 {
                return true;
            }
        }
        (0..NB).any(|c| Curve::from_points(b.curve(c)) != self.env.params.curve_snapshot(c) || crate::params::asym_from(b.asym(c)) != self.env.params.asym_snapshot(c))
    }

    fn capture(&self, name: &str) -> Preset {
        let params = TABLE
            .iter()
            .filter(|e| !presets::EXCLUDED.contains(&e.1))
            .map(|e| (e.1.to_string(), self.sget(e.0)))
            .collect();
        let mut p = Preset::new(name, params);
        let curves: Vec<Curve> = (0..NB).map(|b| self.env.params.curve_snapshot(b)).collect();
        if curves.iter().any(|c| *c != Curve::linear()) {
            p.curves = curves.iter().map(|c| c.pts().to_vec()).collect();
        }
        let asyms: Vec<[f64; satxcomp_dsp::params::SHAPER_COUNT]> = (0..NB).map(|b| self.env.params.asym_snapshot(b)).collect();
        if asyms.iter().any(|a| a.iter().any(|&v| v != 100.0)) {
            p.asym = asyms.iter().map(|a| a.iter().map(|&v| v as f32).collect()).collect();
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
            None => {
                if dir > 0 {
                    0
                } else {
                    n - 1
                }
            }
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
        ];
        // factory[0] is the built-in INIT: it is the "Initialize preset" entry above, so not listed twice
        if self.factory.len() > 1 {
            items.push(MItem { label: String::new(), kind: 2, id: -1 });
            items.push(MItem { label: "FACTORY".into(), kind: 1, id: -1 });
            for (i, p) in self.factory.iter().enumerate().skip(1) {
                items.push(MItem { label: p.name.clone(), kind: 0, id: 1000 + i as i32 });
            }
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

    /// File dialogs run on a worker thread: a modal NSSavePanel started inside our frame callback re-enters the host window's
    /// event handling (resign-key) while the editor state is borrowed, which aborted REAPER. rfd dispatches the panel to the
    /// main thread itself; the result is picked up by `poll_dialog`.
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
        let (y, h) = (8.0, 16.0);
        if self.btn(152.0, y, 26.0, h, "", 0) {
            self.pending = 2;
        }
        self.setc(false);
        self.cv.boxr(160.0, y + 3.0, 10.0, 10.0);
        self.cv.fill(163.0, y + 3.0, 4.0, 3.0);
        self.cv.boxr(162.0, y + 8.0, 6.0, 5.0);
        // name -> dropdown
        let name = self.preset_name();
        let shown = if name.is_empty() || name == "INIT" { "-- INIT --".to_string() } else { name };
        let hv = self.hit(178.0, y, 122.0, h);
        if hv && self.kit.press && self.kit.act == 0 {
            self.open_preset_menu(178.0, y + h);
            self.kit.press = false;
        }
        self.setc(false);
        self.cv.boxr(178.0, y, 122.0, h);
        self.cv.set_font(2);
        let modified = self.is_modified();
        self.shown_modified = modified;
        let mut t = shown;
        while self.cv.text_w(&t) > if modified { 104.0 } else { 116.0 } && t.chars().count() > 3 {
            t.pop();
        }
        if modified {
            t = format!("*{t}*");
        }
        self.cv.textc(178.0, y + ((h - 8.0) * 0.5).floor(), 122.0, &t);
        // prev / next
        for (i, dir) in [(0, -1), (1, 1)] {
            let x = 300.0 + i as f32 * 25.0;
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
        // window scale
        let label = format!("{:.0}%", self.cur_scale() * 100.0);
        if self.btn(356.0, y, 42.0, h, &label, 0) {
            self.open_size_menu(356.0, y + h);
        }
    }

    fn select_band(&mut self, b: usize) {
        let old = self.sel();
        if b != old {
            self.curve_edit = false; // the draw view belongs to one band
            // SOLO and DELTA follow the selected band: whichever of them is on for the band being left moves to the new one
            for off in [SOLO, DELTA] {
                if self.sget(band_base(old) + off) > 0.5 {
                    self.click_set(band_base(old) + off, 0.0);
                    self.click_set(band_base(b) + off, 1.0);
                }
            }
        }
        self.env.params.sel_band.store(b as i32, Ordering::Relaxed);
    }

    // ------------------------------------------------------------------ header
    /// plug-in icon: click = next colour scheme, double click = light / dark, right click = scheme dropdown
    fn logo(&mut self, x: f32, y: f32) {
        let hv = self.hit(x - 3.0, y - 3.0, 27.0, 27.0);
        if hv && self.kit.press && self.kit.act == 0 {
            // single click = next scheme (after the double-click window), double click = light/dark
            if self.logo_pend >= 0.0 && self.kit.now - self.logo_pend < 0.3 {
                self.logo_pend = -1.0;
                self.set_light(!self.light);
            } else {
                self.logo_pend = self.kit.now;
            }
            self.kit.press = false;
        }
        if self.logo_pend >= 0.0 && self.kit.now - self.logo_pend >= 0.3 {
            self.logo_pend = -1.0;
            let n = self.app.n_schemes();
            self.set_pal((self.pal + 1) % n);
        }
        if hv && self.kit.rpress {
            self.logo_pend = -1.0;
            let mut items = vec![MItem { label: format!("MODE: {}", if self.light { "LIGHT" } else { "DARK" }), kind: 0, id: 1000 }];
            items.push(MItem { label: String::new(), kind: 2, id: -1 });
            items.extend((0..self.app.n_schemes()).map(|i| MItem { label: self.app.scheme_name(i), kind: 0, id: i as i32 }));
            self.menu = Some(Menu { kind: MenuKind::Palette, x, y: y + 22.0, items, checked: self.pal as i32, scroll: 0 });
        }
        self.setc(false);
        if self.light {
            self.cv.fill(x - 3.0, y - 3.0, 27.0, 27.0);
            self.cv.setbg();
        } else {
            self.cv.boxr(x - 3.0, y - 3.0, 27.0, 27.0);
            self.setc(false);
        }
        self.cv.boxr(x + 6.0, y + 6.0, 9.0, 9.0);
        let l = |c: &Canvas, a: f32, b: f32, cc: f32, d: f32| c.line(x + a, y + b, x + cc, y + d);
        l(&self.cv, 12.5, 0.5, 20.5, 0.5);
        l(&self.cv, 20.5, 0.5, 20.5, 7.5);
        l(&self.cv, 8.5, 20.5, 0.5, 20.5);
        l(&self.cv, 0.5, 20.5, 0.5, 13.5);
        l(&self.cv, 6.5, 14.5, 0.5, 20.5);
        l(&self.cv, 14.5, 6.5, 20.5, 0.5);
        // second diagonal: the X of SatXComp
        l(&self.cv, 6.5, 6.5, 0.5, 0.5);
        l(&self.cv, 14.5, 14.5, 20.5, 20.5);
    }

    fn out_meter(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let rw = 8.0;
        let bw = w - rw - 2.0;
        let bh = ((h - 3.0) * 0.5).floor();
        let sh = self.env.shared;
        let pa = sh.opk[0].swap(0.0, Ordering::Relaxed);
        let pb = sh.opk[1].swap(0.0, Ordering::Relaxed);
        if sh.clip.swap(false, Ordering::Relaxed) {
            self.clip_lat = true;
        }
        if self.hit(x, y, w, h) && self.kit.press && self.kit.act == 0 {
            self.opk_max = 0.0;
            self.clip_lat = false;
            self.hl_l = 0.0;
            self.hl_r = 0.0;
            self.kit.press = false;
        }
        self.ovl = pa.max(self.ovl * 0.85);
        self.ovr = pb.max(self.ovr * 0.85);
        let now = self.kit.now;
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
        self.opk_max = self.opk_max.max(pa.max(pb));
        let lvf = |v: f32| ((8.6858896 * v.max(0.0000001).ln() + 48.0) / 48.0).clamp(0.0, 1.0);
        self.setc(false);
        self.cv.boxr(x, y, bw, h);
        self.cv.fill(x + 1.0, y + 1.0, ((bw - 2.0) * lvf(self.ovl)).floor(), bh);
        self.cv.fill(x + 1.0, y + 2.0 + bh, ((bw - 2.0) * lvf(self.ovr)).floor(), bh);
        if self.hl_l > 0.0001 {
            self.cv.fill(x + 1.0 + ((bw - 3.0) * lvf(self.hl_l)).floor(), y + 1.0, 1.0, bh);
        }
        if self.hl_r > 0.0001 {
            self.cv.fill(x + 1.0 + ((bw - 3.0) * lvf(self.hl_r)).floor(), y + 2.0 + bh, 1.0, bh);
        }
        if self.clip_lat {
            self.setc(true);
            self.cv.fill(x + bw + 2.0, y, rw, h);
        } else {
            self.setc(false);
            self.cv.boxr(x + bw + 2.0, y, rw, h);
        }
        self.setc(false);
    }

    /// Colour of gain that is ADDED (the spectrum's gain area and the upward compressor's lift): the inverse of the accent.
    fn gain_colour(&self) -> [f32; 3] {
        let ac = self.cv.ac;
        let mut inv = [1.0 - ac[0], 1.0 - ac[1], 1.0 - ac[2]];
        // grey schemes: the inverted accent lands on the background colour, so the gain area vanishes - use solid white (black on a light scheme) then
        let lum = |c: [f32; 3]| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        if (lum(inv) - lum(self.cv.bk)).abs() < 0.3 {
            inv = if lum(self.cv.bk) < 0.5 { [1.0; 3] } else { [0.0; 3] };
        }
        inv
    }

    // ------------------------------------------------------------------ meters
    fn update_gr(&mut self) {
        let sh = self.env.shared;
        let now = self.kit.now;
        for b in 0..NB {
            let g = sh.gr_pk[b].swap(0.0, Ordering::Relaxed);
            self.gr_disp[b] = g.max(self.gr_disp[b] * 0.9);
            if g >= self.gr_hold[b] {
                self.gr_hold[b] = g;
                self.gr_hold_t[b] = now;
            } else if now - self.gr_hold_t[b] > 1.0 {
                self.gr_hold[b] = (self.gr_hold[b] - 0.4).max(0.0);
            }
            self.gr_txt_pk[b] = self.gr_txt_pk[b].max(self.gr_disp[b]);
            // upward compressor lift: same ballistics, shown by the centre-tapped meter
            let l = sh.gl_pk[b].swap(0.0, Ordering::Relaxed);
            self.gl_disp[b] = l.max(self.gl_disp[b] * 0.9);
            if l >= self.gl_hold[b] {
                self.gl_hold[b] = l;
                self.gl_hold_t[b] = now;
            } else if now - self.gl_hold_t[b] > 1.0 {
                self.gl_hold[b] = (self.gl_hold[b] - 0.4).max(0.0);
            }
            self.gl_txt_pk[b] = self.gl_txt_pk[b].max(self.gl_disp[b]);
            // post-saturator level meter
            let pk = sh.sat_out_pk[b].swap(0.0, Ordering::Relaxed);
            self.vu[b] = pk.max(self.vu[b] * 0.85);
            if pk >= self.vu_hold[b] {
                self.vu_hold[b] = pk;
                self.vu_hold_t[b] = now;
            } else if now - self.vu_hold_t[b] > 1.0 {
                self.vu_hold[b] *= 0.9;
            }
            if pk >= 1.0 {
                self.vu_clip_t[b] = now;
            }
        }
        let lg = sh.lim_gr_pk.swap(0.0, Ordering::Relaxed);
        self.lim_disp = lg.max(self.lim_disp * 0.9);
        self.lim_txt_pk = self.lim_txt_pk.max(self.lim_disp);
        if now - self.gr_txt_t >= 0.25 {
            self.lim_txt = self.lim_txt_pk;
            self.lim_txt_pk = 0.0;
            self.gr_txt = self.gr_txt_pk;
            self.gr_txt_pk = [0.0; NB];
            self.gl_txt = self.gl_txt_pk;
            self.gl_txt_pk = [0.0; NB];
            self.gr_txt_t = now;
        }
    }

    /// vertical gain-reduction meter of the selected band; click cycles the range (3 / 6 / 12 / 18 dB)
    fn draw_gr_meter(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let b = self.sel();
        if self.hit(x, y, w, h) && self.kit.press && self.kit.act == 0 {
            self.gr_ri = if self.gr_ri >= 3 { 0 } else { self.gr_ri + 1 };
            self.kit.press = false;
        }
        if self.sget(band_base(b) + UP) > 0.5 {
            self.draw_gr_meter_up(x, y, w, h, b);
            return;
        }
        let rng = |i: usize| [3.0f32, 6.0, 12.0, 18.0][i.min(3)];
        while self.gr_ri < 3 && self.gr_disp[b] > rng(self.gr_ri) {
            self.gr_ri += 1;
        }
        let rg = rng(self.gr_ri);
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        self.setc(true);
        self.cv.fill(x + 1.0, y + 1.0, w - 2.0, ((h - 2.0) * (self.gr_disp[b] / rg).min(1.0)).floor());
        self.setc(false);
        let hp = y + 1.0 + ((h - 3.0) * (self.gr_hold[b] / rg).min(1.0)).floor();
        self.cv.fill(x + 1.0, hp, w - 2.0, 1.0);
        self.cv.set_font(1);
        // compressor GR as a number in a window at the top of the bar (positive, like the bar; slow refresh)
        self.cv.setbg();
        self.cv.fill(x + 1.0, y + 1.0, w - 2.0, 11.0);
        self.setc(false);
        self.cv.fill(x + 1.0, y + 12.0, w - 2.0, 1.0);
        let gs = format!("{:.1}", self.gr_txt[b]);
        self.cv.textc(x, y + 3.0, w, &gs);
        let mj = if rg == 3.0 { 1.0 } else if rg == 6.0 { 2.0 } else if rg == 12.0 { 3.0 } else { 6.0 };
        let stp = if rg <= 6.0 { 0.5 } else { 1.0 };
        for j in 1..=((rg / stp) as i32) {
            let v = j as f32 * stp;
            let ty = y + 1.0 + (h - 2.0) * (v / rg);
            if v == v.floor() && (v.floor() % mj) == 0.0 {
                self.cv.fill(x + w - 5.0, ty, 4.0, 1.0);
                let s = format!("{}", v as i32);
                self.cv.text(x + 2.0, (ty - 4.0).min(y + h - 11.0), &s);
            } else {
                self.cv.fill(x + w - 3.0, ty, 2.0, 1.0);
            }
        }
    }

    /// The meter of a band in upward mode: centre-tapped. Gain LIFT grows upward from the centre line in the spectrum's gain
    /// colour (inverse of the accent), gain reduction (the band is not reducing in this mode, but the scale stays symmetric)
    /// grows downward in the accent. Same range cycling as the plain meter; the number window shows the lift.
    fn draw_gr_meter_up(&mut self, x: f32, y: f32, w: f32, h: f32, b: usize) {
        let both = self.sget(band_base(b) + UP) > 1.5;
        let rng = |i: usize| [3.0f32, 6.0, 12.0, 18.0][i.min(3)];
        let peak = self.gl_disp[b].max(self.gr_disp[b]);
        while self.gr_ri < 3 && peak > rng(self.gr_ri) {
            self.gr_ri += 1;
        }
        let rg = rng(self.gr_ri);
        let inv = self.gain_colour();
        let cy = y + (h * 0.5).floor();
        let half = ((h - 2.0) * 0.5).floor();
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        // lift up from the centre, reduction down from it
        self.cv.setrgb(inv);
        let up_px = (half * (self.gl_disp[b] / rg).min(1.0)).floor();
        self.cv.fill(x + 1.0, cy - up_px, w - 2.0, up_px);
        self.setc(true);
        let dn_px = (half * (self.gr_disp[b] / rg).min(1.0)).floor();
        self.cv.fill(x + 1.0, cy + 1.0, w - 2.0, dn_px);
        // holds
        self.setc(false);
        let hu = cy - (half * (self.gl_hold[b] / rg).min(1.0)).floor();
        self.cv.fill(x + 1.0, hu, w - 2.0, 1.0);
        let hd = cy + 1.0 + (half * (self.gr_hold[b] / rg).min(1.0)).floor();
        self.cv.fill(x + 1.0, hd, w - 2.0, 1.0);
        // centre line
        self.cv.fill(x + 1.0, cy, w - 2.0, 1.0);
        self.cv.set_font(1);
        // number window at the top: the lift, in the gain colour
        self.cv.setbg();
        self.cv.fill(x + 1.0, y + 1.0, w - 2.0, 11.0);
        self.setc(false);
        self.cv.fill(x + 1.0, y + 12.0, w - 2.0, 1.0);
        let l = self.gl_txt[b];
        let gs = if l >= 9.95 { format!("+{:.0}", l) } else if l > 0.04 { format!("+{:.1}", l) } else { "0.0".to_string() };
        self.cv.setrgb(inv);
        self.cv.textc(x, y + 3.0, w, &gs);
        self.setc(false);
        // UP/DWN also reduces: a second number window at the bottom, in the accent colour
        if both {
            self.cv.setbg();
            self.cv.fill(x + 1.0, y + h - 12.0, w - 2.0, 11.0);
            self.setc(false);
            self.cv.fill(x + 1.0, y + h - 13.0, w - 2.0, 1.0);
            let r = self.gr_txt[b];
            let rs = if r > 0.04 { format!("-{:.1}", r) } else { "0.0".to_string() };
            self.setc(true);
            self.cv.textc(x, y + h - 10.0, w, &rs);
            self.setc(false);
        }
        // scale: ticks and numbers mirrored around the centre
        let mj = if rg == 3.0 { 1.0 } else if rg == 6.0 { 2.0 } else if rg == 12.0 { 3.0 } else { 6.0 };
        let stp = if rg <= 6.0 { 0.5 } else { 1.0 };
        for j in 1..=((rg / stp) as i32) {
            let v = j as f32 * stp;
            let off = (half + 1.0) * (v / rg);
            for (ty, dir) in [(cy + off, 1.0f32), (cy - off, -1.0)] {
                if (dir < 0.0 && ty < y + 13.0) || (both && dir > 0.0 && ty > y + h - 13.0) {
                    continue; // under a number window
                }
                if v == v.floor() && (v.floor() % mj) == 0.0 {
                    self.cv.fill(x + w - 5.0, ty, 4.0, 1.0);
                    let s = format!("{}", v as i32);
                    self.cv.text(x + 2.0, (ty - 4.0).clamp(y + 14.0, y + h - if both { 24.0 } else { 11.0 }), &s);
                } else {
                    self.cv.fill(x + w - 3.0, ty, 2.0, 1.0);
                }
            }
        }
    }

    fn diff_box(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, "IN>OUT");
        self.cv.set_font(2);
        let s = format!("{:+.1}dB", self.rd_diff);
        self.cv.textc(x, y + (h * 0.4 + 1.0).floor(), w, &s);
        let cx = x + (w * 0.5).floor();
        let d = self.env.shared.diff_disp.load(Ordering::Relaxed).clamp(-12.0, 12.0) / 12.0;
        let bw = (d.abs() * (w * 0.5 - 3.0)).floor();
        if d >= 0.0 {
            self.cv.fill(cx, y + h - 4.0, bw, 2.0);
        } else {
            self.cv.fill(cx - bw, y + h - 4.0, bw, 2.0);
        }
        self.cv.fill(cx, y + h - 6.0, 1.0, 5.0);
    }

    // ------------------------------------------------------------------ viewport: spectrum + crossover
    fn draw_spectrum(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let sh = self.env.shared;
        sh.spec_vis_ms.store(sh.now_ms(), Ordering::Relaxed);
        let now = self.kit.now;
        let sr = sh.sample_rate.load(Ordering::Relaxed);
        {
            let t = &sh.tap_out;
            self.s.spec_out.update(now, t.w.load(Ordering::Acquire), |i| t.get(i));
            let t = &sh.tap_dry;
            self.s.spec_dry.update(now, t.w.load(Ordering::Acquire), |i| t.get(i));
        }
        let n = (w - 2.0) as usize;
        let (co, cd) = (self.spec_out.columns(n, sr), self.spec_dry.columns(n, sr));
        let hh = |v: f32| (((v + 100.0) / 95.0).clamp(0.0, 1.0) * (h - 2.0)).floor();
        let inv = self.gain_colour();
        // spectrum look (Appearance): solid UI-colour fill under the output, or background fill with a stroke (accent shows the gap to dry)
        let solid = self.env.params.fft_solid.load(Ordering::Relaxed);
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        let mut prev: Option<f32> = None;
        for i in 0..n {
            let (ho, hd) = (hh(co[i]), hh(cd[i]));
            let (lo, hi) = (ho.min(hd), ho.max(hd));
            self.setc(false);
            if solid && lo > 0.0 {
                self.cv.fill(x + 1.0 + i as f32, y + h - 1.0 - lo, 1.0, lo);
            }
            if hi > lo {
                // between output and dry: accent = the compressor took it away, inverted accent = output is above dry
                if hd > ho {
                    self.setc(true);
                } else {
                    self.cv.setrgb(inv);
                }
                self.cv.fill(x + 1.0 + i as f32, y + h - 1.0 - hi, 1.0, hi - lo);
            }
            if !solid {
                // white stroke along the output spectrum, joined to its neighbour so steep edges stay connected
                self.setc(false);
                let (a, b) = match prev {
                    Some(p) => (p.min(ho), p.max(ho)),
                    None => (ho, ho),
                };
                self.cv.fill(x + 1.0 + i as f32, y + h - 1.0 - b, 1.0, (b - a).max(1.0));
            }
            prev = Some(ho);
        }
        self.setc(false);
    }

    fn draw_crossover(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let mut f = [self.env.dsp.x1 as f32, self.env.dsp.x2 as f32, self.env.dsp.x3 as f32];
        f[1] = f[1].max(f[0] * 1.05);
        f[2] = f[2].max(f[1] * 1.05);
        let px: Vec<f32> = f.iter().map(|&v| freq_to_x(v, x + 1.0, w - 2.0)).collect();
        // drag a crossover line (the nearest one within 6 px)
        if self.hit(x, y, w, h) && self.kit.press && self.kit.act == 0 {
            let (k, d) = (0..3).map(|k| (k, (self.kit.mx - px[k]).abs())).fold((0, f32::MAX), |a, c| if c.1 < a.1 { c } else { a });
            if d < 6.0 {
                self.kit.act = [ACT_X1, ACT_X2, ACT_X3][k];
                self.kit.press = false;
            }
        }
        for k in 0..3 {
            let act = [ACT_X1, ACT_X2, ACT_X3][k];
            if self.kit.act == act && self.kit.down {
                let v = x_to_freq(self.kit.mx, x + 1.0, w - 2.0);
                let lo = if k == 0 { 20.0 } else { f[k - 1] * 1.05 };
                let hi = if k == 2 { 20000.0 } else { f[k + 1] / 1.05 };
                self.setv(XOV[k], v.clamp(lo, hi.max(lo)));
            }
        }
        let active = self.kit.act;
        // dotted lines
        self.setc(false);
        for k in 0..3 {
            let lit = active == [ACT_X1, ACT_X2, ACT_X3][k];
            if lit {
                self.setc(true);
            }
            let mut yy = y + 1.0;
            while yy < y + h - 2.0 {
                self.cv.fill(px[k].floor(), yy, 1.0, 2.0);
                yy += 4.0;
            }
            self.setc(false);
        }
        // Band handles. The handle sits at the band's threshold on the spectrum's dB axis: drag = threshold (double-click =
        // default), click = select. A line marks the band's frequency range, a small bar hangs from the handle for the gain
        // reduction, and a horizontal bar on top of it shows the band's level change (left = quieter, right = louder).
        let sel = self.sel();
        let edges = [x + 1.0, px[0], px[1], px[2], x + w - 1.0];
        let y_of = |t: f32| y + h - 1.0 - ((t + 100.0) / 95.0).clamp(0.0, 1.0) * (h - 2.0);
        for b in 0..NB {
            let o = band_base(b);
            let (lo_f, hi_f) = (if b == 0 { 20.0 } else { f[b - 1] }, if b == NB - 1 { 20000.0 } else { f[b] });
            let cx = freq_to_x((lo_f * hi_f).sqrt(), x + 1.0, w - 2.0).floor();
            let (bw, bx) = if b == sel { (26.0, cx - 13.0) } else { (18.0, cx - 9.0) };
            let cyy = y_of(self.sget(o + THRESH)).clamp(y + 11.0, y + h - 7.0).floor();
            let by = cyy - 6.0;
            if self.hit(bx, by, bw, 12.0) && self.kit.press && self.kit.act == 0 {
                self.select_band(b);
                if self.dbl(160 + b as i32) {
                    let d = self.kit.meta[o + THRESH].def;
                    self.click_set(o + THRESH, d);
                } else {
                    self.kit.act = ACT_THR + b as i32;
                }
                self.kit.press = false;
            }
            if self.kit.act == ACT_THR + b as i32 && self.kit.down {
                let t = (y + h - 1.0 - self.kit.my) / (h - 2.0) * 95.0 - 100.0;
                self.setv(o + THRESH, t.clamp(-40.0, 0.0));
            }
            let on = self.sget(o + ON) > 0.5;
            let held = self.kit.act == ACT_THR + b as i32;
            // range line
            self.setc(false);
            let (xl, xr) = (edges[b], edges[b + 1]);
            self.cv.fill(xl.floor(), cyy, (xr - xl).floor().max(1.0), 1.0);
            // gain reduction bar from the centre of the handle, 2 px per dB
            let gr = self.gr_disp[b];
            if gr > 0.05 && on {
                self.setc(true);
                self.cv.fill(cx - 1.0, by + 12.0, 3.0, (gr * 2.0).min(36.0).floor().max(1.0));
            }
            // level change across the band on top of the handle: centre tick, bar to the left (accent) or right
            let d = if on { self.env.shared.band_diff[b].load(Ordering::Relaxed).clamp(-12.0, 12.0) } else { 0.0 };
            let half = ((bw * 0.5 + 2.0) * d.abs() / 12.0).floor();
            self.setc(d < -0.05);
            if d < 0.0 {
                self.cv.fill(cx - half, by - 4.0, half, 2.0);
            } else {
                self.cv.fill(cx + 1.0, by - 4.0, half, 2.0);
            }
            self.setc(false);
            self.cv.fill(cx, by - 5.0, 1.0, 4.0);
            self.setc(held);
            if b == sel {
                self.cv.fill(bx, by, bw, 12.0);
                self.cv.setbg();
            } else {
                self.cv.setbg();
                self.cv.fill(bx, by, bw, 12.0);
                self.setc(held);
                self.cv.boxr(bx, by, bw, 12.0);
            }
            self.cv.set_font(1);
            self.cv.textc(bx, by + 2.0, bw, BAND_LETTERS[b]);
            self.setc(false);
        }
    }

    /// Post-saturator level of the selected band: digital bar (-48..0 dB), accent stroke on top while it clips (held ~1 s).
    fn draw_vu(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let b = self.sel();
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        let lvf = |v: f32| ((8.6858896 * v.max(0.0000001).ln() + 48.0) / 48.0).clamp(0.0, 1.0);
        let ih = h - 2.0;
        let lh = (ih * lvf(self.vu[b])).floor();
        if lh > 0.0 {
            self.cv.fill(x + 1.0, y + h - 1.0 - lh, w - 2.0, lh);
        }
        // segment gaps give the digital look
        self.cv.setbg();
        let mut k = 2.0;
        while k < ih {
            self.cv.fill(x + 1.0, y + h - 1.0 - k, w - 2.0, 1.0);
            k += 3.0;
        }
        // held peak
        self.setc(false);
        if self.vu_hold[b] > 0.0001 {
            let hy = (ih * lvf(self.vu_hold[b])).floor().max(1.0);
            self.cv.fill(x + 1.0, y + h - 1.0 - hy, w - 2.0, 1.0);
        }
        // scale ticks every 6 dB
        for db in (0..=48).step_by(6) {
            let ty = y + 1.0 + ih * (db as f32 / 48.0);
            self.cv.fill(x + w - 4.0, ty.floor(), 3.0, 1.0);
        }
        if self.kit.now - self.vu_clip_t[b] < 1.0 {
            self.setc(true);
            self.cv.fill(x + 1.0, y + 1.0, w - 2.0, 3.0);
        }
        self.setc(false);
    }

    // ------------------------------------------------------------------ band modules
    /// shape selector: [<] NAME [>]; click the name or right-click anywhere for the full list
    /// ASYM of the selected band's current shaper: how much of the shaper's even part (the part that carries DC and sub-audio
    /// pulses) is kept. One value per shaper, stored in the project / presets (not a host parameter). Drag, wheel, double-click = 100 %.
    fn asym_cell(&mut self, b: usize, x: f32, y: f32, w: f32, h: f32) {
        let shape = ((self.sget(band_base(b) + SHAPE) + 0.5).floor() as i32).clamp(0, CUSTOM) as usize;
        let cell = self.env.params.asym_cell(b).clone();
        let get = || crate::params::asym_from(&cell.lock())[shape] as f32;
        let set = |v: f32| {
            let mut g = cell.lock();
            if g.len() < SHAPER_NAMES.len() {
                g.resize(SHAPER_NAMES.len(), 100.0);
            }
            g[shape] = v.clamp(0.0, 100.0);
        };
        let hv = self.hit(x, y, w, h);
        if hv && self.kit.press && self.kit.act == 0 {
            if self.dbl(170 + b as i32) {
                set(100.0);
            } else {
                self.kit.act = ACT_ASYM;
                self.asym_drag = (self.kit.my, get() * 0.01);
            }
            self.kit.press = false;
        }
        if hv && self.kit.wheel != 0 {
            let d = if self.kit.wheel > 0 { 1.0 } else { -1.0 } * if self.kit.shift { 0.5 } else { 3.0 };
            set((get() + d).round());
        }
        if self.kit.act == ACT_ASYM && self.kit.down {
            let k = if self.kit.shift { 0.0008 } else { 0.006 };
            let t = (self.asym_drag.1 + (self.asym_drag.0 - self.kit.my) * k).clamp(0.0, 1.0);
            set((t * 100.0).round());
        }
        let v = get();
        let act = self.kit.act == ACT_ASYM;
        self.setc(act);
        self.cv.boxr(x, y, w, h);
        self.cv.set_font(1);
        let ty = y + ((h - 8.0) * 0.5).floor();
        self.cv.text(x + 3.0, ty, "ASYM");
        self.cv.textr(x, ty, w - 3.0, &format!("{:.0}%", v));
        self.cv.fill(x + 2.0, y + h - 3.0, (((v * 0.01) * (w - 4.0)).floor()).max(1.0), 1.0);
        self.setc(false);
    }

    fn shape_selector(&mut self, n: usize, x: f32, y: f32, w: f32, h: f32) {
        let aw = 16.0;
        let cy = y + h * 0.5;
        let hv = self.hit(x, y, w, h);
        let mut sel = (self.sget(n) + 0.5).floor() as i32;
        if hv && self.kit.wheel != 0 {
            let d = if self.kit.wheel > 0 { -1 } else { 1 };
            self.click_set(n, (sel + d).clamp(0, CUSTOM) as f32);
        }
        let open = |s: &mut Self| {
            let items = SHAPER_NAMES.iter().enumerate().map(|(i, n)| MItem { label: n.to_string(), kind: 0, id: i as i32 }).collect();
            let band = s.sel();
            s.menu = Some(Menu { kind: MenuKind::Shape(band), x, y: y + h, items, checked: sel, scroll: 0 });
        };
        if hv && self.kit.rpress {
            open(self);
            self.kit.rpress = false;
        }
        if hv && self.kit.press && self.kit.act == 0 {
            if self.kit.mx < x + aw {
                self.click_set(n, if sel <= 0 { CUSTOM as f32 } else { (sel - 1) as f32 });
            } else if self.kit.mx >= x + w - aw {
                self.click_set(n, if sel >= CUSTOM { 0.0 } else { (sel + 1) as f32 });
            } else {
                open(self);
            }
            self.kit.press = false;
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

    // ------------------------------------------------------------------ About page: signal flow
    /// IN > crossover > three SAT + COMP bands > sum > EQ > OUT. A band is lit while it takes gain reduction, dim when switched
    /// off; the EQ is lit when it does something and dim when bypassed; the sidechain key drops into the compressors.
    fn draw_flow_at(&mut self, r: Rect, _byp: bool) {
        let prev = self.kit.dk;
        let sh = self.env.shared;
        self.setc(false);
        self.cv.boxr(r.x, r.y, r.w, r.h);
        self.cv.set_font(1);
        self.cv.text(r.x + 6.0, r.y + 5.0, "SIGNAL FLOW");
        let eq = self.env.dsp.eq;
        let eq_any = eq.bands.iter().any(|b| b.on) || eq.tilt != 0.0 || eq.gain != 0.0;
        let eq_byp = self.sget(EQ_BYPASS) > 0.5;
        let sc_on = self.sget(SC_ON) > 0.5;
        let lim_on = self.sget(LIM_ON) > 0.5;
        let (bh, bwb) = (22.0f32, 92.0f32);
        // main line: IN, X-OVER, [four bands], SUM, EQ, LIM, OUT. The diagram is centred: the gaps share what the widths leave over
        let w = [24.0f32, 58.0, bwb, 34.0, 30.0, 30.0, 30.0];
        let gw = [1.0f32, 2.0, 2.0, 1.2, 1.2, 1.2];
        let unit = (r.w - 24.0 - w.iter().sum::<f32>()) / gw.iter().sum::<f32>();
        let mut bx = [0.0f32; 7];
        let mut cx = r.x + 12.0;
        for i in 0..7 {
            bx[i] = cx.floor();
            cx += w[i] + if i < 6 { gw[i] * unit } else { 0.0 };
        }
        // the band rows are centred between the title and the caption; the main line runs at their middle
        let mid = ((r.y + 24.0) + (r.y + r.h - 22.0)) * 0.5;
        let cy = (mid - bh * 0.5).floor();
        let (bhb, step) = (18.0f32, 22.0f32);
        let top = (mid - (3.0 * step + bhb) * 0.5).floor();
        let ys: [f32; NB] = std::array::from_fn(|i| top + i as f32 * step);
        let ym: [f32; NB] = std::array::from_fn(|i| ys[i] + bhb * 0.5);
        let my = cy + 11.0;
        // signal levels for the flowing dots: input side (loudest band input), per band in / out, output
        let lvf = |v: f32| ((8.6858896 * v.max(1e-7).ln() + 48.0) / 48.0).clamp(0.0, 1.0);
        let lo = lvf(self.ovl.max(self.ovr));
        let mut b_in = [0.0f32; NB];
        let mut b_out = [0.0f32; NB];
        for b in 0..NB {
            let on = self.sget(band_base(b) + ON) > 0.5;
            b_in[b] = lvf(sh.sat_vz[b].load(Ordering::Relaxed));
            b_out[b] = if on { (lvf(self.vu[b]) - self.gr_disp[b] / 48.0).max(0.0) } else { b_in[b] };
        }
        let inl = b_in.iter().cloned().fold(0.0, f32::max);
        let phase = ((self.kit.now * 26.0) as f32) % 6.0;
        // dots walk along a polyline, 6 px apart, sized by level (the animated "dashes" of the flow)
        let along = |c: &Canvas, pts: &[(f32, f32)], lv: f32| {
            if lv <= 0.02 {
                return;
            }
            let sz = (1.0 + lv * 2.99).floor();
            let (mut acc, mut d) = (0.0f32, phase);
            for sg in pts.windows(2) {
                let (a, b) = (sg[0], sg[1]);
                let len = (b.0 - a.0).abs() + (b.1 - a.1).abs();
                while d < acc + len && len > 0.0 {
                    let t = (d - acc) / len;
                    let (px, py) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                    c.fill((px - sz * 0.5).floor(), (py - sz * 0.5).floor() + 0.5, sz, sz);
                    d += 6.0;
                }
                acc += len;
            }
        };
        let arrow = |c: &Canvas, x0: f32, x1: f32, y: f32| {
            c.fill(x0, y, x1 - x0 - 3.0, 1.0);
            c.tri((x1 - 4.0, y - 3.0), (x1 - 4.0, y + 4.0), (x1, y + 0.5));
        };
        // ---- lines, arrows and flowing dots
        self.setc(false);
        let fork = (bx[1] + w[1] + ((bx[2] - bx[1] - w[1]) * 0.5).floor()).floor();
        let join = (bx[2] + bwb + ((bx[3] - bx[2] - bwb) * 0.5).floor()).floor();
        arrow(&self.cv, bx[0] + w[0], bx[1], my);
        self.cv.fill(bx[1] + w[1], my, fork - bx[1] - w[1] + 1.0, 1.0);
        self.cv.fill(fork, ym[0], 1.0, ym[NB - 1] - ym[0] + 1.0);
        for i in 0..NB {
            arrow(&self.cv, fork, bx[2], ym[i]);
            self.cv.fill(bx[2] + bwb, ym[i], join - bx[2] - bwb + 1.0, 1.0);
        }
        self.cv.fill(join, ym[0], 1.0, ym[NB - 1] - ym[0] + 1.0);
        arrow(&self.cv, join, bx[3], my);
        for i in 3..6 {
            arrow(&self.cv, bx[i] + w[i], bx[i + 1], my);
        }
        along(&self.cv, &[(bx[0] + w[0], my), (bx[1] - 4.0, my)], inl);
        along(&self.cv, &[(bx[1] + w[1], my), (fork, my)], inl);
        for i in 0..NB {
            let on = self.sget(band_base(i) + ON) > 0.5;
            let into = [(fork, my), (fork, ym[i]), (bx[2] - 4.0, ym[i])];
            along(&self.cv, &into, if on { b_in[i] } else { 0.0 });
            let out = [(bx[2] + bwb, ym[i]), (join, ym[i]), (join, my)];
            along(&self.cv, &out, b_out[i]);
        }
        along(&self.cv, &[(join, my), (bx[3] - 4.0, my)], lo);
        for i in 3..6 {
            along(&self.cv, &[(bx[i] + w[i], my), (bx[i + 1] - 4.0, my)], lo);
        }
        // ---- main-line blocks
        let names = ["IN", "X-OVER", "", "SUM", "EQ", "LIM", "OUT"];
        for i in [0usize, 1, 3, 4, 5, 6] {
            let (x, bwi) = (bx[i], w[i]);
            let dim = (i == 4 && (eq_byp || !eq_any)) || (i == 5 && !lim_on);
            self.set_dk(if dim { prev * 0.5 } else { prev });
            let lit = (i == 4 && eq_any && !eq_byp) || (i == 5 && lim_on && self.lim_disp > 0.1);
            self.setc(false);
            if lit {
                self.cv.fill(x, cy, bwi, bh);
                self.cv.setbg();
            } else {
                self.cv.setbg();
                self.cv.fill(x + 1.0, cy + 1.0, bwi - 2.0, bh - 2.0);
                self.setc(false);
                self.cv.boxr(x, cy, bwi, bh);
            }
            self.cv.textc(x, cy + 7.0, bwi, names[i]);
            self.setc(false);
        }
        // ---- the bands
        for b in 0..NB {
            let on = self.sget(band_base(b) + ON) > 0.5;
            let lit = on && self.gr_disp[b] > 0.1;
            self.set_dk(if on { prev } else { prev * 0.5 });
            self.setc(false);
            if lit {
                self.cv.fill(bx[2], ys[b], bwb, bhb);
                self.cv.setbg();
            } else {
                self.cv.setbg();
                self.cv.fill(bx[2] + 1.0, ys[b] + 1.0, bwb - 2.0, bhb - 2.0);
                self.setc(false);
                self.cv.boxr(bx[2], ys[b], bwb, bhb);
            }
            self.cv.textc(bx[2], ys[b] + 5.0, bwb, &format!("{} SAT>COMP", BAND_NAMES[b]));
            self.setc(false);
        }
        self.set_dk(prev);
        // sidechain key: SIDECHAIN box above, dropping into the top band's compressor
        if sc_on {
            let (kw, ky) = (78.0f32, r.y + 3.0);
            let kx = (bx[2] + (bwb - kw) * 0.5).floor();
            let mx = (kx + kw * 0.5).floor();
            self.setc(false);
            self.cv.boxr(kx, ky, kw, 16.0);
            self.cv.textc(kx, ky + 4.0, kw, "SIDECHAIN");
            self.cv.fill(mx, ky + 16.0, 1.0, ys[0] - ky - 16.0 - 3.0);
            self.cv.tri((mx - 3.0, ys[0] - 4.0), (mx + 4.0, ys[0] - 4.0), (mx + 0.5, ys[0]));
            self.cv.text(kx + kw + 8.0, ky + 4.0, "KEY > COMP");
        }
        self.cv.text(r.x + 6.0, r.y + r.h - 15.0, "EACH BAND: SAT > COMP, THEN THE SUM GOES THROUGH THE POST EQ AND THE LIMITER");
    }

    // ------------------------------------------------------------------ popup menu
    /// (x, y, w, h, visible rows) of the open menu, clamped to the canvas.
    fn menu_geom(&mut self) -> Option<(f32, f32, f32, f32, usize)> {
        self.cv.set_font(1);
        let m = self.menu.as_ref()?;
        let tw = m.items.iter().map(|i| self.cv.text_w(&i.label)).fold(0.0, f32::max);
        let rows = m.items.len().min(MENU_ROWS);
        let (mw, mh) = (tw + 26.0, MENU_ROW_H * rows as f32 + 2.0);
        Some((m.x.min(super::W - mw).max(0.0), m.y.min(super::H - mh).max(0.0), mw, mh, rows))
    }

    fn menu_frame(&mut self) {
        let Some((x, y, mw, mh, rows)) = self.menu_geom() else { return };
        let inside = self.hit(x, y, mw, mh);
        let row = ((self.kit.my - y - 1.0) / MENU_ROW_H).floor() as i32;
        let mut chosen: Option<(MenuKind, i32)> = None;
        let mut close = false;
        let (wheel, press, rpress) = (self.kit.wheel, self.kit.press, self.kit.rpress);
        if let Some(m) = self.menu.as_mut() {
            if wheel != 0 && m.items.len() > rows {
                let max = m.items.len() - rows;
                m.scroll = (m.scroll as i32 - wheel).clamp(0, max as i32) as usize;
            }
            if press {
                if inside && row >= 0 && (row as usize) < rows {
                    if let Some(it) = m.items.get(m.scroll + row as usize) {
                        if it.kind == 0 {
                            chosen = Some((m.kind, it.id));
                        } else {
                            // header / separator: swallow the click, keep the menu open
                            self.kit.press = false;
                            self.kit.rpress = false;
                            self.kit.wheel = 0;
                            return;
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
            if let Some((kind, id)) = chosen {
                match kind {
                    MenuKind::Shape(band) => self.click_set(band_base(band) + SHAPE, id as f32),
                    MenuKind::Size => self.request_scale(SIZES[(id as usize).min(SIZES.len() - 1)]),
                    MenuKind::Preset => self.preset_action(id),
                    MenuKind::Slope(k) => self.click_set(XOV_SLOPE[k], id as f32),
                    MenuKind::Palette if id >= 1000 => self.set_light(!self.light),
                    MenuKind::Palette => self.set_pal(id as usize),
                }
            }
        }
        // a menu swallows every click and wheel turn while it is open
        self.kit.press = false;
        self.kit.rpress = false;
        self.kit.wheel = 0;
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
        let hov = ((self.kit.my - y - 1.0) / MENU_ROW_H).floor() as i32;
        let inside = self.hit(x, y, mw, mh);
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
                    self.setc(false);
                    if inside && hov == i as i32 {
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

    /// 12x12 pencil icon; lit while the draw view is open, bright on hover.
    fn pencil_glyph(&mut self, x: f32, y: f32, on: bool, hover: bool) {
        let prev = self.kit.dk; // keep the bypass dimming of the surrounding module
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

    /// Transfer curve of the selected band's saturator at its current drive and bias. Drag in the plot = SAT GAIN (marker on
    /// the left edge), pencil (top right) = draw the band's custom curve, live input peak marked on the curve.
    fn draw_sat_plot(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let b = self.sel();
        let o = band_base(b);
        let gain_n = o + SAT_GAIN;
        let sh = self.env.shared;
        let bp = self.env.dsp.bands[b];
        let sat_on = bp.drive > 0.0 || bp.bias != 0.0;
        let pre = (bp.drive * 0.01 * 36.0 * 0.11512925).exp();
        let bias = bp.bias * 0.01;
        self.lut.sync(bp.shape, &bp.curve);
        // static curve as heard: the shaper minus the part of its even half that ASYM removes
        let ak = bp.asym[(bp.shape.max(0) as usize).min(bp.asym.len() - 1)] * 0.01;
        let eff = move |l: &ShaperLut, v: f64| {
            let r = 0.5 * (1.0 - ak);
            l.eval(v) * (1.0 - r) - r * l.eval(-v)
        };
        // pencil: toggles the draw view; entering selects the Custom shaper so edits are audible
        let pen = self.hit(x + w - 14.0, y + 2.0, 12.0, 12.0);
        if pen && self.kit.press && self.kit.act == 0 {
            self.curve_edit = !self.curve_edit;
            self.edit_seen = false;
            if self.curve_edit {
                self.click_set(o + SHAPE, CUSTOM as f32);
                // symmetry is the default tool, unless the stored curve is lopsided: opening must not alter it
                self.sym_mode = self.env.params.curve_snapshot(b).is_odd();
            }
            self.kit.press = false;
        }
        if self.curve_edit {
            if (self.sget(o + SHAPE) + 0.5).floor() as i32 == CUSTOM {
                self.edit_seen = true;
            } else if self.edit_seen {
                self.curve_edit = false; // another shaper was chosen
            }
        }
        let hv = self.hit(x, y, w, h) && !self.curve_edit;
        // the SAT readout in the bottom right corner: click = measure 3 s, then write this band's SAT GAIN (click again cancels)
        let tb = self.hit(x + w - 84.0, y + h - 14.0, 82.0, 12.0);
        if hv && self.kit.press && self.kit.act == 0 {
            if tb {
                sh.sat_req[b].store(true, Ordering::Relaxed);
                self.kit.press = false;
            } else {
                self.begin_drag(gain_n);
            }
        }
        if hv && self.kit.wheel != 0 {
            self.wheel_adj(gain_n);
        }
        let gain_act = self.kit.act == gain_n as i32;
        self.setc(gain_act);
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
        let pk = sh.sat_vz[b].load(Ordering::Relaxed).min(1.0);
        let n = 64;
        let (mut lx, mut ly) = (0.0, 0.0);
        for i in 0..=n {
            let vx = -1.0 + 2.0 * i as f64 / n as f64;
            let mut vy = if sat_on { eff(&self.lut, vx * pre + bias) } else { vx };
            vy = vy.clamp(-1.0, 1.0);
            let px = x + ((vx + 1.0) * 0.5) as f32 * (w - 1.0);
            let py = y + (1.0 - ((vy + 1.0) * 0.5) as f32) * (h - 1.0);
            self.setc(sat_on && pk > 0.002 && vx.abs() <= pk as f64);
            if i > 0 {
                self.cv.line(lx, ly, px, py);
            }
            lx = px;
            ly = py;
        }
        if sat_on && pk > 0.002 {
            self.setc(true);
            for sgn in [1.0f64, -1.0] {
                let vy = eff(&self.lut, sgn * pk as f64 * pre + bias).clamp(-1.0, 1.0) as f32;
                let vx = sgn as f32 * pk;
                self.cv.fill(x + (vx + 1.0) * 0.5 * (w - 1.0) - 1.0, y + (1.0 - (vy + 1.0) * 0.5) * (h - 1.0) - 1.0, 3.0, 3.0);
            }
            self.setc(false);
        }
        // SAT GAIN level marker on the left edge (-24..+12 dB)
        let tot = self.sget(gain_n);
        let ty = y + (1.0 - (tot.clamp(-24.0, 12.0) + 24.0) / 36.0) * (h - 3.0);
        self.setc(gain_act);
        self.cv.fill(x + 1.0, ty, 7.0, 2.0);
        if gain_act {
            for i in 0..(w / 3.0).floor() as i32 {
                self.cv.fill(x + i as f32 * 3.0, ty, 1.0, 1.0);
            }
        }
        self.setc(false);
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, &format!("OUT {:+.1}dB", tot));
        let s = if sh.sat_meas[b].load(Ordering::Relaxed) {
            self.setc(appearance::blink(self.kit.now));
            format!("MEAS {:.1}s", sh.sat_mt[b].load(Ordering::Relaxed))
        } else {
            format!("SAT {:+.1}dB", self.rd_sat[b])
        };
        self.cv.textr(x, y + h - 14.0, w - 3.0, &s);
        self.setc(false);
    }

    /// Draw view of the selected band's custom curve (unity input, independent of drive/bias).
    /// Left click empty space = add a point (and drag it), drag = move, double click = remove.
    /// The two end points are fixed in x and cannot be removed.
    fn draw_curve_edit(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let b = self.sel();
        let mut c = self.env.params.curve_snapshot(b);
        let before = c;
        // tool icons left of the close cross: free draw (pencil), symmetry
        let (pen_x, sym_x) = (x + w - 28.0, x + w - 42.0);
        let (hp, hs) = (self.hit(pen_x, y + 2.0, 12.0, 12.0), self.hit(sym_x, y + 2.0, 12.0, 12.0));
        if self.kit.press && self.kit.act == 0 {
            if hp && self.sym_mode {
                self.sym_mode = false;
                self.kit.press = false;
            } else if hs && !self.sym_mode {
                // free -> symmetric keeps the odd part of the curve (what a symmetric curve can hold)
                c = Curve::from_odd_half(&c.odd_half());
                self.sym_mode = true;
                self.kit.press = false;
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
        if in_box && self.kit.press && self.kit.act == 0 {
            let n = hv.len();
            if let Some(i) = pick(self, &hv) {
                if self.dbl(200 + i as i32) && interior(i, n) {
                    hv.remove(i);
                    self.kit.last_clk_n = 0;
                } else {
                    self.kit.act = ACT_CURVE;
                    self.cpt = i;
                }
            } else if n < cap {
                let (mut vx, mut vy) = to_v(self.kit.mx, self.kit.my);
                if sym && vx < 0.0 {
                    vx = -vx;
                    vy = -vy;
                }
                let at = if sym { hv.iter().position(|p| p.0 > vx) } else { (1..n).find(|&k| hv[k].0 > vx) }.unwrap_or(n - 1);
                let room = vx - xlo(&hv, at) >= MIN_GAP && hv[at].0 - vx >= MIN_GAP;
                if room {
                    hv.insert(at, (vx, vy.clamp(-1.0, 1.0)));
                    self.kit.act = ACT_CURVE;
                    self.cpt = at;
                    self.kit.last_clk_n = 0; // a creating click never counts towards a double click
                }
            }
            self.kit.press = false;
        }
        if self.kit.act == ACT_CURVE && self.kit.down && self.cpt < hv.len() {
            let (mut vx, mut vy) = to_v(self.kit.mx, self.kit.my);
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
            let cell = self.curve_cell(b);
            *cell.lock() = c.pts().to_vec();
        }
        // identity reference (dotted), the curve, then the points
        let prev = self.kit.dk;
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
        let hot = if self.kit.act == ACT_CURVE { Some(self.cpt) } else { pick(self, &hv) };
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
                let pd = self.kit.dk;
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

    /// Sidechain control row under the spectrum: SC gain | HP | LP | SOLO (solo = hear the key as the detectors see it,
    /// i.e. after gain / HP / LP). Global: the key is conditioned once, then split into the four bands.
    fn sc_row(&mut self, x: f32, y: f32) {
        let h = SC_ROW_H;
        let cells = [(SC_GAIN, "SC", 160.0f32), (SC_HP, "HP", 135.0), (SC_LP, "LP", 135.0)];
        let mut cx = x;
        for (n, lbl, w) in cells {
            let hv = self.hit(cx, y, w, h);
            if hv && self.kit.press && self.kit.act == 0 {
                self.begin_drag(n);
            }
            if hv && self.kit.wheel != 0 {
                self.wheel_adj(n);
            }
            let act = self.kit.act == n as i32;
            self.setc(act);
            self.cv.boxr(cx, y, w, h);
            self.cv.set_font(1);
            self.cv.text(cx + 4.0, y + 5.0, lbl);
            let v = self.sget(n);
            let t = self.fmt(n, v);
            self.cv.textr(cx, y + 5.0, w - 4.0, &t);
            let tn = self.n2t(n, v);
            self.cv.fill(cx + 2.0, y + h - 3.0, ((tn * (w - 4.0)).floor()).max(1.0), 1.0);
            self.setc(false);
            cx += w - 1.0;
        }
        let solo = self.env.shared.sc_solo.load(Ordering::Relaxed);
        if self.btn(cx, y, x + VW - cx, h, "SOLO", if solo { 2 } else { 0 }) {
            self.env.shared.sc_solo.store(!solo, Ordering::Relaxed);
        }
    }

    /// One crossover: its frequency as a slider cell with a small slope button at the right end (click = list of 12 / 24 / 48 / 96 dB per octave)
    fn xover_row(&mut self, k: usize, x: f32, y: f32) {
        let bw = 34.0;
        self.cell1(XOV[k], x, y, RW - bw + 1.0, 16.0, &format!("XOV{}", k + 1));
        let cur = (self.sget(XOV_SLOPE[k]) + 0.5).floor().clamp(0.0, 3.0) as i32;
        let open = matches!(&self.menu, Some(Menu { kind: MenuKind::Slope(m), .. }) if *m == k);
        if self.btn(x + RW - bw, y, bw, 16.0, &format!("{}dB", [12, 24, 48, 96][cur as usize]), open as u8) {
            let items = [12, 24, 48, 96].iter().enumerate().map(|(i, v)| MItem { label: format!("{} dB/oct", v), kind: 0, id: i as i32 }).collect();
            self.menu = Some(Menu { kind: MenuKind::Slope(k), x: x + RW - bw - 36.0, y: y + 16.0, items, checked: cur, scroll: 0 });
        }
    }

    /// back to the factory state of EQ band `b`: on / type / freq / Q / gain
    fn reset_eq_band(&mut self, b: usize) {
        for k in 0..5 {
            let n = eq_base(b) + k;
            let d = self.kit.meta[n].def;
            self.click_set(n, d);
        }
    }

    /// short read-out row: label left, value right, centre-tap level bar on the bottom edge
    fn diff_row(&mut self, x: f32, y: f32, w: f32, h: f32, lbl: &str, db: f32) {
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        self.cv.set_font(1);
        let ty = y + ((h - 8.0) * 0.5).floor();
        self.cv.text(x + 3.0, ty, lbl);
        self.cv.textr(x, ty, w - 3.0, &format!("{:+.1}dB", db));
        let cx = x + (w * 0.5).floor();
        let d = db.clamp(-12.0, 12.0) / 12.0;
        let bw = (d.abs() * (w * 0.5 - 3.0)).floor();
        if d >= 0.0 {
            self.cv.fill(cx, y + h - 3.0, bw, 2.0);
        } else {
            self.cv.fill(cx - bw, y + h - 3.0, bw, 2.0);
        }
        self.cv.fill(cx, y + h - 5.0, 1.0, 4.0);
    }

    fn band_row(&mut self, y: f32) {
        let sel = self.sel();
        let f = [self.env.dsp.x1 as f32, self.env.dsp.x2 as f32, self.env.dsp.x3 as f32];
        let f = [f[0], f[1].max(f[0] * 1.05), f[2].max(f[1].max(f[0] * 1.05) * 1.05)];
        let ranges = [(0.0, f[0]), (f[0], f[1]), (f[1], f[2]), (f[2], 20000.0)];
        let short = |v: f32| {
            if v < 1.0 {
                "0".to_string()
            } else if v < 1000.0 {
                format!("{:.0}", v)
            } else if v < 9950.0 {
                format!("{:.1}k", v * 0.001)
            } else {
                format!("{:.0}k", v * 0.001)
            }
        };
        let tw = (VW - 96.0) / NB as f32;
        for b in 0..NB {
            let x = VX + tw * b as f32;
            let base = band_base(b);
            let on = self.sget(base + ON) > 0.5;
            self.sq_tog(base + ON, x + 7.0, y + 4.0, 8.0);
            self.set_dk(if self.sget(8) > 0.5 { 0.5 } else { 1.0 });
            if self.btn(x, y, tw, 16.0, "", (b == sel) as u8) {
                self.select_band(b);
            }
            // two-letter label left (after the on / off square), frequency range right, no unit
            if b == sel {
                self.cv.setbg();
            } else {
                self.setc(false);
            }
            self.cv.set_font(1);
            self.cv.text(x + 20.0, y + 4.0, BAND_LETTERS[b]);
            self.cv.textr(x, y + 4.0, tw - 5.0, &format!("{}-{}", short(ranges[b].0), short(ranges[b].1)));
            self.setc(false);
            self.sqind(x + 7.0, y + 4.0, 8.0, on, b == sel, !on);
        }
        let base = band_base(sel);
        let (d, s) = (self.sget(base + DELTA) > 0.5, self.sget(base + SOLO) > 0.5);
        if self.btn(VX + VW - 96.0, y, 48.0, 16.0, "DELTA", if d { 2 } else { 0 }) {
            self.click_set(base + DELTA, if d { 0.0 } else { 1.0 });
        }
        if self.btn(VX + VW - 48.0, y, 48.0, 16.0, "SOLO", if s { 2 } else { 0 }) {
            self.click_set(base + SOLO, if s { 0.0 } else { 1.0 });
        }

    }

    fn band_cells(&mut self, y0: f32) {
        let b = self.sel();
        let o = band_base(b);
        let rows = [y0, y0 + CELL_P, y0 + 2.0 * CELL_P, y0 + 3.0 * CELL_P];
        let ph = 3.0 * CELL_P + CELL_H;
        // saturator module: the transfer plot is square (rows 2-4), selector and drive row above it
        let sat_w = ph - CELL_P;
        let sat_x = VX + sat_w + 6.0;
        // compressor section sits flush right (GR meter + three columns); what is left over is the gap to the saturator
        let col0 = VX + VW - (3.0 * CELL_W + 12.0);
        let meter_x = col0 - 6.0 - 30.0;
        self.shape_selector(o + SHAPE, VX, y0, sat_w, 19.0);
        let drive_w = (sat_w * 0.5).floor();
        self.cell1(o + DRIVE, VX, y0 + 19.0, drive_w, 19.0, "DRIVE");
        self.cell1(o + BIAS, VX + drive_w - 1.0, y0 + 19.0, sat_w - drive_w + 1.0, 19.0, "BIAS");
        self.draw_sat_plot(VX, y0 + CELL_P, sat_w, sat_w);
        // The theme editor's 4 x 3 grid starts right of the saturator's left half, so while editing the saturator's right
        // column (ASYM / TONE / HP / LP / SAT MIX / SAT GAIN), its band VU and the GR meter are hidden to give it the room.
        if !self.app.editing() {
            // two stacked slider groups (the halves share one border line): ASYM over TONE, HP over LP
            self.asym_cell(b, sat_x, rows[0], CELL_W, 19.0);
            self.cell1(o + TONE, sat_x, rows[0] + 18.0, CELL_W, 20.0, "TONE");
            self.cell1(o + HP, sat_x, rows[1], CELL_W, 19.0, "HP");
            self.cell1(o + LP, sat_x, rows[1] + 18.0, CELL_W, 20.0, "LP");
            self.cell(o + SAT_MIX, sat_x, rows[2], CELL_W, CELL_H, "SAT MIX");
            self.cell_o(o + SAT_GAIN, sat_x, rows[3], CELL_W, CELL_H, "SAT GAIN");
            // band level after the saturator, in the gap between the saturator and compressor sections
            self.draw_vu(sat_x + CELL_W + 8.0, y0, 24.0, ph);
            // gain reduction meter
            self.draw_gr_meter(meter_x, y0, 30.0, ph);
        }
        if self.app.editing() {
            // theme editor: its cells take the compressor section (4 x 3 grid)
            self.theme_cells(CellGrid { x0: VX + VW - (3.0 * CELL_W + 12.0) - (CELL_W + 6.0), y0, cw: CELL_W, ch: CELL_H, px: CELL_W + 6.0, py: CELL_P });
            return;
        }
        // compressor: 3 columns x 4 rows
        let cells = [
            [(THRESH, "THRESH"), (RATIO, "RATIO"), (ATTACK, "ATTACK"), (RELEASE, "RELEASE")],
            [(KNEE, "KNEE"), (KNEE_SHAPE, "K.SHAPE"), (GR_DIST, "GR DIST"), (SPEED, "SPEED")],
            [(DETECTOR, "PEAK>RMS"), (FF_FB, "FF>FB"), (COMP_MIX, "COMP MIX"), (COMP_GAIN, "CMP GAIN")],
        ];
        for (c, col) in cells.iter().enumerate() {
            let cx = col0 + (CELL_W + 6.0) * c as f32;
            if c == 1 {
                // the COMP DOWN / UP switch over RANGE, KNEE over K.SHAPE (two stacked groups, the halves share a border line),
                // then GR DIST and SPEED as single cells
                self.up_group(b, cx, rows[0]);
                self.cell1(o + KNEE, cx, rows[1], CELL_W, 19.0, "KNEE");
                self.cell1(o + KNEE_SHAPE, cx, rows[1] + 18.0, CELL_W, 20.0, "K.SHAPE");
                self.cell(o + GR_DIST, cx, rows[2], CELL_W, CELL_H, "GR DIST");
                self.cell(o + SPEED, cx, rows[3], CELL_W, CELL_H, "SPEED");
                continue;
            }
            for (r, (off, lbl)) in col.iter().enumerate() {
                if *off == COMP_GAIN {
                    self.cell_o(o + off, cx, rows[r], CELL_W, CELL_H, lbl);
                } else if *off == FF_FB && self.sget(SC_ON) > 0.5 {
                    // KEY MIX stands in for FF>FB while the sidechain is on (one global value for all bands)
                    self.cell(SC_MIX, cx, rows[r], CELL_W, CELL_H, "KEY MIX");
                } else {
                    self.cell(o + off, cx, rows[r], CELL_W, CELL_H, lbl);
                }
            }
        }
    }

    /// Compressor mode switch (DWNWARD / UPWARD / UP/DWN, click or wheel steps through them) with the RANGE slider (the most
    /// the upward part lifts) stacked under it; the range reads dim while the band is in the plain downward mode.
    fn up_group(&mut self, b: usize, x: f32, y: f32) {
        let n = band_base(b) + UP;
        let mode = (self.sget(n) + 0.5).floor().clamp(0.0, 2.0) as i32;
        let label = ["DWNWARD", "UPWARD", "UP/DWN"][mode as usize];
        if self.btn(x, y, CELL_W, 19.0, label, (mode > 0) as u8) {
            self.click_set(n, ((mode + 1) % 3) as f32);
        } else if self.hit(x, y, CELL_W, 19.0) && self.kit.wheel != 0 {
            let nx = (mode + if self.kit.wheel > 0 { 1 } else { -1 }).clamp(0, 2);
            if nx != mode {
                self.click_set(n, nx as f32);
            }
        }
        let keep = self.kit.dk;
        if mode == 0 {
            self.set_dk(keep * 0.5);
        }
        self.cell1(range_slot(b), x, y + 18.0, CELL_W, 20.0, "RANGE");
        self.set_dk(keep);
        self.setc(false);
    }

    fn out_column(&mut self, y0: f32) {
        let sh = self.env.shared;
        let gmatch = self.sget(7) > 0.5;
        let meas = sh.out_meas.load(Ordering::Relaxed);
        self.setc(false);
        self.cv.set_font(1);
        let (lbl, val) = if gmatch {
            if meas {
                ("MEAS", format!("{:.1}/3.0s", sh.out_mt.load(Ordering::Relaxed)))
            } else {
                ("LOCK", format!("{:+05.1}dB", self.env.params.match_trim.load(Ordering::Relaxed)))
            }
        } else {
            ("TRIM", "-----dB".to_string())
        };
        self.cv.text(RX, y0 + 4.0, lbl);
        self.cv.textr(RX, y0 + 4.0, RW, &val);
        let blink = ((self.kit.now * 4.0).floor() as i64) & 1 == 1;
        let lit = if gmatch { if meas && blink { 1 } else { 2 } } else { 0 };
        if self.btn(RX, y0 + 18.0, RW, 16.0, "GAIN MATCH", lit) {
            if gmatch {
                self.click_set(7, 0.0);
            } else {
                self.click_set(7, 1.0);
                sh.match_req.store(true, Ordering::Relaxed);
            }
        }
        self.diff_box(RX, y0 + CELL_P, RW, CELL_H);
        self.cell(5, RX, y0 + 2.0 * CELL_P, RW, CELL_H, "MIX");
        self.cell_o(6, RX, y0 + 3.0 * CELL_P, RW, CELL_H, "OUT GAIN");
    }

    // ------------------------------------------------------------------ EQ page
    /// filter glyphs. kind: 0 HPF, 1 LPF, 2 low shelf, 3 high shelf, 4 bell, 5 all-pass, 6 band-pass, 7 notch
    fn eq_glyph(&mut self, kind: u8, cx: f32, cy: f32) {
        let gk = 0.75;
        let (gx0, gy0) = (cx - 9.0 * gk, cy - 5.5 * gk);
        let seg = |ax: f32, ay: f32, bx: f32, by: f32| {
            self.cv.line(gx0 + ax * gk, gy0 + ay * gk, gx0 + bx * gk, gy0 + by * gk);
        };
        match kind {
            0 => {
                seg(0.85, 8.0, 5.23, 1.0);
                seg(5.23, 1.0, 18.85, 1.0);
            }
            1 => {
                seg(0.0, 1.0, 13.6, 1.0);
                seg(13.6, 1.0, 18.0, 8.0);
            }
            2 => {
                seg(0.0, 1.0, 6.88, 1.0);
                seg(6.88, 1.0, 11.5, 9.0);
                seg(11.5, 9.0, 18.0, 9.0);
            }
            3 => {
                seg(0.0, 9.0, 6.5, 9.0);
                seg(6.5, 9.0, 11.1, 1.0);
                seg(11.1, 1.0, 18.0, 1.0);
            }
            5 => {
                // flat magnitude, phase turn: a line with a dot
                seg(0.0, 5.0, 18.0, 5.0);
                seg(7.0, 2.0, 11.0, 2.0);
                seg(7.0, 8.0, 11.0, 8.0);
                seg(7.0, 2.0, 7.0, 8.0);
                seg(11.0, 2.0, 11.0, 8.0);
            }
            6 => {
                seg(0.0, 10.0, 4.5, 10.0);
                seg(4.5, 10.0, 7.5, 1.0);
                seg(7.5, 1.0, 10.5, 1.0);
                seg(10.5, 1.0, 13.5, 10.0);
                seg(13.5, 10.0, 18.0, 10.0);
            }
            7 => {
                seg(0.0, 1.0, 5.69, 1.0);
                seg(5.69, 1.0, 9.27, 9.0);
                seg(9.27, 9.0, 12.84, 1.0);
                seg(12.84, 1.0, 18.0, 1.0);
            }
            _ => {
                seg(0.0, 10.45, 5.69, 10.45);
                seg(5.69, 10.45, 9.27, 2.45);
                seg(9.27, 2.45, 12.84, 10.45);
                seg(12.84, 10.45, 18.0, 10.45);
            }
        }
    }

    /// the 7 filters of the EQ (5 bands + tilt pair) as (coefficients, active)
    fn eq_filters(&self) -> Vec<([f64; 5], bool)> {
        let sr = self.env.shared.sample_rate.load(Ordering::Relaxed) as f64;
        let eq = &self.env.dsp.eq;
        let mut v = Vec::new();
        for b in 0..5 {
            let bp = &eq.bands[b];
            let c = eq_coef(eq_kind(b, bp.ty), bp.freq.clamp(20.0, 20000.0), bp.gain.clamp(-18.0, 18.0), bp.q.clamp(0.1, 10.0), sr);
            v.push((c, bp.on));
        }
        let t = eq.tilt.clamp(-12.0, 12.0);
        let pv = if eq.pivot < 19.5 { 600.0 } else { eq.pivot.min(20000.0) };
        v.push((eq_coef(2, pv, -t * 0.5, 0.4, sr), t != 0.0));
        v.push((eq_coef(3, pv, t * 0.5, 0.4, sr), t != 0.0));
        v
    }

    /// EQ viewport: the spectrum behind, the summed response curve and the five draggable band nodes
    fn draw_eq(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.draw_spectrum(x, y, w, h);
        let sr = self.env.shared.sample_rate.load(Ordering::Relaxed) as f64;
        let filters = self.eq_filters();
        let (x0, pw) = (x + 1.0, w - 2.0);
        let (cy, span) = (y + (h * 0.5).floor(), h * 0.5 - 8.0);
        let gy = |db: f32| cy - (db / 18.0).clamp(-1.0, 1.0) * span;
        // 0 dB line, dotted, in the background colour then UI colour so it reads over the spectrum
        self.setc(false);
        let mut xx = x0;
        while xx < x0 + pw {
            self.cv.fill(xx, cy, 2.0, 1.0);
            xx += 4.0;
        }
        // pivot of the tilt: dashed vertical line (drag = pivot, drag empty space sideways = tilt)
        let pvx = freq_to_x(self.env.dsp.eq.pivot as f32, x0, pw);
        self.setc(self.kit.act == ACT_PIVOT);
        let dash = if self.kit.act == ACT_PIVOT { 3.0 } else { 2.0 };
        for i in 0..((h - 2.0) / 3.0).floor() as i32 {
            self.cv.fill(pvx.floor(), y + 1.0 + i as f32 * 3.0, 1.0, dash);
        }
        // response curve
        let wk = |f: f32| 2.0 * std::f64::consts::PI * (f as f64).min(sr * 0.49) / sr;
        self.setc(true);
        let n = pw as i32;
        let mut last: Option<(f32, f32)> = None;
        for i in 0..=n {
            let f = x_to_freq(x0 + i as f32 * pw / n as f32, x0, pw);
            let mut db = 0.0f64;
            for (c, on) in &filters {
                if *on {
                    db += mag_db(c, wk(f));
                }
            }
            let p = (x0 + i as f32 * pw / n as f32, gy(db as f32));
            if let Some(l) = last {
                self.cv.line(l.0, l.1, p.0, p.1);
                self.cv.line(l.0, l.1 + 1.0, p.0, p.1 + 1.0);
            }
            last = Some(p);
        }
        // nodes
        let eq = self.env.dsp.eq;
        let mut best: Option<usize> = None;
        let mut bd = 100.0f32;
        let mut pos = [(0.0f32, 0.0f32); 5];
        for b in 0..5 {
            let bp = &eq.bands[b];
            let has_gain = matches!(eq_kind(b, bp.ty), 2 | 3 | 4);
            pos[b] = (freq_to_x(bp.freq as f32, x0, pw), if has_gain { gy(bp.gain as f32) } else { cy });
            let d2 = (self.kit.mx - pos[b].0).powi(2) + (self.kit.my - pos[b].1).powi(2);
            if d2 < bd {
                bd = d2;
                best = Some(b);
            }
        }
        let hv = self.hit(x, y, w, h);
        if hv && self.kit.wheel != 0 {
            if let Some(b) = best {
                self.wheel_adj(eq_base(b) + 3);
            }
        }
        if hv && self.kit.press && self.kit.act == 0 {
            if let Some(b) = best {
                self.eq_sel = b;
                if self.dbl(140 + b as i32) {
                    self.reset_eq_band(b);
                } else {
                    if self.sget(eq_base(b)) < 0.5 {
                        self.click_set(eq_base(b), 1.0);
                    }
                    self.kit.act = ACT_NODE + b as i32;
                }
            } else if (self.kit.mx - pvx).abs() <= 3.0 {
                if self.dbl(145) {
                    let d = self.kit.meta[EQ_PIVOT].def;
                    self.click_set(EQ_PIVOT, d);
                } else {
                    self.kit.act = ACT_PIVOT;
                    self.gdx = pvx - self.kit.mx;
                }
            } else if self.dbl(146) {
                self.click_set(EQ_TILT, 0.0);
            } else {
                self.kit.act = ACT_TILT;
                self.tacc = self.sget(EQ_TILT);
                self.tlast = self.kit.mx;
            }
            self.kit.press = false;
        }
        if self.kit.act == ACT_PIVOT && self.kit.down {
            let f = x_to_freq(self.kit.mx + self.gdx, x0, pw);
            self.setv(EQ_PIVOT, f);
        }
        if self.kit.act == ACT_TILT && self.kit.down {
            self.tacc += (self.kit.mx - self.tlast) * (24.0 / pw) * if self.kit.shift { 0.15 } else { 1.0 };
            self.tlast = self.kit.mx;
            self.tacc = self.tacc.clamp(-12.0, 12.0);
            let t = self.tacc;
            self.setv(EQ_TILT, t);
        }
        if self.kit.act >= ACT_NODE && self.kit.act < ACT_NODE + 5 && self.kit.down {
            let b = (self.kit.act - ACT_NODE) as usize;
            let f = x_to_freq(self.kit.mx, x0, pw);
            self.setv(eq_base(b) + 2, f);
            if matches!(eq_kind(b, self.env.dsp.eq.bands[b].ty), 2 | 3 | 4) {
                let g = ((cy - self.kit.my) / span * 18.0).clamp(-18.0, 18.0);
                self.setv(eq_base(b) + 4, g);
            }
        }
        for b in 0..5 {
            let on = eq.bands[b].on;
            let (px, py) = pos[b];
            let sel = b == self.eq_sel;
            let bw = 12.0;
            self.setc(false);
            if on {
                self.cv.fill(px.floor() - 6.0, py.floor() - 6.0, bw, bw);
                self.cv.setbg();
            } else {
                self.cv.setbg();
                self.cv.fill(px.floor() - 6.0, py.floor() - 6.0, bw, bw);
                self.setc(false);
                self.cv.boxr(px.floor() - 6.0, py.floor() - 6.0, bw, bw);
            }
            if sel {
                self.setc(true);
                self.cv.boxr(px.floor() - 8.0, py.floor() - 8.0, bw + 4.0, bw + 4.0);
                if on {
                    self.setc(false);
                    self.cv.setbg();
                } else {
                    self.setc(false);
                }
            } else if !on {
                self.setc(false);
            }
            self.cv.set_font(1);
            self.cv.textc(px.floor() - 6.0, py.floor() - 4.0, bw, &format!("{}", b + 1));
        }
        self.setc(false);
        self.cv.set_font(1);
        let t = self.env.dsp.eq.tilt;
        let txt = if t.abs() < 0.05 { "TILT 0.0dB".to_string() } else { format!("TILT {:+.1}dB", t) };
        self.cv.text(x + 3.0, y + 2.0, &txt);
    }

    /// the five EQ band toggles: type name while on, "EQ n" while off
    fn eq_row(&mut self, y: f32) {
        for b in 0..5 {
            let x = VX + EQ_P * b as f32;
            let on = self.sget(eq_base(b)) > 0.5;
            let ty = (self.sget(eq_base(b) + 1) + 0.5).floor() as i32;
            let label = if on { KIND_NAMES[eq_kind(b, ty) as usize].to_string() } else { format!("EQ {}", b + 1) };
            if self.btn(x, y, EQ_W, 16.0, &label, on as u8) {
                self.eq_sel = b;
                if self.dbl(150 + b as i32) {
                    self.reset_eq_band(b);
                } else {
                    self.click_set(eq_base(b), if on { 0.0 } else { 1.0 });
                }
            }
        }
        let d = self.rd_eq_diff;
        self.diff_row(VX + EQ_P * 5.0, y, EQ_PW, 16.0, "I>O", d);
    }

    fn eq_cells(&mut self, y0: f32) {
        // the whole page dims while the EQ (or the plug-in) is bypassed
        let page_dk = if self.sget(EQ_BYPASS) > 0.5 || self.sget(8) > 0.5 { 0.5 } else { 1.0 };
        for b in 0..5 {
            let x = VX + EQ_P * b as f32;
            let o = eq_base(b);
            let on = self.sget(o) > 0.5;
            self.set_dk(page_dk * if on { 1.0 } else { 0.5 });
            // type glyph grid (2 x 3)
            let ty = (self.sget(o + 1) + 0.5).floor() as i32;
            self.setc(false);
            self.cv.boxr(x, y0, EQ_W, CELL_H);
            for t in 0..6usize {
                let gp = ((CELL_H - 2.0) * 0.5).floor();
                let (gx, gy) = (x + 1.0 + (t % 3) as f32 * EQ_W / 3.0, y0 + 1.0 + (t / 3) as f32 * gp);
                let cur = t as i32 == ty;
                if self.hit(gx, gy, EQ_W / 3.0 - 1.0, gp - 1.0) && self.kit.press && self.kit.act == 0 {
                    self.click_set(o + 1, t as f32);
                    self.kit.press = false;
                }
                self.setc(false);
                if cur {
                    self.cv.fill(gx, gy, EQ_W / 3.0 - 1.0, gp - 1.0);
                    self.cv.setbg();
                }
                self.eq_glyph(EQ_KINDS[b][t], gx + EQ_W / 6.0, gy + (gp - 1.0) * 0.5);
                self.setc(false);
            }
            self.cell(o + 3, x, y0 + CELL_P, EQ_W, CELL_H, "Q");
            self.cell(o + 2, x, y0 + 2.0 * CELL_P, EQ_W, CELL_H, "FREQ");
            if matches!(eq_kind(b, ty), 2 | 3 | 4) {
                self.cell(o + 4, x, y0 + 3.0 * CELL_P, EQ_W, CELL_H, "GAIN");
            } else {
                self.setc(false);
                self.cv.boxr(x, y0 + 3.0 * CELL_P, EQ_W, CELL_H);
                self.cv.set_font(1);
                self.cv.text(x + 3.0, y0 + 3.0 * CELL_P + 3.0, "GAIN");
                self.cv.set_font(2);
                self.cv.textc(x, y0 + 3.0 * CELL_P + (CELL_H * 0.4 + 1.0).floor(), EQ_W, "---");
            }
        }
        self.set_dk(page_dk);
        let x = VX + EQ_P * 5.0;
        self.cell(EQ_PIVOT, x, y0, EQ_PW, CELL_H, "PIVOT");
        self.cell(EQ_TILT, x, y0 + CELL_P, EQ_PW, CELL_H, "TILT");
        self.cell(EQ_MIX, x, y0 + 2.0 * CELL_P, EQ_PW, CELL_H, "EQ MIX");
        self.cell_o(EQ_GAIN, x, y0 + 3.0 * CELL_P, EQ_PW, CELL_H, "EQ GAIN");
    }

    // ------------------------------------------------------------------ frame
    pub fn frame(&mut self) {
        let sh = self.env.shared;
        #[cfg(test)]
        {
            if let Some(p) = self.s.test_apply.take() {
                self.apply_preset(&p);
            }
            if std::mem::take(&mut self.s.test_want_capture) {
                self.s.test_capture = Some(self.capture("test"));
            }
        }
        // a finished sat level match writes that band's SAT GAIN (the audio thread cannot set parameters)
        for b in 0..NB {
            if sh.sat_result_new[b].swap(false, Ordering::Acquire) {
                let v = sh.sat_result[b].load(Ordering::Relaxed);
                self.click_set(band_base(b) + SAT_GAIN, v);
            }
        }
        // readouts refresh ~8x/s
        if self.kit.now - self.rd_last >= 0.12 {
            self.rd_last = self.kit.now;
            self.rd_pk = self.opk_max;
            for b in 0..NB {
                self.rd_sat[b] = sh.sat_db[b].load(Ordering::Relaxed);
            }
            self.rd_rms = sh.fms.load(Ordering::Relaxed);
            self.rd_diff = sh.diff_disp.load(Ordering::Relaxed);
            self.rd_eq_diff = sh.eq_diff.load(Ordering::Relaxed);
        }
        // Theme editor: every widget draws as a dummy with the input masked; only the editor cells and the picker get it.
        let editing = self.app.editing();
        self.begin_frame_editing();
        if editing {
            self.logo_pend = -1.0;
            self.eq_page = false;
            self.curve_edit = false;
            self.menu = None;
        }
        self.env.dsp = self.env.params.to_dsp();
        self.update_gr();
        self.menu_frame();

        // drag update (single active widget): slots (vertical drag) below VIEW_ACT, viewport drags from VIEW_ACT
        if self.kit.act != 0 && self.kit.act < VIEW_ACT {
            let a = self.kit.act as usize;
            if self.kit.down {
                let k = self.kit.shift;
                let t_new = (self.kit.act_t + (self.kit.last_my - self.kit.my) * if k { 0.0008 } else { 0.006 }).clamp(0.0, 1.0);
                self.kit.act_t = t_new;
                self.commit(a, t_new);
            } else {
                self.end_touch(a);
                self.kit.act = 0;
            }
        }
        if self.kit.act >= VIEW_ACT && !self.kit.down {
            for n in XOV {
                self.end_touch(n);
            }
            self.end_touch(EQ_TILT);
            self.end_touch(EQ_PIVOT);
            for b in 0..NB {
                self.end_touch(band_base(b) + THRESH);
            }
            for b in 0..5 {
                for k in 2..5 {
                    self.end_touch(eq_base(b) + k);
                }
            }
            self.kit.act = 0;
        }
        self.kit.last_my = self.kit.my;

        let byp = self.sget(8) > 0.5;
        let eq_byp = self.sget(EQ_BYPASS) > 0.5;
        let band_on = self.sget(band_base(self.sel()) + ON) > 0.5;

        self.resize_grip();

        // ---------- header: logo, title, preset bar, sidechain (not wired yet), EQ page toggle, output meter
        self.set_dk(1.0);
        self.logo(13.0, 5.0);
        self.setc(false);
        self.cv.set_font(2);
        let title = format!("SATXCOMP {}", version_short());
        let tw = self.cv.text_w(&title);
        if self.hit(42.0, 5.0, tw + 4.0, 21.0) && self.kit.press && self.kit.act == 0 {
            self.app.about = if self.app.about == 0 { 1 } else { 0 };
            self.app.about_scroll = 0;
            self.kit.press = false;
        }
        self.cv.text(44.0, 12.0, &title);
        self.preset_bar();
        // sidechain key: white = on and the host gave a bus, red = on but no bus
        let sc_on = self.sget(SC_ON) > 0.5;
        let sc_lit = if !sc_on { 0 } else if sh.sc_bus.load(Ordering::Relaxed) { 1 } else { 2 };
        if self.btn(402.0, 8.0, 76.0, 16.0, "SIDECHAIN", sc_lit) {
            self.click_set(SC_ON, if sc_on { 0.0 } else { 1.0 });
        }
        if !sc_on && sh.sc_solo.load(Ordering::Relaxed) {
            sh.sc_solo.store(false, Ordering::Relaxed); // solo never outlives the sidechain switch
        }
        {
            // EQ page toggle; the square in it bypasses the whole EQ module (X = bypassed, like SatComp)
            let eq = self.env.dsp.eq;
            let eq_any = eq.bands.iter().any(|b| b.on) || eq.tilt != 0.0 || eq.gain != 0.0;
            self.sq_tog(EQ_BYPASS, 486.0, 12.0, 8.0);
            if self.btn(478.0, 8.0, 46.0, 16.0, "", self.eq_page as u8) {
                self.eq_page = !self.eq_page;
            }
            self.sqind(486.0, 12.0, 8.0, eq_any, self.eq_page, eq_byp);
            // label drawn by hand so it keeps a clear gap from the square
            if self.eq_page {
                self.cv.setbg();
            } else {
                self.setc(false);
            }
            self.cv.set_font(1);
            self.cv.text(501.0, 12.0, "EQ");
            self.setc(false);
        }
        // LIM switch + output peak meter
        let lim_on = self.sget(LIM_ON) > 0.5;
        if self.btn(RX, 8.0, 30.0, 16.0, "LIM", lim_on as u8) {
            self.click_set(LIM_ON, if lim_on { 0.0 } else { 1.0 });
        }
        self.out_meter(RX + 34.0, 8.0, RW - 34.0, 16.0);

        // ---------- viewport row; right column: delta / bypass, levels, then the parameter group on the bottom edge
        let delta = sh.delta_on.load(Ordering::Relaxed);
        if self.btn(RX, VY, RW * 0.5, 16.0, "DELTA", if delta { 2 } else { 0 }) {
            sh.delta_on.store(!delta, Ordering::Relaxed);
        }
        if self.btn(RX + RW * 0.5, VY, RW * 0.5, 16.0, "BYPASS", byp as u8) {
            self.click_set(8, if byp { 0.0 } else { 1.0 });
        }
        let ab = self.app.about > 0;
        self.set_dk(if byp || (self.eq_page && eq_byp) { 0.5 } else { 1.0 });
        if ab {
            // About / Appearance / Changelog replace the viewport and the band section (the right column stays)
            self.set_dk(if byp { 0.5 } else { 1.0 });
            self.draw_about(Rect::new(VX, VY + 22.0, VW, CELLS_Y + 3.0 * CELL_P + CELL_H - VY - 22.0), byp);
        } else if self.eq_page {
            self.draw_eq(VX, VY, VW, VH);
        } else {
            // the sidechain controls and the scope settings take the bottom of the viewport while they are on
            let scope_on = self.s.scope_on();
            let (sc_h, sp_h) = (if sc_on { SC_ROW_H } else { 0.0 }, if scope_on { SC_ROW_H } else { 0.0 });
            let vh = VH - sc_h - sp_h;
            self.scope_icon(false);
            if self.s.scope_on() {
                self.draw_scope(VX, VY, VW, vh);
            } else {
                self.draw_spectrum(VX, VY, VW, vh);
                self.draw_crossover(VX, VY, VW, vh);
            }
            self.scope_icon(true);
            self.set_dk(if byp { 0.5 } else { 1.0 });
            if sc_on {
                self.sc_row(VX, VY + vh);
            }
            if scope_on {
                self.scope_row(VX, VY + vh + sc_h);
            }
        }
        self.set_dk(1.0);
        self.cv.set_font(1);
        self.setc(false);
        // limiter reduction first (same order as SatComp): accent once it has engaged
        let lim_on = self.sget(LIM_ON) > 0.5;
        self.setc(lim_on && self.lim_txt > 0.05);
        self.cv.text(RX, VY + 22.0, "LIM");
        let s1 = if lim_on { format!("{:04.1}dB", self.lim_txt) } else { "-----dB".to_string() };
        self.cv.textr(RX, VY + 22.0, RW, &s1);
        let s1 = if self.rd_pk > 0.00001 { format!("{:+05.1}dB", 8.6858896 * self.rd_pk.ln()) } else { "-----dB".into() };
        self.setc(self.rd_pk > 0.9999);
        self.cv.text(RX, VY + 34.0, "PK");
        self.cv.textr(RX, VY + 34.0, RW, &s1);
        self.setc(false);
        let s1 = if self.rd_rms > 0.000000001 { format!("{:+05.1}dB", 4.3429448 * self.rd_rms.ln()) } else { "-----dB".into() };
        self.cv.text(RX, VY + 46.0, "RMS");
        self.cv.textr(RX, VY + 46.0, RW, &s1);
        // parameter group, evenly spaced and aligned to the viewport's bottom edge: three crossovers, oversampling
        let os_y = VY + VH - 16.0;
        for k in 0..3 {
            let y = os_y - (3 - k) as f32 * 22.0;
            self.xover_row(k, RX, y);
        }
        self.set_dk(1.0); // oversampling stays lit while bypassed
        let os_i = (self.sget(3) + 0.5).floor() as i32;
        for i in 0..3 {
            let l = ["x1", "x2", "x4"][i as usize];
            if self.btn(RX + i as f32 * RW / 3.0, os_y, RW / 3.0, 16.0, l, (os_i == i) as u8) {
                self.click_set(3, i as f32);
            }
        }
        self.set_dk(1.0);

        // ---------- band row, band cells, output column
        if ab {
        } else if self.eq_page {
            self.set_dk(if byp || eq_byp { 0.5 } else { 1.0 });
            self.eq_row(BAND_Y);
            self.eq_cells(CELLS_Y);
        } else {
            self.band_row(BAND_Y);
            self.set_dk(if byp || !band_on { 0.5 } else { 1.0 });
            self.band_cells(CELLS_Y);
        }
        // lookahead belongs to the output module: visible on both pages
        self.set_dk(1.0);
        self.cell1(4, RX, BAND_Y, RW, 16.0, "L.AHEAD");
        self.out_column(CELLS_Y);
        self.menu_draw();
        self.poll_dialog();
        self.run_pending();
        if editing {
            // the colour picker / on-screen keyboard sit over the viewport, live on top of the masked dummy page
            let over = Rect::new(VX + ((VW - 266.0) * 0.5).floor(), VY + ((VH - 128.0) * 0.5).floor(), 266.0, 128.0);
            self.unmask_in();
            let osk = self.app.osk_open();
            if osk {
                self.mask_in(); // picker stays visible underneath but inert
            }
            self.theme_picker(over);
            if osk {
                self.unmask_in();
                self.theme_osk(over);
            }
        }
    }
}
