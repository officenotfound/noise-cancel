//! The application: supervisor thread, tray / menu-bar icon, main window glue and the headless self-test.

mod backend;
mod controller;
mod logging;
pub mod os;
mod processor;
mod selftest;
mod tray;

use crate::settings::Settings;

pub use os::models_dir;

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let dir = os::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    logging::init(&dir);
    // The app has no console on Windows; make sure a crash leaves a trace in the log file.
    std::panic::set_hook(Box::new(|info| logging::log(format!("PANIC: {info}"))));
    // One instance at a time: a second start asks the running one to show its window and exits. Design previews
    // and the self-test do not need the lock.
    let _instance = if has("--demo") || has("--selftest") {
        None
    } else {
        match os::single_instance(&dir) {
            Some(lock) => Some(lock),
            None => {
                logging::log("QuietMic is already running; asking it to show its window");
                os::request_show(&dir);
                return;
            }
        }
    };
    os::keep_awake_for_audio();
    logging::log(format!("QuietMic {} starting; args {:?}", env!("CARGO_PKG_VERSION"), &args[1..]));

    // Design preview with simulated data (no audio): `--demo [--state running|bypass|calibrating|error|recording|recorded|playing]
    //   [--screenshot out.png] [--open file|options|help|about]`
    if has("--demo") {
        let backend = crate::ui::demo::DemoBackend::new(&flag_value(&args, "--state").unwrap_or_else(|| "running".into()));
        let opts = crate::ui::WindowOptions { screenshot: flag_value(&args, "--screenshot").map(Into::into), start_hidden: false, open: flag_value(&args, "--open") };
        if let Err(e) = crate::ui::run(Box::new(move |_| Box::new(backend)), opts) {
            eprintln!("window failed: {e}");
        }
        return;
    }

    if has("--selftest") {
        selftest::run(&args, &dir);
        return;
    }
    // `--output default` plays to the default speakers instead of the virtual cable (testing only).
    let output_override = flag_value(&args, "--output");
    let settings = Settings::load(&dir);
    let ctl = std::sync::Arc::new(controller::Controller::start(dir.clone(), settings, output_override));

    #[cfg(windows)]
    if has("--tray-only") {
        backend::run_tray_only(backend::AppBackend::new(ctl.clone(), dir, None));
        ctl.shutdown();
        return;
    }

    // Normal start: the retro main window plus the tray / menu-bar icon. `--minimized` (autostart) begins hidden.
    let (ctl2, dir2) = (ctl.clone(), dir.clone());
    let make: crate::ui::BackendFactory = Box::new(move |ctx| Box::new(backend::AppBackend::new(ctl2, dir2, Some(ctx.clone()))));
    let opts = crate::ui::WindowOptions { start_hidden: has("--minimized"), ..Default::default() };
    if let Err(e) = crate::ui::run(make, opts) {
        logging::log(format!("main window failed: {e}"));
        // No usable graphics: on Windows stay useful as a tray-only app.
        #[cfg(windows)]
        backend::run_tray_only(backend::AppBackend::new(ctl.clone(), dir, None));
    }
    ctl.shutdown();
}
