//! Golden-render scenarios for the 0.5.2 refactor safety net.
//!
//! Only the public `Engine` / `Params` API is used, so this exact file compiles against v0.5.1 (the baseline) and against
//! the refactored tree. `examples/golden.rs` renders every scenario to disk (`render`) and diffs two render folders
//! (`diff`); `tests/golden.rs` checks the current build against the committed `tests/golden/golden.txt` fixture.

use satcomp_dsp::eq::decode_band;
use satcomp_dsp::shapers::Curve;
use satcomp_dsp::{Engine, Params};

pub struct Scn {
    pub name: String,
    pub sr: f64,
    pub secs: f64,
    /// 0 = fixed 256, 1 = ragged mix of block sizes, 2 = one sample at a time through `tick`
    pub blocks: u8,
    pub p: Params,
    /// change parameters between blocks (automation) using `vary`
    pub vary: bool,
    pub smoothing: bool,
    pub key: bool,
    pub solo: bool,
    pub delta: bool,
    /// EQ page tap on: 0 = pre, 1 = post
    pub tap: Option<usize>,
    pub match_req: bool,
    pub sat_req: bool,
}

impl Scn {
    fn new(name: String, p: Params) -> Self {
        Self {
            name,
            sr: 48000.0,
            secs: 1.5,
            blocks: 0,
            p,
            vary: false,
            smoothing: true,
            key: false,
            solo: false,
            delta: false,
            tap: None,
            match_req: false,
            sat_req: false,
        }
    }
}

fn lcg(s: &mut u64) -> f64 {
    *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
}

/// Deterministic programme: noise bed + decaying 220 Hz hits every 0.5 s + a 3 kHz tone, with a quiet stretch in the
/// last quarter (exercises release, the level gate and auto-release).
pub fn input(n: usize, sr: f64, seed: u64) -> (Vec<f32>, Vec<f32>) {
    let mut s = seed;
    let (mut l, mut r) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n {
        let t = i as f64 / sr;
        let hit = (-(t % 0.5) * 9.0).exp();
        let quiet = if (i as f64) > n as f64 * 0.75 { 0.05 } else { 1.0 };
        let body = (t * 220.0 * 6.283185307).sin() * 0.7 * hit + (t * 3000.0 * 6.283185307).sin() * 0.08;
        l.push(((body + lcg(&mut s) * 0.12) * quiet) as f32);
        r.push(((body * 0.8 + (t * 1700.0 * 6.283185307).sin() * 0.1 + lcg(&mut s) * 0.12) * quiet) as f32);
    }
    (l, r)
}

/// Sidechain key: a different rhythm (pulses every 0.3 s).
fn key(n: usize, sr: f64) -> (Vec<f32>, Vec<f32>) {
    let mut s = 99u64;
    let (mut l, mut r) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for i in 0..n {
        let t = i as f64 / sr;
        let env = (-(t % 0.3) * 14.0).exp();
        let v = (t * 90.0 * 6.283185307).sin() * 0.8 * env + lcg(&mut s) * 0.05;
        l.push(v as f32);
        r.push((v * 0.9) as f32);
    }
    (l, r)
}

fn custom_curve() -> Curve {
    Curve::from_points(&[(-1.0, -1.0), (-0.45, -0.6), (-0.1, 0.1), (0.3, 0.95), (1.0, 0.7)])
}

fn base() -> Params {
    let mut p = Params::default();
    p.drive = 55.0;
    p.bias = 8.0;
    p.thresh = -22.0;
    p.ratio = 4.0;
    p
}

