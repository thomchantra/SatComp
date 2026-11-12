//! Parameter set, mirroring the JSFX sliders (see docs/PARAMETERS.md). Slider numbers in comments.

use crate::compressor::CompParams;
use crate::saturator::SatParams;
use crate::shapers::Curve;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EqBandParams {
    /// Stored type, relative to the band's default (0 = default). 0..2. (sliders 29, 33, 37, ...)
    pub ty: f64,
    pub freq: f64,
    pub gain: f64,
    pub q: f64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EqStageParams {
    pub bands: [EqBandParams; 3],
    pub tilt: f64,
    pub pivot: f64,
    pub out_gain: f64,
}

impl Default for EqStageParams {
    fn default() -> Self {
        let b = |f| EqBandParams { ty: 0.0, freq: f, gain: 0.0, q: 0.5 };
        Self { bands: [b(80.0), b(200.0), b(6000.0)], tilt: 0.0, pivot: 600.0, out_gain: 0.0 }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Params {
    // SAT
    pub shape: i32,       // 1   0..14 (14 = custom curve, Rust only)
    pub curve: Curve,     // user-drawn transfer curve, used when shape == 14
    pub drive: f64,       // 2   0..100 %
    pub bias: f64,        // 3   -100..100 %
    pub sat_hp: f64,      // 4   20..2000 Hz (<=20.5 = off)
    pub sat_lp: f64,      // 5   200..20000 Hz (>=19900 = off)
    pub tone: f64,        // 6   1000..20000 Hz (>=19900 = off)
    pub os: i32,          // 7   0 off, 1 x2, 2 x4
    // COMP
    pub thresh: f64,      // 8   dB
    pub ratio: f64,       // 9   1..20 (>=19.9 = INF)
    pub attack: f64,      // 10  ms
    pub release: f64,     // 11  ms (>=1990 = AUTO)
    pub speed: f64,       // 12  0..100 %
    pub knee: f64,        // 13  dB
    pub knee_shape: f64,  // 14  0..100 %
    pub detector: f64,    // 15  0..100 % (peak > rms)
    pub det_hp: f64,      // 16  0..400 Hz (<=15 = off)
    pub ff_fb: f64,       // 18  0..100 %
    pub gr_dist: f64,     // 19  0..100 %
    // OUT
    pub makeup: f64,      // 20  dB
    pub mix: f64,         // 21  0..100 %
    pub out_gain: f64,    // 22  dB
    pub gain_match: bool, // 23  (Auto retired: Manual / Match)
    pub sat_mix: f64,     // 25  0..100 %
    pub bypass: bool,     // 26
    pub sat_out: f64,     // 27  dB
    // EQ stages (29..58)
    pub eq: [EqStageParams; 2],
    pub lookahead_ms: f64,      // 59
    pub eq_pre_bypass: bool,    // 60
    pub eq_post_bypass: bool,   // 61
    pub eq_module_bypass: bool, // 62
    // SIDECHAIN (detector key from the aux input)
    pub sc_on: bool,   // 63
    pub sc_gain: f64,  // 64  -24..24 dB
    pub sc_hp: f64,    // 65  20..2000 Hz (<=20.5 = off)
    pub sc_lp: f64,    // 66  200..20000 Hz (>=19900 = off)
    pub sc_mix: f64,   // 67  0..100 % key mix (0 = main detector only, 100 = key only)
    // OUTPUT LIMITER (true-peak brickwall at -0.2 dBTP; the delay is always reported, this only engages the gain)
    pub lim_on: bool,  // 68
}

impl Params {
    /// The saturator block's slice of the parameter set.
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
            out_db: self.sat_out,
            asym: None,
        }
    }

    /// The compressor block's slice of the parameter set.
    pub fn comp(&self) -> CompParams {
        CompParams {
            thresh: self.thresh,
            ratio: self.ratio,
            attack: self.attack,
            release: self.release,
            speed: self.speed,
            knee: self.knee,
            knee_shape: self.knee_shape,
            detector: self.detector,
            det_hp: self.det_hp,
            ff_fb: self.ff_fb,
            gr_dist: self.gr_dist,
            key_mix: self.sc_mix,
        }
    }
}

impl Default for Params {
    fn default() -> Self {
        Self {
            shape: 0,
            curve: Curve::linear(),
            drive: 0.0,
            bias: 0.0,
            sat_hp: 20.0,
            sat_lp: 20000.0,
            tone: 20000.0,
            os: 0,
            thresh: 0.0,
            ratio: 4.0,
            attack: 10.0,
            release: 150.0,
            speed: 50.0,
            knee: 6.0,
            knee_shape: 0.0,
            detector: 30.0,
            det_hp: 0.0,
            ff_fb: 0.0,
            gr_dist: 0.0,
            makeup: 0.0,
            mix: 100.0,
            out_gain: 0.0,
            gain_match: false,
            sat_mix: 100.0,
            bypass: false,
            sat_out: 0.0,
            eq: [EqStageParams::default(); 2],
            lookahead_ms: 0.0,
            eq_pre_bypass: false,
            eq_post_bypass: false,
            eq_module_bypass: false,
            sc_on: false,
            sc_gain: 0.0,
            sc_hp: 20.0,
            sc_lp: 20000.0,
            sc_mix: 100.0,
            lim_on: false, // the plug-in's own default is on; the engine default stays off so bare-engine renders are untouched
        }
    }
}
