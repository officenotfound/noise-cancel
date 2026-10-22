//! QuietMic: system tray noise-cancelling microphone for Windows.
//! Audio path: microphone -> engine (model + gate + limiter) -> VB-Audio Virtual Cable ("CABLE Input").
//! Pick "CABLE Output" as the microphone in Zoom, Teams, softphones, etc.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg_attr(not(windows), allow(dead_code))]
mod recorder;
mod report;
mod settings;
mod ui;

#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    win::run();
}

/// Off Windows there is no audio layer; run the window with simulated data so the design can be previewed.
/// `QuietMic --screenshot out.png [--state running|bypass|calibrating|error]`
#[cfg(not(windows))]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let backend = ui::demo::DemoBackend::new(&value("--state").unwrap_or_else(|| "running".into()));
    let opts = ui::WindowOptions { screenshot: value("--screenshot").map(Into::into), start_hidden: false, open: value("--open") };
    if let Err(e) = ui::run(Box::new(move |_| Box::new(backend)), opts) {
        eprintln!("window failed: {e}");
    }
}
