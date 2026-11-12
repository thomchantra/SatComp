//! 4-way Linkwitz-Riley crossover: low / low-mid / high-mid / high that sum back to a flat magnitude (all-pass) response.
//!
//! Split 1 at `f1` (LP = low, HP = rest), split 2 at `f2` on the rest (LP = low-mid, HP = rest), split 3 at `f3` on that rest
//! (LP = high-mid, HP = high). Lower bands take all-passes at the splits they do not pass through, so all four bands have
//! the same phase response and sum flat.
//! Each split has its own slope: 12 / 24 / 48 / 96 dB/oct = Butterworth order m = 1 / 2 / 4 / 8 squared (Linkwitz-Riley).
//! LR2 (12 dB/oct) only sums flat with its high side inverted, so the engine sums the bands with `signs()`; the bands
//! themselves keep their natural polarity (so asymmetric saturation behaves the same at every slope).
//! Coefficients glide linearly over each host block when a frequency or slope moves (no zipper noise while dragging).

use satcomp_dsp::biquad::{allpass, bq, hp, lp, Coef, IDENTITY};
use std::f64::consts::PI;

/// Number of bands (and `NB - 1` crossover splits).
pub const NB: usize = 4;
const NS: usize = NB - 1;

/// Butterworth section Qs per order (order 1 is a first-order section, handled separately).
const Q2: [f64; 1] = [0.707_106_781_186_547_5];
const Q4: [f64; 2] = [0.541_196_100_146_197, 1.306_562_964_876_376_4];
const Q8: [f64; 4] = [0.509_795_579_104_159_3, 0.601_344_886_935_045_2, 0.899_976_223_136_415_6, 2.562_915_447_741_505];

/// Number of slope steps and their names (dB/oct).
pub const SLOPES: usize = 4;
pub const SLOPE_DB: [u32; SLOPES] = [12, 24, 48, 96];
const MAX_SEC: usize = 8; // LR8x2: 4 biquads, squared
const AP_SEC: usize = 4;
/// filter slots: per split an LP chain then an HP chain, then the all-pass chains
/// (low band at splits 2 and 3, low-mid band at split 3)
const fn lp_at(s: usize) -> usize {
    2 * s * MAX_SEC
}
const fn hp_at(s: usize) -> usize {
    (2 * s + 1) * MAX_SEC
}
const AP0: usize = 2 * NS * MAX_SEC;
/// all-pass chains: [low @ split 2, low @ split 3, low-mid @ split 3]
const N_AP: usize = 3;
const N: usize = AP0 + N_AP * AP_SEC;

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
    slopes: [usize; NS],
    cur: [Coef; N],
    tgt: [Coef; N],
    delta: [Coef; N],
    st: [[[f64; 2]; 2]; N],
    ramping: bool,
    sign_cur: [f64; NB],
    sign_tgt: [f64; NB],
    sign_delta: [f64; NB],
}

impl Core {
    fn new() -> Self {
        Self {
            slopes: [1; NS],
            cur: [IDENTITY; N],
            tgt: [IDENTITY; N],
            delta: [[0.0; 5]; N],
            st: [[[0.0; 2]; 2]; N],
            ramping: false,
            sign_cur: [1.0; NB],
            sign_tgt: [1.0; NB],
            sign_delta: [0.0; NB],
        }
    }

