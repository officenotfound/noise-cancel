#![allow(deprecated)]
//! The retro main window (Windows 95 look) and the simulated backend used to preview it on any OS.

pub mod app;
#[cfg_attr(windows, allow(dead_code))]
pub mod demo;
pub mod model;
pub mod theme;
pub mod widgets;

pub use app::{run, BackendFactory, WindowOptions};
