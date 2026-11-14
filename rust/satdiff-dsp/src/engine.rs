//! SatDiff engine, step 1: SAT (shared saturator, with SAT MIX / SAT GAIN) -> OUT GAIN (+ gain-match trim) -> true-peak
//! output limiter (optional). The phase rotator and the colour-fx slot (docs/internal/SatDiff_spec.md section 2) go in
//! front of / around the saturator in the next steps. Stereo, f64 inside.
//!
//! The limiter's delay is always reported (so toggling LIMIT never changes the latency); bypass returns the input at that
//! same latency. DELTA monitors what the processed path changed (processed minus the aligned input, before the limiter).

use crate::params::DiffParams;
use satcomp_dsp::delay::StereoDelay;
use satcomp_dsp::level::{ms_step, one_pole, Matcher, DB_AMP, DB_POW, LN10_20};
use satcomp_dsp::{Limiter, SatParams, Saturator, ScopeRing};
use std::sync::Arc;

/// Delay ring length: larger than the longest latency (saturator oversampler + the limiter's lookahead at the highest rates).
const RING: usize = 2048;

pub struct DiffEngine {
    sr: f64,
    sr_prev: f64,
    p: DiffParams,
    dirty: bool,
    first_set: bool,
    snap: bool,
    /// Smooth Drive / Bias / Mix (set false for null tests).
    pub smoothing: bool,
    sat: Saturator,
    lim: Limiter,
    /// the input, kept to line up the bypass and the delta with the processed path
    dry: StereoDelay<RING>,
    /// the processed signal before the limiter (for the delta)
    pre: StereoDelay<RING>,
    og: f64,
    og_t: f64,
    byp_s: f64,
    d_s: f64,
    sm_c: f64,
    byp_c: f64,
    rms_c: f64,
    match_on: bool,
    prev_match_on: bool,
    out_m: Matcher,
    pub match_trim: f64,
    match_req: bool,
    ims: f64,
    oms: f64,
    diff_db: f64,
    gate_open: bool,
    sat_m: Matcher,
    sat_req: bool,
    sat_result: Option<f64>,
    /// monitor what the chain changed (UI -> audio)
    pub delta_on: bool,
    pub scope: Arc<ScopeRing>,
    sc_len: usize,
    sc_n: usize,
    sc_o: f64,
    sc_p: f64,
    // telemetry, audio -> UI. The shell takes the peaks with fetch_max and clears them.
    /// output peak per channel, linear
    pub opk: [f64; 2],
    /// post-saturator (after SAT GAIN) peak, linear
    pub sat_out: f64,
    /// decaying peak of the saturator input for the plot's live dot
    pub sat_vz: f64,
    /// limiter gain reduction peak, dB positive
    pub gr_pk: f64,
    /// smoothed output mean-square (RMS readout)
    pub fms: f64,
    /// loudness change across the saturator (before SAT GAIN), dB: the plot's SAT readout
    pub sat_db: f64,
    /// loudness change across the whole chain (before the limiter), dB
    pub diff_disp: f64,
    pub clip_f: bool,
}

impl DiffEngine {
    pub fn new(sr: f64) -> Self {
        let mut e = Self {
            sr,
            sr_prev: 0.0,
            p: DiffParams::default(),
            dirty: true,
            first_set: true,
            snap: true,
            smoothing: true,
            sat: Saturator::new(),
            lim: Limiter::new(sr),
            dry: StereoDelay::new(),
            pre: StereoDelay::new(),
            og: 1.0,
            og_t: 1.0,
            byp_s: 0.0,
            d_s: 0.0,
            sm_c: 0.0,
            byp_c: 0.0,
            rms_c: 0.0,
            match_on: false,
            prev_match_on: false,
            out_m: Matcher::default(),
            match_trim: 0.0,
            match_req: false,
            ims: 1e-9,
            oms: 1e-9,
            diff_db: 0.0,
            gate_open: false,
            sat_m: Matcher::default(),
            sat_req: false,
            sat_result: None,
            delta_on: false,
            scope: Arc::new(ScopeRing::new()),
            sc_len: 96,
            sc_n: 0,
            sc_o: 0.0,
            sc_p: 0.0,
            opk: [0.0; 2],
            sat_out: 0.0,
            sat_vz: 0.0,
            gr_pk: 0.0,
            fms: 1e-9,
            sat_db: 0.0,
            diff_disp: 0.0,
            clip_f: false,
        };
        e.derive();
        e
    }

