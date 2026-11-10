//! Level measurement helpers: one-pole mean-square meters and the 3-second gain-match accumulator.

use crate::DENORM;

/// One-pole smoothing coefficient for a time constant, `1 - exp(-1/(sr*secs))`.
pub fn one_pole(sr: f64, secs: f64) -> f64 {
    1.0 - (-1.0 / (sr * secs)).exp()
}

/// Smoothed stereo mean-square level: `v += ((l^2 + r^2)/2 + DENORM - v) * c`.
#[inline]
pub fn ms_step(v: &mut f64, l: f64, r: f64, c: f64) {
    *v += ((l * l + r * r) * 0.5 + DENORM - *v) * c;
}

/// `10/ln(10)`: ln of a power ratio to dB.
pub const DB_POW: f64 = 4.3429448;
/// `20/ln(10)`: ln of an amplitude ratio to dB.
pub const DB_AMP: f64 = 8.6858896;
/// dB to ln(gain).
pub const LN10_20: f64 = 0.11512925;

/// Time-average of a dB difference over `LEN` seconds of gated signal (the level-match measurement).
#[derive(Clone, Copy, Default)]
pub struct Matcher {
    pub active: bool,
    acc: f64,
    /// seconds measured so far
    pub t: f64,
}

impl Matcher {
    pub const LEN: f64 = 3.0;

    pub fn start(&mut self) {
        self.active = true;
        self.acc = 0.0;
        self.t = 0.0;
    }

    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Feed `diff_db` for `dt` seconds. Returns the negated average once `LEN` seconds are collected (the measurement
    /// ends); the caller clamps / rounds it.
    pub fn step(&mut self, diff_db: f64, dt: f64) -> Option<f64> {
        self.acc += diff_db * dt;
        self.t += dt;
        if self.t >= Self::LEN {
            self.active = false;
            Some(-self.acc / self.t)
        } else {
            None
        }
    }
}
