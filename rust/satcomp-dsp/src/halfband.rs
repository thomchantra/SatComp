//! Kaiser halfband filters + the shaper oversampling wrapper (x2: K=24 / x4: +K=8).
//! Port of mk_taps / hb_up / hb_down / sh_proc; latency 24 (x2) / 28 (x4) samples.

use crate::shapers::ShaperLut;

fn i0(x: f64) -> f64 {
    let (mut sm, mut t, mut k) = (1.0, 1.0, 1.0);
    for _ in 0..30 {
        t *= (x * 0.5 / k) * (x * 0.5 / k);
        sm += t;
        k += 1.0;
    }
    sm
}

/// Odd taps of the halfband, tp[j] = h[2j+1], normalised so they sum to 0.5.
pub fn make_taps(k: usize, beta: f64) -> Vec<f64> {
    let mut tp = vec![0.0; k];
    let mut sm = 0.0;
    for (j, t) in tp.iter_mut().enumerate() {
        let m = (2 * j + 1) as f64 - k as f64;
        let r = m / k as f64;
        let w = i0(beta * (1.0 - r * r).sqrt()) / i0(beta);
        let a = std::f64::consts::PI * m * 0.5;
        let h = a.sin() / a * 0.5 * w;
        *t = h;
        sm += h;
    }
    for t in tp.iter_mut() {
        *t *= 0.5 / sm;
    }
    tp
}

#[derive(Clone)]
pub struct Hb {
    up_pos: usize,
    down_pos: usize,
    up: [f64; 32],
    down: [f64; 64],
}

impl Default for Hb {
    fn default() -> Self {
        Self { up_pos: 0, down_pos: 0, up: [0.0; 32], down: [0.0; 64] }
    }
}

impl Hb {
    /// One input -> (even, odd) outputs.
    #[inline]
    pub fn up(&mut self, tp: &[f64], k: usize, um: usize, x: f64) -> (f64, f64) {
        let p = self.up_pos;
        self.up[p] = x;
        let y0 = self.up[(p + um + 1 - k / 2) & um];
        let mut acc = 0.0;
        for (j, t) in tp.iter().enumerate().take(k) {
            acc += t * self.up[(p + um + 1 - j) & um];
        }
        self.up_pos = (p + 1) & um;
        (y0, 2.0 * acc)
    }

    /// (even, odd) pair -> one output.
    #[inline]
    pub fn down(&mut self, tp: &[f64], k: usize, dm: usize, a: f64, b: f64) -> f64 {
        let mut q = self.down_pos;
        self.down[q] = a;
        let mut acc = 0.5 * self.down[(q + dm + 1 - k) & dm];
        for (j, t) in tp.iter().enumerate().take(k) {
            acc += t * self.down[(q + dm + 1 - 2 * j - 1) & dm];
        }
        q = (q + 1) & dm;
        self.down[q] = b;
        self.down_pos = (q + 1) & dm;
        acc
    }
}

/// Per-channel oversampling state (A = x2 stage, B = x4 stage).
#[derive(Clone, Default)]
pub struct Oversampler {
    a: Hb,
    b: Hb,
}

pub struct Taps {
    pub a: Vec<f64>,
    pub b: Vec<f64>,
}

impl Taps {
    pub fn new() -> Self {
        Self { a: make_taps(24, 8.0), b: make_taps(8, 6.0) }
    }
}

impl Default for Taps {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// os: 0 off, 1 x2, 2 x4.
    #[inline]
    pub fn process(&mut self, taps: &Taps, lut: &ShaperLut, os: i32, x: f64) -> f64 {
        match os {
            0 => lut.eval(x),
            1 => {
                let (a0, a1) = self.a.up(&taps.a, 24, 31, x);
                self.a.down(&taps.a, 24, 63, lut.eval(a0), lut.eval(a1))
            }
            _ => {
                let (a0, a1) = self.a.up(&taps.a, 24, 31, x);
                let (b0, b1) = self.b.up(&taps.b, 8, 7, a0);
                let (b2, b3) = self.b.up(&taps.b, 8, 7, a1);
                let c0 = self.b.down(&taps.b, 8, 15, lut.eval(b0), lut.eval(b1));
                let c1 = self.b.down(&taps.b, 8, 15, lut.eval(b2), lut.eval(b3));
                self.a.down(&taps.a, 24, 63, c0, c1)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_sum_to_half() {
        for (k, b) in [(24, 8.0), (8, 6.0)] {
            let s: f64 = make_taps(k, b).iter().sum();
            assert!((s - 0.5).abs() < 1e-12);
        }
    }

    /// Linear shaper: the oversampled chain must be a pure delay of `lat` samples with unity gain.
    #[test]
    fn linear_path_is_unity_delay() {
        let mut lut = ShaperLut::new(1);
        for (i, v) in lut.table.iter_mut().enumerate() {
            *v = (i as f64 / 256.0 - 8.0).clamp(-7.9, 7.9); // identity inside range
        }
        lut.periodic = false;
        let taps = Taps::new();
        for (os, lat) in [(1, 24usize), (2, 28usize)] {
            let mut ov = Oversampler::default();
            let n = 400;
            // 300 Hz-ish sine at 48k, well inside the passband
            let x: Vec<f64> = (0..n).map(|i| 0.5 * (i as f64 * 0.04).sin()).collect();
            let y: Vec<f64> = x.iter().map(|&v| ov.process(&taps, &lut, os, v)).collect();
            let mut worst: f64 = 0.0;
            for i in 200..n {
                worst = worst.max((y[i] - x[i - lat]).abs());
            }
            assert!(worst < 1e-3, "os {os}: worst {worst}");
        }
    }
}