    pub fn sample_rate(&self) -> f64 {
        self.sr
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sr = sr;
        self.dirty = true;
    }

    pub fn set_params(&mut self, p: &DiffParams) {
        if self.first_set {
            // a fresh engine starts at the host's settings instead of gliding there from defaults (audible as a thump when
            // the host rebuilds the engine on every FX enable)
            self.first_set = false;
            self.p = *p;
            self.sat.set_out_db(p.sat_gain);
            self.lim.snap_on(p.lim_on);
            self.snap = true;
            let sm = std::mem::replace(&mut self.smoothing, false);
            self.derive();
            self.smoothing = sm;
            return;
        }
        if *p != self.p {
            if p.sat_gain != self.p.sat_gain {
                self.sat.set_out_db(p.sat_gain);
            }
            self.p = *p;
            self.derive(); // applied at once so latency() is valid before the first block
        }
    }

    /// PDC latency to report: saturator oversampler + limiter lookahead (the limiter's always).
    pub fn latency(&self) -> u32 {
        (self.sat.latency() + self.lim.latency()) as u32
    }

    fn derive(&mut self) {
        let p = self.p;
        let sr = self.sr;
        self.dirty = false;
        if sr != self.sr_prev {
            self.sr_prev = sr;
            self.lim.set_sample_rate(sr);
            self.sc_len = ((sr * ScopeRing::BUCKET_S).round() as usize).max(1);
            self.sm_c = one_pole(sr, 0.02);
            self.byp_c = one_pole(sr, 0.01);
            self.rms_c = one_pole(sr, 0.3);
        }
        let sp: SatParams = p.sat();
        self.sat.derive(&sp, sr, self.snap, self.smoothing);
        self.match_on = p.gain_match;
        if self.match_on && !self.prev_match_on {
            self.out_m.start();
        }
        if !self.match_on {
            self.out_m.stop();
        }
        self.prev_match_on = self.match_on;
        if self.snap {
            self.byp_s = if p.bypass { 1.0 } else { 0.0 };
            self.snap = false;
        }
    }

    /// Measure the saturator's loudness change for 3 s, then report the SAT GAIN that cancels it (a second call cancels).
    pub fn request_sat_match(&mut self) {
        self.sat_req = true;
    }

    pub fn sat_matching(&self) -> bool {
        self.sat_m.active
    }

    /// Seconds measured so far (0..3).
    pub fn sat_match_progress(&self) -> f64 {
        self.sat_m.t
    }

    /// The finished match, once (the shell writes it to the SAT GAIN parameter).
    pub fn take_sat_result(&mut self) -> Option<f64> {
        self.sat_result.take()
    }

    /// Re-measure the whole chain's loudness change and trim it away (the GAIN MATCH button).
    pub fn request_match(&mut self) {
        self.match_req = true;
    }

    pub fn out_matching(&self) -> bool {
        self.out_m.active
    }

    pub fn out_match_progress(&self) -> f64 {
        self.out_m.t
    }

    pub fn begin_block(&mut self, n: usize) {
        if self.match_req {
            self.out_m.start();
            self.match_req = false;
        }
        if self.dirty || self.sr != self.sr_prev {
            self.derive();
        }
        let dt = n as f64 / self.sr;
        self.gate_open = self.ims > 0.000001;
        if self.gate_open {
            self.diff_db = DB_POW * (self.oms.max(1e-12) / self.ims.max(1e-12)).ln();
            self.sat_db = DB_POW * (self.sat.ms.max(1e-12) / self.ims.max(1e-12)).ln();
            self.diff_disp = self.diff_db;
        }
        if self.sat_req {
            let was = self.sat_m.active;
            self.sat_m.start();
            if was {
                self.sat_m.stop(); // a second click cancels
            }
            self.sat_req = false;
        }
        if self.sat_m.active && self.gate_open {
            if let Some(avg) = self.sat_m.step(self.sat_db, dt) {
                let v = (avg.clamp(-24.0, 12.0) * 10.0 + 0.5).floor() * 0.1;
                self.sat.set_out_db(v);
                self.sat_result = Some(v);
            }
        }
        self.sat.begin_block();
        if self.out_m.active && self.gate_open {
            if let Some(avg) = self.out_m.step(self.diff_db, dt) {
                self.match_trim = avg.clamp(-12.0, 12.0);
            }
        }
        let trim = if self.match_on { self.match_trim } else { 0.0 };
        self.og_t = ((self.p.out_gain + trim) * LN10_20).exp();
        let vz_d = (-(n as f64) / (self.sr * 0.15)).exp();
        self.sat_vz = self.sat.blk.max(self.sat_vz * vz_d);
        self.sat_out = self.sat_out.max(self.sat.blk);
        self.sat.blk = 0.0;
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let n = left.len().min(right.len());
        self.begin_block(n);
        for i in 0..n {
            let (l, r) = self.tick(left[i] as f64, right[i] as f64);
            left[i] = l as f32;
            right[i] = r as f32;
        }
        let g = self.lim.gmin.max(1e-6);
        self.gr_pk = self.gr_pk.max(-g.ln() * DB_AMP);
        self.lim.gmin = 1.0;
    }

