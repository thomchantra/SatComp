//! Smooth display of a value that the audio thread publishes once per host block.
//!
//! A level, a position or a difference that is `store`d per block reaches the editor as a staircase: the same number for
//! two or three frames, then a jump. With 64-256 sample blocks nobody sees it; with the 1024-4096 sample blocks common on
//! Linux (and with some hosts' anticipative processing everywhere) the dots, bars and readouts of an animation advance at
//! 10-25 Hz and irregularly ("drunken"). The meters that never showed it (VU, GR, spectrum) accumulate peaks between frames
//! instead.
//!
//! `Glide` turns the staircase into a polyline: every time the published value changes it starts a linear segment from
//! what is shown now to the new value and plays it over the observed block period (an average of the intervals between
//! changes, a little stretched so a late block does not leave a hold). The shown value is continuous, never overshoots,
//! and trails the raw one by about a block, which is below what an animation shows. Calling `step` twice in the same frame
//! returns the same number.

/// share by which a segment is stretched past the observed period
const STRETCH: f64 = 1.15;
/// intervals outside this range (seconds) are not block periods: a pause, or two changes in a row
const MIN_IV: f64 = 0.004;
const MAX_IV: f64 = 0.3;

#[derive(Clone, Copy, Debug)]
pub struct Glide {
    from: f32,
    to: f32,
    raw: f32,
    t0: f64,
    dur: f64,
    t_change: f64,
    period: f64,
    init: bool,
}

impl Default for Glide {
    fn default() -> Self {
        Self { from: 0.0, to: 0.0, raw: 0.0, t0: 0.0, dur: 0.03, t_change: 0.0, period: 0.03, init: false }
    }
}

impl Glide {
    pub fn new() -> Self {
        Self::default()
    }

    /// The value to show at time `now` (seconds) for the published value `raw`. A non-finite `raw` reads as 0.
    pub fn step(&mut self, raw: f32, now: f64) -> f32 {
        let raw = if raw.is_finite() { raw } else { 0.0 };
        if !self.init {
            *self = Self { from: raw, to: raw, raw, t0: now, t_change: now, init: true, ..Self::default() };
            return raw;
        }
        // the egui clock restarts from 0 when the editor is reopened: re-base instead of waiting for it to catch up
        if now < self.t0 || now < self.t_change {
            self.t0 = now;
            self.t_change = now;
        }
        if raw != self.raw {
            let shown = self.value(now);
            let iv = now - self.t_change;
            if (MIN_IV..MAX_IV).contains(&iv) {
                self.period += (iv - self.period) * 0.2;
            }
            self.t_change = now;
            self.from = shown;
            self.to = raw;
            self.raw = raw;
            self.t0 = now;
            self.dur = (self.period * STRETCH).clamp(0.016, 0.3);
        }
        self.value(now)
    }

    fn value(&self, now: f64) -> f32 {
        let f = ((now - self.t0) / self.dur).clamp(0.0, 1.0) as f32;
        self.from + (self.to - self.from) * f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulated session: a peak envelope that is published once per host block (`block` samples at 48 kHz), the editor
    /// drawing at ~60 fps with jitter. Returns per-frame (raw, shown).
    fn run(block: usize, secs: f64) -> Vec<(f32, f32)> {
        let mut g = Glide::new();
        let (mut t, mut frame, mut out) = (0.0f64, 0u64, Vec::new());
        while t < secs {
            let dt = 1.0 / 60.0 + (((frame * 7919) % 11) as f64 / 11.0 - 0.5) * 0.004;
            t += dt;
            let blocks = (t * 48000.0 / block as f64).floor();
            let bt = blocks * block as f64 / 48000.0;
            // envelope of a 3 Hz tremolo, decaying peak per block: what the engine would store
            let raw = (0.5 + 0.45 * (std::f64::consts::TAU * 3.0 * bt).sin()) as f32;
            out.push((raw, g.step(raw, t)));
            frame += 1;
        }
        out
    }

    #[test]
    fn a_blocky_staircase_becomes_a_smooth_line() {
        for block in [1024usize, 2048, 4096] {
            let f = run(block, 5.0);
            let tail = &f[120..];
            // stutter = jerky motion: mean |second difference| relative to mean |step|, raw against shown
            let jerk = |sel: &dyn Fn(&(f32, f32)) -> f32| {
                let v: Vec<f32> = tail.iter().map(sel).collect();
                let d1 = v.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f32>();
                let d2 = v.windows(3).map(|w| (w[2] - 2.0 * w[1] + w[0]).abs()).sum::<f32>();
                d2 / d1
            };
            let (raw_j, shown_j) = (jerk(&|f| f.0), jerk(&|f| f.1));
            assert!(shown_j < 0.7 * raw_j, "block {block}: jerk shown {shown_j:.2} vs raw {raw_j:.2}");
            // and the display is never stuck for long while the raw value moves: no run of identical frames
            let mut run_len = 0;
            let mut worst = 0;
            for w in tail.windows(2) {
                if w[1].1 == w[0].1 {
                    run_len += 1;
                    worst = worst.max(run_len);
                } else {
                    run_len = 0;
                }
            }
            assert!(worst <= 1, "block {block}: {worst} identical frames in a row");
        }
    }

    #[test]
    fn it_stays_inside_the_range_of_the_input_and_settles_on_it() {
        let f = run(2048, 5.0);
        assert!(f.iter().all(|&(_, s)| (0.04..=0.96).contains(&s)));
        let mut g = Glide::new();
        g.step(0.0, 0.0);
        g.step(1.0, 0.1);
        assert!((g.step(1.0, 0.4) - 1.0).abs() < 1e-6, "settles");
        assert!((g.step(1.0, 0.4) - g.step(1.0, 0.4)).abs() == 0.0, "idempotent within a frame");
    }

    #[test]
    fn a_fast_block_rate_adds_no_visible_lag_and_a_stopped_host_holds() {
        // a new value every frame (small blocks): the shown value is within a frame of the raw one
        let mut g = Glide::new();
        let mut worst = 0.0f32;
        for i in 0..300 {
            let t = i as f64 / 60.0;
            let raw = (t * 2.0).sin() as f32 * 0.5 + 0.5;
            let s = g.step(raw, t);
            if i > 30 {
                worst = worst.max((s - raw).abs());
            }
        }
        assert!(worst < 0.08, "lag {worst}");
        // the host stops: the value holds and does not drift
        let hold = g.step(0.3, 10.0);
        let later = g.step(0.3, 20.0);
        assert!((later - 0.3).abs() < 1e-6 && hold.is_finite());
    }

    #[test]
    fn a_reopened_editor_and_garbage_input_are_survived() {
        let mut g = Glide::new();
        for i in 0..50 {
            g.step(i as f32 * 0.01, 100.0 + i as f64 * 0.016);
        }
        // the clock restarts at 0
        let v = g.step(0.9, 0.01);
        assert!(v.is_finite());
        assert!((g.step(0.9, 0.5) - 0.9).abs() < 1e-6);
        assert_eq!(Glide::new().step(f32::NAN, 1.0), 0.0);
        assert!(g.step(f32::INFINITY, 1.0).is_finite());
    }
}
