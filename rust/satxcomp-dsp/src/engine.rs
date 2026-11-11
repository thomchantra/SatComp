//! SatXComp signal path: LR4 3-way split -> per band [saturator -> compressor -> comp mix/gain] -> sum (solo / delta) ->
//! 5-band EQ (+ tilt, mix, gain) -> dry/wet mix -> out gain -> equal-loudness bypass.
//! Meters and match logic follow `satcomp_dsp::Engine`.

use crate::params::{eq_kind, EqParams, XParams};
use crate::strip::Strip;
use crate::xover::Crossover3;
use satcomp_dsp::biquad::eq_coef;
use satcomp_dsp::delay::StereoDelay;
use satcomp_dsp::eq::{EqBank, EqSection};
use satcomp_dsp::level::{ms_step, one_pole, Matcher, DB_POW, LN10_20};
use satcomp_dsp::{ScopeRing, TapRing, DENORM};
use std::sync::Arc;

const EQ_FILTERS: usize = 7; // 5 bands + tilt low shelf + tilt high shelf

/// Sets the bank targets for the 5 bands and the tilt pair. Returns whether any filter does something.
fn eq_setup(bank: &mut EqBank<EQ_FILTERS>, p: &EqParams, sr: f64) -> bool {
    let mut any = false;
    for b in 0..5 {
        let bp = &p.bands[b];
        let coef = eq_coef(eq_kind(b, bp.ty), bp.freq.clamp(20.0, 20000.0), bp.gain.clamp(-18.0, 18.0), bp.q.clamp(0.1, 10.0), sr);
        bank.target(b, Some(coef), bp.on);
        any |= bp.on;
    }
    let t = p.tilt.clamp(-12.0, 12.0);
    let pv = if p.pivot < 19.5 { 600.0 } else { p.pivot.min(20000.0) };
    let on = t != 0.0;
    bank.target(5, if on { Some(eq_coef(2, pv, -t * 0.5, 0.4, sr)) } else { None }, on);
    bank.target(6, if on { Some(eq_coef(3, pv, t * 0.5, 0.4, sr)) } else { None }, on);
    any || on
}

pub struct XEngine {
    sr: f64,
    sr_prev: f64,
    p: XParams,
    dirty: bool,
    pub smoothing: bool,
    snap: bool,
    first_set: bool,

    xo: Crossover3,
    pub strips: [Strip; 3],
    eq: EqSection<EQ_FILTERS>,
    eq_mix: f64,
    eq_mix_t: f64,
    dw: StereoDelay<4096>,
    la_s: usize,
    any_solo: bool,
    d_s: [f64; 3],
    solo_s: [f64; 3],
    sm_c: f64,

    og: f64,
    og_t: f64,
    og_c: f64,
    mix: f64,
    mix_t: f64,
    bg: f64,
    bg_t: f64,
    bg_c: f64,
    byp_s: f64,
    byp_c: f64,
    rmsg_c: f64,
    fms_c: f64,
    /// monitor what all compressors removed (UI toggle, not a stored parameter)
    pub delta_on: bool,
    gd_s: f64,

    pub ims: f64,
    pub oms: f64,
    pub pms: f64,
    /// mean squares into / out of the EQ stage (its dry/wet mix and gain included)
    eis: f64,
    eos: f64,
    /// level change across the EQ stage, dB (for the EQ IN>OUT readout)
    pub eq_diff: f64,
    pub diff_db: f64,
    pub diff_disp: f64,
    gate_open: bool,
    match_on: bool,
    prev_match_on: bool,
    out_m: Matcher,
    pub match_trim: f64,
    match_req: bool,

    /// spectrum taps (mono): output signal, and the saturated signal before the compressors ("dry")
    pub tap_out: Arc<TapRing>,
    pub tap_dry: Arc<TapRing>,
    pub tap_on: bool,
    pub scope: Arc<ScopeRing>,

    pub opk: [f64; 2],
    pub clip_f: bool,
    /// per band: gain reduction (dB) of the current block / peak since the owner last cleared it
    pub gr_blk: [f64; 3],
    pub gr_pk: [f64; 3],
    /// decaying peak of each band's saturator input (for the live marker on the transfer plot)
    pub sat_vz: [f64; 3],
    pub fms: f64,
}

