//! Port of the JSFX `@gfx` section: widget kit (cell, btn, selector, ...), the sat / comp / EQ viewports,
//! meters and the main layout. Immediate mode with the same `act` / `press` drag logic as the JSFX.

use super::canvas::{Canvas, PALETTES};
use crate::params::{SatCompParams, TABLE};
use crate::presets::{self, Preset};
use crate::Shared;
use nih_plug::prelude::*;
use satcomp_dsp::biquad::{eq_coef, mag_db};
use satcomp_dsp::eq::{decode_band, decode_tilt, kind_of, ui_idx};
use satcomp_dsp::shapers::{Curve, ShaperLut, CUSTOM, MAX_PTS, MIN_GAP, SHAPER_NAMES};
use satcomp_dsp::GainComputer;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, Default)]
pub struct Meta {
    pub min: f32,
    pub max: f32,
    pub def: f32,
    /// 0 linear, 1 log, 2 sqrt
    pub mode: u8,
    /// bit 0: snap to min near the start of travel, bit 1: snap to max near the end
    pub snap: u8,
    pub step: f32,
}

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

const MENU_ROWS: usize = 18;
const MENU_ROW_H: f32 = 14.0;

/// Window scale steps (percent of the 560x336 base size).
const SIZES: [f32; 6] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5];

struct Menu {
    kind: MenuKind,
    x: f32,
    y: f32,
    items: Vec<MItem>,
    checked: i32,
    scroll: usize,
}

pub struct State {
    meta: [Meta; 63],
    touched: [bool; 63],
    pub pal: usize,
    // interaction
    act: i32,
    act_t: f32,
    last_my: f32,
    last_clk_n: i32,
    last_clk_t: f64,
    vt1: usize,
    vt2: usize,
    gdx: f32,
    gdy: f32,
    tacc: f32,
    tlast: f32,
    last_down: bool,
    last_rdown: bool,
    wacc: f32,
    menu: Option<Menu>,
    // per-frame input (logical units)
    mx: f32,
    my: f32,
    down: bool,
    press: bool,
    rpress: bool,
    wheel: i32,
    shift: bool,
    now: f64,
    // meters
    gr_disp: f32,
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
    // readouts (refresh ~8x/s)
    rd_last: f64,
    rd_tick: bool,
    rd_gr: f32,
    rd_pk: f32,
    rd_rms: f32,
    rd_diff: f32,
    rd_bg: f32,
    rd_lk: f32,
    rd_mt: f32,
    rd_sat: f32,
    rd_ghr: f32,
    rd_lvl: f32,
    // EQ page
    eqv: usize,
    eq_page: bool,
    ex0: f32,
    epw: f32,
    ey0: f32,
    eph: f32,
    enx: [f32; 3],
    eny: [f32; 3],
    ekd: [u8; 3],
    dsc: [[f64; 5]; 5],
    dsa: [bool; 5],
    et_t: f32,
    et_p: f32,
    spk: Vec<f32>,
    spec_t: f64,
    fwin: Vec<f32>,
    // sat plot
    lut: ShaperLut,
    /// custom-curve draw mode (pencil) and the point being dragged
    curve_edit: bool,
    /// shape was observed as Custom since the draw view opened (host writes land a frame or more later)
    edit_seen: bool,
    cpt: usize,
    dk: f32,
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
}

impl State {
    /// Per-frame input snapshot (positions in logical canvas units). Computes press edges like the JSFX.
    pub fn set_input(&mut self, mx: f32, my: f32, down: bool, rdown: bool, scroll: f32, shift: bool, now: f64) {
        self.mx = mx;
        self.my = my;
        self.down = down;
        self.press = down && !self.last_down;
        self.last_down = down;
        self.rpress = rdown && !self.last_rdown;
        self.last_rdown = rdown;
        self.shift = shift;
        self.now = now;
        // notch the scroll: a mouse wheel reports big steps, a trackpad many small ones
        self.wacc += scroll;
        self.wheel = 0;
        if self.wacc.abs() >= 10.0 {
            self.wheel = if self.wacc > 0.0 { 1 } else { -1 };
            self.wacc = 0.0;
        }
    }

    pub fn new(pal: usize) -> Self {
        let mut meta = [Meta::default(); 63];
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
        sp(9, 1, 2, 0.1);
        sp(10, 1, 0, 0.01);
        sp(11, 1, 2, 1.0);
        sp(12, 0, 0, 1.0);
        sp(13, 0, 0, 0.1);
        sp(14, 0, 0, 1.0);
        sp(15, 0, 0, 1.0);
        sp(16, 2, 1, 1.0);
        sp(18, 0, 0, 1.0);
        sp(19, 0, 0, 1.0);
        sp(20, 0, 0, 0.1);
        sp(21, 0, 0, 1.0);
        sp(22, 0, 0, 0.1);
        sp(25, 0, 0, 1.0);
        sp(27, 0, 0, 0.1);
        sp(59, 0, 0, 0.1);
        for st in 0..2 {
            let base = 29 + st * 15;
            for b in 0..3 {
                let n = base + b * 4;
                sp(n, 0, 0, 1.0);
                sp(n + 1, 1, 0, 1.0);
                sp(n + 2, 0, 0, 0.1);
                sp(n + 3, 1, 0, 0.01);
            }
            sp(base + 12, 0, 0, 0.1);
            sp(base + 13, 1, 0, 1.0);
            sp(base + 14, 0, 0, 0.1);
        }
        let fwin = (0..4096).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / 4096.0).cos()).collect();
        Self {
            meta,
            touched: [false; 63],
            pal,
            act: 0,
            act_t: 0.0,
            last_my: 0.0,
            last_clk_n: 0,
            last_clk_t: 0.0,
            vt1: 0,
            vt2: 0,
            gdx: 0.0,
            gdy: 0.0,
            tacc: 0.0,
            tlast: 0.0,
            last_down: false,
            last_rdown: false,
            wacc: 0.0,
            menu: None,
            mx: -1000.0,
            my: -1000.0,
            down: false,
            press: false,
            rpress: false,
            wheel: 0,
            shift: false,
            now: 0.0,
            gr_disp: 0.0,
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
            rd_last: 0.0,
            rd_tick: false,
            rd_gr: 0.0,
            rd_pk: 0.0,
            rd_rms: 0.0,
            rd_diff: 0.0,
            rd_bg: 1.0,
            rd_lk: 0.0,
            rd_mt: 0.0,
            rd_sat: 0.0,
            rd_ghr: 0.0,
            rd_lvl: -100.0,
            eqv: 0,
            eq_page: false,
            ex0: 0.0,
            epw: 0.0,
            ey0: 0.0,
            eph: 0.0,
            enx: [0.0; 3],
            eny: [0.0; 3],
            ekd: [4; 3],
            dsc: [[0.0; 5]; 5],
            dsa: [false; 5],
            et_t: 0.0,
            et_p: 600.0,
            spk: vec![-140.0; 2049],
            spec_t: 0.0,
            fwin,
            lut: ShaperLut::new(0),
            curve_edit: false,
            edit_seen: false,
            cpt: 0,
            dk: 1.0,
            resizing: false,
            resize_off: (0.0, 0.0),
            ptr_pt: (0.0, 0.0),
            win_pt: (560.0, 336.0),
            factory: presets::factory(),
            user: presets::scan_user(),
            pending: 0,
        }
    }
}

pub struct Env<'a> {
    pub params: &'a SatCompParams,
    pub setter: &'a ParamSetter<'a>,
    pub shared: &'a Shared,
    pub dsp: satcomp_dsp::Params,
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

fn blink(t: f64) -> bool {
    ((t * 4.0).floor() as i64) & 1 == 1
}

