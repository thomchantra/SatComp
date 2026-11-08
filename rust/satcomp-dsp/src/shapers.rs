//! The 14 waveshapers and the +-8 lookup table (4097 points) the JSFX uses.

pub const LUT_LEN: usize = 4097;
pub const SHAPER_NAMES: [&str; 14] = [
    "Tube", "Soft", "Rational", "Atan", "Hard", "Tri Fold", "Sine Fold", "Crossover", "Cheby", "Wrap",
    "Rectify", "Bitcrush", "Asym Fold", "Diode",
];

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
}

impl ShaperLut {
    pub fn new(sel: i32) -> Self {
        let mut l = Self { table: vec![0.0; LUT_LEN], periodic: false, sel: -1 };
        l.build(sel);
        l
    }

    pub fn build(&mut self, sel: i32) {
        for (i, v) in self.table.iter_mut().enumerate() {
            *v = shaper(sel, i as f64 / 256.0 - 8.0);
        }
        self.periodic = is_periodic(sel);
        self.sel = sel;
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
}
