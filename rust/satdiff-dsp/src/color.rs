//! The colour-fx slot: DIFFUSOR (diffuse.rs), PHASER, FLANGER and RINGMOD behind one switch. Each type is the wet signal of its
//! effect; the global COLOR MIX blends it with the dry (so a phaser or flanger at 50 % has the classic notches). One type runs at a
//! time: changing the type fades the old one out, resets the new one and fades it in. No latency.
//!
//! * PHASER: 2..12 first-order allpass stages swept by a sine LFO around FREQ (DEPTH = how many octaves, 100 % = +-4), FEEDBACK
//!   around the whole cascade, WIDTH = LFO phase offset between the channels (0..180 degrees).
//! * FLANGER: a modulated delay line (centre DELAY, DEPTH = share of it, 4-point cubic read), FEEDBACK of either sign, WIDTH as above.
//! * RINGMOD: the signal times an internal carrier (sine / triangle / square / saw, band-limited square and saw), FREQ 20 Hz..20 kHz,
//!   AMOUNT blends from the unchanged signal (0 %) through amplitude modulation to ring modulation (100 %).

use crate::diffuse::{Diffusor, MAX_STAGES};
use crate::params::DiffParams;
use satcomp_dsp::level::one_pole;
use std::f64::consts::{PI, TAU};

pub const FX_DIFFUSOR: i32 = 0;
pub const FX_PHASER: i32 = 1;
pub const FX_FLANGER: i32 = 2;
pub const FX_RINGMOD: i32 = 3;

/// Phaser stage count for the shared STAGES percentage (0 % = 2 stages, 100 % = 12), from the diffusor's 1..100 count.
pub fn phaser_stages(count: i32) -> usize {
    let pct = (count.clamp(1, MAX_STAGES as i32) - 1) as f64 / (MAX_STAGES as f64 - 1.0);
    2 * (1 + (pct * 5.0).round() as usize)
}

const MAX_PHASER: usize = 12;
/// longest flanger delay (centre 10 ms plus a 90 % sweep) with a margin
const MAX_FLANGE_MS: f64 = 21.0;

#[derive(Clone, Copy, Default)]
struct Smooth {
    v: f64,
    t: f64,
}

impl Smooth {
    fn new(v: f64) -> Self {
        Self { v, t: v }
    }
    #[inline]
    fn step(&mut self, c: f64) -> f64 {
        self.v += (self.t - self.v) * c;
        self.v
    }
    fn snap(&mut self) {
        self.v = self.t;
    }
}

struct Phaser {
    phase: f64,
    z: [[f64; MAX_PHASER]; 2],
    last: [f64; 2],
    centre: Smooth, // ln Hz
    depth: Smooth,  // octaves
    fb: Smooth,
    width: Smooth, // radians
    rate: f64,
    stages: usize,
}

impl Phaser {
    fn new() -> Self {
        Self { phase: 0.0, z: [[0.0; MAX_PHASER]; 2], last: [0.0; 2], centre: Smooth::new(1000f64.ln()), depth: Smooth::new(2.0), fb: Smooth::new(0.0), width: Smooth::new(0.0), rate: 0.5, stages: 4 }
    }

    fn reset(&mut self) {
        self.z = [[0.0; MAX_PHASER]; 2];
        self.last = [0.0; 2];
    }

    #[inline]
    fn tick(&mut self, x: [f64; 2], sr: f64, c: f64) -> [f64; 2] {
        let (ctr, dep, fb, wid) = (self.centre.step(c), self.depth.step(c), self.fb.step(c), self.width.step(c));
        self.phase = (self.phase + TAU * self.rate / sr).rem_euclid(TAU);
        let top = (sr * 0.45).min(20000.0);
        let mut out = [0.0; 2];
        for ch in 0..2 {
            let lfo = (self.phase + ch as f64 * wid).sin();
            let f = (ctr + dep * lfo * std::f64::consts::LN_2).exp().clamp(20.0, top);
            let t = (PI * f / sr).tan();
            let a = (t - 1.0) / (t + 1.0);
            // a gentle limiter on the feedback keeps a resonant sweep bounded
            let mut v = x[ch] + (fb * self.last[ch]).tanh();
            for k in 0..self.stages {
                let y = a * v + self.z[ch][k];
                self.z[ch][k] = v - a * y;
                v = y;
            }
            self.last[ch] = v;
            out[ch] = v;
        }
        out
    }
}