pub fn scenarios() -> Vec<Scn> {
    let mut v: Vec<Scn> = Vec::new();

    // the v0.5.1 defaults, pinned (INIT now starts with drive 0 / threshold 0)
    let mut p = Params::default();
    p.drive = 25.0;
    p.thresh = -18.0;
    v.push(Scn::new("default".into(), p));
    v.push(Scn::new("base".into(), base()));

    // every shaper x every oversampling mode
    for sh in 0..15 {
        for os in 0..3 {
            let mut p = base();
            p.shape = sh;
            p.os = os;
            if sh == 14 {
                p.curve = custom_curve();
            }
            v.push(Scn::new(format!("shaper{sh}_os{os}"), p));
        }
    }

    // saturator filters, mixes, outs
    let mut p = base();
    p.sat_hp = 300.0;
    p.sat_lp = 4000.0;
    p.tone = 2500.0;
    v.push(Scn::new("sat_filters".into(), p));
    let mut p = base();
    p.sat_mix = 40.0;
    p.sat_out = -3.5;
    p.os = 1;
    v.push(Scn::new("sat_mix_out_os2".into(), p));
    let mut p = base();
    p.drive = 0.0;
    p.bias = 0.0;
    v.push(Scn::new("sat_off".into(), p));
    let mut p = base();
    p.drive = 0.0;
    p.bias = 30.0;
    v.push(Scn::new("bias_only".into(), p));

    // compressor
    for (n, f) in [
        ("ratio_inf", (|p: &mut Params| p.ratio = 20.0) as fn(&mut Params)),
        ("ratio_2", |p| p.ratio = 2.0),
        ("release_auto", |p| p.release = 2000.0),
        ("release_fast_attack_fast", |p| {
            p.release = 30.0;
            p.attack = 0.5;
        }),
        ("knee_hard", |p| p.knee = 0.0),
        ("knee_wide_shape", |p| {
            p.knee = 18.0;
            p.knee_shape = 70.0;
        }),
        ("speed_fast", |p| p.speed = 90.0),
        ("speed_slow", |p| p.speed = 10.0),
        ("detector_peak", |p| p.detector = 0.0),
        ("detector_rms", |p| p.detector = 100.0),
        ("det_hp", |p| p.det_hp = 200.0),
        ("ff_fb_half", |p| p.ff_fb = 50.0),
        ("ff_fb_full", |p| p.ff_fb = 100.0),
        ("gr_dist", |p| p.gr_dist = 60.0),
        ("lookahead", |p| p.lookahead_ms = 5.0),
        ("lookahead_os4", |p| {
            p.lookahead_ms = 3.0;
            p.os = 2;
        }),
        ("makeup_mix", |p| {
            p.makeup = 4.0;
            p.mix = 60.0;
        }),
        ("out_gain", |p| p.out_gain = -6.0),
        ("comp_extreme", |p| {
            p.thresh = -50.0;
            p.ratio = 20.0;
            p.attack = 0.1;
            p.release = 5.0;
            p.gr_dist = 100.0;
        }),
    ] {
        let mut p = base();
        f(&mut p);
        v.push(Scn::new(format!("comp_{n}"), p));
    }

    // EQ stages
    let mut p = base();
    for st in 0..2 {
        p.eq[st].bands[0].gain = 5.0;
        p.eq[st].bands[1].gain = -4.0;
        p.eq[st].bands[2].gain = 3.0;
    }
    v.push(Scn::new("eq_all_bands".into(), p));
    let mut p = base();
    p.eq[0].tilt = 3.0;
    p.eq[1].tilt = -2.0;
    p.eq[0].out_gain = -2.0;
    p.eq[1].out_gain = 3.0;
    v.push(Scn::new("eq_tilt_gain".into(), p));
    for ty in 0..3 {
        let mut p = base();
        for st in 0..2 {
            for b in 0..3 {
                p.eq[st].bands[b].ty = ty as f64;
                p.eq[st].bands[b].gain = 4.0;
            }
        }
        v.push(Scn::new(format!("eq_types{ty}"), p));
    }
    let mut p = base();
    p.eq[0].bands[0].gain = 6.0;
    p.eq[1].bands[2].gain = 6.0;
    p.eq_pre_bypass = true;
    v.push(Scn::new("eq_pre_bypass".into(), p.clone()));
    p.eq_pre_bypass = false;
    p.eq_post_bypass = true;
    v.push(Scn::new("eq_post_bypass".into(), p.clone()));
    p.eq_post_bypass = false;
    p.eq_module_bypass = true;
    v.push(Scn::new("eq_module_bypass".into(), p));
    let mut p = base();
    p.eq[0].bands[1].gain = 6.0;
    p.eq[1].bands[1].gain = 6.0;
    let mut s = Scn::new("eq_tap_pre".into(), p);
    s.tap = Some(0);
    v.push(s);
    let mut p = base();
    p.eq[1].bands[1].gain = 6.0;
    let mut s = Scn::new("eq_tap_post".into(), p);
    s.tap = Some(1);
    v.push(s);

    // sidechain
    for (n, mix, hp, lp, gain) in [
        ("key_only", 100.0, 20.0, 20000.0, 0.0),
        ("key_half", 50.0, 20.0, 20000.0, 3.0),
        ("key_zero", 0.0, 20.0, 20000.0, 0.0),
        ("key_filtered", 100.0, 150.0, 5000.0, -2.0),
    ] {
        let mut p = base();
        p.sc_on = true;
        p.sc_mix = mix;
        p.sc_hp = hp;
        p.sc_lp = lp;
        p.sc_gain = gain;
        let mut s = Scn::new(format!("sc_{n}"), p);
        s.key = true;
        v.push(s);
    }
    let mut p = base();
    p.sc_on = true;
    let mut s = Scn::new("sc_solo".into(), p.clone());
    s.key = true;
    s.solo = true;
    v.push(s);
    let mut s = Scn::new("sc_on_no_bus".into(), p.clone());
    s.key = false;
    v.push(s);
    p.os = 2;
    p.lookahead_ms = 4.0;
    let mut s = Scn::new("sc_os4_lookahead".into(), p);
    s.key = true;
    v.push(s);

    // bypass / delta / match
    let mut p = base();
    p.bypass = true;
    v.push(Scn::new("bypass".into(), p));
    let mut s = Scn::new("delta".into(), base());
    s.delta = true;
    v.push(s);
    let mut p = base();
    p.gain_match = true;
    let mut s = Scn::new("gain_match".into(), p);
    s.secs = 4.2;
    v.push(s);
    let mut s = Scn::new("sat_match".into(), base());
    s.secs = 4.2;
    s.sat_req = true;
    v.push(s);
    let mut p = base();
    p.gain_match = true;
    let mut s = Scn::new("match_request_midrun".into(), p);
    s.secs = 4.2;
    s.match_req = true;
    s.sat_req = true;
    v.push(s);

    // engine-level behaviours
    let mut s = Scn::new("no_smoothing_automation".into(), base());
    s.smoothing = false;
    s.vary = true;
    s.blocks = 1;
    v.push(s);
    for (i, sr) in [44100.0, 96000.0, 22050.0].iter().enumerate() {
        let mut s = Scn::new(format!("sr{i}"), base());
        s.sr = *sr;
        s.p.os = 1;
        s.p.lookahead_ms = 2.0;
        v.push(s);
    }
    let mut s = Scn::new("blocks_ragged".into(), base());
    s.blocks = 1;
    s.p.lookahead_ms = 2.5;
    v.push(s);
    let mut s = Scn::new("tick_api".into(), base());
    s.blocks = 2;
    s.secs = 0.6;
    v.push(s);
    let mut s = Scn::new("automation".into(), base());
    s.vary = true;
    s.blocks = 1;
    v.push(s);
    let mut s = Scn::new("automation_os4_sc".into(), base());
    s.vary = true;
    s.p.os = 2;
    s.p.sc_on = true;
    s.key = true;
    v.push(s);
    let mut s = Scn::new("kitchen_sink".into(), base());
    s.vary = true;
    s.blocks = 1;
    s.key = true;
    s.p.sc_on = true;
    s.p.os = 1;
    s.p.lookahead_ms = 3.0;
    s.p.release = 2000.0;
    s.p.gain_match = true;
    s.secs = 4.2;
    s.sat_req = true;
    for st in 0..2 {
        for b in 0..3 {
            s.p.eq[st].bands[b].gain = 2.5 - b as f64;
        }
        s.p.eq[st].tilt = 1.0;
    }
    v.push(s);
    v
}

