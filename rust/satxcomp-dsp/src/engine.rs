//! SatXComp signal path: LR4 3-way split -> per band [saturator -> compressor -> comp mix/gain] -> sum (solo / delta) ->
//! 5-band EQ (+ tilt, mix, gain) -> dry/wet mix -> out gain -> equal-loudness bypass.
//! Meters and match logic follow `satcomp_dsp::Engine`.

use crate::params::{eq_kind, EqParams, XParams};
use crate::strip::Strip;
use crate::xover::Crossover3;
use satcomp_dsp::biquad::eq_coef;
use satcomp_dsp::delay::StereoDelay;
use satcomp_dsp::eq::{EqBank, EqSection};
use satcomp_dsp::keyinput::KeyInput;
use satcomp_dsp::level::{ms_step, one_pole, Matcher, DB_AMP, DB_POW, LN10_20};
use satcomp_dsp::Limiter;
use satcomp_dsp::{ScopeRing, TapRing, DENORM};
use std::sync::Arc;

const EQ_FILTERS: usize = 7; // 5 bands + tilt low shelf + tilt high shelf

/// Sets the bank targets for the 5 bands and the tilt pair. Returns whether any filter does something.
fn eq_setup(bank: &mut EqBank<EQ_FILTERS>, p: &EqParams, sr: f64) -> bool {
    let mut any = false;
    for b in 0..5 {
        let bp = &p.bands[b];
        let coef = eq_coef(eq_kind(b, bp.ty), bp.freq.clamp(20.0, 20000.0), bp.gain.clamp(-18.0, 18.0), bp.q.clamp(0.1, 10.0), sr);
        bank.target(b, Some(coef), bp.on);
        any |= bp.on;
    }
    let t = p.tilt.clamp(-12.0, 12.0);
    let pv = if p.pivot < 19.5 { 600.0 } else { p.pivot.min(20000.0) };
    let on = t != 0.0;
    bank.target(5, if on { Some(eq_coef(2, pv, -t * 0.5, 0.4, sr)) } else { None }, on);
    bank.target(6, if on { Some(eq_coef(3, pv, t * 0.5, 0.4, sr)) } else { None }, on);
    any || on
}

pub struct XEngine {
    sr: f64,
    sr_prev: f64,
    p: XParams,
    dirty: bool,
    pub smoothing: bool,
    snap: bool,
    first_set: bool,

    xo: Crossover3,
    /// the external key: conditioned globally, then split by its own copy of the crossover into one slice per band
    key: KeyInput,
    key_xo: Crossover3,
    /// the host gave a sidechain bus this block
    pub sc_avail: bool,
    /// monitor the conditioned key instead of the output (UI toggle)
    pub sc_solo: bool,
    sc_s: f64,
    pub strips: [Strip; 3],
    eq: EqSection<EQ_FILTERS>,
    eq_mix: f64,
    eq_mix_t: f64,
    dw: StereoDelay<4096>,
    lim: Limiter,
    /// conditioned key delayed by the limiter latency (sidechain solo lines up with the limited path)
    mon_s: StereoDelay<256>,
    la_s: usize,
    any_solo: bool,
    d_s: [f64; 3],
    solo_s: [f64; 3],
    sm_c: f64,

    og: f64,
    og_t: f64,
    og_c: f64,
    mix: f64,
    mix_t: f64,
    bg: f64,
    bg_t: f64,
    bg_c: f64,
    byp_s: f64,
    byp_c: f64,
    rmsg_c: f64,
    fms_c: f64,
    /// monitor what all compressors removed (UI toggle, not a stored parameter)
    pub delta_on: bool,
    gd_s: f64,

    pub ims: f64,
    pub oms: f64,
    pub pms: f64,
    /// mean squares into / out of the EQ stage (its dry/wet mix and gain included)
    eis: f64,
    eos: f64,
    /// level change across the EQ stage, dB (for the EQ IN>OUT readout)
    pub eq_diff: f64,
    pub diff_db: f64,
    pub diff_disp: f64,
    gate_open: bool,
    match_on: bool,
    prev_match_on: bool,
    out_m: Matcher,
    pub match_trim: f64,
    match_req: bool,

    /// spectrum taps (mono): output signal, and the saturated signal before the compressors ("dry")
    pub tap_out: Arc<TapRing>,
    pub tap_dry: Arc<TapRing>,
    pub tap_on: bool,
    pub scope: Arc<ScopeRing>,
    sc_o: f64,
    sc_p: f64,
    sc_n: u32,
    sc_len: u32,