impl XEngine {
    pub fn new(sr: f64) -> Self {
        let mut e = Self {
            sr,
            sr_prev: 0.0,
            p: XParams::default(),
            dirty: true,
            smoothing: true,
            snap: true,
            first_set: true,
            xo: Crossover3::new(),
            strips: [Strip::new(), Strip::new(), Strip::new()],
            eq: EqSection::default(),
            eq_mix: 1.0,
            eq_mix_t: 1.0,
            dw: StereoDelay::new(),
            la_s: 0,
            any_solo: false,
            d_s: [0.0; 3],
            solo_s: [1.0; 3],
            sm_c: 0.0,
            og: 1.0,
            og_t: 1.0,
            og_c: 0.0,
            mix: 1.0,
            mix_t: 1.0,
            bg: 1.0,
            bg_t: 1.0,
            bg_c: 0.0,
            byp_s: 0.0,
            byp_c: 0.0,
            rmsg_c: 0.0,
            fms_c: 0.0,
            delta_on: false,
            gd_s: 0.0,
            ims: 1e-9,
            oms: 1e-9,
            pms: 1e-9,
            eis: 1e-9,
            eos: 1e-9,
            eq_diff: 0.0,
            diff_db: 0.0,
            diff_disp: 0.0,
            gate_open: false,
            match_on: false,
            prev_match_on: false,
            out_m: Matcher::default(),
            match_trim: 0.0,
            match_req: false,
            tap_out: Arc::new(TapRing::new()),
            tap_dry: Arc::new(TapRing::new()),
            tap_on: false,
            scope: Arc::new(ScopeRing::new()),
            opk: [0.0; 2],
            clip_f: false,
            gr_blk: [0.0; 3],
            gr_pk: [0.0; 3],
            sat_vz: [0.0; 3],
            fms: 1e-9,
        };
        e.derive();
        e
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sr = sr;
        self.dirty = true;
    }

    pub fn set_params(&mut self, p: &XParams) {
        if self.first_set {
            // a fresh engine starts at the host's settings instead of gliding from defaults (host rebuilds on every enable)
            self.first_set = false;
            self.p = *p;
            self.snap = true;
            let sm = std::mem::replace(&mut self.smoothing, false);
            self.derive();
            self.smoothing = sm;
            return;
        }
        if *p != self.p {
            self.p = *p;
            self.derive();
        }
    }

    /// PDC latency to report (saturator oversampling + lookahead; all bands are aligned).
    pub fn latency(&self) -> u32 {
        self.strips[0].latency() as u32
    }

    pub fn request_match(&mut self) {
        self.match_req = true;
    }
    pub fn out_matching(&self) -> bool {
        self.out_m.active
    }
    pub fn out_match_progress(&self) -> f64 {
        self.out_m.t
    }
    pub fn bg_gain(&self) -> f64 {
        self.bg
    }
    pub fn gate_open(&self) -> bool {
        self.gate_open
    }
    pub fn sample_rate(&self) -> f64 {
        self.sr
    }

    fn derive(&mut self) {
        let p = self.p;
        let sr = self.sr;
        self.sr_prev = sr;
        self.dirty = false;
        let snap = self.snap;
        self.la_s = ((p.lookahead_ms * 0.001 * sr + 0.5).floor().max(0.0) as usize).min(1900);
        self.xo.set(p.x1, p.x2, p.x1_slope.clamp(0, 3) as usize, p.x2_slope.clamp(0, 3) as usize, sr, snap);
        for b in 0..3 {
            self.strips[b].derive(&p.bands[b], p.os, self.la_s, sr, snap, self.smoothing);
        }
        self.any_solo = p.bands.iter().any(|b| b.solo);
        let any = eq_setup(&mut self.eq.bank, &p.eq, sr);
        self.eq.derive_gain(any, p.eq.gain, !p.eq.bypass, sr, snap);
        self.eq_mix_t = p.eq.mix * 0.01;
        self.mix_t = p.mix * 0.01;
        self.match_on = p.gain_match;
        if self.match_on && !self.prev_match_on {
            self.out_m.start();
        }
        if !self.match_on {
            self.out_m.stop();
        }
        self.prev_match_on = self.match_on;
        self.rmsg_c = one_pole(sr, 0.5);
        self.og_c = one_pole(sr, 0.02);
        self.byp_c = one_pole(sr, 0.01);
        self.bg_c = one_pole(sr, 0.2);
        self.fms_c = one_pole(sr, 0.3);
        self.sm_c = one_pole(sr, 0.02);
        if snap {
            self.snap = false;
            self.eq_mix = self.eq_mix_t;
            self.mix = self.mix_t;
        }
    }

