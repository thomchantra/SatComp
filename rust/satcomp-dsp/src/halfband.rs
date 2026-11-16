//! Kaiser halfband filters + the shaper oversampling wrapper (x2: K=24 / x4: +K=8 / x8: +K=8 again).
//! Port of mk_taps / hb_up / hb_down / sh_proc; latency 24 (x2) / 28 (x4) / 30 (x8, SatLim only) samples.

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

/// One halfband stage of `K` taps (the filter is `2 * K` long, symmetric: `h` holds the first `K / 2` taps, see `Taps`). `N` = `2 * K`
/// (a const-generic expression is not allowed in an array length). The histories are linear double buffers: every sample is
/// written at `p` and `p + K`, so the last `K` samples are always one contiguous slice (no masked ring indexing), and the taps are
/// summed pairwise (symmetry halves the multiplies) over four independent accumulators (no 24-step dependency chain).
#[derive(Clone)]
pub struct Hb<const K: usize, const N: usize> {
    up_p: usize,
    up: [f64; N],
    /// down: the previous `b` samples (what the taps weigh) and the `a` samples (the centre tap)
    db_p: usize,
    db: [f64; N],
    da_p: usize,
    da: [f64; N],
}

impl<const K: usize, const N: usize> Default for Hb<K, N> {
    fn default() -> Self {
        Self { up_p: 0, up: [0.0; N], db_p: 0, db: [0.0; N], da_p: 0, da: [0.0; N] }
    }
}

impl<const K: usize, const N: usize> Hb<K, N> {
    /// Symmetric FIR over `win` (K samples, oldest first) with the first K / 2 taps `h`.
    #[inline(always)]
    fn fir(win: &[f64], h: &[f64]) -> f64 {
        let mut a = [0.0f64; 4];
        for (hc, j) in h.chunks_exact(4).zip((0..K / 2).step_by(4)) {
            for u in 0..4 {
                a[u] += hc[u] * (win[j + u] + win[K - 1 - j - u]);
            }
        }
        (a[0] + a[1]) + (a[2] + a[3])
    }

    #[inline]
    pub fn up(&mut self, h: &[f64], x: f64) -> (f64, f64) {
        let p = self.up_p;
        self.up[p] = x;
        self.up[p + K] = x;
        // newest sample at p + K, window oldest..newest = [p + 1, p + K]
        let y1 = 2.0 * Self::fir(&self.up[p + 1..p + 1 + K], h);
        let y0 = self.up[p + K - K / 2];
        self.up_p = if p + 1 == K { 0 } else { p + 1 };
        (y0, y1)
    }

    #[inline]
    pub fn down(&mut self, h: &[f64], a: f64, b: f64) -> f64 {
        // the taps weigh the previous `b` samples, newest first; the centre tap is the `a` sample K / 2 pairs back
        let qa = self.da_p;
        self.da[qa] = a;
        self.da[qa + K] = a;
        let centre = 0.5 * self.da[qa + K - K / 2];
        let qb = self.db_p;
        let acc = centre + Self::fir(&self.db[qb..qb + K], h);
        self.db[qb] = b;
        self.db[qb + K] = b;
        self.db_p = if qb + 1 == K { 0 } else { qb + 1 };
        self.da_p = if qa + 1 == K { 0 } else { qa + 1 };
        acc
    }
}

#[derive(Clone, Default)]
pub struct Oversampler {
    a: Hb<24, 48>,
    b: Hb<8, 16>,
    c: Hb<8, 16>,
}

/// PDC latency (base-rate samples) of the oversampling chain for `os`: 0 off, 1 x2, 2 x4, 3 x8.
pub fn latency(os: i32) -> usize {
    match os {
        0 => 0,
        1 => 24,
        2 => 28,
        _ => 30,
    }
}

