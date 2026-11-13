//! SatXComp: 4-band multiband saturator + compressor with a post EQ. nih-plug shell around `satxcomp_dsp::XEngine`.

mod gui;
mod params;
mod presets;

use nih_plug::prelude::*;
use params::SatXCompParams;
use satcomp_dsp::shapers::Curve;
use satcomp_dsp::{ScopeRing, TapRing};
use satxcomp_dsp::XEngine;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// State shared between the audio thread and the editor (all lock-free).
pub struct Shared {
    pub tap_out: Arc<TapRing>,
    pub tap_dry: Arc<TapRing>,
    /// per 2 ms bucket: output peak and the same with the gain reduction added back (waveform scope)
    pub scope: Arc<ScopeRing>,
    pub epoch: Instant,
    pub sample_rate: AtomicF32,
    // audio -> UI. Peaks accumulate with fetch_max and are consumed with swap(0).
    pub opk: [AtomicF32; 2],
    pub clip: AtomicBool,
    pub gr_pk: [AtomicF32; 4],
    /// per band: gain lift of the upward compressor (dB, >= 0), accumulated like `gr_pk`
    pub gl_pk: [AtomicF32; 4],
    /// per band: level change across the strip, dB
    pub band_diff: [AtomicF32; 4],
    /// output limiter gain reduction (dB, positive), accumulated like `gr_pk`
    pub lim_gr_pk: AtomicF32,
    /// per band: decaying peak of the saturator input
    pub sat_vz: [AtomicF32; 4],
    /// per band: post-saturator peak since the UI last took it (fetch_max / swap(0))
    pub sat_out_pk: [AtomicF32; 4],
    pub fms: AtomicF32,
    pub diff_disp: AtomicF32,
    /// level change across the EQ stage, dB
    pub eq_diff: AtomicF32,
    pub bg: AtomicF32,
    pub out_meas: AtomicBool,
    pub out_mt: AtomicF32,
    // UI -> audio
    pub match_req: AtomicBool,
    /// per band: SAT GAIN level match request (UI -> audio), live loudness change across the saturator (dB), measurement
    /// state and progress, and the finished result the UI writes to the band's SAT GAIN (audio -> UI)
    pub sat_req: [AtomicBool; 4],
    pub sat_db: [AtomicF32; 4],
    pub sat_meas: [AtomicBool; 4],
    pub sat_mt: [AtomicF32; 4],
    pub sat_result: [AtomicF32; 4],
    pub sat_result_new: [AtomicBool; 4],
    pub delta_on: AtomicBool,
    /// monitor the conditioned sidechain key instead of the output (UI -> audio)
    pub sc_solo: AtomicBool,
    /// the host provided a sidechain bus in the last block (audio -> UI)
    pub sc_bus: AtomicBool,
    /// last time (ms since epoch + 1000) a spectrum view drew; the audio thread only feeds the taps while this is fresh
    pub spec_vis_ms: AtomicU64,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            tap_out: Arc::new(TapRing::new()),
            tap_dry: Arc::new(TapRing::new()),
            scope: Arc::new(ScopeRing::new()),
            epoch: Instant::now(),
            sample_rate: AtomicF32::new(48000.0),
            opk: [AtomicF32::new(0.0), AtomicF32::new(0.0)],
            clip: AtomicBool::new(false),
            gr_pk: [AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0)],
            gl_pk: [AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0)],
            band_diff: [AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0)],
            lim_gr_pk: AtomicF32::new(0.0),
            sat_vz: [AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0)],
            sat_out_pk: [AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0)],
            fms: AtomicF32::new(1e-9),
            diff_disp: AtomicF32::new(0.0),
            eq_diff: AtomicF32::new(0.0),
            bg: AtomicF32::new(1.0),
            out_meas: AtomicBool::new(false),
            out_mt: AtomicF32::new(0.0),
            match_req: AtomicBool::new(false),
            sat_req: std::array::from_fn(|_| AtomicBool::new(false)),
            sat_db: std::array::from_fn(|_| AtomicF32::new(0.0)),
            sat_meas: std::array::from_fn(|_| AtomicBool::new(false)),
            sat_mt: std::array::from_fn(|_| AtomicF32::new(0.0)),
            sat_result: std::array::from_fn(|_| AtomicF32::new(0.0)),
            sat_result_new: std::array::from_fn(|_| AtomicBool::new(false)),
            delta_on: AtomicBool::new(false),
            sc_solo: AtomicBool::new(false),
            sc_bus: AtomicBool::new(true),
            spec_vis_ms: AtomicU64::new(0),
        }
    }
    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64 + 1000
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SatXComp {
    params: Arc<SatXCompParams>,
    shared: Arc<Shared>,
    engine: XEngine,
    last_latency: u32,
    last_curves: [Curve; 4],
    last_asym: [[f64; satxcomp_dsp::params::SHAPER_COUNT]; 4],
}

