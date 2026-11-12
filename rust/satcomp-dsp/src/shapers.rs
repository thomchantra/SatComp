//! The 14 waveshapers and the +-8 lookup table (4097 points) the JSFX uses.

pub const LUT_LEN: usize = 4097;
pub const SHAPER_NAMES: [&str; 15] = [
    "Tube", "Soft", "Rational", "Atan", "Hard", "Tri Fold", "Sine Fold", "Crossover", "Cheby", "Wrap",
    "Rectify", "Bitcrush", "Asym Fold", "Diode", "Custom",
];
/// Number of waveshapers (14 built-in + custom).
pub const SHAPER_COUNT: usize = SHAPER_NAMES.len();

/// Persisted per-shaper ASYM values (0..100 %) -> the DSP array (missing / non-finite entries = 100 %, clamped).
pub fn asym_from(v: &[f32]) -> [f64; SHAPER_COUNT] {
    let mut a = [100.0; SHAPER_COUNT];
    for (d, s) in a.iter_mut().zip(v) {
        if s.is_finite() {
            *d = (*s as f64).clamp(0.0, 100.0);
        }
    }
    a
}

/// Shaper index of the user-drawn curve.
pub const CUSTOM: i32 = 14;

/// Max points of the custom curve, including the two fixed-x end points.
pub const MAX_PTS: usize = 10;
/// Minimum x distance between neighbouring points.
pub const MIN_GAP: f32 = 0.02;

/// User-drawn transfer curve on x in [-1, 1]: sorted by x, first/last x fixed at -1/+1,
/// y in [-1, 1]. Plain `Copy` data so it can ride inside the DSP `Params` snapshot.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Curve {
    pub n: usize,
    pub p: [(f32, f32); MAX_PTS],
}

impl Default for Curve {
    fn default() -> Self {
        Self::linear()
    }
}

impl Curve {
    pub fn linear() -> Self {
        let mut p = [(0.0, 0.0); MAX_PTS];
        p[0] = (-1.0, -1.0);
        p[1] = (1.0, 1.0);
        Self { n: 2, p }
    }

    pub fn pts(&self) -> &[(f32, f32)] {
        &self.p[..self.n]
    }

