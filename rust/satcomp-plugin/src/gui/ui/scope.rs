//! Waveform scope ("transient meter"): an alternate view of the compressor plot. Half or full wave peak display of the
//! output in the UI colour, with the gain reduction the compressor removed stacked on top in the accent colour.

use super::*;

const SRC: [&str; 3] = ["OUT+RED", "OUT", "RED"];
const WAVE: [&str; 2] = ["HALF", "FULL"];
const WIN: [&str; 4] = ["0.5s", "1s", "2s", "4s"];
const WIN_S: [f32; 4] = [0.5, 1.0, 2.0, 4.0];
const ZOOM: [&str; 4] = ["x1", "x2", "x4", "x8"];
const ZOOM_F: [f32; 4] = [1.0, 2.0, 4.0, 8.0];

/// Packed settings: bit 0 on, bits 1-2 source, bit 3 full wave, bits 4-5 window, bits 6-7 zoom.
pub const SCOPE_DEFAULT: i32 = 2 << 4;

impl State {
    pub fn scope_on(&self) -> bool {
        self.scope_cfg & 1 != 0 && self.edit.is_none()
    }
}

impl<'a> Gui<'a> {
    fn sc_get(&self, shift: u32, bits: i32) -> usize {
        ((self.scope_cfg >> shift) & bits) as usize
    }

    fn sc_set(&mut self, shift: u32, bits: i32, v: usize) {
        self.scope_cfg = (self.scope_cfg & !(bits << shift)) | ((v as i32 & bits) << shift);
        self.env.params.scope.store(self.scope_cfg, Ordering::Relaxed);
    }

    /// Toggle icon in the top right corner of the compressor plot (draw = false: handle the click only).
    pub(super) fn scope_icon(&mut self, draw: bool) {
        let (x, y) = (152.0 + 266.0 - 16.0, 56.0 + 3.0);
        let on = self.scope_cfg & 1 != 0;
        if !draw {
            if self.hit(x, y, 13.0, 12.0) && self.press && self.act == 0 {
                self.sc_set(0, 1, !on as usize);
                self.press = false;
            }
            return;
        }
        // 3 bars of different heights, like a tiny waveform
        self.setc(false);
        if on {
            self.cv.fill(x, y, 13.0, 12.0);
            self.cv.setbg();
        } else {
            self.cv.setbg();
            self.cv.fill(x, y, 13.0, 12.0);
            self.setc(false);
            self.cv.boxr(x, y, 13.0, 12.0);
        }
        for (i, bh) in [3.0f32, 7.0, 4.0, 6.0].iter().enumerate() {
            self.cv.fill(x + 2.0 + i as f32 * 3.0, y + 10.0 - bh, 2.0, *bh);
        }
        self.setc(false);
    }

    pub(super) fn draw_scope(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (src, full) = (self.sc_get(1, 3).min(2), self.sc_get(3, 1) == 1);
        let (win, zoom) = (self.sc_get(4, 3), ZOOM_F[self.sc_get(6, 3)]);
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
    }

    /// Settings under the scope: source, half / full wave, time window, vertical zoom. Click or wheel cycles a value.
    pub(super) fn scope_row(&mut self, x: f32, y: f32) {
        let h = 17.0;
        let cells: [(&str, &[&str], u32, i32, f32); 4] = [
            ("SRC", &SRC, 1, 3, 78.0),
            ("WAVE", &WAVE, 3, 1, 68.0),
            ("WIN", &WIN, 4, 3, 58.0),
            ("ZOOM", &ZOOM, 6, 3, 266.0 - 78.0 - 68.0 - 58.0 + 3.0),
        ];
        let mut cx = x;
        for (lbl, names, shift, bits, w) in cells {
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
    }
}
