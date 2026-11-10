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
        }
    }
}

/// What one `Compressor::tick` produced.
#[derive(Clone, Copy, Debug)]
pub struct CompOut {
    /// the compressed signal (gain applied to the `sig` input, with GR distortion)
    pub y: [f64; 2],
    /// gain reduction per channel, dB
    pub g: [f64; 2],
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
}

impl Compressor {
    pub fn new() -> Self {
        Self { dhc: biquad::IDENTITY, ..Default::default() }
    }

    pub fn gain_computer(&self) -> GainComputer {
        self.gc
    }

    pub fn derive(&mut self, p: &CompParams, sr: f64) {
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

        let mut y = [sig[0] * (-g[0] * LN10_20).exp(), sig[1] * (-g[1] * LN10_20).exp()];
        if self.gdist > 0.0 {
            for c in 0..2 {
                let dd = self.gdist * (g[c] * 0.0833333).min(1.0);
                y[c] += dd * (y[c] / (1.0 + 0.5 * y[c].abs()) - y[c]);
            }
        }
        self.prev = y;
        CompOut { y, g, level: lmx }
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
