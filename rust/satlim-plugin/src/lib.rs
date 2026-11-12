//! SatLim: the SatComp saturator into an oversampled clipper and a true-peak brickwall limiter. nih-plug shell around
//! `satlim_dsp::LimEngine`.

mod gui;
mod params;
#[allow(dead_code)] // used by the editor
mod presets;

use nih_plug::prelude::*;
use params::SatLimParams;
use satcomp_dsp::shapers::Curve;
use satcomp_dsp::ScopeRing;
use satlim_dsp::LimEngine;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// State shared between the audio thread and the editor (all lock-free).
pub struct Shared {
    /// per 2 ms bucket: output peak and the same with what the clipper + limiter removed added back
    pub scope: Arc<ScopeRing>,
    pub epoch: Instant,
    pub sample_rate: AtomicF32,
    // audio -> UI. Peaks accumulate with fetch_max and are consumed with swap(0).
    pub opk: [AtomicF32; 2],
    /// output true peak (sample peak while the lookahead is off), linear
    pub tpk: AtomicF32,
    pub clip: AtomicBool,
    /// limiter gain reduction, dB positive
    pub gr_pk: AtomicF32,
    /// post-saturator peak (after SAT GAIN), linear
    pub sat_pk: AtomicF32,
    /// decaying peak of the saturator input (live dot on the plot)
    pub sat_vz: AtomicF32,
    /// smoothed output mean-square (RMS readout)
    pub fms: AtomicF32,
    /// loudness change across the saturator (before SAT GAIN), dB
    pub sat_db: AtomicF32,
    /// sat level match: measuring (audio -> UI), progress seconds, finished result (set once, UI writes SAT GAIN)
    pub sat_meas: AtomicBool,
    pub sat_mt: AtomicF32,
    pub sat_result: AtomicF32,
    pub sat_result_new: AtomicBool,
    // UI -> audio
    pub delta_on: AtomicBool,
    pub sat_req: AtomicBool,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            scope: Arc::new(ScopeRing::new()),
            epoch: Instant::now(),
            sample_rate: AtomicF32::new(48000.0),
            opk: [AtomicF32::new(0.0), AtomicF32::new(0.0)],
            tpk: AtomicF32::new(0.0),
            clip: AtomicBool::new(false),
            gr_pk: AtomicF32::new(0.0),
            sat_pk: AtomicF32::new(0.0),
            sat_vz: AtomicF32::new(0.0),
            fms: AtomicF32::new(1e-9),
            sat_db: AtomicF32::new(0.0),
            sat_meas: AtomicBool::new(false),
            sat_mt: AtomicF32::new(0.0),
            sat_result: AtomicF32::new(0.0),
            sat_result_new: AtomicBool::new(false),
            delta_on: AtomicBool::new(false),
            sat_req: AtomicBool::new(false),
        }
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SatLim {
    params: Arc<SatLimParams>,
    shared: Arc<Shared>,
    engine: LimEngine,
    last_latency: u32,
    last_curve: Curve,
    last_asym: [f64; satcomp_dsp::shapers::SHAPER_COUNT],
}

impl SatLim {
    fn new_engine(&self, sr: f64) -> LimEngine {
        let mut e = LimEngine::new(sr);
        e.scope = self.shared.scope.clone();
        e
    }
}

impl Default for SatLim {
    fn default() -> Self {
        let shared = Arc::new(Shared::new());
        let mut engine = LimEngine::new(48000.0);
        engine.scope = shared.scope.clone();
        Self {
            params: Arc::new(SatLimParams::default()),
            shared,
            engine,
            last_latency: u32::MAX,
            last_curve: Curve::linear(),
            last_asym: [100.0; satcomp_dsp::shapers::SHAPER_COUNT],
        }
    }
}

impl Plugin for SatLim {
    const NAME: &'static str = "SatLim";
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
        engine.set_params(&self.params.to_dsp_rt(&mut self.last_curve, &mut self.last_asym));
        self.engine = engine;
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        let sh = &self.shared;
        self.engine.set_params(&self.params.to_dsp_rt(&mut self.last_curve, &mut self.last_asym));
        let lat = self.engine.latency();
        if lat != self.last_latency {
            self.last_latency = lat;
            context.set_latency_samples(lat);
        }
        self.engine.delta_on = sh.delta_on.load(Ordering::Relaxed);
        if sh.sat_req.swap(false, Ordering::Relaxed) {
            self.engine.request_sat_match();
        }
        if let [l, r] = buffer.as_slice() {
            self.engine.process(l, r);
        }
        let e = &mut self.engine;
        sh.opk[0].fetch_max(e.opk[0] as f32, Ordering::Relaxed);
        sh.opk[1].fetch_max(e.opk[1] as f32, Ordering::Relaxed);
        e.opk = [0.0; 2];
        sh.tpk.fetch_max(e.tpk as f32, Ordering::Relaxed);
        e.tpk = 0.0;
        if e.clip_f {
            sh.clip.store(true, Ordering::Relaxed);
            e.clip_f = false;
        }
        sh.gr_pk.fetch_max(e.gr_pk as f32, Ordering::Relaxed);
        e.gr_pk = 0.0;
        sh.sat_pk.fetch_max(e.sat_out as f32, Ordering::Relaxed);
        e.sat_out = 0.0;
        sh.sat_vz.store(e.sat_vz as f32, Ordering::Relaxed);
        sh.fms.store(e.fms as f32, Ordering::Relaxed);
        sh.sat_db.store(e.sat_db as f32, Ordering::Relaxed);
        sh.sat_meas.store(e.sat_matching(), Ordering::Relaxed);
        sh.sat_mt.store(e.sat_match_progress() as f32, Ordering::Relaxed);
        if let Some(v) = e.take_sat_result() {
            sh.sat_result.store(v as f32, Ordering::Relaxed);
            sh.sat_result_new.store(true, Ordering::Release);
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for SatLim {
    // 16 bytes, must stay stable forever once released (plug-in identity).
    const VST3_CLASS_ID: [u8; 16] = *b"thomchantraSatLm";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics, Vst3SubCategory::Distortion];
}

impl ClapPlugin for SatLim {
    const CLAP_ID: &'static str = "com.thomchantra.satlim";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Saturator into a clipper and a true-peak limiter");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Limiter, ClapFeature::Distortion, ClapFeature::Stereo];
}

nih_export_vst3!(SatLim);
nih_export_clap!(SatLim);

#[cfg(test)]
mod tests {
    use super::*;
    use satlim_dsp::LimParams;

    #[test]
    fn generated_defaults_match_engine_defaults() {
        let p = SatLimParams::default();
        // plug-in parameters are f32, the engine's are f64: compare the way the plug-in stores them
        let want = LimParams { ceiling: -0.3f32 as f64, ..LimParams::default() };
        assert_eq!(p.to_dsp(), want);
    }

    #[test]
    fn every_table_slot_resolves_and_is_unique() {
        let p = SatLimParams::default();
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
        assert_eq!(<SatLim as Vst3Plugin>::VST3_CLASS_ID.len(), 16);
        assert!(<SatLim as ClapPlugin>::CLAP_ID.starts_with("com.thomchantra."));
        // distinct from the other plug-ins' identities
        assert_ne!(&<SatLim as Vst3Plugin>::VST3_CLASS_ID, b"thomchantraSatXC");
    }
}