impl SatXComp {
    fn new_engine(&self, sr: f64) -> XEngine {
        let mut e = XEngine::new(sr);
        e.tap_out = self.shared.tap_out.clone();
        e.tap_dry = self.shared.tap_dry.clone();
        e.scope = self.shared.scope.clone();
        e.match_trim = self.params.match_trim.load(Ordering::Relaxed) as f64;
        e
    }
}

impl Default for SatXComp {
    fn default() -> Self {
        let shared = Arc::new(Shared::new());
        let mut engine = XEngine::new(48000.0);
        engine.tap_out = shared.tap_out.clone();
        engine.tap_dry = shared.tap_dry.clone();
        engine.scope = shared.scope.clone();
        Self { params: Arc::new(SatXCompParams::default()), shared, engine, last_latency: u32::MAX, last_curves: [Curve::linear(); 4], last_asym: [[100.0; satxcomp_dsp::params::SHAPER_COUNT]; 4] }
    }
}

impl Plugin for SatXComp {
    const NAME: &'static str = "SatXComp";
    const VENDOR: &'static str = "thomchantra";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // First layout = stereo + stereo sidechain; hosts without sidechain support fall back to the second.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            names: PortNames { aux_inputs: &["Sidechain"], ..PortNames::const_default() },
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
    ];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        gui::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, buffer_config: &BufferConfig, context: &mut impl InitContext<Self>) -> bool {
        let sr = buffer_config.sample_rate as f64;
        self.shared.sample_rate.store(sr as f32, Ordering::Relaxed);
        let mut engine = self.new_engine(sr);
        engine.set_params(&self.params.to_dsp());
        self.engine = engine;
        self.last_latency = self.engine.latency();
        context.set_latency_samples(self.last_latency);
        true
    }

    fn reset(&mut self) {
        let sr = self.engine.sample_rate();
        let mut engine = self.new_engine(sr);
        engine.set_params(&self.params.to_dsp_rt(&mut self.last_curves, &mut self.last_asym));
        self.engine = engine;
    }

    fn process(&mut self, buffer: &mut Buffer, aux: &mut AuxiliaryBuffers, context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let sh = &self.shared;
        self.engine.set_params(&self.params.to_dsp_rt(&mut self.last_curves, &mut self.last_asym));
        let lat = self.engine.latency();
        if lat != self.last_latency {
            self.last_latency = lat;
            context.set_latency_samples(lat);
        }
        for b in 0..4 {
            if sh.sat_req[b].swap(false, Ordering::Relaxed) {
                self.engine.request_sat_match(b);
            }
        }
        if sh.match_req.swap(false, Ordering::Relaxed) {
            self.engine.request_match();
        }
        self.engine.delta_on = sh.delta_on.load(Ordering::Relaxed);
        self.engine.tap_on = sh.now_ms().saturating_sub(sh.spec_vis_ms.load(Ordering::Relaxed)) < 300;
        self.engine.sc_solo = sh.sc_solo.load(Ordering::Relaxed);
        let ch = buffer.as_slice();
        let sc_in = aux.inputs.first_mut().map(|b| b.as_slice_immutable());
        let sc_pair = match sc_in {
            Some([a, b]) => Some((&a[..], &b[..])),
            _ => None,
        };
        sh.sc_bus.store(sc_pair.is_some(), Ordering::Relaxed);
        if let [l, r] = ch {
            self.engine.process_sc(l, r, sc_pair);
        }
        let e = &mut self.engine;
        sh.opk[0].fetch_max(e.opk[0] as f32, Ordering::Relaxed);
        sh.opk[1].fetch_max(e.opk[1] as f32, Ordering::Relaxed);
        e.opk = [0.0; 2];
        if e.clip_f {
            sh.clip.store(true, Ordering::Relaxed);
            e.clip_f = false;
        }
        sh.lim_gr_pk.fetch_max(e.lim_gr_pk as f32, Ordering::Relaxed);
        e.lim_gr_pk = 0.0;
        for b in 0..4 {
            sh.gr_pk[b].fetch_max(e.gr_pk[b] as f32, Ordering::Relaxed);
            e.gr_pk[b] = 0.0;
            sh.gl_pk[b].fetch_max(e.gl_pk[b] as f32, Ordering::Relaxed);
            e.gl_pk[b] = 0.0;
            sh.band_diff[b].store(e.band_diff[b] as f32, Ordering::Relaxed);
        }
        for b in 0..4 {
            sh.sat_vz[b].store(e.sat_vz[b] as f32, Ordering::Relaxed);
            sh.sat_out_pk[b].fetch_max(e.sat_out[b] as f32, Ordering::Relaxed);
            e.sat_out[b] = 0.0;
            sh.sat_db[b].store(e.sat_db[b] as f32, Ordering::Relaxed);
            sh.sat_meas[b].store(e.sat_matching(b), Ordering::Relaxed);
            sh.sat_mt[b].store(e.sat_match_progress(b) as f32, Ordering::Relaxed);
            if let Some(v) = e.take_sat_result(b) {
                sh.sat_result[b].store(v as f32, Ordering::Relaxed);
                sh.sat_result_new[b].store(true, Ordering::Release);
            }
        }
        sh.fms.store(e.fms as f32, Ordering::Relaxed);
        sh.diff_disp.store(e.diff_disp as f32, Ordering::Relaxed);
        sh.eq_diff.store(e.eq_diff as f32, Ordering::Relaxed);
        sh.bg.store(e.bg_gain() as f32, Ordering::Relaxed);
        sh.out_meas.store(e.out_matching(), Ordering::Relaxed);
        sh.out_mt.store(e.out_match_progress() as f32, Ordering::Relaxed);
        self.params.match_trim.store(e.match_trim as f32, Ordering::Relaxed);
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for SatXComp {
    // 16 bytes, must stay stable forever once released (plug-in identity).
    const VST3_CLASS_ID: [u8; 16] = *b"thomchantraSatXC";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics, Vst3SubCategory::Distortion];
}

