//! The SatComp signal path: sat (HP/LP split, shaper + optional oversampling, DC block, tone, sat mix, sat out)
//! -> EQ pre -> lookahead -> compressor -> makeup/mix -> EQ post -> out gain -> equal-loudness bypass -> delta.
//! Port of the JSFX `@block` / `@sample` sections. Names follow the JSFX variables where practical.

use crate::biquad::{self, bq, Coef};
use crate::eq::EqBank;
use crate::halfband::{Oversampler, Taps};
use crate::params::Params;
use crate::shapers::ShaperLut;
use crate::DENORM;
use std::f64::consts::PI;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

/// Lock-free mono sample ring shared with the UI (spectrum tap). f32 bits in atomics; tearing is harmless for a display.
pub struct TapRing {
    pub buf: Vec<AtomicU32>,
    pub w: AtomicUsize,
}

impl TapRing {
    pub const LEN: usize = 8192;
    pub fn new() -> Self {
        Self { buf: (0..Self::LEN).map(|_| AtomicU32::new(0)).collect(), w: AtomicUsize::new(0) }
    }
    #[inline]
    fn push(&self, v: f32) {
        let w = self.w.load(Ordering::Relaxed);
        self.buf[w & (Self::LEN - 1)].store(v.to_bits(), Ordering::Relaxed);
        self.w.store(w.wrapping_add(1), Ordering::Release);
    }
    pub fn get(&self, i: usize) -> f32 {
        f32::from_bits(self.buf[i & (Self::LEN - 1)].load(Ordering::Relaxed))
    }
}

impl Default for TapRing {
    fn default() -> Self {
        Self::new()
    }
}

/// Lock-free ring of scope buckets for the UI's waveform view: per 2 ms bucket the output peak and the same peak with the
/// compressor's gain reduction added back (so `pre - out` is what the compressor removed). f32 bits in atomics.
pub struct ScopeRing {
    out: Vec<AtomicU32>,
    pre: Vec<AtomicU32>,
    w: AtomicUsize,
}

impl ScopeRing {
    pub const LEN: usize = 8192;
    /// seconds per bucket
    pub const BUCKET_S: f64 = 0.002;
    pub fn new() -> Self {
        Self {
            out: (0..Self::LEN).map(|_| AtomicU32::new(0)).collect(),
            pre: (0..Self::LEN).map(|_| AtomicU32::new(0)).collect(),
            w: AtomicUsize::new(0),
        }
    }
    fn push(&self, o: f32, p: f32) {
        let w = self.w.load(Ordering::Relaxed);
        self.out[w & (Self::LEN - 1)].store(o.to_bits(), Ordering::Relaxed);
        self.pre[w & (Self::LEN - 1)].store(p.to_bits(), Ordering::Relaxed);
        self.w.store(w.wrapping_add(1), Ordering::Release);
    }
    /// number of buckets written so far (bucket `head - 1` is the newest)
    pub fn head(&self) -> usize {
        self.w.load(Ordering::Acquire)
    }
    pub fn get(&self, i: usize) -> (f32, f32) {
        let k = i & (Self::LEN - 1);
        (f32::from_bits(self.out[k].load(Ordering::Relaxed)), f32::from_bits(self.pre[k].load(Ordering::Relaxed)))
    }
}

impl Default for ScopeRing {
    fn default() -> Self {
        Self::new()
    }
}

const LN10_20: f64 = 0.11512925; // dB -> ln(gain)
const DB_POW: f64 = 4.3429448; // 10/ln(10)
const DB_AMP: f64 = 8.6858896; // 20/ln(10)

/// Static gain computer: returns GR in dB (>= 0) for a level in dB. C1 knee, quad -> smoothstep -> smootherstep.
#[derive(Clone, Copy, Debug)]
pub struct GainComputer {
    pub thr: f64,
    pub knw: f64,
    pub cslope: f64,
    pub kc0: f64,
    pub kc1: f64,
    pub kc2: f64,
}

impl Default for GainComputer {
    fn default() -> Self {
        Self { thr: -18.0, knw: 6.0, cslope: 0.75, kc0: 1.0, kc1: 0.0, kc2: 0.0 }
    }
}

impl GainComputer {
    pub fn from_params(p: &Params) -> Self {
        let inv_r = if p.ratio >= 19.9 { 0.0 } else { 1.0 / p.ratio };
        let sh2 = p.knee_shape * 0.02;
        let (kc0, kc1, kc2) = if sh2 <= 1.0 { (1.0 - sh2, sh2, 0.0) } else { (0.0, 2.0 - sh2, sh2 - 1.0) };
        Self { thr: p.thresh, knw: p.knee, cslope: 1.0 - inv_r, kc0, kc1, kc2 }
    }

    #[inline]
    pub fn gr(&self, x: f64) -> f64 {
        let d = x - self.thr;
        if self.knw > 0.001 {
            if d <= -self.knw * 0.5 {
                0.0
            } else if d >= self.knw * 0.5 {
                d * self.cslope
            } else {
                let u = (d + self.knw * 0.5) / self.knw;
                let (u2, u3) = (u * u, u * u * u);
                let u4 = u2 * u2;
                let g = self.kc0 * u2 * 0.5 + self.kc1 * (u3 - u4 * 0.5) + self.kc2 * (u4 * u2 - 3.0 * u4 * u + 2.5 * u4);
                self.cslope * self.knw * g
            }
        } else if d > 0.0 {
            d * self.cslope
        } else {
            0.0
        }
    }
}

fn one_pole(sr: f64, secs: f64) -> f64 {
    1.0 - (-1.0 / (sr * secs)).exp()
}

pub struct Engine {
    sr: f64,
    sr_prev: f64,
    p: Params,
    dirty: bool,
    /// Smooth Drive/Bias/Makeup/Mix/Sat Mix (JSFX has none; set false for null tests against it).
    pub smoothing: bool,
    snap: bool,

    // ---- sat
    lut: ShaperLut,
    lut_cnt: u32,
    taps: Taps,
    ov: [Oversampler; 2],
    pre: f64,
    pre_t: f64,
    bias: f64,
    bias_t: f64,
    sat_on: bool,
    use_hp: bool,
    use_lp: bool,
    use_tone: bool,
    hpc: Coef,
    lpc: Coef,
    tnc: Coef,
    hps: [[f64; 2]; 2],
    lps: [[f64; 2]; 2],
    tns: [[f64; 2]; 2],
    os: i32,
    last_os: i32,
    lat: usize,
    la_s: usize,
    dc_r: f64,
    dcx: [f64; 2],
    dcy: [f64; 2],
    /// True until the first saturated sample: a fresh engine (REAPER rebuilds it on every FX enable) must start the
    /// oversampler and DC blocker in their settled state, else the saturator's constant output (bias) leaks as a step.
    dc_warm: bool,
    /// True until the first real `set_params`; `new()` derives from defaults, which would otherwise use up `snap`.
    first_set: bool,
    dry: [[f64; 64]; 2],
    res: [[f64; 64]; 2],
    bd: [[f64; 64]; 2],
    dw: [Vec<f64>; 2],
    lk: [Vec<f64>; 2],
    wp: usize,
    dwp: usize,
    lkw: usize,
    smix: f64,
    smix_t: f64,
    sg: f64,
    sg_t: f64,
    sat_out_eff: f64,
    sm_c: f64,

