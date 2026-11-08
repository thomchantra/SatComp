//! Pure DSP for SatComp: a line-by-line port of `SatComp.jsfx` (v0.3).
//! No plugin or GUI dependencies. All maths in f64 so it can be null-tested
//! against JSFX renders (EEL2 is f64 as well).

pub mod biquad;
pub mod engine;
pub mod eq;
pub mod halfband;
pub mod params;
pub mod shapers;

pub use engine::{Engine, GainComputer, TapRing};
pub use params::{EqBandParams, EqStageParams, Params};

/// dB (amplitude) to linear gain, same constant as the JSFX (`exp(db*0.11512925)`).
pub fn db_to_gain(db: f64) -> f64 {
    (db * 0.11512925).exp()
}

/// Tiny offset the JSFX adds to recursive stages to avoid denormals.
pub const DENORM: f64 = 1e-30;
