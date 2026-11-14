//! Diffusor: a cascade of second-order allpass stages (RBJ) at FREQ with resonance Q. Linear: it keeps every frequency at the
//! same level and delays the ones around FREQ more (a chirp / ring on transients, a lower crest factor). With the mix below
//! 100 % the wet path interferes with the dry one and carves real notches (a phaser sound). IIR, zero latency.

use satcomp_dsp::biquad::{allpass, bq, Coef, IDENTITY};
use satcomp_dsp::level::one_pole;

pub const MAX_STAGES: usize = 16;
/// coefficients are recomputed every this many samples while FREQ / Q glide
const CTRL: u32 = 8;

pub struct Diffusor {
    sr: f64,
    st: [[[f64; 2]; MAX_STAGES]; 2],
    coef: Coef,
    /// smoothed log-frequency and Q, and their targets
    lf: f64,
    lf_t: f64,
    q: f64,
    q_t: f64,
    stages: usize,
    mix: f64,
    mix_t: f64,
    /// enable crossfade (0 = off)
    eng: f64,
    eng_t: f64,
    sm_c: f64,
    en_c: f64,
    cnt: u32,
    clean: bool,
}

impl Diffusor {
    pub fn new(sr: f64) -> Self {
        let mut d = Self {
            sr,
            st: [[[0.0; 2]; MAX_STAGES]; 2],
            coef: IDENTITY,
            lf: 1000f64.ln(),
            lf_t: 1000f64.ln(),
            q: 0.7,
            q_t: 0.7,
            stages: 4,
            mix: 1.0,
            mix_t: 1.0,
            eng: 0.0,
            eng_t: 0.0,
            sm_c: 0.0,
            en_c: 0.0,
            cnt: 0,
            clean: true,
        };
        d.set_sample_rate(sr);
        d.recompute();
        d
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sr = sr;
        self.sm_c = one_pole(sr, 0.01);
        self.en_c = one_pole(sr, 0.02);
    }

    /// `snap` jumps every smoothed value to its target (a fresh engine starts at the host's settings).
    pub fn set(&mut self, on: bool, freq: f64, q: f64, stages: i32, mix_pct: f64, snap: bool) {
        self.lf_t = freq.clamp(20.0, 20000.0).ln();
        self.q_t = q.clamp(0.3, 8.0);
        self.stages = (stages.clamp(1, MAX_STAGES as i32)) as usize;
        self.mix_t = (mix_pct * 0.01).clamp(0.0, 1.0);
        self.eng_t = if on { 1.0 } else { 0.0 };
        if snap {
            self.lf = self.lf_t;
            self.q = self.q_t;
            self.mix = self.mix_t;
            self.eng = self.eng_t;
            self.recompute();
        }
    }

    fn recompute(&mut self) {
        self.coef = allpass(self.lf.exp(), self.q, self.sr);
    }

    /// Clear the filter state (the engine does this when the fx moves between PRE and POST).
    pub fn reset(&mut self) {
        self.st = [[[0.0; 2]; MAX_STAGES]; 2];
        self.clean = true;
    }

    /// Whether the next `tick` is a plain pass-through (off and faded out).
    pub fn idle(&self) -> bool {
        self.eng < 1e-9 && self.eng_t == 0.0
    }

