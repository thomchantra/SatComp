//! SatDiff engine: PHASE (rotator) -> [DIFFUSOR if PRE] -> SAT (shared saturator, with SAT MIX / SAT GAIN) -> [DIFFUSOR if
//! POST] -> OUT GAIN (+ gain-match trim) -> true-peak output limiter (optional). The rotator and the diffusor are linear
//! IIR blocks with no latency (docs/internal/SatDiff_spec.md section 2). Tile, morph and the A/B scenes are not built yet.
//! Stereo, f64 inside.
//!
//! The limiter's delay is always reported (so toggling LIMIT never changes the latency); bypass returns the input at that
//! same latency. DELTA monitors what the processed path changed (processed minus the aligned input, before the limiter).

use crate::diffuse::Diffusor;
use crate::follower::Follower;
use crate::params::DiffParams;
use crate::phase::Rotator;
use satcomp_dsp::delay::StereoDelay;
use satcomp_dsp::level::{ms_step, one_pole, Matcher, DB_AMP, DB_POW, LN10_20};
use satcomp_dsp::{Limiter, SatParams, Saturator, ScopeRing, TapRing};
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
    /// shaper crossfade time when the shape changes, ms
    pub morph_ms: f64,
    /// take the DC a shaper change adds off the DC blocker in step with the crossfade (see `Saturator::set_dc_comp`)
    pub dc_comp: bool,
    rot: Rotator,
    fx: Diffusor,
    /// MORPH detector (a level follower on the saturator's input): position 0..1 of the blend towards scene B, detector mode only
    det: Follower,
    morph_det: bool,
    morph_m: f64,
    det_cnt: u32,
    /// where the fx currently runs (0 before the saturator, 1 after)
    fx_pos: i32,
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
    /// mono output samples for the editor's spectrum / oscilloscope
    pub tap: Arc<TapRing>,
    sc_len: usize,
    sc_n: usize,
    sc_o: f64,
    sc_p: f64,
    // telemetry, audio -> UI. The shell takes the peaks with fetch_max and clears them.
    /// output peak per channel, linear
    pub opk: [f64; 2],
    /// input peak per channel, linear (the left meter)
    /// peak of the saturator's output (after SAT MIX and SAT GAIN, before a POST fx), since the owner last cleared it
    pub ipk: [f64; 2],
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
            morph_ms: 30.0,
            dc_comp: true,
            rot: Rotator::new(sr),
            fx: Diffusor::new(sr),
            det: Follower::default(),
            morph_det: false,
            morph_m: 0.0,
            det_cnt: 0,
            fx_pos: 0,
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
            tap: Arc::new(TapRing::new()),
            sc_len: 96,
            sc_n: 0,
            sc_o: 0.0,
            sc_p: 0.0,
            opk: [0.0; 2],
            ipk: [0.0; 2],
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

    /// Where the MORPH blend sits right now, 0 (scene A) .. 1 (scene B): the detector's position, or DEPTH in MIX, 0 when off.
    pub fn morph_pos(&self) -> f64 {
        if !self.p.blending() {
            0.0
        } else if self.p.morph_dir == 2 {
            self.p.morph_depth * 0.01
        } else {
            self.morph_m
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
            self.rot.set_sample_rate(sr);
            self.fx.set_sample_rate(sr);
            self.sc_len = ((sr * ScopeRing::BUCKET_S).round() as usize).max(1);
            self.sm_c = one_pole(sr, 0.02);
            self.byp_c = one_pole(sr, 0.01);
            self.rms_c = one_pole(sr, 0.3);
        }
        let sp: SatParams = p.sat();
        // MORPH: DEPTH 0 = one scene (the edited one); MIX = a static blend at DEPTH; A>B / B>A = the detector moves it
        let depth = p.morph_depth * 0.01;
        self.morph_det = p.blending() && p.morph_dir != 2;
        if !p.blending() {
            self.morph_m = 0.0;
        }
        self.det.set_speed(p.morph_speed * 0.01, sr);
        let m0 = if p.morph_dir == 2 { depth } else { self.morph_m };
        self.sat.set_scene_b(if p.blending() { Some((p.shape_b, p.tiled_curve_b(), m0, self.morph_det)) } else { None });
        self.sat.set_dc_comp(self.dc_comp);
        self.sat.set_morph_steps(((self.morph_ms * 0.001 * sr / crate::MORPH_EVERY as f64).round() as u32).max(1));
        self.sat.derive(&sp, sr, self.snap, self.smoothing);
        self.rot.set(p.phase, self.snap);
        // only the diffusor exists so far: the other fx types behave as off
        self.fx.set(p.fx_on && p.fx_type == 0, p.fx_freq, p.fx_q, p.fx_stages, p.fx_spread, p.fx_mix, self.snap);
        if p.fx_pos != self.fx_pos {
            self.fx_pos = p.fx_pos;
            self.fx.reset(); // the filter state belongs to the other side of the saturator
        }
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

        // PHASE, then the diffusor when it runs before the saturator
        let r = self.rot.tick(inp);
        let pre = if self.fx_pos == 0 { self.fx.tick(r) } else { r };

        // MORPH detector: follows the level entering the saturator, moves the blend every 32 samples
        if self.morph_det {
            let lv = self.det.step(pre[0].abs().max(pre[1].abs()));
            self.det_cnt += 1;
            if self.det_cnt >= 32 {
                self.det_cnt = 0;
                let depth = self.p.morph_depth * 0.01;
                let l = Follower::to_unit(lv);
                self.morph_m = if self.p.morph_dir == 0 { depth * l } else { depth * (1.0 - l) };
                self.sat.blend_now(self.morph_m);
            }
        }

        // SAT (+ SAT GAIN), then the diffusor when it runs after
        let mut s = self.sat.tick(pre, self.smoothing);
        // the segmented meter reads the saturator's output (after SAT MIX and SAT GAIN, before a POST fx): over 0 dBFS = the SAT
        // stage itself clips, whatever the later gain does
        self.ipk[0] = self.ipk[0].max(s[0].abs());
        self.ipk[1] = self.ipk[1].max(s[1].abs());
        if self.fx_pos == 1 {
            s = self.fx.tick(s);
        }

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
        self.tap.push(((out[0] + out[1]) * 0.5) as f32);
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
            self.scope.push3(self.sc_o as f32, self.sc_p as f32, self.sc_p as f32);
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

    fn rms(v: &[f32]) -> f64 {
        (v.iter().map(|a| (*a as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
    }

    #[test]
    fn phase_section_adds_no_latency() {
        let base = DiffEngine::new(SR).latency();
        let mut e = DiffEngine::new(SR);
        e.set_params(&DiffParams { phase: 90.0, fx_on: true, fx_stages: 16, ..DiffParams::default() });
        assert_eq!(e.latency(), base);
    }

    #[test]
    fn the_diffusor_and_the_rotator_keep_the_level_on_a_linear_path() {
        let x = noise(48000, 41, 0.4);
        let (y0, _) = run(&DiffParams::default(), &x);
        for p in [
            DiffParams { fx_on: true, fx_freq: 700.0, fx_q: 4.0, fx_stages: 12, ..DiffParams::default() },
            DiffParams { phase: 90.0, ..DiffParams::default() },
            DiffParams { phase: -40.0, fx_on: true, fx_pos: 1, fx_stages: 6, ..DiffParams::default() },
        ] {
            let (y, _) = run(&p, &x);
            let r = rms(&y[8000..]) / rms(&y0[8000..]);
            assert!((r - 1.0).abs() < 0.03, "{p:?}: level ratio {r}");
        }
    }

    #[test]
    fn pre_and_post_only_differ_through_the_saturator() {
        let x = noise(24000, 43, 0.5);
        let fx = |pos: i32, drive: f64, shape: i32| DiffParams { fx_on: true, fx_pos: pos, fx_freq: 900.0, fx_q: 5.0, fx_stages: 8, drive, shape, ..DiffParams::default() };
        let diff = |a: &[f32], b: &[f32]| rms(&a.iter().zip(b).skip(6000).map(|(p, q)| p - q).collect::<Vec<f32>>());
        // linear path: allpass filters commute with the (unity) saturator, order is irrelevant
        let (a, _) = run(&fx(0, 0.0, 0), &x);
        let (b, _) = run(&fx(1, 0.0, 0), &x);
        assert!(diff(&a, &b) < 1e-4, "linear {}", diff(&a, &b));
        // a hot shaper in between: dispersing the waveform first changes what it clips
        let (a, _) = run(&fx(0, 70.0, 1), &x);
        let (b, _) = run(&fx(1, 70.0, 1), &x);
        assert!(diff(&a, &b) > 0.02, "saturated {}", diff(&a, &b));
    }

    #[test]
    fn phase_changes_what_the_shaper_does_to_the_same_spectrum() {
        let x: Vec<f32> = (0..24000).map(|i| (0.45 * (2.0 * PI * 220.0 * i as f64 / SR).sin() + 0.25 * (2.0 * PI * 440.0 * i as f64 / SR).sin()) as f32).collect();
        let at = |deg: f64| run(&DiffParams { phase: deg, drive: 60.0, shape: 1, ..DiffParams::default() }, &x).0;
        let (a, b) = (at(0.5), at(90.0));
        let d = rms(&a.iter().zip(&b).skip(8000).map(|(p, q)| p - q).collect::<Vec<f32>>());
        assert!(d > 0.02, "a quarter turn should reshape the clipped output: {d}");
    }

    #[test]
    fn other_fx_types_do_nothing_yet() {
        let x = noise(8000, 47, 0.4);
        let (a, _) = run(&DiffParams::default(), &x);
        let (b, _) = run(&DiffParams { fx_on: true, fx_type: 2, fx_stages: 16, fx_q: 8.0, ..DiffParams::default() }, &x);
        assert_eq!(a, b);
    }

    #[test]
    fn telemetry_reads_the_stages() {
        let x = noise(24000, 23, 0.8);
        let (_, e) = run(&DiffParams { drive: 30.0, shape: 1, lim_on: true, ..DiffParams::default() }, &x);
        assert!(e.sat_out > 0.2 && e.opk[0] > 0.2, "{} {:?}", e.sat_out, e.opk);
        assert!(e.scope.head() > 100);
    }

    #[test]
    fn tile_forces_x4_oversampling_except_where_the_shaper_already_tiles() {
        let base = DiffEngine::new(SR).latency();
        let lat = |p: DiffParams| {
            let mut e = DiffEngine::new(SR);
            e.set_params(&p);
            e.latency()
        };
        let custom = satcomp_dsp::shapers::CUSTOM;
        let x4 = lat(DiffParams { os: 2, ..DiffParams::default() });
        assert!(x4 > base);
        assert_eq!(lat(DiffParams { tile: 0, shape: custom, ..DiffParams::default() }), base);
        for shape in [0, 1, 4, 7, 10, 13, custom] {
            assert_eq!(lat(DiffParams { tile: 1, shape, ..DiffParams::default() }), x4, "shape {shape}");
        }
        // Tri Fold, Sine Fold, Wrap, Asym Fold range-reduce themselves: TILE does not apply and costs nothing
        for shape in [5, 6, 9, 12] {
            assert_eq!(lat(DiffParams { tile: 2, shape, ..DiffParams::default() }), base, "shape {shape}");
            assert_eq!(DiffParams { tile: 2, shape, ..DiffParams::default() }.effective_tile(), 0);
        }
        // already at x8: stays there
        assert_eq!(lat(DiffParams { tile: 2, shape: custom, os: 3, ..DiffParams::default() }), lat(DiffParams { os: 3, ..DiffParams::default() }));
    }

    #[test]
    fn tile_works_on_factory_shapers_too() {
        // Soft (tanh) driven past full scale: off = clipped flat, mirror = folds back, repeat = jumps back to the bottom
        let x: Vec<f32> = (0..20000).map(|i| (0.45 * (2.0 * PI * 220.0 * i as f64 / SR).sin()) as f32).collect();
        let out = |tile: i32| run(&DiffParams { shape: 1, drive: 60.0, tile, ..DiffParams::default() }, &x).0;
        let (off, mir, rep) = (out(0), out(1), out(2));
        let d = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(p, q)| (p - q).abs() as f64).sum::<f64>() / a.len() as f64;
        assert!(d(&off, &mir) > 0.02 && d(&off, &rep) > 0.02 && d(&mir, &rep) > 0.02);
        // and a periodic shaper ignores the switch entirely
        let wrap = |tile: i32| run(&DiffParams { shape: 9, drive: 60.0, tile, ..DiffParams::default() }, &x).0;
        assert_eq!(wrap(0), wrap(1));
    }

    #[test]
    fn tile_changes_what_a_hot_drawn_curve_does() {
        // S-ish curve driven past +-1: flat clip vs mirror vs repeat give different outputs
        let custom = satcomp_dsp::shapers::CUSTOM;
        let curve = satcomp_dsp::shapers::Curve::from_points(&[(-1.0, -1.0), (-0.5, -0.8), (0.5, 0.8), (1.0, 1.0)]);
        let x: Vec<f32> = (0..20000).map(|i| (0.45 * (2.0 * PI * 220.0 * i as f64 / SR).sin()) as f32).collect();
        let out = |tile: i32| run(&DiffParams { shape: custom, curve, drive: 60.0, tile, ..DiffParams::default() }, &x).0;
        let (off, mir, rep) = (out(0), out(1), out(2));
        let d = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(p, q)| (p - q).abs() as f64).sum::<f64>() / a.len() as f64;
        assert!(d(&off, &mir) > 0.02 && d(&off, &rep) > 0.02 && d(&mir, &rep) > 0.02, "{} {} {}", d(&off, &mir), d(&off, &rep), d(&mir, &rep));
        assert!(mir.iter().chain(&rep).all(|v| v.is_finite() && v.abs() < 4.0));
    }

    /// Slow (below 20 Hz) deviation, after the crossfade, of a render that changes shape mid-stream from the same shape
    /// rendered all the way through, relative to its rms.
    fn shape_switch_error(from: DiffParams, to: DiffParams, comp: bool) -> f64 {
        let n = 48000;
        let sw = 24000;
        let x: Vec<f32> = (0..n).map(|i| (0.5 * (2.0 * PI * 110.0 * i as f64 / SR).sin()) as f32).collect();
        let render = |a: &DiffParams, b: &DiffParams| {
            let mut e = DiffEngine::new(SR);
            e.dc_comp = comp;
            e.set_params(a);
            let (mut l, mut r) = (x.clone(), x.clone());
            let mut i = 0;
            while i < n {
                if i == sw {
                    e.set_params(b);
                }
                let m = (i + 64).min(n);
                e.process(&mut l[i..m], &mut r[i..m]);
                i = m;
            }
            (l, e.latency() as usize)
        };
        let (y, lat) = render(&from, &to);
        let (yb, _) = render(&to, &to);
        let c = 1.0 - (-2.0 * PI * 20.0 / SR).exp();
        let after = sw + lat + (0.03 * SR) as usize + 100;
        let (mut lp, mut worst) = (0.0f64, 0.0f64);
        for i in sw..n - 100 {
            lp += ((y[i] - yb[i]) as f64 - lp) * c;
            if i > after {
                worst = worst.max(lp.abs());
            }
        }
        let rms = ((sw..sw + 9600).map(|i| (yb[i] as f64).powi(2)).sum::<f64>() / 9600.0).sqrt().max(1e-6);
        worst / rms
    }

    #[test]
    fn changing_the_shape_does_not_thump_the_output() {
        // pairs whose DC differs a lot (Tube, Crossover, Rectify, Diode, Asym Fold) with and without bias
        let mut gain_total = 0.0;
        let mut off_total = 0.0;
        for (a, b) in [(0, 10), (7, 10), (10, 13), (12, 10), (1, 12), (13, 7), (4, 10)] {
            for bias in [0.0, 25.0] {
                let pa = DiffParams { shape: a, drive: 40.0, bias, ..DiffParams::default() };
                let pb = DiffParams { shape: b, ..pa };
                let on = shape_switch_error(pa, pb, true);
                let off = shape_switch_error(pa, pb, false);
                assert!(on < 0.2, "{a} -> {b} bias {bias}: error {on} (without compensation {off})");
                on_off(&mut gain_total, &mut off_total, on, off);
            }
        }
        // overall the compensation must clearly beat leaving the blocker to ring
        assert!(gain_total < 0.5 * off_total, "with {gain_total} vs without {off_total}");
    }

    fn on_off(on_sum: &mut f64, off_sum: &mut f64, on: f64, off: f64) {
        *on_sum += on;
        *off_sum += off;
    }

    #[test]
    fn the_segmented_meter_reads_the_saturator_output() {
        let x: Vec<f32> = (0..24000).map(|i| (0.3 * (2.0 * PI * 90.0 * i as f64 / SR).sin()) as f32).collect();
        let peak = |p: DiffParams| run_peak(&p, &x).1;
        // saturator off: the input
        let dry = peak(DiffParams::default());
        assert!((dry - 0.3).abs() < 0.01, "{dry}");
        // SAT GAIN raises it, SAT MIX at 0 takes the shaping (and the gain's reach on the shaped part) away
        let gained = peak(DiffParams { drive: 30.0, sat_gain: 12.0, ..DiffParams::default() });
        assert!(gained > 0.9, "{gained}");
        assert!(peak(DiffParams { drive: 30.0, sat_gain: 12.0, sat_mix: 0.0, ..DiffParams::default() }) < gained * 0.5);
        // a hard-clipped, hot signal peaks near the clip level: the meter sees the clipped output, not the input
        let hot = peak(DiffParams { shape: 4, drive: 80.0, ..DiffParams::default() });
        assert!(hot > 0.85 && hot < 1.2, "{hot}");
        // over 1.0 when the SAT stage is pushed past full scale
        assert!(peak(DiffParams { shape: 4, drive: 80.0, sat_gain: 12.0, ..DiffParams::default() }) > 1.0);
        // OUT GAIN comes after the sat stage: it does not move the meter
        assert!((peak(DiffParams { drive: 30.0, out_gain: 12.0, ..DiffParams::default() }) - peak(DiffParams { drive: 30.0, ..DiffParams::default() })).abs() < 0.01);
    }

    fn run_peak(p: &DiffParams, x: &[f32]) -> (Vec<f32>, f64) {
        let mut e = DiffEngine::new(SR);
        e.set_params(p);
        let (mut l, mut r) = (x.to_vec(), x.to_vec());
        let mut pk = 0.0f64;
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
            pk = pk.max(e.ipk[0]);
            e.ipk = [0.0; 2];
        }
        (l, pk)
    }

    /// Peak |output| while BIAS is dragged from 0 to `to` % over 150 ms (one step per 64-sample block) on `x`, then held.
    fn bias_drag_peak(x: &[f32], to: f64, drive: f64, shape: i32, comp: bool) -> f64 {
        let mut e = DiffEngine::new(SR);
        e.dc_comp = comp;
        e.set_params(&DiffParams { shape, drive, ..DiffParams::default() });
        let (mut l, mut r) = (x.to_vec(), x.to_vec());
        let mut pk = 0.0f64;
        let lat = e.latency() as usize;
        for (k, (cl, cr)) in l.chunks_mut(64).zip(r.chunks_mut(64)).enumerate() {
            let t = (k as f64 * 64.0 / (0.15 * SR)).min(1.0);
            e.set_params(&DiffParams { shape, drive, bias: to * t, ..DiffParams::default() });
            e.process(cl, cr);
            if k * 64 > 2 * lat {
                pk = cl.iter().fold(pk, |m, v| m.max(v.abs() as f64));
            }
        }
        pk
    }

    #[test]
    fn dragging_bias_in_silence_does_not_bump() {
        let silence = vec![0.0f32; 48000];
        for shape in [0, 1, 4, 7, 10, 13] {
            for to in [10.0, 40.0, -40.0] {
                let on = bias_drag_peak(&silence, to, 0.0, shape, true);
                let off = bias_drag_peak(&silence, to, 0.0, shape, false);
                assert!(on < 0.01, "shape {shape} bias {to}: {on} (without compensation {off})");
                eprintln!("shape {shape} bias {to}: on {on:.4} off {off:.4}");
            }
        }
        // and with a quiet tone going: the bump must not grow beyond the tone itself
        let tone: Vec<f32> = (0..48000).map(|i| (0.05 * (2.0 * PI * 440.0 * i as f64 / SR).sin()) as f32).collect();
        let on = bias_drag_peak(&tone, 40.0, 20.0, 1, true);
        let base = bias_drag_peak(&tone, 0.0, 20.0, 1, true);
        assert!(on < base * 1.5 + 0.02, "tone: {on} vs {base}");
    }

    fn tone(n: usize, amp: f64) -> Vec<f32> {
        (0..n).map(|i| (amp * (2.0 * PI * 180.0 * i as f64 / SR).sin()) as f32).collect()
    }

    fn scenes(depth: f64, dir: i32) -> DiffParams {
        // scene A = Soft, scene B = Hard clip, both driven
        DiffParams { shape: 1, shape_b: 4, drive: 45.0, morph_depth: depth, morph_dir: dir, ..DiffParams::default() }
    }

    #[test]
    fn the_ab_switch_is_a_view_only_and_depth_zero_is_scene_a() {
        let x = tone(20000, 0.4);
        let (a, _) = run(&scenes(0.0, 2), &x);
        let (a_ref, _) = run(&DiffParams { shape: 1, drive: 45.0, ..DiffParams::default() }, &x);
        assert_eq!(a, a_ref, "no morph must be exactly the plain scene A shaper");
        // editing scene B changes nothing that is heard
        let (a_viewing_b, _) = run(&DiffParams { ab: 1, ..scenes(0.0, 2) }, &x);
        assert_eq!(a, a_viewing_b);
        // scene B is only ever heard through MORPH: MIX at 100 % is B alone
        let (b_ref, _) = run(&DiffParams { shape: 4, drive: 45.0, ..DiffParams::default() }, &x);
        let (b, _) = run(&scenes(100.0, 2), &x);
        let d = a.iter().zip(&b).map(|(p, q)| (p - q).abs()).fold(0.0f32, f32::max);
        assert!(d > 0.01);
        let db = b.iter().zip(&b_ref).map(|(p, q)| (p - q).abs()).fold(0.0f32, f32::max);
        assert!(db < 1e-5, "{db}");
    }

    #[test]
    fn mix_crossfades_the_two_scenes_statically() {
        let x = tone(20000, 0.4);
        let (a, _) = run(&scenes(0.0, 2), &x);
        let (b, _) = run(&DiffParams { shape: 4, drive: 45.0, ..DiffParams::default() }, &x);
        let (full, _) = run(&scenes(100.0, 2), &x);
        let maxd = |p: &[f32], q: &[f32]| p.iter().zip(q).map(|(u, v)| (u - v).abs()).fold(0.0f32, f32::max);
        assert!(maxd(&full, &b) < 1e-5, "MIX 100 % is scene B: {}", maxd(&full, &b));
        let (half, _) = run(&scenes(50.0, 2), &x);
        let avg: Vec<f32> = a.iter().zip(&b).map(|(u, v)| 0.5 * (u + v)).collect();
        assert!(maxd(&half, &avg) < 2e-3, "MIX 50 % is the average: {}", maxd(&half, &avg));
        assert!(maxd(&half, &a) > 0.01 && maxd(&half, &b) > 0.01);
        // scene A is still what the saturator is edited against when ab says B: MIX does not depend on ab
        let (half_b, _) = run(&DiffParams { ab: 1, ..scenes(50.0, 2) }, &x);
        assert!(maxd(&half, &half_b) < 1e-5);
    }

    #[test]
    fn the_detector_moves_the_blend_with_the_level_in_both_directions() {
        // quiet 0.5 s, loud 0.5 s, quiet again
        let mut x = tone(72000, 0.02);
        for (i, v) in x.iter_mut().enumerate().skip(24000).take(24000) {
            *v = (0.6 * (2.0 * PI * 180.0 * i as f64 / SR).sin()) as f32;
        }
        let pos = |dir: i32, speed: f64| {
            let mut e = DiffEngine::new(SR);
            e.set_params(&DiffParams { morph_speed: speed, ..scenes(100.0, dir) });
            let (mut l, mut r) = (x.clone(), x.clone());
            let mut at = vec![];
            for (k, (cl, cr)) in l.chunks_mut(256).zip(r.chunks_mut(256)).enumerate() {
                e.process(cl, cr);
                if k % 94 == 90 {
                    at.push(e.morph_pos());
                }
            }
            at
        };
        let up = pos(0, 100.0);
        // end of the quiet part, end of the loud part, end of the second quiet part
        let (q1, loud, q2) = (up[0], up[1], up[2]);
        assert!(q1 < 0.2 && loud > 0.8 && q2 < 0.4, "A>B: {up:?}");
        let down = pos(1, 100.0);
        assert!(down[0] > 0.8 && down[1] < 0.2 && down[2] > 0.6, "B>A: {down:?}");
        // MIX ignores the detector entirely
        let mix = pos(2, 100.0);
        assert!(mix.iter().all(|&m| (m - 1.0).abs() < 1e-9), "{mix:?}");
        // depth scales the excursion and 0 switches the morph off
        let mut e = DiffEngine::new(SR);
        e.set_params(&scenes(0.0, 0));
        assert_eq!(e.morph_pos(), 0.0);
    }

    #[test]
    fn speed_sets_how_fast_the_detector_follows() {
        let x: Vec<f32> = (0..24000).map(|i| if i < 4800 { 0.0 } else { (0.6 * (2.0 * PI * 180.0 * i as f64 / SR).sin()) as f32 }).collect();
        let reach = |speed: f64| {
            let mut e = DiffEngine::new(SR);
            e.set_params(&DiffParams { morph_speed: speed, ..scenes(100.0, 0) });
            let (mut l, mut r) = (x.clone(), x.clone());
            for (cl, cr) in l.chunks_mut(32).zip(r.chunks_mut(32)) {
                e.process(cl, cr);
                if e.morph_pos() > 0.7 {
                    return 1;
                }
            }
            0
        };
        assert_eq!(reach(100.0), 1, "fast speed gets there");
        assert_eq!(reach(0.0), 1, "slow gets there too");
        // time to reach 90 % of the way: slow takes clearly longer than fast
        let t50 = |speed: f64| {
            let mut e = DiffEngine::new(SR);
            e.set_params(&DiffParams { morph_speed: speed, ..scenes(100.0, 0) });
            let (mut l, mut r) = (x.clone(), x.clone());
            for (k, (cl, cr)) in l.chunks_mut(32).zip(r.chunks_mut(32)).enumerate() {
                e.process(cl, cr);
                if e.morph_pos() > 0.9 {
                    return k;
                }
            }
            usize::MAX
        };
        let (fast, slow) = (t50(100.0), t50(0.0));
        // (the level steps up at block 150)
        let (fast, slow) = (fast.saturating_sub(150).max(1), slow.saturating_sub(150));
        assert!(slow > fast * 10, "fast {fast} blocks, slow {slow} blocks after the step");
    }

    #[test]
    fn a_morph_blend_does_not_thump_when_it_moves() {
        // drag DEPTH of a MIX between two scenes with very different DC (Rectify vs Soft) in silence plus a quiet tone
        let x = tone(48000, 0.05);
        let mut e = DiffEngine::new(SR);
        let base = DiffParams { shape: 1, shape_b: 10, drive: 40.0, bias: 20.0, morph_dir: 2, ..DiffParams::default() };
        e.set_params(&base);
        let (mut l, mut r) = (x.clone(), x.clone());
        let mut pk = 0.0f64;
        for (k, (cl, cr)) in l.chunks_mut(64).zip(r.chunks_mut(64)).enumerate() {
            let t = (k as f64 * 64.0 / (0.2 * SR)).min(1.0);
            e.set_params(&DiffParams { morph_depth: 100.0 * t, ..base });
            e.process(cl, cr);
            if k > 20 {
                pk = cl.iter().fold(pk, |m, v| m.max(v.abs() as f64));
            }
        }
        // the tone is 0.05 through a heavily driven shaper: a clean crossfade stays within the shaper's own range
        assert!(pk.is_finite() && pk < 1.5, "{pk}");
    }
}