    // ---- comp
    gc: GainComputer,
    a_att: f64,
    auto_rel: bool,
    a_rel_f: f64,
    a_rel_s: f64,
    rms_c: f64,
    pk_d: f64,
    ms_c: f64,
    avg_c: f64,
    det_m: f64,
    use_dhp: bool,
    dhc: Coef,
    dhs: [[f64; 2]; 2],
    fb: f64,
    gdist: f64,
    prev: [f64; 2],
    rm: [f64; 2],
    grf: [f64; 2],
    grs: [f64; 2],
    pk_env: f64,
    ms_env: f64,
    gavg: f64,

    // ---- sidechain
    /// Set by the shell per block: an aux input bus exists. Sidechain only takes over the detector when true.
    pub sc_avail: bool,
    /// Solo the (gain/HP/LP-filtered) sidechain signal to the output. UI monitor, not a stored parameter.
    pub sc_solo: bool,
    sc_act: bool,
    sc_g: f64,
    sc_g_t: f64,
    use_sc_hp: bool,
    use_sc_lp: bool,
    sc_hpc: Coef,
    sc_lpc: Coef,
    sc_hps: [[f64; 2]; 2],
    sc_lps: [[f64; 2]; 2],
    scd: [[f64; 64]; 2],
    sc_s: f64,
    sc_mix: f64,
    rm_k: [f64; 2],
    dhs_k: [[f64; 2]; 2],

    // ---- out
    mk: f64,
    mk_t: f64,
    mix: f64,
    mix_t: f64,
    og: f64,
    og_t: f64,
    og_c: f64,
    bg: f64,
    bg_t: f64,
    bg_c: f64,
    byp: bool,
    byp_s: f64,
    byp_c: f64,
    rmsg_c: f64,
    fms_c: f64,
    pub delta_on: bool,
    d_s: f64,

    // ---- level measurement / match
    pub ims: f64,
    pub oms: f64,
    pub pms: f64,
    pub sms: f64,
    pub diff_db: f64,
    pub diff_disp: f64,
    pub sat_db: f64,
    gate_open: bool,
    match_on: bool,
    prev_match_on: bool,
    meas: bool,
    macc: f64,
    mt: f64,
    pub match_trim: f64,
    match_req: bool,
    sat_meas: bool,
    sat_macc: f64,
    sat_mt: f64,
    sat_req: bool,
    sat_result: Option<f64>,

    // ---- EQ
    bank: EqBank,
    eg: [f64; 2],
    eg_t: [f64; 2],
    pub eq_act: [bool; 2],
    eq_en: [f64; 2],
    eq_pen: [bool; 2],
    eq_bs: [f64; 2],
    eq_run: [bool; 2],
    /// Set by the shell while the EQ page is visible; fills `tap` (mono, pre or post stage per `eq_vs`).
    pub eq_tap_on: bool,
    pub eq_vs: usize,
    pub tap: Arc<TapRing>,
    /// output / pre-compression peaks per 2 ms for the UI scope
    pub scope: Arc<ScopeRing>,
    sc_o: f64,
    sc_p: f64,
    sc_n: u32,
    sc_len: u32,

    // ---- meters (consumed/reset by the UI side)
    pub opk: [f64; 2],
    pub clip_f: bool,
    pub gr_blk: f64,
    pub gr_pk: f64,
    pub vz_blk: f64,
    pub vz_lv: f64,
    /// Main-signal level at the compressor input (decaying peak), for the sidechain viewport.
    pub mn_blk: f64,
    pub mn_lv: f64,
    pub sat_blk: f64,
    pub sat_vz: f64,
    pub fms: f64,
}

impl Engine {
    pub fn new(sr: f64) -> Self {
        let p = Params::default();
        let mut e = Self {
            sr,
            sr_prev: 0.0,
            p,
            dirty: true,
            smoothing: true,
            snap: true,
            lut: ShaperLut::new(0),
            lut_cnt: 0,
            taps: Taps::new(),
            ov: [Oversampler::default(), Oversampler::default()],
            pre: 1.0,
            pre_t: 1.0,
            bias: 0.0,
            bias_t: 0.0,
            sat_on: false,
            use_hp: false,
            use_lp: false,
            use_tone: false,
            hpc: biquad::IDENTITY,
            lpc: biquad::IDENTITY,
            tnc: biquad::IDENTITY,
            hps: [[0.0; 2]; 2],
            lps: [[0.0; 2]; 2],
            tns: [[0.0; 2]; 2],
            os: 0,
            last_os: -1,
            lat: 0,
            la_s: 0,
            dc_r: 1.0,
            dcx: [0.0; 2],
            dcy: [0.0; 2],
            dc_warm: true,
            first_set: true,
            dry: [[0.0; 64]; 2],
            res: [[0.0; 64]; 2],
            bd: [[0.0; 64]; 2],
            dw: [vec![0.0; 4096], vec![0.0; 4096]],
            lk: [vec![0.0; 4096], vec![0.0; 4096]],
            wp: 0,
            dwp: 0,
            lkw: 0,
            smix: 1.0,
            smix_t: 1.0,
            sg: 1.0,
            sg_t: 1.0,
            sat_out_eff: 0.0,
            sm_c: 0.0,
            gc: GainComputer::default(),
            a_att: 0.0,
            auto_rel: false,
            a_rel_f: 0.0,
            a_rel_s: 0.0,
            rms_c: 0.0,
            pk_d: 0.0,
            ms_c: 0.0,
            avg_c: 0.0,
            det_m: 0.0,
            use_dhp: false,
            dhc: biquad::IDENTITY,
            dhs: [[0.0; 2]; 2],
            fb: 0.0,
            gdist: 0.0,
            prev: [0.0; 2],
            rm: [0.0; 2],
            grf: [0.0; 2],
            grs: [0.0; 2],
            pk_env: 0.0,
            ms_env: 0.0,
            gavg: 0.0,
            mk: 1.0,
            mk_t: 1.0,
            mix: 1.0,
            mix_t: 1.0,
            og: 1.0,
            og_t: 1.0,
            og_c: 0.0,
            bg: 1.0,
            bg_t: 1.0,
            bg_c: 0.0,
            byp: false,
            byp_s: 0.0,
            byp_c: 0.0,
            rmsg_c: 0.0,
            fms_c: 0.0,
            delta_on: false,
            d_s: 0.0,
            ims: 1e-9,
            oms: 1e-9,
            pms: 1e-9,
            sms: 1e-9,
            diff_db: 0.0,
            diff_disp: 0.0,
            sat_db: 0.0,
            gate_open: false,
            match_on: false,
            prev_match_on: false,
            meas: false,
            macc: 0.0,
            sc_avail: false,
            sc_solo: false,
            sc_act: false,
            sc_g: 1.0,
            sc_g_t: 1.0,
            use_sc_hp: false,
            use_sc_lp: false,
            sc_hpc: biquad::IDENTITY,
            sc_lpc: biquad::IDENTITY,
            sc_hps: [[0.0; 2]; 2],
            sc_lps: [[0.0; 2]; 2],
            scd: [[0.0; 64]; 2],
            sc_s: 0.0,
            sc_mix: 1.0,
            rm_k: [0.0; 2],
            dhs_k: [[0.0; 2]; 2],
            mt: 0.0,
            match_trim: 0.0,
            match_req: false,
            sat_meas: false,
            sat_macc: 0.0,
            sat_mt: 0.0,
            sat_req: false,
            sat_result: None,
            bank: EqBank::default(),
            eg: [1.0; 2],
            eg_t: [1.0; 2],
            eq_act: [false; 2],
            eq_en: [1.0; 2],
            eq_pen: [true; 2],
            eq_bs: [1.0; 2],
            eq_run: [false; 2],
            eq_tap_on: false,
            eq_vs: 0,
            tap: Arc::new(TapRing::new()),
            scope: Arc::new(ScopeRing::new()),
            sc_o: 0.0,
            sc_p: 0.0,
            sc_n: 0,
            sc_len: 96,
            opk: [0.0; 2],
            clip_f: false,
            gr_blk: 0.0,
            gr_pk: 0.0,
            vz_blk: 0.0,
            vz_lv: 0.0,
            mn_blk: 0.0,
            mn_lv: 0.0,
            sat_blk: 0.0,
            sat_vz: 0.0,
            fms: 1e-9,
        };
        e.derive();
        e
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sr = sr;
        self.dirty = true;
    }

