use serde::{Deserialize, Serialize};

use crate::models::ModelId;

/// User-facing presets. Each one is a *ladder* of models (heaviest first, the governor walks down it when the
/// CPU cannot keep up) plus residual-gate settings.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    /// Default: calibrates the PC and picks the strongest low-latency tier it can run safely.
    Auto,
    /// Lowest delay first, then noise removal. For live calls.
    LowLatency,
    /// Good noise removal at modest delay and CPU.
    Balanced,
    /// Strongest suppression: heaviest model that fits, aggressive gate. Accepts extra delay (DPDFNet ~50 ms).
    MaxNoiseRemoval,
    /// Strongest rejection of other and distant voices; tight level gate.
    MaxVoiceSeparation,
    /// Old or battery-limited PCs.
    Light,
}

impl Preset {
    pub const ALL: [Preset; 6] =
        [Preset::Auto, Preset::LowLatency, Preset::Balanced, Preset::MaxNoiseRemoval, Preset::MaxVoiceSeparation, Preset::Light];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Auto => "auto",
            Preset::LowLatency => "low-latency",
            Preset::Balanced => "balanced",
            Preset::MaxNoiseRemoval => "max-noise-removal",
            Preset::MaxVoiceSeparation => "max-voice-separation",
            Preset::Light => "light",
        }
    }
}

impl std::str::FromStr for Preset {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        Preset::ALL.into_iter().find(|p| p.name() == s).ok_or_else(|| anyhow::anyhow!("unknown preset {s:?}"))
    }
}

/// Residual gate: a level-aware downward expander that mutes whatever the model let through that is much quieter
/// than the user's own voice (leftover barks, rain, distant yelling), while leaving the voice untouched.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GateConfig {
    pub enabled: bool,
    /// Gate opens for sounds within this many dB of the learned speech level.
    pub threshold_below_speech_db: f32,
    /// Downward expansion ratio below the threshold (dB of extra attenuation per dB under it = ratio - 1).
    pub ratio: f32,
    pub max_atten_db: f32,
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub release_ms: f32,
    /// Audio is delayed by this much so the detector sees speech onsets before they are heard (adds latency).
    pub lookahead_ms: f32,
    /// Speech level (dBFS RMS) assumed until the engine has learned the real one.
    pub initial_speech_db: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub preset: Preset,
    pub ladder: Vec<ModelId>,
    /// DeepFilterNet attenuation limit (dB); None = unlimited.
    pub atten_lim_db: Option<f32>,
    pub gate: GateConfig,
    /// Linear output ceiling.
    pub limiter_ceiling: f32,
    /// Audio callback size in samples (10 ms = 480 at 48 kHz).
    pub block: usize,
    /// Let the governor step down/up the ladder from measured CPU timings.
    pub auto_govern: bool,
}

impl Config {
    pub fn from_preset(preset: Preset) -> Self {
        use ModelId::*;
        let gate = |thr, ratio, maxa, hold, rel| GateConfig {
            enabled: true,
            threshold_below_speech_db: thr,
            ratio,
            max_atten_db: maxa,
            attack_ms: 3.0,
            hold_ms: hold,
            release_ms: rel,
            lookahead_ms: 2.5,
            initial_speech_db: -28.0,
        };
        let (ladder, g): (Vec<ModelId>, GateConfig) = match preset {
            Preset::Auto | Preset::LowLatency => (vec![Dfn3Ll, FeS, FeB, FeT, Rnnoise], gate(20.0, 4.0, 45.0, 120.0, 40.0)),
            Preset::Balanced => (vec![FeM, Dfn3Ll, FeS, FeB, FeT, Rnnoise], gate(18.0, 5.0, 50.0, 120.0, 40.0)),
            Preset::MaxNoiseRemoval => (vec![Dp8, Dp2, FeM, Dfn3Ll, FeS, FeB, FeT, Rnnoise], gate(15.0, 6.0, 60.0, 150.0, 50.0)),
            Preset::MaxVoiceSeparation => (vec![FeM, Dfn3Ll, FeS, FeB, FeT, Rnnoise], gate(12.0, 6.0, 60.0, 150.0, 50.0)),
            Preset::Light => (vec![FeB, FeT, Rnnoise], gate(20.0, 4.0, 40.0, 120.0, 40.0)),
        };
        Config { preset, ladder, atten_lim_db: None, gate: g, limiter_ceiling: 0.89, block: 480, auto_govern: true }
    }
}
