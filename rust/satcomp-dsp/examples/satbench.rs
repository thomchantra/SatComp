//! Saturator-only timing: `satbench IN.wav [secs] [variant]` (16-bit WAV). Median us per 512-sample block of one stereo `Saturator`
//! (`tick_split` + `gain_step`) per configuration, to see where the shared core spends its time.
//! `XB_DUMP=dir satbench ...` writes each variant's output (f32 LE, interleaved) to dir/NN.f32 and `satbench cmp dirA dirB` compares
//! two dumps (max abs / RMS difference), to prove an optimisation leaves the audio alone.

use satcomp_dsp::{SatParams, Saturator};
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

struct V {
    name: String,
    p: SatParams,
    dc_comp: bool,
    /// drive / bias are moved every few blocks (the glide and the DC tracker run)
    moving: bool,
}

fn variants() -> Vec<V> {
    let base = SatParams { drive: 30.0, asym: Some(100.0), steep: true, ..SatParams::default() };
    let mut v = Vec::new();
    let mut add = |name: &str, p: SatParams, dc: bool, moving: bool| v.push(V { name: name.to_string(), p, dc_comp: dc, moving });
    for os in 0..=3 {
        add(&format!("asym 100, tube, x{}", [1, 2, 4, 8][os as usize]), SatParams { os, ..base }, false, false);
    }
    for os in 0..=2 {
        add(&format!("legacy (no asym), tube, x{}", [1, 2, 4][os as usize]), SatParams { os, asym: None, ..base }, false, false);
    }
    add("asym 100, soft (odd), x1", SatParams { shape: 1, ..base }, false, false);
    add("asym 100, soft (odd), x4", SatParams { shape: 1, os: 2, ..base }, false, false);
    add("asym 0, tube, x1", SatParams { asym: Some(0.0), ..base }, false, false);
    add("drive 0 (off), x1", SatParams { drive: 0.0, ..base }, false, false);
    add("x1 + HP 100 Hz + LP 8 kHz (48 dB)", SatParams { hp: 100.0, lp: 8000.0, ..base }, false, false);
    add("x1 + tone 6 kHz", SatParams { tone: 6000.0, ..base }, false, false);
    add("x1 + mix 50 %", SatParams { mix: 50.0, ..base }, false, false);
    add("x1 + dc_comp (SatDiff), still", base, true, false);
    add("x1 + dc_comp, drive moving", base, true, true);
    add("x4 + dc_comp, drive moving", SatParams { os: 2, ..base }, true, true);
    v
}

fn cmp(a: &str, b: &str) {
    let mut names: Vec<_> = std::fs::read_dir(a).unwrap().map(|e| e.unwrap().file_name()).collect();
    names.sort();
    println!("| file | samples | max abs diff | diff RMS dBFS | signal RMS dBFS |\n|---|---|---|---|---|");
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
    let (l, r, sr) = read_wav(&a[1]);
    let secs: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let only = a.get(3);
    let dump = std::env::var("XB_DUMP").ok();
    let block = 512usize;
    println!("| configuration (one stereo saturator) | median us/block | CPU % of a core |\n|---|---|---|");
    for (vi, v) in variants().into_iter().enumerate() {
        if only.map_or(false, |o| *o != v.name) {
            continue;
        }
        let mut s = Saturator::new();
        let mut p = v.p;
        s.derive(&p, sr, true, true);
        s.set_dc_comp(v.dc_comp);
        let total = (secs * sr) as usize / block;
        let warm = (1.0 * sr) as usize / block;
        let mut t = Vec::new();
        let mut dumped: Vec<u8> = Vec::new();
        let mut pos = 0;
        for k in 0..warm + total {
            if v.moving && k % 3 == 0 {
                p.drive = 30.0 + 20.0 * (((k / 3) as f64) * 0.7).sin();
                p.bias = 10.0 * (((k / 3) as f64) * 0.3).cos();
                s.derive(&p, sr, false, true);
            }
            s.begin_block();
            let t0 = Instant::now();
            let mut out = Vec::with_capacity(block * 2);
            for i in 0..block {
                let j = (pos + i) % l.len();
                let (cc, _) = s.tick_split([l[j] as f64, r[j] as f64], true);
                let g = s.gain_step();
                out.push((cc[0] * g) as f32);
                out.push((cc[1] * g) as f32);
            }
            if k >= warm {
                t.push(t0.elapsed().as_secs_f64() * 1e6);
            }
            if dump.is_some() {
                for x in &out {
                    dumped.extend(x.to_le_bytes());
                }
            }
            pos += block;
        }
        if let Some(d) = &dump {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(format!("{d}/{vi:02}.f32"), dumped).unwrap();
        }
        t.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let m = t[t.len() / 2];
        println!("| {} | {m:.1} | {:.2} |", v.name, m / (block as f64 / sr * 1e6) * 100.0);
    }
}