    /// Sanitising constructor (persisted/preset data is untrusted): sorts, clamps, enforces end points
    /// and the gap, truncates to MAX_PTS. Falls back to linear if fewer than 2 points.
    pub fn from_points(pts: &[(f32, f32)]) -> Self {
        // No heap allocation: this runs on the audio thread every block (a malloc there can stall behind the UI).
        const CAP: usize = 32;
        let mut v = [(0.0f32, 0.0f32); CAP];
        let mut n = 0;
        for &(x, y) in pts {
            if n < CAP && x.is_finite() && y.is_finite() {
                v[n] = (x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0));
                n += 1;
            }
        }
        if n < 2 {
            return Self::linear();
        }
        let v = &mut v[..n];
        v.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let mut c = Self::linear();
        c.p[0] = (-1.0, v[0].1);
        // interior points only, kept inside the gap-padded range
        let mut cnt = 1;
        let mut lo = -1.0 + MIN_GAP;
        for &(x, y) in &v[1..n - 1] {
            if cnt + 1 >= MAX_PTS {
                break;
            }
            if x >= lo && x <= 1.0 - MIN_GAP {
                c.p[cnt] = (x, y);
                cnt += 1;
                lo = x + MIN_GAP;
            }
        }
        c.p[cnt] = (1.0, v[n - 1].1);
        c.n = cnt + 1;
        c
    }

    /// True when the curve is (nearly) odd, `f(-x) = -f(x)`: the symmetry tool can edit it without changing it.
    pub fn is_odd(&self) -> bool {
        (0..=20).all(|i| {
            let x = i as f64 * 0.05;
            (self.eval(x) + self.eval(-x)).abs() < 0.02
        })
    }

    /// The right half of the curve's odd part: points at the |x| of its interior points (at most `MAX_PTS / 2 - 1`) and at
    /// x = 1, with y = (f(x) - f(-x)) / 2. Feed it to `from_odd_half`.
    pub fn odd_half(&self) -> Vec<(f32, f32)> {
        let mut xs: Vec<f32> = self.pts().iter().filter(|p| p.0 > -1.0 && p.0 < 1.0).map(|p| p.0.abs()).filter(|x| *x >= MIN_GAP).collect();
        xs.sort_by(|a, b| a.total_cmp(b));
        let mut keep: Vec<f32> = Vec::new();
        for x in xs {
            if keep.last().map_or(true, |l| x - l >= MIN_GAP) && x <= 1.0 - MIN_GAP {
                keep.push(x);
            }
        }
        keep.truncate(MAX_PTS / 2 - 1);
        keep.push(1.0);
        keep.into_iter().map(|x| (x, ((self.eval(x as f64) - self.eval(-x as f64)) * 0.5) as f32)).collect()
    }

    /// The full odd curve of a right half (x > 0, ending at x = 1): the left half is the point-mirror image.
    pub fn from_odd_half(h: &[(f32, f32)]) -> Self {
        let mut v: Vec<(f32, f32)> = h.iter().rev().map(|&(x, y)| (-x, -y)).collect();
        v.extend_from_slice(h);
        Self::from_points(&v)
    }

    /// Value of the monotone cubic (Fritsch-Carlson) through the points; flat beyond the ends.
    /// Never overshoots between points, so the transfer curve stays single-valued and tame.
    pub fn eval(&self, x: f64) -> f64 {
        let pts = self.pts();
        let n = pts.len();
        let xs = |i: usize| pts[i].0 as f64;
        let ys = |i: usize| pts[i].1 as f64;
        if x <= xs(0) {
            return ys(0);
        }
        if x >= xs(n - 1) {
            return ys(n - 1);
        }
        let mut k = 0;
        while k + 2 < n && x > xs(k + 1) {
            k += 1;
        }
        let delta = |i: usize| (ys(i + 1) - ys(i)) / (xs(i + 1) - xs(i)).max(1e-9);
        let tangent = |i: usize| -> f64 {
            if i == 0 {
                return delta(0);
            }
            if i == n - 1 {
                return delta(n - 2);
            }
            let (d0, d1) = (delta(i - 1), delta(i));
            if d0 * d1 <= 0.0 {
                return 0.0;
            }
            // weighted harmonic mean (keeps monotonicity for uneven spacing)
            let (h0, h1) = (xs(i) - xs(i - 1), xs(i + 1) - xs(i));
            let (w1, w2) = (2.0 * h1 + h0, h1 + 2.0 * h0);
            (w1 + w2) / (w1 / d0 + w2 / d1)
        };
        let h = xs(k + 1) - xs(k);
        let t = (x - xs(k)) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * ys(k)
            + (t3 - 2.0 * t2 + t) * h * tangent(k)
            + (-2.0 * t3 + 3.0 * t2) * ys(k + 1)
            + (t3 - t2) * h * tangent(k + 1)
    }
}

fn th(x: f64) -> f64 {
    let x = x.clamp(-20.0, 20.0);
    let e = (2.0 * x).exp();
    (e - 1.0) / (e + 1.0)
}

/// EEL2 `sign`: -1, 0, +1.
fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

pub fn shaper(sel: i32, x: f64) -> f64 {
    match sel {
        0 => {
            if x >= 0.0 {
                th(x)
            } else {
                th(0.6 * x) / 0.6
            }
        }
        1 => th(x),
        2 => x / (1.0 + 1.5 * x.abs()),
        3 => 0.63661977 * (1.5707963 * x).atan(),
        4 => {
            let ax = x.abs();
            if ax <= 0.9 {
                x
            } else {
                sign(x) * (0.9 + 0.1 * th((ax - 0.9) * 10.0))
            }
        }
        5 => 0.63661977 * (1.5707963 * x).sin().asin(),
        6 => (1.5707963 * x).sin() * 0.63661977,
        7 => x - 0.12 * th(x / 0.15),
        8 => {
            let t = th(x);
            1.6 * t + 0.6 * t * t + 0.2 * (4.0 * t * t * t - 3.0 * t)
        }
        9 => x - 2.0 * ((x + 1.0) * 0.5).floor(),
        10 => 0.25 * th(x) + 0.75 * th(x.abs()),
        11 => ((x * 8.0 + 0.5).floor() * 0.125).clamp(-1.0, 1.0),
        12 => ((1.5707963 * (x + 0.35)).sin() - 0.52249856) * 0.63661977 / 0.85264016,
        13 => {
            if x >= 0.0 {
                1.0 - (-x).exp()
            } else {
                -0.6 * (1.0 - (x / 0.6).exp())
            }
        }
        _ => 0.0,
    }
}

/// Periodic shapers (Tri Fold, Sine Fold, Wrap, Asym Fold) are range-reduced rather than clamped.
pub fn is_periodic(sel: i32) -> bool {
    matches!(sel, 5 | 6 | 9 | 12)
}

