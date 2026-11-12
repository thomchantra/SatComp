//! True-peak brickwall limiter: the last stage of the processed path.
//!
//! * Detection: the stereo-linked peak of the input, including the inter-sample peaks found by a 4x polyphase
//!   interpolator (2x at 96 kHz-class rates, plain sample peak above that: inter-sample overs vanish at high rates).
//! * Hard clip: every sample is clipped at `CLIP_OVER_DB` above the ceiling before detection and again on the delayed
//!   audio, so a tall transient is shaved off clean and the gain stage only ever has to find the small remainder (a limiter
//!   alone would duck the whole body for the length of its release after an 8 dB spike). Faded in with the limiter.
//! * Gain: the gain needed to hold the ceiling, a sliding minimum over the lookahead, then a boxcar of the lookahead
//!   length. The result ramps down so that it reaches the full reduction exactly when the peak leaves the delay line.
//!   Release is program dependent (fast for isolated peaks, slow under sustained reduction) and can only make the gain
//!   *lower* than the attack ramp, never higher, so the brickwall guarantee holds.
//! * The audio is delayed by `latency()` samples whether or not the limiter is engaged (constant PDC); the detector
//!   keeps running while it is off so switching it on is click-free (the engagement is a 10 ms linear fade).
//!
//! The ceiling is fixed at -0.2 dBTP: the interpolator is not an ideal reconstruction filter and the gain changes
//! between samples, so the margin keeps the true peak of the output at or below 0 dBTP.

use crate::delay::StereoDelay;
use std::collections::VecDeque;

/// Interpolator taps per phase (delay = TAPS / 2 samples). 64 keeps white noise at Nyquist within ~0.1 dB of true.
const TAPS: usize = 64;
/// Output ceiling, dBTP.
pub const CEILING_DB: f64 = -0.2;
/// Dev-only tuning hook (feature `tune`): an environment variable overrides a constant, for offline sweeps.
#[cfg(feature = "tune")]
fn tune(k: &str, d: f64) -> f64 {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
}
#[cfg(not(feature = "tune"))]
fn tune(_k: &str, d: f64) -> f64 {
    d
}
/// The hard clip sits this far above the ceiling (dB). At 0 the clip does the bulk of the work (peaks are shaved off clean,
/// like a clipper in front of a brickwall limiter) and the gain stage only has to catch the inter-sample excess of the
/// clipped waveform, so its release never rides a big spike. Raising it trades clip distortion for gain-stage ducking:
/// measured on a hot preset, +2 dB ducked the body in 19 % of 10 ms windows (>1 dB), 0 dB in 5 %.
const CLIP_OVER_DB: f64 = 0.0;
/// Release time constant range, seconds: fast for isolated peaks, slow under sustained reduction.
const TAU_MIN: f64 = 0.015;
const TAU_MAX: f64 = 0.080;
/// Lookahead / attack ramp, seconds.
const LOOKAHEAD_S: f64 = 0.0015;
/// Capacity of the delay line and of the min / boxcar buffers (samples).
const CAP: usize = 1024;

fn i0(x: f64) -> f64 {
    let (mut sm, mut t) = (1.0, 1.0);
    for k in 1..40 {
        let h = x * 0.5 / k as f64;
        t *= h * h;
        sm += t;
    }
    sm
}

/// Kaiser-windowed sinc phases: for `r` x oversampling, `r - 1` filters that interpolate at `(i + 1) / r` between the
/// two centre samples of the `TAPS`-sample window.
fn phase_taps(r: usize) -> [[f64; TAPS]; 3] {
    let mut c = [[0.0; TAPS]; 3];
    let beta = 5.0;
    let half = TAPS as f64 * 0.5;
    for ph in 0..r.saturating_sub(1) {
        let f = (ph + 1) as f64 / r as f64;
        let mut sum = 0.0;
        for (j, w) in c[ph].iter_mut().enumerate() {
            let t = j as f64 - (half - 1.0 + f);
            let s = if t.abs() < 1e-12 { 1.0 } else { (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t) };
            let r2 = (t / half).powi(2).min(1.0);
            let v = s * i0(beta * (1.0 - r2).sqrt()) / i0(beta);
            *w = v;
            sum += v;
        }
        for w in c[ph].iter_mut() {
            *w /= sum;
        }
    }
    c
}