    pub fn set_params(&mut self, p: &Params) {
        if self.first_set {
            // A fresh engine must start at the host's settings, not glide there from defaults (that glide is audible as a
            // thump when REAPER rebuilds the engine on every FX enable).
            self.first_set = false;
            self.p = *p;
            self.sat_out_eff = p.sat_out;
            self.snap = true;
            let sm = std::mem::replace(&mut self.smoothing, false); // shaper table commits at once, no morph from shape 0
            self.derive();
            self.smoothing = sm;
            return;
        }
        if *p != self.p {
            if p.sat_out != self.p.sat_out {
                self.sat_out_eff = p.sat_out;
            }
            self.p = *p;
            self.derive(); // applied immediately so latency() is valid before the first block
        }
    }

    /// PDC latency to report (oversampling + lookahead).
    pub fn latency(&self) -> u32 {
        (self.lat + self.la_s) as u32
    }

    pub fn request_match(&mut self) {
        self.match_req = true;
    }
    pub fn request_sat_match(&mut self) {
        self.sat_req = true;
    }
    pub fn sat_matching(&self) -> bool {
        self.sat_meas
    }
    pub fn out_matching(&self) -> bool {
        self.meas
    }
    pub fn out_match_progress(&self) -> f64 {
        self.mt
    }
    pub fn sat_match_progress(&self) -> f64 {
        self.sat_mt
    }
    /// A finished sat level-match (new Sat Out, dB); the shell should write it to the Sat Out parameter.
    pub fn take_sat_result(&mut self) -> Option<f64> {
        self.sat_result.take()
    }
    pub fn bg_gain(&self) -> f64 {
        self.bg
    }
    pub fn gate_open(&self) -> bool {
        self.gate_open
    }
    pub fn gain_computer(&self) -> GainComputer {
        self.gc
    }
    pub fn sample_rate(&self) -> f64 {
        self.sr
    }

