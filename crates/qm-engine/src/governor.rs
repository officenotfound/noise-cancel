use std::collections::VecDeque;
use std::path::Path;
use std::time::Instant;

use anyhow::Result;

use crate::models::ModelId;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Stay,
    StepDown,
    StepUp,
}

/// Watches per-call model times against the real-time deadline (one hop) and decides when to change tier.
///
/// Down: 3 calls in a row slower than the hop, or >= 20 of the last 100 above 70% of the hop.
/// Up: 6000 consecutive calls (about 60 s) all below 40% of the hop, and not within the cooldown after a change.
pub struct Governor {
    recent: VecDeque<f64>,
    over_streak: usize,
    good_streak: usize,
    cooldown: usize,
}

const WINDOW: usize = 100;
const COOLDOWN_CALLS: usize = 3000;
const GOOD_CALLS_FOR_STEP_UP: usize = 6000;

impl Default for Governor {
    fn default() -> Self {
        Self::new()
    }
}

impl Governor {
    pub fn new() -> Self {
        Self { recent: VecDeque::with_capacity(WINDOW), over_streak: 0, good_streak: 0, cooldown: 0 }
    }

    /// `elapsed_s`: time the model call took; `hop_s`: audio duration it covers.
    pub fn observe(&mut self, elapsed_s: f64, hop_s: f64) -> Action {
        let ratio = elapsed_s / hop_s;
        if self.recent.len() == WINDOW {
            self.recent.pop_front();
        }
        self.recent.push_back(ratio);
        self.cooldown = self.cooldown.saturating_sub(1);
        self.over_streak = if ratio > 1.0 { self.over_streak + 1 } else { 0 };
        self.good_streak = if ratio < 0.4 { self.good_streak + 1 } else { 0 };

        let heavy = self.recent.iter().filter(|&&r| r > 0.7).count();
        if self.over_streak >= 3 || (self.recent.len() == WINDOW && heavy >= 20) {
            self.reset(COOLDOWN_CALLS);
            return Action::StepDown;
        }
        if self.good_streak >= GOOD_CALLS_FOR_STEP_UP && self.cooldown == 0 {
            self.reset(COOLDOWN_CALLS);
            return Action::StepUp;
        }
        Action::Stay
    }

    fn reset(&mut self, cooldown: usize) {
        self.recent.clear();
        self.over_streak = 0;
        self.good_streak = 0;
        self.cooldown = cooldown;
    }
}

/// How one model performed on this machine.
#[derive(Clone, Debug)]
pub struct Timing {
    pub model: ModelId,
    pub hop_ms: f64,
    pub algorithmic_ms: f64,
    pub mean_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

impl Timing {
    pub fn mean_frac(&self) -> f64 {
        self.mean_ms / self.hop_ms
    }
    pub fn p99_frac(&self) -> f64 {
        self.p99_ms / self.hop_ms
    }
}

/// Deterministic speech-like signal plus noise, to exercise the models during calibration.
fn calibration_signal(n: usize) -> Vec<f32> {
    let mut seed = 0x1234_5678u32;
    let mut rnd = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    };
    (0..n)
        .map(|i| {
            let t = i as f32 / 48_000.0;
            let env = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin().max(0.0);
            let voice: f32 = (1..=12).map(|h| (2.0 * std::f32::consts::PI * 120.0 * h as f32 * t).sin() / h as f32).sum();
            0.05 * env * voice + 0.08 * rnd()
        })
        .collect()
}

/// Time each model for `seconds` of audio on this machine (single thread, same call pattern as live use).
pub fn calibrate(models_dir: &Path, ids: &[ModelId], seconds: f32) -> Result<Vec<Timing>> {
    let mut out = Vec::new();
    for &id in ids {
        let mut m = id.build(models_dir, None)?;
        let hop = m.hop();
        let sig = calibration_signal((seconds * 48_000.0) as usize + hop * 40);
        let mut buf = vec![0.0f32; hop];
        let mut ms = Vec::new();
        for (i, chunk) in sig.chunks_exact(hop).enumerate() {
            let t = Instant::now();
            m.process_frame(chunk, &mut buf)?;
            if i >= 30 {
                ms.push(t.elapsed().as_secs_f64() * 1e3);
                // Clearly too heavy for real time: stop early so calibration stays short on slow PCs.
                let hop_ms = hop as f64 * 1e3 / 48_000.0;
                if ms.len() >= 40 && ms.iter().sum::<f64>() / ms.len() as f64 > 0.6 * hop_ms {
                    break;
                }
            }
        }
        ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        out.push(Timing {
            model: id,
            hop_ms: hop as f64 * 1e3 / 48_000.0,
            algorithmic_ms: m.algorithmic_delay() as f64 * 1e3 / 48_000.0,
            mean_ms: ms.iter().sum::<f64>() / ms.len() as f64,
            p99_ms: ms[((ms.len() - 1) as f64 * 0.99) as usize],
            max_ms: *ms.last().unwrap(),
        });
    }
    Ok(out)
}

