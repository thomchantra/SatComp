//! FFT spectrum analyser for the viewports: 4096-point Hann window over the newest tap samples, smoothed in dB (fast attack,
//! slower release), then resampled to log-spaced screen columns with the usual +4.5 dB/oct tilt so pink noise reads flat.

use crate::fft::fft;

pub struct Spectrum {
    win: Vec<f32>,
    /// smoothed magnitude per bin in dB (2049 bins)
    pub db: Vec<f32>,
    t: f64,
}

impl Default for Spectrum {
    fn default() -> Self {
        Self::new()
    }
}

impl Spectrum {
    pub const N: usize = 4096;

    pub fn new() -> Self {
        let win = (0..Self::N).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / Self::N as f32).cos()).collect();
        Self { win, db: vec![-140.0; Self::N / 2 + 1], t: 0.0 }
    }

    /// Re-base the update clock (the egui clock restarts when the editor window is reopened).
    pub fn rebase(&mut self, now: f64) {
        self.t = now;
    }

    /// Analyse at most every 30 ms. `head` = write position of the tap ring, `get(i)` reads sample `i` of it.
    pub fn update(&mut self, now: f64, head: usize, get: impl Fn(usize) -> f32) {
        if now - self.t < 0.03 {
            return;
        }
        self.t = now;
        let mut re = vec![0.0f32; Self::N];
        let mut im = vec![0.0f32; Self::N];
        let ix = head.wrapping_sub(Self::N);
        for i in 0..Self::N {
            re[i] = get(ix.wrapping_add(i)) * self.win[i];
        }
        fft(&mut re, &mut im);
        for k in 0..=Self::N / 2 {
            let db = 8.6858896 * ((re[k] * re[k] + im[k] * im[k]).sqrt() * 0.0009765625 + 0.0000001).ln();
            let sp = &mut self.db[k];
            *sp += (db - *sp) * if db > *sp { 0.6 } else { 0.12 };
        }
    }

    /// Level in dB (tilt-compensated) for `n` log-spaced columns from 20 Hz to 20 kHz.
    pub fn columns(&self, n: usize, sample_rate: f32) -> Vec<f32> {
        let bw = sample_rate / Self::N as f32;
        let top = (Self::N / 2) as f32;
        (0..n)
            .map(|i| {
                let flo = 20.0 * (i as f32 / n as f32 * 6.9077553).exp();
                let fhi = 20.0 * ((i + 1) as f32 / n as f32 * 6.9077553).exp();
                let bl = (flo / bw).min(top - 2.0);
                let bh = (fhi / bw).min(top - 1.0);
                let mut v;
                if bh - bl < 1.0 {
                    let k = bl.floor() as usize;
                    let fr = bl - k as f32;
                    v = self.db[k] + (self.db[k + 1] - self.db[k]) * fr;
                } else {
                    v = -140.0;
                    let (mut k, kk) = (bl.floor() as usize, bh.floor() as usize);
                    let mut cnt = 0;
                    while k <= kk && cnt < 512 {
                        v = v.max(self.db[k]);
                        k += 1;
                        cnt += 1;
                    }
                }
                v + 6.4921 * ((flo * fhi).sqrt() / 1000.0).ln()
            })
            .collect()
    }
}

/// Screen x for a frequency on the 20 Hz .. 20 kHz log axis.
pub fn freq_to_x(f: f32, x0: f32, w: f32) -> f32 {
    x0 + (f.max(20.0) / 20.0).ln() / 6.9077553 * w
}

/// Frequency for a screen x on the 20 Hz .. 20 kHz log axis.
pub fn x_to_freq(x: f32, x0: f32, w: f32) -> f32 {
    20.0 * (((x - x0) / w).clamp(0.0, 1.0) * 6.9077553).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_peaks_at_its_log_column() {
        let sr = 48000.0f32;
        let mut sp = Spectrum::new();
        let sig: Vec<f32> = (0..8192).map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr).sin()).collect();
        for k in 0..30 {
            sp.update(1.0 + k as f64 * 0.05, 8192, |i| sig[i % 8192]);
        }
        let cols = sp.columns(400, sr);
        let peak = cols.iter().cloned().enumerate().fold((0, -999.0f32), |a, (i, v)| if v > a.1 { (i, v) } else { a }).0;
        let expect = ((1000.0f32 / 20.0).ln() / 6.9077553 * 400.0) as i32;
        assert!((peak as i32 - expect).abs() <= 3, "{peak} vs {expect}");
    }

    #[test]
    fn axis_mapping_round_trips() {
        for f in [20.0, 100.0, 1000.0, 20000.0] {
            let x = freq_to_x(f, 10.0, 400.0);
            assert!((x_to_freq(x, 10.0, 400.0) - f).abs() / f < 1e-3);
        }
    }
}
