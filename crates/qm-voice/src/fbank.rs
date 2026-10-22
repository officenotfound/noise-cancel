//! Kaldi-compatible filter-bank features (what `torchaudio.compliance.kaldi.fbank` / kaldi-native-fbank compute),
//! configured for the WeSpeaker models: 16 kHz, 25 ms Hamming frames every 10 ms, 80 mel bins from 20 Hz to Nyquist,
//! pre-emphasis 0.97, per-frame DC removal, no dither, no energy, log of the mel energies.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

pub const SAMPLE_RATE: usize = 16_000;
pub const FRAME_LEN: usize = 400; // 25 ms
pub const FRAME_SHIFT: usize = 160; // 10 ms
pub const NUM_BINS: usize = 80;
const N_FFT: usize = 512;
const LOW_FREQ: f32 = 20.0;
const PREEMPH: f32 = 0.97;

fn mel(f: f32) -> f32 {
    1127.0 * (1.0 + f / 700.0).ln()
}

pub struct Fbank {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    /// For each mel bin: (first FFT bin, weights).
    banks: Vec<(usize, Vec<f32>)>,
    time: Vec<f32>,
    spec: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

impl Default for Fbank {
    fn default() -> Self {
        Self::new()
    }
}

impl Fbank {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(N_FFT);
        let window: Vec<f32> = (0..FRAME_LEN)
            .map(|i| 0.54 - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (FRAME_LEN as f32 - 1.0)).cos())
            .collect();

        let nyquist = SAMPLE_RATE as f32 / 2.0;
        let (mel_low, mel_high) = (mel(LOW_FREQ), mel(nyquist));
        let delta = (mel_high - mel_low) / (NUM_BINS as f32 + 1.0);
        let bin_width = SAMPLE_RATE as f32 / N_FFT as f32;
        let banks = (0..NUM_BINS)
            .map(|b| {
                let (left, center, right) = (mel_low + b as f32 * delta, mel_low + (b as f32 + 1.0) * delta, mel_low + (b as f32 + 2.0) * delta);
                let mut first = None;
                let mut w = Vec::new();
                for i in 0..N_FFT / 2 {
                    let m = mel(bin_width * i as f32);
                    let weight = if m > left && m < center {
                        (m - left) / (center - left)
                    } else if m > center && m < right {
                        (right - m) / (right - center)
                    } else {
                        0.0
                    };
                    if weight > 0.0 {
                        first.get_or_insert(i);
                        w.push(weight);
                    } else if first.is_some() && m >= right {
                        break;
                    }
                }
                (first.unwrap_or(0), w)
            })
            .collect();
        Self { spec: fft.make_output_vec(), scratch: fft.make_scratch_vec(), fft, window, banks, time: vec![0.0; N_FFT] }
    }

    /// Number of frames for `n` samples (frames that fit completely, like Kaldi's `snip_edges`).
    pub fn num_frames(n: usize) -> usize {
        if n < FRAME_LEN { 0 } else { 1 + (n - FRAME_LEN) / FRAME_SHIFT }
    }

    /// `samples` are 16 kHz mono in -1..1. They are scaled to the 16-bit range first (WeSpeaker `normalize_samples = 0`).
    /// Returns `frames x 80` log-mel energies, row-major.
    pub fn compute(&mut self, samples: &[f32]) -> Vec<f32> {
        let frames = Self::num_frames(samples.len());
        let mut out = Vec::with_capacity(frames * NUM_BINS);
        let mut frame = [0.0f32; FRAME_LEN];
        for t in 0..frames {
            let src = &samples[t * FRAME_SHIFT..t * FRAME_SHIFT + FRAME_LEN];
            for (d, s) in frame.iter_mut().zip(src) {
                *d = s * 32768.0;
            }
            // remove DC offset, then pre-emphasis (first sample uses itself), then window
            let mean = frame.iter().sum::<f32>() / FRAME_LEN as f32;
            frame.iter_mut().for_each(|v| *v -= mean);
            for i in (1..FRAME_LEN).rev() {
                frame[i] -= PREEMPH * frame[i - 1];
            }
            frame[0] -= PREEMPH * frame[0];
            for i in 0..FRAME_LEN {
                self.time[i] = frame[i] * self.window[i];
            }
            self.time[FRAME_LEN..].fill(0.0);
            self.fft.process_with_scratch(&mut self.time, &mut self.spec, &mut self.scratch).expect("fft sizes match");
            let power: Vec<f32> = self.spec[..N_FFT / 2].iter().map(|c| c.norm_sqr()).collect();
            for (first, w) in &self.banks {
                let e: f32 = w.iter().enumerate().map(|(k, wk)| wk * power[first + k]).sum();
                out.push(e.max(f32::EPSILON).ln());
            }
        }
        out
    }
}

/// Subtract the mean of every band over time (cepstral mean normalisation), in place.
pub fn mean_normalize(feats: &mut [f32]) {
    let frames = feats.len() / NUM_BINS;
    if frames == 0 {
        return;
    }
    for b in 0..NUM_BINS {
        let mean = (0..frames).map(|t| feats[t * NUM_BINS + b]).sum::<f32>() / frames as f32;
        for t in 0..frames {
            feats[t * NUM_BINS + b] -= mean;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, secs: f32) -> Vec<f32> {
        (0..(SAMPLE_RATE as f32 * secs) as usize).map(|i| 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / SAMPLE_RATE as f32).sin()).collect()
    }

    #[test]
    fn frame_counts_match_kaldi() {
        assert_eq!(Fbank::num_frames(399), 0);
        assert_eq!(Fbank::num_frames(400), 1);
        assert_eq!(Fbank::num_frames(16_000), 98); // 1 s -> 98 frames
        assert_eq!(Fbank::num_frames(32_000), 198);
    }

    #[test]
    fn sine_energy_lands_in_the_matching_mel_band() {
        let mut fb = Fbank::new();
        let f = fb.compute(&sine(1000.0, 0.5));
        let row = &f[..NUM_BINS];
        let peak = row.iter().cloned().enumerate().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).unwrap().0;
        // 1 kHz is about mel 1000, in the 80-band layout that is a band around index 25-35
        assert!((22..=38).contains(&peak), "peak band {peak}");
        assert!(row[peak] > row[NUM_BINS - 1] + 5.0);
    }

    #[test]
    fn higher_pitch_moves_the_peak_up() {
        let mut fb = Fbank::new();
        let mut argmax = |hz: f32| {
            let f = fb.compute(&sine(hz, 0.3));
            f[..NUM_BINS].iter().cloned().enumerate().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).unwrap().0
        };
        assert!(argmax(500.0) < argmax(1500.0) && argmax(1500.0) < argmax(4000.0));
    }

    #[test]
    fn silence_gives_the_floor_value_and_mean_normalisation_zeroes_each_band() {
        let mut fb = Fbank::new();
        let f = fb.compute(&vec![0.0; 4000]);
        assert!(f.iter().all(|v| (*v - f32::EPSILON.ln()).abs() < 1e-3));
        let mut g = fb.compute(&sine(700.0, 0.4));
        mean_normalize(&mut g);
        let frames = g.len() / NUM_BINS;
        for b in 0..NUM_BINS {
            let m = (0..frames).map(|t| g[t * NUM_BINS + b]).sum::<f32>() / frames as f32;
            assert!(m.abs() < 1e-3);
        }
    }
}
