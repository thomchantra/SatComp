//! 3-way Linkwitz-Riley crossover: low / mid / high that sum back to a flat magnitude (all-pass) response.
//!
//! Split 1 at `f1` (LP = low, HP = rest), split 2 at `f2` on the rest (LP = mid, HP = high).
//! The low band takes an all-pass at `f2` so all three bands have the same phase response and sum flat.
//! Each split has its own slope: 12 / 24 / 48 / 96 dB/oct = Butterworth order m = 1 / 2 / 4 / 8 squared (Linkwitz-Riley).
//! LR2 (12 dB/oct) only sums flat with its high side inverted, so the engine sums the bands with `signs()`; the bands
//! themselves keep their natural polarity (so asymmetric saturation behaves the same at every slope).
//! Coefficients glide linearly over each host block when a frequency or slope moves (no zipper noise while dragging).

use satcomp_dsp::biquad::{allpass, bq, hp, lp, Coef, IDENTITY};
use std::f64::consts::PI;

/// Butterworth section Qs per order (order 1 is a first-order section, handled separately).
const Q2: [f64; 1] = [0.707_106_781_186_547_5];
const Q4: [f64; 2] = [0.541_196_100_146_197, 1.306_562_964_876_376_4];
const Q8: [f64; 4] = [0.509_795_579_104_159_3, 0.601_344_886_935_045_2, 0.899_976_223_136_415_6, 2.562_915_447_741_505];

/// Number of slope steps and their names (dB/oct).
pub const SLOPES: usize = 4;
pub const SLOPE_DB: [u32; SLOPES] = [12, 24, 48, 96];
const MAX_SEC: usize = 8; // LR8x2: 4 biquads, squared
const AP_SEC: usize = 4;
const LP1: usize = 0;
const HP1: usize = MAX_SEC;
const LP2: usize = 2 * MAX_SEC;
const HP2: usize = 3 * MAX_SEC;
const AP2: usize = 4 * MAX_SEC;
const N: usize = 4 * MAX_SEC + AP_SEC;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Lp,
    Hp,
    Ap,
}

fn first_order(kind: Kind, f: f64, sr: f64) -> Coef {
    let k = (PI * f.min(sr * 0.45) / sr).tan();
    let a1 = (k - 1.0) / (k + 1.0);
    match kind {
        Kind::Lp => [k / (k + 1.0), k / (k + 1.0), 0.0, a1, 0.0],
        Kind::Hp => [1.0 / (k + 1.0), -1.0 / (k + 1.0), 0.0, a1, 0.0],
        Kind::Ap => [a1, 1.0, 0.0, a1, 0.0],
    }
}

fn section(kind: Kind, f: f64, q: f64, sr: f64) -> Coef {
    match kind {
        Kind::Lp => lp(f, q, sr),
        Kind::Hp => hp(f, q, sr),
        Kind::Ap => allpass(f, q, sr),
    }
}

/// Fills `out` (unused sections become identity) with the Butterworth order `1 << slope` sections of `kind`, `repeat` times over.
fn fill(out: &mut [Coef], kind: Kind, f: f64, slope: usize, repeat: usize, sr: f64) {
    for c in out.iter_mut() {
        *c = IDENTITY;
    }
    let qs: &[f64] = match slope {
        0 => &[],
        1 => &Q2,
        2 => &Q4,
        _ => &Q8,
    };
    let mut i = 0;
    for _ in 0..repeat {
        if slope == 0 {
            out[i] = first_order(kind, f, sr);
            i += 1;
        } else {
            for &q in qs {
                out[i] = section(kind, f, q, sr);
                i += 1;
            }
        }
    }
}

struct Core {
    slopes: (usize, usize),
    cur: [Coef; N],
    tgt: [Coef; N],
    delta: [Coef; N],
    st: [[[f64; 2]; 2]; N],
    ramping: bool,
    sign_cur: [f64; 3],
    sign_tgt: [f64; 3],
    sign_delta: [f64; 3],
}

