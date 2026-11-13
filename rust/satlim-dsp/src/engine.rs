//! SatLim engine: SAT (shared saturator, optionally oversampled) -> level into the ceiling (`ceiling - threshold`) ->
//! CLIP (oversampled hard clip at the ceiling, driven up to +6 dB into it) -> true-peak brickwall limiter -> OUT GAIN.
//!
//! OUT GAIN is after the limiter on purpose: the ceiling is what the limiter holds, the output gain is the owner's to
//! trim (a clipped -0.4 dBTP still has inter-sample peaks that a downstream converter may see). The reported latency is
//! always the sum of the saturator oversampler, the clipper oversampler and the limiter lookahead (0 with L.AHD off).
//! Stereo, f64 inside.

use crate::params::LimParams;
use satcomp_dsp::delay::StereoDelay;
use satcomp_dsp::halfband::{self, Oversampler, Taps};
use satcomp_dsp::level::{ms_step, one_pole, Matcher, DB_AMP, LN10_20};
use satcomp_dsp::{Limiter, SatParams, Saturator, ScopeRing, TruePeak};
use std::sync::Arc;

/// Delay ring length: larger than the longest latency (30 + 30 + the limiter's 32 + 952 at the highest rates).
const RING: usize = 2048;

pub struct LimEngine {
    sr: f64,
    sr_prev: f64,
    p: LimParams,
    dirty: bool,
    first_set: bool,
    snap: bool,
    /// Smooth Drive / Bias / Mix (set false for null tests).
    pub smoothing: bool,
    sat: Saturator,
    taps: Taps,
    ov: [Oversampler; 2],
    lim: Limiter,
    tp: TruePeak,
    /// the signal entering the clipper, delayed to line up with the limiter output (for the delta and the scope)
    pre: StereoDelay<RING>,
    dry: StereoDelay<RING>,
    thr_g: f64,
    thr_t: f64,
    og: f64,
    og_t: f64,
    byp_s: f64,
    sm_c: f64,
    byp_c: f64,
    rms_c: f64,
    ceil: f64,
    clip_k: f64,
    clip_on: bool,
    os: i32,
    /// monitor what the clipper + limiter removed (UI -> audio)
    pub delta_on: bool,
    pub scope: Arc<ScopeRing>,
    sc_len: usize,
    sc_n: usize,
    sc_o: f64,
    sc_m: f64,
    sc_p: f64,
    // telemetry, audio -> UI. The shell takes the peaks with fetch_max and clears them.
    /// output peak per channel (after OUT GAIN), linear
    pub opk: [f64; 2],
    /// output true peak (also while L.AHD is off), linear
    pub tpk: f64,
    /// post-saturator (after SAT GAIN) peak, linear
    pub sat_out: f64,
    /// decaying peak of the saturator input for the plot's live dot
    pub sat_vz: f64,
    /// limiter gain reduction peak, dB positive
    pub gr_pk: f64,
    /// what the clipper took off the loudest peak, dB positive (0 with CLIP off)
    pub clip_gr_pk: f64,
    /// smoothed output mean-square (RMS readout)
    pub fms: f64,
    /// loudness change across the saturator (before SAT GAIN), dB: the plot's SAT readout
    pub sat_db: f64,
    ims: f64,
    sat_m: Matcher,
    sat_req: bool,
    sat_result: Option<f64>,
    pub clip_f: bool,
}

