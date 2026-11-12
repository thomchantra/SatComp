//! SatLim parameter set (plain f64 values; the plug-in shell maps its nih-plug parameters onto this).

use satcomp_dsp::shapers::{Curve, SHAPER_COUNT};
use satcomp_dsp::SatParams;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LimParams {
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
    /// oversampling of the saturator and the clipper: 0 x1, 1 x2, 2 x4, 3 x8
    pub os: i32,
    // LIMITER
    /// lookahead + true-peak detection on (latency) or off (zero latency, sample peak, instant attack)
    pub lookahead: bool,
    /// dBTP the output is held under (before OUT GAIN)
    pub ceiling: f64,
    /// dB: the input level that is mapped onto the ceiling, so the signal is lifted by `ceiling - threshold`
    pub threshold: f64,
    /// ms, the slow end of the program-dependent release (the fast end stays at 15 ms or below)
    pub release: f64,
    /// 0..100 %: oversampled hard clip at the ceiling, driven up to +6 dB into it. 0 = off
    pub clip: f64,
    /// dB after the limiter: free by design, the way StandardClip's output gain is
    pub out_gain: f64,
    pub bypass: bool,
}

impl Default for LimParams {
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
            lookahead: true,
            ceiling: -0.3,
            threshold: 0.0,
            release: 80.0,
            clip: 0.0,
            out_gain: 0.0,
            bypass: false,
        }
    }
}

impl LimParams {
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
