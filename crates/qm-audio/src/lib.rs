//! Audio layer for QuietMic: capture the microphone, run a processor, render to a virtual cable, with clock-drift
//! compensation, plus playback for the in-app recorder.
//!
//! * Windows: WASAPI (`session.rs`, `player.rs`), virtual cable = VB-Audio Virtual Cable ("CABLE Input").
//! * macOS: CoreAudio through `cpal` (`mac.rs`), virtual cable = BlackHole.
//!
//! `drift`, `player_core` and `types` are platform independent (and unit-tested everywhere).

pub mod drift;
pub mod player_core;
mod types;

pub use types::*;

#[cfg(windows)]
mod player;
#[cfg(windows)]
mod session;
#[cfg(windows)]
pub use player::Player;
#[cfg(windows)]
pub use session::*;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
pub use mac::*;
