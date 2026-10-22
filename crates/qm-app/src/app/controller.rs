//! Supervisor thread: owns the audio session and keeps it healthy. It calibrates the PC, starts the engine for the
//! chosen preset, restarts after device changes or failures, and publishes a status line for the tray.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use qm_engine::governor::{calibrate, pick};
use qm_engine::{Config, Engine, ModelId, Preset};
use qm_audio::{Processor, Session, SessionConfig, DEFAULT_TARGET_FILL};

use super::logging::log;
use super::models_dir;
use super::processor::{EngineProcessor, EngineStatus};
use crate::recorder::Recorder;
use crate::report;
use crate::settings::Settings;

pub use crate::ui::model::Phase;

/// Numbers for the window's meters and performance panel.
#[derive(Clone, Debug, Default)]
pub struct Live {
    pub model: String,
    pub est_delay_ms: f32,
    pub cpu_pct: f32,
    pub dropouts: u64,
    pub input_peak: f32,
    pub output_peak: f32,
    pub input_name: String,
    pub output_name: String,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub phase: Phase,
    pub headline: String,
    pub detail: String,
}

pub enum Cmd {
    SetPreset(Preset),
    SetInput(Option<String>),
    SetBypass(bool),
    SetAutostart(bool),
    SetGuard(bool),
    Recalibrate,
    Quit,
}

pub struct Controller {
    tx: Sender<Cmd>,
    pub status: Arc<Mutex<Status>>,
    pub settings: Arc<Mutex<Settings>>,
    pub live: Arc<Mutex<Live>>,
    pub recorder: Arc<Recorder>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

struct App {
    dir: PathBuf,
    settings: Arc<Mutex<Settings>>,
    status: Arc<Mutex<Status>>,
    live: Arc<Mutex<Live>>,
    recorder: Arc<Recorder>,
    engine_status: Arc<Mutex<EngineStatus>>,
    bypass: Arc<AtomicBool>,
    output_override: Option<String>,
    /// True while running without a virtual cable (capture and process, send nowhere).
    test_mode: AtomicBool,
}

impl App {
    fn set_status(&self, phase: Phase, headline: impl Into<String>, detail: impl Into<String>) {
        *self.status.lock().unwrap() = Status { phase, headline: headline.into(), detail: detail.into() };
    }

    fn save_settings(&self) {
        if let Err(e) = self.settings.lock().unwrap().save(&self.dir) {
            log(format!("could not save settings: {e}"));
        }
    }
}

impl Controller {
    pub fn start(dir: PathBuf, settings: Settings, output_override: Option<String>) -> Controller {
        let (tx, rx) = mpsc::channel();
        let bypass = Arc::new(AtomicBool::new(settings.bypass));
        let app = App {
            dir,
            settings: Arc::new(Mutex::new(settings)),
            status: Arc::new(Mutex::new(Status { phase: Phase::Starting, headline: "Starting...".into(), detail: String::new() })),
            live: Arc::new(Mutex::new(Live::default())),
            recorder: Recorder::new(),
            engine_status: Arc::new(Mutex::new(EngineStatus::default())),
            bypass,
            output_override,
            test_mode: AtomicBool::new(false),
        };
        let (status, settings, live, recorder) = (app.status.clone(), app.settings.clone(), app.live.clone(), app.recorder.clone());
        let handle = std::thread::Builder::new().name("qm-supervisor".into()).spawn(move || run(app, rx)).ok();
        Controller { tx, status, settings, live, recorder, handle: Mutex::new(handle) }
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Cmd::Quit);
        if let Some(h) = self.handle.lock().unwrap().take() {
            let _ = h.join();
        }
    }
}

