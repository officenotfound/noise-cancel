//! QuietMic engine: presets, model ladders, re-framing, residual gate, limiter and the tier governor.
//! Platform-independent; the Windows audio layer feeds it blocks of whatever size the device uses.

pub mod config;
pub mod engine;
pub mod gate;
pub mod governor;
pub mod limiter;
pub mod models;
pub mod reframer;

pub use config::{Config, GateConfig, Preset};
pub use engine::Engine;
pub use models::ModelId;
