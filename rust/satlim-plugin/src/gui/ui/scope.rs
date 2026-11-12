//! Waveform scope ("transient meter"): half or full wave peak display of the output in the UI colour, with what the
//! clipper and limiter removed stacked on top in the accent colour.

use super::*;

const SRC: [&str; 3] = ["OUT+GR", "OUT", "GR"];
const WAVE: [&str; 2] = ["HALF", "FULL"];
/// time window steps (seconds) and their labels
const WIN_S: [f32; 10] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];
const WIN_L: [&str; 10] = ["250ms", "0.5s", "0.75s", "1s", "1.5s", "2s", "3s", "4s", "6s", "8s"];
/// ceiling steps: +4 dB (room to watch inter-sample overshoots over a limiter's ceiling) down to -14 dB; the default step is +2 dB
const CEIL_N: usize = 19;

fn ceil_db(i: usize) -> i32 {
    4 - i as i32
}

fn fmt_db(db: i32) -> String {
    if db > 0 { format!("+{db}dB") } else { format!("{db}dB") }
}

// Packed settings (params.scope, default (5 << 4) | (2 << 8) = 2 s window, +2 dB ceiling): bit 0 on, bits 1-2 source,
// bit 3 full wave, bits 4-7 window step, bits 8-12 ceiling step (ceil_db).

impl<'a> Gui<'a> {
    fn sc_get(&self, shift: u32, bits: i32) -> usize {
        ((self.scope_cfg >> shift) & bits) as usize
    }

    fn sc_set(&mut self, shift: u32, bits: i32, v: usize) {
        self.scope_cfg = (self.scope_cfg & !(bits << shift)) | ((v as i32 & bits) << shift);
        self.env.params.scope.store(self.scope_cfg, Ordering::Relaxed);
    }

    pub(super) fn draw_scope(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (src, full) = (self.sc_get(1, 3).min(2), self.sc_get(3, 1) == 1);
        let (win, ceil) = (self.sc_get(4, 15).min(WIN_S.len() - 1), self.sc_get(8, 31).min(CEIL_N - 1));
        // viewport drag (relative): x steps through the window, y moves the ceiling (down = lower ceiling = zoomed in)
        if self.hit(x, y, w, h) && self.press && self.act == 0 {
            self.act = 144;
            self.vt1 = 0;
            self.vt2 = 0;
            self.scope_drag = (self.mx, self.my, win, ceil);
            self.press = false;
        }
        let dragging = self.act == 144;
        if dragging && self.down {
            let (sx, sy, w0, c0) = self.scope_drag;
            let nw = (w0 as f32 + (self.mx - sx) / w * (WIN_S.len() - 1) as f32).round().clamp(0.0, (WIN_S.len() - 1) as f32);
            let nc = (c0 as f32 + (self.my - sy) / h * (CEIL_N - 1) as f32).round().clamp(0.0, (CEIL_N - 1) as f32);
            self.sc_set(4, 15, nw as usize);
            self.sc_set(8, 31, nc as usize);
        }
        let (win, ceil) = (self.sc_get(4, 15).min(WIN_S.len() - 1), self.sc_get(8, 31).min(CEIL_N - 1));
        let zoom = 10f32.powf(-ceil_db(ceil) as f32 / 20.0); // 1 / ceiling amplitude
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        let ring = self.env.shared.scope.clone();
        let head = ring.head();
        let ncol = (w - 2.0) as usize;
        let k = WIN_S[win] / satcomp_dsp::ScopeRing::BUCKET_S as f32 / ncol as f32;
        let len = satcomp_dsp::ScopeRing::LEN as i64;
        let mut cols = vec![(0.0f32, 0.0f32); ncol];
        for (c, col) in cols.iter_mut().enumerate() {
            let b0 = head as f32 - (ncol - c) as f32 * k;
            let i0 = b0.floor() as i64;
            let i1 = ((b0 + k).floor() as i64).max(i0 + 1);
            let (mut o, mut p) = (0.0f32, 0.0f32);
            for i in i0..i1 {
                if i < 0 || head as i64 - i >= len {
                    continue;
                }
                let (a, b) = ring.get(i as usize);
                o = o.max(a);
                p = p.max(b);
            }
            *col = (o, p.max(o));
        }
        let span = if full { ((h - 2.0) * 0.5).floor() } else { h - 2.0 };
        let hpx = |v: f32| ((v * zoom).min(1.0) * span).floor();
        let base = if full { y + (h * 0.5).floor() } else { y + h - 1.0 };
        // output (UI colour)
        if src < 2 {
            self.setc(false);
            for (c, &(o, _)) in cols.iter().enumerate() {
                let ho = hpx(o);
                if ho > 0.0 {
                    let cx = x + 1.0 + c as f32;
                    if full {
                        self.cv.fill(cx, base - ho, 1.0, 2.0 * ho + 1.0);
                    } else {
                        self.cv.fill(cx, base - ho, 1.0, ho + 1.0);
                    }
                }
            }
        }
        // reduction (accent): what the compressor took off, stacked on the output (or alone from the baseline)
        if src != 1 {
            self.setc(true);
            for (c, &(o, p)) in cols.iter().enumerate() {
                let cx = x + 1.0 + c as f32;
                let (lo, hi) = if src == 0 { (hpx(o), hpx(p)) } else { (0.0, hpx(p - o)) };
                if hi > lo {
                    if full {
                        self.cv.fill(cx, base - hi, 1.0, hi - lo);
                        self.cv.fill(cx, base + lo + 1.0, 1.0, hi - lo);
                    } else {
                        self.cv.fill(cx, base - hi, 1.0, hi - lo);
                    }
                }
            }
        }
        self.setc(false);
        if full {
            self.cv.fill(x + 1.0, base, 1.0, 1.0); // centre tick
        }
        if zoom < 0.999 {
            // ceiling above 0 dBFS: a faint UI-colour line marks 0 dBFS (the ceiling of a normal signal); anything above it
            // is an overshoot
            let off = (zoom * span).floor();
            let keep = self.dk;
            self.set_dk(keep * 0.35);
            self.setc(false);
            self.cv.fill(x + 1.0, base - off, w - 2.0, 1.0);
            if full {
                self.cv.fill(x + 1.0, base + off, w - 2.0, 1.0);
            }
            self.set_dk(keep);
            self.setc(false);
        }
        if dragging {
            let t = format!("WIN {}  CEIL {}", WIN_L[win], fmt_db(ceil_db(ceil)));
            let tw = self.cv.text_w(&t);
            self.cv.setbg();
            self.cv.fill(x + 4.0, y + 4.0, tw + 8.0, 13.0);
            self.setc(false);
            self.cv.boxr(x + 4.0, y + 4.0, tw + 8.0, 13.0);
            self.cv.set_font(1);
            self.cv.text(x + 8.0, y + 8.0, &t);
        }
    }

