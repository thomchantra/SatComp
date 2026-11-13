//! The compressor block: peak/RMS detector with optional low cut, feed-forward/feedback blend, optional external key
//! (blended with the main detector level), static gain computer, attack/release ballistics with auto-release, and
//! gain-reduction distortion. The detector listens to one signal and the gain is applied to another, so lookahead (and a
//! parallel dry path) are the caller's business. Stereo, f64.

use crate::biquad::{self, bq, Coef};
use crate::level::{one_pole, DB_AMP, LN10_20};
use crate::DENORM;

/// Static gain computer: returns GR in dB (>= 0) for a level in dB. C1 knee, quad -> smoothstep -> smootherstep.
#[derive(Clone, Copy, Debug)]
pub struct GainComputer {
    pub thr: f64,
    pub knw: f64,
    pub cslope: f64,
    pub kc0: f64,
    pub kc1: f64,
    pub kc2: f64,
}

impl Default for GainComputer {
    fn default() -> Self {
        Self { thr: -18.0, knw: 6.0, cslope: 0.75, kc0: 1.0, kc1: 0.0, kc2: 0.0 }
    }
}

impl GainComputer {
    pub fn from_params(p: &CompParams) -> Self {
        let inv_r = if p.ratio >= 19.9 { 0.0 } else { 1.0 / p.ratio };
        let sh2 = p.knee_shape * 0.02;
        let (kc0, kc1, kc2) = if sh2 <= 1.0 { (1.0 - sh2, sh2, 0.0) } else { (0.0, 2.0 - sh2, sh2 - 1.0) };
        Self { thr: p.thresh, knw: p.knee, cslope: 1.0 - inv_r, kc0, kc1, kc2 }
    }

