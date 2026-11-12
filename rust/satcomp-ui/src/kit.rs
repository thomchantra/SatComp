//! Immediate-mode widget kit: the interaction state (`Kit`) and the widgets (`Widgets`), port of the JSFX `@gfx` helpers.
//!
//! Parameters are addressed by slot number `n` (the JSFX slider number): the host plug-in maps a slot to a nih-plug
//! `ParamPtr` and formats its value; this kit does the dragging, snapping, double-click reset, wheel and drawing.
//! A plug-in implements `Widgets` for its per-frame GUI type, which must `Deref` to its own state that in turn derefs to
//! `Kit` (so widget code and page code both read `self.mx`, `self.press`, ...).

use crate::canvas::Canvas;
use nih_plug::prelude::{GuiContext, ParamPtr};

/// Value range and drag behaviour of one parameter slot.
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

/// Interaction state shared by all widgets.
pub struct Kit {
    pub meta: Vec<Meta>,
    pub touched: Vec<bool>,
    /// the widget being dragged: 1..99 = slot number (vertical drag), >= 100 = viewport drags owned by the page
    pub act: i32,
    pub act_t: f32,
    pub last_my: f32,
    pub last_clk_n: i32,
    pub last_clk_t: f64,
    pub last_down: bool,
    pub last_rdown: bool,
    pub wacc: f32,
    // per-frame input (logical units)
    pub mx: f32,
    pub my: f32,
    pub down: bool,
    pub press: bool,
    pub rpress: bool,
    pub wheel: i32,
    pub shift: bool,
    pub now: f64,
    /// 1 = normal, 0.5 = dimmed (modules dimmed while bypassed)
    pub dk: f32,
}

impl Kit {
    pub fn new(slots: usize) -> Self {
        Self {
            meta: vec![Meta::default(); slots],
            touched: vec![false; slots],
            act: 0,
            act_t: 0.0,
            last_my: 0.0,
            last_clk_n: 0,
            last_clk_t: 0.0,
            last_down: false,
            last_rdown: false,
            wacc: 0.0,
            mx: -1000.0,
            my: -1000.0,
            down: false,
            press: false,
            rpress: false,
            wheel: 0,
            shift: false,
            now: 0.0,
            dk: 1.0,
        }
    }

    /// Per-frame input snapshot (positions in logical canvas units). Computes press edges like the JSFX.
    /// Returns true when the clock went backwards (the egui clock restarts from 0 whenever the editor window is reopened
    /// while the state lives on): the owner must then re-base its own stored timestamps to `now`.
    pub fn set_input(&mut self, mx: f32, my: f32, down: bool, rdown: bool, scroll: f32, shift: bool, now: f64) -> bool {
        self.mx = mx;
        self.my = my;
        self.down = down;
        self.press = down && !self.last_down;
        self.last_down = down;
        self.rpress = rdown && !self.last_rdown;
        self.last_rdown = rdown;
        self.shift = shift;
        let restarted = now < self.now;
        if restarted {
            self.last_clk_t = now;
        }
        self.now = now;
        // notch the scroll: a mouse wheel reports big steps, a trackpad many small ones
        self.wacc += scroll;
        self.wheel = 0;
        if self.wacc.abs() >= 10.0 {
            self.wheel = if self.wacc > 0.0 { 1 } else { -1 };
            self.wacc = 0.0;
        }
        restarted
    }
}

