//! DSP-level timing of XEngine: `xbench IN.wav [secs] [variant]` (16-bit WAV). Prints median us per 512-sample block per variant,
//! the same number plugbench reports, without the plug-in shell. With a variant name only that one runs (for `sample` / profiling).
//!
//! Audio regression check around an optimisation: `XB_DUMP=dir xbench IN.wav 6` writes every variant's output (f32 LE, interleaved)
//! to dir/NN.f32; run it before and after the change, then `xbench cmp dirA dirB` prints the max / RMS difference per variant.

use satxcomp_dsp::{XEngine, XParams};
use std::time::Instant;

fn read_wav(path: &str) -> (Vec<f32>, Vec<f32>, f64) {
    let b = std::fs::read(path).unwrap();
    let sr = u32::from_le_bytes(b[24..28].try_into().unwrap()) as f64;
    let ch = u16::from_le_bytes(b[22..24].try_into().unwrap()) as usize;
    let mut i = 12;
    while i + 8 <= b.len() {
        let sz = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        if &b[i..i + 4] == b"data" {
            let d = &b[i + 8..(i + 8 + sz).min(b.len())];
            let s: Vec<f32> = d.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
            return (s.iter().step_by(ch).cloned().collect(), s.iter().skip(ch - 1).step_by(ch).cloned().collect(), sr);
        }
        i += 8 + sz + (sz & 1);
    }
    panic!("no data");
}

fn compressing() -> XParams {
    let mut p = XParams::default();
    for b in p.bands.iter_mut() {
        b.thresh = -20.0;
        b.ratio = 4.0;
    }
    p
}

/// variants that change parameters while running (crossover glide, slope change with its crossfade), by name
fn automated(name: &str, k: usize, p: &mut XParams) -> bool {
    if !name.starts_with("auto") {
        return false;
    }
    let before = *p;
    if k % 37 == 0 {
        p.x1 = 100.0 + 200.0 * ((k / 37) % 5) as f64;
        p.x2 = 600.0 + 700.0 * ((k / 37) % 3) as f64;
    }
    if name.ends_with("slopes") && k % 151 == 0 {
        let j = (k / 151) as i32;
        p.x1_slope = j % 4;
        p.x2_slope = (j + 1) % 4;
        p.x3_slope = (j + 2) % 4;
    }
    *p != before
}

fn variants() -> Vec<(&'static str, XParams)> {
    let base = compressing();
    let mut v: Vec<(&'static str, XParams)> = vec![("defaults", XParams::default()), ("compressing", base)];
    let mut p = base;
    p.lim_on = false;
    v.push(("compressing, limiter off", p));
    let mut p = base;
    p.eq.bypass = true;
    v.push(("compressing, EQ bypass", p));
    let mut p = base;
    for b in p.bands.iter_mut() {
        b.on = false;
    }
    v.push(("all 4 bands off", p));
    let mut p = base;
    for b in p.bands.iter_mut() {
        b.ratio = 1.0;
    }
    v.push(("ratio 1:1", p));
    let mut p = base;
    for b in p.bands.iter_mut() {
        b.drive = 30.0;
    }
    v.push(("compressing + drive 30 x1", p));
    p.os = 2;
    v.push(("compressing + drive 30 x4", p));
    let mut p = base;
    (p.x1_slope, p.x2_slope, p.x3_slope) = (3, 0, 2);
    for (i, b) in p.bands.iter_mut().enumerate() {
        b.drive = 20.0 + 10.0 * i as f64;
    }
    v.push(("slopes 96/12/48 + drive", p));
    let mut p = base;
    p.bands[1].drive = 40.0;
    p.bands[2].solo = true;
    p.bands[3].delta = true;
    v.push(("solo + delta + one band drive", p));
    let mut p = base;
    p.bands[2].drive = 30.0;
    v.push(("auto crossover glide", p));
    v.push(("auto crossover + slopes", p));
    v
}

fn cmp(a: &str, b: &str) {
    let mut names: Vec<_> = std::fs::read_dir(a).unwrap().map(|e| e.unwrap().file_name()).collect();
    names.sort();
    println!("| variant file | samples | max abs diff | diff RMS dBFS | signal RMS dBFS |\n|---|---|---|---|---|");
    for n in names {
        let rd = |d: &str| -> Vec<f32> { std::fs::read(format!("{d}/{}", n.to_string_lossy())).unwrap().chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect() };
        let (x, y) = (rd(a), rd(b));
        assert_eq!(x.len(), y.len(), "length differs");
        let (mut mx, mut se, mut ss) = (0.0f64, 0.0f64, 0.0f64);
        for (u, v) in x.iter().zip(&y) {
            let d = (*u as f64 - *v as f64).abs();
            mx = mx.max(d);
            se += d * d;
            ss += (*u as f64) * (*u as f64);
        }
        let db = |v: f64| 20.0 * v.max(1e-30).log10();
        println!("| {} | {} | {:.3e} | {:.1} | {:.1} |", n.to_string_lossy(), x.len(), mx, db((se / x.len() as f64).sqrt()), db((ss / x.len() as f64).sqrt()));
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(|s| s.as_str()) == Some("cmp") {
        cmp(&a[2], &a[3]);
        return;
    }
    let dump = std::env::var("XB_DUMP").ok();
    let (l, r, sr) = read_wav(&a[1]);
    let secs: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(8.0);
    let only = a.get(3);
    let block = 512usize;
    println!("| variant | median us/block | CPU % of a core |\n|---|---|---|");
    for (vi, (name, p)) in variants().into_iter().enumerate() {
        let mut p = p;
        if only.map_or(false, |o| o != name) {
            continue;
        }
        let mut e = XEngine::new(sr);
        e.set_params(&p);
        let (mut bl, mut br) = (vec![0.0f32; block], vec![0.0f32; block]);
        let total = (secs * sr) as usize / block;
        let warm = (2.0 * sr) as usize / block;
        let mut t = Vec::new();
        let mut pos = 0;
        let mut dumped: Vec<u8> = Vec::new();
        for k in 0..warm + total {
            if automated(name, k, &mut p) {
                e.set_params(&p);
            }
            for i in 0..block {
                bl[i] = l[(pos + i) % l.len()];
                br[i] = r[(pos + i) % l.len()];
            }
            pos += block;
            let t0 = Instant::now();
            e.process(&mut bl, &mut br);
            if k >= warm {
                t.push(t0.elapsed().as_secs_f64() * 1e6);
            }
            if dump.is_some() {
                for i in 0..block {
                    dumped.extend(bl[i].to_le_bytes());
                    dumped.extend(br[i].to_le_bytes());
                }
            }
        }
        if let Some(d) = &dump {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(format!("{d}/{vi:02}.f32"), dumped).unwrap();
        }
        t.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let m = t[t.len() / 2];
        println!("| {name} | {m:.1} | {:.2} |", m / (block as f64 / sr * 1e6) * 100.0);
    }
}
