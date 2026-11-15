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
    /// IN GAIN, dB: first in the chain (before PHASE and a PRE fx); the dry input meter reads before it
    pub in_gain: f64,
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
    /// A (0) or B (1) waveshaper scene: which one the editor shows and edits. A view switch only, it never changes what is heard
    /// (MORPH / MIX does: DEPTH 0 = scene A, 100 % in MIX = scene B).
    pub ab: i32,
    /// scene B's whole waveshaper module (scene A's is `shape` / `curve` / `tile` / `asym` / `drive` / `bias` / `sat_hp` / `sat_lp` /
    /// `tone`): each scene is tuned on its own, SAT MIX stays shared
    pub shape_b: i32,
    pub curve_b: Curve,
    pub tile_b: i32,
    pub asym_b: [f64; SHAPER_COUNT],
    pub drive_b: f64,
    pub bias_b: f64,
    pub sat_hp_b: f64,
    pub sat_lp_b: f64,
    pub tone_b: f64,
    /// phase rotator, degrees
    pub phase: f64,
    pub fx_on: bool,
    /// 0 diffusor, 1 phaser, 2 flanger, 3 ringmod, 4 frequency shifter
    pub fx_type: i32,
    /// 0 = before the shaper, 1 = after
    pub fx_pos: i32,
    /// the colour-fx parameters (diffusor: FREQ Hz, Q, STAGES, SPREAD) and the colour mix, 0..100 %
    pub fx_freq: f64,
    pub fx_q: f64,
    /// number of allpass stages, 1..=100 (the plug-in's STAGES percentage goes through `diffuse::stages_from_pct`)
    pub fx_stages: i32,
    /// how far the stages fan out around FREQ, 0..100 % of 8 octaves (diffusor); the stereo offset of the LFOs, 0..100 % of 180 degrees
    /// (phaser, flanger)
    pub fx_spread: f64,
    /// LFO rate, Hz (phaser, flanger)
    pub fx_rate: f64,
    /// sweep depth 0..100 % (phaser: of 4 octaves; flanger: of the centre delay) / ring modulator amount
    pub fx_depth: f64,
    /// feedback, -95..95 % (phaser, flanger)
    pub fx_fb: f64,
    /// flanger centre delay, ms
    pub fx_delay: f64,
    /// ring modulator carrier: 0 sine, 1 triangle, 2 square, 3 saw
    pub fx_shape: i32,
    /// frequency shifter: shift in Hz, negative = down
    pub fx_shift: f64,
    /// COLOR MIX: wet / dry of the whole colour fx, 0..100 %
    pub fx_mix: f64,
    /// 0 off, 1 mirror, 2 repeat
    pub tile: i32,
    /// 0 = A>B (the detector moves the blend towards B as the level rises), 1 = B>A (towards B as it falls), 2 = MIX (a static
    /// crossfade by DEPTH, no detector, SPEED unused)
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
            in_gain: 0.0,
            os: 0,
            out_gain: 0.0,
            gain_match: false,
            lim_on: false, // the plug-in's own default is on; the engine default stays off so bare-engine renders are untouched
            bypass: false,
            ab: 0,
            shape_b: 0,
            curve_b: Curve::linear(),
            tile_b: 0,
            asym_b: [100.0; SHAPER_COUNT],
            drive_b: 0.0,
            bias_b: 0.0,
            sat_hp_b: 20.0,
            sat_lp_b: 20000.0,
            tone_b: 20000.0,
            phase: 0.0,
            fx_on: false,
            fx_type: 0,
            fx_pos: 0,
            fx_freq: 1000.0,
            fx_q: 0.7,
            fx_stages: 4,
            fx_spread: 0.0, // the plug-in starts at 50 %; the bare engine stays a plain stack
            fx_rate: 0.5,
            fx_depth: 50.0,
            fx_fb: 0.0,
            fx_delay: 2.0,
            fx_shape: 0,
            fx_shift: 0.0,
            fx_mix: 100.0,
            tile: 0,
            morph_dir: 2,
            morph_depth: 0.0,
            morph_speed: 50.0,
        }
    }
}

impl DiffParams {
    /// TILE as it acts for a scene: 0 for the periodic shapers (they tile themselves) and when off, else 1 mirror / 2 repeat.
    fn tile_of(shape: i32, tile: i32) -> u8 {
        if tile != 0 && satcomp_dsp::shapers::tile_applies(shape) {
            tile.clamp(0, 2) as u8
        } else {
            0
        }
    }

    /// Scene A's effective tile.
    pub fn effective_tile(&self) -> u8 {
        Self::tile_of(self.shape, self.tile)
    }

    /// Scene A's drawn curve with the effective tile mode set (the shaper table takes the mode from it for factory shapers too).
    pub fn tiled_curve(&self) -> Curve {
        Curve { tile: self.effective_tile(), ..self.curve }
    }

    /// Scene B's effective tile and tiled curve.
    pub fn effective_tile_b(&self) -> u8 {
        Self::tile_of(self.shape_b, self.tile_b)
    }

    pub fn tiled_curve_b(&self) -> Curve {
        Curve { tile: self.effective_tile_b(), ..self.curve_b }
    }

    /// The scene `ab` (0 = A, 1 = B): shape and tiled curve, as the editor draws it.
    pub fn scene_view(&self, ab: i32) -> (i32, Curve) {
        if ab == 1 {
            (self.shape_b, self.tiled_curve_b())
        } else {
            (self.shape, self.tiled_curve())
        }
    }

    /// MORPH is on (DEPTH above 0): both scenes sound, blended.
    pub fn blending(&self) -> bool {
        self.morph_depth > 0.0
    }

    /// Whether either scene has an effective tile (its folded / wrapped edges alias). Counted whether or not the morph is on, so the
    /// forced oversampling (and with it the reported latency) does not change when DEPTH moves off 0.
    pub fn tile_active(&self) -> bool {
        self.effective_tile() != 0 || self.effective_tile_b() != 0
    }

    /// Scene A's saturator settings (the one that is heard when MORPH is off).
    pub fn sat(&self) -> SatParams {
        let (shape, curve) = self.scene_view(0);
        self.sat_for(shape, curve, self.drive, self.bias, self.sat_hp, self.sat_lp, self.tone, &self.asym)
    }

    /// Scene B's saturator settings (blended in by MORPH).
    pub fn sat_b(&self) -> SatParams {
        let (shape, curve) = self.scene_view(1);
        self.sat_for(shape, curve, self.drive_b, self.bias_b, self.sat_hp_b, self.sat_lp_b, self.tone_b, &self.asym_b)
    }

    #[allow(clippy::too_many_arguments)]
    fn sat_for(&self, shape: i32, curve: Curve, drive: f64, bias: f64, hp: f64, lp: f64, tone: f64, asym: &[f64; SHAPER_COUNT]) -> SatParams {
        SatParams {
            shape,
            curve,
            drive,
            bias,
            hp,
            lp,
            tone,
            // TILE's folded / wrapped edges alias: x4 oversampling at least (for both scenes, so their latencies match)
            os: if self.tile_active() { self.os.max(2) } else { self.os },
            mix: self.sat_mix,
            out_db: 0.0,
            asym: Some(asym[(shape.max(0) as usize).min(SHAPER_COUNT - 1)]),
            steep: false,
        }
    }
}