impl Core {
    fn new() -> Self {
        Self {
            slopes: (1, 1),
            cur: [IDENTITY; N],
            tgt: [IDENTITY; N],
            delta: [[0.0; 5]; N],
            st: [[[0.0; 2]; 2]; N],
            ramping: false,
            sign_cur: [1.0; 3],
            sign_tgt: [1.0; 3],
            sign_delta: [0.0; 3],
        }
    }

    /// Per-band polarity the engine multiplies each band with when summing (-1 only for the 12 dB/oct high sides).
    #[inline]
    fn signs(&self) -> [f64; 3] {
        self.sign_cur
    }

    /// New crossover frequencies and slopes (0..3 = 12 / 24 / 48 / 96 dB/oct); `snap` jumps to them (first parameter set),
    /// otherwise they glide over the next block.
    fn set(&mut self, f1: f64, f2: f64, s1: usize, s2: usize, sr: f64, snap: bool) {
        self.slopes = (s1, s2);
        fill(&mut self.tgt[LP1..HP1], Kind::Lp, f1, s1, 2, sr);
        fill(&mut self.tgt[HP1..LP2], Kind::Hp, f1, s1, 2, sr);
        fill(&mut self.tgt[LP2..HP2], Kind::Lp, f2, s2, 2, sr);
        fill(&mut self.tgt[HP2..AP2], Kind::Hp, f2, s2, 2, sr);
        fill(&mut self.tgt[AP2..N], Kind::Ap, f2, s2, 1, sr);
        let (g1, g2) = (if s1 == 0 { -1.0 } else { 1.0 }, if s2 == 0 { -1.0 } else { 1.0 });
        self.sign_tgt = [1.0, g1, g1 * g2];
        if snap {
            self.cur = self.tgt;
            self.delta = [[0.0; 5]; N];
            self.sign_cur = self.sign_tgt;
            self.sign_delta = [0.0; 3];
            self.ramping = false;
        }
    }

    /// Per host block: ramp towards the targets over `n` samples.
    fn begin_block(&mut self, n: usize) {
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
        for b in 0..3 {
            let d = self.sign_tgt[b] - self.sign_cur[b];
            if d.abs() < 1e-12 {
                self.sign_cur[b] = self.sign_tgt[b];
                self.sign_delta[b] = 0.0;
            } else {
                self.sign_delta[b] = d * rn;
                moving = true;
            }
        }
        self.ramping = moving;
    }

    #[inline]
    fn run(&mut self, f: usize, c: usize, x: f64) -> f64 {
        bq(&mut self.st[f][c], &self.cur[f], x)
    }

    #[inline]
    fn chain(&mut self, base: usize, len: usize, c: usize, mut x: f64) -> f64 {
        for f in base..base + len {
            x = self.run(f, c, x);
        }
        x
    }

    /// One stereo sample -> natural-polarity [low, mid, high], each [l, r] (sum them with `signs()`).
    #[inline]
    fn tick(&mut self, x: [f64; 2]) -> [[f64; 2]; 3] {
        if self.ramping {
            for f in 0..N {
                for i in 0..5 {
                    self.cur[f][i] += self.delta[f][i];
                }
            }
            for b in 0..3 {
                self.sign_cur[b] += self.sign_delta[b];
            }
        }
        let mut out = [[0.0f64; 2]; 3];
        for c in 0..2 {
            let low = self.chain(LP1, MAX_SEC, c, x[c]);
            let rest = self.chain(HP1, MAX_SEC, c, x[c]);
            let mid = self.chain(LP2, MAX_SEC, c, rest);
            let high = self.chain(HP2, MAX_SEC, c, rest);
            out[0][c] = self.chain(AP2, AP_SEC, c, low);
            out[1][c] = mid;
            out[2][c] = high;
        }
        out
    }
}

/// Samples of input history replayed through a freshly built crossover so it starts warm.
const HIST: usize = 4096;
/// Length of the crossfade when a slope changes (about 10 ms at 48 kHz).
const FADE: usize = 512;

