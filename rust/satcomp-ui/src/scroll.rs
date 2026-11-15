//! Read position for a scrolling view of a ring that the audio thread fills in host-block chunks.
//!
//! The audio side delivers buckets in bursts (one host block at a time: 3 ms on a small buffer, 20-45 ms on the 1024-2048
//! sample buffers common on Linux, more with a host's anticipative processing), while the editor draws at its own frame rate.
//! Easing the read position toward the newest bucket makes the scroll speed follow that staircase: fine for tiny blocks,
//! a visible speed wobble (stutter) for big ones. Instead the position runs at the nominal bucket rate as a free clock and
//! is only nudged toward the head with a weak proportional term, so the speed stays within a few percent of constant
//! whatever the block size. It keeps a margin of one observed chunk behind the head, so it never runs into buckets that
//! are not written yet, and it never moves backwards except on a re-sync (first frame, a ring that restarted, or a gap
//! of more than half a second, such as a hidden editor).

/// how hard the position is pulled toward its target, per second: with a chunk of N buckets the speed wobbles by about
/// +-N/2 buckets/s (2 % at 500 buckets/s for a 2048-sample block at 48 kHz)
const PULL: f64 = 1.0;
/// re-sync when the position is this many seconds of buckets away from the target
const RESYNC_S: f64 = 0.5;
/// spare buckets on top of the observed chunk
const SPARE: f64 = 2.0;
/// integral gain (per second squared): absorbs a real mismatch between the nominal and the delivered rate (bucket rounding,
/// a host clock that drifts); with PULL = 1 the loop is just over critically damped
const TRIM_GAIN: f64 = 0.2;
/// the integral may correct the rate by at most this share
const TRIM_MAX: f64 = 0.05;
/// margin used until a chunk size has been observed (a fresh editor), in seconds of buckets
const START_S: f64 = 0.05;
/// a frame that delivers more than this many seconds of buckets is a gap (hidden editor, stopped host), not a chunk
const CHUNK_MAX_S: f64 = 0.25;

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollClock {
    pos: f64,
    last_head: usize,
    /// peak-held number of buckets that arrived in one frame (decays over a few seconds)
    chunk: f64,
    /// buckets kept between the read position and the head, follows `chunk + SPARE` slowly
    margin: f64,
    /// integral rate correction, buckets per second
    trim: f64,
    init: bool,
}