    /// Settings under the scope: source and wave type cycle on click; window and ceiling are drag sliders (stepped).
    pub(super) fn scope_row(&mut self, x: f32, y: f32, w_all: f32) {
        let h = 17.0;
        let ceil_w = w_all - 74.0 - 64.0 - 62.0 + 3.0;
        let mut cx = x;
        // SRC and WAVE: click / wheel cycles
        for (lbl, names, shift, bits, w) in [("SRC", &SRC[..], 1u32, 3i32, 74.0f32), ("WAVE", &WAVE[..], 3, 1, 64.0)] {
            let n = names.len();
            let cur = self.sc_get(shift, bits).min(n - 1);
            if self.hit(cx, y, w, h) {
                if self.press && self.act == 0 {
                    self.sc_set(shift, bits, (cur + 1) % n);
                    self.press = false;
                } else if self.wheel != 0 {
                    let nx = (cur as i32 + if self.wheel > 0 { 1 } else { -1 }).clamp(0, n as i32 - 1) as usize;
                    self.sc_set(shift, bits, nx);
                }
            }
            self.setc(false);
            self.cv.boxr(cx, y, w, h);
            self.cv.set_font(1);
            self.cv.text(cx + 4.0, y + 5.0, lbl);
            self.cv.textr(cx, y + 5.0, w - 4.0, names[cur]);
            cx += w - 1.0;
        }
        // WIN and CEIL: horizontal drag sliders over their cell, stepped
        for (k, lbl, shift, bits, n, w) in [(0usize, "WIN", 4u32, 15i32, WIN_S.len(), 62.0f32), (1, "CEIL", 8, 31, CEIL_N, ceil_w)] {
            let act = 142 + k as i32;
            let cur = self.sc_get(shift, bits).min(n - 1);
            if self.hit(cx, y, w, h) {
                if self.press && self.act == 0 {
                    self.act = act;
                    self.vt1 = 0;
                    self.vt2 = 0;
                    self.press = false;
                } else if self.wheel != 0 {
                    let nx = (cur as i32 + if self.wheel > 0 { -1 } else { 1 }).clamp(0, n as i32 - 1) as usize;
                    self.sc_set(shift, bits, nx);
                }
            }
            if self.act == act && self.down {
                let f = ((self.mx - cx - 2.0) / (w - 4.0)).clamp(0.0, 1.0);
                self.sc_set(shift, bits, (f * (n - 1) as f32 + 0.5).floor() as usize);
            }
            let cur = self.sc_get(shift, bits).min(n - 1);
            self.setc(self.act == act);
            self.cv.boxr(cx, y, w, h);
            self.cv.set_font(1);
            self.cv.text(cx + 4.0, y + 5.0, lbl);
            let val = if k == 0 { WIN_L[cur].to_string() } else { fmt_db(ceil_db(cur)) };
            self.cv.textr(cx, y + 5.0, w - 4.0, &val);
            let t = cur as f32 / (n - 1) as f32;
            self.cv.fill(cx + 2.0, y + h - 3.0, ((t * (w - 4.0)).floor()).max(1.0), 1.0);
            self.setc(false);
            cx += w - 1.0;
        }
    }
}