/// The 3-way crossover. A frequency change glides the filter coefficients; a slope change builds the new crossover next to
/// the old one (warmed up on recent input) and crossfades, because the two have different filter structures.
pub struct Crossover3 {
    core: Core,
    old: Option<Core>,
    fade: usize,
    hist: Box<[[f64; 2]; HIST]>,
    hpos: usize,
    sign_mix: [f64; 3],
}

impl Default for Crossover3 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crossover3 {
    pub fn new() -> Self {
        Self { core: Core::new(), old: None, fade: 0, hist: Box::new([[0.0; 2]; HIST]), hpos: 0, sign_mix: [1.0; 3] }
    }

    /// Clamp the pair so f1 < f2 with at least a 5 % gap.
    pub fn order(f1: f64, f2: f64) -> (f64, f64) {
        let f1 = f1.clamp(20.0, 19000.0);
        (f1, f2.max(f1 * 1.05).min(20000.0))
    }

    /// Per-band polarity the engine multiplies each band with when summing (-1 only for the 12 dB/oct high sides).
    #[inline]
    pub fn signs(&self) -> [f64; 3] {
        self.sign_mix
    }

    /// True when nothing is moving: no coefficient glide and no slope crossfade in progress.
    pub fn settled(&self) -> bool {
        self.old.is_none() && !self.core.ramping
    }

    /// New crossover frequencies and slopes (0..3 = 12 / 24 / 48 / 96 dB/oct); `snap` jumps to them (first parameter set).
    /// Otherwise frequencies glide over the next block and a slope change crossfades.
    pub fn set(&mut self, f1: f64, f2: f64, slope1: usize, slope2: usize, sr: f64, snap: bool) {
        let (f1, f2) = Self::order(f1, f2);
        let (s1, s2) = (slope1.min(SLOPES - 1), slope2.min(SLOPES - 1));
        if snap {
            self.old = None;
            self.core.set(f1, f2, s1, s2, sr, true);
            self.sign_mix = self.core.sign_cur;
        } else if (s1, s2) != self.core.slopes {
            let mut fresh = Core::new();
            fresh.set(f1, f2, s1, s2, sr, true);
            // warm it up on the last HIST input samples, oldest first
            for k in 0..HIST {
                let _ = fresh.tick(self.hist[(self.hpos + k) % HIST]);
            }
            self.old = Some(std::mem::replace(&mut self.core, fresh));
            self.fade = 0;
        } else {
            self.core.set(f1, f2, s1, s2, sr, false);
        }
    }

    /// Per host block: ramp towards the targets over `n` samples.
    pub fn begin_block(&mut self, n: usize) {
        self.core.begin_block(n);
        if let Some(o) = self.old.as_mut() {
            o.begin_block(n);
        }
    }