    #[inline]
    pub fn tick(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        let inp = [in_l, in_r];
        ms_step(&mut self.ims, in_l, in_r, self.rms_c);
        self.dry.advance();
        self.dry.put(inp);
        self.pre.advance();

        // SAT (+ SAT GAIN)
        let s = self.sat.tick(inp, self.smoothing);

        // OUT GAIN (+ the gain-match trim)
        self.og += (self.og_t - self.og) * self.sm_c;
        let pp = [s[0] * self.og, s[1] * self.og];
        ms_step(&mut self.oms, pp[0], pp[1], self.rms_c);
        self.pre.put(pp);

        // output limiter (processed path only: bypass and delta are monitors / the untouched input)
        let ll = self.lim.latency();
        let lo = self.lim.tick(pp, self.p.lim_on);
        let dd = self.dry.tap(self.sat.latency() + ll);
        self.byp_s += ((if self.p.bypass { 1.0 } else { 0.0 }) - self.byp_s) * self.byp_c;
        let mut out = [lo[0] + (dd[0] - lo[0]) * self.byp_s, lo[1] + (dd[1] - lo[1]) * self.byp_s];

        // delta monitor: the processed signal minus the aligned input, both ahead of the limiter
        self.d_s += ((if self.delta_on { 1.0 } else { 0.0 }) - self.d_s) * self.byp_c;
        if self.d_s > 0.0001 {
            let pd = self.pre.tap(ll);
            let dl = [pd[0] - dd[0], pd[1] - dd[1]];
            for c in 0..2 {
                out[c] += (dl[c] - out[c]) * self.d_s;
            }
        }

        // telemetry
        self.opk[0] = self.opk[0].max(out[0].abs());
        self.opk[1] = self.opk[1].max(out[1].abs());
        if out[0].abs() > 0.9999 || out[1].abs() > 0.9999 {
            self.clip_f = true;
        }
        ms_step(&mut self.fms, out[0], out[1], self.rms_c);
        // scope bucket: output peak, and the peak the limiter was handed
        let ao = out[0].abs().max(out[1].abs());
        let pl = self.lim.last_in[0].abs().max(self.lim.last_in[1].abs());
        self.sc_o = self.sc_o.max(ao);
        self.sc_p = self.sc_p.max(pl).max(ao);
        self.sc_n += 1;
        if self.sc_n >= self.sc_len {
            self.scope.push(self.sc_o as f32, self.sc_p as f32);
            self.sc_o = 0.0;
            self.sc_p = 0.0;
            self.sc_n = 0;
        }
        (out[0], out[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn noise(n: usize, seed: u64, amp: f64) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((((s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0) * amp) as f32
            })
            .collect()
    }

    fn run(p: &DiffParams, x: &[f32]) -> (Vec<f32>, DiffEngine) {
        let mut e = DiffEngine::new(SR);
        e.set_params(p);
        let (mut l, mut r) = (x.to_vec(), x.to_vec());
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
        }
        (l, e)
    }

    #[test]
    fn defaults_are_an_exact_delayed_copy() {
        let x = noise(20000, 3, 0.4);
        let (y, e) = run(&DiffParams::default(), &x);
        let lat = e.latency() as usize;
        assert!(lat > 0);
        for i in lat..x.len() {
            assert_eq!(y[i], x[i - lat], "sample {i}");
        }
    }

    #[test]
    fn latency_is_the_limiter_plus_the_oversampler_and_ignores_the_limit_switch() {
        let base = DiffEngine::new(SR).latency();
        let mut e = DiffEngine::new(SR);
        e.set_params(&DiffParams { lim_on: true, ..DiffParams::default() });
        assert_eq!(e.latency(), base);
        e.set_params(&DiffParams { os: 2, ..DiffParams::default() });
        assert!(e.latency() > base);
    }

    #[test]
    fn bypass_is_the_input_at_the_reported_latency() {
        let p = DiffParams { os: 2, drive: 60.0, out_gain: 6.0, lim_on: true, bypass: true, ..DiffParams::default() };
        let x = noise(20000, 5, 0.5);
        let (y, e) = run(&p, &x);
        let lat = e.latency() as usize;
        for i in lat + 100..x.len() {
            assert!((y[i] - x[i - lat]).abs() < 1e-6, "sample {i}");
        }
    }

    #[test]
    fn out_gain_scales_the_output() {
        let x: Vec<f32> = (0..24000).map(|i| (0.25 * (2.0 * PI * 1000.0 * i as f64 / SR).sin()) as f32).collect();
        let (y, _) = run(&DiffParams { out_gain: -6.0, ..DiffParams::default() }, &x);
        let pk = y[8000..].iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
        assert!((pk - 0.25 * 10f64.powf(-6.0 / 20.0)).abs() < 1e-3, "{pk}");
    }

    #[test]
    fn the_limiter_holds_the_ceiling_only_when_on() {
        let x = noise(48000, 11, 1.4);
        let hot = |on: bool| {
            let (y, _) = run(&DiffParams { lim_on: on, ..DiffParams::default() }, &x);
            y[4000..].iter().fold(0.0f32, |m, v| m.max(v.abs()))
        };
        assert!(hot(false) > 1.2);
        assert!(hot(true) < 1.0, "limited peak {}", hot(true));
    }

    #[test]
    fn delta_is_what_the_saturator_changed() {
        let x = noise(24000, 17, 0.7);
        // start after the monitor's 10 ms crossfade has settled
        let rms = |v: &[f32]| (v[12000..].iter().map(|a| (*a as f64).powi(2)).sum::<f64>() / (v.len() - 12000) as f64).sqrt();
        // nothing changed -> nothing in the delta
        let mut e = DiffEngine::new(SR);
        e.set_params(&DiffParams::default());
        e.delta_on = true;
        let (mut l, mut r) = (x.clone(), x.clone());
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
        }
        assert!(rms(&l) < 1e-9, "{}", rms(&l));
        // a hot saturator changes it
        let mut e = DiffEngine::new(SR);
        e.set_params(&DiffParams { drive: 80.0, shape: 1, ..DiffParams::default() });
        e.delta_on = true;
        let (mut l, mut r) = (x.clone(), x.clone());
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
        }
        assert!(rms(&l) > 0.01, "{}", rms(&l));
    }

    #[test]
    fn sat_match_measures_for_three_seconds_then_reports_the_gain_that_cancels_the_change() {
        let p = DiffParams { drive: 60.0, shape: 1, ..DiffParams::default() };
        let mut e = DiffEngine::new(SR);
        e.set_params(&p);
        e.request_sat_match();
        let x = noise(48000 * 4, 29, 0.3);
        let (mut l, mut r) = (x.clone(), x.clone());
        let mut got = None;
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
            if let Some(v) = e.take_sat_result() {
                got = Some(v);
            }
        }
        let v = got.expect("the measurement finishes within 4 s");
        assert!(v < 0.0 && v >= -24.0, "{v}");
        assert!((v + e.sat_db).abs() < 0.6, "match {v} vs measured change {}", e.sat_db);
        assert!(e.take_sat_result().is_none());
    }

    #[test]
    fn gain_match_trims_the_chain_back_to_the_input_level() {
        let x = noise(48000 * 5, 31, 0.2);
        let (_, e) = run(&DiffParams { out_gain: 6.0, gain_match: true, ..DiffParams::default() }, &x);
        assert!((e.match_trim + 6.0).abs() < 0.4, "{}", e.match_trim);
    }

    #[test]
    fn telemetry_reads_the_stages() {
        let x = noise(24000, 23, 0.8);
        let (_, e) = run(&DiffParams { drive: 30.0, shape: 1, lim_on: true, ..DiffParams::default() }, &x);
        assert!(e.sat_out > 0.2 && e.opk[0] > 0.2, "{} {:?}", e.sat_out, e.opk);
        assert!(e.scope.head() > 100);
    }
}