    /// New crossover frequencies and slopes (0..3 = 12 / 24 / 48 / 96 dB/oct); `snap` jumps to them (first parameter set),
    /// otherwise they glide over the next block.
    fn set(&mut self, f: [f64; NS], s: [usize; NS], sr: f64, snap: bool) {
        self.slopes = s;
        for k in 0..NS {
            fill(&mut self.tgt[lp_at(k)..lp_at(k) + MAX_SEC], Kind::Lp, f[k], s[k], 2, sr);
            fill(&mut self.tgt[hp_at(k)..hp_at(k) + MAX_SEC], Kind::Hp, f[k], s[k], 2, sr);
        }
        // all-passes: low band at splits 2 and 3, low-mid band at split 3
        for (i, k) in [1usize, 2, 2].into_iter().enumerate() {
            fill(&mut self.tgt[AP0 + i * AP_SEC..AP0 + (i + 1) * AP_SEC], Kind::Ap, f[k], s[k], 1, sr);
        }
        let g = |k: usize| if s[k] == 0 { -1.0 } else { 1.0 };
        self.sign_tgt = [1.0, g(0), g(0) * g(1), g(0) * g(1) * g(2)];
        if snap {
            self.cur = self.tgt;
            self.delta = [[0.0; 5]; N];
            self.sign_cur = self.sign_tgt;
            self.sign_delta = [0.0; NB];
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
        for b in 0..NB {
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

    /// One stereo sample -> natural-polarity bands (low .. high), each [l, r] (sum them with the engine's `signs()`).
    #[inline]
    fn tick(&mut self, x: [f64; 2]) -> [[f64; 2]; NB] {
        if self.ramping {
            for f in 0..N {
                for i in 0..5 {
                    self.cur[f][i] += self.delta[f][i];
                }
            }
            for b in 0..NB {
                self.sign_cur[b] += self.sign_delta[b];
            }
        }
        let mut out = [[0.0f64; 2]; NB];
        for c in 0..2 {
            let low = self.chain(lp_at(0), MAX_SEC, c, x[c]);
            let rest1 = self.chain(hp_at(0), MAX_SEC, c, x[c]);
            let ml = self.chain(lp_at(1), MAX_SEC, c, rest1);
            let rest2 = self.chain(hp_at(1), MAX_SEC, c, rest1);
            let mh = self.chain(lp_at(2), MAX_SEC, c, rest2);
            let high = self.chain(hp_at(2), MAX_SEC, c, rest2);
            let low = self.chain(AP0, AP_SEC, c, low);
            out[0][c] = self.chain(AP0 + AP_SEC, AP_SEC, c, low);
            out[1][c] = self.chain(AP0 + 2 * AP_SEC, AP_SEC, c, ml);
            out[2][c] = mh;
            out[3][c] = high;
        }
        out
    }
}

/// Samples of input history replayed through a freshly built crossover so it starts warm.
const HIST: usize = 4096;
/// Length of the crossfade when a slope changes (about 10 ms at 48 kHz).
const FADE: usize = 512;

/// The 4-way crossover. A frequency change glides the filter coefficients; a slope change builds the new crossover next to
/// the old one (warmed up on recent input) and crossfades, because the two have different filter structures.
pub struct Crossover4 {
    core: Core,
    old: Option<Core>,
    fade: usize,
    hist: Box<[[f64; 2]; HIST]>,
    hpos: usize,
    sign_mix: [f64; NB],
}

impl Default for Crossover4 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crossover4 {
    pub fn new() -> Self {
        Self { core: Core::new(), old: None, fade: 0, hist: Box::new([[0.0; 2]; HIST]), hpos: 0, sign_mix: [1.0; NB] }
    }

    /// Clamp the frequencies so f1 < f2 < f3, each at least a 5 % gap above the one below.
    pub fn order(f: [f64; NS]) -> [f64; NS] {
        let f1 = f[0].clamp(20.0, 18000.0);
        let f2 = f[1].max(f1 * 1.05).min(19000.0);
        let f3 = f[2].max(f2 * 1.05).min(20000.0);
        [f1, f2, f3]
    }

    /// Per-band polarity the engine multiplies each band with when summing (-1 only for the 12 dB/oct high sides).
    #[inline]
    pub fn signs(&self) -> [f64; NB] {
        self.sign_mix
    }

    /// True when nothing is moving: no coefficient glide and no slope crossfade in progress.
    pub fn settled(&self) -> bool {
        self.old.is_none() && !self.core.ramping
    }

    /// New crossover frequencies and slopes (0..3 = 12 / 24 / 48 / 96 dB/oct); `snap` jumps to them (first parameter set).
    /// Otherwise frequencies glide over the next block and a slope change crossfades.
    pub fn set(&mut self, freqs: [f64; NS], slopes: [usize; NS], sr: f64, snap: bool) {
        let f = Self::order(freqs);
        let s = slopes.map(|v| v.min(SLOPES - 1));
        if snap {
            self.old = None;
            self.core.set(f, s, sr, true);
            self.sign_mix = self.core.sign_cur;
        } else if s != self.core.slopes {
            let mut fresh = Core::new();
            fresh.set(f, s, sr, true);
            // warm it up on the last HIST input samples, oldest first
            for k in 0..HIST {
                let _ = fresh.tick(self.hist[(self.hpos + k) % HIST]);
            }
            self.old = Some(std::mem::replace(&mut self.core, fresh));
            self.fade = 0;
        } else {
            self.core.set(f, s, sr, false);
        }
    }

    /// Per host block: ramp towards the targets over `n` samples.
    pub fn begin_block(&mut self, n: usize) {
        self.core.begin_block(n);
        if let Some(o) = self.old.as_mut() {
            o.begin_block(n);
        }
    }

    /// One stereo sample -> natural-polarity bands (low .. high), each [l, r] (sum them with `signs()`).
    #[inline]
    pub fn tick(&mut self, x: [f64; 2]) -> [[f64; 2]; NB] {
        self.hist[self.hpos] = x;
        self.hpos = (self.hpos + 1) % HIST;
        let new = self.core.tick(x);
        let Some(old) = self.old.as_mut() else {
            self.sign_mix = self.core.sign_cur;
            return new;
        };
        let a = (self.fade + 1) as f64 / FADE as f64;
        let prev = old.tick(x);
        let mut out = [[0.0f64; 2]; NB];
        for b in 0..NB {
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
    const F: [f64; NS] = [200.0, 1200.0, 5000.0];

    fn impulse_response(f: [f64; NS], s: [usize; NS]) -> ([Vec<f64>; NB], [f64; NB]) {
        let mut x = Crossover4::new();
        x.set(f, s, SR, true);
        x.begin_block(1);
        let mut r: [Vec<f64>; NB] = Default::default();
        for i in 0..16384 {
            let o = x.tick([if i == 0 { 1.0 } else { 0.0 }, 0.0]);
            for b in 0..NB {
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
    fn bands_sum_to_a_flat_magnitude_at_every_slope_combination() {
        for s1 in 0..SLOPES {
            for s2 in 0..SLOPES {
                for s3 in 0..SLOPES {
                    // every combination, thinned: all uniform ones plus a spread of mixed ones
                    if !((s1 == s2 && s2 == s3) || (s1 + 2 * s2 + 3 * s3) % 5 == 0) {
                        continue;
                    }
                    let (ir, sg) = impulse_response(F, [s1, s2, s3]);
                    let sum: Vec<f64> = (0..ir[0].len()).map(|i| (0..NB).map(|b| ir[b][i] * sg[b]).sum()).collect();
                    for f in [30.0, 100.0, 200.0, 600.0, 1200.0, 3000.0, 5000.0, 9000.0, 15000.0] {
                        let db = 20.0 * mag(&sum, f).log10();
                        assert!(db.abs() < 0.05, "slopes {s1}/{s2}/{s3} at {f} Hz: {db} dB");
                    }
                }
            }
        }
    }

    #[test]
    fn only_the_12_db_splits_invert_their_high_side() {
        for (s, want) in [
            ([0, 0, 0], [1.0, -1.0, 1.0, -1.0]),
            ([1, 1, 1], [1.0, 1.0, 1.0, 1.0]),
            ([0, 2, 1], [1.0, -1.0, -1.0, -1.0]),
            ([3, 0, 2], [1.0, 1.0, -1.0, -1.0]),
            ([1, 1, 0], [1.0, 1.0, 1.0, -1.0]),
        ] {
            let (_, sg) = impulse_response(F, s);
            assert_eq!(sg, want, "slopes {s:?}");
        }
    }

    #[test]
    fn each_band_is_minus_6db_at_its_crossover_and_slope_sets_the_rejection() {
        let mut rej = [0.0f64; SLOPES];
        for s in 0..SLOPES {
            let (ir, _) = impulse_response(F, [s; NS]);
            let db = |b: usize, f: f64| 20.0 * mag(&ir[b], f).log10();
            for (k, f) in F.iter().enumerate() {
                // the inner splits sit close to their neighbours' skirts, so they may stray a little further from -6 dB
                let tol = if k == 0 { 0.3 } else { 1.2 };
                assert!((db(k, *f) + 6.02).abs() < tol && (db(k + 1, *f) + 6.02).abs() < tol, "slope {s} split {k}: {} / {}", db(k, *f), db(k + 1, *f));
            }
            rej[s] = db(0, 800.0); // two octaves above the low crossover
            // roughly the nominal dB/oct over the two octaves (less the 6 dB at the corner)
            let want = -(SLOPE_DB[s] as f64) * 2.0;
            assert!(rej[s] < want * 0.6 && rej[s] > want * 1.4 - 8.0, "slope {s}: {} dB at 4x, nominal {want}", rej[s]);
        }
        assert!(rej.windows(2).all(|w| w[1] < w[0] - 6.0), "steeper slopes must reject more: {rej:?}");
    }

    #[test]
    fn gliding_frequencies_and_slopes_stay_finite_and_settle() {
        let mut x = Crossover4::new();
        x.set([150.0, 900.0, 3500.0], [1; NS], SR, true);
        let (mut peak, mut settled) = (0.0f64, 0.0f64);
        for blk in 0..400 {
            if blk == 50 {
                x.set([400.0, 2000.0, 6000.0], [2, 0, 1], SR, false);
            }
            x.begin_block(64);
            for i in 0..64 {
                let s = (2.0 * std::f64::consts::PI * 330.0 * (blk * 64 + i) as f64 / SR).sin();
                let o = x.tick([s, s]);
                let sg = x.signs();
                let sum: f64 = (0..NB).map(|b| o[b][0] * sg[b]).sum();
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
    fn order_keeps_each_split_above_the_one_below() {
        let [a, b, c] = Crossover4::order([3000.0, 100.0, 50.0]);
        assert!(b >= a * 1.05 && c >= b * 1.05);
    }
}