pub struct ShaperLut {
    pub table: Vec<f64>,
    pub periodic: bool,
    pub sel: i32,
    /// Curve the table was last baked from (only meaningful while `sel == CUSTOM`).
    pub curve: Curve,
    /// Audio use: morph to a newly built table over `FADE_STEPS` x `FADE_EVERY` samples instead of
    /// swapping it (an instant swap is a step in the output = click). The UI plot leaves this off.
    pub fade: bool,
    tgt: Vec<f64>,
    steps: u32,
}

/// Table morph: `FADE_STEPS` updates, one every `FADE_EVERY` samples (~10 ms at 48 kHz).
pub const FADE_STEPS: u32 = 16;
pub const FADE_EVERY: u32 = 32;

impl ShaperLut {
    pub fn new(sel: i32) -> Self {
        let mut l = Self { table: vec![0.0; LUT_LEN], periodic: false, sel: -1, curve: Curve::linear(), fade: false, tgt: vec![0.0; LUT_LEN], steps: 0 };
        l.build(sel);
        l
    }

    pub fn build(&mut self, sel: i32) {
        for (i, v) in self.tgt.iter_mut().enumerate() {
            *v = shaper(sel, i as f64 / 256.0 - 8.0);
        }
        self.periodic = is_periodic(sel);
        self.commit(sel);
    }

    /// New target is in `tgt`: snap to it, or start the morph when fading and a table already exists.
    fn commit(&mut self, sel: i32) {
        if self.fade && self.sel != -1 {
            self.steps = FADE_STEPS;
        } else {
            self.table.copy_from_slice(&self.tgt);
            self.steps = 0;
        }
        self.sel = sel;
    }

    /// Advance the morph by one step (call every `FADE_EVERY` samples; cheap no-op when idle).
    pub fn step(&mut self) {
        if self.steps == 0 {
            return;
        }
        let k = 1.0 / self.steps as f64;
        for (v, t) in self.table.iter_mut().zip(&self.tgt) {
            *v += (t - *v) * k;
        }
        self.steps -= 1;
    }

    /// Bake the custom curve into the table (flat outside +-1, like a hard clip).
    pub fn build_custom(&mut self, curve: &Curve) {
        for (i, v) in self.tgt.iter_mut().enumerate() {
            *v = curve.eval(i as f64 / 256.0 - 8.0);
        }
        self.periodic = false;
        self.curve = *curve;
        self.commit(CUSTOM);
    }

    /// Make the table match `sel` (and `curve` when custom); no-op when already current.
    pub fn sync(&mut self, sel: i32, curve: &Curve) {
        if sel == CUSTOM {
            if self.sel != CUSTOM || self.curve != *curve {
                self.build_custom(curve);
            }
        } else if self.sel != sel {
            self.build(sel);
        }
    }