struct Flanger {
    buf: [Vec<f64>; 2],
    w: usize,
    phase: f64,
    delay: Smooth, // ms
    depth: Smooth, // 0..1
    fb: Smooth,
    width: Smooth,
    rate: f64,
}

impl Flanger {
    fn new(sr: f64) -> Self {
        let n = (sr * MAX_FLANGE_MS * 0.001) as usize + 8;
        Self { buf: [vec![0.0; n], vec![0.0; n]], w: 0, phase: 0.0, delay: Smooth::new(2.0), depth: Smooth::new(0.5), fb: Smooth::new(0.0), width: Smooth::new(0.0), rate: 0.5 }
    }

    fn resize(&mut self, sr: f64) {
        let n = (sr * MAX_FLANGE_MS * 0.001) as usize + 8;
        self.buf = [vec![0.0; n], vec![0.0; n]];
        self.w = 0;
    }

    fn reset(&mut self) {
        for b in &mut self.buf {
            b.iter_mut().for_each(|v| *v = 0.0);
        }
    }

    #[inline]
    fn read(buf: &[f64], w: usize, d: f64) -> f64 {
        // 4-point cubic (Catmull-Rom) read `d` samples behind the write head
        let n = buf.len();
        let di = d.floor();
        let f = d - di;
        let at = |k: isize| buf[(w as isize - k).rem_euclid(n as isize) as usize];
        let i = di as isize;
        let (y0, y1, y2, y3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        let (c1, c2, c3) = (0.5 * (y2 - y0), y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3, 0.5 * (y3 - y0) + 1.5 * (y1 - y2));
        ((c3 * f + c2) * f + c1) * f + y1
    }

    #[inline]
    fn tick(&mut self, x: [f64; 2], sr: f64, c: f64) -> [f64; 2] {
        let (dl, dep, fb, wid) = (self.delay.step(c), self.depth.step(c), self.fb.step(c), self.width.step(c));
        self.phase = (self.phase + TAU * self.rate / sr).rem_euclid(TAU);
        let n = self.buf[0].len();
        let mut out = [0.0; 2];
        for ch in 0..2 {
            let lfo = (self.phase + ch as f64 * wid).sin();
            let ms = (dl * (1.0 + 0.9 * dep * lfo)).clamp(0.05, MAX_FLANGE_MS - 1.5);
            let d = (ms * 0.001 * sr).max(2.0);
            let del = Self::read(&self.buf[ch], self.w, d);
            self.buf[ch][self.w] = x[ch] + (fb * del).tanh();
            out[ch] = del;
        }
        self.w = (self.w + 1) % n;
        out
    }
}

struct RingMod {
    phase: f64,
    freq: Smooth,
    amount: Smooth,
    shape: i32,
}

impl RingMod {
    fn new() -> Self {
        Self { phase: 0.0, freq: Smooth::new(440.0), amount: Smooth::new(1.0), shape: 0 }
    }

    /// PolyBLEP correction around a discontinuity at phase 0 (`dt` = phase step)
    fn blep(t: f64, dt: f64) -> f64 {
        if t < dt {
            let t = t / dt;
            2.0 * t - t * t - 1.0
        } else if t > 1.0 - dt {
            let t = (t - 1.0) / dt;
            t * t + 2.0 * t + 1.0
        } else {
            0.0
        }
    }

    fn wave(&self, dt: f64) -> f64 {
        let p = self.phase;
        match self.shape {
            1 => 4.0 * (p - 0.5).abs() - 1.0,
            2 => (if p < 0.5 { 1.0 } else { -1.0 }) + Self::blep(p, dt) - Self::blep((p + 0.5).rem_euclid(1.0), dt),
            3 => 2.0 * p - 1.0 - Self::blep(p, dt),
            _ => (TAU * p).sin(),
        }
    }