    #[inline]
    pub fn gr(&self, x: f64) -> f64 {
        let d = x - self.thr;
        if self.knw > 0.001 {
            if d <= -self.knw * 0.5 {
                0.0
            } else if d >= self.knw * 0.5 {
                d * self.cslope
            } else {
                let u = (d + self.knw * 0.5) / self.knw;
                let (u2, u3) = (u * u, u * u * u);
                let u4 = u2 * u2;
                let g = self.kc0 * u2 * 0.5 + self.kc1 * (u3 - u4 * 0.5) + self.kc2 * (u4 * u2 - 3.0 * u4 * u + 2.5 * u4);
                self.cslope * self.knw * g
            }
        } else if d > 0.0 {
            d * self.cslope
        } else {
            0.0
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CompParams {
    /// dB
    pub thresh: f64,
    /// 1..20 (>= 19.9 = inf)
    pub ratio: f64,
    /// ms
    pub attack: f64,
    /// ms (>= 1990 = auto)
    pub release: f64,
    /// 0..100 %
    pub speed: f64,
    /// dB
    pub knee: f64,
    /// 0..100 %
    pub knee_shape: f64,
    /// 0..100 % (peak > rms)
    pub detector: f64,
    /// 0..400 Hz (<= 15 = off)
    pub det_hp: f64,
    /// 0..100 % feed-forward > feedback
    pub ff_fb: f64,
    /// 0..100 %
    pub gr_dist: f64,
    /// 0..100 %: external key vs main detector level when a key is supplied (0 = main only, 100 = key only)
    pub key_mix: f64,
    /// upward: signals BELOW the threshold are lifted (ratio and knee as in the downward mode, mirrored around the threshold).
    /// With `down` off nothing above the threshold is touched; with both on the band lifts below and reduces above the same
    /// threshold. The default (up off, down on) is the ordinary downward compressor, bit-identical.
    pub up: bool,
    /// reduce what is above the threshold (the ordinary compressor); only meaningful together with `up` (without `up` it is
    /// always on)
    pub down: bool,
    /// dB: the most the upward mode lifts
    pub up_range: f64,
}

impl Default for CompParams {
    fn default() -> Self {
        Self {
            thresh: -18.0,
            ratio: 4.0,
            attack: 10.0,
            release: 150.0,
            speed: 50.0,
            knee: 6.0,
            knee_shape: 0.0,
            detector: 30.0,
            det_hp: 0.0,
            ff_fb: 0.0,
            gr_dist: 0.0,
            key_mix: 100.0,
            up: false,
            down: true,
            up_range: 12.0,
        }
    }
}

/// What one `Compressor::tick` produced.
#[derive(Clone, Copy, Debug)]
pub struct CompOut {
    /// the compressed signal (gain applied to the `sig` input, with GR distortion)
    pub y: [f64; 2],
    /// gain reduction per channel, dB (0 in the upward mode)
    pub g: [f64; 2],
    /// gain lift per channel, dB (>= 0, only in the upward mode)
    pub lift: [f64; 2],
    /// loudest detector level (linear)
    pub level: f64,
}

#[derive(Default)]
pub struct Compressor {
    gc: GainComputer,
    a_att: f64,
    auto_rel: bool,
    a_rel_f: f64,
    a_rel_s: f64,
    rms_c: f64,
    pk_d: f64,
    ms_c: f64,
    avg_c: f64,
    det_m: f64,
    use_dhp: bool,
    dhc: Coef,
    dhs: [[f64; 2]; 2],
    fb: f64,
    gdist: f64,
    key_mix: f64,
    prev: [f64; 2],
    rm: [f64; 2],
    rm_k: [f64; 2],
    dhs_k: [[f64; 2]; 2],
    grf: [f64; 2],
    grs: [f64; 2],
    pk_env: f64,
    ms_env: f64,
    gavg: f64,
    up: bool,
    down: bool,
    up_range: f64,
    /// smoothed gain lift per channel (upward mode)
    gl: [f64; 2],
}

impl Compressor {
    pub fn new() -> Self {
        Self { dhc: biquad::IDENTITY, ..Default::default() }
    }

    pub fn gain_computer(&self) -> GainComputer {
        self.gc
    }

    pub fn derive(&mut self, p: &CompParams, sr: f64) {
        if p.up != self.up || p.down != self.down {
            // the two modes keep separate ballistics state: start the new mode from unity
            self.gl = [0.0; 2];
            self.grf = [0.0; 2];
            self.grs = [0.0; 2];
            self.gavg = 0.0;
        }
        self.up = p.up;
        self.down = p.down;
        self.up_range = p.up_range.max(0.0);
        self.gc = GainComputer::from_params(p);
        let spd = 4f64.powf((p.speed - 50.0) / 50.0);
        self.a_att = 1.0 - (-1.0 / (sr * (p.attack * 0.001 * spd).max(0.00001))).exp();
        self.auto_rel = p.release >= 1990.0;
        let a_rel = 1.0 - (-1.0 / (sr * (p.release * 0.001 * spd).max(0.00001))).exp();
        self.a_rel_f = if self.auto_rel { 1.0 - (-1.0 / (sr * 0.04 * spd)).exp() } else { a_rel };
        self.a_rel_s = 1.0 - (-1.0 / (sr * 0.6 * spd)).exp();
        self.rms_c = one_pole(sr, 0.01);
        self.pk_d = (-1.0 / (sr * 0.05)).exp();
        self.ms_c = one_pole(sr, 0.3);
        self.avg_c = one_pole(sr, 0.25);
        self.det_m = p.detector * 0.01;
        self.use_dhp = p.det_hp > 15.0;
        if self.use_dhp {
            self.dhc = biquad::hp(p.det_hp, 0.70710678, sr);
        }
        self.fb = p.ff_fb * 0.01;
        self.key_mix = (p.key_mix * 0.01).clamp(0.0, 1.0);
        self.gdist = p.gr_dist * 0.01;
    }

    /// `det` = what the detector listens to, `sig` = what the gain is applied to (the detector signal delayed by the
    /// lookahead), `key` = external key when one is active (replaces feed-forward/feedback with the key mix).
    #[inline]
    pub fn tick(&mut self, det: [f64; 2], sig: [f64; 2], key: Option<[f64; 2]>) -> CompOut {
        let mut lv = [0.0f64; 2];
        for c in 0..2 {
            // KEY MIX: detector level = (1-m) * main + m * key (levels, not audio, so the two never phase-interact).
            // Without an active key m = 0. With one, FF>FB is not applied (it is replaced by KEY MIX).
            let km = if key.is_some() { self.key_mix } else { 0.0 };
            let mut l = 0.0;
            if km < 1.0 {
                let xd = if key.is_some() { det[c] } else { det[c] + self.fb * (self.prev[c] - det[c]) };
                l += (1.0 - km) * det_level(xd, &mut self.rm[c], &mut self.dhs[c], self.use_dhp, &self.dhc, self.det_m, self.rms_c);
            }
            if km > 0.0 {
                let k = key.unwrap_or([0.0; 2]);
                l += km * det_level(k[c], &mut self.rm_k[c], &mut self.dhs_k[c], self.use_dhp, &self.dhc, self.det_m, self.rms_c);
            }
            lv[c] = l;
        }
        let lmx = lv[0].max(lv[1]);

        // Upward part: the gain computer mirrored around the threshold gives the lift for a level below it (same ratio, same
        // knee), capped at the range. The lift rises when the level falls (release ballistics) and is pulled back quickly
        // when it rises (attack), so loud transients are never boosted.
        let mut lift = [0.0f64; 2];
        if self.up {
            for c in 0..2 {
                let x = DB_AMP * lv[c].max(0.000001).ln();
                let tl = self.gc.gr(2.0 * self.gc.thr - x).min(self.up_range);
                let k = if tl < self.gl[c] {
                    self.a_att
                } else if self.auto_rel {
                    self.a_rel_s
                } else {
                    self.a_rel_f
                };
                self.gl[c] += (tl - self.gl[c]) * k;
                if self.gl[c] < 0.000000001 {
                    self.gl[c] = 0.0;
                }
                lift[c] = self.gl[c];
            }
            if !self.down {
                let y = [sig[0] * (lift[0] * LN10_20).exp(), sig[1] * (lift[1] * LN10_20).exp()];
                self.prev = y;
                return CompOut { y, g: [0.0; 2], lift, level: lmx };
            }
        }

        let mut tg = [0.0f64; 2];
        for c in 0..2 {
            tg[c] = self.gc.gr(DB_AMP * lv[c].max(0.000001).ln());
        }
        let mut g = [0.0f64; 2];
        for c in 0..2 {
            let k = if tg[c] > self.grf[c] { self.a_att } else { self.a_rel_f };
            self.grf[c] += (tg[c] - self.grf[c]) * k;
            if self.grf[c] < 0.000000001 {
                self.grf[c] = 0.0;
            }
        }
        if self.auto_rel {
            for c in 0..2 {
                let k = if tg[c] > self.grs[c] { self.a_att } else { self.a_rel_s };
                self.grs[c] += (tg[c] - self.grs[c]) * k;
                if self.grs[c] < 0.000000001 {
                    self.grs[c] = 0.0;
                }
            }
            self.pk_env = lmx.max(self.pk_env * self.pk_d);
            self.ms_env += (lmx * lmx + DENORM - self.ms_env) * self.ms_c;
            let crest = self.pk_env / (self.ms_env.sqrt() + 0.000000001);
            let cn = ((crest - 1.5) * 0.2857).clamp(0.0, 1.0);
            self.gavg += (self.grf[0].max(self.grf[1]) - self.gavg) * self.avg_c;
            let wgt = 0.1 + 0.75 * (self.gavg * 0.1111).min(1.0) * (1.0 - cn);
            for c in 0..2 {
                g[c] = self.grf[c] + wgt * (self.grs[c] - self.grf[c]);
            }
        } else {
            g = self.grf;
        }

        let mut y = [sig[0] * ((lift[0] - g[0]) * LN10_20).exp(), sig[1] * ((lift[1] - g[1]) * LN10_20).exp()];
        if self.gdist > 0.0 {
            for c in 0..2 {
                let dd = self.gdist * (g[c] * 0.0833333).min(1.0);
                y[c] += dd * (y[c] / (1.0 + 0.5 * y[c].abs()) - y[c]);
            }
        }
        self.prev = y;
        CompOut { y, g, lift, level: lmx }
    }
}

/// Detector level of one source: optional low cut, then peak/RMS blend.
#[inline]
fn det_level(mut xd: f64, rm: &mut f64, hs: &mut [f64; 2], use_hp: bool, hc: &Coef, det_m: f64, rms_c: f64) -> f64 {
    if use_hp {
        xd = bq(hs, hc, xd);
    }
    let a = xd.abs();
    *rm += (xd * xd + DENORM - *rm) * rms_c;
    if det_m > 0.0 {
        a + det_m * (1.41421356 * rm.sqrt() - a)
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    /// run a 1 kHz tone of `amp` through the compressor for `secs`, return (output rms ratio to input, last lift, last gr)
    fn run(p: &CompParams, amp: f64, secs: f64) -> (f64, f64, f64) {
        let mut c = Compressor::new();
        c.derive(p, SR);
        let n = (SR * secs) as usize;
        let (mut si, mut so) = (0.0, 0.0);
        let (mut lift, mut gr) = (0.0, 0.0);
        for i in 0..n {
            let x = amp * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / SR).sin();
            let o = c.tick([x, x], [x, x], None);
            if i > n / 2 {
                si += x * x;
                so += o.y[0] * o.y[0];
            }
            lift = o.lift[0];
            gr = o.g[0];
        }
        ((so / si).sqrt(), lift, gr)
    }

    fn base() -> CompParams {
        CompParams { thresh: -30.0, ratio: 2.0, knee: 0.0, attack: 5.0, release: 50.0, detector: 100.0, ..CompParams::default() }
    }

    #[test]
    fn downward_mode_is_unchanged_and_reports_no_lift() {
        let (_, lift, gr) = run(&base(), 0.5, 1.0); // -6 dB, well above -30
        assert_eq!(lift, 0.0);
        assert!(gr > 5.0, "downward still reduces: {gr}");
    }

    #[test]
    fn upward_mode_lifts_a_quiet_signal_up_to_the_range_and_leaves_a_loud_one_alone() {
        let p = CompParams { up: true, down: false, up_range: 12.0, ..base() };
        // -50 dB tone: 20 dB under the threshold, ratio 2 would lift 10 dB (under the 12 dB range)
        let (ratio, lift, gr) = run(&p, 0.00316, 1.0);
        assert!((lift - 10.0).abs() < 1.0, "lift {lift}");
        assert!((20.0 * ratio.log10() - lift).abs() < 1.0, "output follows the lift: {}", 20.0 * ratio.log10());
        assert_eq!(gr, 0.0);
        // -70 dB: would be 20 dB, capped at the 12 dB range
        let (_, lift, _) = run(&p, 0.000316, 1.0);
        assert!((lift - 12.0).abs() < 0.5, "capped at the range: {lift}");
        // -6 dB (above the threshold): untouched
        let (ratio, lift, gr) = run(&p, 0.5, 1.0);
        assert!(lift < 0.01 && gr == 0.0 && (ratio - 1.0).abs() < 0.01, "loud signal untouched: lift {lift} ratio {ratio}");
    }

    #[test]
    fn both_mode_lifts_below_and_reduces_above_the_same_threshold() {
        let p = CompParams { up: true, down: true, up_range: 12.0, ..base() };
        let (ratio, lift, gr) = run(&p, 0.00316, 1.0); // -50 dB: lifted
        assert!((lift - 10.0).abs() < 1.0 && gr == 0.0, "quiet: lift {lift} gr {gr}");
        assert!((20.0 * ratio.log10() - lift).abs() < 1.0);
        let (ratio, lift, gr) = run(&p, 0.5, 1.0); // -6 dB: reduced like the plain compressor
        let (plain, _, plain_gr) = run(&base(), 0.5, 1.0);
        assert!(lift < 0.01, "no lift up there: {lift}");
        assert!((gr - plain_gr).abs() < 0.2 && (ratio - plain).abs() < 0.01, "same reduction as downward: {gr} vs {plain_gr}");
    }

    #[test]
    fn upward_lift_gives_way_quickly_when_the_level_rises() {
        let p = CompParams { up: true, down: false, up_range: 12.0, attack: 1.0, release: 400.0, ..base() };
        let mut c = Compressor::new();
        c.derive(&p, SR);
        let tone = |i: usize, a: f64| a * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / SR).sin();
        let mut lift = 0.0;
        for i in 0..(SR * 2.0) as usize {
            let x = tone(i, 0.001); // -60 dB: lifts to the range
            lift = c.tick([x, x], [x, x], None).lift[0];
        }
        assert!(lift > 11.0, "{lift}");
        let mut after = lift;
        for i in 0..(SR * 0.02) as usize {
            let x = tone(i, 0.5); // a loud hit: the lift must collapse within 20 ms (attack 1 ms)
            after = c.tick([x, x], [x, x], None).lift[0];
        }
        assert!(after < 1.0, "lift after a loud hit: {after}");
    }
}
