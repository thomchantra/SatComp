//! The compressor plot (transfer curve + live level dot). Both axes show the same dB range [floor, ceiling]. The ceiling can sit
//! above 0 dBFS so a clipped / overshooting signal is visible (a dashed line marks 0 dB), and the floor can go below -48.
//! Two small labels in the lower right (CEIL, FLOOR) are drag / wheel sliders for the range. The ratio and threshold can be
//! grabbed on the curve itself: the line above the threshold drags the ratio, the line below it drags the threshold.

use super::*;

/// floor steps: -12 dB (step 0) down to -72 dB in 6 dB steps; default step 6 = -48 dB
const FLOOR_N: usize = 11;
/// ceiling steps: 0 dB up to +18 dB in 3 dB steps; default step 2 = +6 dB
const CEIL_N: usize = 7;

fn floor_db(i: usize) -> f32 {
    -(12.0 + 6.0 * i as f32)
}

fn ceil_db(i: usize) -> f32 {
    3.0 * i as f32
}

/// "no signal" level: below any floor, so the dot and fill are not drawn and the readout shows dashes
const SILENT: f32 = -95.9;

fn db_str(v: f32) -> String {
    if v > 0.5 { format!("+{}", v as i32) } else { format!("{}", v as i32) }
}

/// dashed 1 px line (3 on, 3 off); `horiz`: from (x, y) for `len` px to the right, otherwise down
fn dashed(cv: &Canvas, x: f32, y: f32, len: f32, horiz: bool) {
    let mut t = 0.0;
    while t < len {
        let l = 3.0f32.min(len - t);
        if horiz {
            cv.fill(x + t, y, l, 1.0);
        } else {
            cv.fill(x, y + t, 1.0, l);
        }
        t += 6.0;
    }
}

impl<'a> Gui<'a> {
    fn cc_get(&self, shift: u32) -> usize {
        ((self.cc_cfg >> shift) & 15) as usize
    }

    fn cc_set(&mut self, shift: u32, v: usize) {
        self.cc_cfg = (self.cc_cfg & !(15 << shift)) | ((v as i32 & 15) << shift);
        self.env.params.ccurve.store(self.cc_cfg, Ordering::Relaxed);
    }

