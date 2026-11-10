//! 3-way Linkwitz-Riley (LR4) crossover: low / mid / high that sum back to a flat magnitude (all-pass) response.
//!
//! Split 1 at `f1` (LP twice = low, HP twice = rest), split 2 at `f2` on the rest (LP twice = mid, HP twice = high).
//! The low band takes an all-pass at `f2` so all three bands have the same phase response and sum flat.
//! Coefficients glide linearly over each host block when a frequency moves (no zipper noise while dragging).

use satcomp_dsp::biquad::{allpass, bq, hp, lp, Coef, IDENTITY};

const Q: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// filters: 0,1 = LP f1 (low) | 2,3 = HP f1 (rest) | 4,5 = LP f2 (mid) | 6,7 = HP f2 (high) | 8 = AP f2 on low
const N: usize = 9;

pub struct Crossover3 {
    cur: [Coef; N],
    tgt: [Coef; N],
    delta: [Coef; N],
    st: [[[f64; 2]; 2]; N],
    ramping: bool,
}

impl Default for Crossover3 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crossover3 {
    pub fn new() -> Self {
        Self { cur: [IDENTITY; N], tgt: [IDENTITY; N], delta: [[0.0; 5]; N], st: [[[0.0; 2]; 2]; N], ramping: false }
    }

    /// Clamp the pair so f1 < f2 with at least a 5 % gap.
    pub fn order(f1: f64, f2: f64) -> (f64, f64) {
        let f1 = f1.clamp(20.0, 19000.0);
        (f1, f2.max(f1 * 1.05).min(20000.0))
    }

    /// New crossover frequencies; `snap` jumps to them (first parameter set), otherwise they glide over the next block.
    pub fn set(&mut self, f1: f64, f2: f64, sr: f64, snap: bool) {
        let (f1, f2) = Self::order(f1, f2);
        let (l1, h1) = (lp(f1, Q, sr), hp(f1, Q, sr));
        let (l2, h2) = (lp(f2, Q, sr), hp(f2, Q, sr));
        self.tgt = [l1, l1, h1, h1, l2, l2, h2, h2, allpass(f2, Q, sr)];
        if snap {
            self.cur = self.tgt;
            self.delta = [[0.0; 5]; N];
            self.ramping = false;
        }
    }

    /// Per host block: ramp towards the targets over `n` samples.
    pub fn begin_block(&mut self, n: usize) {
        let rn = 1.0 / n.max(1) as f64;
        let mut moving = false;
        for f in 0..N {
            let mut d = 0.0;
            for i in 0..5 {
                self.delta[f][i] = (self.tgt[f][i] - self.cur[f][i]) * rn;
                d += (self.tgt[f][i] - self.cur[f][i]).abs();
            }
            if d < 1e-12 {
                self.cur[f] = self.tgt[f];
                self.delta[f] = [0.0; 5];
            } else {
                moving = true;
            }
        }
        self.ramping = moving;
    }

    #[inline]
    fn run(&mut self, f: usize, c: usize, x: f64) -> f64 {
        bq(&mut self.st[f][c], &self.cur[f], x)
    }

    /// One stereo sample -> [low, mid, high], each [l, r].
    #[inline]
    pub fn tick(&mut self, x: [f64; 2]) -> [[f64; 2]; 3] {
        if self.ramping {
            for f in 0..N {
                for i in 0..5 {
                    self.cur[f][i] += self.delta[f][i];
                }
            }
        }
        let mut out = [[0.0f64; 2]; 3];
        for c in 0..2 {
            let low = self.run(0, c, x[c]);
            let low = self.run(1, c, low);
            let rest = self.run(2, c, x[c]);
            let rest = self.run(3, c, rest);
            let mid = self.run(4, c, rest);
            let mid = self.run(5, c, mid);
            let high = self.run(6, c, rest);
            let high = self.run(7, c, high);
            out[0][c] = self.run(8, c, low);
            out[1][c] = mid;
            out[2][c] = high;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn impulse_response(f1: f64, f2: f64) -> [Vec<f64>; 3] {
        let mut x = Crossover3::new();
        x.set(f1, f2, SR, true);
        x.begin_block(1);
        let mut r = [vec![], vec![], vec![]];
        for i in 0..8192 {
            let o = x.tick([if i == 0 { 1.0 } else { 0.0 }, 0.0]);
            for b in 0..3 {
                r[b].push(o[b][0]);
            }
        }
        r
    }

    fn mag(ir: &[f64], f: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (n, v) in ir.iter().enumerate() {
            let w = 2.0 * std::f64::consts::PI * f * n as f64 / SR;
            re += v * w.cos();
            im -= v * w.sin();
        }
        (re * re + im * im).sqrt()
    }

    #[test]
    fn bands_sum_to_a_flat_magnitude() {
        let ir = impulse_response(200.0, 2500.0);
        let sum: Vec<f64> = (0..ir[0].len()).map(|i| ir[0][i] + ir[1][i] + ir[2][i]).collect();
        for f in [30.0, 100.0, 200.0, 800.0, 2500.0, 6000.0, 15000.0] {
            let db = 20.0 * mag(&sum, f).log10();
            assert!(db.abs() < 0.05, "{f} Hz: {db} dB");
        }
    }

    #[test]
    fn each_band_is_minus_6db_at_its_crossover_and_separates() {
        let ir = impulse_response(200.0, 2500.0);
        let db = |b: usize, f: f64| 20.0 * mag(&ir[b], f).log10();
        assert!((db(0, 200.0) + 6.02).abs() < 0.3 && (db(1, 200.0) + 6.02).abs() < 0.3);
        assert!((db(1, 2500.0) + 6.02).abs() < 0.3 && (db(2, 2500.0) + 6.02).abs() < 0.3);
        assert!(db(0, 3000.0) < -35.0, "low leaks up: {}", db(0, 3000.0));
        assert!(db(2, 200.0) < -35.0, "high leaks down: {}", db(2, 200.0));
        assert!(db(1, 30.0) < -30.0 && db(1, 12000.0) < -30.0);
    }

    #[test]
    fn gliding_frequencies_stay_finite_and_settle() {
        let mut x = Crossover3::new();
        x.set(150.0, 2500.0, SR, true);
        let (mut peak, mut settled) = (0.0f64, 0.0f64);
        for blk in 0..400 {
            if blk == 50 {
                x.set(400.0, 6000.0, SR, false);
            }
            x.begin_block(64);
            for i in 0..64 {
                let s = (2.0 * std::f64::consts::PI * 330.0 * (blk * 64 + i) as f64 / SR).sin();
                let o = x.tick([s, s]);
                let sum = o[0][0] + o[1][0] + o[2][0];
                assert!(sum.is_finite());
                if blk < 100 {
                    peak = peak.max(sum.abs()); // while the crossover moves (a swept tone overshoots a little)
                } else if blk >= 380 {
                    settled = settled.max(sum.abs());
                }
            }
        }
        assert!(peak < 1.5, "{peak}");
        assert!((settled - 1.0).abs() < 0.02, "{settled}");
        assert!(!x.ramping);
    }

    #[test]
    fn order_keeps_f2_above_f1() {
        let (a, b) = Crossover3::order(3000.0, 100.0);
        assert!(b >= a * 1.05);
    }
}
