//! Golden-render tool.
//!
//!   golden render <dir>          render every scenario to <dir>/<name>.f32 + .txt (telemetry), print a summary
//!   golden diff <dirA> <dirB> [shift]
//!                                compare two render folders: max |diff|, differing samples, telemetry equality.
//!                                With `shift` (stereo samples) B is compared with its first `shift` samples dropped, for
//!                                a change that only added constant latency (A[i] must equal B[i + shift]).
//!   golden record                rewrite tests/golden/golden.txt from the current build
//!
//! Build it once against the stable tag and once against the refactor, render both, diff.
//! `render` honours GOLDEN_ONLY=<substring> to run a subset.

#[path = "../tests/golden/scen.rs"]
mod scen;

use std::fs;
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("render") => {
            let dir = Path::new(&a[2]);
            fs::create_dir_all(dir).unwrap();
            let only = std::env::var("GOLDEN_ONLY").ok();
            for sc in scen::scenarios() {
                if only.as_ref().map_or(false, |o| !sc.name.contains(o.as_str())) {
                    continue;
                }
                let o = scen::render(&sc);
                let bytes: Vec<u8> = o.audio.iter().flat_map(|s| s.to_bits().to_le_bytes()).collect();
                fs::write(dir.join(format!("{}.f32", sc.name)), bytes).unwrap();
                let txt: String = o.telem.iter().map(|(k, v)| format!("{k} {v:016x}\n")).collect();
                fs::write(dir.join(format!("{}.txt", sc.name)), txt).unwrap();
                let (ha, ht, rms, pk) = scen::summarize(&o);
                println!("{:34} audio {ha:016x} telem {ht:016x} rms {rms:.6} pk {pk:.6}", sc.name);
            }
        }
        Some("diff") => diff(Path::new(&a[2]), Path::new(&a[3]), a.get(4).and_then(|v| v.parse().ok()).unwrap_or(0)),
        Some("record") => {
            let mut out = String::from("# name audio-hash telemetry-hash rms peak (cargo run --release -p satcomp-dsp --example golden -- record)\n");
            for sc in scen::scenarios() {
                let o = scen::render(&sc);
                let (ha, ht, rms, pk) = scen::summarize(&o);
                out += &format!("{} {ha:016x} {ht:016x} {rms:.12e} {pk:.12e}\n", sc.name);
            }
            fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/golden.txt"), out).unwrap();
            println!("wrote tests/golden/golden.txt");
        }
        _ => eprintln!("usage: golden render <dir> | diff <dirA> <dirB> | record"),
    }
}

fn diff(a: &Path, b: &Path, shift: usize) {
    let mut names: Vec<String> = fs::read_dir(a)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".f32").map(String::from)))
        .collect();
    names.sort();
    let (mut same, mut total) = (0, 0);
    println!("{:34} {:>12} {:>10} {:>10}  telemetry", "scenario", "max|diff|", "differing", "samples");
    for n in &names {
        total += 1;
        let rd = |d: &Path| -> Vec<f32> {
            fs::read(d.join(format!("{n}.f32")))
                .map(|v| v.chunks_exact(4).map(|c| f32::from_bits(u32::from_le_bytes([c[0], c[1], c[2], c[3]]))).collect())
                .unwrap_or_default()
        };
        let (x, y) = (rd(a), rd(b));
        let mut max = 0.0f64;
        let mut cnt = 0usize;
        if x.len() != y.len() {
            println!("{n:34} LENGTH DIFFERS {} vs {}", x.len(), y.len());
            continue;
        }
        let sh = shift * 2;
        let (x, y) = (&x[..x.len().saturating_sub(sh)], &y[sh.min(y.len())..]);
        for (p, q) in x.iter().zip(y) {
            if p.to_bits() != q.to_bits() {
                cnt += 1;
                max = max.max((*p as f64 - *q as f64).abs());
            }
        }
        let ta = fs::read_to_string(a.join(format!("{n}.txt"))).unwrap_or_default();
        let tb = fs::read_to_string(b.join(format!("{n}.txt"))).unwrap_or_default();
        let tdiff: Vec<&str> = ta.lines().zip(tb.lines()).filter(|(p, q)| p != q).map(|(p, _)| p.split(' ').next().unwrap_or("")).collect();
        let tel = if ta == tb { "identical".to_string() } else { format!("DIFFERS in {} ({} lines; first: {})", tdiff.len(), tdiff.len(), tdiff.first().unwrap_or(&"-")) };
        if cnt == 0 && (ta == tb || shift > 0) {
            same += 1;
        }
        println!("{n:34} {max:12.3e} {cnt:10} {:10}  {tel}", x.len());
    }
    println!("\n{same}/{total} scenarios bit-identical (audio and telemetry)");
}
