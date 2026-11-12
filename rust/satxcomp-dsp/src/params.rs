//! SatXComp parameter set (plain f64 values; the plug-in shell maps its nih-plug parameters onto this).

use satcomp_dsp::shapers::Curve;

pub use satcomp_dsp::shapers::SHAPER_COUNT;

/// One band's saturator + compressor + output stage.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BandParams {
    /// band module on (off = the band passes through untouched, still part of the crossover sum)
    pub on: bool,
    /// monitor only this band (several soloed bands are heard together)
    pub solo: bool,
    /// monitor what this band's compressor removed
    pub delta: bool,
    // SAT
    pub shape: i32,
    pub curve: Curve,
    /// ASYM per shaper (0..100 %: how much of the shaper's even part is kept, see `SatParams::asym`); the active one is `asym[shape]`
    pub asym: [f64; SHAPER_COUNT],
    pub drive: f64,
    pub bias: f64,
    pub tone: f64,
    /// saturator input high-pass / low-pass, Hz (20 / 20000 = off)
    pub hp: f64,
    pub lp: f64,
    pub sat_mix: f64,
    pub sat_gain: f64,
    // COMP
    pub thresh: f64,
    pub ratio: f64,
    pub attack: f64,
    pub release: f64,
    pub speed: f64,
    pub knee: f64,
    pub knee_shape: f64,
    pub detector: f64,
    pub ff_fb: f64,
    pub gr_dist: f64,
    /// 0..100 %: compressed vs the un-compressed saturated signal (parallel compression)
    pub comp_mix: f64,
    /// dB after the compressor
    pub comp_gain: f64,
}

impl Default for BandParams {
    fn default() -> Self {
        Self {
            on: true,
            solo: false,
            delta: false,
            shape: 0,
            curve: Curve::linear(),
            asym: [100.0; SHAPER_COUNT],
            drive: 0.0,
            bias: 0.0,
            tone: 20000.0,
            hp: 20.0,
            lp: 20000.0,
            sat_mix: 100.0,
            sat_gain: 0.0,
            thresh: 0.0,
            ratio: 4.0,
            attack: 10.0,
            release: 150.0,
            speed: 50.0,
            knee: 6.0,
            knee_shape: 0.0,
            detector: 30.0,
            ff_fb: 0.0,
            gr_dist: 0.0,
            comp_mix: 100.0,
            comp_gain: 0.0,
        }
    }
}

/// Filter kinds each EQ band can take, by stored type index (0 HPF, 1 LPF, 2 low shelf, 3 high shelf, 4 bell, 5 all-pass,
/// 6 band-pass, 7 notch): bands 1-2 start from a high-pass, band 3 is shelf/shelf, bands 4-5 end in a low-pass.
pub const EQ_KINDS: [[u8; 6]; 5] = [[0, 2, 4, 5, 6, 7], [0, 2, 4, 5, 6, 7], [2, 3, 4, 5, 6, 7], [1, 3, 4, 5, 6, 7], [1, 3, 4, 5, 6, 7]];
pub const EQ_DEFAULT_TYPE: [i32; 5] = [1, 2, 2, 2, 1];
pub const EQ_DEFAULT_FREQ: [f64; 5] = [80.0, 200.0, 1000.0, 5000.0, 10000.0];
pub const EQ_DEFAULT_ON: [bool; 5] = [true, true, false, false, true];

pub fn eq_kind(band: usize, ty: i32) -> u8 {
    EQ_KINDS[band.min(4)][(ty.max(0) as usize).min(5)]
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EqBandParams {
    pub on: bool,
    /// index into `EQ_KINDS[band]`
    pub ty: i32,
    pub freq: f64,
    pub q: f64,
    pub gain: f64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EqParams {
    pub bands: [EqBandParams; 5],
    pub tilt: f64,
    pub pivot: f64,
    /// 0..100 % dry/wet of the whole EQ
    pub mix: f64,
    pub gain: f64,
    pub bypass: bool,
}

impl Default for EqParams {
    fn default() -> Self {
        let b = |i: usize| EqBandParams { on: EQ_DEFAULT_ON[i], ty: EQ_DEFAULT_TYPE[i], freq: EQ_DEFAULT_FREQ[i], q: 0.71, gain: 0.0 };
        Self { bands: [b(0), b(1), b(2), b(3), b(4)], tilt: 0.0, pivot: 600.0, mix: 100.0, gain: 0.0, bypass: false }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct XParams {
    /// crossover frequencies, Hz (each is kept above the one below)
    pub x1: f64,
    pub x2: f64,
    pub x3: f64,
    /// crossover slopes: 0 = 12, 1 = 24, 2 = 48, 3 = 96 dB/oct
    pub x1_slope: i32,
    pub x2_slope: i32,
    pub x3_slope: i32,
    /// saturator oversampling: 0 off, 1 x2, 2 x4
    pub os: i32,
    pub lookahead_ms: f64,
    /// external key for the compressors (the sidechain bus); gain / HP / LP condition it before it is split into bands
    pub sc_on: bool,
    pub sc_gain: f64,
    pub sc_hp: f64,
    pub sc_lp: f64,
    /// 0..100 %: how much of every band's detector level comes from the key instead of the band's own signal
    pub key_mix: f64,
    pub bands: [BandParams; crate::xover::NB],
    pub eq: EqParams,
    /// 0..100 % dry/wet of the whole plug-in
    pub mix: f64,
    pub out_gain: f64,
    pub gain_match: bool,
    pub bypass: bool,
    /// true-peak output limiter (-0.2 dBTP) after OUT GAIN; its lookahead latency is always reported
    pub lim_on: bool,
}

impl Default for XParams {
    fn default() -> Self {
        Self {
            x1: 150.0,
            x2: 800.0,
            x3: 3500.0,
            x1_slope: 1,
            x2_slope: 1,
            x3_slope: 1,
            os: 0,
            lookahead_ms: 0.0,
            sc_on: false,
            sc_gain: 0.0,
            sc_hp: 20.0,
            sc_lp: 20000.0,
            key_mix: 100.0,
            bands: [BandParams::default(); crate::xover::NB],
            eq: EqParams::default(),
            mix: 100.0,
            out_gain: 0.0,
            gain_match: false,
            bypass: false,
            lim_on: true,
        }
    }
}