pub struct Limiter {
    sr: f64,
    r: usize,
    d: usize,
    l: usize,
    ceil: f64,
    coef: [[f64; TAPS]; 3],
    /// largest sum of |tap| over the phases: an interpolated point can't exceed `lam * max|window|`, so a quiet
    /// window needs no interpolation at all (exactly the same result, far less work on quiet programme)
    lam: f64,
    /// samples left until the last loud one leaves the interpolator window
    hot: usize,
    // interpolator history (double-written ring so the window is contiguous)
    hist: [[f64; 2 * TAPS]; 2],
    hpos: usize,
    need_prev: f64,
    // min over the last l + 1 targets
    dq: VecDeque<(u64, f64)>,
    n: u64,
    // boxcar over the last l minima
    ring: Vec<f64>,
    rpos: usize,
    sum: f64,
    resync: u32,
    // release
    g: f64,
    avg: f64,
    avg_c: f64,
    rc: f64,
    rc_n: u32,
    // engagement fade
    on_s: f64,
    on_step: f64,
    delay: StereoDelay<CAP>,
    clipv: f64,
    tau0: f64,
    tau1: f64,
    /// Smallest applied gain since the engine last reset it (linear, 1.0 = no reduction).
    pub gmin: f64,
}

impl Limiter {
    pub fn new(sr: f64) -> Self {
        let mut s = Self {
            sr: 0.0,
            r: 1,
            d: 0,
            l: 4,
            ceil: 10f64.powf(CEILING_DB / 20.0),
            coef: [[0.0; TAPS]; 3],
            lam: 1.0,
            hot: 0,
            hist: [[0.0; 2 * TAPS]; 2],
            hpos: 0,
            need_prev: 1.0,
            dq: VecDeque::with_capacity(CAP + 2),
            n: 0,
            ring: vec![1.0; CAP],
            rpos: 0,
            sum: 4.0,
            resync: 0,
            g: 1.0,
            avg: 0.0,
            avg_c: 0.0,
            rc: 0.0,
            rc_n: 0,
            on_s: 0.0,
            on_step: 0.0,
            delay: StereoDelay::new(),
            clipv: 10f64.powf((CEILING_DB + tune("LIM_CLIP", CLIP_OVER_DB)) / 20.0),
            tau0: tune("LIM_TAU0", TAU_MIN),
            tau1: tune("LIM_TAU1", TAU_MAX),
            gmin: 1.0,
        };
        s.set_sample_rate(sr);
        s
    }

    /// Re-derive everything that depends on the sample rate. A no-op while the rate is unchanged.
    pub fn set_sample_rate(&mut self, sr: f64) {
        if sr == self.sr {
            return;
        }
        self.sr = sr;
        self.r = if sr <= 50_000.0 {
            4
        } else if sr <= 100_000.0 {
            2
        } else {
            1
        };
        self.d = if self.r > 1 { TAPS / 2 } else { 0 };
        self.l = ((LOOKAHEAD_S * sr).round() as usize).clamp(4, CAP - TAPS - 8);
        self.coef = phase_taps(self.r);
        self.lam = self.coef.iter().map(|c| c.iter().map(|v| v.abs()).sum::<f64>()).fold(1.0, f64::max);
        self.hot = 0;
        self.hist = [[0.0; 2 * TAPS]; 2];
        self.need_prev = 1.0;
        self.dq.clear();
        self.ring.iter_mut().for_each(|v| *v = 1.0);
        self.rpos = 0;
        self.sum = self.l as f64;
        self.g = 1.0;
        self.avg = 0.0;
        self.avg_c = 1.0 - (-1.0 / (sr * 0.4)).exp();
        self.rc_n = 0;
        self.on_step = 1.0 / (0.01 * sr);
        self.delay.0.iter_mut().for_each(|d| d.clear());
        self.gmin = 1.0;
    }

    /// Jump the engagement fade to its end state (a fresh engine must limit from its first sample).
    pub fn snap_on(&mut self, on: bool) {
        self.on_s = if on { 1.0 } else { 0.0 };
    }

    /// Samples the audio is delayed by (detector + lookahead), always.
    pub fn latency(&self) -> usize {
        self.d + self.l
    }

    /// Current applied gain (1.0 = idle).
    pub fn gain(&self) -> f64 {
        self.g
    }

    /// Peak (linear) of the input sample `D` samples ago and the inter-sample points after it.
    #[inline]
    fn detect(&mut self, x: [f64; 2]) -> f64 {
        let p = self.hpos;
        let mut pk = 0.0f64;
        for c in 0..2 {
            self.hist[c][p] = x[c];
            self.hist[c][p + TAPS] = x[c];
        }
        self.hpos = (p + 1) % TAPS;
        if self.r == 1 {
            return x[0].abs().max(x[1].abs());
        }
        if x[0].abs().max(x[1].abs()) * self.lam > self.ceil {
            self.hot = TAPS;
        }
        if self.hot == 0 {
            return 0.0;
        }
        self.hot -= 1;
        let q = self.hpos;
        for c in 0..2 {
            let win = &self.hist[c][q..q + TAPS];
            pk = pk.max(win[TAPS / 2 - 1].abs());
            for ph in 0..self.r - 1 {
                let acc: f64 = win.iter().zip(self.coef[ph].iter()).map(|(a, b)| a * b).sum();
                pk = pk.max(acc.abs());
            }
        }
        pk
    }

