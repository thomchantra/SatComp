//! SatXComp pages: header, tab row, spectrum / crossover viewport, the three band tabs with their saturator and compressor
//! cells, and the output column. Widgets (cells, buttons, drag logic) come from `satcomp_ui::kit`.

use crate::params::{SatXCompParams, SLOTS, TABLE};
use crate::Shared;
use nih_plug::prelude::*;
use satcomp_dsp::biquad::{eq_coef, mag_db};
use satcomp_dsp::shapers::{Curve, ShaperLut, CUSTOM, MAX_PTS, MIN_GAP, SHAPER_NAMES};
use satcomp_ui::canvas::{Canvas, PALETTES};
use satcomp_ui::kit::{Kit, Meta, Widgets};
use satcomp_ui::spectrum::{freq_to_x, x_to_freq, Spectrum};
use satxcomp_dsp::{eq_kind, XParams, EQ_KINDS};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;

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

/// first UI slot of EQ band `i`: on, type, freq, q, gain
const fn eq_base(i: usize) -> usize {
    90 + 5 * i
}
const EQ_TILT: usize = 115;
const EQ_PIVOT: usize = 116;
const EQ_MIX: usize = 117;
const EQ_GAIN: usize = 118;
const EQ_BYPASS: usize = 119;
const X1_SLOPE: usize = 82;
const X2_SLOPE: usize = 83;
const SC_ON: usize = 84;
const SC_GAIN: usize = 85;
const SC_HP: usize = 86;
const SC_LP: usize = 87;
const SC_MIX: usize = 88;
/// height of the sidechain control row under the spectrum
const SC_ROW_H: f32 = 17.0;
// ---- layout (logical units; the window is W x H = 680 x 480)
/// viewport (spectrum / crossover / EQ curve)
const VX: f32 = 10.0;
const VY: f32 = 34.0;
const VW: f32 = 530.0;
const VH: f32 = 150.0;
/// right-hand column (levels, crossover sliders, output module)
const RX: f32 = 550.0;
const RW: f32 = 120.0;
/// row of band tabs / EQ band toggles under the viewport
const BAND_Y: f32 = VY + VH + 6.0;
/// first row of cells; four rows, CELL_P apart
const CELLS_Y: f32 = BAND_Y + 22.0;
const CELL_H: f32 = 44.0;
const CELL_P: f32 = 48.0;
/// band module cells (saturator / compressor)
const CELL_W: f32 = 74.0;
/// spectrum look: true = solid UI-colour fill under the output, false = background fill with a stroke (accent shows the gap to dry)
const FFT_SOLID_FILL: bool = false;
/// EQ page columns: five bands plus the pivot column, across the viewport width
const EQ_W: f32 = 80.0;
const EQ_P: f32 = 86.0;
const EQ_PW: f32 = VW - 5.0 * EQ_P;
/// window scale steps (x the 680 x 410 base)
const SIZES: [f32; 6] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5];
/// `kit.act` values: 1..VIEW_ACT are parameter slots (vertical drag, slots run to 119); VIEW_ACT and up are viewport drags
const VIEW_ACT: i32 = 1000;
const ACT_X1: i32 = VIEW_ACT + 1;
const ACT_X2: i32 = VIEW_ACT + 2;
/// EQ node `b` is ACT_NODE + b
const ACT_NODE: i32 = VIEW_ACT + 3;
/// EQ viewport: drag the dashed pivot line / drag empty space sideways for tilt
const ACT_PIVOT: i32 = VIEW_ACT + 10;
const ACT_TILT: i32 = VIEW_ACT + 11;
/// custom-curve point drag in the draw view
const ACT_CURVE: i32 = VIEW_ACT + 20;
/// band handle drag in the spectrum (threshold); band b is ACT_THR + b
const ACT_THR: i32 = VIEW_ACT + 30;
const MENU_ROWS: usize = 18;
const MENU_ROW_H: f32 = 14.0;

/// popup list (shaper dropdown): plain label rows, one checked
#[derive(Clone, Copy)]
enum MenuKind {
    /// shaper list of band `b`
    Shape(usize),
    Size,
}

struct Menu {
    kind: MenuKind,
    x: f32,
    y: f32,
    items: Vec<String>,
    checked: i32,
    scroll: usize,
}