    pub opk: [f64; 2],
    pub clip_f: bool,
    /// per band: gain reduction (dB) of the current block / peak since the owner last cleared it
    pub gr_blk: [f64; 3],
    pub gr_pk: [f64; 3],
    /// decaying peak of each band's saturator input (for the live marker on the transfer plot)
    pub sat_vz: [f64; 3],
    /// per band peak of the saturator output, accumulated until the plug-in wrapper takes it (like `gr_pk`)
    pub sat_out: [f64; 3],
    pub fms: f64,
    /// Peak limiter gain reduction (dB, positive) since the shell last took it.
    pub lim_gr_pk: f64,
}

impl XEngine {
    pub fn new(sr: f64) -> Self {
        let mut e = Self {
            sr,
            sr_prev: 0.0,
            p: XParams::default(),
            dirty: true,
            smoothing: true,
            snap: true,
            first_set: true,
            xo: Crossover3::new(),
            key: KeyInput::new(),
            key_xo: Crossover3::new(),
            sc_avail: false,
            sc_solo: false,
            sc_s: 0.0,
            strips: [Strip::new(), Strip::new(), Strip::new()],
            eq: EqSection::default(),
            eq_mix: 1.0,
            eq_mix_t: 1.0,
            dw: StereoDelay::new(),
            lim: Limiter::new(sr),
            mon_s: StereoDelay::new(),
            la_s: 0,
            any_solo: false,
            d_s: [0.0; 3],
            solo_s: [1.0; 3],
            sm_c: 0.0,
            og: 1.0,
            og_t: 1.0,
            og_c: 0.0,
            mix: 1.0,
            mix_t: 1.0,
            bg: 1.0,
            bg_t: 1.0,
            bg_c: 0.0,
            byp_s: 0.0,
            byp_c: 0.0,
            rmsg_c: 0.0,
            fms_c: 0.0,
            delta_on: false,
            gd_s: 0.0,
            ims: 1e-9,
            oms: 1e-9,
            pms: 1e-9,
            eis: 1e-9,
            eos: 1e-9,
            eq_diff: 0.0,
            diff_db: 0.0,
            diff_disp: 0.0,
            gate_open: false,
            match_on: false,
            prev_match_on: false,
            out_m: Matcher::default(),
            match_trim: 0.0,
            match_req: false,
            tap_out: Arc::new(TapRing::new()),
            tap_dry: Arc::new(TapRing::new()),
            tap_on: false,
            scope: Arc::new(ScopeRing::new()),
            sc_o: 0.0,
            sc_p: 0.0,
            sc_n: 0,
            sc_len: 96,
            opk: [0.0; 2],
            clip_f: false,
            gr_blk: [0.0; 3],
            gr_pk: [0.0; 3],
            sat_vz: [0.0; 3],
            sat_out: [0.0; 3],
            fms: 1e-9,
            lim_gr_pk: 0.0,
        };
        e.derive();
        e
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sr = sr;
        self.dirty = true;
    }

    pub fn set_params(&mut self, p: &XParams) {
        if self.first_set {
            // a fresh engine starts at the host's settings instead of gliding from defaults (host rebuilds on every enable)
            self.first_set = false;
            self.p = *p;
            self.lim.snap_on(p.lim_on);
            self.snap = true;
            let sm = std::mem::replace(&mut self.smoothing, false);
            self.derive();
            self.smoothing = sm;
            return;
        }
        if *p != self.p {
            self.p = *p;
            self.derive();
        }
    }

    /// PDC latency to report (saturator oversampling + lookahead; all bands are aligned).
    pub fn latency(&self) -> u32 {
        (self.strips[0].latency() + self.lim.latency()) as u32
    }

    pub fn request_match(&mut self) {
        self.match_req = true;
    }
    pub fn out_matching(&self) -> bool {
        self.out_m.active
    }
    pub fn out_match_progress(&self) -> f64 {
        self.out_m.t
    }
    pub fn bg_gain(&self) -> f64 {
        self.bg
    }
    pub fn gate_open(&self) -> bool {
        self.gate_open
    }
    pub fn sample_rate(&self) -> f64 {
        self.sr
    }

    fn derive(&mut self) {
        let p = self.p;
        let sr = self.sr;
        self.sr_prev = sr;
        self.dirty = false;
        self.lim.set_sample_rate(sr);
        self.sc_len = ((sr * ScopeRing::BUCKET_S).round() as u32).max(1);
        let snap = self.snap;
        self.la_s = ((p.lookahead_ms * 0.001 * sr + 0.5).floor().max(0.0) as usize).min(1900);
        self.xo.set(p.x1, p.x2, p.x1_slope.clamp(0, 3) as usize, p.x2_slope.clamp(0, 3) as usize, sr, snap);
        self.key_xo.set(p.x1, p.x2, p.x1_slope.clamp(0, 3) as usize, p.x2_slope.clamp(0, 3) as usize, sr, snap);
        self.key.derive(p.sc_gain, p.sc_hp, p.sc_lp, sr);
        for b in 0..3 {
            self.strips[b].derive(&p.bands[b], p.os, self.la_s, p.key_mix, sr, snap, self.smoothing);
        }
        self.any_solo = p.bands.iter().any(|b| b.solo);
        let any = eq_setup(&mut self.eq.bank, &p.eq, sr);
        self.eq.derive_gain(any, p.eq.gain, !p.eq.bypass, sr, snap);
        self.eq_mix_t = p.eq.mix * 0.01;
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
        self.byp_c = one_pole(sr, 0.01);
        self.bg_c = one_pole(sr, 0.2);
        self.fms_c = one_pole(sr, 0.3);
        self.sm_c = one_pole(sr, 0.02);
        if snap {
            self.snap = false;
            self.eq_mix = self.eq_mix_t;
            self.mix = self.mix_t;
        }
    }

