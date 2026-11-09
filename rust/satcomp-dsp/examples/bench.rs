use satcomp_dsp::{Engine, Params};
use std::time::Instant;

fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut s = seed;
    (0..n).map(|_| { s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (((s >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0) as f32 * 0.3 }).collect()
}

fn run(name: &str, p: &Params, sc: bool, vary: bool) {
    let sr = 48000.0;
    let secs = 20usize;
    let x = noise(sr as usize * secs, 1);
    let y = noise(sr as usize * secs, 2);
    let mut e = Engine::new(sr);
    e.set_params(p);
    let (mut l, mut r) = (x.clone(), y.clone());
    let sca = noise(sr as usize * secs, 3);
    let t = Instant::now();
    let mut q = *p;
    for (k, (cl, cr)) in l.chunks_mut(256).zip(r.chunks_mut(256)).enumerate() {
        if vary { q.thresh = -18.0 + (k % 7) as f64 * 0.5; q.drive = 25.0 + (k % 5) as f64; e.set_params(&q); }
        let o = k * 256;
        if sc { e.process_sc(cl, cr, Some((&sca[o..o + cl.len()], &sca[o..o + cl.len()]))); } else { e.process(cl, cr); }
    }
    let dt = t.elapsed().as_secs_f64();
    println!("{name:34} {:6.2}% of one core ({:.0}x realtime)", dt / secs as f64 * 100.0, secs as f64 / dt);
}

fn main() {
    let t = Instant::now();
    for _ in 0..20 { let _ = Engine::new(48000.0); }
    println!("Engine::new                        {:.2} ms each", t.elapsed().as_secs_f64() * 1000.0 / 20.0);
    let mut p = Params::default();
    run("default (os off)", &p, false, false);
    p.os = 1; run("os x2", &p, false, false);
    p.os = 2; run("os x4", &p, false, false);
    run("os x4 + automation every block", &p, false, true);
    p.os = 0; p.lookahead_ms = 5.0; p.sc_on = true; run("lookahead 5ms + sidechain", &p, true, false);
    let mut p = Params::default();
    for st in 0..2 { for b in 0..3 { p.eq[st].bands[b].gain = 4.0; } p.eq[st].tilt = 2.0; }
    run("EQ pre+post all bands + tilt", &p, false, false);
    let mut p = Params::default();
    p.shape = 14;
    p.curve = satcomp_dsp::shapers::Curve::from_points(&[(-1.0, -1.0), (-0.3, -0.8), (0.2, 0.9), (1.0, 1.0)]);
    run("custom shaper", &p, false, false);
    p.bypass = true; run("bypass (still computes)", &p, false, false);
}
