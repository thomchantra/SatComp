//! The two filter blocks that sit around the colour fx.
//!
//! * `OutHp`: a 24 dB/oct (4th-order Butterworth) high-pass right before OUT GAIN, with M / S switches that choose whether it cuts
//!   the mid, the side, or both (both = the same as filtering L and R; side only = a mono-sub cleanup). Zero latency. At its lowest
//!   setting it is an exact bypass and keeps no state; engaging, releasing and flipping M / S all crossfade.
//! * `FxFilter`: an HPF / LPF pair (12 dB/oct each, like the saturator's) for the wide-band wet signal of RINGMOD and FREQ SHIFT.

use satcomp_dsp::biquad::{self, Coef, IDENTITY};
use satcomp_dsp::level::one_pole;

/// Q of the two sections of a 4th-order Butterworth
const BUTTER4_Q: [f64; 2] = [0.5411961, 1.3065630];
const OFF_HZ: f64 = 20.5;

pub struct OutHp {
    sr: f64,
    f: f64,
    f_t: f64,
    c: [Coef; 2],
    /// [M, S][section]
    st: [[[f64; 2]; 2]; 2],
    eng: f64,
    gm: f64,
    gs: f64,
    on_m: bool,
    on_s: bool,
    cnt: u32,
    sm_c: f64,
    fade_c: f64,
    clean: bool,
}

impl OutHp {
    pub fn new(sr: f64) -> Self {
        let mut o = Self { sr, f: 20.0, f_t: 20.0, c: [IDENTITY; 2], st: [[[0.0; 2]; 2]; 2], eng: 0.0, gm: 1.0, gs: 1.0, on_m: true, on_s: true, cnt: 0, sm_c: 0.0, fade_c: 0.0, clean: true };
        o.set_sample_rate(sr);
        o
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sr = sr;
        self.sm_c = one_pole(sr, 0.01);
        self.fade_c = one_pole(sr, 0.02);
        self.coefs();
    }

    fn coefs(&mut self) {
        for k in 0..2 {
            self.c[k] = biquad::hp(self.f.max(1.0), BUTTER4_Q[k], self.sr);
        }
    }

    /// `snap` jumps to the settings (a fresh engine starts at the host's).
    pub fn set(&mut self, hz: f64, mid: bool, side: bool, snap: bool) {
        self.f_t = hz.clamp(20.0, 2000.0);
        self.on_m = mid;
        self.on_s = side;
        if snap {
            self.f = self.f_t;
            self.gm = mid as u8 as f64;
            self.gs = side as u8 as f64;
            self.eng = if self.f_t > OFF_HZ { 1.0 } else { 0.0 };
            self.coefs();
        }
    }

    #[inline]
    fn section(&self, ch: usize, v: f64, st: &mut [[[f64; 2]; 2]; 2]) -> f64 {
        let a = biquad::bq(&mut st[ch][0], &self.c[0], v);
        biquad::bq(&mut st[ch][1], &self.c[1], a)
    }

    #[inline]
    pub fn tick(&mut self, x: [f64; 2]) -> [f64; 2] {
        let target = if self.f_t > OFF_HZ { 1.0 } else { 0.0 };
        if self.eng < 1e-9 && target == 0.0 {
            if !self.clean {
                self.st = [[[0.0; 2]; 2]; 2];
                self.clean = true;
            }
            self.eng = 0.0;
            self.f = self.f_t;
            self.gm = self.on_m as u8 as f64;
            self.gs = self.on_s as u8 as f64;
            return x;
        }
        if self.clean {
            // engaging from idle: start the cutoff where it is asked for
            self.f = self.f_t;
            self.coefs();
        }
        self.clean = false;
        self.eng += (target - self.eng) * self.fade_c;
        self.gm += ((self.on_m as u8 as f64) - self.gm) * self.sm_c;
        self.gs += ((self.on_s as u8 as f64) - self.gs) * self.sm_c;
        if (self.f_t - self.f).abs() > 1e-4 * self.f {
            // glide the cutoff in the log domain, recompute the coefficients every 16 samples
            self.f *= (self.f_t / self.f).powf(self.sm_c.min(1.0));
            self.cnt += 1;
            if self.cnt >= 16 {
                self.cnt = 0;
                self.coefs();
            }
        }
        let (m, s) = ((x[0] + x[1]) * 0.5, (x[0] - x[1]) * 0.5);
        let mut st = self.st;
        let (hm, hs) = (self.section(0, m, &mut st), self.section(1, s, &mut st));
        self.st = st;
        let m2 = m + (hm - m) * self.gm * self.eng;
        let s2 = s + (hs - s) * self.gs * self.eng;
        [m2 + s2, m2 - s2]
    }
}

pub struct FxFilter {
    sr: f64,
    hp: Coef,
    lp: Coef,
    hp_on: bool,
    lp_on: bool,
    s: [[[f64; 2]; 2]; 2],
}

impl FxFilter {
    pub fn new(sr: f64) -> Self {
        Self { sr, hp: IDENTITY, lp: IDENTITY, hp_on: false, lp_on: false, s: [[[0.0; 2]; 2]; 2] }
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sr = sr;
    }

    pub fn reset(&mut self) {
        self.s = [[[0.0; 2]; 2]; 2];
    }

