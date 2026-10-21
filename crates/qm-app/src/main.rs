//! QuietMic: system tray noise-cancelling microphone for Windows.
//! Audio path: microphone -> engine (model + gate + limiter) -> VB-Audio Virtual Cable ("CABLE Input").
//! Pick "CABLE Output" as the microphone in Zoom, Teams, softphones, etc.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod report;
mod settings;

#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("QuietMic is a Windows app. On other systems use the `qm` command-line tool.");
}
