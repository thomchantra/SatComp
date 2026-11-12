//! Band-limits what a band's saturator added.
//!
//! The saturator's harmonics spill above and below the band it works on. Filtering the saturator's *added* content (shaped
//! minus clean) with the band's own crossover edges and slopes keeps them inside the band, while the clean signal goes
//! around the filter untouched, so the bands still sum flat and the crossover needs no second copy. The lowest band has no
//! low edge and the highest no high edge.

use crate::xover::{fill, Kind, MAX_SEC};
use satcomp_dsp::biquad::{bq, Coef, IDENTITY};

const N: usize = 2 * MAX_SEC;

/// One edge: crossover frequency (Hz) and slope step (0..3 = 12 / 24 / 48 / 96 dB/oct).
pub type Edge = Option<(f64, usize)>;

pub struct Confine {
    cur: [Coef; N],
    tgt: [Coef; N],
    delta: [Coef; N],
    st: [[[f64; 2]; 2]; N],
    ramping: bool,
}

impl Default for Confine {
    fn default() -> Self {
        Self::new()
    }
}

impl Confine {
    pub fn new() -> Self {
        Self { cur: [IDENTITY; N], tgt: [IDENTITY; N], delta: [[0.0; 5]; N], st: [[[0.0; 2]; 2]; N], ramping: false }
    }

    /// `lo` = the band's low edge (high-pass), `hi` = its high edge (low-pass). Glides unless `snap`.
    pub fn set(&mut self, lo: Edge, hi: Edge, sr: f64, snap: bool) {
        match hi {
            Some((f, s)) => fill(&mut self.tgt[..MAX_SEC], Kind::Lp, f, s, 2, sr),
            None => self.tgt[..MAX_SEC].iter_mut().for_each(|c| *c = IDENTITY),
        }
        match lo {
            Some((f, s)) => fill(&mut self.tgt[MAX_SEC..], Kind::Hp, f, s, 2, sr),
            None => self.tgt[MAX_SEC..].iter_mut().for_each(|c| *c = IDENTITY),
        }
        if snap {
            self.cur = self.tgt;
            self.delta = [[0.0; 5]; N];
            self.ramping = false;
        }
    }

    /// Per host block: ramp the coefficients towards the targets over `n` samples.
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
    pub fn tick(&mut self, x: [f64; 2]) -> [f64; 2] {
        if self.ramping {
            for f in 0..N {
                for i in 0..5 {
                    self.cur[f][i] += self.delta[f][i];
                }
            }
        }
        let mut y = x;
        for c in 0..2 {
            for f in 0..N {
                y[c] = bq(&mut self.st[f][c], &self.cur[f], y[c]);
            }
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_edges_is_a_pass_through_and_edges_cut_outside_the_band() {
        let sr = 48000.0;
        let mag = |c: &mut Confine, f: f64| {
            let mut peak = 0.0f64;
            for i in 0..48000 {
                let v = (2.0 * std::f64::consts::PI * f * i as f64 / sr).sin();
                let y = c.tick([v, v]);
                if i > 24000 {
                    peak = peak.max(y[0].abs());
                }
            }
            peak
        };
        let mut c = Confine::new();
        c.set(None, None, sr, true);
        assert!((mag(&mut c, 1000.0) - 1.0).abs() < 1e-9);
        let mut c = Confine::new();
        c.set(Some((300.0, 2)), Some((3000.0, 2)), sr, true);
        let (mid, below, above) = (mag(&mut c, 1000.0), mag(&mut c, 40.0), mag(&mut c, 15000.0));
        assert!(mid > 0.95, "{mid}");
        assert!(below < 0.001 && above < 0.001, "{below} {above}");
    }
}