    /// One stereo sample -> natural-polarity [low, mid, high], each [l, r] (sum them with `signs()`).
    #[inline]
    pub fn tick(&mut self, x: [f64; 2]) -> [[f64; 2]; 3] {
        self.hist[self.hpos] = x;
        self.hpos = (self.hpos + 1) % HIST;
        let new = self.core.tick(x);
        let Some(old) = self.old.as_mut() else {
            self.sign_mix = self.core.sign_cur;
            return new;
        };
        let a = (self.fade + 1) as f64 / FADE as f64;
        let prev = old.tick(x);
        let mut out = [[0.0f64; 2]; 3];
        for b in 0..3 {
            for c in 0..2 {
                out[b][c] = prev[b][c] + (new[b][c] - prev[b][c]) * a;
            }
            self.sign_mix[b] = old.sign_cur[b] + (self.core.sign_cur[b] - old.sign_cur[b]) * a;
        }
        self.fade += 1;
        if self.fade >= FADE {
            self.old = None;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn impulse_response(f1: f64, f2: f64, s1: usize, s2: usize) -> ([Vec<f64>; 3], [f64; 3]) {
        let mut x = Crossover3::new();
        x.set(f1, f2, s1, s2, SR, true);
        x.begin_block(1);
        let mut r = [vec![], vec![], vec![]];
        for i in 0..16384 {
            let o = x.tick([if i == 0 { 1.0 } else { 0.0 }, 0.0]);
            for b in 0..3 {
                r[b].push(o[b][0]);
            }
        }
        (r, x.signs())
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
    fn bands_sum_to_a_flat_magnitude_at_every_slope_pair() {
        for s1 in 0..SLOPES {
            for s2 in 0..SLOPES {
                let (ir, sg) = impulse_response(200.0, 2500.0, s1, s2);
                let sum: Vec<f64> = (0..ir[0].len()).map(|i| ir[0][i] * sg[0] + ir[1][i] * sg[1] + ir[2][i] * sg[2]).collect();
                for f in [30.0, 100.0, 200.0, 800.0, 2500.0, 6000.0, 15000.0] {
                    let db = 20.0 * mag(&sum, f).log10();
                    assert!(db.abs() < 0.05, "slopes {s1}/{s2} at {f} Hz: {db} dB");
                }
            }
        }
    }

    #[test]
    fn only_the_12_db_split_inverts_its_high_side() {
        for (s1, s2, want) in [(0, 0, [1.0, -1.0, 1.0]), (1, 1, [1.0, 1.0, 1.0]), (0, 2, [1.0, -1.0, -1.0]), (3, 0, [1.0, 1.0, -1.0])] {
            let (_, sg) = impulse_response(200.0, 2500.0, s1, s2);
            assert_eq!(sg, want, "slopes {s1}/{s2}");
        }
    }

    #[test]
    fn each_band_is_minus_6db_at_its_crossover_and_slope_sets_the_rejection() {
        let mut rej = [0.0f64; SLOPES];
        for s in 0..SLOPES {
            let (ir, _) = impulse_response(200.0, 2500.0, s, s);
            let db = |b: usize, f: f64| 20.0 * mag(&ir[b], f).log10();
            assert!((db(0, 200.0) + 6.02).abs() < 0.3 && (db(1, 200.0) + 6.02).abs() < 0.3, "slope {s}");
            assert!((db(1, 2500.0) + 6.02).abs() < 0.3 && (db(2, 2500.0) + 6.02).abs() < 0.3, "slope {s}");
            rej[s] = db(0, 800.0); // two octaves above the low crossover
            // roughly the nominal dB/oct over the two octaves (less the 6 dB at the corner)
            let want = -(SLOPE_DB[s] as f64) * 2.0;
            assert!(rej[s] < want * 0.6 && rej[s] > want * 1.4 - 8.0, "slope {s}: {} dB at 4x, nominal {want}", rej[s]);
        }
        assert!(rej.windows(2).all(|w| w[1] < w[0] - 6.0), "steeper slopes must reject more: {rej:?}");
    }

    #[test]
    fn gliding_frequencies_and_slopes_stay_finite_and_settle() {
        let mut x = Crossover3::new();
        x.set(150.0, 2500.0, 1, 1, SR, true);
        let (mut peak, mut settled) = (0.0f64, 0.0f64);
        for blk in 0..400 {
            if blk == 50 {
                x.set(400.0, 6000.0, 2, 0, SR, false);
            }
            x.begin_block(64);
            for i in 0..64 {
                let s = (2.0 * std::f64::consts::PI * 330.0 * (blk * 64 + i) as f64 / SR).sin();
                let o = x.tick([s, s]);
                let sg = x.signs();
                let sum = o[0][0] * sg[0] + o[1][0] * sg[1] + o[2][0] * sg[2];
                assert!(sum.is_finite());
                if blk < 100 {
                    peak = peak.max(sum.abs()); // while the crossover moves (a swept tone overshoots a little)
                } else if blk >= 380 {
                    settled = settled.max(sum.abs());
                }
            }
        }
        assert!(peak < 2.0, "{peak}");
        assert!((settled - 1.0).abs() < 0.02, "{settled}");
        assert!(x.settled());
    }

    #[test]
    fn order_keeps_f2_above_f1() {
        let (a, b) = Crossover3::order(3000.0, 100.0);
        assert!(b >= a * 1.05);
    }
}