impl<'a> Gui<'a> {
    // ------------------------------------------------------------------ params (slider-number addressing)
    fn sget(&self, n: usize) -> f32 {
        match self.env.params.ptr(n) {
            Some(p) => {
                let m = &self.meta[n];
                m.min + (m.max - m.min) * unsafe { p.unmodulated_normalized_value() }
            }
            None => 0.0,
        }
    }

    fn sset(&mut self, n: usize, v: f32) {
        if let Some(p) = self.env.params.ptr(n) {
            let m = self.meta[n];
            let norm = ((v - m.min) / (m.max - m.min)).clamp(0.0, 1.0);
            unsafe {
                if !self.touched[n] {
                    self.env.setter.raw_context.raw_begin_set_parameter(p);
                    self.touched[n] = true;
                }
                self.env.setter.raw_context.raw_set_parameter_normalized(p, norm);
            }
        }
    }

    fn end_touch(&mut self, n: usize) {
        if n < 63 && self.touched[n] {
            if let Some(p) = self.env.params.ptr(n) {
                unsafe { self.env.setter.raw_context.raw_end_set_parameter(p) };
            }
            self.touched[n] = false;
        }
    }

    /// set + release in one go (a click, not a drag)
    fn click_set(&mut self, n: usize, v: f32) {
        self.sset(n, v);
        self.end_touch(n);
    }

    fn setv(&mut self, n: usize, mut v: f32) {
        let m = self.meta[n];
        if m.step > 0.0 {
            v = (v / m.step + 0.5).floor() * m.step;
        }
        v = v.clamp(m.min, m.max);
        if v != self.sget(n) {
            self.sset(n, v);
        }
    }

    fn n2t(&self, n: usize, v: f32) -> f32 {
        let m = self.meta[n];
        let t = match m.mode {
            1 => (v.max(m.min) / m.min).ln() / (m.max / m.min).ln(),
            2 => ((v - m.min).max(0.0) / (m.max - m.min)).sqrt(),
            _ => (v - m.min) / (m.max - m.min),
        };
        t.clamp(0.0, 1.0)
    }

    fn t2n(&self, n: usize, mut t: f32) -> f32 {
        let m = self.meta[n];
        if m.snap & 1 != 0 && t < 0.03 {
            t = 0.0;
        }
        if m.snap & 2 != 0 && t > 0.97 {
            t = 1.0;
        }
        let mut v = match m.mode {
            1 => m.min * (t * (m.max / m.min).ln()).exp(),
            2 => m.min + (m.max - m.min) * t * t,
            _ => m.min + (m.max - m.min) * t,
        };
        if m.step > 0.0 {
            v = (v / m.step + 0.5).floor() * m.step;
        }
        v.clamp(m.min, m.max)
    }

    fn commit(&mut self, n: usize, t: f32) {
        let v = self.t2n(n, t);
        if v != self.sget(n) {
            self.sset(n, v);
        }
    }

    fn hit(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        self.mx >= x && self.mx < x + w && self.my >= y && self.my < y + h
    }

    fn dbl(&mut self, id: i32) -> bool {
        let r = self.last_clk_n == id && self.now - self.last_clk_t < 0.35;
        self.last_clk_n = if r { 0 } else { id };
        self.last_clk_t = self.now;
        r
    }

    fn begin_drag(&mut self, n: usize) {
        if self.last_clk_n == n as i32 && self.now - self.last_clk_t < 0.35 {
            let d = self.meta[n].def;
            self.click_set(n, d);
            self.act = 0;
            self.last_clk_n = 0;
        } else {
            self.act = n as i32;
            self.act_t = self.n2t(n, self.sget(n));
            self.last_my = self.my;
            self.last_clk_n = n as i32;
            self.last_clk_t = self.now;
        }
    }

    fn wheel_adj(&mut self, n: usize) {
        let t = self.n2t(n, self.sget(n)) + if self.wheel > 0 { 1.0 } else { -1.0 } * if self.shift { 0.005 } else { 0.03 };
        self.commit(n, t.clamp(0.0, 1.0));
        self.end_touch(n);
    }

    fn setc(&mut self, red: bool) {
        self.cv.dk = self.dk;
        self.cv.setc(red);
    }

    fn set_dk(&mut self, d: f32) {
        self.dk = d;
        self.cv.dk = d;
    }

    fn refresh_dsp(&mut self) {
        self.env.dsp = self.env.params.to_dsp();
    }

    // ------------------------------------------------------------------ formatting
    fn fmt(&self, n: usize, v: f32) -> String {
        if (29..=58).contains(&n) {
            let o = (n - 29) % 15;
            return if o < 12 {
                match o % 4 {
                    1 => {
                        if v >= 10000.0 {
                            format!("{:.0}kHz", v * 0.001)
                        } else if v >= 1000.0 {
                            format!("{:.1}kHz", v * 0.001)
                        } else {
                            format!("{:.0}Hz", v)
                        }
                    }
                    2 => {
                        if v.abs() < 0.05 {
                            "0.0dB".into()
                        } else {
                            format!("{:+.1}dB", v)
                        }
                    }
                    _ => format!("{:.2}", v),
                }
            } else if o == 13 {
                if v >= 1000.0 {
                    format!("{:.1}kHz", v * 0.001)
                } else {
                    format!("{:.0}Hz", v)
                }
            } else if v.abs() < 0.05 {
                "0.0dB".into()
            } else {
                format!("{:+.1}dB", v)
            };
        }
        match n {
            59 => {
                if v < 0.05 {
                    "OFF".into()
                } else {
                    format!("{:.1}ms", v)
                }
            }
            2 | 12 | 14 | 15 | 17 | 18 | 19 | 21 | 25 => format!("{:.0}%", v),
            3 => format!("{:+.0}%", v),
            4 | 16 => {
                if v <= if n == 4 { 20.5 } else { 15.0 } {
                    "OFF".into()
                } else {
                    format!("{:.0}Hz", v)
                }
            }
            5 | 6 => {
                if v >= 19900.0 {
                    "OFF".into()
                } else {
                    format!("{:.1}k", v * 0.001)
                }
            }
            8 => format!("{:.1}dB", v),
            9 => {
                if v >= 19.9 {
                    "INF".into()
                } else {
                    format!("{:.1}:1", v)
                }
            }
            10 => {
                if v < 1.0 {
                    format!("{:.2}ms", v)
                } else {
                    format!("{:.1}ms", v)
                }
            }
            11 => {
                if v >= 1990.0 {
                    "AUTO".into()
                } else if v >= 1000.0 {
                    format!("{:.2}s", v * 0.001)
                } else {
                    format!("{:.0}ms", v)
                }
            }
            13 | 20 => format!("{:.1}dB", v),
            22 | 27 => format!("{:+.1}dB", v),
            _ => format!("{:.1}", v),
        }
    }