const RAGGED: [usize; 8] = [1, 7, 64, 100, 333, 480, 1024, 31];

/// Deterministic parameter "automation" applied before block `k`.
fn vary(p: &mut Params, base: &Params, k: usize) {
    p.thresh = base.thresh + (k % 7) as f64 * 1.5;
    p.drive = base.drive + (k % 5) as f64 * 4.0;
    p.bias = base.bias + ((k % 3) as f64 - 1.0) * 6.0;
    p.makeup = (k % 4) as f64 * 0.7;
    p.mix = 100.0 - (k % 6) as f64 * 5.0;
    p.sat_mix = 100.0 - (k % 4) as f64 * 10.0;
    p.sat_hp = if k % 9 < 3 { 20.0 } else { 120.0 };
    p.eq[0].bands[1].gain = ((k % 8) as f64 - 4.0) * 0.8;
    p.eq[1].bands[0].gain = ((k % 5) as f64 - 2.0) * 1.0;
    p.eq[1].bands[0].freq = 100.0 + (k % 11) as f64 * 20.0;
    p.shape = if k % 40 < 20 { base.shape } else { (base.shape + 3) % 14 };
}

pub struct Out {
    pub audio: Vec<f32>,
    pub telem: Vec<(String, u64)>,
}

pub fn render(sc: &Scn) -> Out {
    let n = (sc.sr * sc.secs) as usize;
    let (xl, xr) = input(n, sc.sr, 1);
    let (kl, kr) = key(n, sc.sr);
    let mut e = Engine::new(sc.sr);
    e.smoothing = sc.smoothing;
    e.set_params(&sc.p);
    e.sc_solo = sc.solo;
    e.delta_on = sc.delta;
    if let Some(t) = sc.tap {
        e.eq_tap_on = true;
        e.eq_vs = t;
    }
    if sc.match_req {
        e.request_match();
    }
    if sc.sat_req {
        e.request_sat_match();
    }
    let mut l = xl.clone();
    let mut r = xr.clone();
    let mut p = sc.p;
    let mut audio = Vec::with_capacity(n * 2);
    let mut sat_results: Vec<f64> = Vec::new();
    let mut lat_trace: Vec<u32> = Vec::new();
    let mut pos = 0usize;
    let mut k = 0usize;
    if sc.blocks == 2 {
        for i in 0..n {
            if sc.vary && i % 480 == 0 {
                vary(&mut p, &sc.p, i / 480);
                e.set_params(&p);
            }
            let (a, b) = e.tick(xl[i] as f64, xr[i] as f64);
            audio.push(a as f32);
            audio.push(b as f32);
        }
    } else {
        while pos < n {
            let len = if sc.blocks == 1 { RAGGED[k % RAGGED.len()] } else { 256 }.min(n - pos);
            if sc.vary {
                vary(&mut p, &sc.p, k);
                e.set_params(&p);
            }
            if k == 40 && sc.match_req {
                e.request_match();
            }
            let (ls, rs) = (&mut l[pos..pos + len], &mut r[pos..pos + len]);
            if sc.key {
                e.process_sc(ls, rs, Some((&kl[pos..pos + len], &kr[pos..pos + len])));
            } else {
                e.process(ls, rs);
            }
            for i in 0..len {
                audio.push(l[pos + i]);
                audio.push(r[pos + i]);
            }
            if let Some(v) = e.take_sat_result() {
                sat_results.push(v);
            }
            if k % 16 == 0 {
                lat_trace.push(e.latency());
            }
            pos += len;
            k += 1;
        }
    }

    let mut t: Vec<(String, u64)> = Vec::new();
    let mut f = |name: &str, v: f64| t.push((name.to_string(), v.to_bits()));
    f("latency", e.latency() as f64);
    for (i, v) in lat_trace.iter().enumerate() {
        f(&format!("lat_trace{i}"), *v as f64);
    }
    f("opk0", e.opk[0]);
    f("opk1", e.opk[1]);
    f("clip", e.clip_f as u8 as f64);
    f("gr_blk", e.gr_blk);
    f("gr_pk", e.gr_pk);
    f("vz_blk", e.vz_blk);
    f("vz_lv", e.vz_lv);
    f("mn_blk", e.mn_blk);
    f("mn_lv", e.mn_lv);
    f("sat_blk", e.sat_blk);
    f("sat_vz", e.sat_vz);
    f("fms", e.fms);
    f("ims", e.ims);
    f("oms", e.oms);
    f("pms", e.pms);
    f("sms", e.sms);
    f("diff_db", e.diff_db);
    f("diff_disp", e.diff_disp);
    f("sat_db", e.sat_db);
    f("match_trim", e.match_trim);
    f("bg_gain", e.bg_gain());
    f("gate_open", e.gate_open() as u8 as f64);
    f("out_matching", e.out_matching() as u8 as f64);
    f("sat_matching", e.sat_matching() as u8 as f64);
    f("out_prog", e.out_match_progress());
    f("sat_prog", e.sat_match_progress());
    f("eq_act0", e.eq_act[0] as u8 as f64);
    f("eq_act1", e.eq_act[1] as u8 as f64);
    for (i, v) in sat_results.iter().enumerate() {
        f(&format!("sat_result{i}"), *v);
    }
    let gc = e.gain_computer();
    f("gc_thr", gc.thr);
    f("gc_knw", gc.knw);
    f("gc_cslope", gc.cslope);
    f("gc_kc0", gc.kc0);
    f("gc_kc1", gc.kc1);
    f("gc_kc2", gc.kc2);
    let head = e.scope.head();
    f("scope_head", head as f64);
    for i in 0..head.min(200) {
        let (o, pr) = e.scope.get(head - 1 - i);
        f(&format!("scope{i}o"), o as f64);
        f(&format!("scope{i}p"), pr as f64);
    }
    let th = e.tap.w.load(std::sync::atomic::Ordering::Acquire);
    f("tap_head", th as f64);
    for i in 0..th.min(300) {
        f(&format!("tap{i}"), e.tap.get(th - 1 - i) as f64);
    }
    // parameters decode as the UI sees them (keeps eq decode in the net)
    for b in 0..3 {
        let d = decode_band(&sc.p.eq[0], b);
        f(&format!("dec{b}"), d.freq + d.gain + d.q + d.kind as f64);
    }
    Out { audio, telem: t }
}

pub fn fnv(h: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *h ^= *b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    }
}

/// (audio hash, telemetry hash, rms, peak)
pub fn summarize(o: &Out) -> (u64, u64, f64, f64) {
    let mut ha = 0xcbf29ce484222325u64;
    let (mut sq, mut pk) = (0.0f64, 0.0f64);
    for s in &o.audio {
        fnv(&mut ha, &s.to_bits().to_le_bytes());
        sq += (*s as f64) * (*s as f64);
        pk = pk.max((*s as f64).abs());
    }
    let mut ht = 0xcbf29ce484222325u64;
    for (k, v) in &o.telem {
        fnv(&mut ht, k.as_bytes());
        fnv(&mut ht, &v.to_le_bytes());
    }
    (ha, ht, (sq / o.audio.len().max(1) as f64).sqrt(), pk)
}