/// Widgets over `Kit`. `'c` is the canvas lifetime of the implementing GUI type.
pub trait Widgets<'c> {
    fn kit(&mut self) -> &mut Kit;
    fn kit_ref(&self) -> &Kit;
    fn cv(&mut self) -> &mut Canvas<'c>;
    /// the parameter behind slot `n`
    fn param_ptr(&self, n: usize) -> Option<ParamPtr>;
    fn host(&self) -> &dyn GuiContext;
    /// widgets draw but must not write parameters (theme editor preview page)
    fn masked(&self) -> bool {
        false
    }
    /// display text of a slot's value
    fn fmt(&self, n: usize, v: f32) -> String;

    // ------------------------------------------------------------------ params (slot-number addressing)
    fn sget(&self, n: usize) -> f32 {
        match self.param_ptr(n) {
            Some(p) => {
                let m = self.kit_ref().meta[n];
                m.min + (m.max - m.min) * unsafe { p.unmodulated_normalized_value() }
            }
            None => 0.0,
        }
    }

    fn sset(&mut self, n: usize, v: f32) {
        if self.masked() {
            return; // dummy widgets never write parameters
        }
        if let Some(p) = self.param_ptr(n) {
            let m = self.kit().meta[n];
            let norm = ((v - m.min) / (m.max - m.min)).clamp(0.0, 1.0);
            let first = !self.kit().touched[n];
            unsafe {
                if first {
                    self.host().raw_begin_set_parameter(p);
                    self.kit().touched[n] = true;
                }
                self.host().raw_set_parameter_normalized(p, norm);
            }
        }
    }

    fn end_touch(&mut self, n: usize) {
        if self.masked() {
            return;
        }
        if n < self.kit().touched.len() && self.kit().touched[n] {
            if let Some(p) = self.param_ptr(n) {
                unsafe { self.host().raw_end_set_parameter(p) };
            }
            self.kit().touched[n] = false;
        }
    }

    /// set + release in one go (a click, not a drag)
    fn click_set(&mut self, n: usize, v: f32) {
        self.sset(n, v);
        self.end_touch(n);
    }

    fn setv(&mut self, n: usize, mut v: f32) {
        let m = self.kit().meta[n];
        if m.step > 0.0 {
            v = (v / m.step + 0.5).floor() * m.step;
        }
        v = v.clamp(m.min, m.max);
        if v != self.sget(n) {
            self.sset(n, v);
        }
    }

    fn n2t(&self, n: usize, v: f32) -> f32 {
        let m = self.kit_ref().meta[n];
        let t = match m.mode {
            1 => (v.max(m.min) / m.min).ln() / (m.max / m.min).ln(),
            2 => ((v - m.min).max(0.0) / (m.max - m.min)).sqrt(),
            _ => (v - m.min) / (m.max - m.min),
        };
        t.clamp(0.0, 1.0)
    }

    fn t2n(&self, n: usize, mut t: f32) -> f32 {
        let m = self.kit_ref().meta[n];
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
        let k = self.kit_ref();
        k.mx >= x && k.mx < x + w && k.my >= y && k.my < y + h
    }

    fn dbl(&mut self, id: i32) -> bool {
        let k = self.kit();
        let r = k.last_clk_n == id && k.now - k.last_clk_t < 0.35;
        k.last_clk_n = if r { 0 } else { id };
        k.last_clk_t = k.now;
        r
    }

    fn begin_drag(&mut self, n: usize) {
        let dbl = {
            let k = self.kit();
            k.last_clk_n == n as i32 && k.now - k.last_clk_t < 0.35
        };
        if dbl {
            let d = self.kit().meta[n].def;
            self.click_set(n, d);
            let k = self.kit();
            k.act = 0;
            k.last_clk_n = 0;
        } else {
            let t = {
                let v = self.sget(n);
                self.n2t(n, v)
            };
            let k = self.kit();
            k.act = n as i32;
            k.act_t = t;
            k.last_my = k.my;
            k.last_clk_n = n as i32;
            k.last_clk_t = k.now;
        }
    }

    fn wheel_adj(&mut self, n: usize) {
        let (wheel, shift) = {
            let k = self.kit();
            (k.wheel, k.shift)
        };
        let cur = {
            let v = self.sget(n);
            self.n2t(n, v)
        };
        let t = cur + if wheel > 0 { 1.0 } else { -1.0 } * if shift { 0.005 } else { 0.03 };
        self.commit(n, t.clamp(0.0, 1.0));
        self.end_touch(n);
    }

    /// colour select: UI colour, or accent when `red`; honours the dim factor
    fn setc(&mut self, red: bool) {
        let dk = self.kit().dk;
        let cv = self.cv();
        cv.dk = dk;
        cv.setc(red);
    }

    fn set_dk(&mut self, d: f32) {
        self.kit().dk = d;
        self.cv().dk = d;
    }

    // ------------------------------------------------------------------ widgets
    fn cell(&mut self, n: usize, x: f32, y: f32, w: f32, h: f32, lbl: &str) {
        let hv = self.hit(x, y, w, h);
        if hv && self.kit().press && self.kit().act == 0 {
            self.begin_drag(n);
        }
        if hv && self.kit().wheel != 0 {
            self.wheel_adj(n);
        }
        let v = self.sget(n);
        let t = self.n2t(n, v);
        let isact = self.kit().act == n as i32;
        self.setc(isact);
        let s = self.fmt(n, v);
        let cv = self.cv();
        cv.boxr(x, y, w, h);
        cv.set_font(1);
        cv.text(x + 3.0, y + 3.0, lbl);
        cv.set_font(2);
        cv.textc(x, y + (h * 0.4 + 1.0).floor(), w, &s);
        cv.fill(x + 2.0, y + h - 4.0, ((t * (w - 4.0)).floor()).max(1.0), 2.0);
    }

    /// inverted cell (lit block) for the gain-stage cells
    fn cell_o(&mut self, n: usize, x: f32, y: f32, w: f32, h: f32, lbl: &str) {
        let hv = self.hit(x, y, w, h);
        if hv && self.kit().press && self.kit().act == 0 {
            self.begin_drag(n);
        }
        if hv && self.kit().wheel != 0 {
            self.wheel_adj(n);
        }
        let v = self.sget(n);
        let t = self.n2t(n, v);
        let isact = self.kit().act == n as i32;
        self.setc(isact);
        let s = self.fmt(n, v);
        {
            let cv = self.cv();
            cv.fill(x, y, w, h);
            cv.setbg();
            cv.set_font(1);
            cv.text(x + 3.0, y + 3.0, lbl);
            cv.set_font(2);
            cv.textc(x, y + (h * 0.4 + 1.0).floor(), w, &s);
            cv.fill(x + 2.0, y + h - 4.0, ((t * (w - 4.0)).floor()).max(1.0), 2.0);
        }
        self.setc(false);
    }

    /// one param in a single short row (label left, value right, value bar on the bottom edge)
    fn cell1(&mut self, n: usize, x: f32, y: f32, w: f32, h: f32, lbl: &str) {
        let hv = self.hit(x, y, w, h);
        if hv && self.kit().press && self.kit().act == 0 {
            self.begin_drag(n);
        }
        if hv && self.kit().wheel != 0 {
            self.wheel_adj(n);
        }
        let v = self.sget(n);
        let t = self.n2t(n, v);
        let isact = self.kit().act == n as i32;
        self.setc(isact);
        let s = self.fmt(n, v);
        let cv = self.cv();
        cv.boxr(x, y, w, h);
        cv.set_font(1);
        let ty = y + ((h - 8.0) * 0.5).floor();
        cv.text(x + 3.0, ty, lbl);
        cv.textr(x, ty, w - 3.0, &s);
        cv.fill(x + 2.0, y + h - 3.0, ((t * (w - 4.0)).floor()).max(1.0), 1.0);
        self.setc(false);
    }

    /// two stacked params in one cell (label left, value right)
    fn cell2(&mut self, na: usize, nb: usize, x: f32, y: f32, w: f32, h: f32, la: &str, lb: &str) {
        let hh = (h * 0.5).floor();
        let ha = self.hit(x, y, w, hh);
        let hb = self.hit(x, y + hh, w, h - hh);
        if ha && self.kit().press && self.kit().act == 0 {
            self.begin_drag(na);
        }
        if hb && self.kit().press && self.kit().act == 0 {
            self.begin_drag(nb);
        }
        if ha && self.kit().wheel != 0 {
            self.wheel_adj(na);
        }
        if hb && self.kit().wheel != 0 {
            self.wheel_adj(nb);
        }
        let act = self.kit().act;
        self.setc(act == na as i32 || act == nb as i32);
        self.cv().boxr(x, y, w, h);
        self.setc(false);
        self.cv().fill(x, y + hh, w, 1.0);
        self.cv().set_font(1);
        self.setc(act == na as i32);
        let va = self.sget(na);
        let s = self.fmt(na, va);
        let ta = self.n2t(na, va);
        {
            let cv = self.cv();
            cv.text(x + 3.0, y + 3.0, la);
            cv.textr(x, y + 3.0, w - 3.0, &s);
            cv.fill(x + 2.0, y + hh - 3.0, ((ta * (w - 4.0)).floor()).max(1.0), 1.0);
        }
        self.setc(act == nb as i32);
        let vb = self.sget(nb);
        let s = self.fmt(nb, vb);
        let tb = self.n2t(nb, vb);
        {
            let cv = self.cv();
            cv.text(x + 3.0, y + hh + 3.0, lb);
            cv.textr(x, y + hh + 3.0, w - 3.0, &s);
            cv.fill(x + 2.0, y + h - 4.0, ((tb * (w - 4.0)).floor()).max(1.0), 2.0);
        }
        self.setc(false);
    }

    /// lit: 0 off, 1 white, 2 red
    fn btn(&mut self, x: f32, y: f32, w: f32, h: f32, s: &str, lit: u8) -> bool {
        let hv = self.hit(x, y, w, h);
        let clicked = hv && self.kit().press && self.kit().act == 0;
        if lit > 0 {
            self.setc(lit > 1);
            self.cv().fill(x, y, w, h);
            self.cv().setbg();
        } else {
            self.setc(false);
            self.cv().boxr(x, y, w, h);
        }
        let cv = self.cv();
        cv.set_font(1);
        cv.textc(x, y + ((h - 8.0) * 0.5).floor(), w, s);
        self.setc(false);
        clicked
    }

    /// 12x12 close cross (draw view corner); bright on hover.
    fn close_glyph(&mut self, x: f32, y: f32, hover: bool) {
        let prev = self.kit().dk;
        self.set_dk(prev * if hover { 1.0 } else { 0.6 });
        self.setc(false);
        self.cv().line(x + 1.0, y + 1.0, x + 10.0, y + 10.0);
        self.cv().line(x + 10.0, y + 1.0, x + 1.0, y + 10.0);
        self.set_dk(prev);
    }

    /// 12x12 symmetry glyph: an odd S-curve through a centre dot (the curve is mirrored through the origin).
    fn sym_glyph(&mut self, x: f32, y: f32, on: bool, hover: bool) {
        let prev = self.kit().dk;
        self.set_dk(prev * if on || hover { 1.0 } else { 0.6 });
        self.setc(on);
        let pts = [(0.0, 10.0), (3.0, 10.0), (5.0, 8.0), (6.0, 5.0), (7.0, 2.0), (9.0, 0.0), (12.0, 0.0)];
        for w in pts.windows(2) {
            self.cv().line(x + w[0].0, y + w[0].1, x + w[1].0, y + w[1].1);
        }
        self.cv().fill(x + 5.0, y + 5.0, 2.0, 2.0);
        self.set_dk(prev);
        self.setc(false);
    }

    /// module-active square: filled = active, outline = untouched; byp = outlined with an X
    fn sqind(&mut self, x: f32, y: f32, s: f32, on: bool, lit: bool, byp: bool) {
        if lit {
            self.cv().setbg();
        } else {
            self.setc(false);
        }
        {
            let cv = self.cv();
            if byp {
                cv.boxr(x, y, s, s);
                cv.line(x, y, x + s - 1.0, y + s - 1.0);
                cv.line(x + s - 1.0, y, x, y + s - 1.0);
            } else if on {
                cv.fill(x, y, s, s);
            } else {
                cv.boxr(x, y, s, s);
            }
        }
        self.setc(false);
    }

    /// click toggle of a 0/1 slot over a small square
    fn sq_tog(&mut self, n: usize, x: f32, y: f32, s: f32) {
        if self.hit(x - 3.0, y - 3.0, s + 6.0, s + 6.0) && self.kit().press && self.kit().act == 0 {
            let v = if self.sget(n) > 0.5 { 0.0 } else { 1.0 };
            self.click_set(n, v);
            self.kit().press = false;
        }
    }
}
