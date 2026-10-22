//! Clock-drift compensation between the capture device and the virtual cable.
//!
//! The two devices run on different clocks, so the ring between them slowly fills or drains. Instead of dropping or
//! repeating samples (audible clicks), the render side reads the ring through a linear-interpolating resampler whose
//! ratio is nudged by a tiny amount (well under 0.5%, inaudible) to hold the ring at a target fill.

use std::collections::VecDeque;

/// Anything samples can be pulled from one at a time.
pub trait Source {
    fn pop(&mut self) -> Option<f32>;
    /// Samples currently available.
    fn available(&self) -> usize;
}

impl Source for VecDeque<f32> {
    fn pop(&mut self) -> Option<f32> {
        self.pop_front()
    }
    fn available(&self) -> usize {
        self.len()
    }
}

/// Maximum speed deviation applied for drift compensation.
pub const MAX_DEVIATION: f64 = 0.005;

/// Ratio (input samples consumed per output sample) that steers `fill` toward `target`.
/// Above target consume slightly faster; below target slightly slower. Proportional, clamped to +/-`MAX_DEVIATION`.
pub fn ratio_for_fill(fill: usize, target: usize) -> f64 {
    if target == 0 {
        return 1.0;
    }
    let err = (fill as f64 - target as f64) / target as f64;
    (1.0 + 0.01 * err).clamp(1.0 - MAX_DEVIATION, 1.0 + MAX_DEVIATION)
}

pub struct DriftResampler {
    s0: f32,
    s1: f32,
    frac: f64,
    primed: bool,
}

impl Default for DriftResampler {
    fn default() -> Self {
        Self::new()
    }
}

impl DriftResampler {
    pub fn new() -> Self {
        Self { s0: 0.0, s1: 0.0, frac: 0.0, primed: false }
    }

    /// Forget history (use after an underrun so the next block starts cleanly).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Fill `out` from `src` at `ratio`. Returns how many output samples are real; the rest are zero-filled
    /// (the caller counts that as an underrun).
    pub fn process(&mut self, out: &mut [f32], src: &mut impl Source, ratio: f64) -> usize {
        if !self.primed {
            match (src.pop(), src.pop()) {
                (Some(a), Some(b)) => {
                    self.s0 = a;
                    self.s1 = b;
                    self.primed = true;
                }
                _ => {
                    out.fill(0.0);
                    return 0;
                }
            }
        }
        for (i, o) in out.iter_mut().enumerate() {
            *o = self.s0 + (self.s1 - self.s0) * self.frac as f32;
            self.frac += ratio;
            while self.frac >= 1.0 {
                match src.pop() {
                    Some(next) => {
                        self.s0 = self.s1;
                        self.s1 = next;
                        self.frac -= 1.0;
                    }
                    None => {
                        out[i + 1..].fill(0.0);
                        // Hold position; next call re-primes from scratch so there is no stale-sample click.
                        self.primed = false;
                        return i + 1;
                    }
                }
            }
        }
        out.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(n: usize, hz: f32) -> VecDeque<f32> {
        (0..n).map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / 48_000.0).sin() * 0.5).collect()
    }

    #[test]
    fn unity_ratio_is_a_one_sample_delayed_copy() {
        let src_data = sine(2000, 300.0);
        let mut src = src_data.clone();
        let mut r = DriftResampler::new();
        let mut out = vec![0.0; 480];
        assert_eq!(r.process(&mut out, &mut src, 1.0), 480);
        for i in 0..480 {
            assert!((out[i] - src_data[i]).abs() < 1e-6, "sample {i}");
        }
    }

    #[test]
    fn faster_ratio_consumes_proportionally_more_input() {
        let mut src = sine(100_000, 300.0);
        let mut r = DriftResampler::new();
        let mut out = vec![0.0; 48_000];
        r.process(&mut out, &mut src, 1.001);
        let consumed = 100_000 - src.len();
        assert!((consumed as i64 - 48_048).abs() <= 3, "consumed {consumed}");
    }

    #[test]
    fn output_stays_smooth_while_ratio_changes() {
        let mut src = sine(60_000, 440.0);
        let mut r = DriftResampler::new();
        let mut prev = 0.0f32;
        let mut max_step = 0.0f32;
        for block in 0..50 {
            let mut out = vec![0.0; 480];
            let ratio = 1.0 + 0.004 * ((block as f64) / 8.0).sin();
            assert_eq!(r.process(&mut out, &mut src, ratio), 480);
            for v in out {
                max_step = max_step.max((v - prev).abs());
                prev = v;
            }
        }
        // a 440 Hz sine at amplitude 0.5 moves at most ~0.03 per sample; a click would be far larger
        assert!(max_step < 0.05, "max step {max_step}");
    }

    #[test]
    fn underrun_zero_fills_and_recovers_cleanly() {
        let mut src: VecDeque<f32> = sine(300, 300.0);
        let mut r = DriftResampler::new();
        let mut out = vec![1.0; 480];
        let real = r.process(&mut out, &mut src, 1.0);
        assert!(real < 480 && real >= 290);
        assert!(out[real..].iter().all(|v| *v == 0.0));
        // refill; resampler must restart without garbage
        src.extend(sine(2000, 300.0));
        let mut out2 = vec![9.0; 480];
        assert_eq!(r.process(&mut out2, &mut src, 1.0), 480);
        assert!(out2.iter().all(|v| v.abs() <= 0.51));
    }

    #[test]
    fn empty_source_yields_silence() {
        let mut src: VecDeque<f32> = VecDeque::new();
        let mut r = DriftResampler::new();
        let mut out = vec![1.0; 100];
        assert_eq!(r.process(&mut out, &mut src, 1.0), 0);
        assert!(out.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn ratio_controller_is_bounded_and_directional() {
        assert_eq!(ratio_for_fill(480, 480), 1.0);
        assert!(ratio_for_fill(960, 480) > 1.0);
        assert!(ratio_for_fill(100, 480) < 1.0);
        assert!(ratio_for_fill(1_000_000, 480) <= 1.0 + MAX_DEVIATION);
        assert!(ratio_for_fill(0, 480) >= 1.0 - MAX_DEVIATION);
    }
}
