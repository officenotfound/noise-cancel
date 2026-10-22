//! QuietMic: noise-cancelling microphone for Windows and macOS (tray / menu bar icon plus a retro main window).
//! Audio path: microphone -> engine (model + gate + limiter) -> virtual audio cable (VB-Cable on Windows,
//! BlackHole on macOS). Pick the cable as the microphone in Zoom, Teams, softphones, etc.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod recorder;
mod report;
mod settings;
mod ui;

fn main() {
    app::run();
}
