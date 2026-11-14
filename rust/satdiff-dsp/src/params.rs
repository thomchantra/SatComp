//! SatDiff parameter set (plain f64 values; the plug-in shell maps its nih-plug parameters onto this).

use satcomp_dsp::shapers::{Curve, SHAPER_COUNT};
use satcomp_dsp::SatParams;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DiffParams {
    // SAT (the SatComp saturator module, verbatim)
    pub shape: i32,
    pub curve: Curve,
    /// ASYM per shaper, 0..100 %; the active one is `asym[shape]`
    pub asym: [f64; SHAPER_COUNT],
    pub drive: f64,
    pub bias: f64,
    pub sat_hp: f64,
    pub sat_lp: f64,
    pub tone: f64,
    pub sat_mix: f64,
    /// dB after the saturator (SAT GAIN)
    pub sat_gain: f64,
    /// oversampling of the saturator: 0 x1, 1 x2, 2 x4, 3 x8
    pub os: i32,
    // OUT
    /// dB after the saturator section, before the limiter
    pub out_gain: f64,
    /// loudness match: measures the level change of the whole chain and trims it away (3 s measurement)
    pub gain_match: bool,
    /// true-peak output limiter at -0.2 dBTP (the delay is always reported; this only engages the gain)
    pub lim_on: bool,
    pub bypass: bool,
}

impl Default for DiffParams {
    fn default() -> Self {
        Self {
            shape: 0,
            curve: Curve::linear(),
            asym: [100.0; SHAPER_COUNT],
            drive: 0.0,
            bias: 0.0,
            sat_hp: 20.0,
            sat_lp: 20000.0,
            tone: 20000.0,
            sat_mix: 100.0,
            sat_gain: 0.0,
            os: 0,
            out_gain: 0.0,
            gain_match: false,
            lim_on: false, // the plug-in's own default is on; the engine default stays off so bare-engine renders are untouched
            bypass: false,
        }
    }
}

impl DiffParams {
    pub fn sat(&self) -> SatParams {
        SatParams {
            shape: self.shape,
            curve: self.curve,
            drive: self.drive,
            bias: self.bias,
            hp: self.sat_hp,
            lp: self.sat_lp,
            tone: self.tone,
            os: self.os,
            mix: self.sat_mix,
            out_db: self.sat_gain,
            asym: Some(self.asym[(self.shape.max(0) as usize).min(SHAPER_COUNT - 1)]),
            steep: false,
        }
    }
}