impl LimEngine {
    pub fn new(sr: f64) -> Self {
        let mut e = Self {
            sr,
            sr_prev: 0.0,
            p: LimParams::default(),
            dirty: true,
            first_set: true,
            snap: true,
            smoothing: true,
            sat: Saturator::new(),
            taps: Taps::new(),
            ov: [Oversampler::default(), Oversampler::default()],
            lim: {
                // 8x detection: what reaches this limiter is clipped / heavily shaped, all inter-sample peaks
                let mut l = Limiter::new(sr);
                l.set_hi(true);
                l
            },
            tp: TruePeak::with_precision(sr, true),
            pre: StereoDelay::new(),
            dry: StereoDelay::new(),
            thr_g: 1.0,
            thr_t: 1.0,
            og: 1.0,
            og_t: 1.0,
            byp_s: 0.0,
            sm_c: 0.0,
            byp_c: 0.0,
            rms_c: 0.0,
            ceil: 1.0,
            clip_k: 1.0,
            clip_on: false,
            os: 0,
            delta_on: false,
            scope: Arc::new(ScopeRing::new()),
            sc_len: 96,
            sc_n: 0,
            sc_o: 0.0,
            sc_m: 0.0,
            sc_p: 0.0,
            opk: [0.0; 2],
            tpk: 0.0,
            sat_out: 0.0,
            sat_vz: 0.0,
            gr_pk: 0.0,
            clip_gr_pk: 0.0,
            fms: 1e-9,
            sat_db: 0.0,
            ims: 1e-9,
            sat_m: Matcher::default(),
            sat_req: false,
            sat_result: None,
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

    pub fn set_params(&mut self, p: &LimParams) {
        if self.first_set {
            // a fresh engine starts at the host's settings instead of gliding there from defaults
            self.first_set = false;
            self.p = *p;
            self.sat.set_out_db(p.sat_gain);
            self.lim.snap_on(true);
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

    /// PDC latency to report: saturator oversampler + clipper oversampler + limiter lookahead.
    pub fn latency(&self) -> u32 {
        (self.sat.latency() + halfband::latency(self.os) + self.lim.latency()) as u32
    }

    fn derive(&mut self) {
        let p = self.p;
        let sr = self.sr;
        self.dirty = false;
        if sr != self.sr_prev {
            self.sr_prev = sr;
            self.lim.set_sample_rate(sr);
            self.tp = TruePeak::with_precision(sr, true);
            self.sc_len = ((sr * ScopeRing::BUCKET_S).round() as usize).max(1);
            self.sm_c = one_pole(sr, 0.02);
            self.byp_c = one_pole(sr, 0.005);
            self.rms_c = one_pole(sr, 0.3);
        }
        let sp: SatParams = p.sat();
        self.sat.derive(&sp, sr, self.snap, self.smoothing);
        self.os = p.os.clamp(0, 3);
        self.ceil = 10f64.powf(p.ceiling / 20.0);
        self.lim.set_ceiling_db(p.ceiling);
        self.lim.set_release_ms(p.release);
        self.lim.set_clip(false); // the clipper is this engine's own (oversampled)
        self.lim.set_lookahead(p.lookahead);
        self.clip_on = p.clip > 0.0;
        self.clip_k = 10f64.powf(p.clip * 0.01 * 6.0 / 20.0);
        self.thr_t = ((p.ceiling - p.threshold) * LN10_20).exp();
        self.og_t = (p.out_gain * LN10_20).exp();
        if self.snap {
            self.thr_g = self.thr_t;
            self.og = self.og_t;
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

    pub fn begin_block(&mut self, n: usize) {
        if self.dirty || self.sr != self.sr_prev {
            self.derive();
        }
        self.sat.begin_block();
        let vz_d = (-(n as f64) / (self.sr * 0.15)).exp();
        self.sat_vz = self.sat.blk.max(self.sat_vz * vz_d);
        self.sat.blk = 0.0;
        // loudness change across the saturator, and the 3 s level match that writes SAT GAIN (only while there is signal)
        let gate = self.ims > 0.000001;
        if gate {
            self.sat_db = 4.3429448 * (self.sat.ms.max(1e-12) / self.ims.max(1e-12)).ln();
        }
        if self.sat_req {
            let was = self.sat_m.active;
            self.sat_m.start();
            if was {
                self.sat_m.stop(); // a second click cancels
            }
            self.sat_req = false;
        }
        if self.sat_m.active && gate {
            if let Some(avg) = self.sat_m.step(self.sat_db, n as f64 / self.sr) {
                let v = (avg.clamp(-24.0, 12.0) * 10.0 + 0.5).floor() * 0.1;
                self.sat.set_out_db(v);
                self.sat_result = Some(v);
            }
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
        let g = self.lim.gmin.max(1e-6);
        self.gr_pk = self.gr_pk.max(-g.ln() * DB_AMP);
        self.lim.gmin = 1.0;
    }

    #[inline]
    pub fn tick(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        let inp = [in_l, in_r];
        self.dry.advance();
        self.dry.put(inp);
        self.pre.advance();

        // SAT, then the level that maps the threshold onto the ceiling
        let s = self.sat.tick(inp, self.smoothing);
        ms_step(&mut self.ims, inp[0], inp[1], self.rms_c);
        self.sat_out = self.sat_out.max(s[0].abs().max(s[1].abs()));
        self.thr_g += (self.thr_t - self.thr_g) * self.sm_c;
        let t = [s[0] * self.thr_g, s[1] * self.thr_g];
        self.pre.put(t);

        // CLIP: hard clip at the ceiling in the oversampled domain (always through the filters: constant latency)
        let (k, c, on) = (self.clip_k, self.ceil, self.clip_on);
        let mut cl = [0.0f64; 2];
        for ch in 0..2 {
            cl[ch] = self.ov[ch].process_fn(&self.taps, self.os, t[ch], |v| if on { (v * k).clamp(-c, c) } else { v });
        }

        // what the clipper took off: how far the pre-clip peak stood above the clipped one at the same instant, counted
        // only above the ceiling. Oversampled, the two also differ by the filters alone (near a zero crossing of the
        // clipped signal that ratio is anything), so the clipped side never counts as lower than the ceiling. At x1 a
        // clipped sample is exactly the ceiling: the same number as the plain ratio.
        if on {
            let td = self.pre.tap(halfband::latency(self.os));
            let (pt, pc) = (td[0].abs().max(td[1].abs()), cl[0].abs().max(cl[1].abs()).max(c));
            if pt > pc * 1.0001 {
                self.clip_gr_pk = self.clip_gr_pk.max(8.6858896 * (pt / pc).ln());
            }
        }

        // LIMITER
        let lo = self.lim.tick(cl, true);
        let lat_pre = halfband::latency(self.os) + self.lim.latency();
        let pd = self.pre.tap(lat_pre);
        self.og += (self.og_t - self.og) * self.sm_c;
        let y = [lo[0] * self.og, lo[1] * self.og];

        // monitors and bypass
        let removed = [pd[0] - lo[0], pd[1] - lo[1]];
        let proc = if self.delta_on { removed } else { y };
        self.byp_s += ((if self.p.bypass { 1.0 } else { 0.0 }) - self.byp_s) * self.byp_c;
        let dd = self.dry.tap(self.sat.latency() + lat_pre);
        let out = if self.byp_s > 1e-9 { [proc[0] + (dd[0] - proc[0]) * self.byp_s, proc[1] + (dd[1] - proc[1]) * self.byp_s] } else { proc };

        // telemetry
        self.opk[0] = self.opk[0].max(out[0].abs());
        self.opk[1] = self.opk[1].max(out[1].abs());
        if out[0].abs() > 0.9999 || out[1].abs() > 0.9999 {
            self.clip_f = true;
        }
        // true peak at all times: with L.AHD off the limiter only holds the sample peaks, and the readout must show what got past
        let pkv = self.tp.tick(out);
        self.tpk = self.tpk.max(pkv);
        ms_step(&mut self.fms, out[0], out[1], self.rms_c);
        let ao = out[0].abs().max(out[1].abs());
        // scope: output, the peak the limiter was handed (after the clipper), the peak before the clipper
        let am = self.lim.last_in[0].abs().max(self.lim.last_in[1].abs()) * self.og;
        self.sc_o = self.sc_o.max(ao);
        self.sc_m = self.sc_m.max(am).max(ao);
        self.sc_p = self.sc_p.max((pd[0].abs().max(pd[1].abs())) * self.og).max(am).max(ao);
        self.sc_n += 1;
        if self.sc_n >= self.sc_len {
            self.scope.push3(self.sc_o as f32, self.sc_m as f32, self.sc_p as f32);
            self.sc_o = 0.0;
            self.sc_m = 0.0;
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

    fn run(p: &LimParams, x: &[f32]) -> (Vec<f32>, LimEngine) {
        let mut e = LimEngine::new(SR);
        e.set_params(p);
        let (mut l, mut r) = (x.to_vec(), x.to_vec());
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
        }
        (l, e)
    }

    fn db(x: f64) -> f64 {
        20.0 * x.max(1e-12).log10()
    }

    #[test]
    fn quiet_signal_is_an_exact_delayed_copy_at_x1() {
        for look in [true, false] {
            let p = LimParams { ceiling: 0.0, lookahead: look, ..LimParams::default() };
            let x = noise(20000, 3, 0.4);
            let (y, e) = run(&p, &x);
            let lat = e.latency() as usize;
            assert_eq!(lat, if look { 104 } else { 0 });
            for i in lat..x.len() {
                assert_eq!(y[i], x[i - lat], "look {look} sample {i}");
            }
        }
    }

    #[test]
    fn latency_adds_the_saturator_the_clipper_and_the_limiter() {
        for (os, extra) in [(0, 0), (1, 48), (2, 56), (3, 60)] {
            let mut e = LimEngine::new(SR);
            e.set_params(&LimParams { os, ..LimParams::default() });
            assert_eq!(e.latency() as usize, 104 + extra, "os {os}");
            e.set_params(&LimParams { os, lookahead: false, ..LimParams::default() });
            assert_eq!(e.latency() as usize, extra, "os {os}, no lookahead");
        }
    }

    #[test]
    fn bypass_is_the_input_at_the_reported_latency() {
        let p = LimParams { os: 2, drive: 60.0, threshold: -12.0, clip: 80.0, bypass: true, ..LimParams::default() };
        let x = noise(20000, 5, 0.5);
        let (y, e) = run(&p, &x);
        let lat = e.latency() as usize;
        for i in lat + 100..x.len() {
            assert!((y[i] - x[i - lat]).abs() < 1e-6, "sample {i}");
        }
    }

    #[test]
    fn the_output_holds_the_ceiling_before_out_gain() {
        // hot programme, threshold -14 lifts it 13 dB into a -1 dB ceiling
        for (os, look, clip) in [(0, true, 0.0), (2, true, 60.0), (0, false, 0.0), (3, false, 100.0)] {
            let p = LimParams { ceiling: -1.0, threshold: -14.0, os, lookahead: look, clip, release: 40.0, ..LimParams::default() };
            let x = noise(48000, 11, 0.9);
            let (y, e) = run(&p, &x);
            let pk = y[4000..].iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
            // sample peaks: exact with the lookahead off; the true-peak limiter keeps the interpolated peaks near the ceiling
            // and the filtered clipper can ring a little over it
            assert!(db(pk) < -1.0 + 0.3, "os {os} look {look} clip {clip}: peak {:.2} dB", db(pk));
            // (with the lookahead off only the sample peaks are held: the readout is still the true peak, and reads over)
            assert!(!look || db(e.tpk) < -1.0 + 0.6, "os {os} look {look} clip {clip}: true peak {:.2} dB", db(e.tpk));
            assert!(db(pk) > -2.5, "it limits close to the ceiling, not far below it: {:.2}", db(pk));
        }
    }

    #[test]
    fn out_gain_is_after_the_ceiling() {
        let p = LimParams { ceiling: -1.0, threshold: -10.0, out_gain: -3.0, ..LimParams::default() };
        let x: Vec<f32> = (0..48000).map(|i| (0.9 * (2.0 * PI * 1000.0 * i as f64 / SR).sin()) as f32).collect();
        let (y, _) = run(&p, &x);
        let pk = y[4000..].iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
        assert!(db(pk) < -4.0 + 0.2 && db(pk) > -4.4, "{:.2}", db(pk));
    }

    #[test]
    fn clip_takes_the_spikes_so_the_limiter_ducks_less() {
        let mut x = vec![0.0f32; 48000];
        for (i, v) in x.iter_mut().enumerate() {
            let t = i as f64 / SR;
            *v = (0.3 * (2.0 * PI * 200.0 * t).sin()) as f32;
            if i % 4800 == 2400 {
                *v += 3.0;
            }
        }
        let base = LimParams { ceiling: -1.0, lookahead: true, ..LimParams::default() };
        let (_, off) = run(&LimParams { clip: 0.0, ..base }, &x);
        let (_, on) = run(&LimParams { clip: 100.0, ..base }, &x);
        assert!(off.gr_pk > 6.0, "limiter alone ducks on the spikes: {}", off.gr_pk);
        assert!(on.gr_pk < off.gr_pk * 0.6, "clip on {} vs off {}", on.gr_pk, off.gr_pk);
    }

    #[test]
    fn delta_is_what_the_clipper_and_limiter_removed() {
        let x = noise(24000, 17, 0.9);
        let p = LimParams { ceiling: -1.0, threshold: -12.0, clip: 50.0, ..LimParams::default() };
        let mut e = LimEngine::new(SR);
        e.set_params(&p);
        e.delta_on = true;
        let (mut l, mut r) = (x.clone(), x.clone());
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
        }
        let rms = |v: &[f32]| (v[4000..].iter().map(|a| (*a as f64).powi(2)).sum::<f64>() / (v.len() - 4000) as f64).sqrt();
        assert!(rms(&l) > 0.05, "heavy limiting leaves a real delta: {}", rms(&l));
        // nothing to remove on a quiet signal
        let q = noise(24000, 19, 0.05);
        let mut e = LimEngine::new(SR);
        e.set_params(&LimParams { ceiling: 0.0, ..LimParams::default() });
        e.delta_on = true;
        let (mut l, mut r) = (q.clone(), q.clone());
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
        }
        assert!(rms(&l) < 1e-9, "{}", rms(&l));
    }

    #[test]
    fn telemetry_reads_the_stages() {
        let p = LimParams { ceiling: -1.0, threshold: -10.0, drive: 30.0, ..LimParams::default() };
        let x = noise(24000, 23, 0.8);
        let (_, e) = run(&p, &x);
        assert!(e.gr_pk > 3.0, "{}", e.gr_pk);
        assert!(e.sat_out > 0.2 && e.opk[0] > 0.5 && e.tpk >= e.opk[0] as f64 - 1e-9, "{} {:?} {}", e.sat_out, e.opk, e.tpk);
        assert!(e.scope.head() > 100);
    }

    #[test]
    fn sat_match_measures_for_three_seconds_then_reports_the_gain_that_cancels_the_change() {
        let p = LimParams { drive: 60.0, ceiling: 0.0, ..LimParams::default() };
        let mut e = LimEngine::new(SR);
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
        assert!(v < -1.0 && v > -24.0, "drive 60 on noise is louder, the match turns it down: {v}");
        assert!((v + e.sat_db).abs() < 0.6, "match {v} vs measured change {}", e.sat_db);
        assert!(e.take_sat_result().is_none());
    }

    #[test]
    fn the_clipper_and_the_limiter_report_their_own_reduction() {
        let mut x = vec![0.0f32; 48000];
        for (i, v) in x.iter_mut().enumerate() {
            *v = (0.3 * (2.0 * PI * 200.0 * i as f64 / SR).sin()) as f32;
            if i % 4800 == 2400 {
                *v += 3.0;
            }
        }
        let base = LimParams { ceiling: -1.0, ..LimParams::default() };
        let (_, off) = run(&LimParams { clip: 0.0, ..base }, &x);
        let (_, on) = run(&LimParams { clip: 100.0, ..base }, &x);
        assert!(off.clip_gr_pk < 0.01, "no clip, no clip reduction: {}", off.clip_gr_pk);
        assert!(off.gr_pk > 6.0);
        assert!(on.clip_gr_pk > 6.0, "the clipper takes the spike: {}", on.clip_gr_pk);
        assert!(on.gr_pk < off.gr_pk * 0.6, "so the limiter has less to do: {} vs {}", on.gr_pk, off.gr_pk);
    }

    #[test]
    fn the_clip_reduction_readout_stays_sane_when_oversampled() {
        // spikes of 3.3 into a -1 dB ceiling: the clipper takes about 11 dB at every oversampling setting
        let mut x = vec![0.0f32; 48000];
        for (i, v) in x.iter_mut().enumerate() {
            *v = (0.3 * (2.0 * PI * 200.0 * i as f64 / SR).sin()) as f32;
            if i % 4800 == 2400 {
                *v += 3.0;
            }
        }
        for os in 0..4 {
            let (_, e) = run(&LimParams { ceiling: -1.0, clip: 100.0, os, ..LimParams::default() }, &x);
            assert!(e.clip_gr_pk > 5.0 && e.clip_gr_pk < 11.6, "os {os}: {}", e.clip_gr_pk);
        }
        // full-band noise far over the ceiling: never more than the input stood above it (peak 0.9 * 11 dB lift = 11.1 dB over)
        let n = noise(48000, 31, 0.9);
        for os in 0..4 {
            let (_, e) = run(&LimParams { ceiling: -1.0, threshold: -12.0, clip: 100.0, os, ..LimParams::default() }, &n);
            assert!(e.clip_gr_pk > 6.0 && e.clip_gr_pk < 11.2, "noise os {os}: {}", e.clip_gr_pk);
        }
    }

    #[test]
    fn switching_the_lookahead_on_mid_stream_lets_nothing_over_the_ceiling() {
        let p0 = LimParams { ceiling: -1.0, threshold: -12.0, lookahead: false, ..LimParams::default() };
        let mut e = LimEngine::new(SR);
        e.set_params(&p0);
        let mut l: Vec<f32> = (0..48000).map(|i| (0.9 * (2.0 * PI * 1000.0 * i as f64 / SR).sin()) as f32).collect();
        let mut r = l.clone();
        let mut pk = 0.0f32;
        for (i, (a, b)) in l.chunks_mut(64).zip(r.chunks_mut(64)).enumerate() {
            if i == 375 {
                e.set_params(&LimParams { lookahead: true, ..p0 });
            }
            e.process(a, b);
            pk = a.iter().fold(pk, |m, v| m.max(v.abs()));
        }
        assert!(db(pk as f64) <= -1.0 + 1e-3, "peak {:.2} dB", db(pk as f64));
    }

    #[test]
    fn the_peak_readout_is_true_peak_with_the_lookahead_off_too() {
        // fs/4 tone sampled 45 degrees off its crests: the sample-peak limiter holds the samples at the ceiling, the true peak is 3 dB above
        let x: Vec<f32> = (0..24000).map(|i| (2.0 * (PI * 0.5 * i as f64 + PI / 4.0).sin()) as f32).collect();
        let (y, e) = run(&LimParams { ceiling: -1.0, lookahead: false, ..LimParams::default() }, &x);
        let pk = y[4000..].iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;
        assert!(db(pk) <= -1.0 + 1e-3, "samples held: {:.2}", db(pk));
        assert!(db(e.tpk) > 1.5, "the readout shows the inter-sample over: {:.2}", db(e.tpk));
    }

    #[test]
    fn the_scope_buckets_are_ordered_output_then_limiter_then_clipper() {
        let mut x = vec![0.0f32; 24000];
        for (i, v) in x.iter_mut().enumerate() {
            *v = (0.4 * (2.0 * PI * 300.0 * i as f64 / SR).sin()) as f32;
            if i % 2400 == 1200 {
                *v += 2.5;
            }
        }
        for (clip, want_lim, want_clip) in [(0.0, true, false), (100.0, false, true)] {
            let (_, e) = run(&LimParams { ceiling: -1.0, clip, ..LimParams::default() }, &x);
            let head = e.scope.head();
            let (mut saw_lim, mut saw_clip) = (false, false);
            for i in 20..head {
                let (o, m, p) = e.scope.get3(i);
                assert!(o <= m + 1e-6 && m <= p + 1e-6, "clip {clip} bucket {i}: {o} {m} {p}");
                saw_lim |= m > o * 1.05;
                saw_clip |= p > m * 1.05;
            }
            assert_eq!((saw_lim, saw_clip), (want_lim, want_clip), "clip {clip}");
        }
    }
}