const KIND_NAMES: [&str; 8] = ["HPF", "LPF", "LO SHELF", "HI SHELF", "BAND", "ALLPASS", "BANDPASS", "NOTCH"];

const BAND_NAMES: [&str; 3] = ["LOW", "MID", "HI"];
const BAND_LETTERS: [&str; 3] = ["L", "M", "H"];

pub struct State {
    pub kit: Kit,
    pub pal: usize,
    pub light: bool,
    pub face: usize,
    spec_out: Spectrum,
    spec_dry: Spectrum,
    gr_disp: [f32; 3],
    gr_hold: [f32; 3],
    gr_hold_t: [f64; 3],
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
    rd_rms: f32,
    rd_diff: f32,
    lut: ShaperLut,
    eq_page: bool,
    eq_sel: usize,
    /// custom-curve draw view of the selected band (pencil), and the point being dragged
    curve_edit: bool,
    edit_seen: bool,
    cpt: usize,
    menu: Option<Menu>,
    /// gain-reduction readouts refresh slowly (peak over the last interval) so the numbers stay readable
    gr_txt: [f32; 3],
    gr_txt_pk: [f32; 3],
    gr_txt_t: f64,
    /// per band post-saturator meter: decaying level, hold, and the last time it clipped
    vu: [f32; 3],
    vu_hold: [f32; 3],
    vu_hold_t: [f64; 3],
    vu_clip_t: [f64; 3],
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
        sp(1, 1, 0, 1.0);
        sp(2, 1, 0, 1.0);
        sp(4, 0, 0, 0.1);
        sp(6, 0, 0, 0.1);
        for b in 0..3 {
            let o = band_base(b);
            sp(o + DRIVE, 0, 0, 0.5);
            sp(o + BIAS, 0, 0, 1.0);
            sp(o + TONE, 1, 2, 10.0);
            sp(o + SAT_GAIN, 0, 0, 0.1);
            sp(o + THRESH, 0, 0, 0.1);
            sp(o + RATIO, 1, 2, 0.1);
            sp(o + ATTACK, 1, 0, 0.01);
            sp(o + RELEASE, 1, 2, 1.0);
            sp(o + KNEE, 0, 0, 0.1);
            sp(o + COMP_GAIN, 0, 0, 0.1);
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
        let pal = (params.palette_v2.load(Ordering::Relaxed).max(0) as usize).min(PALETTES.len() - 1);
        Self {
            kit,
            pal,
            light: params.light.load(Ordering::Relaxed),
            face: (params.font.load(Ordering::Relaxed).max(0) as usize).min(satcomp_ui::canvas::FONT_NAMES.len() - 1),
            spec_out: Spectrum::new(),
            spec_dry: Spectrum::new(),
            gr_disp: [0.0; 3],
            gr_hold: [0.0; 3],
            gr_hold_t: [0.0; 3],
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
            rd_rms: 0.0,
            rd_diff: 0.0,
            lut: ShaperLut::new(0),
            eq_page: false,
            eq_sel: 0,
            curve_edit: false,
            edit_seen: false,
            cpt: 0,
            menu: None,
            gr_txt: [0.0; 3],
            gr_txt_pk: [0.0; 3],
            gr_txt_t: 0.0,
            vu: [0.0; 3],
            vu_hold: [0.0; 3],
            vu_hold_t: [0.0; 3],
            vu_clip_t: [-10.0; 3],
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

    /// last published GR number of band `b` (tests)
    #[cfg(test)]
    pub fn gr_text(&self, b: usize) -> f32 {
        self.gr_txt[b]
    }

    pub fn set_input(&mut self, mx: f32, my: f32, down: bool, rdown: bool, scroll: f32, shift: bool, now: f64) {
        if self.kit.set_input(mx, my, down, rdown, scroll, shift, now) {
            // the egui clock restarts when the editor window is reopened: re-base every stored timestamp
            self.rd_last = now;
            self.hl_l_t = now;
            self.hl_r_t = now;
            self.gr_hold_t = [now; 3];
            self.gr_txt_t = now;
            self.vu_hold_t = [now; 3];
            self.vu_clip_t = [-10.0; 3];
            self.spec_out.rebase(now);
            self.spec_dry.rebase(now);
        }
    }

    /// [BG rgb, UI rgb, ACCENT rgb] of the active scheme and mode.
    pub fn colors(&self) -> [f32; 9] {
        let e = &PALETTES[self.pal.min(PALETTES.len() - 1)];
        if self.light {
            e.2
        } else {
            e.1
        }
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
    fn fmt(&self, n: usize, v: f32) -> String {
        fmt_slot(n, v)
    }
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
    if n == X1_SLOPE || n == X2_SLOPE {
        return format!("{}dB", [12, 24, 48, 96][(v + 0.5).floor().clamp(0.0, 3.0) as usize]);
    }
    if (10..82).contains(&n) {
        return match (n - 10) % 24 {
            DRIVE | SAT_MIX | SPEED | KNEE_SHAPE | DETECTOR | FF_FB | GR_DIST | COMP_MIX => format!("{:.0}%", v),
            BIAS => format!("{:+.0}%", v),
            TONE => {
                if v >= 19900.0 {
                    "OFF".into()
                } else {
                    format!("{:.1}k", v * 0.001)
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
    if (90..115).contains(&n) {
        return match (n - 90) % 5 {
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
        1 | 2 => hz(v),
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
        (self.env.params.sel_band.load(Ordering::Relaxed).max(0) as usize).min(2)
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
        let items = SIZES.iter().map(|v| format!("{:.0}%", v * 100.0)).collect();
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

    /// Header preset bar (save icon | name | prev next) and the window-scale button. Layout only for now: the preset
    /// controls are drawn but not wired.
    fn preset_bar(&mut self) {
        let (y, h) = (8.0, 16.0);
        let _ = self.btn(152.0, y, 26.0, h, "", 0);
        self.setc(false);
        self.cv.boxr(160.0, y + 3.0, 10.0, 10.0);
        self.cv.fill(163.0, y + 3.0, 4.0, 3.0);
        self.cv.boxr(162.0, y + 8.0, 6.0, 5.0);
        self.setc(false);
        self.cv.boxr(178.0, y, 138.0, h);
        self.cv.set_font(2);
        self.cv.textc(178.0, y + ((h - 8.0) * 0.5).floor(), 138.0, "-- INIT --");
        for (i, dir) in [(0, -1), (1, 1)] {
            let x = 316.0 + i as f32 * 25.0;
            let _ = self.btn(x, y, 25.0, h, "", 0);
            self.setc(false);
            let (cx, cy) = (x + 12.5, y + h * 0.5);
            if dir < 0 {
                self.cv.tri((cx + 2.0, cy - 3.0), (cx + 2.0, cy + 3.0), (cx - 2.0, cy));
            } else {
                self.cv.tri((cx - 2.0, cy - 3.0), (cx - 2.0, cy + 3.0), (cx + 2.0, cy));
            }
        }
        let label = format!("{:.0}%", self.cur_scale() * 100.0);
        if self.btn(372.0, y, 42.0, h, &label, 0) {
            self.open_size_menu(372.0, y + h);
        }
    }

    fn select_band(&mut self, b: usize) {
        if b != self.sel() {
            self.curve_edit = false; // the draw view belongs to one band
        }
        self.env.params.sel_band.store(b as i32, Ordering::Relaxed);
    }

    fn cycle_scheme(&mut self) {
        self.pal = (self.pal + 1) % PALETTES.len();
        self.env.params.palette_v2.store(self.pal as i32, Ordering::Relaxed);
    }

    fn toggle_light(&mut self) {
        self.light = !self.light;
        self.env.params.light.store(self.light, Ordering::Relaxed);
    }

    // ------------------------------------------------------------------ header
    /// plug-in icon: left click = next colour scheme, right click = light / dark
    fn logo(&mut self, x: f32, y: f32) {
        if self.hit(x - 3.0, y - 3.0, 27.0, 27.0) && self.kit.act == 0 {
            if self.kit.press {
                self.cycle_scheme();
                self.kit.press = false;
            }
            if self.kit.rpress {
                self.toggle_light();
            }
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

    // ------------------------------------------------------------------ meters
    fn update_gr(&mut self) {
        let sh = self.env.shared;
        let now = self.kit.now;
        for b in 0..3 {
            let g = sh.gr_pk[b].swap(0.0, Ordering::Relaxed);
            self.gr_disp[b] = g.max(self.gr_disp[b] * 0.9);
            if g >= self.gr_hold[b] {
                self.gr_hold[b] = g;
                self.gr_hold_t[b] = now;
            } else if now - self.gr_hold_t[b] > 1.0 {
                self.gr_hold[b] = (self.gr_hold[b] - 0.4).max(0.0);
            }
            self.gr_txt_pk[b] = self.gr_txt_pk[b].max(self.gr_disp[b]);
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
        if now - self.gr_txt_t >= 0.25 {
            self.gr_txt = self.gr_txt_pk;
            self.gr_txt_pk = [0.0; 3];
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
        let ac = self.cv.ac;
        let inv = [1.0 - ac[0], 1.0 - ac[1], 1.0 - ac[2]];
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        let mut prev: Option<f32> = None;
        for i in 0..n {
            let (ho, hd) = (hh(co[i]), hh(cd[i]));
            let (lo, hi) = (ho.min(hd), ho.max(hd));
            self.setc(false);
            if FFT_SOLID_FILL && lo > 0.0 {
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
            if !FFT_SOLID_FILL {
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
        let (f1, f2) = (self.env.dsp.x1 as f32, self.env.dsp.x2 as f32);
        let f2 = f2.max(f1 * 1.05);
        let (px1, px2) = (freq_to_x(f1, x + 1.0, w - 2.0), freq_to_x(f2, x + 1.0, w - 2.0));
        // drag a crossover line
        if self.hit(x, y, w, h) && self.kit.press && self.kit.act == 0 {
            let (d1, d2) = ((self.kit.mx - px1).abs(), (self.kit.mx - px2).abs());
            if d1.min(d2) < 6.0 {
                self.kit.act = if d1 <= d2 { ACT_X1 } else { ACT_X2 };
                self.kit.press = false;
            }
        }
        if self.kit.act == ACT_X1 && self.kit.down {
            let f = x_to_freq(self.kit.mx, x + 1.0, w - 2.0);
            self.setv(1, f.min(f2 * 0.95));
        } else if self.kit.act == ACT_X2 && self.kit.down {
            let f = x_to_freq(self.kit.mx, x + 1.0, w - 2.0);
            self.setv(2, f.max(f1 * 1.05));
        }
        let active = self.kit.act;
        // dotted lines
        self.setc(false);
        for (k, px) in [px1, px2].into_iter().enumerate() {
            let lit = active == ACT_X1 + k as i32;
            if lit {
                self.setc(true);
            }
            let mut yy = y + 1.0;
            while yy < y + h - 2.0 {
                self.cv.fill(px.floor(), yy, 1.0, 2.0);
                yy += 4.0;
            }
            self.setc(false);
        }
        // Band handles. The handle sits at the band's threshold on the spectrum's dB axis: drag = threshold (double-click =
        // default), click = select. A line marks the band's frequency range, a small bar hangs from the handle for the
        // gain reduction and its number sits to the right.
        let sel = self.sel();
        let centers = [(20.0 * f1).sqrt(), (f1 * f2).sqrt(), (f2 * 20000.0).sqrt()];
        let edges = [(x + 1.0, px1), (px1, px2), (px2, x + w - 1.0)];
        let y_of = |t: f32| y + h - 1.0 - ((t + 100.0) / 95.0).clamp(0.0, 1.0) * (h - 2.0);
        for b in 0..3 {
            let o = band_base(b);
            let cx = freq_to_x(centers[b], x + 1.0, w - 2.0).floor();
            let (bw, bx) = if b == sel { (26.0, cx - 13.0) } else { (14.0, cx - 7.0) };
            let cyy = y_of(self.sget(o + THRESH)).clamp(y + 7.0, y + h - 7.0).floor();
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
            let (xl, xr) = edges[b];
            self.cv.fill(xl.floor(), cyy, (xr - xl).floor().max(1.0), 1.0);
            // gain reduction bar from the centre of the handle, 2 px per dB
            let gr = self.gr_disp[b];
            if gr > 0.05 && on {
                self.setc(true);
                self.cv.fill(cx - 1.0, by + 12.0, 3.0, (gr * 2.0).min(36.0).floor().max(1.0));
            }
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
            // gain reduction number, right of the handle (left of it when there is no room), slow refresh
            let g = self.gr_txt[b];
            let txt = if g > 0.05 { format!("-{:.1}", g) } else { "0.0".to_string() };
            let tw = self.cv.text_w(&txt);
            let tx = if bx + bw + 3.0 + tw > x + w - 2.0 { bx - 3.0 - tw } else { bx + bw + 3.0 };
            self.setc(g > 0.05 && on);
            self.cv.text(tx, by + 12.0, &txt);
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
            let items = SHAPER_NAMES.iter().map(|s| s.to_string()).collect();
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

    // ------------------------------------------------------------------ popup menu
    /// (x, y, w, h, visible rows) of the open menu, clamped to the canvas.
    fn menu_geom(&mut self) -> Option<(f32, f32, f32, f32, usize)> {
        self.cv.set_font(1);
        let m = self.menu.as_ref()?;
        let tw = m.items.iter().map(|i| self.cv.text_w(i)).fold(0.0, f32::max);
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
                    chosen = Some((m.kind, (m.scroll + row as usize) as i32));
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
        let items: Vec<String> = m.items.iter().skip(m.scroll).take(rows).cloned().collect();
        let checked = m.checked - m.scroll as i32;
        let (more_up, more_down) = (m.scroll > 0, m.scroll + rows < m.items.len());
        self.cv.dk = 1.0;
        self.cv.setbg();
        self.cv.fill(x, y, mw, mh);
        self.setc(false);
        self.cv.boxr(x, y, mw, mh);
        self.cv.set_font(1);
        let hov = ((self.kit.my - y - 1.0) / MENU_ROW_H).floor() as i32;
        let inside = self.hit(x, y, mw, mh);
        for (i, label) in items.iter().enumerate() {
            let iy = y + 1.0 + i as f32 * MENU_ROW_H;
            self.setc(false);
            if inside && hov == i as i32 {
                self.cv.fill(x + 1.0, iy, mw - 2.0, MENU_ROW_H);
                self.cv.setbg();
            }
            if i as i32 == checked {
                self.cv.fill(x + 5.0, iy + 5.0, 4.0, 4.0);
            }
            self.cv.text(x + 14.0, iy + 1.0, label);
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
        // pencil: toggles the draw view; entering selects the Custom shaper so edits are audible
        let pen = self.hit(x + w - 14.0, y + 2.0, 12.0, 12.0);
        if pen && self.kit.press && self.kit.act == 0 {
            self.curve_edit = !self.curve_edit;
            self.edit_seen = false;
            if self.curve_edit {
                self.click_set(o + SHAPE, CUSTOM as f32);
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
        if hv && self.kit.press && self.kit.act == 0 {
            self.begin_drag(gain_n);
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
        self.pencil_glyph(x + w - 14.0, y + 2.0, self.curve_edit, pen);
        if self.curve_edit {
            self.draw_curve_edit(x, y, w, h);
            return;
        }
        let pk = sh.sat_vz[b].load(Ordering::Relaxed).min(1.0);
        let n = 64;
        let (mut lx, mut ly) = (0.0, 0.0);
        for i in 0..=n {
            let vx = -1.0 + 2.0 * i as f64 / n as f64;
            let mut vy = if sat_on { self.lut.eval(vx * pre + bias) } else { vx };
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
                let vy = self.lut.eval(sgn * pk as f64 * pre + bias).clamp(-1.0, 1.0) as f32;
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
    }

    /// Draw view of the selected band's custom curve (unity input, independent of drive/bias).
    /// Left click empty space = add a point (and drag it), drag = move, double click = remove.
    /// The two end points are fixed in x and cannot be removed.
    fn draw_curve_edit(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let b = self.sel();
        let mut c = self.env.params.curve_snapshot(b);
        let before = c;
        // domain [-1,1]^2 is inset so end points and the pencil never collide (symmetric margins keep the centre cross true)
        let (ex, ey, ew, eh) = (x + 9.0, y + 14.0, w - 18.0, h - 28.0);
        let (px, py) = (|vx: f32| ex + (vx + 1.0) * 0.5 * (ew - 1.0), |vy: f32| ey + (1.0 - (vy + 1.0) * 0.5) * (eh - 1.0));
        let to_v = |mx: f32, my: f32| (((mx - ex) / (ew - 1.0)) * 2.0 - 1.0, 1.0 - ((my - ey) / (eh - 1.0)) * 2.0);
        let pick = |s: &Self, c: &Curve| -> Option<usize> {
            let mut best: Option<(usize, f32)> = None;
            for (i, &(vx, vy)) in c.pts().iter().enumerate() {
                let d = ((s.kit.mx - px(vx)).powi(2) + (s.kit.my - py(vy)).powi(2)).sqrt();
                if d <= 6.0 && best.map_or(true, |b| d < b.1) {
                    best = Some((i, d));
                }
            }
            best.map(|b| b.0)
        };
        let in_box = self.hit(x, y, w, h) && !self.hit(x + w - 14.0, y + 2.0, 12.0, 12.0);
        if in_box && self.kit.press && self.kit.act == 0 {
            if let Some(i) = pick(self, &c) {
                let interior = i > 0 && i + 1 < c.n;
                if self.dbl(200 + i as i32) && interior {
                    for k in i..c.n - 1 {
                        c.p[k] = c.p[k + 1];
                    }
                    c.n -= 1;
                    self.kit.last_clk_n = 0;
                } else {
                    self.kit.act = ACT_CURVE;
                    self.cpt = i;
                }
            } else if c.n < MAX_PTS {
                let (vx, vy) = to_v(self.kit.mx, self.kit.my);
                let at = (1..c.n).find(|&k| c.p[k].0 > vx).unwrap_or(c.n - 1);
                let room = vx - c.p[at - 1].0 >= MIN_GAP && c.p[at].0 - vx >= MIN_GAP;
                if room {
                    for k in (at..c.n).rev() {
                        c.p[k + 1] = c.p[k];
                    }
                    c.p[at] = (vx, vy.clamp(-1.0, 1.0));
                    c.n += 1;
                    self.kit.act = ACT_CURVE;
                    self.cpt = at;
                    self.kit.last_clk_n = 0; // a creating click never counts towards a double click
                }
            }
            self.kit.press = false;
        }
        if self.kit.act == ACT_CURVE && self.kit.down && self.cpt < c.n {
            let (vx, vy) = to_v(self.kit.mx, self.kit.my);
            let i = self.cpt;
            if i > 0 && i + 1 < c.n {
                c.p[i].0 = vx.clamp(c.p[i - 1].0 + MIN_GAP, c.p[i + 1].0 - MIN_GAP);
            }
            c.p[i].1 = vy.clamp(-1.0, 1.0);
        }
        if c != before {
            let cell = match b {
                0 => &self.env.params.curve_lo,
                1 => &self.env.params.curve_mid,
                _ => &self.env.params.curve_hi,
            };
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
        let hot = if self.kit.act == ACT_CURVE { Some(self.cpt) } else { pick(self, &c) };
        for (i, &(vx, vy)) in c.pts().iter().enumerate() {
            let (qx, qy) = (px(vx).round(), py(vy).round());
            self.setc(hot == Some(i));
            if i == 0 || i + 1 == c.n {
                self.cv.boxr(qx - 2.0, qy - 2.0, 5.0, 5.0); // y-only end points
            } else {
                self.cv.fill(qx - 2.0, qy - 2.0, 5.0, 5.0);
            }
        }
        self.setc(false);
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, "DRAW");
        self.cv.textr(x, y + h - 11.0, w - 3.0, &format!("{}/{} PTS", c.n, MAX_PTS));
    }

    /// Sidechain control row under the spectrum: SC gain | HP | LP | SOLO (solo = hear the key as the detectors see it,
    /// i.e. after gain / HP / LP). Global: the key is conditioned once, then split into the three bands.
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

    /// the four slope steps of a crossover as one button group (12 / 24 / 48 / 96 dB per octave)
    fn slope_buttons(&mut self, n: usize, x: f32, y: f32) {
        let cur = (self.sget(n) + 0.5).floor() as i32;
        for (i, l) in ["12", "24", "48", "96"].iter().enumerate() {
            if self.btn(x + RW / 4.0 * i as f32, y, RW / 4.0, 16.0, l, (cur == i as i32) as u8) {
                self.click_set(n, i as f32);
            }
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
        let (f1, f2) = (self.env.dsp.x1 as f32, (self.env.dsp.x2 as f32).max(self.env.dsp.x1 as f32 * 1.05));
        let ranges = [(20.0, f1), (f1, f2), (f2, 20000.0)];
        for b in 0..3 {
            let tw = (VW - 96.0) / 3.0;
            let x = VX + tw * b as f32;
            let base = band_base(b);
            let on = self.sget(base + ON) > 0.5;
            self.sq_tog(base + ON, x + 7.0, y + 4.0, 8.0);
            let label = format!("{} {}-{}", BAND_NAMES[b], hz(ranges[b].0), hz(ranges[b].1));
            self.set_dk(if self.sget(8) > 0.5 { 0.5 } else { 1.0 });
            if self.btn(x, y, tw, 16.0, &label, (b == sel) as u8) {
                self.select_band(b);
            }
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
        self.shape_selector(o + SHAPE, VX, y0, sat_w, 22.0);
        self.cell1(o + DRIVE, VX, y0 + 22.0, sat_w, 22.0, "DRIVE");
        self.draw_sat_plot(VX, y0 + CELL_P, sat_w, sat_w);
        self.cell(o + BIAS, sat_x, rows[0], CELL_W, CELL_H, "BIAS");
        self.cell(o + TONE, sat_x, rows[1], CELL_W, CELL_H, "TONE");
        self.cell(o + SAT_MIX, sat_x, rows[2], CELL_W, CELL_H, "SAT MIX");
        self.cell_o(o + SAT_GAIN, sat_x, rows[3], CELL_W, CELL_H, "SAT GAIN");
        // band level after the saturator, in the gap between the saturator and compressor sections
        self.draw_vu(sat_x + CELL_W + 8.0, y0, 24.0, ph);
        // gain reduction meter
        self.draw_gr_meter(meter_x, y0, 30.0, ph);
        // compressor: 3 columns x 4 rows
        let cells = [
            [(THRESH, "THRESH"), (RATIO, "RATIO"), (ATTACK, "ATTACK"), (RELEASE, "RELEASE")],
            [(KNEE, "KNEE"), (KNEE_SHAPE, "K.SHAPE"), (GR_DIST, "GR DIST"), (SPEED, "SPEED")],
            [(DETECTOR, "PEAK>RMS"), (FF_FB, "FF>FB"), (COMP_MIX, "COMP MIX"), (COMP_GAIN, "CMP GAIN")],
        ];
        for (c, col) in cells.iter().enumerate() {
            let cx = col0 + (CELL_W + 6.0) * c as f32;
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
        self.diff_row(VX + EQ_P * 5.0, y, EQ_PW, 16.0, "IN>OUT", d);
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
                let (gx, gy) = (x + 1.0 + (t % 3) as f32 * EQ_W / 3.0, y0 + 1.0 + (t / 3) as f32 * 22.0);
                let cur = t as i32 == ty;
                if self.hit(gx, gy, EQ_W / 3.0 - 1.0, 21.0) && self.kit.press && self.kit.act == 0 {
                    self.click_set(o + 1, t as f32);
                    self.kit.press = false;
                }
                self.setc(false);
                if cur {
                    self.cv.fill(gx, gy, EQ_W / 3.0 - 1.0, 21.0);
                    self.cv.setbg();
                }
                self.eq_glyph(EQ_KINDS[b][t], gx + EQ_W / 6.0, gy + 10.5);
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
        // readouts refresh ~8x/s
        if self.kit.now - self.rd_last >= 0.12 {
            self.rd_last = self.kit.now;
            self.rd_pk = self.opk_max;
            self.rd_rms = sh.fms.load(Ordering::Relaxed);
            self.rd_diff = sh.diff_disp.load(Ordering::Relaxed);
            self.rd_eq_diff = sh.eq_diff.load(Ordering::Relaxed);
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
            self.end_touch(1);
            self.end_touch(2);
            self.end_touch(EQ_TILT);
            self.end_touch(EQ_PIVOT);
            for b in 0..3 {
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
        self.cv.text(44.0, 12.0, &title);
        self.preset_bar();
        // sidechain key: white = on and the host gave a bus, red = on but no bus
        let sc_on = self.sget(SC_ON) > 0.5;
        let sc_lit = if !sc_on { 0 } else if sh.sc_bus.load(Ordering::Relaxed) { 1 } else { 2 };
        if self.btn(418.0, 8.0, 76.0, 16.0, "SIDECHAIN", sc_lit) {
            self.click_set(SC_ON, if sc_on { 0.0 } else { 1.0 });
        }
        if !sc_on && sh.sc_solo.load(Ordering::Relaxed) {
            sh.sc_solo.store(false, Ordering::Relaxed); // solo never outlives the sidechain switch
        }
        {
            // EQ page toggle; the square in it bypasses the whole EQ module (X = bypassed, like SatComp)
            let eq = self.env.dsp.eq;
            let eq_any = eq.bands.iter().any(|b| b.on) || eq.tilt != 0.0 || eq.gain != 0.0;
            self.sq_tog(EQ_BYPASS, 502.0, 12.0, 8.0);
            if self.btn(494.0, 8.0, 46.0, 16.0, "", self.eq_page as u8) {
                self.eq_page = !self.eq_page;
            }
            self.sqind(502.0, 12.0, 8.0, eq_any, self.eq_page, eq_byp);
            // label drawn by hand so it keeps a clear gap from the square
            if self.eq_page {
                self.cv.setbg();
            } else {
                self.setc(false);
            }
            self.cv.set_font(1);
            self.cv.text(517.0, 12.0, "EQ");
            self.setc(false);
        }
        self.out_meter(RX, 8.0, RW, 16.0);

        // ---------- viewport row; right column: delta / bypass, levels, then the parameter group on the bottom edge
        let delta = sh.delta_on.load(Ordering::Relaxed);
        if self.btn(RX, VY, RW * 0.5, 16.0, "DELTA", if delta { 2 } else { 0 }) {
            sh.delta_on.store(!delta, Ordering::Relaxed);
        }
        if self.btn(RX + RW * 0.5, VY, RW * 0.5, 16.0, "BYPASS", byp as u8) {
            self.click_set(8, if byp { 0.0 } else { 1.0 });
        }
        self.set_dk(if byp || (self.eq_page && eq_byp) { 0.5 } else { 1.0 });
        if self.eq_page {
            self.draw_eq(VX, VY, VW, VH);
        } else {
            // the sidechain controls take the bottom of the viewport while the key is on
            let vh = if sc_on { VH - SC_ROW_H } else { VH };
            self.draw_spectrum(VX, VY, VW, vh);
            self.draw_crossover(VX, VY, VW, vh);
            if sc_on {
                self.set_dk(if byp { 0.5 } else { 1.0 });
                self.sc_row(VX, VY + vh);
            }
        }
        self.set_dk(1.0);
        self.cv.set_font(1);
        self.setc(false);
        let s1 = if self.rd_pk > 0.00001 { format!("{:+05.1}dB", 8.6858896 * self.rd_pk.ln()) } else { "-----dB".into() };
        self.setc(self.rd_pk > 0.9999);
        self.cv.text(RX, VY + 22.0, "PK");
        self.cv.textr(RX, VY + 22.0, RW, &s1);
        self.setc(false);
        let s1 = if self.rd_rms > 0.000000001 { format!("{:+05.1}dB", 4.3429448 * self.rd_rms.ln()) } else { "-----dB".into() };
        self.cv.text(RX, VY + 34.0, "RMS");
        self.cv.textr(RX, VY + 34.0, RW, &s1);
        // parameter group, evenly spaced and aligned to the viewport's bottom edge: crossover 1, crossover 2, oversampling
        let bottom = VY + VH;
        let os_y = bottom - 16.0;
        let x2_y = os_y - 6.0 - 32.0;
        let x1_y = x2_y - 6.0 - 32.0;
        self.cell1(1, RX, x1_y, RW, 16.0, "X-OVR1");
        self.slope_buttons(X1_SLOPE, RX, x1_y + 16.0);
        self.cell1(2, RX, x2_y, RW, 16.0, "X-OVR2");
        self.slope_buttons(X2_SLOPE, RX, x2_y + 16.0);
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
        if self.eq_page {
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
    }
}