    /// Derived parameters (JSFX @block "dirty" branch).
    fn derive(&mut self) {
        let p = self.p;
        let sr = self.sr;
        self.sr_prev = sr;
        self.dirty = false;

        // ---- SAT
        self.lut.fade = self.smoothing;
        self.lut.sync(p.shape, &p.curve);
        self.pre_t = (p.drive * 0.01 * 36.0 * LN10_20).exp();
        self.bias_t = p.bias * 0.01;
        self.sat_on = p.drive > 0.0 || p.bias != 0.0;
        self.use_hp = p.sat_hp > 20.5;
        if self.use_hp {
            self.hpc = biquad::hp(p.sat_hp, 0.70710678, sr);
        }
        self.use_lp = p.sat_lp < 19900.0;
        if self.use_lp {
            self.lpc = biquad::lp(p.sat_lp, 0.70710678, sr);
        }
        self.use_tone = p.tone < 19900.0;
        if self.use_tone {
            self.tnc = biquad::lp(p.tone, 0.70710678, sr);
        }
        self.os = (p.os as f64 + 0.5).floor() as i32;
        if self.os != self.last_os {
            self.last_os = self.os;
            self.ov[0].clear();
            self.ov[1].clear();
            self.res = [[0.0; 64]; 2];
        }
        self.lat = match self.os {
            0 => 0,
            1 => 24,
            _ => 28,
        };
        self.la_s = ((p.lookahead_ms * 0.001 * sr + 0.5).floor().max(0.0) as usize).min(1900);
        self.dc_r = 1.0 - 2.0 * PI * 5.0 / sr;

        // ---- COMP
        self.gc = GainComputer::from_params(&p);
        let spd = 4f64.powf((p.speed - 50.0) / 50.0);
        self.a_att = 1.0 - (-1.0 / (sr * (p.attack * 0.001 * spd).max(0.00001))).exp();
        self.auto_rel = p.release >= 1990.0;
        let a_rel = 1.0 - (-1.0 / (sr * (p.release * 0.001 * spd).max(0.00001))).exp();
        self.a_rel_f = if self.auto_rel { 1.0 - (-1.0 / (sr * 0.04 * spd)).exp() } else { a_rel };
        self.a_rel_s = 1.0 - (-1.0 / (sr * 0.6 * spd)).exp();
        self.rms_c = one_pole(sr, 0.01);
        self.pk_d = (-1.0 / (sr * 0.05)).exp();
        self.ms_c = one_pole(sr, 0.3);
        self.avg_c = one_pole(sr, 0.25);
        self.det_m = p.detector * 0.01;
        self.use_dhp = p.det_hp > 15.0;
        if self.use_dhp {
            self.dhc = biquad::hp(p.det_hp, 0.70710678, sr);
        }
        self.fb = p.ff_fb * 0.01;
        self.sc_mix = (p.sc_mix * 0.01).clamp(0.0, 1.0);
        self.sc_g_t = (p.sc_gain * LN10_20).exp();
        self.use_sc_hp = p.sc_hp > 20.5;
        if self.use_sc_hp {
            self.sc_hpc = biquad::hp(p.sc_hp, 0.70710678, sr);
        }
        self.use_sc_lp = p.sc_lp < 19900.0;
        if self.use_sc_lp {
            self.sc_lpc = biquad::lp(p.sc_lp, 0.70710678, sr);
        }
        self.gdist = p.gr_dist * 0.01;
        self.mk_t = (p.makeup * LN10_20).exp();
        self.mix_t = p.mix * 0.01;
        self.match_on = p.gain_match;
        if self.match_on && !self.prev_match_on {
            self.meas = true;
            self.macc = 0.0;
            self.mt = 0.0;
        }
        if !self.match_on {
            self.meas = false;
        }
        self.prev_match_on = self.match_on;
        self.rmsg_c = one_pole(sr, 0.5);
        self.og_c = one_pole(sr, 0.02);
        self.smix_t = p.sat_mix * 0.01;
        self.byp = p.bypass;
        self.byp_c = one_pole(sr, 0.01);
        self.sc_len = ((sr * ScopeRing::BUCKET_S).round() as u32).max(1);
        self.bg_c = one_pole(sr, 0.2);
        self.fms_c = one_pole(sr, 0.3);
        self.sm_c = one_pole(sr, 0.02);

        // ---- EQ
        for st in 0..2 {
            let any = self.bank.setup_stage(st, &p.eq[st], sr);
            let g2 = p.eq[st].out_gain.clamp(-12.0, 12.0);
            self.eg_t[st] = (g2 * LN10_20).exp();
            self.eq_act[st] = any || g2 != 0.0;
        }
        self.eq_en[0] = if !p.eq_pre_bypass && !p.eq_module_bypass { 1.0 } else { 0.0 };
        self.eq_en[1] = if !p.eq_post_bypass && !p.eq_module_bypass { 1.0 } else { 0.0 };
        for st in 0..2 {
            let en = self.eq_en[st] > 0.5;
            if en && !self.eq_pen[st] {
                self.bank.clear_stage(st);
                self.eg[st] = self.eg_t[st];
            }
            self.eq_pen[st] = en;
        }

        if self.snap {
            self.snap = false;
            self.pre = self.pre_t;
            self.bias = self.bias_t;
            self.mk = self.mk_t;
            self.mix = self.mix_t;
            self.smix = self.smix_t;
            self.sat_out_eff = p.sat_out;
            self.sg = (self.sat_out_eff * LN10_20).exp();
            self.sg_t = self.sg;
            self.eg = self.eg_t;
            self.eq_bs = self.eq_en;
        }
    }

    /// Per-block housekeeping (JSFX @block). Call once per host buffer with its length.
    pub fn begin_block(&mut self, n: usize) {
        if self.match_req {
            self.meas = true;
            self.macc = 0.0;
            self.mt = 0.0;
            self.match_req = false;
        }
        if self.dirty || self.sr != self.sr_prev {
            self.derive();
        }
        for s in 0..2 {
            if (self.eq_bs[s] - self.eq_en[s]).abs() < 0.00001 {
                self.eq_bs[s] = self.eq_en[s];
            }
        }
        self.bank.prep(n);
        for s in 0..2 {
            if (self.eg[s] - self.eg_t[s]).abs() < 0.0000001 {
                self.eg[s] = self.eg_t[s];
            }
            self.eq_run[s] = self.bank.stage_running(s) || self.eg[s] != 1.0 || self.eg_t[s] != 1.0;
        }

        // ---- gain diff / match
        let dt = n as f64 / self.sr;
        self.gate_open = self.ims > 0.000001;
        if self.gate_open {
            self.diff_db = DB_POW * (self.oms.max(1e-12) / self.ims.max(1e-12)).ln();
            self.diff_disp = DB_POW * (self.pms.max(1e-12) / self.ims.max(1e-12)).ln();
            self.sat_db = DB_POW * (self.sms.max(1e-12) / self.ims.max(1e-12)).ln();
        }
        if self.sat_req {
            self.sat_meas = !self.sat_meas;
            self.sat_macc = 0.0;
            self.sat_mt = 0.0;
            self.sat_req = false;
        }
        if self.sat_meas && self.gate_open {
            self.sat_macc += self.sat_db * dt;
            self.sat_mt += dt;
            if self.sat_mt >= 3.0 {
                let v = ((-self.sat_macc / self.sat_mt).clamp(-24.0, 12.0) * 10.0 + 0.5).floor() * 0.1;
                self.sat_out_eff = v;
                self.sat_result = Some(v);
                self.sat_meas = false;
            }
        }
        self.sg_t = (self.sat_out_eff * LN10_20).exp();
        if self.meas && self.gate_open {
            self.macc += self.diff_db * dt;
            self.mt += dt;
            if self.mt >= 3.0 {
                self.match_trim = (-self.macc / self.mt).clamp(-12.0, 12.0);
                self.meas = false;
            }
        }
        let trim = if self.match_on { self.match_trim } else { 0.0 };
        self.og_t = ((self.p.out_gain + trim) * LN10_20).exp();
        self.gr_pk = self.gr_pk.max(self.gr_blk);
        self.gr_blk = 0.0;
        let vz_d = (-(n as f64) / (self.sr * 0.15)).exp();
        self.vz_lv = self.vz_blk.max(self.vz_lv * vz_d);
        self.vz_blk = 0.0;
        self.mn_lv = self.mn_blk.max(self.mn_lv * vz_d);
        self.mn_blk = 0.0;
        self.sat_vz = self.sat_blk.max(self.sat_vz * vz_d);
        self.sat_blk = 0.0;
    }

