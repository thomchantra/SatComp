//! Regression net for the engine: every scenario in `golden/scen.rs` must reproduce the committed fixture.
//! Bit-exact where the platform's maths library matches the recording machine; otherwise the RMS and peak must agree to
//! 1e-9 relative (a different libm can move the last bits of exp/ln/sin). Re-record deliberately with
//! `cargo run --release -p satcomp-dsp --example golden -- record` when the sound is *meant* to change.

#[path = "golden/scen.rs"]
mod scen;

#[test]
fn engine_matches_golden_fixture() {
    let fixture = include_str!("golden/golden.txt");
    let mut checked = 0;
    let mut loose = Vec::new();
    for sc in scen::scenarios() {
        let line = fixture
            .lines()
            .find(|l| l.split(' ').next() == Some(sc.name.as_str()))
            .unwrap_or_else(|| panic!("scenario {} missing from golden.txt (re-record)", sc.name));
        let f: Vec<&str> = line.split(' ').collect();
        let (ha, ht, rms, pk) = scen::summarize(&scen::render(&sc));
        let (fa, ft) = (u64::from_str_radix(f[1], 16).unwrap(), u64::from_str_radix(f[2], 16).unwrap());
        let (frms, fpk): (f64, f64) = (f[3].parse().unwrap(), f[4].parse().unwrap());
        if ha != fa || ht != ft {
            let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs().max(1e-12);
            assert!(close(rms, frms) && close(pk, fpk), "{}: output changed (rms {rms:e} vs {frms:e}, peak {pk:e} vs {fpk:e})", sc.name);
            loose.push(sc.name.clone());
        }
        checked += 1;
    }
    assert!(checked >= 100, "expected the full scenario list, ran {checked}");
    if !loose.is_empty() {
        eprintln!("note: {} scenarios matched to 1e-9 but not bit-exactly on this platform", loose.len());
    }
}

#[test]
fn render_is_deterministic() {
    let sc = &scen::scenarios()[1];
    let a = scen::summarize(&scen::render(sc));
    let b = scen::summarize(&scen::render(sc));
    assert_eq!((a.0, a.1), (b.0, b.1));
}
