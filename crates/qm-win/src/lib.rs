//! Windows audio layer for QuietMic: WASAPI capture of the microphone, processing, and rendering to the virtual
//! cable, with clock-drift compensation. `drift` is platform independent (and unit-tested everywhere); the rest only
//! exists on Windows.

pub mod drift;
pub mod player_core;

#[cfg(windows)]
mod player;
#[cfg(windows)]
mod session;
#[cfg(windows)]
pub use player::Player;
#[cfg(windows)]
pub use session::*;