pub struct Taps {
    pub a: Vec<f64>,
    pub b: Vec<f64>,
    /// the first half of each (they are symmetric): what the stages use
    pub ha: Vec<f64>,
    pub hb: Vec<f64>,
}

impl Taps {
    pub fn new() -> Self {
        let (a, b) = (make_taps(24, 8.0), make_taps(8, 6.0));
        let (ha, hb) = (a[..12].to_vec(), b[..4].to_vec());
        Self { a, b, ha, hb }
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

    /// os: 0 off, 1 x2, 2 x4, 3 x8.
    #[inline]
    pub fn process(&mut self, taps: &Taps, lut: &ShaperLut, os: i32, x: f64) -> f64 {
        self.process_fn(taps, os, x, |v| lut.eval(v))
    }

    /// As `process`, with any memoryless nonlinearity `f` run at the oversampled rate.
    #[inline]
    pub fn process_fn(&mut self, taps: &Taps, os: i32, x: f64, f: impl Fn(f64) -> f64) -> f64 {
        match os {
            0 => f(x),
            1 => {
                let (a0, a1) = self.a.up(&taps.ha, x);
                self.a.down(&taps.ha, f(a0), f(a1))
            }
            2 => {
                let (a0, a1) = self.a.up(&taps.ha, x);
                let (b0, b1) = self.b.up(&taps.hb, a0);
                let (b2, b3) = self.b.up(&taps.hb, a1);
                let c0 = self.b.down(&taps.hb, f(b0), f(b1));
                let c1 = self.b.down(&taps.hb, f(b2), f(b3));
                self.a.down(&taps.ha, c0, c1)
            }
            _ => {
                let (a0, a1) = self.a.up(&taps.ha, x);
                let (b0, b1) = self.b.up(&taps.hb, a0);
                let (b2, b3) = self.b.up(&taps.hb, a1);
                let (c0, c1) = self.c.up(&taps.hb, b0);
                let (c2, c3) = self.c.up(&taps.hb, b1);
                let (c4, c5) = self.c.up(&taps.hb, b2);
                let (c6, c7) = self.c.up(&taps.hb, b3);
                let d0 = self.c.down(&taps.hb, f(c0), f(c1));
                let d1 = self.c.down(&taps.hb, f(c2), f(c3));
                let d2 = self.c.down(&taps.hb, f(c4), f(c5));
                let d3 = self.c.down(&taps.hb, f(c6), f(c7));
                let e0 = self.b.down(&taps.hb, d0, d1);
                let e1 = self.b.down(&taps.hb, d2, d3);
                self.a.down(&taps.ha, e0, e1)
            }
        }
    }
}

impl Oversampler {
    /// Like `process`, plus the even part e(x) = (f(x) + f(-x)) / 2 of the shaper through the same up / down filters
    /// (so the two are sample-aligned and `f - e` is the odd part). Returns (f, e).
    #[inline]
    pub fn process_fe(&mut self, taps: &Taps, lut: &ShaperLut, os: i32, x: f64, eb: &mut Oversampler) -> (f64, f64) {
        let ev = |v: f64| 0.5 * (lut.eval(v) + lut.eval(-v));
        match os {
            0 => (lut.eval(x), ev(x)),
            1 => {
                let (a0, a1) = self.a.up(&taps.ha, x);
                let f = self.a.down(&taps.ha, lut.eval(a0), lut.eval(a1));
                let e = eb.a.down(&taps.ha, ev(a0), ev(a1));
                (f, e)
            }
            2 => {
                let (a0, a1) = self.a.up(&taps.ha, x);
                let (b0, b1) = self.b.up(&taps.hb, a0);
                let (b2, b3) = self.b.up(&taps.hb, a1);
                let c0 = self.b.down(&taps.hb, lut.eval(b0), lut.eval(b1));
                let c1 = self.b.down(&taps.hb, lut.eval(b2), lut.eval(b3));
                let f = self.a.down(&taps.ha, c0, c1);
                let d0 = eb.b.down(&taps.hb, ev(b0), ev(b1));
                let d1 = eb.b.down(&taps.hb, ev(b2), ev(b3));
                let e = eb.a.down(&taps.ha, d0, d1);
                (f, e)
            }
            _ => {
                let (a0, a1) = self.a.up(&taps.ha, x);
                let (b0, b1) = self.b.up(&taps.hb, a0);
                let (b2, b3) = self.b.up(&taps.hb, a1);
                let u = [b0, b1, b2, b3];
                let mut c = [0.0f64; 8];
                for (i, v) in u.iter().enumerate() {
                    let (p, q) = self.c.up(&taps.hb, *v);
                    c[2 * i] = p;
                    c[2 * i + 1] = q;
                }
                let mut d = [0.0f64; 4];
                let mut de = [0.0f64; 4];
                for i in 0..4 {
                    d[i] = self.c.down(&taps.hb, lut.eval(c[2 * i]), lut.eval(c[2 * i + 1]));
                    de[i] = eb.c.down(&taps.hb, ev(c[2 * i]), ev(c[2 * i + 1]));
                }
                let e0 = self.b.down(&taps.hb, d[0], d[1]);
                let e1 = self.b.down(&taps.hb, d[2], d[3]);
                let f = self.a.down(&taps.ha, e0, e1);
                let g0 = eb.b.down(&taps.hb, de[0], de[1]);
                let g1 = eb.b.down(&taps.hb, de[2], de[3]);
                let e = eb.a.down(&taps.ha, g0, g1);
                (f, e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The previous implementation (masked rings, serial accumulation), kept as the reference for the rewrite.
    struct RefHb {
        up_pos: usize,
        down_pos: usize,
        up: [f64; 32],
        down: [f64; 64],
    }
    impl RefHb {
        fn new() -> Self {
            Self { up_pos: 0, down_pos: 0, up: [0.0; 32], down: [0.0; 64] }
        }
        fn up(&mut self, tp: &[f64], k: usize, um: usize, x: f64) -> (f64, f64) {
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
        fn down(&mut self, tp: &[f64], k: usize, dm: usize, a: f64, b: f64) -> f64 {
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

    #[test]
    fn taps_are_symmetric() {
        let t = Taps::new();
        for v in [&t.a, &t.b] {
            for j in 0..v.len() {
                assert_eq!(v[j], v[v.len() - 1 - j]);
            }
        }
    }

    /// The rewritten stages give the old stages' output (up and down, both filter lengths) to rounding.
    #[test]
    fn rewritten_stages_match_the_reference() {
        let t = Taps::new();
        let mut s = 987654321u64;
        let mut rnd = || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
        };
        let (mut ra, mut na) = (RefHb::new(), Hb::<24, 48>::default());
        let (mut rb, mut nb) = (RefHb::new(), Hb::<8, 16>::default());
        let mut worst = 0.0f64;
        for _ in 0..20000 {
            let x = rnd();
            let (r0, r1) = ra.up(&t.a, 24, 31, x);
            let (n0, n1) = na.up(&t.ha, x);
            worst = worst.max((r0 - n0).abs()).max((r1 - n1).abs());
            let (a, b) = (rnd(), rnd());
            worst = worst.max((ra.down(&t.a, 24, 63, a, b) - na.down(&t.ha, a, b)).abs());
            let (r0, r1) = rb.up(&t.b, 8, 7, x);
            let (n0, n1) = nb.up(&t.hb, x);
            worst = worst.max((r0 - n0).abs()).max((r1 - n1).abs());
            worst = worst.max((rb.down(&t.b, 8, 15, a, b) - nb.down(&t.hb, a, b)).abs());
        }
        assert!(worst < 1e-12, "worst difference {worst}");
    }

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
        for (os, lat) in [(1, 24usize), (2, 28usize), (3, 30usize)] {
            assert_eq!(latency(os), lat);
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
