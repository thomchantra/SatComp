//! The saturator block: optional HP/LP split (only the middle band is shaped, the rest is added back), drive + bias into a
//! waveshaper LUT (optionally 2x/4x oversampled), DC blocker, tone low-pass, dry/wet mix, then SAT OUT gain.
//! Reports its latency and a smoothed output mean-square level (for the level match). Stereo, f64.

use crate::biquad::{self, bq, Coef};
use crate::delay::StereoDelay;
use crate::halfband::{Oversampler, Taps};
use crate::level::{ms_step, one_pole, LN10_20};
use crate::shapers::{Curve, ShaperLut};
use crate::DENORM;
use std::f64::consts::PI;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SatParams {
    pub shape: i32,
    /// user curve, used when `shape` is the custom shaper
    pub curve: Curve,
    /// 0..100 %
    pub drive: f64,
    /// -100..100 %
    pub bias: f64,
    /// 20..2000 Hz (<= 20.5 = off)
    pub hp: f64,
    /// 200..20000 Hz (>= 19900 = off)
    pub lp: f64,
    /// 1000..20000 Hz (>= 19900 = off)
    pub tone: f64,
    /// 0 off, 1 x2, 2 x4 (3 = x8, SatLim only)
    pub os: i32,
    /// 0..100 %
    pub mix: f64,
    /// dB
    pub out_db: f64,
    /// Even / odd split of the shaper: `None` = the plain shaper (legacy). `Some(a)`, a in 0..100 %: output = odd part +
    /// a * (even part high-passed at `EVEN_HP_HZ`). The even part of an asymmetric curve is what carries DC and
    /// envelope-following sub-audio pulses (they sink the waveform into the headroom); 0 % drops it altogether.
    pub asym: Option<f64>,
    /// HP / LP as 48 dB/oct Butterworth (8th order, flat passband, no corner peak) instead of the legacy 12 dB biquad.
    pub steep: bool,
}

/// Section Qs of an 8th-order Butterworth (48 dB/oct): maximally flat, the high-Q sections cancel each other's peaks.
const BUTTER8_Q: [f64; 4] = [0.509_795_579_104_159_3, 0.601_344_886_935_045_2, 0.899_976_223_136_415_6, 2.562_915_447_741_505];

/// Corner of the high-pass on the even part of the shaper (2nd order), Hz.
pub const EVEN_HP_HZ: f64 = 25.0;

impl Default for SatParams {
    fn default() -> Self {
        Self { shape: 0, curve: Curve::linear(), drive: 25.0, bias: 0.0, hp: 20.0, lp: 20000.0, tone: 20000.0, os: 0, mix: 100.0, out_db: 0.0, asym: None, steep: false }
    }
}

pub struct Saturator {
    lut: ShaperLut,
    lut_cnt: u32,
    taps: Taps,
    ov: [Oversampler; 2],
    /// down-filter state of the even part (see `Oversampler::process_fe`)
    ove: [Oversampler; 2],
    asym_k: Option<f64>,
    evc: Coef,
    evs: [[f64; 2]; 2],
    pre: f64,
    pre_t: f64,
    bias: f64,
    bias_t: f64,
    on: bool,
    use_hp: bool,
    use_lp: bool,
    use_tone: bool,
    hpc: [Coef; 4],
    lpc: [Coef; 4],
    /// sections used by the HP / LP: 1 (12 dB) or 4 (48 dB)
    n_filt: usize,
    tnc: Coef,
    hps: [[[f64; 2]; 4]; 2],
    lps: [[[f64; 2]; 4]; 2],
    tns: [[f64; 2]; 2],
    os: i32,
    last_os: i32,
    lat: usize,
    dc_r: f64,
    dcx: [f64; 2],
    dcy: [f64; 2],
    /// True until the first saturated sample: a fresh engine (REAPER rebuilds it on every FX enable) must start the
    /// oversampler and DC blocker in their settled state, else the saturator's constant output (bias) leaks as a step.
    dc_warm: bool,
    dry: StereoDelay<64>,
    res: StereoDelay<64>,
    band: StereoDelay<64>,
    mix: f64,
    mix_t: f64,
    gain: f64,
    gain_t: f64,
    out_db: f64,
    sm_c: f64,
    gain_c: f64,
    ms_c: f64,
    /// Smoothed mean-square level of the stage output before SAT OUT (reference for the sat level-match).
    pub ms: f64,
    /// Peak of the shaped band since the owner last cleared it (UI meter).
    pub blk: f64,
}