    pub(super) fn draw_ccurve(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (fi, ci) = (self.cc_get(0).min(FLOOR_N - 1), self.cc_get(4).min(CEIL_N - 1));
        let (lo, hi) = (floor_db(fi), ceil_db(ci));

        // ---- floor / ceiling labels (lower right): boxed so they stay legible over the fill
        self.cv.set_font(1);
        let ct = format!("CEIL {}", db_str(hi));
        let ft = format!("FLOOR {}", db_str(lo));
        let (cw, fw) = (self.cv.text_w(&ct) + 8.0, self.cv.text_w(&ft) + 8.0);
        let (by, bh) = (y + h - 15.0, 12.0);
        let fx = x + w - 3.0 - fw;
        let cx = fx - 3.0 - cw;
        for (k, bx, bw) in [(0usize, cx, cw), (1, fx, fw)] {
            let act = 145 + k as i32;
            let (shift, n) = if k == 0 { (4u32, CEIL_N) } else { (0, FLOOR_N) };
            let cur = if k == 0 { ci } else { fi };
            if self.hit(bx, by, bw, bh) {
                if self.press && self.act == 0 {
                    if self.dbl(act) {
                        self.cc_set(shift, if k == 0 { 2 } else { 6 });
                    } else {
                        self.act = act;
                        self.vt1 = 0;
                        self.vt2 = 0;
                        self.cc_drag = (self.my, cur);
                    }
                    self.press = false;
                } else if self.wheel != 0 {
                    let nx = (cur as i32 + if self.wheel > 0 { 1 } else { -1 }).clamp(0, n as i32 - 1) as usize;
                    self.cc_set(shift, nx);
                }
            }
            if self.act == act && self.down {
                // drag up = higher value (a higher ceiling, a floor closer to 0), 6 px per step
                let d = ((self.cc_drag.0 - self.my) / 6.0).round() as i32;
                self.cc_set(shift, (self.cc_drag.1 as i32 + d).clamp(0, n as i32 - 1) as usize);
            }
        }
        // the range may have just changed: recompute with the final values
        let (fi, ci) = (self.cc_get(0).min(FLOOR_N - 1), self.cc_get(4).min(CEIL_N - 1));
        let (lo, hi) = (floor_db(fi), ceil_db(ci));
        let span = hi - lo;
        let vx = |db: f32| x + (db - lo) / span * (w - 1.0);
        let vy = |db: f32| y + (1.0 - (db - lo) / span) * (h - 1.0);
        let mx_db = |px: f32| lo + (px - x) / (w - 1.0) * span;
        let my_db = |py: f32| lo + (1.0 - (py - y) / (h - 1.0)) * span;
        let vyc = |db: f32| vy(db).clamp(y, y + h - 1.0);

        // ---- threshold / ratio
        let gc = GainComputer::from_params(&self.env.dsp.comp());
        let hv = self.hit(x, y, w, h);
        let thr = self.sget(8);
        if self.act == 120 && self.down {
            self.setv(8, ((mx_db(self.mx) + my_db(self.my)) * 0.5).clamp(lo, hi.min(0.0)));
        }
        if self.act == 121 && self.down {
            let (xin, yo) = (mx_db(self.mx), my_db(self.my).max(thr).min(hi));
            if xin > thr + 0.3 {
                self.setv(9, (xin - thr) / (yo - thr).max(0.05));
            }
        }
        let thr = self.sget(8);
        let gc = if self.act == 120 || self.act == 121 {
            self.refresh_dsp();
            GainComputer::from_params(&self.env.dsp.comp())
        } else {
            gc
        };
        let hxt = vx(thr);
        let hyt = vy(thr);
        // the ratio handle sits on the curve a little below the ceiling (never under the scope icon in the corner)
        let hin = (hi - 6.0).min(0.0);
        let hxr = vx(hin);
        let hyr = vyc(hin - gc.gr(hin as f64) as f32);
        let on_lbl = self.hit(cx, by, cw, bh) || self.hit(fx, by, fw, bh);
        if hv && self.press && self.act == 0 && !on_lbl {
            // an interaction point: the handles first, then the curve itself (above the threshold = ratio, below = threshold)
            let xin = mx_db(self.mx);
            let curve_y = vyc(xin - gc.gr(xin as f64) as f32);
            let pick = if (self.mx - hxr).abs() <= 6.0 && (self.my - hyr).abs() <= 6.0 {
                121
            } else if (self.mx - hxt).abs() <= 6.0 && (self.my - hyt).abs() <= 6.0 {
                120
            } else if (self.my - curve_y).abs() <= 4.0 {
                if xin >= thr { 121 } else { 120 }
            } else {
                0
            };
            if pick != 0 {
                let n = if pick == 121 { 9 } else { 8 };
                if self.dbl(pick) {
                    let d = self.meta[n].def;
                    self.click_set(n, d);
                } else {
                    self.act = pick;
                    self.vt1 = n;
                    self.vt2 = 0;
                }
                self.press = false;
            }
        }

        // ---- plot
        self.setc(false);
        self.cv.boxr(x, y, w, h);
        let vz = self.mval(1);
        let lvl = if vz > 0.000001 { (8.6858896 * vz.ln()).clamp(-96.0, hi) } else { -96.0 };
        let live = lvl > SILENT;
        self.setc(true);
        for i in 0..(w - 2.0) as i32 {
            let xin = lo + span * i as f32 / (w - 3.0);
            if live && xin <= lvl {
                let py = vyc(xin - gc.gr(xin as f64) as f32);
                self.cv.fill(x + 1.0 + i as f32, py, 1.0, y + h - 1.0 - py);
            }
        }
        self.setc(false);
        for i in 0..(w / 3.0).floor() as i32 {
            self.cv.fill(x + i as f32 * 3.0 + 1.0, y + h - 1.0 - ((i * 3) as f32 * (h - 1.0) / (w - 1.0)).floor(), 1.0, 1.0);
        }
        // dB legends (dim): ticks and numbers every 12 dB along the left and bottom edges, skipping what the readout, the
        // floor / ceiling labels and the other axis would overlap
        let keep = self.dk;
        self.set_dk(keep * 0.55);
        self.setc(false);
        self.cv.set_font(1);
        let mut db = (hi / 12.0).floor() * 12.0;
        while db > lo + 3.0 {
            let s = db_str(db);
            let tw = self.cv.text_w(&s);
            let (px, py) = (vx(db), vy(db));
            self.cv.fill(x + 1.0, py, 3.0, 1.0); // tick on the left edge
            if py + 2.0 >= y + 13.0 && py + 11.0 < y + h - 12.0 {
                self.cv.text(x + 6.0, py + 2.0, &s);
            }
            self.cv.fill(px, y + h - 4.0, 1.0, 3.0); // tick on the bottom edge
            let tx = (px - tw * 0.5).max(x + 3.0);
            if tx > x + 22.0 && tx + tw < cx - 3.0 {
                self.cv.text(tx, y + h - 13.0, &s);
            }
            db -= 12.0;
        }
        self.set_dk(keep);
        self.setc(false);
        // 0 dB reference (dashed), when the ceiling is above it: anything past these lines is an overshoot
        if hi > 0.5 {
            dashed(&self.cv, x + 1.0, vy(0.0), w - 2.0, true);
            dashed(&self.cv, vx(0.0), y + 1.0, h - 2.0, false);
        }
        let n = 96;
        let (mut lx, mut ly) = (0.0, 0.0);
        for i in 0..=n {
            let xin = lo + span * i as f32 / n as f32;
            let px = x + (i as f32 / n as f32) * (w - 1.0);
            let py = vyc(xin - gc.gr(xin as f64) as f32);
            if i > 0 {
                self.cv.line(lx, ly, px, py);
            }
            lx = px;
            ly = py;
        }
        if live {
            let (px, py) = (vx(lvl), vyc(lvl - gc.gr(lvl as f64) as f32));
            self.cv.fill(px - 1.0, py - 1.0, 3.0, 3.0);
        }
        // threshold: a dashed line from its handle straight down
        if hyt < y + h - 2.0 {
            dashed(&self.cv, hxt, hyt.max(y), y + h - 1.0 - hyt.max(y), false);
        }
        // sidechain: the plot's x axis is the KEY level. Markers on the side edges show the main signal: left = level
        // entering the compressor, right = level after the gain reduction (the gap between them is the GR).
        let sc_mode = self.sget(63) > 0.5;
        let mn = self.mval(2);
        let mn_db = if mn > 0.000001 { (8.6858896 * mn.ln()).clamp(-96.0, hi) } else { -96.0 };
        if sc_mode && mn_db > SILENT {
            let gr_now = if live { gc.gr(lvl as f64) as f32 } else { 0.0 };
            let yin = vyc(mn_db);
            let yout = vyc((mn_db - gr_now).max(lo));
            self.cv.tri((x + 1.0, yin - 3.0), (x + 1.0, yin + 4.0), (x + 6.0, yin + 0.5));
            self.cv.tri((x + w - 2.0, yout - 3.0), (x + w - 2.0, yout + 4.0), (x + w - 7.0, yout + 0.5));
        }
        // handles
        self.setc(self.act == 120);
        self.cv.disc(hxt, hyt.clamp(y, y + h - 1.0), 3.5);
        self.setc(self.act == 121);
        self.cv.fill(hxr - 3.0, hyr - 3.0, 7.0, 7.0);
        // floor / ceiling labels
        for (k, bx, bw, t) in [(0usize, cx, cw, &ct), (1, fx, fw, &ft)] {
            let act = 145 + k as i32;
            self.cv.setbg();
            self.cv.fill(bx, by, bw, bh);
            self.setc(self.act == act);
            self.cv.boxr(bx, by, bw, bh);
            self.cv.set_font(1);
            self.cv.text(bx + 4.0, by + 3.0, t);
        }
        self.setc(false);
        self.cv.set_font(1);
        if self.rd_tick {
            self.rd_lvl = lvl;
            self.rd_ghr = if live { gc.gr(lvl as f64) as f32 } else { 0.0 };
        }
        let shown = self.rd_lvl > -90.0;
        let s = if sc_mode {
            let mn = if mn_db > -90.0 { format!("{:+05.1}", mn_db) } else { "-----".to_string() };
            if shown {
                format!("SC {:+05.1}  IN {}  GR {:04.1}dB", self.rd_lvl, mn, self.rd_ghr)
            } else {
                format!("SC -----  IN {}  GR 00.0dB", mn)
            }
        } else if shown {
            format!("LVL {:+05.1}  OUT {:+05.1}  GR {:04.1}dB", self.rd_lvl, self.rd_lvl - self.rd_ghr, self.rd_ghr)
        } else {
            "LVL -----  OUT -----  GR 00.0dB".to_string()
        };
        self.cv.text(x + 3.0, y + 2.0, &s);
    }
}
