//! Biquads (transposed direct form II), RBJ coefficients. Coeffs = [b0, b1, b2, a1, a2].
use crate::DENORM;
use std::f64::consts::PI;

pub type Coef = [f64; 5];
pub const IDENTITY: Coef = [1.0, 0.0, 0.0, 0.0, 0.0];

#[inline]
pub fn bq(s: &mut [f64; 2], c: &Coef, x: f64) -> f64 {
    let y = c[0] * x + s[0] + DENORM;
    s[0] = c[1] * x - c[3] * y + s[1];
    s[1] = c[2] * x - c[4] * y;
    y
}

fn w0(f: f64, sr: f64) -> f64 {
    2.0 * PI * f.min(sr * 0.45) / sr
}

pub fn lp(f: f64, q: f64, sr: f64) -> Coef {
    let w = w0(f, sr);
    let (cs, al) = (w.cos(), w.sin() / (2.0 * q));
    let a0 = 1.0 + al;
    let b0 = (1.0 - cs) * 0.5 / a0;
    [b0, (1.0 - cs) / a0, b0, -2.0 * cs / a0, (1.0 - al) / a0]
}

pub fn hp(f: f64, q: f64, sr: f64) -> Coef {
    let w = w0(f, sr);
    let (cs, al) = (w.cos(), w.sin() / (2.0 * q));
    let a0 = 1.0 + al;
    let b0 = (1.0 + cs) * 0.5 / a0;
    [b0, -(1.0 + cs) / a0, b0, -2.0 * cs / a0, (1.0 - al) / a0]
}

pub fn peaking(f: f64, g: f64, q: f64, sr: f64) -> Coef {
    let a = (g * 0.05756463).exp();
    let w = w0(f, sr);
    let (cs, al) = (w.cos(), w.sin() / (2.0 * q));
    let a0 = 1.0 + al / a;
    [(1.0 + al * a) / a0, -2.0 * cs / a0, (1.0 - al * a) / a0, -2.0 * cs / a0, (1.0 - al / a) / a0]
}

pub fn low_shelf(f: f64, g: f64, q: f64, sr: f64) -> Coef {
    let a = (g * 0.05756463).exp();
    let w = w0(f, sr);
    let cs = w.cos();
    let sa = 2.0 * a.sqrt() * w.sin() / (2.0 * q);
    let a0 = (a + 1.0) + (a - 1.0) * cs + sa;
    [
        a * ((a + 1.0) - (a - 1.0) * cs + sa) / a0,
        2.0 * a * ((a - 1.0) - (a + 1.0) * cs) / a0,
        a * ((a + 1.0) - (a - 1.0) * cs - sa) / a0,
        -2.0 * ((a - 1.0) + (a + 1.0) * cs) / a0,
        ((a + 1.0) + (a - 1.0) * cs - sa) / a0,
    ]
}

pub fn high_shelf(f: f64, g: f64, q: f64, sr: f64) -> Coef {
    let a = (g * 0.05756463).exp();
    let w = w0(f, sr);
    let cs = w.cos();
    let sa = 2.0 * a.sqrt() * w.sin() / (2.0 * q);
    let a0 = (a + 1.0) - (a - 1.0) * cs + sa;
    [
        a * ((a + 1.0) + (a - 1.0) * cs + sa) / a0,
        -2.0 * a * ((a - 1.0) + (a + 1.0) * cs) / a0,
        a * ((a + 1.0) + (a - 1.0) * cs - sa) / a0,
        2.0 * ((a - 1.0) - (a + 1.0) * cs) / a0,
        ((a + 1.0) - (a - 1.0) * cs - sa) / a0,
    ]
}

/// Filter kinds: 0 HPF, 1 LPF, 2 low shelf, 3 high shelf, 4 bell.
pub fn eq_coef(kind: u8, f: f64, g: f64, q: f64, sr: f64) -> Coef {
    match kind {
        0 => hp(f, q, sr),
        1 => lp(f, q, sr),
        2 => low_shelf(f, g, q, sr),
        3 => high_shelf(f, g, q, sr),
        _ => peaking(f, g, q, sr),
    }
}

/// Magnitude in dB at angle w (for the UI curve and tests).
pub fn mag_db(c: &Coef, w: f64) -> f64 {
    let (cw, sw, c2w, s2w) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
    let nr = c[0] + c[1] * cw + c[2] * c2w;
    let ni = c[1] * sw + c[2] * s2w;
    let dr = 1.0 + c[3] * cw + c[4] * c2w;
    let di = c[3] * sw + c[4] * s2w;
    4.3429448 * ((nr * nr + ni * ni + 1e-12) / (dr * dr + di * di + 1e-12)).ln()
}

#[cfg(test)]
mod tests {
    use super::*;
    const SR: f64 = 48000.0;

    #[test]
    fn butterworth_is_minus_3db_at_cutoff() {
        let c = lp(1000.0, 0.70710678, SR);
        let m = mag_db(&c, 2.0 * PI * 1000.0 / SR);
        assert!((m + 3.01).abs() < 0.05, "{m}");
        let c = hp(1000.0, 0.70710678, SR);
        let m = mag_db(&c, 2.0 * PI * 1000.0 / SR);
        assert!((m + 3.01).abs() < 0.05, "{m}");
    }

    #[test]
    fn bell_and_shelves_hit_requested_gain() {
        let w = |f: f64| 2.0 * PI * f / SR;
        assert!((mag_db(&peaking(1000.0, 6.0, 1.0, SR), w(1000.0)) - 6.0).abs() < 0.01);
        assert!((mag_db(&low_shelf(200.0, -6.0, 0.7, SR), w(20.0)) + 6.0).abs() < 0.2);
        assert!((mag_db(&high_shelf(4000.0, 6.0, 0.7, SR), w(20000.0)) - 6.0).abs() < 0.3);
    }

    #[test]
    fn identity_passes_through() {
        let mut s = [0.0; 2];
        for x in [0.5, -0.25, 1.0] {
            assert!((bq(&mut s, &IDENTITY, x) - x).abs() < 1e-20);
        }
    }
}
