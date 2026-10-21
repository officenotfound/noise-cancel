//! Windows-only parts of the app: supervisor thread, tray UI, headless self-test.

mod controller;
mod logging;
mod processor;
mod selftest;
mod tray;

use std::path::PathBuf;

use crate::settings::{data_dir, Settings};

pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_else(|| PathBuf::from("."))
}

/// Models live next to the exe in `models\` (override with the QM_MODELS environment variable).
pub fn models_dir() -> PathBuf {
    std::env::var_os("QM_MODELS").map(PathBuf::from).unwrap_or_else(|| exe_dir().join("models"))
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    logging::init(&dir);
    logging::log(format!("QuietMic {} starting; args {:?}", env!("CARGO_PKG_VERSION"), &args[1..]));

    if args.iter().any(|a| a == "--selftest") {
        selftest::run(&args, &dir);
        return;
    }
    // `--output default` plays to the default speakers instead of VB-Cable (testing only).
    let output_override = flag_value(&args, "--output");
    let settings = Settings::load(&dir);
    let ctl = controller::Controller::start(dir.clone(), settings, output_override);
    tray::run(ctl, dir);
}
