//! Phase rotator: a constant phase shift across the whole spectrum, `y = x cos(t) - H(x) sin(t)`, built from a Hilbert-style
//! pair of allpass chains (Olli Niemitalo's 2 x 4 first-order sections in z^-2, the first chain delayed by one sample). Pure IIR: zero latency, unity gain at every
//! frequency. The two chains' own phases are not zero (only their difference is 90 degrees), so the output is "the input,
//! dispersed by the fixed chain phase, then rotated by `t`": it is engaged with a short crossfade and is an exact bypass while
//! PHASE is 0 and settled.

use satcomp_dsp::level::one_pole;
use std::f64::consts::PI;

/// a-coefficients of the in-phase and quadrature chains (a^2 is the section's allpass coefficient)
const A_I: [f64; 4] = [0.6923878, 0.9360654322959, 0.9882295226860, 0.9987488452737];
const A_Q: [f64; 4] = [0.4021921162426, 0.8561710882420, 0.9722909545651, 0.9952884791278];

/// One chain of four sections `y[n] = a^2 (x[n] + y[n-2]) - x[n-2]`.
#[derive(Clone, Copy, Default)]
struct Chain {
    x: [[f64; 2]; 4],
    y: [[f64; 2]; 4],
}

impl Chain {
    #[inline]
    fn tick(&mut self, a: &[f64; 4], mut v: f64) -> f64 {
        for k in 0..4 {
            let a2 = a[k] * a[k];
            let y = a2 * (v + self.y[k][1]) - self.x[k][1];
            self.x[k][1] = self.x[k][0];
            self.x[k][0] = v;
            self.y[k][1] = self.y[k][0];
            self.y[k][0] = y;
            v = y;
        }
        v
    }
}

#[derive(Clone, Copy, Default)]
struct Chan {
    i: Chain,
    q: Chain,
    /// the in-phase chain's output is one sample late (that is what puts the other chain 90 degrees ahead of it)
    id: f64,
}

pub struct Rotator {
    ch: [Chan; 2],
    deg_t: f64,
    cos_t: f64,
    sin_t: f64,
    cos: f64,
    sin: f64,
    /// 0 = bypass, 1 = rotated signal (crossfade)
    eng: f64,
    eng_t: f64,
    sm_c: f64,
    en_c: f64,
    clean: bool,
}

impl Rotator {
    pub fn new(sr: f64) -> Self {
        let mut r = Self {
            ch: [Chan::default(); 2],
            deg_t: 0.0,
            cos_t: 1.0,
            sin_t: 0.0,
            cos: 1.0,
            sin: 0.0,
            eng: 0.0,
            eng_t: 0.0,
            sm_c: 0.0,
            en_c: 0.0,
            clean: true,
        };
        r.set_sample_rate(sr);
        r
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sm_c = one_pole(sr, 0.01);
        self.en_c = one_pole(sr, 0.02);
    }

    /// Phase in degrees. `snap` jumps to it (a fresh engine starts at the host's setting).
    pub fn set(&mut self, deg: f64, snap: bool) {
        self.deg_t = deg;
        let t = deg * PI / 180.0;
        self.cos_t = t.cos();
        self.sin_t = t.sin();
        self.eng_t = if deg.abs() < 1e-9 { 0.0 } else { 1.0 };
        if snap {
            self.cos = self.cos_t;
            self.sin = self.sin_t;
            self.eng = self.eng_t;
        }
    }

    pub fn degrees(&self) -> f64 {
        self.deg_t
    }

    /// The raw in-phase / quadrature pair of one channel (the quadrature leads the in-phase by 90 degrees): for tests.
    pub fn quad(&mut self, c: usize, x: f64) -> (f64, f64) {
        let ch = &mut self.ch[c];
        let i_raw = ch.i.tick(&A_I, x);
        let i = ch.id;
        ch.id = i_raw;
        let q = ch.q.tick(&A_Q, x);
        (i, q)
    }