impl ScrollClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget the position; the next `update` re-syncs to the head.
    pub fn reset(&mut self) {
        self.init = false;
    }

    /// Advance by `dt` seconds. `head` = buckets written so far (the newest is `head - 1`), `rate` = nominal buckets per
    /// second. Returns the read position in buckets: everything below it is written. Non-decreasing between re-syncs.
    pub fn update(&mut self, head: usize, dt: f64, rate: f64) -> f64 {
        let dt = if dt.is_finite() { dt.clamp(0.0, 1.0) } else { 0.0 };
        let h = head as f64;
        let restarted = head < self.last_head;
        let arrived = if restarted { 0.0 } else { (head - self.last_head) as f64 };
        self.last_head = head;
        if arrived <= rate * CHUNK_MAX_S {
            self.chunk = arrived.max(self.chunk * (-dt / 3.0).exp());
        }
        let want = self.chunk + SPARE;
        if !self.init || restarted {
            // nothing observed yet: start well behind the head, the margin then settles down to the real chunk size
            self.margin = want.max(rate * START_S);
            self.trim = 0.0;
        } else {
            self.margin += (want - self.margin) * (1.0 - (-dt).exp());
        }
        let target = h - self.margin;
        if !self.init || restarted || (target - self.pos).abs() > rate * RESYNC_S {
            self.init = true;
            self.pos = target.max(0.0);
            return self.pos;
        }
        // the error is taken where this frame's step lands, so the position settles on the target after advancing (not one
        // frame's step past it)
        let err = target - (self.pos + (rate + self.trim) * dt);
        let v = (rate + self.trim + err * PULL).max(0.0);
        let next = self.pos + v * dt;
        if next < h {
            // integrate only while not held at the head (a stopped host must not wind the trim up)
            self.trim = (self.trim + err * TRIM_GAIN * dt).clamp(-rate * TRIM_MAX, rate * TRIM_MAX);
            self.pos = next;
        } else {
            self.pos = h;
        }
        self.pos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 500.0; // 2 ms buckets

    /// Simulated session: buckets arrive in chunks of `block` samples at 48 kHz (96 samples per bucket), frames come at
    /// `fps` with a little deterministic jitter. Returns per-frame (time, head, position).
    fn run(block: usize, fps: f64, secs: f64) -> Vec<(f64, usize, f64)> {
        let mut c = ScrollClock::new();
        let (mut t, mut samples, mut out) = (0.0f64, 0usize, Vec::new());
        let mut frame = 0u64;
        while t < secs {
            let jitter = ((frame * 7919) % 11) as f64 / 11.0 - 0.5; // +-0.5 of 3 ms
            let dt = 1.0 / fps + jitter * 0.003;
            t += dt;
            // the audio thread has delivered every whole block whose end lies before t
            while (samples + block) as f64 / 48000.0 <= t {
                samples += block;
            }
            let head = samples / 96;
            out.push((t, head, c.update(head, dt, RATE)));
            frame += 1;
        }
        out
    }

    #[test]
    fn the_speed_stays_nearly_constant_with_big_host_blocks() {
        for block in [64usize, 256, 1024, 2048, 4096] {
            let f = run(block, 60.0, 6.0);
            // after a 2 s settle: speed per frame relative to the nominal rate
            let speeds: Vec<f64> = f.windows(2).skip(120).map(|w| (w[1].2 - w[0].2) / (w[1].0 - w[0].0) / RATE).collect();
            let (lo, hi) = speeds.iter().fold((f64::MAX, 0.0f64), |(l, h), &s| (l.min(s), h.max(s)));
            // 4096 samples = 85 ms chunks: still within +-10 %; the old easing swung about +-20 % at 2048
            let tol = if block >= 4096 { 0.1 } else { 0.05 };
            assert!(lo > 1.0 - tol && hi < 1.0 + tol, "block {block}: speed {lo:.3} .. {hi:.3}");
        }
    }

    #[test]
    fn it_never_reads_unwritten_buckets_or_goes_backwards() {
        for block in [128usize, 1024, 2048, 8192] {
            let f = run(block, 50.0, 5.0);
            for w in f.windows(2) {
                assert!(w[1].2 >= w[0].2, "block {block}: went back {} -> {}", w[0].2, w[1].2);
            }
            assert!(f.iter().all(|&(_, h, p)| p <= h as f64), "block {block}: read ahead of the head");
        }
    }

    #[test]
    fn it_stops_when_the_audio_stops_and_resyncs_after_a_gap() {
        let mut c = ScrollClock::new();
        let mut head = 0usize;
        for _ in 0..300 {
            head += 8;
            c.update(head, 1.0 / 60.0, RATE);
        }
        // audio stops: the position runs up to the head and holds there
        let mut p = 0.0;
        for _ in 0..200 {
            p = c.update(head, 1.0 / 60.0, RATE);
        }
        assert!((head as f64 - p).abs() < 1e-9, "{p} vs {head}");
        // two seconds later the head is far ahead: re-sync instead of a long catch-up
        head += 1000;
        let p = c.update(head, 1.0 / 60.0, RATE);
        assert!(head as f64 - p < 40.0, "{p} vs {head}"); // within the 50 ms start margin
        // a ring that restarts from zero (a new engine) re-syncs too
        let p = c.update(5, 1.0 / 60.0, RATE);
        assert!(p <= 5.0);
    }

    #[test]
    fn nonsense_time_steps_do_not_break_it() {
        let mut c = ScrollClock::new();
        for (i, dt) in [0.0, -1.0, f64::NAN, f64::INFINITY, 1.0 / 60.0].iter().enumerate() {
            let p = c.update(100 + i * 8, *dt, RATE);
            assert!(p.is_finite() && p <= (100 + i * 8) as f64);
        }
    }
}