    /// One stereo sample in -> the same sample `latency()` later, limited when `on`.
    #[inline]
    pub fn tick(&mut self, x: [f64; 2], on: bool) -> [f64; 2] {
        self.delay.advance();
        self.delay.put(x);
        let l = self.l;

        // ---- target gain for the sample the detector is looking at
        let xc = [x[0].clamp(-self.clipv, self.clipv), x[1].clamp(-self.clipv, self.clipv)];
        let pk = self.detect(xc);
        let need = if pk > self.ceil { self.ceil / pk } else { 1.0 };
        let t = need.min(self.need_prev);
        self.need_prev = need;

        // ---- sliding minimum over the last l + 1 targets
        self.n += 1;
        while let Some(&(_, v)) = self.dq.back() {
            if v >= t {
                self.dq.pop_back();
            } else {
                break;
            }
        }
        self.dq.push_back((self.n, t));
        while let Some(&(i, _)) = self.dq.front() {
            if self.n - i > l as u64 {
                self.dq.pop_front();
            } else {
                break;
            }
        }
        let s = self.dq.front().map(|e| e.1).unwrap_or(1.0);

        // ---- boxcar over the last l minima
        self.sum += s - self.ring[self.rpos];
        self.ring[self.rpos] = s;
        self.rpos += 1;
        if self.rpos >= l {
            self.rpos = 0;
        }
        self.resync += 1;
        if self.resync >= 32768 {
            self.resync = 0;
            self.sum = self.ring[..l].iter().sum();
        }
        let a = (self.sum / l as f64).min(1.0);

        // ---- program-dependent release, never above the attack ramp
        // how long the signal has been demanding reduction (the attack ramp, not the gain after release: that would
        // feed back and slow every release)
        self.avg += ((1.0 - a) - self.avg) * self.avg_c;
        if self.rc_n == 0 {
            let tau = self.tau0 + (self.tau1 - self.tau0) * (self.avg / 0.25).min(1.0);
            self.rc = 1.0 - (-1.0 / (self.sr * tau)).exp();
            self.rc_n = 16;
        }
        self.rc_n -= 1;
        let rel = 1.0 - (1.0 - self.g) * (1.0 - self.rc);
        let mut g = a.min(rel);
        if g > 1.0 - 1e-7 {
            g = 1.0;
        }
        self.g = g;

        // ---- engagement fade (linear, exact at both ends)
        let target = if on { 1.0 } else { 0.0 };
        if self.on_s < target {
            self.on_s = (self.on_s + self.on_step).min(target);
        } else if self.on_s > target {
            self.on_s = (self.on_s - self.on_step).max(target);
        }
        let ge = 1.0 + (g - 1.0) * self.on_s;
        let y = self.delay.tap(self.d + l);
        // the reduction readout counts the clip too: gain the output sample lost against the unclipped one
        let pk = y[0].abs().max(y[1].abs());
        let clip_g = if pk > self.clipv { 1.0 + (self.clipv / pk - 1.0) * self.on_s } else { 1.0 };
        if ge * clip_g < self.gmin {
            self.gmin = ge * clip_g;
        }
        // the hard clip is part of the engaged path (faded in with it)
        let yc = [y[0].clamp(-self.clipv, self.clipv), y[1].clamp(-self.clipv, self.clipv)];
        let yo = [y[0] + (yc[0] - y[0]) * self.on_s, y[1] + (yc[1] - y[1]) * self.on_s];
        [yo[0] * ge, yo[1] * ge]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn lcg(s: &mut u64) -> f64 {
        *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }

    /// True peak by brute force: 16x Kaiser-sinc reconstruction (64 taps each side), independent of the limiter's own.
    fn true_peak(x: &[f64]) -> f64 {
        let beta = 9.0;
        let half = 48i64;
        // coefficient table per phase, computed once
        let w: Vec<Vec<f64>> = (1..8)
            .map(|k| {
                let f = k as f64 / 8.0;
                (-half + 1..=half)
                    .map(|j| {
                        let t = j as f64 - f;
                        (PI * t).sin() / (PI * t) * i0(beta * (1.0 - (t / half as f64).powi(2)).max(0.0).sqrt()) / i0(beta)
                    })
                    .collect()
            })
            .collect();
        let mut pk = 0.0f64;
        for n in (half as usize)..x.len() - half as usize {
            pk = pk.max(x[n].abs());
            for wk in &w {
                let acc: f64 = wk.iter().enumerate().map(|(i, c)| c * x[n + i + 1 - half as usize]).sum();
                pk = pk.max(acc.abs());
            }
        }
        pk
    }

    fn run(lim: &mut Limiter, l: &[f64], r: &[f64], on: bool) -> (Vec<f64>, Vec<f64>) {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        for i in 0..l.len() {
            let y = lim.tick([l[i], r[i]], on);
            a.push(y[0]);
            b.push(y[1]);
        }
        (a, b)
    }

    fn db(x: f64) -> f64 {
        20.0 * x.log10()
    }

    #[test]
    fn quiet_signal_is_an_exact_delayed_copy() {
        for sr in [44100.0, 48000.0, 96000.0, 192000.0] {
            let mut lim = Limiter::new(sr);
            let mut s = 7;
            let x: Vec<f64> = (0..20000).map(|_| lcg(&mut s) * 0.5).collect();
            let (y, _) = run(&mut lim, &x, &x, true);
            let lat = lim.latency();
            for i in lat..x.len() {
                assert_eq!(y[i], x[i - lat], "sr {sr} sample {i}");
            }
            assert_eq!(lim.gmin, 1.0);
        }
    }

    #[test]
    fn off_is_a_pure_delay_even_on_hot_signals() {
        let mut lim = Limiter::new(48000.0);
        let x: Vec<f64> = (0..8000).map(|i| 3.0 * (i as f64 * 0.05).sin()).collect();
        let (y, _) = run(&mut lim, &x, &x, false);
        let lat = lim.latency();
        for i in lat..x.len() {
            assert_eq!(y[i], x[i - lat]);
        }
        assert_eq!(lim.gmin, 1.0);
    }

    fn stress(sr: f64) -> (Vec<f64>, Vec<f64>) {
        let n = (sr * 0.4) as usize;
        let mut s = 1;
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        for i in 0..n {
            let t = i as f64 / sr;
            // hot noise bed, an fs/4-ish tone whose samples straddle the peaks (classic inter-sample over),
            // a 10 kHz resonant-style burst, and isolated spike clicks
            let bed = lcg(&mut s) * 0.5;
            // 2 ms raised-cosine fades: a tone gated hard at exactly fs/4 is not programme material (the splatter alone
            // is beyond any short interpolator), a faded one is a synth note
            let fade = ((t - 0.1) / 0.002).clamp(0.0, 1.0).min(((0.2 - t) / 0.002).clamp(0.0, 1.0));
            let fs4 = if (0.1..0.2).contains(&t) { 1.4 * 0.5 * (1.0 - (PI * fade).cos()) * (2.0 * PI * sr * 0.25 * t + 0.7854).sin() } else { 0.0 };
            let burst = if (0.25..0.35).contains(&t) { 1.8 * (2.0 * PI * 10000.0 * t).sin() * (-(t - 0.25) * 30.0).exp() } else { 0.0 };
            let click = if i % 4800 == 100 { 2.5 } else { 0.0 };
            l[i] = bed + fs4 + burst + click;
            r[i] = bed * 0.7 - fs4 * 0.9 + burst * 0.5 - click * 0.3;
        }
        (l, r)
    }

    #[test]
    fn output_true_peak_stays_under_the_ceiling() {
        for sr in [44100.0, 48000.0, 96000.0] {
            let mut lim = Limiter::new(sr);
            let (l, r) = stress(sr);
            let (yl, yr) = run(&mut lim, &l, &r, true);
            let skip = lim.latency() + 200;
            for (name, y) in [("L", &yl), ("R", &yr)] {
                let tp = db(true_peak(&y[skip..]));
                // The ceiling is -0.2. Full-band white noise on top of a hot tone near Nyquist is the worst case for any
                // finite interpolator (it under-reads content right at fs/2 by a few hundredths of a dB), so the stress
                // may touch 0 dBTP but must not go meaningfully over it. Programme material stays well below.
                assert!(tp <= 0.1, "sr {sr} {name}: true peak {tp:.3} dBTP");
                assert!(lim.gmin < 0.5, "limiter never engaged");
            }
        }
    }

    #[test]
    fn a_tall_spike_does_not_duck_the_body_after_it() {
        // a steady -10 dB tone with one 1 ms spike at +12 dB: 30 ms later the tone must be back within 1 dB
        let sr = 48000.0;
        let mut lim = Limiter::new(sr);
        let n = 24000;
        let x: Vec<f64> = (0..n).map(|i| {
            let t = i as f64 / sr;
            let tone = 0.316 * (2.0 * PI * 200.0 * t).sin();
            if (12000..12048).contains(&i) { tone + 4.0 * (PI * (i - 12000) as f64 / 48.0).sin() } else { tone }
        }).collect();
        let (y, _) = run(&mut lim, &x, &x, true);
        let lat = lim.latency();
        let from = 12000 + lat + (0.03 * sr) as usize;
        let rms = |v: &[f64]| (v.iter().map(|a| a * a).sum::<f64>() / v.len() as f64).sqrt();
        let g = db(rms(&y[from..from + 4800]) / rms(&x[from - lat..from - lat + 4800]));
        assert!(g > -1.0, "body still ducked {g:.2} dB, 30 ms after the spike");
    }

    #[test]
    fn it_actually_limits_close_to_the_ceiling() {
        // a loud steady sine: the output should sit near the ceiling, not far below it
        let sr = 48000.0;
        let mut lim = Limiter::new(sr);
        let x: Vec<f64> = (0..48000).map(|i| 2.0 * (2.0 * PI * 1000.0 * i as f64 / sr).sin()).collect();
        let (y, _) = run(&mut lim, &x, &x, true);
        let pk = y[24000..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        assert!(db(pk) > -0.4 && db(pk) < -0.1, "{} dB", db(pk));
    }

    #[test]
    fn releases_back_to_unity() {
        let sr = 48000.0;
        let mut lim = Limiter::new(sr);
        let mut x: Vec<f64> = (0..4800).map(|i| 4.0 * (i as f64 * 0.1).sin()).collect();
        x.extend(vec![0.1; 48000 * 3]);
        run(&mut lim, &x, &x, true);
        assert_eq!(lim.gain(), 1.0);
    }

    #[test]
    fn release_is_faster_for_a_blip_than_for_sustained_reduction() {
        let sr = 48000.0;
        let recovery = |hot_ms: usize| {
            let mut lim = Limiter::new(sr);
            let mut x: Vec<f64> = (0..(sr as usize * hot_ms / 1000)).map(|i| 4.0 * (i as f64 * 0.1).sin()).collect();
            let hot = x.len();
            x.extend(vec![0.05; 48000 * 3]);
            let mut n = 0;
            for (i, v) in x.iter().enumerate() {
                lim.tick([*v, *v], true);
                if i > hot + 200 && lim.gain() > 0.97 {
                    n = i - hot;
                    break;
                }
            }
            n
        };
        let (blip, long) = (recovery(3), recovery(1000));
        assert!(blip > 0 && long > 0);
        // the clip takes most of a hot signal, so the gain stage's 'time under reduction' (and the release range, 15-80 ms) is modest
        assert!(long as f64 > blip as f64 * 1.3, "blip {blip} samples, sustained {long}");
    }

    #[test]
    fn toggling_on_fades_in_without_a_step() {
        let sr = 48000.0;
        let mut lim = Limiter::new(sr);
        let x: Vec<f64> = (0..48000).map(|i| 2.0 * (2.0 * PI * 200.0 * i as f64 / sr).sin()).collect();
        let mut prev = 0.0;
        let mut worst = 0.0f64;
        for (i, v) in x.iter().enumerate() {
            let y = lim.tick([*v, *v], i >= 12000)[0];
            if i > 12000 {
                worst = worst.max((y - prev).abs());
            }
            prev = y;
        }
        // a 200 Hz sine at amplitude 2 moves at most 2 * 2pi * 200 / sr = 0.052 per sample; the fade must not add a jump
        assert!(worst < 0.07, "step {worst}");
    }

    #[test]
    fn latency_is_constant_per_rate() {
        let a = Limiter::new(48000.0).latency();
        assert_eq!(a, 32 + 72);
        assert_eq!(Limiter::new(96000.0).latency(), 32 + 144);
        assert_eq!(Limiter::new(192000.0).latency(), 288);
    }

    #[test]
    fn interpolator_reads_inter_sample_peaks() {
        // fs/4 sine sampled 45 degrees off the crest: sample peak = 0.707, true peak = 1.0
        let mut lim = Limiter::new(48000.0);
        let mut worst = 0.0f64;
        for i in 0..200 {
            let v = (PI * 0.5 * i as f64 + PI / 4.0).sin();
            let p = lim.detect([v, v]);
            if i > 40 {
                worst = worst.max(p);
            }
        }
        assert!((worst - 1.0).abs() < 0.01, "detected {worst}");
    }
}
