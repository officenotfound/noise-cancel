use std::collections::VecDeque;

use qm_dsp::SAMPLE_RATE;

use crate::config::GateConfig;

const SUB: usize = 120; // 2.5 ms detector/gain step at 48 kHz
const HIST: usize = 4; // detector window = 4 sub-blocks = 10 ms
const FLOOR_DB: f32 = -55.0;

/// Level-aware downward expander. See [`GateConfig`].
pub struct ResidualGate {
    cfg: GateConfig,
    energy: [f32; HIST],
    energy_i: usize,
    speech_ref_db: f32,
    gain_db: f32,
    hold_left: usize,
    delay: VecDeque<f32>,
}

fn db(x: f32) -> f32 {
    10.0 * (x + 1e-12).log10()
}

impl ResidualGate {
    pub fn new(cfg: GateConfig) -> Self {
        let la = Self::lookahead_samples_for(&cfg);
        Self {
            energy: [1e-12; HIST],
            energy_i: 0,
            speech_ref_db: cfg.initial_speech_db,
            gain_db: 0.0,
            hold_left: 0,
            delay: std::iter::repeat(0.0).take(la).collect(),
            cfg,
        }
    }

    fn lookahead_samples_for(cfg: &GateConfig) -> usize {
        if cfg.enabled { (cfg.lookahead_ms * SAMPLE_RATE as f32 / 1000.0).round() as usize } else { 0 }
    }

    /// Delay (samples) this stage adds.
    pub fn lookahead_samples(&self) -> usize {
        self.delay.len()
    }

    pub fn speech_reference_db(&self) -> f32 {
        self.speech_ref_db
    }

    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    pub fn process(&mut self, io: &mut [f32]) {
        if !self.cfg.enabled {
            return;
        }
        let step_s = SUB as f32 / SAMPLE_RATE as f32;
        let coef = |ms: f32| 1.0 - (-step_s / (ms / 1000.0).max(1e-4)).exp();
        let (attack, release) = (coef(self.cfg.attack_ms), coef(self.cfg.release_ms));
        let hold_blocks = (self.cfg.hold_ms / 1000.0 / step_s).round() as usize;

        for block in io.chunks_mut(SUB) {
            // Detector sees the *undelayed* signal.
            let e = block.iter().map(|x| x * x).sum::<f32>() / block.len() as f32;
            self.energy[self.energy_i] = e;
            self.energy_i = (self.energy_i + 1) % HIST;
            let level = db(self.energy.iter().sum::<f32>() / HIST as f32);

            // Learn the speech level from active sounds only; clamp the innovation so a loud leak cannot drag it.
            if level > FLOOR_DB && level > self.speech_ref_db - 15.0 {
                let innov = (level - self.speech_ref_db).clamp(-6.0, 6.0);
                self.speech_ref_db = (self.speech_ref_db + 0.004 * innov).clamp(-45.0, -10.0);
            }

            let thresh = self.speech_ref_db - self.cfg.threshold_below_speech_db;
            let target = if level >= thresh {
                self.hold_left = hold_blocks;
                0.0
            } else if self.hold_left > 0 {
                self.hold_left -= 1;
                0.0
            } else {
                -((thresh - level) * (self.cfg.ratio - 1.0)).min(self.cfg.max_atten_db)
            };
            let c = if target < self.gain_db { attack } else { release };
            let prev_gain = self.gain_db;
            self.gain_db += c * (target - self.gain_db);

            // Delay line, then apply a gain ramp across the block (no zipper noise).
            let n = block.len();
            for (i, s) in block.iter_mut().enumerate() {
                let delayed = if self.delay.is_empty() {
                    *s
                } else {
                    self.delay.push_back(*s);
                    self.delay.pop_front().unwrap()
                };
                let g_db = prev_gain + (self.gain_db - prev_gain) * (i + 1) as f32 / n as f32;
                *s = delayed * 10f32.powf(g_db / 20.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> GateConfig {
        GateConfig {
            enabled: true,
            threshold_below_speech_db: 18.0,
            ratio: 4.0,
            max_atten_db: 45.0,
            attack_ms: 3.0,
            hold_ms: 120.0,
            release_ms: 40.0,
            lookahead_ms: 0.0,
            initial_speech_db: -28.0,
        }
    }

    fn tone(amp: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| amp * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 48_000.0).sin()).collect()
    }

    fn rms_db(x: &[f32]) -> f32 {
        db(x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32)
    }

    #[test]
    fn passes_speech_level_untouched() {
        let mut g = ResidualGate::new(cfg());
        let mut x = tone(0.05, 48_000); // about -29 dBFS RMS, close to the assumed speech level
        let before = rms_db(&x);
        g.process(&mut x);
        assert!((rms_db(&x) - before).abs() < 0.5);
    }

    #[test]
    fn mutes_quiet_residual_after_speech() {
        let mut g = ResidualGate::new(cfg());
        let mut speech = tone(0.05, 24_000);
        g.process(&mut speech);
        let mut leak = tone(0.002, 48_000); // about -57 dBFS, far below speech - 18 dB
        g.process(&mut leak);
        // after hold + release the leak is strongly attenuated
        assert!(rms_db(&leak[24_000..]) < rms_db(&tone(0.002, 24_000)) - 30.0);
    }

    #[test]
    fn short_pause_inside_speech_is_held_open() {
        let mut g = ResidualGate::new(cfg());
        let mut a = tone(0.05, 24_000);
        g.process(&mut a);
        let mut pause = tone(0.0005, 2_400); // 50 ms of near-silence < hold time
        g.process(&mut pause);
        assert!(g.gain_db() > -3.0);
    }

    #[test]
    fn lookahead_delays_audio_by_exactly_that_much() {
        let mut c = cfg();
        c.lookahead_ms = 2.5;
        let mut g = ResidualGate::new(c);
        assert_eq!(g.lookahead_samples(), 120);
        let input = tone(0.05, 4_800); // speech-level tone so the gate stays open
        let mut x = input.clone();
        g.process(&mut x);
        assert!(x[..120].iter().all(|v| *v == 0.0));
        for i in 2_400..4_800 {
            assert!((x[i] - input[i - 120]).abs() < 0.002, "sample {i}");
        }
    }

    #[test]
    fn disabled_gate_is_a_no_op_with_no_delay() {
        let mut c = cfg();
        c.enabled = false;
        let mut g = ResidualGate::new(c);
        assert_eq!(g.lookahead_samples(), 0);
        let mut x = tone(0.01, 1000);
        let y = x.clone();
        g.process(&mut x);
        assert_eq!(x, y);
    }
}
