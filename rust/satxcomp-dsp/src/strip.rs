//! One band's channel strip: saturator -> lookahead -> compressor -> parallel mix and gain, with a latency-aligned bypass.

use crate::params::BandParams;
use satcomp_dsp::delay::StereoDelay;
use satcomp_dsp::level::{one_pole, LN10_20};
use satcomp_dsp::{Compressor, SatParams, Saturator};
use satcomp_dsp::CompParams;

pub struct StripOut {
    /// the band's output (module on/off crossfade applied), aligned to the plug-in latency
    pub y: [f64; 2],
    /// what the compressor removed (delta monitor)
    pub removed: [f64; 2],
    /// the saturated signal before the compressor (for the "dry" spectrum)
    pub pre: [f64; 2],
}

pub struct Strip {
    pub sat: Saturator,
    pub comp: Compressor,
    lk: StereoDelay<4096>,
    dry: StereoDelay<4096>,
    la_s: usize,
    cmix: f64,
    cmix_t: f64,
    cg: f64,
    cg_t: f64,
    on_s: f64,
    on_t: f64,
    sm_c: f64,
    on_c: f64,
    /// gain reduction of the loudest channel in the current block, dB (the owner resets it)
    pub gr_blk: f64,
    /// peak of the saturator output (before the compressor) since the owner last reset it
    pub sat_pk: f64,
}

impl Default for Strip {
    fn default() -> Self {
        Self::new()
    }
}

impl Strip {
    pub fn new() -> Self {
        Self {
            sat: Saturator::new(),
            comp: Compressor::new(),
            lk: StereoDelay::new(),
            dry: StereoDelay::new(),
            la_s: 0,
            cmix: 1.0,
            cmix_t: 1.0,
            cg: 1.0,
            cg_t: 1.0,
            on_s: 1.0,
            on_t: 1.0,
            sm_c: 0.0,
            on_c: 0.0,
            gr_blk: 0.0,
            sat_pk: 0.0,
        }
    }

    pub fn sat_params(p: &BandParams, os: i32) -> SatParams {
        SatParams { shape: p.shape, curve: p.curve, drive: p.drive, bias: p.bias, hp: 20.0, lp: 20000.0, tone: p.tone, os, mix: p.sat_mix, out_db: p.sat_gain }
    }

    pub fn comp_params(p: &BandParams, key_mix: f64) -> CompParams {
        CompParams {
            thresh: p.thresh,
            ratio: p.ratio,
            attack: p.attack,
            release: p.release,
            speed: p.speed,
            knee: p.knee,
            knee_shape: p.knee_shape,
            detector: p.detector,
            det_hp: 0.0,
            ff_fb: p.ff_fb,
            gr_dist: p.gr_dist,
            key_mix,
        }
    }

    pub fn latency(&self) -> usize {
        self.sat.latency() + self.la_s
    }

    pub fn derive(&mut self, p: &BandParams, os: i32, la_s: usize, key_mix: f64, sr: f64, snap: bool, smoothing: bool) {
        self.sat.derive(&Self::sat_params(p, os), sr, snap, smoothing);
        // Saturator::derive only latches SAT OUT on a snap; the target has to follow the parameter every time
        self.sat.set_out_db(p.sat_gain);
        self.comp.derive(&Self::comp_params(p, key_mix), sr);
        self.la_s = la_s;
        self.cmix_t = p.comp_mix * 0.01;
        self.cg_t = (p.comp_gain * LN10_20).exp();
        self.on_t = if p.on { 1.0 } else { 0.0 };
        self.sm_c = one_pole(sr, 0.02);
        self.on_c = one_pole(sr, 0.01);
        if snap {
            self.cmix = self.cmix_t;
            self.cg = self.cg_t;
            self.on_s = self.on_t;
        }
    }

    pub fn begin_block(&mut self) {
        self.sat.begin_block();
    }

    #[inline]
    /// `key` = this band's slice of the external key while the sidechain is active (replaces the band's own detector signal
    /// according to KEY MIX), aligned with the saturator output.
    pub fn tick(&mut self, inp: [f64; 2], smoothing: bool, key: Option<[f64; 2]>) -> StripOut {
        self.dry.advance();
        self.dry.put(inp);
        let dd = self.dry.tap(self.latency());
        let cc = self.sat.tick(inp, smoothing);
        self.sat_pk = self.sat_pk.max(cc[0].abs().max(cc[1].abs()));
        self.lk.advance();
        self.lk.put(cc);
        let cd = if self.la_s > 0 { self.lk.tap(self.la_s) } else { cc };
        let co = self.comp.tick(cc, cd, key);
        if smoothing {
            self.cmix += (self.cmix_t - self.cmix) * self.sm_c;
            self.cg += (self.cg_t - self.cg) * self.sm_c;
        } else {
            self.cmix = self.cmix_t;
            self.cg = self.cg_t;
        }
        self.on_s += (self.on_t - self.on_s) * self.on_c;
        let mut y = [0.0f64; 2];
        let mut removed = [0.0f64; 2];
        let mut pre = [0.0f64; 2];
        for c in 0..2 {
            let w = co.y[c] * self.cg;
            let proc = cd[c] + (w - cd[c]) * self.cmix;
            y[c] = dd[c] + (proc - dd[c]) * self.on_s;
            removed[c] = (cd[c] - co.y[c]) * self.on_s;
            pre[c] = dd[c] + (cd[c] - dd[c]) * self.on_s;
        }
        self.gr_blk = self.gr_blk.max(co.g[0].max(co.g[1]));
        StripOut { y, removed, pre }
    }
}
