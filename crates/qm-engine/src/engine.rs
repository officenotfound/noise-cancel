use std::path::PathBuf;

use anyhow::Result;
use qm_dsp::SAMPLE_RATE;

use crate::config::Config;
use crate::gate::ResidualGate;
use crate::governor::{Action, Governor};
use crate::limiter::Limiter;
use crate::models::ModelId;
use crate::reframer::Reframer;

/// Denoise chain: re-framer + model, residual gate, soft limiter, plus the tier governor.
pub struct Engine {
    cfg: Config,
    models_dir: PathBuf,
    tier: usize,
    reframer: Reframer,
    gate: ResidualGate,
    limiter: Limiter,
    governor: Governor,
    switches: Vec<(usize, ModelId)>,
    /// Exponential moving average of (model call time / hop duration): the share of one core the model uses.
    load: f64,
}

impl Engine {
    /// `start` overrides the first ladder entry (e.g. the result of calibration).
    pub fn new(cfg: Config, models_dir: impl Into<PathBuf>, start: Option<ModelId>) -> Result<Self> {
        let models_dir = models_dir.into();
        let tier = start.and_then(|m| cfg.ladder.iter().position(|x| *x == m)).unwrap_or(0);
        let reframer = Reframer::new(cfg.ladder[tier].build(&models_dir, cfg.atten_lim_db)?, cfg.block);
        let gate = ResidualGate::new(cfg.gate.clone());
        let limiter = Limiter::new(cfg.limiter_ceiling * 0.8, cfg.limiter_ceiling);
        Ok(Self { cfg, models_dir, tier, reframer, gate, limiter, governor: Governor::new(), switches: Vec::new(), load: 0.0 })
    }

    pub fn model(&self) -> ModelId {
        self.cfg.ladder[self.tier]
    }

    /// Total input-to-output delay in samples: model + re-framing + gate look-ahead.
    pub fn total_delay(&self) -> usize {
        self.reframer.total_delay() + self.gate.lookahead_samples()
    }

    pub fn total_delay_ms(&self) -> f64 {
        self.total_delay() as f64 * 1e3 / SAMPLE_RATE as f64
    }

    /// Tier changes made by the governor so far: (block-call index not tracked, new model).
    pub fn switches(&self) -> &[(usize, ModelId)] {
        &self.switches
    }

    /// Recent share of one CPU core used by the model (0.0 to 1.0+).
    pub fn load(&self) -> f64 {
        self.load
    }

    pub fn gate(&self) -> &ResidualGate {
        &self.gate
    }

    /// Process one block (any size). `output.len() == input.len()`.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<()> {
        let timings = self.reframer.process(input, output)?;
        self.gate.process(output);
        for s in output.iter_mut() {
            *s = self.limiter.process(*s);
        }
        for (elapsed, hop) in timings {
            let hop_s = hop as f64 / SAMPLE_RATE as f64;
            self.load += 0.02 * (elapsed / hop_s - self.load);
            if self.cfg.auto_govern {
                match self.governor.observe(elapsed, hop_s) {
                    Action::StepDown if self.tier + 1 < self.cfg.ladder.len() => self.set_tier(self.tier + 1)?,
                    Action::StepUp if self.tier > 0 => self.set_tier(self.tier - 1)?,
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Switch model. Note: the new model starts with fresh state; the audio layer should crossfade or switch in a pause.
    pub fn set_tier(&mut self, tier: usize) -> Result<()> {
        self.reframer = Reframer::new(self.cfg.ladder[tier].build(&self.models_dir, self.cfg.atten_lim_db)?, self.cfg.block);
        self.tier = tier;
        self.switches.push((tier, self.cfg.ladder[tier]));
        Ok(())
    }
}