impl Default for Saturator {
    fn default() -> Self {
        Self::new()
    }
}

impl Saturator {
    pub fn new() -> Self {
        Self {
            lut: ShaperLut::new(0),
            lut_cnt: 0,
            taps: Taps::new(),
            ov: [Oversampler::default(), Oversampler::default()],
            ove: [Oversampler::default(), Oversampler::default()],
            asym_k: None,
            evc: biquad::IDENTITY,
            evs: [[0.0; 2]; 2],
            pre: 1.0,
            pre_t: 1.0,
            bias: 0.0,
            bias_t: 0.0,
            on: false,
            use_hp: false,
            use_lp: false,
            use_tone: false,
            hpc: [biquad::IDENTITY; 4],
            lpc: [biquad::IDENTITY; 4],
            n_filt: 1,
            tnc: biquad::IDENTITY,
            hps: [[[0.0; 2]; 4]; 2],
            lps: [[[0.0; 2]; 4]; 2],
            tns: [[0.0; 2]; 2],
            os: 0,
            last_os: -1,
            lat: 0,
            dc_r: 1.0,
            dcx: [0.0; 2],
            dcy: [0.0; 2],
            dc_warm: true,
            dry: StereoDelay::new(),
            res: StereoDelay::new(),
            band: StereoDelay::new(),
            mix: 1.0,
            mix_t: 1.0,
            gain: 1.0,
            gain_t: 1.0,
            out_db: 0.0,
            sm_c: 0.0,
            gain_c: 0.0,
            ms_c: 0.0,
            ms: 1e-9,
            blk: 0.0,
        }
    }

    /// PDC latency of the oversampler, samples.
    pub fn latency(&self) -> usize {
        self.lat
    }

    /// Effective SAT OUT in dB (the parameter, or a finished level match).
    pub fn out_db(&self) -> f64 {
        self.out_db
    }

    pub fn set_out_db(&mut self, db: f64) {
        self.out_db = db;
    }

    /// `snap` jumps the smoothed drive / bias / mix / out gain to their targets. `smoothing` false also commits the shaper
    /// table at once (no morph) and disables the per-sample glide.
    pub fn derive(&mut self, p: &SatParams, sr: f64, snap: bool, smoothing: bool) {
        self.lut.fade = smoothing;
        self.lut.sync(p.shape, &p.curve);
        self.pre_t = (p.drive * 0.01 * 36.0 * LN10_20).exp();
        self.bias_t = p.bias * 0.01;
        self.on = p.drive > 0.0 || p.bias != 0.0;
        self.use_hp = p.hp > 20.5;
        self.n_filt = if p.steep { 4 } else { 1 };
        if self.use_hp {
            for k in 0..self.n_filt {
                self.hpc[k] = biquad::hp(p.hp, if p.steep { BUTTER8_Q[k] } else { 0.70710678 }, sr);
            }
        }
        self.use_lp = p.lp < 19900.0;
        if self.use_lp {
            for k in 0..self.n_filt {
                self.lpc[k] = biquad::lp(p.lp, if p.steep { BUTTER8_Q[k] } else { 0.70710678 }, sr);
            }
        }
        self.use_tone = p.tone < 19900.0;
        if self.use_tone {
            self.tnc = biquad::lp(p.tone, 0.70710678, sr);
        }
        self.asym_k = p.asym.map(|a| (a * 0.01).clamp(0.0, 1.0));
        if self.asym_k.is_some() {
            self.evc = biquad::hp(EVEN_HP_HZ, 0.70710678, sr);
        }
        self.os = (p.os as f64 + 0.5).floor() as i32;
        if self.os != self.last_os {
            self.last_os = self.os;
            self.ov[0].clear();
            self.ov[1].clear();
            self.ove[0].clear();
            self.ove[1].clear();
            self.res.clear();
        }
        self.lat = crate::halfband::latency(self.os);
        self.dc_r = 1.0 - 2.0 * PI * 5.0 / sr;
        self.mix_t = p.mix * 0.01;
        self.gain_c = one_pole(sr, 0.02);
        self.sm_c = one_pole(sr, 0.02);
        self.ms_c = one_pole(sr, 0.5);
        if snap {
            self.pre = self.pre_t;
            self.bias = self.bias_t;
            self.mix = self.mix_t;
            self.out_db = p.out_db;
            self.gain = (self.out_db * LN10_20).exp();
            self.gain_t = self.gain;
        }
    }

