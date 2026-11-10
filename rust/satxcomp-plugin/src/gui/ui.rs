//! SatXComp pages: header, tab row, spectrum / crossover viewport, the three band tabs with their saturator and compressor
//! cells, and the output column. Widgets (cells, buttons, drag logic) come from `satcomp_ui::kit`.

use crate::params::{SatXCompParams, SLOTS, TABLE};
use crate::Shared;
use nih_plug::prelude::*;
use satcomp_dsp::biquad::{eq_coef, mag_db};
use satcomp_dsp::shapers::{ShaperLut, CUSTOM, SHAPER_NAMES};
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
        }
    }

    pub fn set_input(&mut self, mx: f32, my: f32, down: bool, rdown: bool, scroll: f32, shift: bool, now: f64) {
        if self.kit.set_input(mx, my, down, rdown, scroll, shift, now) {
            // the egui clock restarts when the editor window is reopened: re-base every stored timestamp
            self.rd_last = now;
            self.hl_l_t = now;
            self.hl_r_t = now;
            self.gr_hold_t = [now; 3];
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

    fn select_band(&mut self, b: usize) {
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
        for i in 0..n {
            let (ho, hd) = (hh(co[i]), hh(cd[i]));
            let (lo, hi) = (ho.min(hd), ho.max(hd));
            self.setc(false);
            if lo > 0.0 {
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
                self.kit.act = if d1 <= d2 { 101 } else { 102 };
                self.kit.press = false;
            }
        }
        if self.kit.act == 101 && self.kit.down {
            let f = x_to_freq(self.kit.mx, x + 1.0, w - 2.0);
            self.setv(1, f.min(f2 * 0.95));
        } else if self.kit.act == 102 && self.kit.down {
            let f = x_to_freq(self.kit.mx, x + 1.0, w - 2.0);
            self.setv(2, f.max(f1 * 1.05));
        }
        let active = self.kit.act;
        // dotted lines
        self.setc(false);
        for (k, px) in [px1, px2].into_iter().enumerate() {
            let lit = active == 101 + k as i32;
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
        // band markers: selected = wide box, others small; click selects
        let sel = self.sel();
        let centers = [(20.0 * f1).sqrt(), (f1 * f2).sqrt(), (f2 * 20000.0).sqrt()];
        for b in 0..3 {
            let cx = freq_to_x(centers[b], x + 1.0, w - 2.0).floor();
            let (bw, bx) = if b == sel { (26.0, cx - 13.0) } else { (14.0, cx - 7.0) };
            let by = y + 5.0;
            if self.hit(bx, by, bw, 12.0) && self.kit.press && self.kit.act == 0 {
                self.select_band(b);
                self.kit.press = false;
            }
            let on = self.sget(band_base(b) + ON) > 0.5;
            self.setc(false);
            if b == sel {
                self.cv.fill(bx, by, bw, 12.0);
                self.cv.setbg();
            } else {
                self.cv.setbg();
                self.cv.fill(bx, by, bw, 12.0);
                self.setc(false);
                self.cv.boxr(bx, by, bw, 12.0);
            }
            self.cv.set_font(1);
            self.cv.textc(bx, by + 2.0, bw, BAND_LETTERS[b]);
            // small gain-reduction readout under the marker
            let s = if self.gr_disp[b] > 0.05 { format!("-{:.1}", self.gr_disp[b]) } else { "0.0".to_string() };
            self.setc(self.gr_disp[b] > 0.05 && on);
            self.cv.textc(cx - 20.0, by + 16.0, 40.0, &s);
            self.setc(false);
        }
    }

    // ------------------------------------------------------------------ band modules
    fn shape_selector(&mut self, n: usize, x: f32, y: f32, w: f32, h: f32) {
        let aw = 16.0;
        let cy = y + h * 0.5;
        let hv = self.hit(x, y, w, h);
        let mut sel = (self.sget(n) + 0.5).floor() as i32;
        if hv && self.kit.wheel != 0 {
            let d = if self.kit.wheel > 0 { -1 } else { 1 };
            self.click_set(n, (sel + d).clamp(0, CUSTOM) as f32);
        }
        if hv && self.kit.press && self.kit.act == 0 {
            if self.kit.mx < x + aw {
                self.click_set(n, if sel <= 0 { CUSTOM as f32 } else { (sel - 1) as f32 });
            } else {
                self.click_set(n, if sel >= CUSTOM { 0.0 } else { (sel + 1) as f32 });
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

    /// transfer curve of the selected band's saturator at its current drive and bias
    fn draw_sat_plot(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let b = self.sel();
        let bp = self.env.dsp.bands[b];
        self.lut.sync(bp.shape, &bp.curve);
        let pre = (bp.drive * 0.01 * 36.0 * 0.11512925).exp();
        let bias = bp.bias * 0.01;
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        let (cx, cy) = (x + (w * 0.5).floor(), y + (h * 0.5).floor());
        self.cv.fill(x + 1.0, cy, w - 2.0, 1.0);
        self.cv.fill(cx, y + 1.0, 1.0, h - 2.0);
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, "SAT");
        self.setc(true);
        let n = (w - 3.0) as i32;
        let mut last: Option<(f32, f32)> = None;
        for i in 0..=n {
            let xin = (i as f64 / n as f64) * 2.0 - 1.0;
            let yo = self.lut.eval(xin * pre + bias).clamp(-1.0, 1.0) as f32;
            let p = (x + 1.0 + i as f32, cy - yo * (h * 0.5 - 2.0));
            if let Some(l) = last {
                self.cv.line(l.0, l.1, p.0, p.1);
            }
            last = Some(p);
        }
        self.setc(false);
    }

    fn band_row(&mut self, y: f32) {
        let sel = self.sel();
        let (f1, f2) = (self.env.dsp.x1 as f32, (self.env.dsp.x2 as f32).max(self.env.dsp.x1 as f32 * 1.05));
        let ranges = [(20.0, f1), (f1, f2), (f2, 20000.0)];
        for b in 0..3 {
            let x = 10.0 + 136.0 * b as f32;
            let base = band_base(b);
            let on = self.sget(base + ON) > 0.5;
            self.sq_tog(base + ON, x + 7.0, y + 4.0, 8.0);
            let label = format!("{} {}-{}", BAND_NAMES[b], hz(ranges[b].0), hz(ranges[b].1));
            self.set_dk(1.0);
            if self.btn(x, y, 136.0, 16.0, &label, (b == sel) as u8) {
                self.select_band(b);
            }
            self.sqind(x + 7.0, y + 4.0, 8.0, on, b == sel, false);
        }
        let base = band_base(sel);
        let (d, s) = (self.sget(base + DELTA) > 0.5, self.sget(base + SOLO) > 0.5);
        if self.btn(430.0, y, 60.0, 16.0, "DELTA", if d { 2 } else { 0 }) {
            self.click_set(base + DELTA, if d { 0.0 } else { 1.0 });
        }
        if self.btn(490.0, y, 60.0, 16.0, "SOLO", if s { 2 } else { 0 }) {
            self.click_set(base + SOLO, if s { 0.0 } else { 1.0 });
        }
    }

    fn band_cells(&mut self, y0: f32) {
        let b = self.sel();
        let o = band_base(b);
        let rows = [y0, y0 + 42.0, y0 + 84.0, y0 + 126.0];
        // saturator
        self.shape_selector(o + SHAPE, 10.0, y0, 120.0, 16.0);
        self.cell(o + DRIVE, 10.0, y0 + 20.0, 120.0, 38.0, "DRIVE");
        self.draw_sat_plot(10.0, y0 + 62.0, 120.0, 102.0);
        self.cell(o + BIAS, 136.0, rows[0], 60.0, 38.0, "BIAS");
        self.cell(o + TONE, 136.0, rows[1], 60.0, 38.0, "TONE");
        self.cell(o + SAT_MIX, 136.0, rows[2], 60.0, 38.0, "SAT MIX");
        self.cell_o(o + SAT_GAIN, 136.0, rows[3], 60.0, 38.0, "SAT GAIN");
        // gain reduction meter
        self.draw_gr_meter(200.0, y0, 26.0, 164.0);
        // compressor: 3 columns x 4 rows
        let cols = [232.0, 296.0, 360.0];
        let cells = [
            [(THRESH, "THRESH"), (RATIO, "RATIO"), (ATTACK, "ATTACK"), (RELEASE, "RELEASE")],
            [(KNEE, "KNEE"), (KNEE_SHAPE, "K.SHAPE"), (GR_DIST, "GR DIST"), (SPEED, "SPEED")],
            [(DETECTOR, "PEAK>RMS"), (FF_FB, "FF>FB"), (COMP_MIX, "COMP MIX"), (COMP_GAIN, "CMP GAIN")],
        ];
        for (c, col) in cells.iter().enumerate() {
            for (r, (off, lbl)) in col.iter().enumerate() {
                if *off == COMP_GAIN {
                    self.cell_o(o + off, cols[c], rows[r], 60.0, 38.0, lbl);
                } else {
                    self.cell(o + off, cols[c], rows[r], 60.0, 38.0, lbl);
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
        self.cv.text(430.0, y0 + 2.0, lbl);
        self.cv.textr(430.0, y0 + 2.0, 120.0, &val);
        let blink = ((self.kit.now * 4.0).floor() as i64) & 1 == 1;
        let lit = if gmatch { if meas && blink { 1 } else { 2 } } else { 0 };
        if self.btn(430.0, y0 + 16.0, 120.0, 16.0, "GAIN MATCH", lit) {
            if gmatch {
                self.click_set(7, 0.0);
            } else {
                self.click_set(7, 1.0);
                sh.match_req.store(true, Ordering::Relaxed);
            }
        }
        self.diff_box(430.0, y0 + 42.0, 120.0, 38.0);
        self.cell(5, 430.0, y0 + 84.0, 120.0, 38.0, "MIX");
        self.cell_o(6, 430.0, y0 + 126.0, 120.0, 38.0, "OUT GAIN");
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
                if self.sget(eq_base(b)) < 0.5 {
                    self.click_set(eq_base(b), 1.0);
                }
                self.kit.act = 103 + b as i32;
                self.kit.press = false;
            }
        }
        if self.kit.act >= 103 && self.kit.act < 108 && self.kit.down {
            let b = (self.kit.act - 103) as usize;
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
    }

    /// the five EQ band toggles: type name while on, "EQ n" while off
    fn eq_row(&mut self, y: f32) {
        for b in 0..5 {
            let x = 10.0 + 82.0 * b as f32;
            let on = self.sget(eq_base(b)) > 0.5;
            let ty = (self.sget(eq_base(b) + 1) + 0.5).floor() as i32;
            let label = if on { KIND_NAMES[eq_kind(b, ty) as usize].to_string() } else { format!("EQ {}", b + 1) };
            if self.btn(x, y, 78.0, 16.0, &label, on as u8) {
                self.eq_sel = b;
                self.click_set(eq_base(b), if on { 0.0 } else { 1.0 });
            }
        }
    }

    fn eq_cells(&mut self, y0: f32) {
        for b in 0..5 {
            let x = 10.0 + 66.0 * b as f32;
            let o = eq_base(b);
            let on = self.sget(o) > 0.5;
            self.set_dk(if on { 1.0 } else { 0.5 });
            // type glyph grid (2 x 3)
            let ty = (self.sget(o + 1) + 0.5).floor() as i32;
            self.setc(false);
            self.cv.boxr(x, y0, 62.0, 38.0);
            for t in 0..6usize {
                let (gx, gy) = (x + 1.0 + (t % 3) as f32 * 20.0, y0 + 1.0 + (t / 3) as f32 * 18.5);
                let cur = t as i32 == ty;
                if self.hit(gx, gy, 20.0, 18.5) && self.kit.press && self.kit.act == 0 {
                    self.click_set(o + 1, t as f32);
                    self.kit.press = false;
                }
                self.setc(false);
                if cur {
                    self.cv.fill(gx, gy, 20.0, 18.5);
                    self.cv.setbg();
                }
                self.eq_glyph(EQ_KINDS[b][t], gx + 10.0, gy + 9.5);
                self.setc(false);
            }
            self.cell(o + 3, x, y0 + 42.0, 62.0, 38.0, "Q");
            self.cell(o + 2, x, y0 + 84.0, 62.0, 38.0, "FREQ");
            if matches!(eq_kind(b, ty), 2 | 3 | 4) {
                self.cell(o + 4, x, y0 + 126.0, 62.0, 38.0, "GAIN");
            } else {
                self.setc(false);
                self.cv.boxr(x, y0 + 126.0, 62.0, 38.0);
                self.cv.set_font(1);
                self.cv.text(x + 3.0, y0 + 129.0, "GAIN");
                self.cv.set_font(2);
                self.cv.textc(x, y0 + 142.0, 62.0, "---");
            }
        }
        self.set_dk(1.0);
        let x = 10.0 + 66.0 * 5.0;
        self.cell(EQ_PIVOT, x, y0, 70.0, 38.0, "PIVOT");
        self.cell(EQ_TILT, x, y0 + 42.0, 70.0, 38.0, "TILT");
        self.cell(EQ_MIX, x, y0 + 84.0, 70.0, 38.0, "EQ MIX");
        self.cell_o(EQ_GAIN, x, y0 + 126.0, 70.0, 38.0, "EQ GAIN");
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
        }
        self.env.dsp = self.env.params.to_dsp();
        self.update_gr();

        // drag update (single active widget): 1..99 = slot (vertical drag), >= 100 = viewport drags
        if self.kit.act != 0 && self.kit.act < 100 {
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
        if self.kit.act >= 100 && !self.kit.down {
            self.end_touch(1);
            self.end_touch(2);
            for b in 0..5 {
                for k in 2..5 {
                    self.end_touch(eq_base(b) + k);
                }
            }
            self.kit.act = 0;
        }
        self.kit.last_my = self.kit.my;

        let byp = self.sget(8) > 0.5;

        // ---------- header
        self.set_dk(1.0);
        self.logo(13.0, 5.0);
        self.setc(false);
        self.cv.set_font(2);
        let title = format!("SATXCOMP {}", version_short());
        self.cv.text(44.0, 12.0, &title);
        self.out_meter(430.0, 8.0, 120.0, 16.0);

        // ---------- tab row
        self.set_dk(if byp { 0.5 } else { 1.0 });
        let os_i = (self.sget(3) + 0.5).floor() as i32;
        for i in 0..3 {
            let l = ["x1", "x2", "x4"][i as usize];
            if self.btn(10.0 + i as f32 * 40.0, 34.0, 40.0, 16.0, l, (os_i == i) as u8) {
                self.click_set(3, i as f32);
            }
        }
        self.set_dk(1.0);
        {
            // EQ page toggle; the square in it switches the whole EQ module on / off
            let eq_on = self.sget(EQ_BYPASS) < 0.5;
            self.sq_tog(EQ_BYPASS, 150.0, 38.0, 8.0);
            if self.btn(140.0, 34.0, 70.0, 16.0, "EQ", self.eq_page as u8) {
                self.eq_page = !self.eq_page;
            }
            self.sqind(150.0, 38.0, 8.0, eq_on, self.eq_page, false);
        }
        let delta = sh.delta_on.load(Ordering::Relaxed);
        if self.btn(430.0, 34.0, 60.0, 16.0, "DELTA", if delta { 2 } else { 0 }) {
            sh.delta_on.store(!delta, Ordering::Relaxed);
        }
        if self.btn(490.0, 34.0, 60.0, 16.0, "BYPASS", byp as u8) {
            self.click_set(8, if byp { 0.0 } else { 1.0 });
        }

        // ---------- viewport row
        self.set_dk(if byp { 0.5 } else { 1.0 });
        if self.eq_page {
            self.draw_eq(10.0, 56.0, 408.0, 128.0);
        } else {
            self.draw_spectrum(10.0, 56.0, 408.0, 128.0);
            self.draw_crossover(10.0, 56.0, 408.0, 128.0);
        }
        self.set_dk(1.0);
        // right column: levels, crossover sliders, lookahead
        self.cv.set_font(1);
        self.setc(false);
        let s1 = if self.rd_pk > 0.00001 { format!("{:+05.1}dB", 8.6858896 * self.rd_pk.ln()) } else { "-----dB".into() };
        self.setc(self.rd_pk > 0.9999);
        self.cv.text(430.0, 58.0, "PK");
        self.cv.textr(430.0, 58.0, 120.0, &s1);
        self.setc(false);
        let s1 = if self.rd_rms > 0.000000001 { format!("{:+05.1}dB", 4.3429448 * self.rd_rms.ln()) } else { "-----dB".into() };
        self.cv.text(430.0, 70.0, "RMS");
        self.cv.textr(430.0, 70.0, 120.0, &s1);
        self.cell2(1, 2, 430.0, 84.0, 120.0, 38.0, "X-OVER 1", "X-OVER 2");
        self.cell(4, 430.0, 126.0, 120.0, 38.0, "L.AHEAD");

        // ---------- band row, band cells, output column
        if self.eq_page {
            self.eq_row(190.0);
            self.set_dk(if byp || self.sget(EQ_BYPASS) > 0.5 { 0.5 } else { 1.0 });
            self.eq_cells(212.0);
        } else {
            self.band_row(190.0);
            self.set_dk(if byp { 0.5 } else { 1.0 });
            self.band_cells(212.0);
        }
        self.set_dk(1.0);
        self.out_column(212.0);
    }
}