    pub fn begin_block(&mut self, n: usize) {
        if self.match_req {
            self.out_m.start();
            self.match_req = false;
        }
        if self.dirty || self.sr != self.sr_prev {
            self.derive();
        }
        self.xo.begin_block(n);
        for s in self.strips.iter_mut() {
            s.begin_block();
        }
        self.eq.begin_block(n);

        let dt = n as f64 / self.sr;
        self.gate_open = self.ims > 0.000001;
        if self.gate_open {
            self.diff_db = DB_POW * (self.oms.max(1e-12) / self.ims.max(1e-12)).ln();
            self.diff_disp = DB_POW * (self.pms.max(1e-12) / self.ims.max(1e-12)).ln();
        }
        if self.eis > 0.000001 {
            self.eq_diff = DB_POW * (self.eos.max(1e-12) / self.eis.max(1e-12)).ln();
        }
        if self.out_m.active && self.gate_open {
            if let Some(avg) = self.out_m.step(self.diff_db, dt) {
                self.match_trim = avg.clamp(-12.0, 12.0);
            }
        }
        let trim = if self.match_on { self.match_trim } else { 0.0 };
        self.og_t = ((self.p.out_gain + trim) * LN10_20).exp();
        for b in 0..3 {
            self.gr_blk[b] = self.strips[b].gr_blk;
            self.gr_pk[b] = self.gr_pk[b].max(self.gr_blk[b]);
            self.strips[b].gr_blk = 0.0;
        }
        let vz_d = (-(n as f64) / (self.sr * 0.15)).exp();
        for b in 0..3 {
            self.sat_vz[b] = self.strips[b].sat.blk.max(self.sat_vz[b] * vz_d);
            self.strips[b].sat.blk = 0.0;
        }
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        self.begin_block(n);
        for i in 0..n {
            let (l, r) = self.tick(left[i] as f64, right[i] as f64);
            left[i] = l as f32;
            right[i] = r as f32;
        }
    }