    pub fn set(&mut self, hp: f64, lp: f64) {
        let (h_on, l_on) = (hp > OFF_HZ, lp < 19900.0);
        if h_on {
            self.hp = biquad::hp(hp, 0.70710678, self.sr);
        }
        if l_on {
            self.lp = biquad::lp(lp, 0.70710678, self.sr);
        }
        if !h_on && self.hp_on {
            self.s[0] = [[0.0; 2]; 2];
        }
        if !l_on && self.lp_on {
            self.s[1] = [[0.0; 2]; 2];
        }
        self.hp_on = h_on;
        self.lp_on = l_on;
    }

    #[inline]
    pub fn tick(&mut self, x: [f64; 2]) -> [f64; 2] {
        if !self.hp_on && !self.lp_on {
            return x;
        }
        let mut y = x;
        for c in 0..2 {
            if self.hp_on {
                y[c] = biquad::bq(&mut self.s[0][c], &self.hp, y[c]);
            }
            if self.lp_on {
                y[c] = biquad::bq(&mut self.s[1][c], &self.lp, y[c]);
            }
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;
    const SR: f64 = 48000.0;

    fn run(hz: f64, m: bool, s: bool, f: impl Fn(usize) -> [f64; 2], n: usize) -> Vec<[f64; 2]> {
        let mut o = OutHp::new(SR);
        o.set(hz, m, s, true);
        (0..n).map(|i| o.tick(f(i))).collect()
    }

    fn amp(y: &[f64]) -> f64 {
        let t = &y[y.len() / 2..];
        (t.iter().map(|v| v * v).sum::<f64>() / t.len() as f64).sqrt() * 2f64.sqrt()
    }

    fn tone(f: f64, i: usize) -> f64 {
        (TAU * f * i as f64 / SR).sin() * 0.5
    }

    #[test]
    fn at_its_lowest_it_is_an_exact_bypass() {
        let y = run(20.0, true, true, |i| [tone(100.0, i), tone(300.0, i)], 5000);
        for (i, v) in y.iter().enumerate() {
            assert_eq!(*v, [tone(100.0, i), tone(300.0, i)]);
        }
    }

    #[test]
    fn it_is_a_24_db_per_octave_butterworth() {
        // -3 dB at the corner, then 24 dB per octave
        let at = |f: f64| {
            let y = run(200.0, true, true, |i| [tone(f, i); 2], 48000);
            amp(&y.iter().map(|v| v[0]).collect::<Vec<_>>()) / 0.5
        };
        assert!((at(200.0) - 0.7071).abs() < 0.02, "corner {}", at(200.0));
        let (a, b) = (at(50.0), at(100.0));
        assert!((20.0 * (b / a).log10() - 24.0).abs() < 2.0, "slope {} dB / oct", 20.0 * (b / a).log10());
        assert!((at(5000.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn the_switches_choose_what_is_cut() {
        // a 60 Hz tone that is mid-only (L = R) and one that is side-only (L = -R), HPF at 200 Hz
        let mid = |i| [tone(60.0, i); 2];
        let side = |i| [tone(60.0, i), -tone(60.0, i)];
        let get = |m, s, f: &dyn Fn(usize) -> [f64; 2]| {
            let y = run(200.0, m, s, f, 48000);
            amp(&y.iter().map(|v| v[0]).collect::<Vec<_>>()) / 0.5
        };
        assert!(get(true, true, &mid) < 0.05 && get(true, true, &side) < 0.05);
        assert!(get(false, true, &mid) > 0.95 && get(false, true, &side) < 0.05, "side only keeps the mid");
        assert!(get(true, false, &mid) < 0.05 && get(true, false, &side) > 0.95, "mid only keeps the side");
        assert!(get(false, false, &mid) > 0.95 && get(false, false, &side) > 0.95);
    }

    #[test]
    fn moving_the_cutoff_and_the_switches_does_not_click() {
        let mut o = OutHp::new(SR);
        o.set(30.0, true, true, true);
        let mut prev = [0.0; 2];
        let mut worst = 0.0f64;
        for i in 0..96000 {
            if i == 20000 {
                o.set(400.0, true, true, false);
            }
            if i == 50000 {
                o.set(400.0, false, true, false);
            }
            if i == 70000 {
                o.set(20.0, false, true, false);
            }
            let x = [tone(110.0, i), tone(110.0, i) * 0.6];
            let y = o.tick(x);
            if i > 100 {
                worst = worst.max((y[0] - prev[0]).abs());
            }
            prev = y;
            assert!(y[0].is_finite() && y[0].abs() < 2.0);
        }
        // a 110 Hz tone at 0.5 moves at most 2 pi f / sr * 0.5 = 7e-3 per sample
        assert!(worst < 0.03, "step {worst}");
    }

    #[test]
    fn the_fx_pair_cuts_both_ends() {
        let mut f = FxFilter::new(SR);
        f.set(1000.0, 4000.0);
        let run = |fr: f64, f: &mut FxFilter| {
            f.reset();
            let y: Vec<f64> = (0..24000).map(|i| f.tick([tone(fr, i); 2])[0]).collect();
            amp(&y) / 0.5
        };
        assert!(run(60.0, &mut f) < 0.01, "sub");
        assert!(run(16000.0, &mut f) < 0.1, "air");
        assert!((run(2000.0, &mut f) - 1.0).abs() < 0.15, "mid");
        f.set(20.0, 20000.0);
        let x = [0.3, -0.2];
        assert_eq!(f.tick(x), x);
    }
}