    pub fn begin_block(&mut self, n: usize) {
        if self.match_req {
            self.out_m.start();
            self.match_req = false;
        }
        if self.dirty || self.sr != self.sr_prev {
            self.derive();
        }
        self.xo.begin_block(n);
        self.key_xo.begin_block(n);
        for s in self.strips.iter_mut() {
            s.begin_block();
        }
        self.eq.begin_block(n);

        let dt = n as f64 / self.sr;
        self.lim_gr_pk = self.lim_gr_pk.max(-self.lim.gmin.max(1e-6).ln() * DB_AMP);
        self.lim.gmin = 1.0;
        self.gate_open = self.ims > 0.000001;
        if self.gate_open {
            self.diff_db = DB_POW * (self.oms.max(1e-12) / self.ims.max(1e-12)).ln();
            self.diff_disp = DB_POW * (self.pms.max(1e-12) / self.ims.max(1e-12)).ln();
        }
        if self.eis > 0.000001 {
            self.eq_diff = DB_POW * (self.eos.max(1e-12) / self.eis.max(1e-12)).ln();
        }
        if self.out_m.active && self.gate_open {
            if let Some(avg) = self.out_m.step(self.diff_db, dt) {
                self.match_trim = avg.clamp(-12.0, 12.0);
            }
        }
        let trim = if self.match_on { self.match_trim } else { 0.0 };
        self.og_t = ((self.p.out_gain + trim) * LN10_20).exp();
        for b in 0..3 {
            self.gr_blk[b] = self.strips[b].gr_blk;
            self.gr_pk[b] = self.gr_pk[b].max(self.gr_blk[b]);
            self.strips[b].gr_blk = 0.0;
        }
        let vz_d = (-(n as f64) / (self.sr * 0.15)).exp();
        for b in 0..3 {
            self.sat_vz[b] = self.strips[b].sat.blk.max(self.sat_vz[b] * vz_d);
            self.strips[b].sat.blk = 0.0;
            self.sat_out[b] = self.sat_out[b].max(self.strips[b].sat_pk);
            self.strips[b].sat_pk = 0.0;
        }
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.process_sc(left, right, None);
    }

    /// As `process`, with the host's sidechain bus (`None` = no bus: the compressors key off their own bands).
    pub fn process_sc(&mut self, left: &mut [f32], right: &mut [f32], sc: Option<(&[f32], &[f32])>) {
        let mut n = left.len().min(right.len());
        if let Some((a, b)) = sc {
            n = n.min(a.len()).min(b.len());
        }
        self.sc_avail = sc.is_some();
        self.begin_block(n);
        for i in 0..n {
            let (kl, kr) = match sc {
                Some((a, b)) => (a[i] as f64, b[i] as f64),
                None => (0.0, 0.0),
            };
            let (l, r) = self.tick_sc(left[i] as f64, right[i] as f64, kl, kr);
            left[i] = l as f32;
            right[i] = r as f32;
        }
    }

    #[inline]
    pub fn tick(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        self.tick_sc(in_l, in_r, 0.0, 0.0)
    }