    #[inline]
    fn tick(&mut self, x: [f64; 2], sr: f64, c: f64) -> [f64; 2] {
        let (f, a) = (self.freq.step(c), self.amount.step(c));
        let dt = (f / sr).min(0.45);
        let car = self.wave(dt);
        self.phase = (self.phase + dt).rem_euclid(1.0);
        let m = (1.0 - a) + a * car;
        [x[0] * m, x[1] * m]
    }
}

pub struct ColorFx {
    sr: f64,
    dif: Diffusor,
    pha: Phaser,
    fla: Flanger,
    rm: RingMod,
    /// the type that is running and the one the host asks for
    cur: i32,
    want: i32,
    on: bool,
    /// fade of the phaser / flanger / ringmod path (the diffusor fades itself), 0..1
    gate: f64,
    mix: Smooth,
    sm_c: f64,
    gate_c: f64,
    /// the last settings, so the diffusor can be re-armed after another type ran
    p: DiffParams,
}

impl ColorFx {
    pub fn new(sr: f64) -> Self {
        let mut f = Self { sr, dif: Diffusor::new(sr), pha: Phaser::new(), fla: Flanger::new(sr), rm: RingMod::new(), cur: FX_DIFFUSOR, want: FX_DIFFUSOR, on: false, gate: 0.0, mix: Smooth::new(1.0), sm_c: 0.0, gate_c: 0.0, p: DiffParams::default() };
        f.set_sample_rate(sr);
        f
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sr = sr;
        self.dif.set_sample_rate(sr);
        self.fla.resize(sr);
        self.sm_c = one_pole(sr, 0.01);
        self.gate_c = one_pole(sr, 0.02);
    }

    /// Clear the filter state (the engine does this when the fx moves between PRE and POST).
    pub fn reset(&mut self) {
        self.dif.reset();
        self.pha.reset();
        self.fla.reset();
    }

    fn arm_diffusor(&mut self, snap: bool) {
        let p = self.p;
        let on = self.on && self.cur == FX_DIFFUSOR && self.want == FX_DIFFUSOR;
        self.dif.set(on, p.fx_freq, p.fx_q, p.fx_stages, p.fx_spread, p.fx_mix, snap);
    }

    /// `snap` jumps every smoothed value to its target (a fresh engine starts at the host's settings).
    pub fn set(&mut self, p: &DiffParams, snap: bool) {
        self.on = p.fx_on;
        self.want = p.fx_type.clamp(0, 3);
        if snap {
            self.cur = self.want;
        }
        self.p = *p;
        // the diffusor takes everything it needs; it runs only while it is the running type and asked for
        self.arm_diffusor(snap);
        let top = (self.sr * 0.45).min(20000.0);
        self.pha.centre.t = p.fx_freq.clamp(20.0, top).ln();
        self.pha.depth.t = p.fx_depth.clamp(0.0, 100.0) * 0.01 * 4.0;
        self.pha.fb.t = p.fx_fb.clamp(-95.0, 95.0) * 0.01;
        self.pha.width.t = p.fx_spread.clamp(0.0, 100.0) * 0.01 * PI;
        self.pha.rate = p.fx_rate.clamp(0.02, 20.0);
        self.pha.stages = phaser_stages(p.fx_stages);
        self.fla.delay.t = p.fx_delay.clamp(0.1, 10.0);
        self.fla.depth.t = p.fx_depth.clamp(0.0, 100.0) * 0.01;
        self.fla.fb.t = p.fx_fb.clamp(-95.0, 95.0) * 0.01;
        self.fla.width.t = p.fx_spread.clamp(0.0, 100.0) * 0.01 * PI;
        self.fla.rate = p.fx_rate.clamp(0.02, 20.0);
        self.rm.freq.t = p.fx_freq.clamp(20.0, top);
        self.rm.amount.t = p.fx_depth.clamp(0.0, 100.0) * 0.01;
        self.rm.shape = p.fx_shape.clamp(0, 3);
        self.mix.t = (p.fx_mix * 0.01).clamp(0.0, 1.0);
        if snap {
            for s in [&mut self.pha.centre, &mut self.pha.depth, &mut self.pha.fb, &mut self.pha.width, &mut self.fla.delay, &mut self.fla.depth, &mut self.fla.fb, &mut self.fla.width, &mut self.rm.freq, &mut self.rm.amount, &mut self.mix] {
                s.snap();
            }
            self.gate = 0.0;
        }
    }