/// Choose the heaviest model in the preset's ladder that this PC runs with headroom (cached per CPU + preset).
fn pick_model(app: &App, cfg: &Config, preset: Preset) -> Result<ModelId> {
    let cpu = report::cpu_name();
    let key = Settings::calibration_key(&cpu, preset);
    if let Some(m) = app.settings.lock().unwrap().calibration.get(&key).copied() {
        if cfg.ladder.contains(&m) {
            log(format!("calibration cache hit: {} for {key}", m.name()));
            return Ok(m);
        }
    }
    app.set_status(Phase::Calibrating, "Measuring this PC's speed (first run, a few seconds)...", String::new());
    log(format!("calibrating ladder {:?}", cfg.ladder.iter().map(|m| m.name()).collect::<Vec<_>>()));
    let timings = calibrate(&models_dir(), &cfg.ladder, 3.0)?;
    let picked = pick(&cfg.ladder, &timings, 0.4, 0.25);
    let text = report::render(preset, &timings, picked);
    log(&text);
    let _ = std::fs::write(app.dir.join("performance_report.txt"), &text);
    app.settings.lock().unwrap().calibration.insert(key, picked);
    app.save_settings();
    Ok(picked)
}

/// Returns (output device id, test mode). Test mode = no virtual cable installed: capture and process, send nowhere.
fn find_output(app: &App) -> Result<(Option<String>, bool)> {
    match app.output_override.as_deref() {
        Some("default") => Ok((None, false)),
        Some(name) => {
            let want = name.to_lowercase();
            qm_audio::list_outputs()?
                .into_iter()
                .find(|d| d.name.to_lowercase().contains(&want))
                .map(|d| (Some(d.id), false))
                .ok_or_else(|| anyhow!("no output device matching {name:?}"))
        }
        None => Ok(match qm_audio::find_virtual_cable()? {
            Some(d) => (Some(d.id), false),
            None => (None, true),
        }),
    }
}

fn start_session(app: &App) -> Result<Session> {
    let (preset, input_id, guard) = {
        let s = app.settings.lock().unwrap();
        (s.preset, s.input_id.clone(), s.guard)
    };
    let mut cfg = Config::from_preset(preset);
    if guard && cfg.guard.is_none() {
        cfg.guard = Some(qm_engine::GuardConfig::standard());
    }
    let model = pick_model(app, &cfg, preset)?;
    let (output_id, test_mode) = find_output(app)?;
    app.test_mode.store(test_mode, Ordering::Relaxed);
    log(format!("starting session: preset {} model {} input {:?} output {:?} test_mode {}", preset.name(), model.name(), input_id, output_id, test_mode));

    let (bypass, status, recorder) = (app.bypass.clone(), app.engine_status.clone(), app.recorder.clone());
    let models = models_dir();
    let factory: qm_audio::ProcessorFactory = Box::new(move || {
        let engine = Engine::new(cfg, models, Some(model))?;
        Ok(Box::new(EngineProcessor::new(engine, bypass, status, recorder)) as Box<dyn Processor>)
    });
    Session::start(SessionConfig { input_id, output_id, target_fill: DEFAULT_TARGET_FILL, discard_output: test_mode, ..Default::default() }, factory)
}

fn refresh_status(app: &App, session: &Session) {
    let st = session.stats();
    let en = app.engine_status.lock().unwrap().clone();
    let period_ms = st.capture_period_frames as f64 * 1000.0 / 48_000.0;
    // model + capture period + ring target + render period (approximate end-to-end figure)
    let est_ms = en.delay_ms + period_ms + DEFAULT_TARGET_FILL as f64 * 1000.0 / 48_000.0 + period_ms;
    let detail = format!("{} -> {} - dropouts {}", st.input_name, st.output_name, st.underruns + st.overflows);
    *app.live.lock().unwrap() = Live {
        model: en.model.to_string(),
        est_delay_ms: est_ms as f32,
        cpu_pct: (en.load * 100.0) as f32,
        dropouts: st.underruns + st.overflows,
        input_peak: st.input_peak,
        output_peak: st.output_peak,
        input_name: st.input_name.clone(),
        output_name: st.output_name.clone(),
    };
    if en.model.is_empty() {
        app.set_status(Phase::Starting, "Loading model...", detail);
    } else if app.test_mode.load(Ordering::Relaxed) {
        let head = format!("TEST MODE: {} is not installed", super::os::CABLE_PRODUCT);
        app.set_status(Phase::NoCable, head, "Other apps cannot hear you yet. Try the recorder below.");
    } else if app.bypass.load(Ordering::Relaxed) {
        app.set_status(Phase::Bypass, "BYPASS: microphone passes through unprocessed", detail);
    } else {
        app.set_status(
            Phase::Running,
            format!("ON - {} - est. delay {:.0} ms - CPU {:.0}%", en.model, est_ms, en.load * 100.0),
            detail,
        );
    }
}

