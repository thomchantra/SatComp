//! Power-of-two delay ring (const-generic length). `advance()` once per sample, then optionally `put()`, then `tap(d)` reads `d` samples back
//! (`tap(0)` is the sample just put). Writing is separate from advancing on purpose: some rings are only written while a
//! feature is active but must keep moving every sample so the read index stays aligned with the other rings.

/// `N` must be a power of two and larger than any delay that will be tapped. Fixed size (boxed array) so the masked
/// index needs no bounds check.
#[derive(Clone)]
pub struct Delay<const N: usize> {
    buf: Box<[f64; N]>,
    pos: usize,
}

impl<const N: usize> Default for Delay<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Delay<N> {
    const MASK: usize = N - 1;

    pub fn new() -> Self {
        assert!(N.is_power_of_two());
        Self { buf: vec![0.0; N].into_boxed_slice().try_into().expect("length is N"), pos: 0 }
    }

    #[inline]
    pub fn advance(&mut self) {
        self.pos = (self.pos + 1) & Self::MASK;
    }

    #[inline]
    pub fn put(&mut self, v: f64) {
        self.buf[self.pos & Self::MASK] = v;
    }

    #[inline]
    pub fn tap(&self, d: usize) -> f64 {
        self.buf[(self.pos + N - d) & Self::MASK]
    }

    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
    }
}

/// A left/right pair of rings.
#[derive(Clone)]
pub struct StereoDelay<const N: usize>(pub [Delay<N>; 2]);

impl<const N: usize> Default for StereoDelay<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> StereoDelay<N> {
    pub fn new() -> Self {
        Self([Delay::new(), Delay::new()])
    }
    #[inline]
    pub fn advance(&mut self) {
        self.0[0].advance();
        self.0[1].advance();
    }
    #[inline]
    pub fn put(&mut self, v: [f64; 2]) {
        self.0[0].put(v[0]);
        self.0[1].put(v[1]);
    }
    #[inline]
    pub fn tap(&self, d: usize) -> [f64; 2] {
        [self.0[0].tap(d), self.0[1].tap(d)]
    }
    pub fn clear(&mut self) {
        self.0[0].clear();
        self.0[1].clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_reads_back_the_delayed_sample() {
        let mut d = Delay::<8>::new();
        for i in 0..20 {
            d.advance();
            d.put(i as f64);
            assert_eq!(d.tap(0), i as f64);
            if i >= 3 {
                assert_eq!(d.tap(3), (i - 3) as f64);
            }
        }
    }

    #[test]
    fn advance_without_put_keeps_stale_value_in_place() {
        let mut d = Delay::<8>::new();
        d.advance();
        d.put(5.0);
        d.advance();
        d.advance();
        assert_eq!(d.tap(2), 5.0);
    }
}
