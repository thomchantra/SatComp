//! EQ decode (stored slider values -> filter kind/freq/gain/Q) and the 10-filter bank
//! (2 stages x [3 bands + tilt low shelf + tilt high shelf]) with per-block coefficient ramps.

use crate::biquad::{bq, eq_coef, Coef, IDENTITY};
use crate::params::EqStageParams;

pub const BAND_DEFAULT_FREQ: [f64; 3] = [80.0, 200.0, 6000.0];

/// Icon index for a stored type (the band's default is shelf/bell middle choice, see JSFX eq_def_idx).
pub fn ui_idx(b: usize, stored: f64) -> usize {
    let def = if b == 1 { 2 } else { 1 };
    ((stored + 0.5).floor() as i64 + def).rem_euclid(3) as usize
}

/// Icon index -> filter kind (0 HPF, 1 LPF, 2 low shelf, 3 high shelf, 4 bell).
pub fn kind_of(b: usize, u: usize) -> u8 {
    match (b, u) {
        (0, 0) => 0,
        (0, 1) => 2,
        (1, 0) => 0,
        (1, 1) => 1,
        (2, 0) => 1,
        (2, 1) => 3,
        _ => 4,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Band {
    pub kind: u8,
    pub freq: f64,
    pub gain: f64,
    pub q: f64,
    /// Shelf/bell are active only when gain != 0; HPF/LPF always.
    pub on: bool,
}

pub fn decode_band(st: &EqStageParams, b: usize) -> Band {
    let p = &st.bands[b];
    let kind = kind_of(b, ui_idx(b, p.ty));
    let freq = (if p.freq < 19.5 { BAND_DEFAULT_FREQ[b] } else { p.freq }).clamp(20.0, 20000.0);
    let gain = p.gain.clamp(-18.0, 18.0);
    let q = if p.q < 0.05 { 0.5 } else { p.q.clamp(0.1, 10.0) };
    Band { kind, freq, gain, q, on: kind <= 1 || gain != 0.0 }
}

/// (tilt dB, pivot Hz)
pub fn decode_tilt(st: &EqStageParams) -> (f64, f64) {
    let t = st.tilt.clamp(-12.0, 12.0);
    let p = if st.pivot < 19.5 { 600.0 } else { st.pivot.min(20000.0) };
    (t, p)
}

pub struct EqBank {
    pub cur: [Coef; 10],
    pub tgt: [Coef; 10],
    pub delta: [Coef; 10],
    pub sl: [[f64; 2]; 10],
    pub sr: [[f64; 2]; 10],
    pub active: [bool; 10],
    pub running: [bool; 10],
}

impl Default for EqBank {
    fn default() -> Self {
        Self {
            cur: [[0.0; 5]; 10],
            tgt: [[0.0; 5]; 10],
            delta: [[0.0; 5]; 10],
            sl: [[0.0; 2]; 10],
            sr: [[0.0; 2]; 10],
            active: [false; 10],
            running: [false; 10],
        }
    }
}

impl EqBank {
    /// Set filter f's target (identity when inactive); wake a sleeping filter from its target with clean state.
    pub fn target(&mut self, f: usize, coef: Option<Coef>, on: bool) {
        self.tgt[f] = if on { coef.unwrap_or(IDENTITY) } else { IDENTITY };
        if on && !self.running[f] {
            self.cur[f] = self.tgt[f];
            self.sl[f] = [0.0; 2];
            self.sr[f] = [0.0; 2];
            self.running[f] = true;
        }
        self.active[f] = on;
    }

    /// Returns (any filter of the stage active). Mirrors eq_setup() minus the output gain.
    pub fn setup_stage(&mut self, st: usize, p: &EqStageParams, sr: f64) -> bool {
        let mut any = false;
        for b in 0..3 {
            let d = decode_band(p, b);
            let c = if d.on { Some(eq_coef(d.kind, d.freq, d.gain, d.q, sr)) } else { None };
            self.target(st * 5 + b, c, d.on);
            any |= d.on;
        }
        let (t, pv) = decode_tilt(p);
        let on = t != 0.0;
        let ls = if on { Some(eq_coef(2, pv, -t * 0.5, 0.4, sr)) } else { None };
        let hs = if on { Some(eq_coef(3, pv, t * 0.5, 0.4, sr)) } else { None };
        self.target(st * 5 + 3, ls, on);
        self.target(st * 5 + 4, hs, on);
        any || on
    }

    pub fn clear_stage(&mut self, st: usize) {
        for f in st * 5..st * 5 + 5 {
            self.sl[f] = [0.0; 2];
            self.sr[f] = [0.0; 2];
        }
    }

    /// Per block: ramp deltas so a coefficient change glides over one block; retire settled identity filters.
    pub fn prep(&mut self, block_len: usize) {
        let rn = 1.0 / block_len.max(1) as f64;
        for f in 0..10 {
            if !self.running[f] {
                continue;
            }
            let mut df = 0.0;
            for i in 0..5 {
                self.delta[f][i] = (self.tgt[f][i] - self.cur[f][i]) * rn;
                df += (self.tgt[f][i] - self.cur[f][i]).abs();
            }
            if !self.active[f] && df < 1e-9 {
                self.running[f] = false;
                self.delta[f] = [0.0; 5];
            }
        }
    }

    pub fn stage_running(&self, st: usize) -> bool {
        self.running[st * 5..st * 5 + 5].iter().any(|&r| r)
    }

    #[inline]
    pub fn process(&mut self, st: usize, xl: &mut f64, xr: &mut f64) {
        for f in st * 5..st * 5 + 5 {
            if self.running[f] {
                for i in 0..5 {
                    self.cur[f][i] += self.delta[f][i];
                }
                let c = self.cur[f];
                *xl = bq(&mut self.sl[f], &c, *xl);
                *xr = bq(&mut self.sr[f], &c, *xr);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_types_match_jsfx() {
        let st = EqStageParams::default();
        assert_eq!(decode_band(&st, 0).kind, 2); // low shelf
        assert_eq!(decode_band(&st, 1).kind, 4); // bell
        assert_eq!(decode_band(&st, 2).kind, 3); // high shelf
        // all neutral at default gain 0
        assert!(!decode_band(&st, 0).on && !decode_band(&st, 1).on && !decode_band(&st, 2).on);
    }

    #[test]
    fn zero_loaded_values_use_defaults() {
        let mut st = EqStageParams::default();
        st.bands[1].freq = 0.0;
        st.bands[1].q = 0.0;
        st.pivot = 0.0;
        assert_eq!(decode_band(&st, 1).freq, 200.0);
        assert_eq!(decode_band(&st, 1).q, 0.5);
        assert_eq!(decode_tilt(&st).1, 600.0);
    }

    #[test]
    fn hpf_counts_active_even_at_zero_gain() {
        let mut st = EqStageParams::default();
        st.bands[0].ty = 2.0; // (2 + 1) % 3 = 0 -> HPF
        let d = decode_band(&st, 0);
        assert_eq!(d.kind, 0);
        assert!(d.on);
    }
}