    #[inline]
    pub fn tick(&mut self, x: [f64; 2]) -> [f64; 2] {
        // type change: the running type fades out, then the new one is cleared and fades in
        if self.cur != self.want {
            let drained = if self.cur == FX_DIFFUSOR {
                self.arm_diffusor(false); // its enable goes down (cur != want); it fades itself
                self.dif.idle()
            } else {
                self.gate < 1e-4
            };
            if drained {
                self.cur = self.want;
                self.reset();
                self.rm.phase = 0.0;
                self.pha.phase = 0.0;
                self.fla.phase = 0.0;
                self.arm_diffusor(false); // a diffusor that just became the running type starts fading in
            }
        }
        if self.cur == FX_DIFFUSOR {
            return self.dif.tick(x);
        }
        let target = if self.on && self.cur == self.want { 1.0 } else { 0.0 };
        self.gate += (target - self.gate) * self.gate_c;
        if self.gate < 1e-6 && target == 0.0 {
            self.gate = 0.0;
            return x;
        }
        let m = self.mix.step(self.sm_c) * self.gate;
        let wet = match self.cur {
            FX_PHASER => self.pha.tick(x, self.sr, self.sm_c),
            FX_FLANGER => self.fla.tick(x, self.sr, self.sm_c),
            _ => self.rm.tick(x, self.sr, self.sm_c),
        };
        [x[0] + (wet[0] - x[0]) * m, x[1] + (wet[1] - x[1]) * m]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48000.0;

    fn params(ty: i32) -> DiffParams {
        DiffParams { fx_on: true, fx_type: ty, fx_mix: 100.0, ..DiffParams::default() }
    }

    fn render(p: &DiffParams, x: &[f64]) -> Vec<f64> {
        let mut f = ColorFx::new(SR);
        f.set(p, true);
        x.iter().map(|&v| f.tick([v, v])[0]).collect()
    }

    fn sine(f: f64, n: usize, a: f64) -> Vec<f64> {
        (0..n).map(|i| a * (TAU * f * i as f64 / SR).sin()).collect()
    }

    fn rms(v: &[f64]) -> f64 {
        (v.iter().map(|a| a * a).sum::<f64>() / v.len() as f64).sqrt()
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

    #[test]
    fn off_is_an_exact_bypass_for_every_type() {
        let x = noise(6000, 3);
        for ty in 0..4 {
            let mut p = params(ty);
            p.fx_on = false;
            assert_eq!(render(&p, &x), x, "type {ty}");
        }
    }

    #[test]
    fn phaser_stage_counts() {
        assert_eq!(phaser_stages(1), 2);
        assert_eq!(phaser_stages(100), 12);
        assert!((1..=100).all(|c| phaser_stages(c) % 2 == 0 && (2..=12).contains(&phaser_stages(c))));
    }

    #[test]
    fn the_phaser_is_flat_alone_and_notches_when_mixed() {
        let x = noise(96000, 5);
        // 100 % wet, no feedback: an allpass cascade keeps the level
        let wet = render(&DiffParams { fx_depth: 0.0, fx_freq: 800.0, ..params(FX_PHASER) }, &x);
        assert!((rms(&wet[20000..]) / rms(&x[20000..]) - 1.0).abs() < 0.03);
        // 50 % mix with the sweep parked: a tone at the allpass' 180 degree point is cancelled (a notch)
        let mut p = DiffParams { fx_depth: 0.0, fx_freq: 800.0, fx_mix: 50.0, fx_stages: 1, ..params(FX_PHASER) };
        p.fx_rate = 0.02;
        // two stages put the half-turn at the centre frequency
        let tone = sine(800.0, 48000, 0.5);
        let y = render(&p, &tone);
        assert!(rms(&y[24000..]) < 0.05 * rms(&tone[24000..]), "notch depth {}", rms(&y[24000..]) / rms(&tone[24000..]));
        // far from the notch it passes
        let y = render(&p, &sine(6000.0, 48000, 0.5));
        assert!(rms(&y[24000..]) > 0.6 * 0.5 / 2f64.sqrt());
    }

    #[test]
    fn the_phaser_sweeps() {
        // swept notches move: the output of a steady tone near the sweep breathes with the LFO
        let tone = sine(1000.0, 96000, 0.5);
        let p = DiffParams { fx_depth: 80.0, fx_freq: 1000.0, fx_mix: 50.0, fx_rate: 2.0, fx_stages: 40, ..params(FX_PHASER) };
        let y = render(&p, &tone);
        let win = |a: usize| rms(&y[a..a + 480]);
        let levels: Vec<f64> = (0..150).map(|k| win(24000 + k * 480)).collect();
        let (lo, hi) = levels.iter().fold((f64::MAX, 0.0f64), |(l, h), &v| (l.min(v), h.max(v)));
        assert!(hi > lo * 3.0, "level swings {lo} .. {hi}");
    }

    #[test]
    fn feedback_stays_bounded_at_the_extremes() {
        let x = noise(96000, 9);
        for ty in [FX_PHASER, FX_FLANGER] {
            for fb in [-95.0, 95.0] {
                let y = render(&DiffParams { fx_fb: fb, fx_depth: 100.0, fx_rate: 20.0, fx_freq: 20.0, fx_delay: 0.1, fx_stages: 100, ..params(ty) }, &x);
                assert!(y.iter().all(|v| v.is_finite() && v.abs() < 20.0), "type {ty} fb {fb}");
            }
        }
    }

    #[test]
    fn the_flanger_makes_a_comb() {
        // 2 ms delay, no sweep, 50 % mix: notches at odd multiples of 250 Hz, peaks at multiples of 500 Hz
        let p = DiffParams { fx_depth: 0.0, fx_delay: 2.0, fx_mix: 50.0, ..params(FX_FLANGER) };
        let notch = render(&p, &sine(250.0, 48000, 0.5));
        let peak = render(&p, &sine(500.0, 48000, 0.5));
        assert!(rms(&notch[24000..]) < 0.05, "{}", rms(&notch[24000..]));
        assert!(rms(&peak[24000..]) > 0.3, "{}", rms(&peak[24000..]));
        // the delay really is 2 ms: an impulse comes back 96 samples later
        let mut imp = vec![0.0; 6000];
        imp[3000] = 1.0; // after the fade-in
        let y = render(&DiffParams { fx_depth: 0.0, fx_delay: 2.0, fx_mix: 100.0, ..params(FX_FLANGER) }, &imp);
        let at = y.iter().enumerate().skip(3050).max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).unwrap().0;
        assert!((at as i64 - 3096).abs() <= 1, "echo at {at}");
    }

    #[test]
    fn ringmod_makes_sum_and_difference_tones() {
        // 1 kHz x 300 Hz sine: 700 Hz and 1300 Hz, and nothing at 1 kHz
        let tone = sine(1000.0, 48000, 0.5);
        let y = render(&DiffParams { fx_freq: 300.0, fx_depth: 100.0, fx_shape: 0, ..params(FX_RINGMOD) }, &tone);
        let bin = |f: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            for i in 24000..48000 {
                let w = TAU * f * i as f64 / SR;
                re += y[i] * w.cos();
                im += y[i] * w.sin();
            }
            (re * re + im * im).sqrt() / 12000.0
        };
        assert!(bin(700.0) > 0.1 && bin(1300.0) > 0.1, "{} {}", bin(700.0), bin(1300.0));
        assert!(bin(1000.0) < 0.01 && bin(300.0) < 0.01, "carrier / input leak {} {}", bin(1000.0), bin(300.0));
        // amount 0 leaves the signal alone
        let same = render(&DiffParams { fx_freq: 300.0, fx_depth: 0.0, ..params(FX_RINGMOD) }, &tone);
        assert!(same.iter().zip(&tone).all(|(a, b)| (a - b).abs() < 1e-9));
    }

