//! The SatComp signal path, composed from the DSP blocks: saturator (HP/LP split, shaper + optional oversampling, DC block,
//! tone, sat mix, sat out) -> EQ pre -> lookahead -> compressor -> makeup/mix -> EQ post -> out gain -> output limiter ->
//! equal-loudness bypass blend -> delta. Port of the JSFX `@block` / `@sample` sections. The blocks (`saturator`, `compressor`, `keyinput`,
//! `eq::EqSection`, `delay`, `level`) hold the algorithms; this file wires them together, owns the output stage and
//! the level-match state machines, and publishes the meters the UI reads.

use crate::compressor::Compressor;
use crate::delay::StereoDelay;
use crate::eq::EqSection;
use crate::keyinput::KeyInput;
use crate::level::{ms_step, one_pole, Matcher, DB_AMP, DB_POW, LN10_20};
use crate::limiter::Limiter;
use crate::params::Params;
use crate::saturator::Saturator;
use crate::DENORM;
pub use crate::compressor::GainComputer;
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
    pub fn push(&self, v: f32) {
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
    pub fn push(&self, o: f32, p: f32) {
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


pub struct Engine {
    sr: f64,
    sr_prev: f64,
    p: Params,
    dirty: bool,
    /// Smooth Drive/Bias/Makeup/Mix/Sat Mix (JSFX has none; set false for null tests against it).
    pub smoothing: bool,
    snap: bool,
    /// True until the first real `set_params`; `new()` derives from defaults, which would otherwise use up `snap`.
    first_set: bool,

    // ---- blocks
    sat: Saturator,
    comp: Compressor,
    key: KeyInput,
    eq: [EqSection; 2],
    lim: Limiter,
    /// delta / sidechain-solo monitor signals, delayed by the limiter latency so they line up with the limited path
    mon_d: StereoDelay<1024>,
    mon_s: StereoDelay<1024>,

    // ---- delay lines: input aligned with the (latency-delayed) wet path, and the compressor lookahead
    dw: StereoDelay<4096>,
    lk: StereoDelay<4096>,
    la_s: usize,
    sm_c: f64,

    // ---- sidechain
    /// Set by the shell per block: an aux input bus exists. Sidechain only takes over the detector when true.
    pub sc_avail: bool,
    /// Solo the (gain/HP/LP-filtered) sidechain signal to the output. UI monitor, not a stored parameter.
    pub sc_solo: bool,
    sc_s: f64,

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
    /// mirror of the saturator's output level (see `Saturator::ms`)
    pub sms: f64,
    pub diff_db: f64,
    pub diff_disp: f64,
    pub sat_db: f64,
    gate_open: bool,
    match_on: bool,
    prev_match_on: bool,
    out_m: Matcher,
    pub match_trim: f64,
    match_req: bool,
    sat_m: Matcher,
    sat_req: bool,
    sat_result: Option<f64>,

    // ---- EQ page tap + scope
    pub eq_act: [bool; 2],
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
    /// Peak limiter gain reduction (dB, positive) since the shell last took it.
    pub lim_gr_pk: f64,
    pub vz_blk: f64,
    pub vz_lv: f64,
    /// Main-signal level at the compressor input (decaying peak), for the sidechain viewport.
    pub mn_blk: f64,
    pub mn_lv: f64,
    /// mirror of the saturator's band peak (see `Saturator::blk`); reset with the other block meters
    pub sat_blk: f64,
    pub sat_vz: f64,
    /// Post-saturator peak since the shell last took it (no decay, so a clip between UI frames is never missed).
    pub sat_pk: f64,
    pub fms: f64,
}

impl Engine {
    pub fn new(sr: f64) -> Self {
        let mut e = Self {
            sr,
            sr_prev: 0.0,
            p: Params::default(),
            dirty: true,
            smoothing: true,
            snap: true,
            first_set: true,
            sat: Saturator::new(),
            comp: Compressor::new(),
            key: KeyInput::new(),
            eq: [EqSection::default(), EqSection::default()],
            lim: Limiter::new(sr),
            mon_d: StereoDelay::new(),
            mon_s: StereoDelay::new(),
            dw: StereoDelay::new(),
            lk: StereoDelay::new(),
            la_s: 0,
            sm_c: 0.0,
            sc_avail: false,
            sc_solo: false,
            sc_s: 0.0,
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
            out_m: Matcher::default(),
            match_trim: 0.0,
            match_req: false,
            sat_m: Matcher::default(),
            sat_req: false,
            sat_result: None,
            eq_act: [false; 2],
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
            lim_gr_pk: 0.0,
            vz_blk: 0.0,
            vz_lv: 0.0,
            mn_blk: 0.0,
            mn_lv: 0.0,
            sat_blk: 0.0,
            sat_vz: 0.0,
            sat_pk: 0.0,
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
            self.sat.set_out_db(p.sat_out);
            self.lim.snap_on(p.lim_on);
            self.snap = true;
            let sm = std::mem::replace(&mut self.smoothing, false); // shaper table commits at once, no morph from shape 0
            self.derive();
            self.smoothing = sm;
            return;
        }
        if *p != self.p {
            if p.sat_out != self.p.sat_out {
                self.sat.set_out_db(p.sat_out);
            }
            self.p = *p;
            self.derive(); // applied immediately so latency() is valid before the first block
        }
    }

    /// PDC latency to report (oversampling + compressor lookahead + limiter lookahead, the last one always).
    pub fn latency(&self) -> u32 {
        (self.sat.latency() + self.la_s + self.lim.latency()) as u32
    }

    pub fn request_match(&mut self) {
        self.match_req = true;
    }
    pub fn request_sat_match(&mut self) {
        self.sat_req = true;
    }
    pub fn sat_matching(&self) -> bool {
        self.sat_m.active
    }
    pub fn out_matching(&self) -> bool {
        self.out_m.active
    }
    pub fn out_match_progress(&self) -> f64 {
        self.out_m.t
    }
    pub fn sat_match_progress(&self) -> f64 {
        self.sat_m.t
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
        self.comp.gain_computer()
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
        self.lim.set_sample_rate(sr);
        let snap = self.snap;

        self.sat.derive(&p.sat(), sr, snap, self.smoothing);
        self.la_s = ((p.lookahead_ms * 0.001 * sr + 0.5).floor().max(0.0) as usize).min(1900);
        self.comp.derive(&p.comp(), sr);
        self.key.derive(p.sc_gain, p.sc_hp, p.sc_lp, sr);
        self.mk_t = (p.makeup * LN10_20).exp();
        self.mix_t = p.mix * 0.01;
        self.match_on = p.gain_match;
        if self.match_on && !self.prev_match_on {
            self.out_m.start();
        }
        if !self.match_on {
            self.out_m.stop();
        }
        self.prev_match_on = self.match_on;
        self.rmsg_c = one_pole(sr, 0.5);
        self.og_c = one_pole(sr, 0.02);
        self.byp = p.bypass;
        self.byp_c = one_pole(sr, 0.01);
        self.sc_len = ((sr * ScopeRing::BUCKET_S).round() as u32).max(1);
        self.bg_c = one_pole(sr, 0.2);
        self.fms_c = one_pole(sr, 0.3);
        self.sm_c = one_pole(sr, 0.02);

        let en = [!p.eq_pre_bypass && !p.eq_module_bypass, !p.eq_post_bypass && !p.eq_module_bypass];
        for st in 0..2 {
            self.eq[st].derive(&p.eq[st], en[st], sr, snap);
            self.eq_act[st] = self.eq[st].act;
        }

        if snap {
            self.snap = false;
            self.mk = self.mk_t;
            self.mix = self.mix_t;
        }
    }

    /// Per-block housekeeping (JSFX @block). Call once per host buffer with its length.
    pub fn begin_block(&mut self, n: usize) {
        if self.match_req {
            self.out_m.start();
            self.match_req = false;
        }
        if self.dirty || self.sr != self.sr_prev {
            self.derive();
        }
        for s in 0..2 {
            self.eq[s].begin_block(n);
        }

        // ---- gain diff / match
        let dt = n as f64 / self.sr;
        self.gate_open = self.ims > 0.000001;
        if self.gate_open {
            self.diff_db = DB_POW * (self.oms.max(1e-12) / self.ims.max(1e-12)).ln();
            self.diff_disp = DB_POW * (self.pms.max(1e-12) / self.ims.max(1e-12)).ln();
            self.sat_db = DB_POW * (self.sat.ms.max(1e-12) / self.ims.max(1e-12)).ln();
        }
        if self.sat_req {
            let was = self.sat_m.active;
            self.sat_m.start();
            if was {
                self.sat_m.stop();
            }
            self.sat_req = false;
        }
        if self.sat_m.active && self.gate_open {
            if let Some(avg) = self.sat_m.step(self.sat_db, dt) {
                let v = (avg.clamp(-24.0, 12.0) * 10.0 + 0.5).floor() * 0.1;
                self.sat.set_out_db(v);
                self.sat_result = Some(v);
            }
        }
        self.sat.begin_block();
        if self.out_m.active && self.gate_open {
            if let Some(avg) = self.out_m.step(self.diff_db, dt) {
                self.match_trim = avg.clamp(-12.0, 12.0);
            }
        }
        let trim = if self.match_on { self.match_trim } else { 0.0 };
        self.og_t = ((self.p.out_gain + trim) * LN10_20).exp();
        self.gr_pk = self.gr_pk.max(self.gr_blk);
        self.lim_gr_pk = self.lim_gr_pk.max(-self.lim.gmin.max(1e-6).ln() * DB_AMP);
        self.lim.gmin = 1.0;
        self.gr_blk = 0.0;
        let vz_d = (-(n as f64) / (self.sr * 0.15)).exp();
        self.vz_lv = self.vz_blk.max(self.vz_lv * vz_d);
        self.vz_blk = 0.0;
        self.mn_lv = self.mn_blk.max(self.mn_lv * vz_d);
        self.mn_blk = 0.0;
        self.sat_vz = self.sat.blk.max(self.sat_vz * vz_d);
        self.sat_pk = self.sat_pk.max(self.sat.blk);
        self.sat.blk = 0.0;
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
        ms_step(&mut self.ims, in_l, in_r, self.rmsg_c);
        if self.smoothing {
            let c = self.sm_c;
            self.mk += (self.mk_t - self.mk) * c;
            self.mix += (self.mix_t - self.mix) * c;
        } else {
            self.mk = self.mk_t;
            self.mix = self.mix_t;
        }

        // input aligned with the wet path (sat latency + lookahead), for the parallel dry mix
        self.dw.advance();
        self.dw.put(inp);
        let lat = self.sat.latency();
        let ll = self.lim.latency();
        let dt = self.dw.tap(lat + self.la_s);
        // the untouched input again, a limiter latency later: lines up with the limited path for the bypass blend
        let db = self.dw.tap(lat + self.la_s + ll);

        // ------------------------------------------------ SAT (+ SAT OUT)
        let mut cc = self.sat.tick(inp, self.smoothing);
        self.sms = self.sat.ms;
        self.sat_blk = self.sat.blk;

        // ------------------------------------------------ EQ PRE
        if self.eq_tap_on && self.eq_vs == 0 {
            self.tap.push(((cc[0] + cc[1]) * 0.5) as f32);
        }
        cc = self.eq[0].tick(cc);

        // ------------------------------------------------ LOOKAHEAD
        self.lk.advance();
        self.lk.put(cc);
        let cd = if self.la_s > 0 { self.lk.tap(self.la_s) } else { cc };

        // ------------------------------------------------ SIDECHAIN (gain > HP > LP), aligned with `cc` (delayed by the sat latency)
        let sc_act = self.p.sc_on && self.sc_avail;
        let scv = self.key.tick(if sc_act { Some([sc_l, sc_r]) } else { None }, lat);
        self.mn_blk = self.mn_blk.max(cc[0].abs().max(cc[1].abs()));

        // ------------------------------------------------ COMP
        let co = self.comp.tick(cc, cd, if sc_act { Some(scv) } else { None });
        self.vz_blk = self.vz_blk.max(co.level);
        let (y, g) = (co.y, co.g);

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
        o = self.eq[1].tick(o);
        ms_step(&mut self.oms, o[0], o[1], self.rmsg_c);
        self.og += (self.og_t - self.og) * self.og_c;
        let pp = [o[0] * self.og, o[1] * self.og];
        ms_step(&mut self.pms, pp[0], pp[1], self.rmsg_c);
        // equal-loudness bypass
        self.byp_s += ((if self.byp { 1.0 } else { 0.0 }) - self.byp_s) * self.byp_c;
        if self.gate_open {
            self.bg_t = (self.pms / self.ims).sqrt().clamp(0.0631, 15.85);
        }
        self.bg += (self.bg_t - self.bg) * self.bg_c;
        // output limiter (processed path only: bypass, delta and solo are monitors / the untouched input)
        let lo = self.lim.tick(pp, self.p.lim_on);
        let mut out = [lo[0] + (db[0] * self.bg - lo[0]) * self.byp_s, lo[1] + (db[1] * self.bg - lo[1]) * self.byp_s];
        // delta monitor
        self.d_s += ((if self.delta_on { 1.0 } else { 0.0 }) - self.d_s) * self.byp_c;
        self.mon_d.advance();
        self.mon_d.put([cd[0] - y[0], cd[1] - y[1]]);
        if self.d_s > 0.0001 {
            let dd = self.mon_d.tap(ll);
            for c in 0..2 {
                out[c] += (dd[c] - out[c]) * self.d_s;
            }
        }
        // sidechain solo: what the detector hears (post gain/HP/LP), replaces the output
        self.sc_s += ((if self.sc_solo && sc_act { 1.0 } else { 0.0 }) - self.sc_s) * self.byp_c;
        self.mon_s.advance();
        self.mon_s.put(scv);
        if self.sc_s > 0.0001 {
            let sv = self.mon_s.tap(ll);
            for c in 0..2 {
                out[c] += (sv[c] - out[c]) * self.sc_s;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

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

    /// The limiter's constant delay at 48 kHz (always reported, limiter on or off).
    const LIM_LAT: usize = 32 + 72;

    #[test]
    fn neutral_settings_are_bit_transparent() {
        let mut e = Engine::new(SR);
        e.set_params(&neutral());
        let x = noise(4800, 1);
        let y = run(&mut e, &x, 256);
        let ll = LIM_LAT;
        for i in ll..x.len() {
            assert!((x[i - ll] - y[i]).abs() < 1e-12);
        }
        assert_eq!(e.latency() as usize, ll);
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
            let lat = lat + LIM_LAT;
            assert_eq!(e.latency() as usize, lat);
            let mut x = vec![0.0; 400];
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
        let mut x = vec![0.0; 400];
        x[5] = 1.0;
        let y = run(&mut e, &x, 64);
        let lat = 48 + LIM_LAT;
        assert_eq!(e.latency() as usize, lat);
        assert!((y[5 + lat] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn limiter_latency_is_reported_on_or_off() {
        let mut p = neutral();
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let off = e.latency();
        p.lim_on = true;
        e.set_params(&p);
        assert_eq!(e.latency(), off);
        assert_eq!(off as usize, LIM_LAT);
    }

    #[test]
    fn limiter_holds_hot_output_at_the_ceiling_and_reports_reduction() {
        let mut p = neutral();
        p.lim_on = true;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let hot = sine(24000, 440.0, 2.0);
        let y = run(&mut e, &hot, 256);
        let pk = y[12000..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        assert!(pk <= 0.9773 && pk > 0.95, "peak {pk}");
        assert!(e.lim_gr_pk > 5.0, "gr {}", e.lim_gr_pk);
        // off: same engine settings pass the signal through untouched (apart from the delay)
        p.lim_on = false;
        let mut e = Engine::new(SR);
        e.set_params(&p);
        let y = run(&mut e, &hot, 256);
        assert!((y[12000..].iter().fold(0.0f64, |m, v| m.max(v.abs())) - 2.0).abs() < 0.01);
        assert_eq!(e.lim_gr_pk, 0.0);
    }

    #[test]
    fn limiter_does_not_touch_bypass_or_misalign_it() {
        // bypass returns the (equal-loudness) dry input, delayed like the processed path, unlimited
        let mut p = neutral();
        p.lim_on = true;
        p.bypass = true;
        let mut e = Engine::new(SR);
        e.smoothing = false;
        e.set_params(&p);
        let hot = sine(24000, 440.0, 2.0);
        let y = run(&mut e, &hot, 256);
        let lat = e.latency() as usize;
        let x = &hot[12000 - lat..24000 - lat];
        let o = &y[12000..24000];
        let (px, po) = (x.iter().fold(0.0f64, |m, v| m.max(v.abs())), o.iter().fold(0.0f64, |m, v| m.max(v.abs())));
        assert!(po > 1.5, "bypass output was limited: {po}");
        assert!((po / px - 1.0).abs() < 0.05, "gain {}", po / px);
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
        let g = GainComputer::from_params(&p.comp());
        assert_eq!(g.gr(-30.0), 0.0);
        assert!((g.gr(-8.0) - 12.0 * 0.75).abs() < 1e-12);
        p.knee = 6.0;
        let g = GainComputer::from_params(&p.comp());
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