fn run(app: App, rx: mpsc::Receiver<Cmd>) {
    let mut session: Option<Session> = None;
    let mut need_start = true;
    let mut retry_at: Option<Instant> = None;
    let mut started_default: Option<String> = None;
    let mut last_poll = Instant::now();
    let mut last_error_logged = String::new();

    loop {
        if need_start && retry_at.map_or(true, |t| Instant::now() >= t) {
            need_start = false;
            if let Some(s) = session.take() {
                s.stop();
            }
            *app.engine_status.lock().unwrap() = EngineStatus::default();
            *app.live.lock().unwrap() = Live::default();
            match start_session(&app) {
                Ok(s) => {
                    started_default = qm_audio::default_input().ok().map(|d| d.id);
                    session = Some(s);
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    if msg != last_error_logged {
                        log(format!("start failed: {msg}"));
                        last_error_logged = msg.clone();
                    }
                    app.set_status(Phase::Error, format!("Not running: {msg}"), &format!("Install {}, then choose {} as your microphone.", super::os::CABLE_PRODUCT, super::os::CABLE_PICK));
                    need_start = true;
                    retry_at = Some(Instant::now() + Duration::from_secs(3));
                }
            }
        }

        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(Cmd::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Cmd::SetPreset(p)) => {
                app.settings.lock().unwrap().preset = p;
                app.save_settings();
                need_start = true;
                retry_at = None;
            }
            Ok(Cmd::SetInput(id)) => {
                app.settings.lock().unwrap().input_id = id;
                app.save_settings();
                need_start = true;
                retry_at = None;
            }
            Ok(Cmd::SetBypass(b)) => {
                app.bypass.store(b, Ordering::Relaxed);
                app.settings.lock().unwrap().bypass = b;
                app.save_settings();
            }
            Ok(Cmd::SetAutostart(on)) => {
                let exe = std::env::current_exe().unwrap_or_default();
                match super::os::set_autostart(on, &exe) {
                    Ok(()) => {
                        app.settings.lock().unwrap().autostart = on;
                        app.save_settings();
                    }
                    Err(e) => log(format!("autostart change failed: {e}")),
                }
            }
            Ok(Cmd::SetGuard(on)) => {
                app.settings.lock().unwrap().guard = on;
                app.save_settings();
                need_start = true;
                retry_at = None;
            }
            Ok(Cmd::Recalibrate) => {
                app.settings.lock().unwrap().calibration.clear();
                app.save_settings();
                need_start = true;
                retry_at = None;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        if let Some(s) = &session {
            if s.failed() {
                let msg = s.stats().error.unwrap_or_default();
                log(format!("session failed: {msg}"));
                app.set_status(Phase::Error, format!("Audio stopped: {msg}"), "Restarting...");
                need_start = true;
                retry_at = Some(Instant::now() + Duration::from_secs(2));
            } else if last_poll.elapsed() >= Duration::from_secs(1) {
                // Test mode: pick the virtual cable up as soon as it is installed.
                if app.test_mode.load(Ordering::Relaxed) && app.output_override.is_none() && qm_audio::find_virtual_cable().ok().flatten().is_some() {
                    log("virtual cable appeared; leaving test mode");
                    need_start = true;
                    retry_at = None;
                }
                // Following the Windows default microphone: restart when it changes (headset plugged in, etc.).
                if app.settings.lock().unwrap().input_id.is_none() {
                    let now = qm_audio::default_input().ok().map(|d| d.id);
                    if now.is_some() && now != started_default {
                        log(format!("default microphone changed: {started_default:?} -> {now:?}"));
                        need_start = true;
                        retry_at = None;
                    }
                }
                last_poll = Instant::now();
            }
            if !need_start {
                refresh_status(&app, s);
            }
        }
    }
    if let Some(s) = session.take() {
        s.stop();
    }
}
