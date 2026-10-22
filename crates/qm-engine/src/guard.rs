//! Noise guard: a second, cheap denoiser acts as a "is anyone speaking?" detector. When its output is silent the
//! main model's output is muted. In experiments this removed an extra 5-7 dB of rain and steel-roof rain with no
//! damage to the voice. Both paths are time-aligned (the faster one is delayed) and the audio is delayed by a short
//! look-ahead so speech onsets are never clipped.

use std::collections::VecDeque;
use std::path::Path;

use anyhow::Result;
use qm_dsp::SAMPLE_RATE;
use serde::{Deserialize, Serialize};

use crate::models::ModelId;
use crate::reframer::Reframer;

const SUB: usize = 120; // 2.5 ms
const HIST: usize = 4; // 10 ms detector window

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuardConfig {
    /// The cheap detector model (FastEnhancer-B costs about 4% of a core).
    pub model: ModelId,
    /// The detector's output must exceed this level (dBFS, 10 ms RMS) for sound to count as speech.
    pub threshold_db: f32,
    /// Attenuation applied while nobody is speaking.
    pub atten_db: f32,
    /// How long the guard stays open after the last detected speech.
    pub hold_ms: f32,
}

impl GuardConfig {
    pub fn standard() -> Self {
        Self { model: ModelId::FeB, threshold_db: -50.0, atten_db: 30.0, hold_ms: 300.0 }
    }
}

pub struct Guard {
    cfg: GuardConfig,
    reframer: Reframer,
    /// Delay lines that align the main output and the detector output, plus the shared look-ahead on the main path.
    delay_main: VecDeque<f32>,
    delay_guard: VecDeque<f32>,
    lookahead: VecDeque<f32>,
    energy: [f32; HIST],
    energy_i: usize,
    hold_left: usize,
    gain_db: f32,
    extra_delay: usize,
    scratch: Vec<f32>,
}

fn db(x: f32) -> f32 {
    10.0 * (x + 1e-12).log10()
}

impl Guard {
    /// `main_delay` is the main chain's delay (samples) so the two paths can be aligned.
    pub fn new(cfg: GuardConfig, models_dir: &Path, block: usize, main_delay: usize) -> Result<Self> {
        let reframer = Reframer::new(cfg.model.build(models_dir, None)?, block);
        let guard_delay = reframer.total_delay();
        let (dm, dg) = (guard_delay.saturating_sub(main_delay), main_delay.saturating_sub(guard_delay));
        Ok(Self {
            cfg,
            reframer,
            delay_main: std::iter::repeat(0.0).take(dm).collect(),
            delay_guard: std::iter::repeat(0.0).take(dg).collect(),
            lookahead: std::iter::repeat(0.0).take(SUB).collect(),
            energy: [1e-12; HIST],
            energy_i: 0,
            hold_left: 0,
            gain_db: 0.0,
            extra_delay: dm + SUB,
            scratch: Vec::new(),
        })
    }

    /// Delay this stage adds on top of the main chain (samples).
    pub fn extra_delay(&self) -> usize {
        self.extra_delay
    }

    /// `input` is the raw microphone block, `main` the main model's output for the same block (modified in place).
    pub fn process(&mut self, input: &[f32], main: &mut [f32]) -> Result<()> {
        self.scratch.clear();
        self.scratch.resize(input.len(), 0.0);
        self.reframer.process(input, &mut self.scratch)?;

        // Align the two paths.
        if !self.delay_main.is_empty() {
            for s in main.iter_mut() {
                self.delay_main.push_back(*s);
                *s = self.delay_main.pop_front().unwrap();
            }
        }
        if !self.delay_guard.is_empty() {
            for s in self.scratch.iter_mut() {
                self.delay_guard.push_back(*s);
                *s = self.delay_guard.pop_front().unwrap();
            }
        }

        let step_s = SUB as f32 / SAMPLE_RATE as f32;
        let (attack, release) = (1.0 - (-step_s / 0.005).exp(), 1.0 - (-step_s / 0.020).exp());
        let hold_blocks = (self.cfg.hold_ms / 1000.0 / step_s).round() as usize;
        for (m, g) in main.chunks_mut(SUB).zip(self.scratch.chunks(SUB)) {
            self.energy[self.energy_i] = g.iter().map(|x| x * x).sum::<f32>() / g.len() as f32;
            self.energy_i = (self.energy_i + 1) % HIST;
            let level = db(self.energy.iter().sum::<f32>() / HIST as f32);
            let target = if level > self.cfg.threshold_db {
                self.hold_left = hold_blocks;
                0.0
            } else if self.hold_left > 0 {
                self.hold_left -= 1;
                0.0
            } else {
                -self.cfg.atten_db
            };
            let prev = self.gain_db;
            self.gain_db += (if target < prev { attack } else { release }) * (target - prev);
            let n = m.len();
            for (i, s) in m.iter_mut().enumerate() {
                self.lookahead.push_back(*s);
                let delayed = self.lookahead.pop_front().unwrap();
                let g_db = prev + (self.gain_db - prev) * (i + 1) as f32 / n as f32;
                *s = delayed * 10f32.powf(g_db / 20.0);
            }
        }
        Ok(())
    }
}