    #[inline]
    pub fn tick(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        let inp = [in_l, in_r];
        ms_step(&mut self.ims, in_l, in_r, self.rmsg_c);
        if self.smoothing {
            self.eq_mix += (self.eq_mix_t - self.eq_mix) * self.sm_c;
            self.mix += (self.mix_t - self.mix) * self.sm_c;
        } else {
            self.eq_mix = self.eq_mix_t;
            self.mix = self.mix_t;
        }
        self.dw.advance();
        self.dw.put(inp);
        let dt = self.dw.tap(self.strips[0].latency());

        // ---- split, process each band, sum with solo / delta weighting
        let bands = self.xo.tick(inp);
        let sgn = self.xo.signs();
        let mut sum = [0.0f64; 2];
        let mut pre = [0.0f64; 2];
        let gdelta = if self.delta_on { 1.0 } else { 0.0 };
        self.gd_s += (gdelta - self.gd_s) * self.byp_c;
        for b in 0..3 {
            let o = self.strips[b].tick(bands[b], self.smoothing);
            let bp = &self.p.bands[b];
            let dtarget: f64 = if bp.delta { 1.0 } else { 0.0 };
            let d = self.d_s[b] + (dtarget.max(gdelta) - self.d_s[b]) * self.byp_c;
            self.d_s[b] = d;
            let stgt = if self.any_solo { if bp.solo { 1.0 } else { 0.0 } } else { 1.0 };
            let sg = self.solo_s[b] + (stgt - self.solo_s[b]) * self.byp_c;
            self.solo_s[b] = sg;
            for c in 0..2 {
                sum[c] += (o.y[c] + (o.removed[c] - o.y[c]) * d) * sg * sgn[b];
                pre[c] += o.pre[c] * sgn[b];
            }
        }

        // ---- EQ (post-sum) with its own dry/wet
        let ew = self.eq.tick(sum);
        let wet = [sum[0] + (ew[0] - sum[0]) * self.eq_mix, sum[1] + (ew[1] - sum[1]) * self.eq_mix];
        ms_step(&mut self.eis, sum[0], sum[1], self.rmsg_c);
        ms_step(&mut self.eos, wet[0], wet[1], self.rmsg_c);

        // ---- OUT
        let mut o = [0.0f64; 2];
        for c in 0..2 {
            o[c] = dt[c] + (wet[c] - dt[c]) * self.mix;
        }
        ms_step(&mut self.oms, o[0], o[1], self.rmsg_c);
        self.og += (self.og_t - self.og) * self.og_c;
        let pp = [o[0] * self.og, o[1] * self.og];
        ms_step(&mut self.pms, pp[0], pp[1], self.rmsg_c);
        self.byp_s += ((if self.p.bypass { 1.0 } else { 0.0 }) - self.byp_s) * self.byp_c;
        if self.gate_open {
            self.bg_t = (self.pms / self.ims).sqrt().clamp(0.0631, 15.85);
        }
        self.bg += (self.bg_t - self.bg) * self.bg_c;
        let out = [pp[0] + (dt[0] * self.bg - pp[0]) * self.byp_s, pp[1] + (dt[1] * self.bg - pp[1]) * self.byp_s];

        if self.tap_on {
            self.tap_out.push(((out[0] + out[1]) * 0.5) as f32);
            self.tap_dry.push(((pre[0] + pre[1]) * 0.5) as f32);
        }
        self.opk[0] = self.opk[0].max(out[0].abs());
        self.opk[1] = self.opk[1].max(out[1].abs());
        if out[0].abs() > 0.9999 || out[1].abs() > 0.9999 {
            self.clip_f = true;
        }
        self.fms += ((out[0] * out[0] + out[1] * out[1]) * 0.5 + DENORM - self.fms) * self.fms_c;
        (out[0], out[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn noise(n: usize, seed: u64) -> Vec<f64> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (((s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0) * 0.3
            })
            .collect()
    }

    /// a plug-in state where nothing alters the signal (no saturation, no compression, EQ flat)
    fn neutral() -> XParams {
        let mut p = XParams::default();
        for b in p.bands.iter_mut() {
            b.drive = 0.0;
            b.ratio = 1.0;
            b.thresh = 0.0;
        }
        p
    }

    fn run(e: &mut XEngine, x: &[f64]) -> Vec<f64> {
        let (mut l, mut r): (Vec<f32>, Vec<f32>) = (x.iter().map(|v| *v as f32).collect(), x.iter().map(|v| *v as f32).collect());
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
        }
        l.iter().map(|v| *v as f64).collect()
    }

    fn rms(x: &[f64]) -> f64 {
        (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
    }

    #[test]
    fn neutral_engine_is_flat_in_level() {
        let x = noise(48000, 1);
        let mut e = XEngine::new(SR);
        e.set_params(&neutral());
        let y = run(&mut e, &x);
        let (a, b) = (rms(&x[4000..]), rms(&y[4000..]));
        assert!((20.0 * (b / a).log10()).abs() < 0.1, "{a} vs {b}");
    }

    #[test]
    fn neutral_engine_stays_flat_at_every_crossover_slope() {
        let x = noise(48000, 5);
        for (s1, s2) in [(0, 0), (0, 3), (2, 1), (3, 3)] {
            let mut p = neutral();
            p.x1_slope = s1;
            p.x2_slope = s2;
            let mut e = XEngine::new(SR);
            e.set_params(&p);
            let y = run(&mut e, &x);
            let g = 20.0 * (rms(&y[6000..]) / rms(&x[6000..])).log10();
            assert!(g.abs() < 0.1, "slopes {s1}/{s2}: {g} dB");
        }
    }

    #[test]
    fn switching_crossover_slope_does_not_click() {
        // a steady tone through the whole engine; the slopes change mid-stream and the output must stay as smooth as the tone itself
        let amp = 0.5;
        let w = 2.0 * PI * 330.0 / SR;
        let tone: Vec<f64> = (0..48000).map(|i| amp * (w * i as f64).sin()).collect();
        for (from, to) in [((1, 1), (2, 1)), ((1, 1), (0, 1)), ((0, 0), (3, 3)), ((3, 3), (1, 1))] {
            let mut p = neutral();
            p.x1 = 200.0;
            p.x2 = 2500.0;
            p.x1_slope = from.0;
            p.x2_slope = from.1;
            let mut e = XEngine::new(SR);
            e.set_params(&p);
            let mut y = run(&mut e, &tone[..24000]);
            p.x1_slope = to.0;
            p.x2_slope = to.1;
            e.set_params(&p);
            y.extend(run(&mut e, &tone[24000..]));
            let worst = y.windows(2).skip(8000).map(|d| (d[1] - d[0]).abs()).fold(0.0, f64::max);
            // a pure sine of this amplitude moves at most amp * w per sample
            assert!(worst < amp * w * 1.5, "slope {from:?} -> {to:?} clicked: step {worst:.4} (tone {:.4})", amp * w);
        }
    }

    #[test]
    fn a_band_only_affects_its_own_range() {
        // compress only the mid band hard: a low tone is untouched, a mid tone is reduced
        let tone = |f: f64| (0..48000).map(|i| 0.5 * (2.0 * PI * f * i as f64 / SR).sin()).collect::<Vec<f64>>();
        let mut p = neutral();
        p.bands[1].thresh = -30.0;
        p.bands[1].ratio = 20.0;
        let (lo, mid) = (tone(60.0), tone(900.0));
        let (mut e1, mut e2) = (XEngine::new(SR), XEngine::new(SR));
        e1.set_params(&p);
        e2.set_params(&p);
        let (ylo, ymid) = (run(&mut e1, &lo), run(&mut e2, &mid));
        let dlo = 20.0 * (rms(&ylo[8000..]) / rms(&lo[8000..])).log10();
        let dmid = 20.0 * (rms(&ymid[8000..]) / rms(&mid[8000..])).log10();
        assert!(dlo.abs() < 0.3, "low changed {dlo}");
        assert!(dmid < -8.0, "mid not compressed {dmid}");
    }

    #[test]
    fn solo_isolates_a_band_and_off_band_passes_through() {
        let x = noise(48000, 2);
        let mut p = neutral();
        p.bands[0].solo = true;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        let y = run(&mut e, &x);
        assert!(rms(&y[6000..]) < rms(&x[6000..]) * 0.6, "solo low should remove most of a white noise");
        let mut p = neutral();
        p.bands[1].on = false;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        let y2 = run(&mut e, &x);
        assert!((20.0 * (rms(&y2[6000..]) / rms(&x[6000..])).log10()).abs() < 0.1);
    }

    #[test]
    fn eq_band_boosts_and_latency_is_reported() {
        let tone: Vec<f64> = (0..48000).map(|i| 0.2 * (2.0 * PI * 5000.0 * i as f64 / SR).sin()).collect();
        let mut p = neutral();
        p.eq.bands[3].on = true;
        p.eq.bands[3].freq = 5000.0;
        p.eq.bands[3].gain = 6.0;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        let y = run(&mut e, &tone);
        let g = 20.0 * (rms(&y[8000..]) / rms(&tone[8000..])).log10();
        assert!((g - 6.0).abs() < 0.3, "{g}");
        p.os = 2;
        p.lookahead_ms = 2.0;
        e.set_params(&p);
        assert_eq!(e.latency(), 28 + (2.0f64 * 0.001 * SR + 0.5).floor() as u32);
    }

    #[test]
    fn sat_gain_changes_the_level_after_the_first_block() {
        let x = noise(48000, 4);
        let mut e = XEngine::new(SR);
        e.set_params(&neutral());
        let y0 = run(&mut e, &x);
        let mut p = neutral();
        for b in p.bands.iter_mut() {
            b.sat_gain = 6.0;
        }
        e.set_params(&p);
        let y1 = run(&mut e, &x);
        let g = 20.0 * (rms(&y1[8000..]) / rms(&y0[8000..])).log10();
        assert!((g - 6.0).abs() < 0.5, "sat gain +6 dB moved the level by {g}");
    }

    fn tone_db(f: f64, p: &XParams, warm: Option<&XParams>) -> f64 {
        let t: Vec<f64> = (0..48000).map(|i| 0.2 * (2.0 * PI * f * i as f64 / SR).sin()).collect();
        let mut e = XEngine::new(SR);
        if let Some(w) = warm {
            e.set_params(w);
            run(&mut e, &t);
        }
        e.set_params(p);
        let y = run(&mut e, &t);
        20.0 * (rms(&y[8000..]) / rms(&t[8000..])).log10()
    }

    #[test]
    fn eq_gain_mix_and_pivot_act() {
        // EQ GAIN on its own (no bands on)
        let mut p = neutral();
        p.eq.gain = 6.0;
        let g = tone_db(1000.0, &p, None);
        assert!((g - 6.0).abs() < 0.3, "eq gain {g}");
        // ... also when changed after the engine has been running
        let g = tone_db(1000.0, &p, Some(&neutral()));
        assert!((g - 6.0).abs() < 0.3, "eq gain after warm-up {g}");
        // EQ MIX 0 takes the whole EQ out
        let mut p = neutral();
        p.eq.bands[3].on = true;
        p.eq.bands[3].freq = 5000.0;
        p.eq.bands[3].gain = 9.0;
        let full = tone_db(5000.0, &p, None);
        p.eq.mix = 0.0;
        let off = tone_db(5000.0, &p, None);
        assert!(full > 8.0 && off.abs() < 0.3, "eq mix: full {full}, mix 0 {off}");
        // pivot moves where the tilt crosses 0 dB
        let mut p = neutral();
        p.eq.tilt = 6.0;
        p.eq.pivot = 200.0;
        let a = tone_db(200.0, &p, None);
        p.eq.pivot = 4000.0;
        let b = tone_db(200.0, &p, None);
        assert!((a - b).abs() > 2.0, "pivot has no effect: {a} vs {b}");
    }

    #[test]
    fn eq_diff_reads_the_level_change_across_the_eq_stage_only() {
        let tone: Vec<f64> = (0..96000).map(|i| 0.2 * (2.0 * PI * 5000.0 * i as f64 / SR).sin()).collect();
        let mut p = neutral();
        p.eq.bands[3].on = true;
        p.eq.bands[3].freq = 5000.0;
        p.eq.bands[3].gain = 6.0;
        p.out_gain = -9.0; // output gain is outside the EQ stage and must not show up
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        run(&mut e, &tone);
        assert!((e.eq_diff - 6.0).abs() < 0.5, "eq diff {}", e.eq_diff);
        p.eq.mix = 0.0;
        e.set_params(&p);
        run(&mut e, &tone);
        assert!(e.eq_diff.abs() < 0.5, "eq mix 0 should read 0 dB, got {}", e.eq_diff);
    }

    #[test]
    fn sat_vz_follows_each_bands_saturator_input() {
        let tone: Vec<f64> = (0..48000).map(|i| 0.4 * (2.0 * PI * 50.0 * i as f64 / SR).sin()).collect();
        let mut e = XEngine::new(SR);
        e.set_params(&XParams::default()); // default drive: the saturator only measures while it is active
        run(&mut e, &tone);
        assert!((e.sat_vz[0] - 0.4).abs() < 0.08, "low band sees the 50 Hz tone: {}", e.sat_vz[0]);
        assert!(e.sat_vz[1] < 0.02 && e.sat_vz[2] < 0.02, "other bands stay quiet: {:?}", e.sat_vz);
    }

    #[test]
    fn saturation_and_all_modes_run_clean() {
        let x = noise(24000, 3);
        for os in 0..3 {
            let mut p = XParams::default();
            p.os = os;
            p.lookahead_ms = 3.0;
            for b in p.bands.iter_mut() {
                b.drive = 80.0;
                b.shape = 5;
                b.thresh = -24.0;
                b.release = 2000.0;
            }
            p.eq.bands[2].on = true;
            p.eq.tilt = 3.0;
            let mut e = XEngine::new(SR);
            e.set_params(&p);
            let y = run(&mut e, &x);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 8.0));
        }
    }
}
