//! External key conditioning for a detector: gain, then high-pass, then low-pass, then a delay so the key lines up with a
//! signal that was delayed by an upstream block (the saturator's oversampling latency).

use crate::biquad::{self, bq, Coef};
use crate::delay::StereoDelay;
use crate::level::{one_pole, LN10_20};

pub struct KeyInput {
    g: f64,
    g_t: f64,
    g_c: f64,
    use_hp: bool,
    use_lp: bool,
    hpc: Coef,
    lpc: Coef,
    hps: [[f64; 2]; 2],
    lps: [[f64; 2]; 2],
    ring: StereoDelay<64>,
}

impl Default for KeyInput {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyInput {
    pub fn new() -> Self {
        Self {
            g: 1.0,
            g_t: 1.0,
            g_c: 0.0,
            use_hp: false,
            use_lp: false,
            hpc: biquad::IDENTITY,
            lpc: biquad::IDENTITY,
            hps: [[0.0; 2]; 2],
            lps: [[0.0; 2]; 2],
            ring: StereoDelay::new(),
        }
    }

    /// `gain_db`, `hp` (<= 20.5 = off) and `lp` (>= 19900 = off) in Hz.
    pub fn derive(&mut self, gain_db: f64, hp: f64, lp: f64, sr: f64) {
        self.g_t = (gain_db * LN10_20).exp();
        self.use_hp = hp > 20.5;
        if self.use_hp {
            self.hpc = biquad::hp(hp, 0.70710678, sr);
        }
        self.use_lp = lp < 19900.0;
        if self.use_lp {
            self.lpc = biquad::lp(lp, 0.70710678, sr);
        }
        self.g_c = one_pole(sr, 0.02);
    }

    /// Call every sample. `key` is the raw key input while a key is active, `None` otherwise (the delay ring keeps
    /// moving either way so it stays aligned). Returns the conditioned key delayed by `delay` samples, or zeros.
    #[inline]
    pub fn tick(&mut self, key: Option<[f64; 2]>, delay: usize) -> [f64; 2] {
        self.g += (self.g_t - self.g) * self.g_c;
        self.ring.advance();
        let mut out = [0.0f64; 2];
        if let Some(s_in) = key {
            for c in 0..2 {
                let mut v = s_in[c] * self.g;
                if self.use_hp {
                    v = bq(&mut self.hps[c], &self.hpc, v);
                }
                if self.use_lp {
                    v = bq(&mut self.lps[c], &self.lpc, v);
                }
                self.ring.0[c].put(v);
                out[c] = self.ring.0[c].tap(delay);
            }
        }
        out
    }
}
