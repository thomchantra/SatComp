mod gui;
mod params;
mod presets;

use nih_plug::prelude::*;
use params::SatCompParams;
use satcomp_dsp::{Engine, TapRing};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// State shared between the audio thread and the editor (all lock-free).
pub struct Shared {
    pub tap: Arc<TapRing>,
    pub epoch: Instant,
    pub sample_rate: AtomicF32,
    // audio -> UI. Peaks accumulate with fetch_max and are consumed with swap(0).
    pub opk: [AtomicF32; 2],
    pub clip: AtomicBool,
    pub gr_pk: AtomicF32,
    pub fms: AtomicF32,
    pub vz_lv: AtomicF32,
    pub sat_vz: AtomicF32,
    pub diff_disp: AtomicF32,
    pub sat_db: AtomicF32,
    pub bg: AtomicF32,
    pub out_meas: AtomicBool,
    pub out_mt: AtomicF32,
    pub sat_meas: AtomicBool,
    pub sat_mt: AtomicF32,
    pub sat_result: AtomicF32,
    pub sat_result_new: AtomicBool,
    // UI -> audio
    pub match_req: AtomicBool,
    pub sat_req: AtomicBool,
    pub delta_on: AtomicBool,
    pub eq_vis_ms: AtomicU64,
    pub eq_vs: AtomicUsize,
    /// UI -> audio: solo the sidechain signal (monitor only, never stored).
    pub sc_solo: AtomicBool,
    /// audio -> UI: the host gave us a sidechain bus (so the SC switch can take effect) and main level at the comp input.
    pub sc_bus: AtomicBool,
    pub mn_lv: AtomicF32,
}

impl Shared {
    fn new() -> Self {
        Self {
            tap: Arc::new(TapRing::new()),
            epoch: Instant::now(),
            sample_rate: AtomicF32::new(48000.0),
            opk: [AtomicF32::new(0.0), AtomicF32::new(0.0)],
            clip: AtomicBool::new(false),
            gr_pk: AtomicF32::new(0.0),
            fms: AtomicF32::new(1e-9),
            vz_lv: AtomicF32::new(0.0),
            sat_vz: AtomicF32::new(0.0),
            diff_disp: AtomicF32::new(0.0),
            sat_db: AtomicF32::new(0.0),
            bg: AtomicF32::new(1.0),
            out_meas: AtomicBool::new(false),
            out_mt: AtomicF32::new(0.0),
            sat_meas: AtomicBool::new(false),
            sat_mt: AtomicF32::new(0.0),
            sat_result: AtomicF32::new(0.0),
            sat_result_new: AtomicBool::new(false),
            match_req: AtomicBool::new(false),
            sat_req: AtomicBool::new(false),
            delta_on: AtomicBool::new(false),
            eq_vis_ms: AtomicU64::new(0),
            eq_vs: AtomicUsize::new(0),
            sc_solo: AtomicBool::new(false),
            sc_bus: AtomicBool::new(true),
            mn_lv: AtomicF32::new(0.0),
        }
    }
    pub fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64 + 1000
    }
}

pub struct SatComp {
    params: Arc<SatCompParams>,
    shared: Arc<Shared>,
    engine: Engine,
    last_latency: u32,
}

impl Default for SatComp {
    fn default() -> Self {
        let shared = Arc::new(Shared::new());
        let mut engine = Engine::new(48000.0);
        engine.tap = shared.tap.clone();
        Self { params: Arc::new(SatCompParams::default()), shared, engine, last_latency: u32::MAX }
    }
}

impl Plugin for SatComp {
    const NAME: &'static str = "SatComp";
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

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        let sr = buffer_config.sample_rate as f64;
        self.shared.sample_rate.store(sr as f32, Ordering::Relaxed);
        let mut engine = Engine::new(sr);
        engine.tap = self.shared.tap.clone();
        engine.match_trim = self.params.match_trim.load(Ordering::Relaxed) as f64;
        engine.set_params(&self.params.to_dsp());
        self.engine = engine;
        self.last_latency = self.engine.latency();
        context.set_latency_samples(self.last_latency);
        true
    }

    fn reset(&mut self) {
        let sr = self.engine.sample_rate();
        let mut engine = Engine::new(sr);
        engine.tap = self.shared.tap.clone();
        engine.match_trim = self.params.match_trim.load(Ordering::Relaxed) as f64;
        engine.set_params(&self.params.to_dsp());
        self.engine = engine;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let sh = &self.shared;
        self.engine.set_params(&self.params.to_dsp());
        let lat = self.engine.latency();
        if lat != self.last_latency {
            self.last_latency = lat;
            context.set_latency_samples(lat);
        }
        if sh.match_req.swap(false, Ordering::Relaxed) {
            self.engine.request_match();
        }
        if sh.sat_req.swap(false, Ordering::Relaxed) {
            self.engine.request_sat_match();
        }
        self.engine.delta_on = sh.delta_on.load(Ordering::Relaxed);
        self.engine.eq_tap_on = sh.now_ms().saturating_sub(sh.eq_vis_ms.load(Ordering::Relaxed)) < 300;
        self.engine.eq_vs = sh.eq_vs.load(Ordering::Relaxed);

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
        sh.gr_pk.fetch_max(e.gr_pk as f32, Ordering::Relaxed);
        e.gr_pk = 0.0;
        sh.fms.store(e.fms as f32, Ordering::Relaxed);
        sh.vz_lv.store(e.vz_lv as f32, Ordering::Relaxed);
        sh.mn_lv.store(e.mn_lv as f32, Ordering::Relaxed);
        sh.sat_vz.store(e.sat_vz as f32, Ordering::Relaxed);
        sh.diff_disp.store(e.diff_disp as f32, Ordering::Relaxed);
        sh.sat_db.store(e.sat_db as f32, Ordering::Relaxed);
        sh.bg.store(e.bg_gain() as f32, Ordering::Relaxed);
        sh.out_meas.store(e.out_matching(), Ordering::Relaxed);
        sh.out_mt.store(e.out_match_progress() as f32, Ordering::Relaxed);
        sh.sat_meas.store(e.sat_matching(), Ordering::Relaxed);
        sh.sat_mt.store(e.sat_match_progress() as f32, Ordering::Relaxed);
        self.params.match_trim.store(e.match_trim as f32, Ordering::Relaxed);
        if let Some(v) = e.take_sat_result() {
            sh.sat_result.store(v as f32, Ordering::Relaxed);
            sh.sat_result_new.store(true, Ordering::Release);
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for SatComp {
    // 16 bytes, must stay stable forever once released (plug-in identity).
    const VST3_CLASS_ID: [u8; 16] = *b"TrtSatCompPlugin";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics, Vst3SubCategory::Distortion];
}

impl ClapPlugin for SatComp {
    const CLAP_ID: &'static str = "com.trirat.satcomp";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Saturator + compressor + EQ");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::AudioEffect, ClapFeature::Compressor, ClapFeature::Distortion, ClapFeature::Stereo];
}

nih_export_vst3!(SatComp);
nih_export_clap!(SatComp);