impl ClapPlugin for SatXComp {
    const CLAP_ID: &'static str = "com.thomchantra.satxcomp";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("4-band saturator + compressor with EQ");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Compressor, ClapFeature::Distortion, ClapFeature::Stereo];
}

nih_export_vst3!(SatXComp);
nih_export_clap!(SatXComp);

#[cfg(test)]
mod tests {
    use super::*;
    use satxcomp_dsp::XParams;

    #[test]
    fn generated_defaults_match_engine_defaults() {
        let p = SatXCompParams::default();
        // plug-in parameters are f32, the engine's are f64: compare the EQ Q the way the plug-in stores it
        let mut want = XParams::default();
        for b in want.eq.bands.iter_mut() {
            b.q = b.q as f32 as f64;
        }
        assert_eq!(p.to_dsp(), want);
    }

    #[test]
    fn every_table_slot_resolves_and_is_unique() {
        let p = SatXCompParams::default();
        let mut seen = std::collections::HashSet::new();
        for &(n, id, mn, mx, df) in params::TABLE {
            assert!(n < params::SLOTS, "{id}");
            assert!(seen.insert(n), "slot {n} used twice");
            assert!(p.ptr(n).is_some(), "{id}");
            assert!(mn < mx && df >= mn && df <= mx, "{id} range");
        }
    }

    #[test]
    fn ids_are_unique_and_identity_is_thomchantra() {
        let mut ids = std::collections::HashSet::new();
        for &(_, id, ..) in params::TABLE {
            assert!(ids.insert(id), "{id}");
        }
        assert_eq!(<SatXComp as Vst3Plugin>::VST3_CLASS_ID.len(), 16);
        assert!(<SatXComp as ClapPlugin>::CLAP_ID.starts_with("com.thomchantra."));
    }
}
