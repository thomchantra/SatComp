//! SatXComp: 3-band multiband saturator + compressor with a post EQ. nih-plug shell around `satxcomp_dsp::XEngine`.

mod params;

use nih_plug::prelude::*;
use params::SatXCompParams;
use satcomp_dsp::shapers::Curve;
use satcomp_dsp::TapRing;
use satxcomp_dsp::XEngine;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// State shared between the audio thread and the editor (all lock-free).
pub struct Shared {
    pub tap_out: Arc<TapRing>,
    pub tap_dry: Arc<TapRing>,
    pub epoch: Instant,
    pub sample_rate: AtomicF32,
    // audio -> UI. Peaks accumulate with fetch_max and are consumed with swap(0).
    pub opk: [AtomicF32; 2],
    pub clip: AtomicBool,
    pub gr_pk: [AtomicF32; 3],
    pub fms: AtomicF32,
    pub diff_disp: AtomicF32,
    pub bg: AtomicF32,
    pub out_meas: AtomicBool,
    pub out_mt: AtomicF32,
    // UI -> audio
    pub match_req: AtomicBool,
    pub delta_on: AtomicBool,
    /// last time (ms since epoch + 1000) a spectrum view drew; the audio thread only feeds the taps while this is fresh
    pub spec_vis_ms: AtomicU64,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            tap_out: Arc::new(TapRing::new()),
            tap_dry: Arc::new(TapRing::new()),
            epoch: Instant::now(),
            sample_rate: AtomicF32::new(48000.0),
            opk: [AtomicF32::new(0.0), AtomicF32::new(0.0)],
            clip: AtomicBool::new(false),
            gr_pk: [AtomicF32::new(0.0), AtomicF32::new(0.0), AtomicF32::new(0.0)],
            fms: AtomicF32::new(1e-9),
            diff_disp: AtomicF32::new(0.0),
            bg: AtomicF32::new(1.0),
            out_meas: AtomicBool::new(false),
            out_mt: AtomicF32::new(0.0),
            match_req: AtomicBool::new(false),
            delta_on: AtomicBool::new(false),
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
    last_curves: [Curve; 3],
}

impl SatXComp {
    fn new_engine(&self, sr: f64) -> XEngine {
        let mut e = XEngine::new(sr);
        e.tap_out = self.shared.tap_out.clone();
        e.tap_dry = self.shared.tap_dry.clone();
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
        Self { params: Arc::new(SatXCompParams::default()), shared, engine, last_latency: u32::MAX, last_curves: [Curve::linear(); 3] }
    }
}

impl Plugin for SatXComp {
    const NAME: &'static str = "SatXComp";
    const VENDOR: &'static str = "thomchantra";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
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
        engine.set_params(&self.params.to_dsp_rt(&mut self.last_curves));
        self.engine = engine;
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let sh = &self.shared;
        self.engine.set_params(&self.params.to_dsp_rt(&mut self.last_curves));
        let lat = self.engine.latency();
        if lat != self.last_latency {
            self.last_latency = lat;
            context.set_latency_samples(lat);
        }
        if sh.match_req.swap(false, Ordering::Relaxed) {
            self.engine.request_match();
        }
        self.engine.delta_on = sh.delta_on.load(Ordering::Relaxed);
        self.engine.tap_on = sh.now_ms().saturating_sub(sh.spec_vis_ms.load(Ordering::Relaxed)) < 300;
        if let [l, r] = buffer.as_slice() {
            self.engine.process(l, r);
        }
        let e = &mut self.engine;
        sh.opk[0].fetch_max(e.opk[0] as f32, Ordering::Relaxed);
        sh.opk[1].fetch_max(e.opk[1] as f32, Ordering::Relaxed);
        e.opk = [0.0; 2];
        if e.clip_f {
            sh.clip.store(true, Ordering::Relaxed);
            e.clip_f = false;
        }
        for b in 0..3 {
            sh.gr_pk[b].fetch_max(e.gr_pk[b] as f32, Ordering::Relaxed);
            e.gr_pk[b] = 0.0;
        }
        sh.fms.store(e.fms as f32, Ordering::Relaxed);
        sh.diff_disp.store(e.diff_disp as f32, Ordering::Relaxed);
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
    const CLAP_DESCRIPTION: Option<&'static str> = Some("3-band saturator + compressor with EQ");
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