    #[inline]
    pub fn eval(&self, mut x: f64) -> f64 {
        if self.periodic && x.abs() > 6.0 {
            x -= 4.0 * (x * 0.25 + 0.5).floor();
        }
        let p = (x.clamp(-7.999, 7.999) + 8.0) * 256.0;
        let i = p.floor();
        let f = p - i;
        let i = i as usize;
        self.table[i] + (self.table[i + 1] - self.table[i]) * f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_small_signal_slope() {
        // all shapers except Crossover (unity large-signal), Rectify, Bitcrush, Wrap-ish steps
        for sel in [0, 1, 2, 3, 4, 5, 6, 8, 12, 13] {
            let e = 1e-4;
            let slope = (shaper(sel, e) - shaper(sel, -e)) / (2.0 * e);
            // Tube/Diode are asymmetric so the average slope is still ~1 around the origin
            assert!((slope - 1.0).abs() < 0.02, "shaper {sel} slope {slope}");
        }
    }

    #[test]
    fn bounded_and_odd_where_expected() {
        for x in [0.3, 1.0, 3.0, 7.0] {
            assert!((shaper(1, x) + shaper(1, -x)).abs() < 1e-12); // Soft is odd
            assert!(shaper(1, x).abs() <= 1.0);
            assert!((shaper(2, x) + shaper(2, -x)).abs() < 1e-12); // Rational odd
        }
        assert!(shaper(0, 2.0) < -shaper(0, -2.0)); // Tube asymmetric: negative side saturates later (th(0.6x)/0.6)
        assert_eq!(shaper(9, 0.0), 0.0);
        assert!((shaper(9, 1.5) - (-0.5)).abs() < 1e-12); // wrap
        assert!((shaper(11, 0.51) - 0.5).abs() < 1e-12); // bitcrush step
    }

    #[test]
    fn custom_linear_is_identity_and_curve_is_monotone_without_overshoot() {
        let lin = Curve::linear();
        for x in [-1.0, -0.37, 0.0, 0.5, 1.0] {
            assert!((lin.eval(x) - x).abs() < 1e-9);
        }
        assert_eq!(lin.eval(3.0), 1.0); // flat beyond the ends
        // steep knee: values must stay between neighbouring point values
        let c = Curve::from_points(&[(-1.0, -1.0), (-0.2, -0.1), (0.1, 0.9), (1.0, 1.0)]);
        assert_eq!(c.n, 4);
        let mut prev = -2.0;
        for i in 0..=400 {
            let y = c.eval(-1.0 + i as f64 / 200.0);
            assert!(y >= prev - 1e-9 && (-1.0..=1.0).contains(&y), "y={y}");
            prev = y;
        }
        // passes through every point
        for &(x, y) in c.pts() {
            assert!((c.eval(x as f64) - y as f64).abs() < 1e-6);
        }
    }

    #[test]
    fn from_points_sanitises() {
        assert_eq!(Curve::from_points(&[]), Curve::linear());
        assert_eq!(Curve::from_points(&[(0.0, 0.0)]), Curve::linear());
        let many: Vec<(f32, f32)> = (0..30).map(|i| (-1.0 + i as f32 / 14.5, 0.5)).collect();
        assert_eq!(Curve::from_points(&many).n, MAX_PTS);
        let c = Curve::from_points(&[(0.5, 2.0), (f32::NAN, 0.0), (-3.0, -2.0), (0.5, 0.1)]);
        assert_eq!((c.p[0].0, c.p[0].1), (-1.0, -1.0)); // clamped end y
        assert!(c.pts().windows(2).all(|w| w[1].0 - w[0].0 >= MIN_GAP - 1e-6));
    }

    #[test]
    fn fade_morphs_without_a_step_and_lands_on_target() {
        let mut l = ShaperLut::new(1);
        l.fade = true;
        let before = l.eval(0.5);
        let c = Curve::from_points(&[(-1.0, -1.0), (1.0, 0.2)]);
        let target = c.eval(0.5);
        l.sync(CUSTOM, &c);
        assert_eq!(l.eval(0.5), before); // nothing jumps at the swap
        let mut last = before;
        for _ in 0..FADE_STEPS {
            l.step();
            let v = l.eval(0.5);
            assert!((v - last).abs() < (before - target).abs() * 0.5, "step too big: {last} -> {v}");
            last = v;
        }
        assert!((l.eval(0.5) - target).abs() < 1e-3);
        l.step(); // idle step is harmless
    }

    #[test]
    fn sync_rebuilds_only_on_change() {
        let mut l = ShaperLut::new(1);
        let mut c = Curve::linear();
        l.sync(CUSTOM, &c);
        assert!((l.eval(0.3) - 0.3).abs() < 1e-3);
        c.p[1].1 = 0.5;
        l.sync(CUSTOM, &c);
        assert!((l.eval(1.0) - 0.5).abs() < 1e-3);
        l.sync(2, &c);
        assert_eq!(l.sel, 2);
    }

    #[test]
    fn lut_matches_direct_and_folds_beyond_range() {
        let lut = ShaperLut::new(6); // Sine Fold, periodic
        for x in [-3.3, -0.7, 0.0, 0.123, 2.5] {
            assert!((lut.eval(x) - shaper(6, x)).abs() < 1e-4, "x={x}");
        }
        // range reduction: period 4, so x and x+8 (outside +-6 after reduction) agree
        assert!((lut.eval(9.3) - shaper(6, 9.3)).abs() < 1e-3);
        let soft = ShaperLut::new(1);
        assert!((soft.eval(50.0) - shaper(1, 7.999)).abs() < 1e-3); // clamped, not folded
    }

    #[test]
    fn odd_half_round_trips_and_folds_a_lopsided_curve() {
        let lop = Curve::from_points(&[(-1.0, -0.6), (-0.4, -0.1), (0.5, 0.8), (1.0, 1.0)]);
        assert!(!lop.is_odd());
        let odd = Curve::from_odd_half(&lop.odd_half());
        assert!(odd.is_odd(), "{:?}", odd.pts());
        // the odd part is what the fold keeps: f(1) = (1.0 + 0.6) / 2
        assert!((odd.eval(1.0) - 0.8).abs() < 1e-6 && (odd.eval(-1.0) + 0.8).abs() < 1e-6);
        assert!(Curve::linear().is_odd());
        assert_eq!(Curve::from_odd_half(&Curve::linear().odd_half()).pts(), Curve::linear().pts());
    }
}
