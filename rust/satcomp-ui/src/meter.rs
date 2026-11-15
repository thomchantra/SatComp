//! Playback of per-bucket animation values (the saturator's live dot, the compressor's level dots, the gain-difference bar, the
//! morph position) that the audio thread writes into a ring, one entry per 2 ms of audio.
//!
//! Publishing them once per host block (an atomic `store`) ties the animation to the moments at which blocks happen to arrive,
//! and those are bursty: small buffers beat against the frame rate, and PulseAudio / large buffers deliver several blocks at once
//! and then nothing for tens of milliseconds, which shows as an irregular, "drunken" motion. The meters that never showed it
//! accumulate peaks between frames. This reader instead replays the buckets on a `ScrollClock` (constant speed, audio time), and
//! applies the decay of the peak channels itself, bucket by bucket.
//!
//! Two kinds of channel: `Peak(tau)` buckets carry the bucket's maximum and the reader keeps a decaying maximum with time
//! constant `tau` seconds (the same law the engines used per block), `State` buckets carry a value that is shown as it is.

use crate::scroll::ScrollClock;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Peak(f64),
    State,
}

/// at most this many buckets are replayed in one frame (a re-sync after a gap); the ring holds 4096
const MAX_CATCH_UP: usize = 3000;

pub struct MeterReader {
    clk: ScrollClock,
    kinds: Vec<Kind>,
    val: Vec<f32>,
    /// next bucket to replay
    next: usize,
    init: bool,
}

impl MeterReader {
    pub fn new(kinds: Vec<Kind>) -> Self {
        let n = kinds.len();
        Self { clk: ScrollClock::new(), kinds, val: vec![0.0; n], next: 0, init: false }
    }

    /// The shown value of channel `c` (0 for a channel that does not exist).
    pub fn value(&self, c: usize) -> f32 {
        self.val.get(c).copied().unwrap_or(0.0)
    }