    /// Process a stereo buffer in place (one host block).
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.process_sc(left, right, None);
    }

    /// As `process`, with an optional sidechain input (same length). `None` = no sidechain bus: the detector keys off
    /// the main signal even if sidechain is switched on.
    pub fn process_sc(&mut self, left: &mut [f32], right: &mut [f32], sc: Option<(&[f32], &[f32])>) {
        let n = left.len().min(right.len());
        self.sc_avail = sc.is_some();
        self.begin_block(n);
        for i in 0..n {
            let (a, b) = match sc {
                Some((l, r)) => (l.get(i).copied().unwrap_or(0.0) as f64, r.get(i).copied().unwrap_or(0.0) as f64),
                None => (0.0, 0.0),
            };
            let (l, r) = self.tick_sc(left[i] as f64, right[i] as f64, a, b);
            left[i] = l as f32;
            right[i] = r as f32;
        }
    }

    #[inline]
    pub fn tick(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        self.tick_sc(in_l, in_r, 0.0, 0.0)
    }

    /// One sample with a sidechain sample pair (ignored unless sidechain is on and `sc_avail`).
    #[inline]
    pub fn tick_sc(&mut self, in_l: f64, in_r: f64, sc_l: f64, sc_r: f64) -> (f64, f64) {
        let inp = [in_l, in_r];
        self.lut_cnt += 1;
        if self.lut_cnt >= crate::shapers::FADE_EVERY {
            self.lut_cnt = 0;
            self.lut.step();
        }
        self.ims += ((in_l * in_l + in_r * in_r) * 0.5 + DENORM - self.ims) * self.rmsg_c;
        if self.smoothing {
            let c = self.sm_c;
            self.pre += (self.pre_t - self.pre) * c;
            self.bias += (self.bias_t - self.bias) * c;
            self.mk += (self.mk_t - self.mk) * c;
            self.mix += (self.mix_t - self.mix) * c;
            self.smix += (self.smix_t - self.smix) * c;
        } else {
            self.pre = self.pre_t;
            self.bias = self.bias_t;
            self.mk = self.mk_t;
            self.mix = self.mix_t;
            self.smix = self.smix_t;
        }

        self.wp = (self.wp + 1) & 63;
        let wp = self.wp;
        for c in 0..2 {
            self.dry[c][wp] = inp[c];
        }
        let rd = (wp + 64 - self.lat) & 63;
        let dl = [self.dry[0][rd], self.dry[1][rd]];
        self.dwp = (self.dwp + 1) & 4095;
        for c in 0..2 {
            self.dw[c][self.dwp] = inp[c];
        }
        let ti = (self.dwp + 4096 - self.lat - self.la_s) & 4095;
        let dt = [self.dw[0][ti], self.dw[1][ti]];

        // ------------------------------------------------ SAT
        let mut cc = [0.0f64; 2];
        if self.sat_on {
            let mut bp = inp;
            if self.use_hp {
                for c in 0..2 {
                    bp[c] = bq(&mut self.hps[c], &self.hpc, bp[c]);
                }
            }
            if self.use_lp {
                for c in 0..2 {
                    bp[c] = bq(&mut self.lps[c], &self.lpc, bp[c]);
                }
            }
            for c in 0..2 {
                self.res[c][wp] = inp[c] - bp[c];
                self.bd[c][wp] = bp[c];
            }
            self.sat_blk = self.sat_blk.max(bp[0].abs().max(bp[1].abs()));
            for c in 0..2 {
                let xin = bp[c] * self.pre + self.bias;
                if self.dc_warm {
                    for _ in 0..256 {
                        self.ov[c].process(&self.taps, &self.lut, self.os, xin);
                    }
                }
                let sh = self.ov[c].process(&self.taps, &self.lut, self.os, xin);
                if self.dc_warm {
                    self.dcx[c] = sh;
                    self.dcy[c] = 0.0;
                }
                let mut y = sh - self.dcx[c] + self.dc_r * self.dcy[c] + DENORM;
                self.dcx[c] = sh;
                self.dcy[c] = y;
                if self.use_tone {
                    y = bq(&mut self.tns[c], &self.tnc, y);
                }
                if self.smix < 0.999 {
                    y = self.bd[c][rd] + (y - self.bd[c][rd]) * self.smix;
                }
                cc[c] = self.res[c][rd] + y;
            }
            self.dc_warm = false;
        } else {
            self.res[0][wp] = 0.0;
            self.res[1][wp] = 0.0;
            cc = dl;
        }

        // SAT OUT
        self.sms += ((cc[0] * cc[0] + cc[1] * cc[1]) * 0.5 + DENORM - self.sms) * self.rmsg_c;
        self.sg += (self.sg_t - self.sg) * self.og_c;
        cc[0] *= self.sg;
        cc[1] *= self.sg;

        // ------------------------------------------------ EQ PRE
        if self.eq_tap_on && self.eq_vs == 0 {
            self.tap.push(((cc[0] + cc[1]) * 0.5) as f32);
        }
        self.eq_bs[0] += (self.eq_en[0] - self.eq_bs[0]) * self.byp_c;
        if self.eq_run[0] && self.eq_bs[0] > 0.0 {
            let (mut xl, mut xr) = (cc[0], cc[1]);
            self.bank.process(0, &mut xl, &mut xr);
            self.eg[0] += (self.eg_t[0] - self.eg[0]) * self.og_c;
            if self.eq_bs[0] >= 1.0 {
                cc = [xl * self.eg[0], xr * self.eg[0]];
            } else {
                let b = self.eq_bs[0];
                cc[0] += (xl * self.eg[0] - cc[0]) * b;
                cc[1] += (xr * self.eg[0] - cc[1]) * b;
            }
        }

        // ------------------------------------------------ LOOKAHEAD
        self.lkw = (self.lkw + 1) & 4095;
        for c in 0..2 {
            self.lk[c][self.lkw] = cc[c];
        }
        let cd = if self.la_s > 0 {
            let i = (self.lkw + 4096 - self.la_s) & 4095;
            [self.lk[0][i], self.lk[1][i]]
        } else {
            cc
        };

        // ------------------------------------------------ SIDECHAIN (gain > HP > LP), aligned with `cc` (delayed by the sat latency)
        self.sc_act = self.p.sc_on && self.sc_avail;
        self.sc_g += (self.sc_g_t - self.sc_g) * self.og_c;
        let mut scv = [0.0f64; 2];
        if self.sc_act {
            let s_in = [sc_l, sc_r];
            for c in 0..2 {
                let mut v = s_in[c] * self.sc_g;
                if self.use_sc_hp {
                    v = bq(&mut self.sc_hps[c], &self.sc_hpc, v);
                }
                if self.use_sc_lp {
                    v = bq(&mut self.sc_lps[c], &self.sc_lpc, v);
                }
                self.scd[c][wp] = v;
                scv[c] = self.scd[c][rd];
            }
        }
        self.mn_blk = self.mn_blk.max(cc[0].abs().max(cc[1].abs()));

        // ------------------------------------------------ COMP
        let mut lv = [0.0f64; 2];
        for c in 0..2 {
            // KEY MIX: detector level = (1-m) * main + m * key (levels, not audio, so the two never phase-interact).
            // Without an active sidechain m = 0. With one, FF>FB is not applied (it is replaced by KEY MIX).
            let km = if self.sc_act { self.sc_mix } else { 0.0 };
            let mut l = 0.0;
            if km < 1.0 {
                let xd = if self.sc_act { cc[c] } else { cc[c] + self.fb * (self.prev[c] - cc[c]) };
                l += (1.0 - km)
                    * det_level(xd, &mut self.rm[c], &mut self.dhs[c], self.use_dhp, &self.dhc, self.det_m, self.rms_c);
            }
            if km > 0.0 {
                l += km
                    * det_level(scv[c], &mut self.rm_k[c], &mut self.dhs_k[c], self.use_dhp, &self.dhc, self.det_m, self.rms_c);
            }
            lv[c] = l;
        }
        let lmx = lv[0].max(lv[1]);
        self.vz_blk = self.vz_blk.max(lmx);

        let mut tg = [0.0f64; 2];
        for c in 0..2 {
            tg[c] = self.gc.gr(DB_AMP * lv[c].max(0.000001).ln());
        }
        let mut g = [0.0f64; 2];
        for c in 0..2 {
            let k = if tg[c] > self.grf[c] { self.a_att } else { self.a_rel_f };
            self.grf[c] += (tg[c] - self.grf[c]) * k;
            if self.grf[c] < 0.000000001 {
                self.grf[c] = 0.0;
            }
        }
        if self.auto_rel {
            for c in 0..2 {
                let k = if tg[c] > self.grs[c] { self.a_att } else { self.a_rel_s };
                self.grs[c] += (tg[c] - self.grs[c]) * k;
                if self.grs[c] < 0.000000001 {
                    self.grs[c] = 0.0;
                }
            }
            self.pk_env = lmx.max(self.pk_env * self.pk_d);
            self.ms_env += (lmx * lmx + DENORM - self.ms_env) * self.ms_c;
            let crest = self.pk_env / (self.ms_env.sqrt() + 0.000000001);
            let cn = ((crest - 1.5) * 0.2857).clamp(0.0, 1.0);
            self.gavg += (self.grf[0].max(self.grf[1]) - self.gavg) * self.avg_c;
            let wgt = 0.1 + 0.75 * (self.gavg * 0.1111).min(1.0) * (1.0 - cn);
            for c in 0..2 {
                g[c] = self.grf[c] + wgt * (self.grs[c] - self.grf[c]);
            }
        } else {
            g = self.grf;
        }

        let mut y = [cd[0] * (-g[0] * LN10_20).exp(), cd[1] * (-g[1] * LN10_20).exp()];
        if self.gdist > 0.0 {
            for c in 0..2 {
                let dd = self.gdist * (g[c] * 0.0833333).min(1.0);
                y[c] += dd * (y[c] / (1.0 + 0.5 * y[c].abs()) - y[c]);
            }
        }
        self.prev = y;

        // ------------------------------------------------ OUT
        let mut o = [0.0f64; 2];
        for c in 0..2 {
            let w = y[c] * self.mk;
            o[c] = dt[c] + (w - dt[c]) * self.mix;
        }
        // EQ POST
        if self.eq_tap_on && self.eq_vs == 1 {
            self.tap.push(((o[0] + o[1]) * 0.5) as f32);
        }
        self.eq_bs[1] += (self.eq_en[1] - self.eq_bs[1]) * self.byp_c;
        if self.eq_run[1] && self.eq_bs[1] > 0.0 {
            let (mut xl, mut xr) = (o[0], o[1]);
            self.bank.process(1, &mut xl, &mut xr);
            self.eg[1] += (self.eg_t[1] - self.eg[1]) * self.og_c;
            if self.eq_bs[1] >= 1.0 {
                o = [xl * self.eg[1], xr * self.eg[1]];
            } else {
                let b = self.eq_bs[1];
                o[0] += (xl * self.eg[1] - o[0]) * b;
                o[1] += (xr * self.eg[1] - o[1]) * b;
            }
        }
        self.oms += ((o[0] * o[0] + o[1] * o[1]) * 0.5 + DENORM - self.oms) * self.rmsg_c;
        self.og += (self.og_t - self.og) * self.og_c;
        let pp = [o[0] * self.og, o[1] * self.og];
        self.pms += ((pp[0] * pp[0] + pp[1] * pp[1]) * 0.5 + DENORM - self.pms) * self.rmsg_c;
        // equal-loudness bypass
        self.byp_s += ((if self.byp { 1.0 } else { 0.0 }) - self.byp_s) * self.byp_c;
        if self.gate_open {
            self.bg_t = (self.pms / self.ims).sqrt().clamp(0.0631, 15.85);
        }
        self.bg += (self.bg_t - self.bg) * self.bg_c;
        let mut out = [pp[0] + (dt[0] * self.bg - pp[0]) * self.byp_s, pp[1] + (dt[1] * self.bg - pp[1]) * self.byp_s];
        // delta monitor
        self.d_s += ((if self.delta_on { 1.0 } else { 0.0 }) - self.d_s) * self.byp_c;
        if self.d_s > 0.0001 {
            for c in 0..2 {
                out[c] += ((cd[c] - y[c]) - out[c]) * self.d_s;
            }
        }
        // sidechain solo: what the detector hears (post gain/HP/LP), replaces the output
        self.sc_s += ((if self.sc_solo && self.sc_act { 1.0 } else { 0.0 }) - self.sc_s) * self.byp_c;
        if self.sc_s > 0.0001 {
            for c in 0..2 {
                out[c] += (scv[c] - out[c]) * self.sc_s;
            }
        }
        // meters
        self.opk[0] = self.opk[0].max(out[0].abs());
        self.opk[1] = self.opk[1].max(out[1].abs());
        if out[0].abs() > 0.9999 || out[1].abs() > 0.9999 {
            self.clip_f = true;
        }
        self.fms += ((out[0] * out[0] + out[1] * out[1]) * 0.5 + DENORM - self.fms) * self.fms_c;
        let gm = g[0].max(g[1]);
        self.gr_blk = self.gr_blk.max(gm);
        // scope bucket: output peak, and the same with the gain reduction added back
        let ao = out[0].abs().max(out[1].abs());
        self.sc_o = self.sc_o.max(ao);
        self.sc_p = self.sc_p.max(ao * (gm * LN10_20).exp());
        self.sc_n += 1;
        if self.sc_n >= self.sc_len {
            self.scope.push(self.sc_o as f32, self.sc_p as f32);
            self.sc_o = 0.0;
            self.sc_p = 0.0;
            self.sc_n = 0;
        }
        (out[0], out[1])
    }
}

