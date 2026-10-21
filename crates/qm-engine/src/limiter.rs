/// Output soft limiter: transparent below `knee`, smoothly saturating to `ceiling` above it. No look-ahead, no delay.
#[derive(Copy, Clone)]
pub struct Limiter {
    knee: f32,
    ceiling: f32,
}

impl Limiter {
    /// Levels in linear amplitude (e.g. 0.7 and 0.89 = -1 dBFS).
    pub fn new(knee: f32, ceiling: f32) -> Self {
        assert!(0.0 < knee && knee < ceiling);
        Self { knee, ceiling }
    }

    #[inline]
    pub fn process(&self, x: f32) -> f32 {
        let a = x.abs();
        if a <= self.knee {
            x
        } else {
            let room = self.ceiling - self.knee;
            x.signum() * (self.knee + room * ((a - self.knee) / room).tanh())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_below_knee_and_bounded_above() {
        let l = Limiter::new(0.7, 0.89);
        assert_eq!(l.process(0.3), 0.3);
        assert!(l.process(5.0) <= 0.89 && l.process(-5.0) >= -0.89);
        assert!(l.process(0.8) > 0.7 && l.process(0.8) < 0.89);
    }

    #[test]
    fn monotonic() {
        let l = Limiter::new(0.7, 0.89);
        let mut prev = -1.0;
        for i in 0..400 {
            let y = l.process(i as f32 * 0.01);
            assert!(y >= prev);
            prev = y;
        }
    }
}