    // ------------------------------------------------------------------ widgets
    fn cell(&mut self, n: usize, x: f32, y: f32, w: f32, h: f32, lbl: &str) {
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
        self.setc(isact);
        self.cv.boxr(x, y, w, h);
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, lbl);
        self.cv.set_font(2);
        let s = self.fmt(n, v);
        self.cv.textc(x, y + (h * 0.4 + 1.0).floor(), w, &s);
        self.cv.fill(x + 2.0, y + h - 4.0, ((t * (w - 4.0)).floor()).max(1.0), 2.0);
    }

    /// inverted cell (lit block) for the gain-stage cells
    fn cell_o(&mut self, n: usize, x: f32, y: f32, w: f32, h: f32, lbl: &str) {
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
        self.setc(isact);
        self.cv.fill(x, y, w, h);
        self.cv.setbg();
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 3.0, lbl);
        self.cv.set_font(2);
        let s = self.fmt(n, v);
        self.cv.textc(x, y + (h * 0.4 + 1.0).floor(), w, &s);
        self.cv.fill(x + 2.0, y + h - 4.0, ((t * (w - 4.0)).floor()).max(1.0), 2.0);
        self.setc(false);
    }

    /// two stacked params in one cell (label left, value right)
    fn cell2(&mut self, na: usize, nb: usize, x: f32, y: f32, w: f32, h: f32, la: &str, lb: &str) {
        let hh = (h * 0.5).floor();
        let ha = self.hit(x, y, w, hh);
        let hb = self.hit(x, y + hh, w, h - hh);
        if ha && self.press && self.act == 0 {
            self.begin_drag(na);
        }
        if hb && self.press && self.act == 0 {
            self.begin_drag(nb);
        }
        if ha && self.wheel != 0 {
            self.wheel_adj(na);
        }
        if hb && self.wheel != 0 {
            self.wheel_adj(nb);
        }
        self.setc(self.act == na as i32 || self.act == nb as i32);
        self.cv.boxr(x, y, w, h);
        self.setc(false);
        self.cv.fill(x, y + hh, w, 1.0);
        self.cv.set_font(1);
        self.setc(self.act == na as i32);
        let s = self.fmt(na, self.sget(na));
        self.cv.text(x + 3.0, y + 3.0, la);
        self.cv.textr(x, y + 3.0, w - 3.0, &s);
        self.cv.fill(x + 2.0, y + hh - 3.0, ((self.n2t(na, self.sget(na)) * (w - 4.0)).floor()).max(1.0), 1.0);
        self.setc(self.act == nb as i32);
        let s = self.fmt(nb, self.sget(nb));
        self.cv.text(x + 3.0, y + hh + 3.0, lb);
        self.cv.textr(x, y + hh + 3.0, w - 3.0, &s);
        self.cv.fill(x + 2.0, y + h - 4.0, ((self.n2t(nb, self.sget(nb)) * (w - 4.0)).floor()).max(1.0), 2.0);
        self.setc(false);
    }

    /// lit: 0 off, 1 white, 2 red
    fn btn(&mut self, x: f32, y: f32, w: f32, h: f32, s: &str, lit: u8) -> bool {
        let hv = self.hit(x, y, w, h);
        let clicked = hv && self.press && self.act == 0;
        if lit > 0 {
            self.setc(lit > 1);
            self.cv.fill(x, y, w, h);
            self.cv.setbg();
        } else {
            self.setc(false);
            self.cv.boxr(x, y, w, h);
        }
        self.cv.set_font(1);
        self.cv.textc(x, y + ((h - 8.0) * 0.5).floor(), w, s);
        self.setc(false);
        clicked
    }

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

    /// filter glyphs traced from svg/*.svg. kind: 0 HPF, 1 LPF, 2 low shelf, 3 high shelf, 4 bell
    fn glyph(&mut self, kind: u8, cx: f32, cy: f32) {
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
            _ => {
                seg(0.0, 10.45, 5.69, 10.45);
                seg(5.69, 10.45, 9.27, 2.45);
                seg(9.27, 2.45, 12.84, 10.45);
                seg(12.84, 10.45, 18.0, 10.45);
            }
        }
    }

    /// header logo: left click = next colour scheme, right click = scheme dropdown
    fn logo(&mut self, x: f32, y: f32) {
        let hv = self.hit(x, y, 21.0, 21.0);
        if hv && self.press && self.act == 0 {
            self.set_pal((self.pal + 1) % 10);
            self.press = false;
        }
        if hv && self.rpress {
            let items = PALETTES.iter().map(|p| p.0.to_string()).collect();
            self.open_menu(MenuKind::Palette, x, y + 22.0, items, self.pal);
        }
        self.setc(false);
        self.cv.boxr(x + 6.0, y + 6.0, 9.0, 9.0);
        let l = |c: &Canvas, a: f32, b: f32, cc: f32, d: f32| c.line(x + a, y + b, x + cc, y + d);
        l(&self.cv, 12.5, 0.5, 20.5, 0.5);
        l(&self.cv, 20.5, 0.5, 20.5, 7.5);
        l(&self.cv, 8.5, 20.5, 0.5, 20.5);
        l(&self.cv, 0.5, 20.5, 0.5, 13.5);
        l(&self.cv, 6.5, 14.5, 0.5, 20.5);
        l(&self.cv, 14.5, 6.5, 20.5, 0.5);
    }

    fn set_pal(&mut self, p: usize) {
        self.pal = p.min(9);
        self.env.params.palette.store(self.pal as i32, Ordering::Relaxed);
        let a = PALETTES[self.pal].1;
        self.cv.bk = [a[0], a[1], a[2]];
        self.cv.fg = [a[3], a[4], a[5]];
        self.cv.ac = [a[6], a[7], a[8]];
    }

    /// module-active square: filled = active, outline = untouched; byp = outlined with an X
    fn sqind(&mut self, x: f32, y: f32, s: f32, on: bool, lit: bool, byp: bool) {
        if lit {
            self.cv.setbg();
        } else {
            self.setc(false);
        }
        if byp {
            self.cv.boxr(x, y, s, s);
            self.cv.line(x, y, x + s - 1.0, y + s - 1.0);
            self.cv.line(x + s - 1.0, y, x, y + s - 1.0);
        } else if on {
            self.cv.fill(x, y, s, s);
        } else {
            self.cv.boxr(x, y, s, s);
        }
        self.setc(false);
    }

    fn sq_tog(&mut self, n: usize, x: f32, y: f32, s: f32) {
        if self.hit(x - 3.0, y - 3.0, s + 6.0, s + 6.0) && self.press && self.act == 0 {
            let v = if self.sget(n) > 0.5 { 0.0 } else { 1.0 };
            self.click_set(n, v);
            self.press = false;
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

    // ------------------------------------------------------------------ meters
    fn draw_meter(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let g = self.env.shared.gr_pk.swap(0.0, Ordering::Relaxed);
        self.gr_disp = g.max(self.gr_disp * 0.9);
        if g >= self.gr_hold {
            self.gr_hold = g;
            self.gr_hold_t = self.now;
        } else if self.now - self.gr_hold_t > 1.0 {
            self.gr_hold = (self.gr_hold - 0.4).max(0.0);
        }
        if self.hit(x, y, w, h) && self.press && self.act == 0 {
            self.gr_ri = if self.gr_ri >= 3 { 0 } else { self.gr_ri + 1 };
            self.press = false;
        }
        let rng = |i: usize| [3.0f32, 6.0, 12.0, 18.0][i.min(3)];
        while self.gr_ri < 3 && self.gr_disp > rng(self.gr_ri) {
            self.gr_ri += 1;
        }
        let rg = rng(self.gr_ri);
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        self.setc(true);
        self.cv.fill(x + 1.0, y + 1.0, w - 2.0, ((h - 2.0) * (self.gr_disp / rg).min(1.0)).floor());
        self.setc(false);
        let hp = y + 1.0 + ((h - 3.0) * (self.gr_hold / rg).min(1.0)).floor();
        self.cv.fill(x + 1.0, hp, w - 2.0, 1.0);
        self.cv.set_font(1);
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
        if self.hit(x, y, w, h) && self.press && self.act == 0 {
            self.opk_max = 0.0;
            self.clip_lat = false;
            self.hl_l = 0.0;
            self.hl_r = 0.0;
            self.press = false;
        }
        self.ovl = pa.max(self.ovl * 0.85);
        self.ovr = pb.max(self.ovr * 0.85);
        if pa >= self.hl_l {
            self.hl_l = pa;
            self.hl_l_t = self.now;
        } else if self.now - self.hl_l_t > 1.0 {
            self.hl_l *= 0.9;
        }
        if pb >= self.hl_r {
            self.hl_r = pb;
            self.hl_r_t = self.now;
        } else if self.now - self.hl_r_t > 1.0 {
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

    // ------------------------------------------------------------------ sat plot
    fn draw_plot(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let sh = self.env.shared;
        let dsp = self.env.dsp;
        let sat_on = dsp.drive > 0.0 || dsp.bias != 0.0;
        let pre = (dsp.drive * 0.01 * 36.0 * 0.11512925).exp() as f32;
        let bias = (dsp.bias * 0.01) as f32;
        self.lut.sync(dsp.shape, &dsp.curve);
        // pencil (top right): toggles the custom-curve draw view; entering selects the Custom shaper so edits are audible
        let pen = self.hit(x + w - 14.0, y + 2.0, 12.0, 12.0);
        if pen && self.press && self.act == 0 {
            self.curve_edit = !self.curve_edit;
            self.edit_seen = false;
            if self.curve_edit {
                self.click_set(1, CUSTOM as f32);
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
                sh.sat_req.store(true, Ordering::Relaxed);
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
        self.pencil_glyph(x + w - 14.0, y + 2.0, self.curve_edit, pen);
        if self.curve_edit {
            self.draw_curve_edit(x, y, w, h);
            return;
        }
        let pk = sh.sat_vz.load(Ordering::Relaxed).min(1.0);
        let n = 64;
        let (mut lx, mut ly) = (0.0, 0.0);
        for i in 0..=n {
            let vx = -1.0 + 2.0 * i as f32 / n as f32;
            let mut vy = if sat_on { self.lut.eval((vx * pre + bias) as f64) as f32 } else { vx };
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
            let vy = (self.lut.eval((pk * pre + bias) as f64) as f32).clamp(-1.0, 1.0);
            self.cv.fill(x + (pk + 1.0) * 0.5 * (w - 1.0) - 1.0, y + (1.0 - (vy + 1.0) * 0.5) * (h - 1.0) - 1.0, 3.0, 3.0);
            let vy = (self.lut.eval((-pk * pre + bias) as f64) as f32).clamp(-1.0, 1.0);
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
        let s = if sh.sat_meas.load(Ordering::Relaxed) {
            self.setc(blink(self.now));
            format!("MEAS {:.1}s", sh.sat_mt.load(Ordering::Relaxed))
        } else {
            format!("SAT {:+.1}dB", self.rd_sat)
        };
        self.cv.textr(x, y + h - 14.0, w - 3.0, &s);
        self.setc(false);
    }

    /// 12x12 pencil icon; lit while the draw view is open, bright on hover.
    fn pencil_glyph(&mut self, x: f32, y: f32, on: bool, hover: bool) {
        self.set_dk(if on || hover { 1.0 } else { 0.6 });
        self.setc(on);
        let l = |s: &Self, a: f32, b: f32, c: f32, d: f32| s.cv.line(x + a, y + b, x + c, y + d);
        l(self, 8.0, 1.0, 10.0, 3.0); // eraser cap
        l(self, 8.0, 1.0, 1.0, 8.0); // body
        l(self, 10.0, 3.0, 3.0, 10.0);
        l(self, 1.0, 8.0, 0.0, 11.0); // tip
        l(self, 3.0, 10.0, 0.0, 11.0);
        self.set_dk(1.0);
        self.setc(false);
    }

    /// Draw view of the custom curve (unity input, independent of drive/bias).
    /// Left click empty space = add a point (and drag it), drag = move, double click = remove.
    /// The two end points are fixed in x and cannot be removed.
    fn draw_curve_edit(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let mut c = self.env.params.curve_snapshot();
        let before = c;
        // domain [-1,1]^2 is inset so end points and the pencil never collide (symmetric margins keep the centre cross true)
        let (ex, ey, ew, eh) = (x + 9.0, y + 14.0, w - 18.0, h - 28.0);
        let (px, py) = (|vx: f32| ex + (vx + 1.0) * 0.5 * (ew - 1.0), |vy: f32| ey + (1.0 - (vy + 1.0) * 0.5) * (eh - 1.0));
        let to_v = |mx: f32, my: f32| (((mx - ex) / (ew - 1.0)) * 2.0 - 1.0, 1.0 - ((my - ey) / (eh - 1.0)) * 2.0);
        let pick = |s: &Self, c: &Curve| -> Option<usize> {
            let mut best: Option<(usize, f32)> = None;
            for (i, &(vx, vy)) in c.pts().iter().enumerate() {
                let d = ((s.mx - px(vx)).powi(2) + (s.my - py(vy)).powi(2)).sqrt();
                if d <= 6.0 && best.map_or(true, |b| d < b.1) {
                    best = Some((i, d));
                }
            }
            best.map(|b| b.0)
        };
        let in_box = self.hit(x, y, w, h) && !self.hit(x + w - 14.0, y + 2.0, 12.0, 12.0);
        if in_box && self.press && self.act == 0 {
            if let Some(i) = pick(self, &c) {
                let interior = i > 0 && i + 1 < c.n;
                if self.dbl(200 + i as i32) && interior {
                    for k in i..c.n - 1 {
                        c.p[k] = c.p[k + 1];
                    }
                    c.n -= 1;
                    self.last_clk_n = 0;
                } else {
                    self.act = 160;
                    self.cpt = i;
                }
            } else if c.n < MAX_PTS {
                let (vx, vy) = to_v(self.mx, self.my);
                let at = (1..c.n).find(|&k| c.p[k].0 > vx).unwrap_or(c.n - 1);
                let room = vx - c.p[at - 1].0 >= MIN_GAP && c.p[at].0 - vx >= MIN_GAP;
                if room {
                    for k in (at..c.n).rev() {
                        c.p[k + 1] = c.p[k];
                    }
                    c.p[at] = (vx, vy.clamp(-1.0, 1.0));
                    c.n += 1;
                    self.act = 160;
                    self.cpt = at;
                    self.last_clk_n = 0; // a creating click never counts towards a double click
                }
            }
            self.vt1 = 0;
            self.vt2 = 0;
            self.press = false;
        }
        if self.act == 160 && self.down && self.cpt < c.n {
            let (vx, vy) = to_v(self.mx, self.my);
            let i = self.cpt;
            if i > 0 && i + 1 < c.n {
                c.p[i].0 = vx.clamp(c.p[i - 1].0 + MIN_GAP, c.p[i + 1].0 - MIN_GAP);
            }
            c.p[i].1 = vy.clamp(-1.0, 1.0);
        }
        if c != before {
            *self.env.params.custom_curve.lock() = c.pts().to_vec();
        }
        // identity reference (dotted), the curve, then the points
        self.set_dk(0.5);
        self.setc(false);
        for i in 0..(ew / 3.0).floor() as i32 {
            let vx = -1.0 + 2.0 * i as f32 / (ew / 3.0).floor();
            self.cv.fill(px(vx), py(vx), 1.0, 1.0); // identity
            self.cv.fill(px(vx), py(1.0), 1.0, 1.0); // domain frame (+-1)
            self.cv.fill(px(vx), py(-1.0), 1.0, 1.0);
            self.cv.fill(px(1.0), py(vx), 1.0, 1.0);
            self.cv.fill(px(-1.0), py(vx), 1.0, 1.0);
        }
        self.set_dk(1.0);
        let n = 96;
        let (mut lx, mut ly) = (px(-1.0), py(c.eval(-1.0) as f32));
        for i in 1..=n {
            let vx = -1.0 + 2.0 * i as f32 / n as f32;
            let (qx, qy) = (px(vx), py(c.eval(vx as f64) as f32));
            self.cv.line(lx, ly, qx, qy);
            lx = qx;
            ly = qy;
        }
        let hot = if self.act == 160 { Some(self.cpt) } else { pick(self, &c) };
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

    // ------------------------------------------------------------------ comp plot
    fn draw_ccurve(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let gc = GainComputer::from_params(&self.env.dsp);
        let hv = self.hit(x, y, w, h);
        let thr = self.sget(8);
        if self.act == 120 && self.down {
            let xin = -48.0 + (self.mx - x) / (w - 1.0) * 48.0;
            let yo = -48.0 + (1.0 - (self.my - y) / (h - 1.0)) * 48.0;
            self.setv(8, (xin + yo) * 0.5);
        }
        if self.act == 121 && self.down {
            let yo = (-48.0 + (1.0 - (self.my - y) / (h - 1.0)) * 48.0).max(thr).min(0.0);
            if thr < -0.3 {
                self.setv(9, -thr / (yo - thr).max(0.05));
            }
        }
        let thr = self.sget(8);
        let gc = if self.act == 120 || self.act == 121 {
            self.refresh_dsp();
            GainComputer::from_params(&self.env.dsp)
        } else {
            gc
        };
        let hxt = x + (thr + 48.0) / 48.0 * (w - 1.0);
        let hyt = y + (1.0 - (thr + 48.0) / 48.0) * (h - 1.0);
        let hxr = x + w - 5.0;
        let hyr = y + (1.0 - (-(gc.gr(0.0) as f32) + 48.0) / 48.0) * (h - 1.0);
        if hv && self.press && self.act == 0 {
            if (self.mx - hxr).abs() <= 6.0 && (self.my - hyr).abs() <= 6.0 {
                if self.dbl(121) {
                    let d = self.meta[9].def;
                    self.click_set(9, d);
                } else {
                    self.act = 121;
                    self.vt1 = 9;
                    self.vt2 = 0;
                }
                self.press = false;
            } else if (self.mx - hxt).abs() <= 6.0 && (self.my - hyt).abs() <= 6.0 {
                if self.dbl(120) {
                    let d = self.meta[8].def;
                    self.click_set(8, d);
                } else {
                    self.act = 120;
                    self.vt1 = 8;
                    self.vt2 = 0;
                }
                self.press = false;
            }
        }
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        let lvl = (8.6858896 * (self.env.shared.vz_lv.load(Ordering::Relaxed).max(0.000001)).ln()).clamp(-48.0, 0.0);
        self.setc(true);
        for i in 0..(w - 2.0) as i32 {
            let xin = -48.0 + 48.0 * i as f32 / (w - 3.0);
            if xin <= lvl {
                let py = y + (1.0 - (xin - gc.gr(xin as f64) as f32 + 48.0) / 48.0) * (h - 1.0);
                self.cv.fill(x + 1.0 + i as f32, py, 1.0, y + h - 1.0 - py);
            }
        }
        self.setc(false);
        for i in 0..(w / 3.0).floor() as i32 {
            self.cv.fill(x + i as f32 * 3.0 + 1.0, y + h - 1.0 - ((i * 3) as f32 * (h - 1.0) / (w - 1.0)).floor(), 1.0, 1.0);
        }
        let n = 96;
        let (mut lx, mut ly) = (0.0, 0.0);
        for i in 0..=n {
            let xin = -48.0 + 48.0 * i as f32 / n as f32;
            let yout = xin - gc.gr(xin as f64) as f32;
            let px = x + (i as f32 / n as f32) * (w - 1.0);
            let py = y + (1.0 - (yout + 48.0) / 48.0) * (h - 1.0);
            if i > 0 {
                self.cv.line(lx, ly, px, py);
            }
            lx = px;
            ly = py;
        }
        if lvl > -47.9 {
            let px = x + (lvl + 48.0) / 48.0 * (w - 1.0);
            let py = y + (1.0 - (lvl - gc.gr(lvl as f64) as f32 + 48.0) / 48.0) * (h - 1.0);
            self.cv.fill(px - 1.0, py - 1.0, 3.0, 3.0);
        }
        let tx = x + (thr + 48.0) / 48.0 * (w - 1.0);
        self.cv.fill(tx, y + h - 4.0, 1.0, 3.0);
        // handles
        self.setc(self.act == 120);
        self.cv.disc(hxt, hyt, 3.5);
        self.setc(self.act == 121);
        self.cv.fill(hxr - 3.0, hyr - 3.0, 7.0, 7.0);
        self.setc(false);
        self.cv.set_font(1);
        if self.rd_tick {
            self.rd_lvl = lvl;
            self.rd_ghr = if lvl > -47.9 { gc.gr(lvl as f64) as f32 } else { 0.0 };
        }
        let s = if self.rd_lvl > -47.9 {
            format!("LVL {:+05.1}  OUT {:+05.1}  GR {:04.1}dB", self.rd_lvl, self.rd_lvl - self.rd_ghr, self.rd_ghr)
        } else {
            "LVL -----  OUT -----  GR 00.0dB".to_string()
        };
        self.cv.text(x + 3.0, y + 2.0, &s);
    }

    // ------------------------------------------------------------------ EQ page
    fn eq_fx(&self, f: f32) -> f32 {
        self.ex0 + (f.max(20.0) / 20.0).ln() * 0.14476483 * (self.epw - 1.0)
    }
    fn eq_xf(&self, px: f32) -> f32 {
        20.0 * ((px - self.ex0) / (self.epw - 1.0) * 6.9077553).exp()
    }
    fn eq_gy(&self, g: f32) -> f32 {
        self.ey0 + (self.eph - 2.0) * 0.5 * (1.0 - g / 18.0)
    }
    fn eq_yg(&self, py: f32) -> f32 {
        (1.0 - (py - self.ey0) / ((self.eph - 2.0) * 0.5)) * 18.0
    }

    fn eq_disp_prep(&mut self, st: usize) {
        let sr = self.env.shared.sample_rate.load(Ordering::Relaxed) as f64;
        let stage = self.env.dsp.eq[st];
        for b in 0..3 {
            let d = decode_band(&stage, b);
            self.dsc[b] = eq_coef(d.kind, d.freq, d.gain, d.q, sr);
            self.dsa[b] = d.on;
            self.enx[b] = self.eq_fx(d.freq as f32);
            self.eny[b] = if d.kind <= 1 { self.eq_gy(0.0) } else { self.eq_gy(d.gain as f32) };
            self.ekd[b] = d.kind;
        }
        let (t, p) = decode_tilt(&stage);
        self.et_t = t as f32;
        self.et_p = p as f32;
        self.dsc[3] = eq_coef(2, p, -t * 0.5, 0.4, sr);
        self.dsc[4] = eq_coef(3, p, t * 0.5, 0.4, sr);
        self.dsa[3] = t != 0.0;
        self.dsa[4] = t != 0.0;
    }

    fn eq_curve_db(&self, f: f32) -> f32 {
        let sr = self.env.shared.sample_rate.load(Ordering::Relaxed) as f64;
        let w = 2.0 * std::f64::consts::PI * (f as f64).min(sr * 0.49) / sr;
        let mut d = 0.0;
        for k in 0..5 {
            if self.dsa[k] {
                d += mag_db(&self.dsc[k], w);
            }
        }
        d as f32
    }

    fn spec_update(&mut self) {
        if self.now - self.spec_t < 0.03 {
            return;
        }
        self.spec_t = self.now;
        let tap = &self.env.shared.tap;
        let fw = tap.w.load(Ordering::Acquire);
        let mut re = vec![0.0f32; 4096];
        let mut im = vec![0.0f32; 4096];
        let ix = fw.wrapping_sub(4096);
        for i in 0..4096 {
            re[i] = tap.get(ix.wrapping_add(i)) * self.fwin[i];
        }
        fft(&mut re, &mut im);
        for k in 0..2049 {
            let db = 8.6858896 * ((re[k] * re[k] + im[k] * im[k]).sqrt() * 0.0009765625 + 0.0000001).ln();
            let sp = &mut self.spk[k];
            *sp += (db - *sp) * if db > *sp { 0.6 } else { 0.12 };
        }
    }

    fn draw_spec(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let n = (w - 2.0) as i32;
        let bw2 = self.env.shared.sample_rate.load(Ordering::Relaxed) / 4096.0;
        self.setc(true);
        for i in 0..n {
            let flo = 20.0 * (i as f32 / n as f32 * 6.9077553).exp();
            let fhi = 20.0 * ((i + 1) as f32 / n as f32 * 6.9077553).exp();
            let bl = (flo / bw2).min(2046.0);
            let bh = (fhi / bw2).min(2047.0);
            let mut v;
            if bh - bl < 1.0 {
                let k = bl.floor() as usize;
                let fr = bl - k as f32;
                v = self.spk[k] + (self.spk[k + 1] - self.spk[k]) * fr;
            } else {
                v = -140.0;
                let (mut k, kk) = (bl.floor() as usize, bh.floor() as usize);
                let mut cnt = 0;
                while k <= kk && cnt < 512 {
                    v = v.max(self.spk[k]);
                    k += 1;
                    cnt += 1;
                }
            }
            v += 6.4921 * ((flo * fhi).sqrt() / 1000.0).ln();
            let hh = (((v + 100.0) / 95.0).clamp(0.0, 1.0) * (h - 2.0)).floor();
            if hh > 0.0 {
                self.cv.fill(x + 1.0 + i as f32, y + h - 1.0 - hh, 1.0, hh);
            }
        }
        self.setc(false);
    }

    /// one band's type strip (3 filter glyphs) over its Q cell. n = the band's Type slider, Q = n+3
    fn typeq(&mut self, n: usize, b: usize, x: f32, y: f32, w: f32, h: f32) {
        let nq = n + 3;
        let sw = (w / 3.0).floor();
        let sel = ui_idx(b, self.sget(n) as f64);
        let def = if b == 1 { 2 } else { 1 };
        for i in 0..3usize {
            let sx = x + i as f32 * sw;
            if self.hit(sx, y, sw, 17.0) && self.press && self.act == 0 {
                self.click_set(n, ((i + 3 - def) % 3) as f32);
                self.press = false;
            }
            if i == sel {
                self.setc(false);
                self.cv.fill(sx, y, sw, 17.0);
                self.cv.setbg();
            } else {
                self.setc(false);
            }
            self.glyph(kind_of(b, i), sx + sw * 0.5, y + 8.5);
        }
        self.setc(self.act == nq as i32);
        self.cv.boxr(x, y, w, h);
        self.setc(false);
        self.cv.fill(x, y + 17.0, w, 1.0);
        let hq = self.hit(x, y + 18.0, w, h - 18.0);
        if hq && self.press && self.act == 0 {
            self.begin_drag(nq);
        }
        if hq && self.wheel != 0 {
            self.wheel_adj(nq);
        }
        let t = self.n2t(nq, self.sget(nq));
        self.setc(self.act == nq as i32);
        self.cv.set_font(1);
        self.cv.text(x + 3.0, y + 21.0, "Q");
        self.cv.set_font(2);
        let s = self.fmt(nq, self.sget(nq));
        self.cv.textc(x + 8.0, y + 19.0, w - 8.0, &s);
        self.cv.fill(x + 2.0, y + h - 4.0, ((t * (w - 4.0)).floor()).max(1.0), 2.0);
        self.setc(false);
    }

    fn draw_eq(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.ex0 = x + 1.0;
        self.epw = w - 2.0;
        self.ey0 = y + 1.0;
        self.eph = h - 14.0;
        let sh = self.env.shared;
        sh.eq_vis_ms.store(sh.now_ms(), Ordering::Relaxed);
        sh.eq_vs.store(self.eqv, Ordering::Relaxed);
        self.spec_update();
        let sb = 29 + self.eqv * 15;
        let tn = sb + 12;
        let hv = self.hit(x, y, w, self.eph);
        self.eq_disp_prep(self.eqv);
        // ---- interaction
        let mut best: i32 = -1;
        let mut bd = 80.0;
        for b in 0..3 {
            let d2 = (self.mx - self.enx[b]).powi(2) + (self.my - self.eny[b]).powi(2);
            if d2 < bd {
                bd = d2;
                best = b as i32;
            }
        }
        let mut pvx = self.eq_fx(self.et_p);
        if hv && self.wheel != 0 && best >= 0 {
            self.wheel_adj(sb + best as usize * 4 + 3);
        }
        if hv && self.press && self.act == 0 {
            if best >= 0 {
                let bi = best as usize;
                let n = sb + bi * 4;
                if self.dbl(130 + best) {
                    let d = self.meta[n + 1].def;
                    self.sset(n + 1, d);
                    self.sset(n + 2, 0.0);
                    self.end_touch(n + 1);
                    self.end_touch(n + 2);
                } else {
                    self.act = 101 + best;
                    self.vt1 = n + 1;
                    self.vt2 = if self.ekd[bi] > 1 { n + 2 } else { 0 };
                    self.gdx = self.enx[bi] - self.mx;
                    self.gdy = self.eny[bi] - self.my;
                }
            } else if (self.mx - pvx).abs() <= 3.0 {
                if self.dbl(140) {
                    let d = self.meta[sb + 13].def;
                    self.click_set(sb + 13, d);
                } else {
                    self.act = 110;
                    self.vt1 = sb + 13;
                    self.vt2 = 0;
                    self.gdx = pvx - self.mx;
                }
            } else if self.dbl(141) {
                self.click_set(tn, 0.0);
            } else {
                self.act = 111;
                self.vt1 = tn;
                self.vt2 = 0;
                self.tacc = self.sget(tn);
                self.tlast = self.mx;
            }
            self.press = false;
        }
        if (101..=103).contains(&self.act) && self.down {
            let b = (self.act - 101) as usize;
            let n = sb + b * 4;
            let f = self.eq_xf(self.mx + self.gdx);
            self.setv(n + 1, f);
            if self.ekd[b] > 1 {
                let g = self.eq_yg(self.my + self.gdy);
                self.setv(n + 2, g);
            }
        }
        if self.act == 110 && self.down {
            let f = self.eq_xf(self.mx + self.gdx);
            self.setv(sb + 13, f);
        }
        if self.act == 111 && self.down {
            self.tacc += (self.mx - self.tlast) * (24.0 / self.epw) * if self.shift { 0.15 } else { 1.0 };
            self.tlast = self.mx;
            self.tacc = self.tacc.clamp(-12.0, 12.0);
            let t = self.tacc;
            self.setv(tn, t);
        }
        self.refresh_dsp();
        self.eq_disp_prep(self.eqv);
        pvx = self.eq_fx(self.et_p);
        // ---- drawing
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        self.draw_spec(x, y, w, self.eph);
        self.setc(false);
        self.cv.fill(x + 1.0, self.eq_gy(0.0).floor(), w - 2.0, 1.0);
        for i in 0..((self.eph - 2.0) / 3.0).floor() as i32 {
            self.cv.fill(pvx, self.ey0 + i as f32 * 3.0, 1.0, if self.act == 110 { 3.0 } else { 2.0 });
        }
        let n = (self.epw / 2.0).floor() as i32;
        let (mut lx, mut ly) = (0.0, 0.0);
        for i in 0..=n {
            let px = self.ex0 + i as f32 * (self.epw - 1.0) / n as f32;
            let d = self.eq_curve_db(self.eq_xf(px));
            let py = self.eq_gy(d.clamp(-18.0, 18.0));
            if i > 0 {
                self.cv.line(lx, ly, px, py);
            }
            lx = px;
            ly = py;
        }
        for b in 0..3 {
            self.cv.setbg();
            self.cv.fill(self.enx[b] - 4.0, self.eny[b] - 4.0, 9.0, 9.0);
            self.setc(false);
            self.cv.fill(self.enx[b] - 3.0, self.eny[b] - 3.0, 7.0, 7.0);
            if self.act == 101 + b as i32 {
                self.setc(true);
                self.cv.fill(self.enx[b] - 1.0, self.eny[b] - 1.0, 3.0, 3.0);
            }
        }
        self.setc(false);
        self.cv.set_font(1);
        let s = if self.et_t.abs() < 0.05 { "TILT 0.0dB".to_string() } else { format!("TILT {:+.1}dB", self.et_t) };
        self.cv.text(x + 3.0, y + 2.0, &s);
        // ---- stage tabs
        let sw = (w / 2.0).floor();
        let ey = y + self.eph;
        self.sq_tog(60, x + 24.0, ey + 3.0, 8.0);
        self.sq_tog(61, x + sw + 20.0, ey + 3.0, 8.0);
        if self.btn(x, ey, sw, 14.0, "PRE-COMP", (self.eqv == 0) as u8) {
            self.eqv = 0;
        }
        if self.btn(x + sw, ey, w - sw, 14.0, "POST-COMP", (self.eqv == 1) as u8) {
            self.eqv = 1;
        }
        let (a0, a1) = self.eq_active();
        let (p0, p1) = (self.sget(60) > 0.5, self.sget(61) > 0.5);
        self.sqind(x + 24.0, ey + 3.0, 8.0, a0, self.eqv == 0, p0);
        self.sqind(x + sw + 20.0, ey + 3.0, 8.0, a1, self.eqv == 1, p1);
    }

    /// (pre, post) stage active: any band on, tilt, or out gain != 0
    fn eq_active(&self) -> (bool, bool) {
        let act = |st: usize| {
            let p = &self.env.dsp.eq[st];
            (0..3).any(|b| decode_band(p, b).on) || decode_tilt(p).0 != 0.0 || p.out_gain.clamp(-12.0, 12.0) != 0.0
        };
        (act(0), act(1))
    }

    fn draw_eq_cells(&mut self) {
        let sb = 29 + self.eqv * 15;
        for b in 0..3usize {
            let n = sb + b * 4;
            let x = 152.0 + b as f32 * 68.0;
            self.typeq(n, b, x, 196.0, 62.0, 38.0);
            self.cell(n + 1, x, 240.0, 62.0, 38.0, "FREQ");
            let d = decode_band(&self.env.dsp.eq[self.eqv], b);
            if d.kind <= 1 {
                self.setc(false);
                self.cv.boxr(x, 284.0, 62.0, 38.0);
                self.cv.set_font(1);
                self.cv.text(x + 3.0, 287.0, "GAIN");
                self.cv.set_font(2);
                self.cv.textc(x, 300.0, 62.0, "---");
            } else {
                self.cell(n + 2, x, 284.0, 62.0, 38.0, "GAIN");
            }
        }
        self.cell(sb + 13, 356.0, 196.0, 62.0, 38.0, "PIVOT");
        self.cell(sb + 12, 356.0, 240.0, 62.0, 38.0, "TILT");
        self.cell_o(sb + 14, 356.0, 284.0, 62.0, 38.0, "OUT GAIN");
    }

    // ------------------------------------------------------------------ window size
    fn cur_scale(&self) -> f32 {
        self.env.params.editor_state.size().0 as f32 / 560.0
    }

    /// Ask the host to resize the editor to `scale` x the 560x336 base size (aspect locked).
    fn request_scale(&mut self, scale: f32) {
        let sc = scale.clamp(SIZES[0], SIZES[SIZES.len() - 1]);
        if (sc - self.cur_scale()).abs() > 0.001 {
            self.env.params.editor_state.set_requested_size(((560.0 * sc).round() as u32, (336.0 * sc).round() as u32));
        }
    }

    /// Bottom-right grip: drag = aspect-locked resize in 25% steps, right-click = size menu.
    fn resize_grip(&mut self) {
        let (gx, gy, gw, gh) = (544.0, 320.0, 16.0, 16.0);
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
                    let w = (self.ptr_pt.0 + self.resize_off.0) / 560.0;
                    let h = (self.ptr_pt.1 + self.resize_off.1) / 336.0;
                    self.request_scale((w.min(h) * 4.0).round() / 4.0);
                }
            } else {
                self.resizing = false;
            }
        }
        self.set_dk(1.0);
        self.setc(self.resizing);
        for o in [4.0, 8.0, 12.0] {
            self.cv.line(559.0, 335.0 - o, 559.0 - o, 335.0);
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
        self.set_preset_name(&p.name);
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
            MItem { label: "Load preset...".into(), kind: 0, id: 0 },
            MItem { label: String::new(), kind: 2, id: -1 },
            MItem { label: "FACTORY".into(), kind: 1, id: -1 },
        ];
        for (i, p) in self.factory.iter().enumerate() {
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
            Some(i) if i < self.factory.len() => 1000 + i as i32,
            Some(i) => 2000 + (i - self.factory.len()) as i32,
            None => -1,
        };
        self.menu = Some(Menu { kind: MenuKind::Preset, x, y, items, checked, scroll: 0 });
    }

    fn preset_action(&mut self, id: i32) {
        match id {
            0 => self.pending = 1,
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

    /// File dialogs block, so they run after the frame has been drawn.
    fn run_pending(&mut self) {
        let what = std::mem::take(&mut self.pending);
        match what {
            1 => {
                if let Some(path) = presets::open_dialog() {
                    if let Some(mut p) = presets::load_file(&path) {
                        if let Some(stem) = path.file_stem() {
                            p.name = stem.to_string_lossy().into_owned();
                        }
                        self.apply_preset(&p);
                    }
                }
            }
            2 => {
                let cur = self.preset_name();
                let def = if cur.is_empty() || cur == "INIT" { "My preset".to_string() } else { cur };
                if let Some(path) = presets::save_dialog(&def) {
                    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(def);
                    if presets::save_file(&path, &self.capture(&name)).is_ok() {
                        self.set_preset_name(&name);
                        self.user = presets::scan_user();
                    }
                }
            }
            _ => {}
        }
    }

    /// Header preset bar (save icon | name | prev next) and the window-scale button.
    fn preset_bar(&mut self) {
        let (y, h) = (8.0, 16.0);
        // save icon
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
        let hv = self.hit(178.0, y, 142.0, h);
        if hv && self.press && self.act == 0 {
            self.open_preset_menu(178.0, y + h);
            self.press = false;
        }
        self.setc(false);
        self.cv.boxr(178.0, y, 142.0, h);
        self.cv.set_font(2);
        let mut t = shown;
        while self.cv.text_w(&t) > 136.0 && t.chars().count() > 3 {
            t.pop();
        }
        self.cv.textc(178.0, y + ((h - 8.0) * 0.5).floor(), 142.0, &t);
        // prev / next
        for (i, dir) in [(0, -1), (1, 1)] {
            let x = 320.0 + i as f32 * 25.0;
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
        if self.btn(376.0, y, 42.0, h, &label, 0) {
            let cur = SIZES.iter().position(|v| (v - self.cur_scale()).abs() < 0.13).unwrap_or(0);
            let items = SIZES.iter().map(|v| format!("{:.0}%", v * 100.0)).collect();
            self.open_menu(MenuKind::Size, 376.0, y + h, items, cur);
        }
    }

    // ------------------------------------------------------------------ main frame
    pub fn frame(&mut self) {
        let sh = self.env.shared;
        // readouts refresh ~8x/s
        self.rd_tick = self.now - self.rd_last >= 0.12;
        if self.rd_tick {
            self.rd_last = self.now;
            self.rd_gr = self.gr_disp;
            self.rd_pk = self.opk_max;
            self.rd_rms = sh.fms.load(Ordering::Relaxed);
            self.rd_diff = sh.diff_disp.load(Ordering::Relaxed);
            self.rd_bg = sh.bg.load(Ordering::Relaxed);
            self.rd_lk = self.env.params.match_trim.load(Ordering::Relaxed);
            self.rd_mt = sh.out_mt.load(Ordering::Relaxed);
            self.rd_sat = sh.sat_db.load(Ordering::Relaxed);
        }
        // a finished sat level-match writes Sat Out (the audio thread cannot set parameters)
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
        let eq_mod_byp = self.sget(62) > 0.5;

        // ---------- header
        self.set_dk(1.0);
        self.logo(10.0, 5.0);
        self.setc(false);
        self.cv.set_font(3);
        self.cv.text(38.0, 10.0, "SATCOMP 0.3");
        self.preset_bar();
        self.out_meter(430.0, 8.0, 120.0, 16.0);

        // ---------- tab row
        self.set_dk(if byp { 0.5 } else { 1.0 });
        self.selector(1, 10.0, 34.0, 130.0, 16.0);
        self.setc(false);
        self.cv.boxr(152.0, 34.0, 96.0, 16.0);
        self.cv.set_font(1);
        self.cv.textc(152.0, 38.0, 96.0, "OVERSAMPLING");
        let os_i = (self.sget(7) + 0.5).floor() as i32;
        for i in 0..3 {
            let l = ["OFF", "x2", "x4"][i as usize];
            if self.btn(248.0 + i as f32 * 34.0, 34.0, 34.0, 16.0, l, (os_i == i) as u8) {
                self.click_set(7, i as f32);
            }
        }
        self.sq_tog(62, 366.0, 38.0, 8.0);
        if self.btn(356.0, 34.0, 62.0, 16.0, "EQ", self.eq_page as u8) {
            self.eq_page = !self.eq_page;
        }
        let (a0, a1) = self.eq_active();
        self.sqind(366.0, 38.0, 8.0, a0 || a1, self.eq_page, eq_mod_byp);
        self.set_dk(1.0);
        let delta = sh.delta_on.load(Ordering::Relaxed);
        if self.btn(430.0, 34.0, 60.0, 16.0, "DELTA", if delta { 2 } else { 0 }) {
            sh.delta_on.store(!delta, Ordering::Relaxed);
        }
        if self.btn(490.0, 34.0, 60.0, 16.0, "BYPASS", byp as u8) {
            self.click_set(26, if byp { 0.0 } else { 1.0 });
        }

        // ---------- viz row
        self.set_dk(if byp { 0.5 } else { 1.0 });
        self.draw_plot(10.0, 56.0, 130.0, 128.0);
        self.set_dk(if byp || (self.eq_page && eq_mod_byp) { 0.5 } else { 1.0 });
        if self.eq_page {
            self.draw_eq(152.0, 56.0, 266.0, 128.0);
        } else {
            self.draw_ccurve(152.0, 56.0, 266.0, 128.0);
        }
        self.set_dk(1.0);
        self.draw_meter(430.0, 56.0, 28.0, 266.0);
        self.set_dk(if byp { 0.5 } else { 1.0 });

        // ---------- SAT cells
        self.cell(2, 10.0, 196.0, 62.0, 38.0, "DRIVE");
        self.cell(3, 78.0, 196.0, 62.0, 38.0, "BIAS");
        self.cell2(4, 5, 10.0, 240.0, 62.0, 38.0, "HP", "LP");
        self.cell(6, 78.0, 240.0, 62.0, 38.0, "TONE");
        self.cell(25, 10.0, 284.0, 62.0, 38.0, "SAT MIX");
        self.cell_o(27, 78.0, 284.0, 62.0, 38.0, "OUT GAIN");

        // ---------- COMP cells (or the EQ page's cells)
        if self.eq_page {
            let stage_byp = self.sget(if self.eqv == 0 { 60 } else { 61 }) > 0.5;
            self.set_dk(if byp || eq_mod_byp || stage_byp { 0.5 } else { 1.0 });
            self.draw_eq_cells();
        } else {
            self.cell(8, 152.0, 196.0, 62.0, 38.0, "THRESH");
            self.cell(9, 220.0, 196.0, 62.0, 38.0, "RATIO");
            self.cell(10, 288.0, 196.0, 62.0, 38.0, "ATTACK");
            self.cell(11, 356.0, 196.0, 62.0, 38.0, "RELEASE");
            self.cell(13, 152.0, 240.0, 62.0, 38.0, "KNEE");
            self.cell(14, 220.0, 240.0, 62.0, 38.0, "K.SHAPE");
            self.cell(15, 288.0, 240.0, 62.0, 38.0, "PEAK>RMS");
            self.cell(16, 356.0, 240.0, 62.0, 38.0, "LOW CUT");
            self.cell(59, 152.0, 284.0, 62.0, 38.0, "L.AHEAD");
            self.cell(12, 220.0, 284.0, 62.0, 38.0, "SPEED");
            self.cell(18, 288.0, 284.0, 62.0, 38.0, "FF>FB");
            self.cell(19, 356.0, 284.0, 62.0, 38.0, "GR DIST");
        }

        // ---------- OUT column
        self.set_dk(1.0);
        self.cv.set_font(1);
        self.setc(false);
        self.cv.text(464.0, 58.0, "GR");
        let s = format!("{:04.1}", self.rd_gr);
        self.cv.textr(464.0, 58.0, 86.0, &s);
        let gmatch = self.sget(23) > 0.5;
        let meas = sh.out_meas.load(Ordering::Relaxed);
        let s1 = if gmatch {
            if meas {
                self.cv.text(464.0, 70.0, "MEAS");
                format!("{:.1}/3.0s", self.rd_mt)
            } else {
                self.cv.text(464.0, 70.0, "LOCK");
                format!("{:+05.1}dB", self.rd_lk)
            }
        } else {
            self.cv.text(464.0, 70.0, "TRIM");
            "-----dB".to_string()
        };
        self.cv.textr(464.0, 70.0, 86.0, &s1);
        let s1 = if self.rd_pk > 0.00001 { format!("{:+05.1}dB", 8.6858896 * self.rd_pk.ln()) } else { "-----dB".into() };
        self.setc(self.rd_pk > 0.9999);
        self.cv.text(464.0, 88.0, "PK");
        self.cv.textr(464.0, 88.0, 86.0, &s1);
        self.setc(false);
        let s1 = if self.rd_rms > 0.000000001 { format!("{:+05.1}dB", 4.3429448 * self.rd_rms.ln()) } else { "-----dB".into() };
        self.cv.text(464.0, 100.0, "RMS");
        self.cv.textr(464.0, 100.0, 86.0, &s1);
        if byp {
            self.cv.text(464.0, 112.0, "BYP");
            let s = format!("{:+05.1}dB", 8.6858896 * self.rd_bg.max(1e-9).ln());
            self.cv.textr(464.0, 112.0, 86.0, &s);
        }
        // GAIN MATCH: off -> click measures 3 s then locks a trim (red); click while measuring cancels; click while locked turns it off
        let gm_lit = if gmatch { if meas && blink(self.now) { 1 } else { 2 } } else { 0 };
        if self.btn(464.0, 128.0, 86.0, 14.0, "GAIN MATCH", gm_lit) {
            if gmatch {
                self.click_set(23, 0.0);
            } else {
                self.click_set(23, 1.0);
                sh.match_req.store(true, Ordering::Relaxed);
            }
        }
        self.diff_box(464.0, 146.0, 86.0, 38.0);
        self.cell(20, 464.0, 196.0, 86.0, 38.0, "MAKEUP");
        self.cell(21, 464.0, 240.0, 86.0, 38.0, "MIX");
        self.cell_o(22, 464.0, 284.0, 86.0, 38.0, "OUT GAIN");

        self.resize_grip();
        self.menu_draw();
        self.run_pending();
    }
}

/// In-place radix-2 complex FFT (n = 4096), bit-reversal included.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (i + k, i + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
            i += len;
        }
        len <<= 1;
    }
}