/// Heaviest model in `ladder` whose p99 call time stays under `max_p99_frac` of its hop and mean under
/// `max_mean_frac` (headroom for thermal throttling and other apps). Falls back to the last (lightest) entry.
pub fn pick(ladder: &[ModelId], timings: &[Timing], max_p99_frac: f64, max_mean_frac: f64) -> ModelId {
    ladder
        .iter()
        .find(|id| {
            timings.iter().any(|t| t.model == **id && t.p99_frac() <= max_p99_frac && t.mean_frac() <= max_mean_frac)
        })
        .copied()
        .unwrap_or(*ladder.last().expect("empty ladder"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOP: f64 = 0.010;

    #[test]
    fn steps_down_after_three_consecutive_overruns() {
        let mut g = Governor::new();
        assert_eq!(g.observe(0.012, HOP), Action::Stay);
        assert_eq!(g.observe(0.012, HOP), Action::Stay);
        assert_eq!(g.observe(0.012, HOP), Action::StepDown);
    }

    #[test]
    fn single_spike_is_tolerated() {
        let mut g = Governor::new();
        for _ in 0..50 {
            assert_eq!(g.observe(0.002, HOP), Action::Stay);
        }
        assert_eq!(g.observe(0.030, HOP), Action::Stay);
        for _ in 0..50 {
            assert_eq!(g.observe(0.002, HOP), Action::Stay);
        }
    }

    #[test]
    fn sustained_high_load_steps_down() {
        let mut g = Governor::new();
        let mut actions = Vec::new();
        for i in 0..200 {
            actions.push(g.observe(if i % 4 == 0 { 0.008 } else { 0.001 }, HOP));
        }
        assert!(actions.contains(&Action::StepDown));
    }

    #[test]
    fn steps_up_only_after_long_comfortable_run_and_cooldown() {
        let mut g = Governor::new();
        let mut up_at = None;
        for i in 0..20_000 {
            if g.observe(0.002, HOP) == Action::StepUp {
                up_at = Some(i);
                break;
            }
        }
        assert_eq!(up_at, Some(GOOD_CALLS_FOR_STEP_UP - 1));
        // right after a change, another step-up is not allowed until cooldown ends
        for _ in 0..GOOD_CALLS_FOR_STEP_UP - 1 {
            assert_ne!(g.observe(0.002, HOP), Action::StepDown);
        }
    }

    fn t(model: ModelId, hop_ms: f64, mean_ms: f64, p99_ms: f64) -> Timing {
        Timing { model, hop_ms, algorithmic_ms: 10.0, mean_ms, p99_ms, max_ms: p99_ms }
    }

    #[test]
    fn pick_takes_heaviest_model_with_headroom() {
        let ladder = [ModelId::Dfn3Ll, ModelId::FeS, ModelId::FeT];
        let timings = [t(ModelId::Dfn3Ll, 10.0, 5.0, 7.0), t(ModelId::FeS, 10.7, 2.0, 3.0), t(ModelId::FeT, 10.7, 0.2, 0.3)];
        assert_eq!(pick(&ladder, &timings, 0.4, 0.25), ModelId::FeS); // DFN3-LL p99 70% of hop is too tight
        let fast = [t(ModelId::Dfn3Ll, 10.0, 1.7, 1.9)];
        assert_eq!(pick(&ladder, &fast, 0.4, 0.25), ModelId::Dfn3Ll);
        assert_eq!(pick(&ladder, &[], 0.4, 0.25), ModelId::FeT); // nothing measured: lightest
    }
}