    #[inline]
    pub fn tick(&mut self, x: [f64; 2]) -> [f64; 2] {
        if self.idle() {
            if !self.clean {
                self.reset();
            }
            self.eng = 0.0;
            return x;
        }
        self.clean = false;
        self.lf += (self.lf_t - self.lf) * self.sm_c;
        self.q += (self.q_t - self.q) * self.sm_c;
        self.mix += (self.mix_t - self.mix) * self.sm_c;
        self.eng += (self.eng_t - self.eng) * self.en_c;
        self.cnt += 1;
        if self.cnt >= CTRL {
            self.cnt = 0;
            self.recompute();
        }
        let m = self.mix * self.eng;
        let mut out = x;
        for c in 0..2 {
            let mut v = x[c];
            for k in 0..self.stages {
                v = bq(&mut self.st[c][k], &self.coef, v);
            }
            out[c] = x[c] + (v - x[c]) * m;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn run(sr: f64, on: bool, f0: f64, q: f64, n: i32, mix: f64, x: &[f64]) -> Vec<f64> {
        let mut d = Diffusor::new(sr);
        d.set(on, f0, q, n, mix, true);
        x.iter().map(|&v| d.tick([v, v])[0]).collect()
    }

    fn rms(v: &[f64]) -> f64 {
        (v.iter().map(|a| a * a).sum::<f64>() / v.len() as f64).sqrt()
    }

    fn sine(sr: f64, f: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| 0.5 * (2.0 * PI * f * i as f64 / sr).sin()).collect()
    }

    fn noise(n: usize, seed: u64) -> Vec<f64> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
            })
            .collect()
    }

    #[test]
    fn off_is_an_exact_bypass_and_so_is_a_zero_mix() {
        let x = noise(4000, 3);
        assert_eq!(run(48000.0, false, 800.0, 3.0, 8, 100.0, &x), x);
        assert_eq!(run(48000.0, true, 800.0, 3.0, 8, 0.0, &x), x);
    }

    #[test]
    fn the_wet_path_keeps_every_frequency_at_the_same_level() {
        // magnitude of an allpass cascade is 1: tones at, below and above FREQ come out at the level they went in
        for (sr, f0, q, n) in [(48000.0, 1000.0, 0.7, 4), (48000.0, 200.0, 8.0, 16), (44100.0, 12000.0, 4.0, 8), (96000.0, 40.0, 8.0, 12), (48000.0, 20000.0, 8.0, 16)] {
            for f in [50.0, 300.0, 1000.0, 5000.0, 14000.0] {
                let x = sine(sr, f, (sr * 1.5) as usize);
                let y = run(sr, true, f0, q, n, 100.0, &x);
                let k = (sr * 1.0) as usize;
                let r = rms(&y[k..]) / rms(&x[k..]);
                assert!((r - 1.0).abs() < 0.02, "sr {sr} f0 {f0} Q {q} x{n} at {f} Hz: gain {r}");
            }
        }
    }

    #[test]
    fn extremes_stay_finite_and_bounded() {
        let x = noise(96000, 9);
        for (sr, f0, q, n) in [(44100.0, 20.0, 8.0, 16), (96000.0, 20000.0, 8.0, 16), (48000.0, 20.0, 0.3, 1)] {
            let y = run(sr, true, f0, q, n, 100.0, &x);
            assert!(y.iter().all(|v| v.is_finite()), "sr {sr} f0 {f0}");
            let r = rms(&y[48000..]) / rms(&x[48000..]);
            assert!((r - 1.0).abs() < 0.1, "sr {sr} f0 {f0}: rms ratio {r}");
        }
    }

    #[test]
    fn more_stages_and_a_higher_q_smear_an_impulse_wider() {
        // the energy-weighted mean delay of any allpass is fixed by its order, so measure the spread around it
        let spread = |q: f64, n: i32| {
            let mut x = vec![0.0; 16384];
            x[0] = 1.0;
            let y = run(48000.0, true, 1000.0, q, n, 100.0, &x);
            let e: f64 = y.iter().map(|v| v * v).sum();
            let c: f64 = y.iter().enumerate().map(|(i, v)| v * v * i as f64).sum::<f64>() / e;
            (y.iter().enumerate().map(|(i, v)| v * v * (i as f64 - c).powi(2)).sum::<f64>() / e).sqrt()
        };
        let (base, more, sharp) = (spread(0.7, 2), spread(0.7, 12), spread(6.0, 2));
        assert!(more > base * 2.0, "stages: {base} -> {more}");
        assert!(sharp > base * 2.0, "Q: {base} -> {sharp}");
    }

    #[test]
    fn a_half_mix_carves_a_notch_where_the_phase_turns_through_180() {
        // one stage: at FREQ the wet path is the dry path inverted, so 50 % cancels a tone sitting on FREQ
        let (sr, f0) = (48000.0, 1000.0);
        let x = sine(sr, f0, 96000);
        let y = run(sr, true, f0, 1.0, 1, 50.0, &x);
        let r = rms(&y[48000..]) / rms(&x[48000..]);
        assert!(r < 0.05, "notch depth {r}");
        // well away from FREQ it stays close to unity
        let x = sine(sr, 8000.0, 96000);
        let y = run(sr, true, f0, 1.0, 1, 50.0, &x);
        assert!(rms(&y[48000..]) / rms(&x[48000..]) > 0.9);
    }

    #[test]
    fn switching_on_fades_in_and_off_leaves_no_state() {
        let mut d = Diffusor::new(48000.0);
        d.set(true, 500.0, 2.0, 6, 100.0, true);
        for i in 0..4000 {
            d.tick([(i as f64 * 0.1).sin() * 0.5; 2]);
        }
        d.set(false, 500.0, 2.0, 6, 100.0, false);
        for i in 0..40000 {
            d.tick([(i as f64 * 0.1).sin() * 0.5; 2]);
        }
        assert!(d.idle());
        let x = [0.3, -0.2];
        assert_eq!(d.tick(x), x);
    }
}