/// Detector level of one source: optional low cut, then peak/RMS blend.
#[inline]
fn det_level(mut xd: f64, rm: &mut f64, hs: &mut [f64; 2], use_hp: bool, hc: &Coef, det_m: f64, rms_c: f64) -> f64 {
    if use_hp {
        xd = bq(hs, hc, xd);
    }
    let a = xd.abs();
    *rm += (xd * xd + DENORM - *rm) * rms_c;
    if det_m > 0.0 {
        a + det_m * (1.41421356 * rm.sqrt() - a)
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn neutral() -> Params {
        let mut p = Params::default();
        p.drive = 0.0;
        p.ratio = 1.0;
        p
    }

    fn noise(n: usize, seed: u64) -> Vec<f64> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
            })
            .collect()
    }

    fn run(e: &mut Engine, x: &[f64], block: usize) -> Vec<f64> {
        let mut out = Vec::with_capacity(x.len());
        for chunk in x.chunks(block) {
            e.begin_block(chunk.len());
            for &v in chunk {
                out.push(e.tick(v, v).0);
            }
        }
        out
    }

    fn rms(x: &[f64]) -> f64 {
        (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
    }

    fn sine(n: usize, f: f64, a: f64) -> Vec<f64> {
        (0..n).map(|i| a * (2.0 * PI * f * i as f64 / SR).sin()).collect()
    }

    #[test]
    fn neutral_settings_are_bit_transparent() {
        let mut e = Engine::new(SR);
        e.set_params(&neutral());
        let x = noise(4800, 1);
        let y = run(&mut e, &x, 256);
        for (a, b) in x.iter().zip(&y) {
            assert!((a - b).abs() < 1e-12);
        }
        assert_eq!(e.latency(), 0);
    }

    #[test]
    fn oversampled_path_is_pure_delay_when_sat_mix_is_zero() {
        for (os, lat) in [(1, 24usize), (2, 28usize)] {
            let mut p = neutral();
            p.drive = 50.0; // sat on
            p.os = os;
            p.sat_mix = 0.0;
            let mut e = Engine::new(SR);
            e.smoothing = false;
            e.set_params(&p);
            assert_eq!(e.latency() as usize, lat);
            let mut x = vec![0.0; 200];
            x[10] = 1.0;
            let y = run(&mut e, &x, 64);
            let peak = y.iter().enumerate().max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap()).unwrap().0;
            assert_eq!(peak, 10 + lat, "os {os}");
            assert!((y[10 + lat] - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn lookahead_reports_and_applies_latency() {
        let mut p = neutral();
        p.lookahead_ms = 1.0;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let mut x = vec![0.0; 200];
        x[5] = 1.0;
        let y = run(&mut e, &x, 64);
        assert_eq!(e.latency(), 48);
        assert!((y[5 + 48] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn compressor_reduces_a_loud_sine_and_ignores_a_quiet_one() {
        let mut p = neutral();
        p.thresh = -30.0;
        p.ratio = 10.0;
        p.detector = 100.0;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let loud = sine(48000, 200.0, 0.8);
        let y = run(&mut e, &loud, 256);
        let r = rms(&y[24000..]) / rms(&loud[24000..]);
        assert!(r < 0.5, "ratio {r}");
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let quiet = sine(48000, 200.0, 0.01);
        let y = run(&mut e, &quiet, 256);
        assert!((rms(&y[24000..]) / rms(&quiet[24000..]) - 1.0).abs() < 0.01);
    }

    #[test]
    fn gain_computer_matches_hard_and_soft_knee_geometry() {
        let mut p = Params::default();
        p.knee = 0.0;
        p.thresh = -20.0;
        p.ratio = 4.0;
        let g = GainComputer::from_params(&p);
        assert_eq!(g.gr(-30.0), 0.0);
        assert!((g.gr(-8.0) - 12.0 * 0.75).abs() < 1e-12);
        p.knee = 6.0;
        let g = GainComputer::from_params(&p);
        assert_eq!(g.gr(-24.0), 0.0);
        assert!((g.gr(-17.0) - 3.0 * 0.75).abs() < 1e-12); // at knee top: full slope
        assert!(g.gr(-20.0) > 0.0 && g.gr(-20.0) < 3.0 * 0.75); // continuous inside
    }

    #[test]
    fn pre_eq_bell_boosts_its_band() {
        let mut p = neutral();
        p.eq[0].bands[1].freq = 1000.0;
        p.eq[0].bands[1].gain = 6.0;
        p.eq[0].bands[1].q = 1.0;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let x = sine(24000, 1000.0, 0.1);
        let y = run(&mut e, &x, 256);
        let gdb = 20.0 * (rms(&y[12000..]) / rms(&x[12000..])).log10();
        assert!((gdb - 6.0).abs() < 0.2, "{gdb}");
        // module bypass restores unity
        p.eq_module_bypass = true;
        e.set_params(&p);
        let y = run(&mut e, &x, 256);
        let gdb = 20.0 * (rms(&y[12000..]) / rms(&x[12000..])).log10();
        assert!(gdb.abs() < 0.05, "{gdb}");
    }

    #[test]
    fn saturation_adds_harmonics_and_stays_bounded() {
        let mut p = Params::default();
        p.ratio = 1.0;
        p.drive = 80.0;
        p.shape = 1;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let x = sine(9600, 1000.0, 0.5);
        let y = run(&mut e, &x, 256);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() < 1.5));
        // crest factor of a clipped sine drops
        let crest = |s: &[f64]| s.iter().fold(0.0f64, |a, v| a.max(v.abs())) / rms(s);
        assert!(crest(&y[4800..]) < crest(&x[4800..]));
    }

    #[test]
    fn all_shapers_and_oversampling_modes_run_clean() {
        for shape in 0..15 {
            for os in 0..3 {
                let mut p = Params::default();
                p.shape = shape;
                p.os = os;
                p.drive = 70.0;
                p.sat_hp = 120.0;
                p.sat_lp = 8000.0;
                p.tone = 6000.0;
                let mut e = Engine::new(SR);
                e.set_params(&p);
                let y = run(&mut e, &noise(4800, 7), 128);
                assert!(y.iter().all(|v| v.is_finite()), "shape {shape} os {os}");
            }
        }
    }

    /// Switching shaper mid-signal must not produce a step: compare the largest sample-to-sample jump
    /// across the switch with the morph on (default) and off (instant table swap).
    #[test]
    fn shaper_switch_morphs_instead_of_clicking() {
        let jump = |fade: bool| {
            let mut p = neutral();
            p.drive = 25.0;
            p.shape = 1; // Soft
            let mut e = Engine::new(SR);
            e.smoothing = fade; // fade follows smoothing
            e.set_params(&p);
            let x = sine(24000, 2.0, 0.5); // near-DC: only a table swap can make a step
            let mut y = run(&mut e, &x[..6000], 256);
            p.shape = 4; // Hard: reads ~0.1 higher than Soft at the sine peak (signal there is flat)
            e.set_params(&p);
            y.extend(run(&mut e, &x[6000..], 256));
            y[5900..7500].windows(2).fold(0.0f64, |m, w| m.max((w[1] - w[0]).abs()))
        };
        let (instant, morphed) = (jump(false), jump(true));
        assert!(morphed <= instant + 1e-9, "morph {morphed} vs instant {instant}");
        eprintln!("max step: instant {instant:.4}, morphed {morphed:.4}");
    }

    #[test]
    fn fresh_engine_with_bias_has_no_startup_thump() {
        let mut p = Params::default();
        p.bias = -6.0;
        p.drive = 22.0;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let y = run(&mut e, &vec![0.0; 9600], 256);
        let pk = y.iter().fold(0.0f64, |m, v| m.max(v.abs()));
                assert!(pk < 0.01, "startup thump peak {pk}");
    }

    #[test]
    fn scope_buckets_show_output_and_reduction() {
        let mut p = Params::default();
        p.drive = 0.0;
        p.bias = 0.0;
        p.thresh = -30.0;
        p.ratio = 8.0;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let x = sine(48000, 200.0, 0.5);
        let _ = run(&mut e, &x, 256);
        let n = e.scope.head();
        assert!((n as f64 - 48000.0 / (SR * ScopeRing::BUCKET_S)).abs() < 3.0, "buckets {n}");
        let (o, pre) = e.scope.get(n - 20);
        assert!(o > 0.01 && pre > o * 1.5, "out {o} pre {pre}");
    }

    #[test]
    fn bypass_equal_loudness_returns_dry_level() {
        let mut p = neutral();
        p.bypass = true;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let x = noise(48000, 3);
        let y = run(&mut e, &x, 256);
        assert!((rms(&y[24000..]) / rms(&x[24000..]) - 1.0).abs() < 0.02);
    }

    #[test]
    fn sat_match_measures_for_three_seconds_then_reports() {
        let mut p = Params::default();
        p.ratio = 1.0;
        p.drive = 90.0;
        p.shape = 1;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        e.request_sat_match();
        let x: Vec<f64> = noise(48000 * 4, 5).iter().map(|v| v * 0.3).collect();
        let _ = run(&mut e, &x, 512);
        let r = e.take_sat_result().expect("measurement should finish within 4 s");
        assert!(r < 0.0, "hot sat should be trimmed down, got {r}");
        assert!(e.take_sat_result().is_none());
    }

    #[test]
    fn out_match_compensates_level_change() {
        let mut p = neutral();
        p.makeup = 6.0;
        p.gain_match = true;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let x: Vec<f64> = noise(48000 * 5, 9).iter().map(|v| v * 0.3).collect();
        let _ = run(&mut e, &x, 512);
        assert!((e.match_trim + 6.0).abs() < 0.3, "{}", e.match_trim);
    }

    fn sc_params() -> Params {
        let mut p = neutral();
        p.ratio = 20.0;
        p.thresh = -30.0;
        p.attack = 0.1;
        p.sc_on = true;
        p
    }

    fn run_sc(e: &mut Engine, x: &[f64], sc: &[f64]) -> Vec<f64> {
        let a: Vec<f32> = x.iter().map(|v| *v as f32).collect();
        let b: Vec<f32> = sc.iter().map(|v| *v as f32).collect();
        let mut out = Vec::new();
        for (ca, cb) in a.chunks(256).zip(b.chunks(256)) {
            let (mut l, mut r) = (ca.to_vec(), ca.to_vec());
            e.process_sc(&mut l, &mut r, Some((cb, cb)));
            out.extend(l.iter().map(|v| *v as f64));
        }
        out
    }

    #[test]
    fn sidechain_loud_key_ducks_quiet_main() {
        let mut e = Engine::new(SR);
        e.set_params(&sc_params());
        let x = sine(24000, 440.0, 0.03); // -30 dB main, would not trigger on its own
        let sc = sine(24000, 440.0, 0.9);
        let y = run_sc(&mut e, &x, &sc);
        assert!(rms(&y[12000..]) < rms(&x[12000..]) * 0.5, "main should be ducked by the loud key");
    }

    #[test]
    fn sidechain_silent_key_leaves_loud_main_alone() {
        let mut e = Engine::new(SR);
        e.set_params(&sc_params());
        let x = sine(24000, 440.0, 0.9);
        let y = run_sc(&mut e, &x, &vec![0.0; 24000]);
        assert!((rms(&y[12000..]) - rms(&x[12000..])).abs() < 1e-6);
    }

    #[test]
    fn sidechain_without_aux_bus_falls_back_to_internal_detector() {
        let mut e = Engine::new(SR);
        e.set_params(&sc_params());
        let x = sine(24000, 440.0, 0.9);
        let y = run(&mut e, &x, 256); // process() = no bus
        assert!(rms(&y[12000..]) < rms(&x[12000..]) * 0.5);
    }

    #[test]
    fn sidechain_hp_ignores_a_low_key() {
        let mut p = sc_params();
        p.sc_hp = 2000.0;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let x = sine(24000, 440.0, 0.03);
        let y = run_sc(&mut e, &x, &sine(24000, 50.0, 0.9)); // 50 Hz key is filtered away
        assert!((rms(&y[12000..]) - rms(&x[12000..])).abs() < rms(&x[12000..]) * 0.02);
    }

    #[test]
    fn sidechain_solo_outputs_the_filtered_key() {
        let mut p = sc_params();
        p.sc_gain = -6.0;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        e.sc_solo = true;
        let key = sine(24000, 1000.0, 0.5);
        let y = run_sc(&mut e, &sine(24000, 440.0, 0.1), &key);
        let want = rms(&key[12000..]) * 0.5012; // -6 dB
        assert!((rms(&y[12000..]) - want).abs() < want * 0.03);
    }

    #[test]
    fn key_mix_zero_ignores_the_key_and_half_sits_between() {
        let x = sine(24000, 440.0, 0.005); // -46 dB, clear of the knee
        let key = sine(24000, 440.0, 0.9);
        let rms_for = |mix: f64| {
            let mut p = sc_params();
            p.sc_mix = mix;
            let mut e = Engine::new(SR);
            e.set_params(&p);
            let y = run_sc(&mut e, &x, &key);
            rms(&y[12000..])
        };
        let (m0, m50, m100) = (rms_for(0.0), rms_for(50.0), rms_for(100.0));
        assert!((m0 - rms(&x[12000..])).abs() < 1e-4, "mix 0 = main detector only (main is below threshold)");
        assert!(m100 < m50 && m50 < m0, "{m0} {m50} {m100}");
    }
}