    /// Call once per frame. `head` = buckets written so far, `dt` = seconds since the last call, `rate` = buckets per second,
    /// `get(i, c)` = channel `c` of bucket `i`.
    pub fn update(&mut self, head: usize, dt: f64, rate: f64, get: impl Fn(usize, usize) -> f32) {
        let end = self.clk.update(head, dt, rate).floor().max(0.0) as usize;
        if !self.init || end < self.next {
            // first frame, or the ring restarted: show the newest bucket as it is
            self.init = true;
            self.next = end.saturating_sub(1);
        }
        if end - self.next > MAX_CATCH_UP {
            self.next = end - MAX_CATCH_UP;
        }
        let step = 1.0 / rate;
        let decay: Vec<f32> = self.kinds.iter().map(|k| if let Kind::Peak(tau) = *k { (-step / tau.max(1e-3)).exp() as f32 } else { 0.0 }).collect();
        for i in self.next..end {
            for c in 0..self.kinds.len() {
                let b = get(i, c);
                let b = if b.is_finite() { b } else { 0.0 };
                self.val[c] = match self.kinds[c] {
                    Kind::Peak(_) => b.max(0.0).max(self.val[c] * decay[c]),
                    Kind::State => b,
                };
            }
        }
        self.next = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 500.0;
    const SR: f64 = 44100.0;

    /// A drum-like audio-time signal: bucket peak of a decaying hit every 0.5 s.
    fn bucket_peak(i: usize) -> f32 {
        let t = i as f64 / RATE;
        (0.9 * (-(t % 0.5) / 0.06).exp()) as f32
    }

    /// What a host does: `pattern` = (blocks delivered, in how many ms), repeated, e.g. PulseAudio bursts. Returns per-frame
    /// (frame time, newest bucket written, the value a per-block store would have shown, the reader's value).
    fn run(block: usize, burst: usize, secs: f64) -> Vec<(f64, f32, f32)> {
        let mut r = MeterReader::new(vec![Kind::Peak(0.15)]);
        let ring: std::cell::RefCell<Vec<f32>> = Default::default();
        let (mut t, mut frame) = (0.0f64, 0u64);
        let mut written = 0usize; // buckets in the ring
        let mut blocks = 0usize; // blocks the audio thread has produced
        let mut env_store = 0.0f32; // the old per-block published decaying peak
        let mut out = Vec::new();
        while t < secs {
            let dt = 1.0 / 60.0 + (((frame * 7919) % 11) as f64 / 11.0 - 0.5) * 0.004;
            t += dt;
            // the audio thread runs in bursts of `burst` blocks: it wakes every burst * block / SR and writes them back to back
            let due = (t * SR / (block * burst) as f64).floor() as usize * burst;
            while blocks < due {
                blocks += 1;
                let upto = (blocks * block) as f64 / SR * RATE;
                let mut blk_peak = 0.0f32;
                while (written as f64) < upto {
                    let v = bucket_peak(written);
                    ring.borrow_mut().push(v);
                    blk_peak = blk_peak.max(v);
                    written += 1;
                }
                env_store = blk_peak.max(env_store * (-(block as f64) / SR / 0.15).exp() as f32);
            }
            r.update(written, dt, RATE, |i, _| ring.borrow()[i]);
            out.push((t, env_store, r.value(0)));
            frame += 1;
        }
        out
    }

    /// Standard deviation (ms) of the delay between a hit in audio time (one every 0.5 s) and the frame on which the shown value
    /// first rises past half its height: the steadier it is, the steadier the animation looks.
    fn latency_jitter_ms(frames: &[(f64, f32)]) -> f64 {
        let mut lat = Vec::new();
        let mut armed = true;
        for &(t, v) in frames {
            if v < 0.15 {
                armed = true;
            } else if armed && v > 0.45 {
                armed = false;
                // delay after the nearest hit; the shift keeps a crossing that happens just before the hit's time from wrapping
                lat.push((((t + 0.1) % 0.5) - 0.1) * 1000.0);
            }
        }
        let n = lat.len() as f64;
        let m = lat.iter().sum::<f64>() / n;
        (lat.iter().map(|l| (l - m) * (l - m)).sum::<f64>() / n).sqrt()
    }

    fn lat_count(frames: &[(f64, f32)]) -> usize {
        let mut n = 0;
        let mut armed = true;
        for &(_, v) in frames {
            if v < 0.15 {
                armed = true;
            } else if armed && v > 0.45 {
                armed = false;
                n += 1;
            }
        }
        n
    }

    #[test]
    fn bursty_delivery_no_longer_shows_in_the_motion() {
        // 512-sample blocks alone (the REAPER test setup), delivered 4 at a time (PulseAudio-like 46 ms bursts), 1024 x 2, 2048
        for (block, burst) in [(512usize, 1usize), (512, 4), (1024, 2), (2048, 1), (2048, 2)] {
            let f = run(block, burst, 12.0);
            let tail = &f[180..];
            let old: Vec<(f64, f32)> = tail.iter().map(|x| (x.0, x.1)).collect();
            let new: Vec<(f64, f32)> = tail.iter().map(|x| (x.0, x.2)).collect();
            let (jo, jn) = (latency_jitter_ms(&old), latency_jitter_ms(&new));
            eprintln!("block {block} x {burst}: onset delay jitter old {jo:.1} ms, new {jn:.1} ms");
            // frame quantisation alone is about 5 ms; bursts add up to a whole burst on top for the old per-block value
            assert!(lat_count(&new) >= 15 && jn < 8.0 && jn <= jo + 2.0, "block {block} x {burst}: jitter old {jo:.1} ms new {jn:.1} ms");
            let top = new.iter().map(|x| x.1).fold(0.0f32, f32::max);
            assert!(top > 0.7 && new.iter().all(|x| x.1.is_finite() && x.1 >= 0.0 && x.1 <= 0.95), "block {block} x {burst}: peak {top}");
        }
    }

    #[test]
    fn peak_channels_decay_like_the_engine_did_and_state_channels_pass_through() {
        let mut r = MeterReader::new(vec![Kind::Peak(0.15), Kind::State]);
        // a single full-scale bucket, then silence; the state channel steps 0 -> 5 at bucket 400
        let get = |i: usize, c: usize| if c == 0 { if i == 10 { 1.0 } else { 0.0 } } else if i >= 400 { 5.0 } else { 0.0 };
        let mut head = 0;
        for _ in 0..200 {
            head += 8;
            r.update(head, 1.0 / 60.0, RATE, get);
        }
        // by now ~1.5 s of buckets have gone by: the peak has decayed to nothing, the state sits at its last value
        assert!(r.value(0) < 1e-4, "{}", r.value(0));
        assert_eq!(r.value(1), 5.0);
        // the decay constant: one tau after the hit the value is 1/e
        let mut r = MeterReader::new(vec![Kind::Peak(0.15)]);
        let g = |i: usize, _c: usize| if i == 20 { 1.0 } else { 0.0 };
        let mut head = 0;
        let mut at_tau = None;
        for f in 0..120 {
            head += 8;
            r.update(head, 1.0 / 60.0, RATE, g);
            // the reader is a margin behind the head: compare against the bucket it has reached
            if f == 60 && at_tau.is_none() {
                at_tau = Some(r.value(0));
            }
        }
        assert!(at_tau.unwrap() < 0.1 && at_tau.unwrap() > 0.0, "{:?}", at_tau);
    }

    #[test]
    fn a_restarted_ring_nonsense_values_and_a_missing_channel_are_survived() {
        let mut r = MeterReader::new(vec![Kind::Peak(0.15)]);
        let mut head = 0;
        for _ in 0..100 {
            head += 8;
            r.update(head, 1.0 / 60.0, RATE, |_, _| f32::NAN);
        }
        assert_eq!(r.value(0), 0.0);
        assert_eq!(r.value(7), 0.0);
        for _ in 0..50 {
            head += 8;
            r.update(head, 1.0 / 60.0, RATE, |_, _| 0.5);
        }
        assert!(r.value(0) > 0.4);
        // a new engine: the ring starts over
        r.update(3, 1.0 / 60.0, RATE, |_, _| 0.2);
        r.update(11, 1.0 / 60.0, RATE, |_, _| 0.2);
        assert!(r.value(0).is_finite());
        // a long gap re-syncs without replaying thousands of buckets of nothing
        r.update(1_000_000, 1.0 / 60.0, RATE, |_, _| 0.1);
        assert!(r.value(0).is_finite());
    }
}
