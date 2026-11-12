//! Offline renderer for limiter / engine comparisons: `render PRESET.kv IN.wav OUT.wav [lim=0|1] [out_gain=dB]`.
//! PRESET.kv is `id value` per line (a SatXComp preset flattened by a script). WAVs are 32-bit float stereo.

use satxcomp_dsp::{XEngine, XParams};
use std::fs;

fn read_wav(path: &str) -> (u32, Vec<f32>, Vec<f32>) {
    let b = fs::read(path).unwrap();
    let sr = u32::from_le_bytes(b[24..28].try_into().unwrap());
    let mut i = 12;
    while i + 8 <= b.len() {
        let sz = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        if &b[i..i + 4] == b"data" {
            let d = &b[i + 8..i + 8 + sz];
            let f: Vec<f32> = d.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
            return (sr, f.iter().step_by(2).cloned().collect(), f.iter().skip(1).step_by(2).cloned().collect());
        }
        i += 8 + sz + (sz & 1);
    }
    panic!("no data chunk");
}

fn write_wav(path: &str, sr: u32, l: &[f32], r: &[f32]) {
    let n = l.len() * 8;
    let mut o = Vec::with_capacity(44 + n);
    o.extend(b"RIFF");
    o.extend(((36 + n) as u32).to_le_bytes());
    o.extend(b"WAVEfmt ");
    o.extend(16u32.to_le_bytes());
    o.extend(3u16.to_le_bytes());
    o.extend(2u16.to_le_bytes());
    o.extend(sr.to_le_bytes());
    o.extend((sr * 8).to_le_bytes());
    o.extend(8u16.to_le_bytes());
    o.extend(32u16.to_le_bytes());
    o.extend(b"data");
    o.extend((n as u32).to_le_bytes());
    for i in 0..l.len() {
        o.extend(l[i].to_le_bytes());
        o.extend(r[i].to_le_bytes());
    }
    fs::write(path, o).unwrap();
}

fn apply(p: &mut XParams, id: &str, v: f64) {
    let band = |p: &mut XParams, b: usize, f: &str| {
        let q = &mut p.bands[b];
        match f {
            "on" => q.on = v > 0.5,
            "solo" => q.solo = v > 0.5,
            "delta" => q.delta = v > 0.5,
            "shape" => q.shape = v.round() as i32,
            "drive" => q.drive = v,
            "bias" => q.bias = v,
            "tone" => q.tone = v,
            "hp" => q.hp = v,
            "lp" => q.lp = v,
            "sat_mix" => q.sat_mix = v,
            "sat_gain" => q.sat_gain = v,
            "thresh" => q.thresh = v,
            "ratio" => q.ratio = v,
            "attack" => q.attack = v,
            "release" => q.release = v,
            "speed" => q.speed = v,
            "knee" => q.knee = v,
            "knee_shape" => q.knee_shape = v,
            "detector" => q.detector = v,
            "ff_fb" => q.ff_fb = v,
            "gr_dist" => q.gr_dist = v,
            "comp_mix" => q.comp_mix = v,
            "comp_gain" => q.comp_gain = v,
            _ => eprintln!("unknown band field {f}"),
        }
    };
    for (b, pre) in ["lo_", "mid_", "mh_", "hi_"].iter().enumerate() {
        if let Some(f) = id.strip_prefix(pre) {
            return band(p, b, f);
        }
    }
    if let Some(rest) = id.strip_prefix("eq").and_then(|r| r.chars().next().filter(|c| c.is_ascii_digit()).map(|_| &id[2..])) {
        let i = (rest.as_bytes()[0] - b'1') as usize;
        let q = &mut p.eq.bands[i];
        match &rest[2..] {
            "on" => q.on = v > 0.5,
            "type" => q.ty = v.round() as i32,
            "freq" => q.freq = v,
            "q" => q.q = v,
            "gain" => q.gain = v,
            f => eprintln!("unknown eq field {f}"),
        }
        return;
    }
    match id {
        "x1" => p.x1 = v,
        "x2" => p.x2 = v,
        "x3" => p.x3 = v,
        "x1_slope" => p.x1_slope = v as i32,
        "x2_slope" => p.x2_slope = v as i32,
        "x3_slope" => p.x3_slope = v as i32,
        "os" => p.os = v as i32,
        "lookahead" => p.lookahead_ms = v,
        "mix" => p.mix = v,
        "out_gain" => p.out_gain = v,
        "gain_match" => p.gain_match = v > 0.5,
        "bypass" => p.bypass = v > 0.5,
        "lim_on" => p.lim_on = v > 0.5,
        "sc_on" => p.sc_on = v > 0.5,
        "sc_gain" => p.sc_gain = v,
        "sc_hp" => p.sc_hp = v,
        "sc_lp" => p.sc_lp = v,
        "sc_mix" => p.key_mix = v,
        "eq_tilt" => p.eq.tilt = v,
        "eq_pivot" => p.eq.pivot = v,
        "eq_mix" => p.eq.mix = v,
        "eq_gain" => p.eq.gain = v,
        "eq_bypass" => p.eq.bypass = v > 0.5,
        _ => eprintln!("unknown id {id}"),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut p = XParams::default();
    for line in fs::read_to_string(&a[1]).unwrap().lines() {
        let mut it = line.split_whitespace();
        if let (Some(k), Some(v)) = (it.next(), it.next()) {
            apply(&mut p, k, v.parse().unwrap());
        }
    }
    for o in &a[4..] {
        let (k, v) = o.split_once('=').unwrap();
        apply(&mut p, if k == "lim" { "lim_on" } else { k }, v.parse().unwrap());
    }
    let (sr, mut l, mut r) = read_wav(&a[2]);
    let mut e = XEngine::new(sr as f64);
    e.set_params(&p);
    let lat = e.latency() as usize;
    let n = l.len();
    l.extend(std::iter::repeat(0.0).take(lat));
    r.extend(std::iter::repeat(0.0).take(lat));
    for (cl, cr) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
        e.process(cl, cr);
    }
    // drop the latency so the render lines up with the input
    write_wav(&a[3], sr, &l[lat..lat + n], &r[lat..lat + n]);
    eprintln!("rendered {n} samples, latency {lat}");
}