    #[inline]
    pub fn tick(&mut self, x: [f64; 2]) -> [f64; 2] {
        if self.eng < 1e-9 && self.eng_t == 0.0 {
            // idle: exact bypass, and the chains are cleared so a later engage does not start from stale state
            if !self.clean {
                self.ch = [Chan::default(); 2];
                self.clean = true;
            }
            self.eng = 0.0;
            self.cos = self.cos_t;
            self.sin = self.sin_t;
            return x;
        }
        self.clean = false;
        self.cos += (self.cos_t - self.cos) * self.sm_c;
        self.sin += (self.sin_t - self.sin) * self.sm_c;
        self.eng += (self.eng_t - self.eng) * self.en_c;
        let mut out = x;
        for c in 0..2 {
            let (i, q) = self.quad(c, x[c]);
            // output phase = chain phase + t (positive t advances)
            let y = i * self.cos + q * self.sin;
            out[c] = x[c] + (y - x[c]) * self.eng;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;
    const FREQS: [f64; 7] = [60.0, 150.0, 500.0, 1500.0, 4000.0, 9000.0, 15000.0];

    /// amplitude and phase (radians) of the tone `f` in `y[n0..]` by correlation with sin / cos
    fn tone(y: &[f64], f: f64, n0: usize) -> (f64, f64) {
        let (mut s, mut c) = (0.0, 0.0);
        let n = y.len() - n0;
        for (i, v) in y[n0..].iter().enumerate() {
            let t = 2.0 * PI * f * (n0 + i) as f64 / SR;
            s += v * t.sin();
            c += v * t.cos();
        }
        (2.0 * (s * s + c * c).sqrt() / n as f64, c.atan2(s))
    }

    fn sine(f: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| 0.5 * (2.0 * PI * f * i as f64 / SR).sin()).collect()
    }

    /// whole periods only, so the correlation is exact: round the length to a multiple of the tone's period
    fn len_for(f: f64) -> (usize, usize) {
        let per = (SR / f).round().max(1.0);
        let n0 = (per * (0.5 * SR / per).ceil()) as usize; // ~0.5 s settle
        (n0, n0 + (per * (0.25 * SR / per).ceil()) as usize)
    }

    fn wrap(d: f64) -> f64 {
        let mut d = d;
        while d > PI {
            d -= 2.0 * PI;
        }
        while d < -PI {
            d += 2.0 * PI;
        }
        d
    }

    #[test]
    fn the_two_chains_are_in_quadrature_across_the_band() {
        for &f in &FREQS {
            let (n0, n) = len_for(f);
            let x = sine(f, n);
            let mut r = Rotator::new(SR);
            let (mut yi, mut yq) = (Vec::new(), Vec::new());
            for &v in &x {
                let (i, q) = r.quad(0, v);
                yi.push(i);
                yq.push(q);
            }
            let ((ai, pi), (aq, pq)) = (tone(&yi, f, n0), tone(&yq, f, n0));
            let diff = wrap(pq - pi).to_degrees();
            assert!((ai / 0.5 - 1.0).abs() < 0.01 && (aq / 0.5 - 1.0).abs() < 0.01, "{f} Hz: allpass gain {ai} {aq}");
            assert!((diff.abs() - 90.0).abs() < 2.5, "{f} Hz: quadrature error, I/Q differ by {diff} degrees");
        }
    }

    #[test]
    fn positive_phase_advances_by_the_set_angle_at_unity_gain() {
        for &f in &FREQS {
            let (n0, n) = len_for(f);
            let x = sine(f, n);
            let run = |deg: f64| {
                let mut r = Rotator::new(SR);
                r.set(deg, true);
                let mut y = Vec::new();
                for &v in &x {
                    y.push(r.tick([v, v])[0]);
                }
                tone(&y, f, n0)
            };
            let (_, p0) = (0, {
                // reference: the in-phase chain alone (PHASE 0 engaged is not the same as bypass, see the module docs)
                let mut r = Rotator::new(SR);
                let mut y = Vec::new();
                for &v in &x {
                    y.push(r.quad(0, v).0);
                }
                tone(&y, f, n0).1
            });
            for deg in [30.0, 90.0, 150.0, -60.0, 180.0] {
                let (a, p) = run(deg);
                let got = wrap(p - p0).to_degrees();
                let want = if deg == 180.0 { 180.0 } else { deg };
                let err = if deg == 180.0 { (got.abs() - 180.0).abs() } else { (got - want).abs() };
                assert!((a / 0.5 - 1.0).abs() < 0.02, "{f} Hz {deg} deg: gain {a}");
                assert!(err < 3.5, "{f} Hz: asked {deg} deg, got {got}");
            }
        }
    }

    #[test]
    fn zero_degrees_is_an_exact_bypass_and_leaves_no_state_behind() {
        let mut r = Rotator::new(SR);
        r.set(0.0, true);
        for i in 0..2000 {
            let x = [(i as f64 * 0.37).sin() * 0.4, (i as f64 * 0.11).cos() * 0.3];
            assert_eq!(r.tick(x), x);
        }
        // engage, run, release: back to exact bypass once the fade is done
        r.set(70.0, false);
        for i in 0..4000 {
            r.tick([(i as f64 * 0.2).sin(); 2]);
        }
        r.set(0.0, false);
        for i in 0..40000 {
            r.tick([(i as f64 * 0.2).sin(); 2]); // the 20 ms fade-out takes ~0.4 s to fall under 1e-9
        }
        for i in 0..100 {
            let x = [(i as f64 * 0.31).sin() * 0.4, 0.2];
            assert_eq!(r.tick(x), x);
        }
    }
}