    /// One sample with a sidechain sample pair (ignored unless the sidechain is on and the host gave a bus).
    #[inline]
    pub fn tick_sc(&mut self, in_l: f64, in_r: f64, sc_l: f64, sc_r: f64) -> (f64, f64) {
        let inp = [in_l, in_r];
        ms_step(&mut self.ims, in_l, in_r, self.rmsg_c);
        if self.smoothing {
            self.eq_mix += (self.eq_mix_t - self.eq_mix) * self.sm_c;
            self.mix += (self.mix_t - self.mix) * self.sm_c;
        } else {
            self.eq_mix = self.eq_mix_t;
            self.mix = self.mix_t;
        }
        self.dw.advance();
        self.dw.put(inp);
        let dt = self.dw.tap(self.strips[0].latency());
        // the untouched input a limiter latency later: lines up with the limited path for the bypass blend
        let ll = self.lim.latency();
        let db = self.dw.tap(self.strips[0].latency() + ll);

        // ---- split, process each band, sum with solo / delta weighting
        let bands = self.xo.tick(inp);
        let sgn = self.xo.signs();
        // key: gain > HP > LP, delayed to line up with the saturator outputs, then split into bands by its own crossover
        let sc_act = self.p.sc_on && self.sc_avail;
        let scv = self.key.tick(if sc_act { Some([sc_l, sc_r]) } else { None }, self.strips[0].sat.latency());
        let kb = if sc_act { Some(self.key_xo.tick(scv)) } else { None };
        let mut sum = [0.0f64; 2];
        let mut pre = [0.0f64; 2];
        let mut gmax = 0.0f64;
        let gdelta = if self.delta_on { 1.0 } else { 0.0 };
        self.gd_s += (gdelta - self.gd_s) * self.byp_c;
        for b in 0..3 {
            let o = self.strips[b].tick(bands[b], self.smoothing, kb.map(|k| k[b]));
            if self.p.bands[b].on {
                gmax = gmax.max(o.g);
            }
            let bp = &self.p.bands[b];
            let dtarget: f64 = if bp.delta { 1.0 } else { 0.0 };
            let d = self.d_s[b] + (dtarget.max(gdelta) - self.d_s[b]) * self.byp_c;
            self.d_s[b] = d;
            let stgt = if self.any_solo { if bp.solo { 1.0 } else { 0.0 } } else { 1.0 };
            let sg = self.solo_s[b] + (stgt - self.solo_s[b]) * self.byp_c;
            self.solo_s[b] = sg;
            for c in 0..2 {
                sum[c] += (o.y[c] + (o.removed[c] - o.y[c]) * d) * sg * sgn[b];
                pre[c] += o.pre[c] * sgn[b];
            }
        }

        // ---- EQ (post-sum) with its own dry/wet
        let ew = self.eq.tick(sum);
        let wet = [sum[0] + (ew[0] - sum[0]) * self.eq_mix, sum[1] + (ew[1] - sum[1]) * self.eq_mix];
        ms_step(&mut self.eis, sum[0], sum[1], self.rmsg_c);
        ms_step(&mut self.eos, wet[0], wet[1], self.rmsg_c);

        // ---- OUT
        let mut o = [0.0f64; 2];
        for c in 0..2 {
            o[c] = dt[c] + (wet[c] - dt[c]) * self.mix;
        }
        ms_step(&mut self.oms, o[0], o[1], self.rmsg_c);
        self.og += (self.og_t - self.og) * self.og_c;
        let pp = [o[0] * self.og, o[1] * self.og];
        ms_step(&mut self.pms, pp[0], pp[1], self.rmsg_c);
        self.byp_s += ((if self.p.bypass { 1.0 } else { 0.0 }) - self.byp_s) * self.byp_c;
        if self.gate_open {
            self.bg_t = (self.pms / self.ims).sqrt().clamp(0.0631, 15.85);
        }
        self.bg += (self.bg_t - self.bg) * self.bg_c;
        // output limiter (processed path only: bypass and the key monitor are the untouched input / a monitor)
        let lo = self.lim.tick(pp, self.p.lim_on);
        let mut out = [lo[0] + (db[0] * self.bg - lo[0]) * self.byp_s, lo[1] + (db[1] * self.bg - lo[1]) * self.byp_s];
        // solo: hear the conditioned key as the detectors get it
        self.sc_s += ((if self.sc_solo && sc_act { 1.0 } else { 0.0 }) - self.sc_s) * self.byp_c;
        self.mon_s.advance();
        self.mon_s.put(scv);
        if self.sc_s > 0.0001 {
            let sv = self.mon_s.tap(ll);
            for c in 0..2 {
                out[c] += (sv[c] - out[c]) * self.sc_s;
            }
        }

        if self.tap_on {
            self.tap_out.push(((out[0] + out[1]) * 0.5) as f32);
            self.tap_dry.push(((pre[0] + pre[1]) * 0.5) as f32);
        }
        self.opk[0] = self.opk[0].max(out[0].abs());
        self.opk[1] = self.opk[1].max(out[1].abs());
        if out[0].abs() > 0.9999 || out[1].abs() > 0.9999 {
            self.clip_f = true;
        }
        // scope bucket: output peak, and the same with the loudest band's gain reduction added back
        let ao = out[0].abs().max(out[1].abs());
        self.sc_o = self.sc_o.max(ao);
        self.sc_p = self.sc_p.max(ao * (gmax * LN10_20).exp());
        self.sc_n += 1;
        if self.sc_n >= self.sc_len {
            self.scope.push(self.sc_o as f32, self.sc_p as f32);
            self.sc_o = 0.0;
            self.sc_p = 0.0;
            self.sc_n = 0;
        }
        self.fms += ((out[0] * out[0] + out[1] * out[1]) * 0.5 + DENORM - self.fms) * self.fms_c;
        (out[0], out[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;
    /// the limiter's constant delay at 48 kHz (always reported)
    const LIM_LAT: u32 = 32 + 72;

    fn noise(n: usize, seed: u64) -> Vec<f64> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (((s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0) * 0.3
            })
            .collect()
    }

    /// a plug-in state where nothing alters the signal (no saturation, no compression, EQ flat)
    fn neutral() -> XParams {
        let mut p = XParams::default();
        for b in p.bands.iter_mut() {
            b.drive = 0.0;
            b.ratio = 1.0;
            b.thresh = 0.0;
        }
        p.lim_on = false;
        p
    }

    fn run(e: &mut XEngine, x: &[f64]) -> Vec<f64> {
        let (mut l, mut r): (Vec<f32>, Vec<f32>) = (x.iter().map(|v| *v as f32).collect(), x.iter().map(|v| *v as f32).collect());
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            e.process(cl, cr);
        }
        l.iter().map(|v| *v as f64).collect()
    }

    fn rms(x: &[f64]) -> f64 {
        (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
    }

    #[test]
    fn neutral_engine_is_flat_in_level() {
        let x = noise(48000, 1);
        let mut e = XEngine::new(SR);
        e.set_params(&neutral());
        let y = run(&mut e, &x);
        let (a, b) = (rms(&x[4000..]), rms(&y[4000..]));
        assert!((20.0 * (b / a).log10()).abs() < 0.1, "{a} vs {b}");
    }

    #[test]
    fn neutral_engine_stays_flat_at_every_crossover_slope() {
        let x = noise(48000, 5);
        for (s1, s2) in [(0, 0), (0, 3), (2, 1), (3, 3)] {
            let mut p = neutral();
            p.x1_slope = s1;
            p.x2_slope = s2;
            let mut e = XEngine::new(SR);
            e.set_params(&p);
            let y = run(&mut e, &x);
            let g = 20.0 * (rms(&y[6000..]) / rms(&x[6000..])).log10();
            assert!(g.abs() < 0.1, "slopes {s1}/{s2}: {g} dB");
        }
    }

    #[test]
    fn switching_crossover_slope_does_not_click() {
        // a steady tone through the whole engine; the slopes change mid-stream and the output must stay as smooth as the tone itself
        let amp = 0.5;
        let w = 2.0 * PI * 330.0 / SR;
        let tone: Vec<f64> = (0..48000).map(|i| amp * (w * i as f64).sin()).collect();
        for (from, to) in [((1, 1), (2, 1)), ((1, 1), (0, 1)), ((0, 0), (3, 3)), ((3, 3), (1, 1))] {
            let mut p = neutral();
            p.x1 = 200.0;
            p.x2 = 2500.0;
            p.x1_slope = from.0;
            p.x2_slope = from.1;
            let mut e = XEngine::new(SR);
            e.set_params(&p);
            let mut y = run(&mut e, &tone[..24000]);
            p.x1_slope = to.0;
            p.x2_slope = to.1;
            e.set_params(&p);
            y.extend(run(&mut e, &tone[24000..]));
            let worst = y.windows(2).skip(8000).map(|d| (d[1] - d[0]).abs()).fold(0.0, f64::max);
            // a pure sine of this amplitude moves at most amp * w per sample
            assert!(worst < amp * w * 1.5, "slope {from:?} -> {to:?} clicked: step {worst:.4} (tone {:.4})", amp * w);
        }
    }

    #[test]
    fn a_band_only_affects_its_own_range() {
        // compress only the mid band hard: a low tone is untouched, a mid tone is reduced
        let tone = |f: f64| (0..48000).map(|i| 0.5 * (2.0 * PI * f * i as f64 / SR).sin()).collect::<Vec<f64>>();
        let mut p = neutral();
        p.bands[1].thresh = -30.0;
        p.bands[1].ratio = 20.0;
        let (lo, mid) = (tone(60.0), tone(900.0));
        let (mut e1, mut e2) = (XEngine::new(SR), XEngine::new(SR));
        e1.set_params(&p);
        e2.set_params(&p);
        let (ylo, ymid) = (run(&mut e1, &lo), run(&mut e2, &mid));
        let dlo = 20.0 * (rms(&ylo[8000..]) / rms(&lo[8000..])).log10();
        let dmid = 20.0 * (rms(&ymid[8000..]) / rms(&mid[8000..])).log10();
        assert!(dlo.abs() < 0.3, "low changed {dlo}");
        assert!(dmid < -8.0, "mid not compressed {dmid}");
    }

    #[test]
    fn solo_isolates_a_band_and_off_band_passes_through() {
        let x = noise(48000, 2);
        let mut p = neutral();
        p.bands[0].solo = true;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        let y = run(&mut e, &x);
        assert!(rms(&y[6000..]) < rms(&x[6000..]) * 0.6, "solo low should remove most of a white noise");
        let mut p = neutral();
        p.bands[1].on = false;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        let y2 = run(&mut e, &x);
        assert!((20.0 * (rms(&y2[6000..]) / rms(&x[6000..])).log10()).abs() < 0.1);
    }

    #[test]
    fn eq_band_boosts_and_latency_is_reported() {
        let tone: Vec<f64> = (0..48000).map(|i| 0.2 * (2.0 * PI * 5000.0 * i as f64 / SR).sin()).collect();
        let mut p = neutral();
        p.eq.bands[3].on = true;
        p.eq.bands[3].freq = 5000.0;
        p.eq.bands[3].gain = 6.0;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        let y = run(&mut e, &tone);
        let g = 20.0 * (rms(&y[8000..]) / rms(&tone[8000..])).log10();
        assert!((g - 6.0).abs() < 0.3, "{g}");
        p.os = 2;
        p.lookahead_ms = 2.0;
        e.set_params(&p);
        assert_eq!(e.latency(), 28 + (2.0f64 * 0.001 * SR + 0.5).floor() as u32 + LIM_LAT);
    }

    #[test]
    fn limiter_latency_is_reported_on_or_off_and_the_limiter_holds_the_ceiling() {
        let mut e = XEngine::new(SR);
        let mut p = neutral();
        e.set_params(&p);
        assert_eq!(e.latency(), LIM_LAT);
        p.lim_on = true;
        e.set_params(&p);
        assert_eq!(e.latency(), LIM_LAT);
        // +18 dB of SAT GAIN on a hot tone: the output stays at the -0.2 dBTP ceiling and the reduction is reported
        for b in p.bands.iter_mut() {
            b.sat_gain = 12.0;
        }
        e.set_params(&p);
        let t: Vec<f64> = (0..48000).map(|i| 0.8 * (2.0 * PI * 1000.0 * i as f64 / SR).sin()).collect();
        let y = run(&mut e, &t);
        let pk = y[24000..].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        assert!(pk <= 0.9777 && pk > 0.9, "peak {pk}");
        assert!(e.lim_gr_pk > 5.0, "gr {}", e.lim_gr_pk);
        p.lim_on = false;
        e.set_params(&p);
        let y = run(&mut e, &t);
        assert!(y[24000..].iter().fold(0.0f64, |m, v| m.max(v.abs())) > 2.0, "off must not limit");
    }

    #[test]
    fn scope_buckets_show_output_and_the_reduction_added_back() {
        let mut p = neutral();
        for b in p.bands.iter_mut() {
            b.thresh = -20.0;
            b.ratio = 8.0;
        }
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        let t: Vec<f64> = (0..48000).map(|i| 0.8 * (2.0 * PI * 200.0 * i as f64 / SR).sin()).collect();
        run(&mut e, &t);
        let n = e.scope.head();
        assert!((n as f64 - 48000.0 / (SR * ScopeRing::BUCKET_S)).abs() < 3.0, "buckets {n}");
        let (o, pre) = e.scope.get(n - 20);
        assert!(o > 0.05 && pre > o * 1.5, "out {o}, with reduction {pre}");
    }

    #[test]
    fn sat_gain_changes_the_level_after_the_first_block() {
        let x = noise(48000, 4);
        let mut e = XEngine::new(SR);
        e.set_params(&neutral());
        let y0 = run(&mut e, &x);
        let mut p = neutral();
        for b in p.bands.iter_mut() {
            b.sat_gain = 6.0;
        }
        e.set_params(&p);
        let y1 = run(&mut e, &x);
        let g = 20.0 * (rms(&y1[8000..]) / rms(&y0[8000..])).log10();
        assert!((g - 6.0).abs() < 0.5, "sat gain +6 dB moved the level by {g}");
    }

    fn tone_db(f: f64, p: &XParams, warm: Option<&XParams>) -> f64 {
        let t: Vec<f64> = (0..48000).map(|i| 0.2 * (2.0 * PI * f * i as f64 / SR).sin()).collect();
        let mut e = XEngine::new(SR);
        if let Some(w) = warm {
            e.set_params(w);
            run(&mut e, &t);
        }
        e.set_params(p);
        let y = run(&mut e, &t);
        20.0 * (rms(&y[8000..]) / rms(&t[8000..])).log10()
    }

    #[test]
    fn eq_gain_mix_and_pivot_act() {
        // EQ GAIN on its own (no bands on)
        let mut p = neutral();
        p.eq.gain = 6.0;
        let g = tone_db(1000.0, &p, None);
        assert!((g - 6.0).abs() < 0.3, "eq gain {g}");
        // ... also when changed after the engine has been running
        let g = tone_db(1000.0, &p, Some(&neutral()));
        assert!((g - 6.0).abs() < 0.3, "eq gain after warm-up {g}");
        // EQ MIX 0 takes the whole EQ out
        let mut p = neutral();
        p.eq.bands[3].on = true;
        p.eq.bands[3].freq = 5000.0;
        p.eq.bands[3].gain = 9.0;
        let full = tone_db(5000.0, &p, None);
        p.eq.mix = 0.0;
        let off = tone_db(5000.0, &p, None);
        assert!(full > 8.0 && off.abs() < 0.3, "eq mix: full {full}, mix 0 {off}");
        // pivot moves where the tilt crosses 0 dB
        let mut p = neutral();
        p.eq.tilt = 6.0;
        p.eq.pivot = 200.0;
        let a = tone_db(200.0, &p, None);
        p.eq.pivot = 4000.0;
        let b = tone_db(200.0, &p, None);
        assert!((a - b).abs() > 2.0, "pivot has no effect: {a} vs {b}");
    }

    #[test]
    fn eq_diff_reads_the_level_change_across_the_eq_stage_only() {
        let tone: Vec<f64> = (0..96000).map(|i| 0.2 * (2.0 * PI * 5000.0 * i as f64 / SR).sin()).collect();
        let mut p = neutral();
        p.eq.bands[3].on = true;
        p.eq.bands[3].freq = 5000.0;
        p.eq.bands[3].gain = 6.0;
        p.out_gain = -9.0; // output gain is outside the EQ stage and must not show up
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        run(&mut e, &tone);
        assert!((e.eq_diff - 6.0).abs() < 0.5, "eq diff {}", e.eq_diff);
        p.eq.mix = 0.0;
        e.set_params(&p);
        run(&mut e, &tone);
        assert!(e.eq_diff.abs() < 0.5, "eq mix 0 should read 0 dB, got {}", e.eq_diff);
    }

    #[test]
    fn sat_out_is_the_post_saturator_peak_per_band() {
        let tone: Vec<f64> = (0..48000).map(|i| 0.5 * (2.0 * PI * 50.0 * i as f64 / SR).sin()).collect();
        let mut e = XEngine::new(SR);
        e.set_params(&neutral());
        run(&mut e, &tone);
        assert!((e.sat_out[0] - 0.5).abs() < 0.1, "low band peak: {}", e.sat_out[0]);
        assert!(e.sat_out[1] < 0.1 && e.sat_out[2] < 0.05, "other bands only see leakage: {:?}", e.sat_out);
        // hot sat gain pushes the post-sat peak over full scale (what the clip indicator watches)
        let mut p = neutral();
        p.bands[0].sat_gain = 12.0;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        run(&mut e, &tone);
        assert!(e.sat_out[0] > 1.5, "+12 dB sat gain on a 0.5 peak: {}", e.sat_out[0]);
    }

    #[test]
    fn sat_vz_follows_each_bands_saturator_input() {
        let tone: Vec<f64> = (0..48000).map(|i| 0.4 * (2.0 * PI * 50.0 * i as f64 / SR).sin()).collect();
        let mut e = XEngine::new(SR);
        let mut p = XParams::default();
        for b in p.bands.iter_mut() {
            b.drive = 25.0; // the saturator only measures while it is active
        }
        e.set_params(&p);
        run(&mut e, &tone);
        assert!((e.sat_vz[0] - 0.4).abs() < 0.08, "low band sees the 50 Hz tone: {}", e.sat_vz[0]);
        assert!(e.sat_vz[1] < 0.02 && e.sat_vz[2] < 0.02, "other bands stay quiet: {:?}", e.sat_vz);
    }

    fn sine(n: usize, f: f64, amp: f64) -> Vec<f64> {
        (0..n).map(|i| amp * (2.0 * PI * f * i as f64 / SR).sin()).collect()
    }

    fn run_sc(e: &mut XEngine, x: &[f64], key: Option<&[f64]>) -> Vec<f64> {
        let mut l: Vec<f32> = x.iter().map(|v| *v as f32).collect();
        let mut r = l.clone();
        let kf: Option<Vec<f32>> = key.map(|k| k.iter().map(|v| *v as f32).collect());
        for (i, (cl, cr)) in l.chunks_mut(256).zip(r.chunks_mut(256)).enumerate() {
            match &kf {
                Some(k) => {
                    let ks = &k[i * 256..(i * 256 + cl.len()).min(k.len())];
                    e.process_sc(cl, cr, Some((ks, ks)));
                }
                None => e.process_sc(cl, cr, None),
            }
        }
        l.iter().map(|v| *v as f64).collect()
    }

    /// every band compresses hard at a low threshold, keyed (not by its own signal) with KEY MIX 100 %
    fn sc_params() -> XParams {
        let mut p = neutral();
        for b in p.bands.iter_mut() {
            b.thresh = -30.0;
            b.ratio = 20.0;
            b.attack = 0.5;
            b.knee = 0.0;
        }
        p.sc_on = true;
        p.key_mix = 100.0;
        p
    }

    fn gain_db(y: &[f64], x: &[f64]) -> f64 {
        20.0 * (rms(&y[12000..]) / rms(&x[12000..])).log10()
    }

    #[test]
    fn a_low_key_only_ducks_the_low_band() {
        let key = sine(48000, 60.0, 0.9);
        // low main (60 Hz) is ducked by the low key ...
        let low = sine(48000, 60.0, 0.1);
        let mut e = XEngine::new(SR);
        e.set_params(&sc_params());
        let g_low = gain_db(&run_sc(&mut e, &low, Some(&key)), &low);
        // ... a high main (5 kHz) is not: its band's slice of the key is silent, and the key replaces the band's own detector
        let high = sine(48000, 5000.0, 0.1);
        let mut e = XEngine::new(SR);
        e.set_params(&sc_params());
        let g_high = gain_db(&run_sc(&mut e, &high, Some(&key)), &high);
        assert!(g_low < -6.0, "low band should be ducked by the low key: {g_low} dB");
        assert!(g_high > -1.0, "high band must ignore a low key: {g_high} dB");
    }

    #[test]
    fn key_is_ignored_when_off_or_when_the_host_has_no_bus() {
        let main = sine(48000, 60.0, 0.1);
        let key = sine(48000, 60.0, 0.9);
        let mut p = sc_params();
        p.sc_on = false;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        // sidechain off: the key does nothing, so the quiet main keeps its own (key-free) detection: key_mix is moot
        let g_off = gain_db(&run_sc(&mut e, &main, Some(&key)), &main);
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        let g_off_nokey = gain_db(&run_sc(&mut e, &main, None), &main);
        assert!((g_off - g_off_nokey).abs() < 0.2, "key must not matter while the sidechain is off: {g_off} vs {g_off_nokey}");
        // sidechain on but no bus: same as off
        let mut e = XEngine::new(SR);
        e.set_params(&sc_params());
        let g_nobus = gain_db(&run_sc(&mut e, &main, None), &main);
        assert!((g_nobus - g_off_nokey).abs() < 0.2, "no bus = no key: {g_nobus} vs {g_off_nokey}");
    }

    #[test]
    fn key_hp_ignores_a_low_key_and_gain_scales_it() {
        let main = sine(48000, 60.0, 0.1);
        let key = sine(48000, 60.0, 0.9);
        let mut p = sc_params();
        p.sc_hp = 2000.0;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        let g = gain_db(&run_sc(&mut e, &main, Some(&key)), &main);
        assert!(g > -1.0, "a 60 Hz key behind a 2 kHz high-pass must not duck: {g} dB");
        // key gain scales the ducking: the same key 40 dB lower ducks far less than at 0 dB
        let g0 = {
            let mut e = XEngine::new(SR);
            e.set_params(&sc_params());
            gain_db(&run_sc(&mut e, &main, Some(&key)), &main)
        };
        let mut p = sc_params();
        p.sc_gain = -40.0;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        let g40 = gain_db(&run_sc(&mut e, &main, Some(&key)), &main);
        assert!(g0 < -6.0 && g40 > g0 + 5.0, "gain -40 dB should duck much less: {g0} dB vs {g40} dB");
    }

    #[test]
    fn sidechain_solo_outputs_the_conditioned_key() {
        let mut p = sc_params();
        p.sc_gain = -6.0;
        let mut e = XEngine::new(SR);
        e.set_params(&p);
        e.sc_solo = true;
        let key = sine(48000, 1000.0, 0.5);
        let y = run_sc(&mut e, &sine(48000, 440.0, 0.1), Some(&key));
        let want = rms(&key[24000..]) * 0.5012; // -6 dB
        assert!((rms(&y[24000..]) - want).abs() < want * 0.1, "solo should be the key at -6 dB: {} vs {want}", rms(&y[24000..]));
    }

    #[test]
    fn saturation_and_all_modes_run_clean() {
        let x = noise(24000, 3);
        for os in 0..3 {
            let mut p = XParams::default();
            p.os = os;
            p.lookahead_ms = 3.0;
            for b in p.bands.iter_mut() {
                b.drive = 80.0;
                b.shape = 5;
                b.thresh = -24.0;
                b.release = 2000.0;
            }
            p.eq.bands[2].on = true;
            p.eq.tilt = 3.0;
            let mut e = XEngine::new(SR);
            e.set_params(&p);
            let y = run(&mut e, &x);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 8.0));
        }
    }
}