    /// Per host block.
    pub fn begin_block(&mut self) {
        self.gain_t = (self.out_db * LN10_20).exp();
    }

    /// One stereo sample in -> shaped stereo sample out (after SAT OUT), delayed by `latency()`.
    #[inline]
    pub fn tick(&mut self, inp: [f64; 2], smoothing: bool) -> [f64; 2] {
        let (cc, _) = self.tick_split(inp, smoothing);
        let g = self.gain_step();
        [cc[0] * g, cc[1] * g]
    }

    /// The SAT OUT gain for this sample (smoothed). `tick` = `tick_split` then this.
    #[inline]
    pub fn gain_step(&mut self) -> f64 {
        self.gain += (self.gain_t - self.gain) * self.gain_c;
        self.gain
    }

    /// As `tick`, but before SAT OUT and also returning the clean (unsaturated) input delayed by `latency()`:
    /// `(shaped, clean)`. Their difference is the content the saturator added (the owner may filter it).
    #[inline]
    pub fn tick_split(&mut self, inp: [f64; 2], smoothing: bool) -> ([f64; 2], [f64; 2]) {
        self.lut_cnt += 1;
        if self.lut_cnt >= crate::shapers::FADE_EVERY {
            self.lut_cnt = 0;
            self.lut.step();
        }
        if smoothing {
            let c = self.sm_c;
            self.pre += (self.pre_t - self.pre) * c;
            self.bias += (self.bias_t - self.bias) * c;
            self.mix += (self.mix_t - self.mix) * c;
        } else {
            self.pre = self.pre_t;
            self.bias = self.bias_t;
            self.mix = self.mix_t;
        }

        self.dry.advance();
        self.res.advance();
        self.band.advance();
        self.dry.put(inp);
        let dl = self.dry.tap(self.lat);

        let mut cc = [0.0f64; 2];
        if self.on {
            let mut bp = inp;
            if self.use_hp {
                for c in 0..2 {
                    for k in 0..self.n_filt {
                        bp[c] = bq(&mut self.hps[c][k], &self.hpc[k], bp[c]);
                    }
                }
            }
            if self.use_lp {
                for c in 0..2 {
                    for k in 0..self.n_filt {
                        bp[c] = bq(&mut self.lps[c][k], &self.lpc[k], bp[c]);
                    }
                }
            }
            self.res.put([inp[0] - bp[0], inp[1] - bp[1]]);
            self.band.put(bp);
            self.blk = self.blk.max(bp[0].abs().max(bp[1].abs()));
            let rd_band = self.band.tap(self.lat);
            let rd_res = self.res.tap(self.lat);
            for c in 0..2 {
                let xin = bp[c] * self.pre + self.bias;
                let sh = if let Some(k) = self.asym_k {
                    if self.dc_warm {
                        for _ in 0..256 {
                            self.ov[c].process_fe(&self.taps, &self.lut, self.os, xin, &mut self.ove[c]);
                        }
                    }
                    let (f, e) = self.ov[c].process_fe(&self.taps, &self.lut, self.os, xin, &mut self.ove[c]);
                    if self.dc_warm {
                        // settle the even-part high-pass on its constant input (output 0)
                        for _ in 0..16384 {
                            bq(&mut self.evs[c], &self.evc, e);
                        }
                    }
                    f - e + k * bq(&mut self.evs[c], &self.evc, e)
                } else {
                    if self.dc_warm {
                        for _ in 0..256 {
                            self.ov[c].process(&self.taps, &self.lut, self.os, xin);
                        }
                    }
                    self.ov[c].process(&self.taps, &self.lut, self.os, xin)
                };
                if self.dc_warm {
                    self.dcx[c] = sh;
                    self.dcy[c] = 0.0;
                }
                let mut y = sh - self.dcx[c] + self.dc_r * self.dcy[c] + DENORM;
                self.dcx[c] = sh;
                self.dcy[c] = y;
                if self.use_tone {
                    y = bq(&mut self.tns[c], &self.tnc, y);
                }
                if self.mix < 0.999 {
                    y = rd_band[c] + (y - rd_band[c]) * self.mix;
                }
                cc[c] = rd_res[c] + y;
            }
            self.dc_warm = false;
        } else {
            self.res.put([0.0, 0.0]);
            cc = dl;
        }

        ms_step(&mut self.ms, cc[0], cc[1], self.ms_c);
        (cc, dl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapers::CUSTOM;

    /// Kick-like loop through the user's zig-zag custom curve: (min of the 20 Hz low-passed output, peak) per ASYM setting.
    fn run(asym: Option<f64>, os: i32) -> (f64, f64, f64) {
        let sr = 48000.0;
        let curve = Curve::from_points(&[(-1.0, -1.0), (-0.6, -0.2), (-0.35, -0.8), (0.0, 0.1), (0.3, 0.7), (0.5, 0.1), (0.8, 0.8), (1.0, 0.9)]);
        let p = SatParams { shape: CUSTOM, curve, drive: 16.0, bias: 0.0, os, mix: 100.0, asym, ..SatParams::default() };
        let mut s = Saturator::new();
        s.derive(&p, sr, true, false);
        let lp = biquad::lp(6.0, 0.70710678, sr);
        let (mut st, mut lo, mut hi, mut pk) = ([0.0; 2], 0.0f64, 0.0f64, 0.0f64);
        let n = (sr * 2.0) as usize;
        for i in 0..n {
            let t = (i as f64 / sr) % 0.345;
            let f = 55.0 + 90.0 * (-t / 0.02).exp();
            let ph = 2.0 * PI * (55.0 * t + 90.0 * 0.02 * (1.0 - (-t / 0.02).exp()));
            let _ = f;
            let x = 0.9 * (-t / 0.08).exp() * ph.sin() + 0.1 * (2.0 * PI * 3000.0 * i as f64 / sr).sin() * (-t / 0.1).exp();
            let y = if asym == Some(-1.0) { x } else { s.tick([x, x], false)[0] };
            let l = bq(&mut st, &lp, y);
            if i > n / 2 {
                lo = lo.min(l);
                hi = hi.max(l);
                pk = pk.max(y.abs());
            }
        }
        (lo, hi, pk)
    }

    #[test]
    fn asym_tames_sub_audio_offset_of_a_lopsided_curve() {
        for os in [0, 2] {
            let legacy = run(None, os);
            let full = run(Some(100.0), os);
            let none = run(Some(0.0), os);
            eprintln!("input only {:?}", run(Some(-1.0), os));
            eprintln!("os {os}: legacy {legacy:?}  asym100 {full:?}  asym0 {none:?}");
            let swing = |r: (f64, f64, f64)| r.1 - r.0;
            assert!(swing(full) < 0.5 * swing(legacy), "asym 100 low-passed swing {full:?} vs legacy {legacy:?}");
        }
    }
}
