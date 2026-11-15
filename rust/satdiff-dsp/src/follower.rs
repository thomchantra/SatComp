//! MORPH detector: a level follower (peak path and RMS path blended) whose SPEED macro sets attack, release and the
//! peak / RMS mix together. SPEED 100 % = fast attack, fast release, peak-leaning; 0 % = slow attack, slow release, RMS-leaning.

use satcomp_dsp::level::one_pole;

/// Level range the blend position sweeps: this (dBFS) or quieter = 0, `TOP_DB` or louder = 1.
pub const FLOOR_DB: f64 = -40.0;
pub const TOP_DB: f64 = -4.0;

#[derive(Clone, Copy, Debug)]
pub struct Follower {
    peak: f64,
    ms: f64,
    att: f64,
    rel: f64,
    rms_c: f64,
    /// weight of the RMS path, 0..1
    w: f64,
}

impl Default for Follower {
    fn default() -> Self {
        let mut f = Self { peak: 0.0, ms: 0.0, att: 0.0, rel: 0.0, rms_c: 0.0, w: 0.0 };
        f.set_speed(0.5, 48000.0);
        f
    }
}

impl Follower {
    /// `speed` 0..1 (1 = fast / peak).
    pub fn set_speed(&mut self, speed: f64, sr: f64) {
        let s = speed.clamp(0.0, 1.0);
        let slow = 1.0 - s;
        self.att = one_pole(sr, 0.0005 + slow * slow * 0.06);
        self.rel = one_pole(sr, 0.02 + slow * slow * 0.6);
        self.rms_c = one_pole(sr, 0.01 + slow * 0.2);
        self.w = slow;
    }

    /// One sample of |input| in, level (linear, peak-referenced) out.
    #[inline]
    pub fn step(&mut self, a: f64) -> f64 {
        let c = if a > self.peak { self.att } else { self.rel };
        self.peak += (a - self.peak) * c;
        self.ms += (a * a - self.ms) * self.rms_c;
        // a sine's RMS is 0.707 of its peak: scale so both paths read the same on a steady tone
        let rms = self.ms.max(0.0).sqrt() * std::f64::consts::SQRT_2;
        (1.0 - self.w) * self.peak + self.w * rms
    }

    /// Linear level -> 0..1 position over `FLOOR_DB`..`TOP_DB`.
    pub fn to_unit(level: f64) -> f64 {
        let db = 20.0 * level.max(1e-9).log10();
        ((db - FLOOR_DB) / (TOP_DB - FLOOR_DB)).clamp(0.0, 1.0)
    }
}
