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
    // PHASE SECTION (built: rotator + diffusor) and tile / morph / scenes (stored and shown by the editor, no DSP behind them yet, spec steps 3 to 5)
    /// A (0) or B (1) waveshaper scene
    pub ab: i32,
    /// phase rotator, degrees
    pub phase: f64,
    pub fx_on: bool,
    /// 0 diffusor, 1 phaser, 2 flanger, 3 ringmod (only the diffusor exists; the others act as off)
    pub fx_type: i32,
    /// 0 = before the shaper, 1 = after
    pub fx_pos: i32,
    /// the colour-fx parameters (diffusor: FREQ Hz, Q, STAGES, SPREAD) and the colour mix, 0..100 %
    pub fx_freq: f64,
    pub fx_q: f64,
    /// number of allpass stages, 1..=100 (the plug-in's STAGES percentage goes through `diffuse::stages_from_pct`)
    pub fx_stages: i32,
    /// how far the stages fan out around FREQ, 0..100 % of 8 octaves
    pub fx_spread: f64,
    /// COLOR MIX: wet / dry of the whole colour fx, 0..100 %
    pub fx_mix: f64,
    /// 0 off, 1 mirror, 2 repeat
    pub tile: i32,
    /// 0 = A>B, 1 = B>A
    pub morph_dir: i32,
    pub morph_depth: f64,
    pub morph_speed: f64,
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
            ab: 0,
            phase: 0.0,
            fx_on: false,
            fx_type: 0,
            fx_pos: 0,
            fx_freq: 1000.0,
            fx_q: 0.7,
            fx_stages: 4,
            fx_spread: 0.0, // the plug-in starts at 50 %; the bare engine stays a plain stack
            fx_mix: 100.0,
            tile: 0,
            morph_dir: 0,
            morph_depth: 0.0,
            morph_speed: 50.0,
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