    #[test]
    fn ringmod_carriers_are_bounded_and_the_band_limited_ones_stay_clean() {
        let x = sine(1234.0, 48000, 0.5);
        for shape in 0..4 {
            let y = render(&DiffParams { fx_freq: 3000.0, fx_depth: 100.0, fx_shape: shape, ..params(FX_RINGMOD) }, &x);
            assert!(y.iter().all(|v| v.abs() <= 0.5 + 1e-6 && v.is_finite()), "shape {shape}");
            assert!(rms(&y[1000..]) > 0.1, "shape {shape}");
        }
    }

    #[test]
    fn changing_the_type_fades_instead_of_clicking() {
        let tone = sine(300.0, 48000, 0.4);
        let mut f = ColorFx::new(SR);
        f.set(&params(FX_PHASER), true);
        let mut out = vec![];
        for (i, &v) in tone.iter().enumerate() {
            if i == 20000 {
                f.set(&params(FX_RINGMOD), false);
            }
            out.push(f.tick([v, v])[0]);
        }
        // no sample-to-sample jump beyond what a 300 Hz tone and a modulated carrier can do
        let step = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f64, f64::max);
        assert!(step < 0.15, "step {step}");
        // and the new type is what ends up running
        assert_eq!(f.cur, FX_RINGMOD);
    }
}
