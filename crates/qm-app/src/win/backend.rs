//! Connects the retro window and the tray to the real audio controller.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

use super::controller::{Cmd, Controller};
use super::logging;
use super::tray::{self, Tray, TrayOut};
use crate::ui::model::{Backend, Snapshot, UiCmd, WindowCmd};

const VB_CABLE_URL: &str = "https://vb-audio.com/Cable/";

pub struct WinBackend {
    ctl: Arc<Controller>,
    dir: PathBuf,
    mics: Vec<(Option<String>, String)>,
    vb_found: bool,
    vb_checked: Option<Instant>,
    smooth_in: f32,
    smooth_out: f32,
    pending: VecDeque<WindowCmd>,
    tray: Option<Tray>,
}

fn list_mics() -> Vec<(Option<String>, String)> {
    let mut v = vec![(None, "Windows default microphone".to_string())];
    match qm_win::list_inputs() {
        Ok(l) => v.extend(l.into_iter().map(|d| (Some(d.id), d.name))),
        Err(e) => logging::log(format!("listing microphones failed: {e}")),
    }
    v
}

impl WinBackend {
    /// `ctx` is the egui context when a window exists (so tray clicks can wake it), `None` in tray-only mode.
    pub fn new(ctl: Arc<Controller>, dir: PathBuf, ctx: Option<egui::Context>) -> Self {
        let mut b = Self {
            ctl,
            dir,
            mics: list_mics(),
            vb_found: false,
            vb_checked: None,
            smooth_in: 0.0,
            smooth_out: 0.0,
            pending: VecDeque::new(),
            tray: None,
        };
        let snap = b.build_snapshot(false);
        b.tray = Tray::new(&snap, ctx);
        b
    }

    fn build_snapshot(&mut self, has_tray: bool) -> Snapshot {
        if self.vb_checked.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
            self.vb_found = qm_win::find_vb_cable().ok().flatten().is_some();
            self.vb_checked = Some(Instant::now());
        }
        let status = self.ctl.status.lock().unwrap().clone();
        let settings = self.ctl.settings.lock().unwrap().clone();
        let live = self.ctl.live.lock().unwrap().clone();
        // Peak-hold with decay so the meters are readable.
        self.smooth_in = live.input_peak.max(self.smooth_in * 0.85);
        self.smooth_out = live.output_peak.max(self.smooth_out * 0.85);
        Snapshot {
            phase: status.phase,
            headline: status.headline,
            detail: status.detail,
            model: live.model,
            est_delay_ms: live.est_delay_ms,
            cpu_pct: live.cpu_pct,
            dropouts: live.dropouts,
            input_level: self.smooth_in,
            output_level: self.smooth_out,
            input_name: live.input_name,
            output_name: live.output_name,
            preset: settings.preset,
            bypass: settings.bypass,
            guard: settings.guard,
            autostart: settings.autostart,
            mics: self.mics.clone(),
            selected_mic: settings.input_id,
            vb_cable_found: self.vb_found,
            has_tray,
        }
    }
}

fn open(target: impl AsRef<std::ffi::OsStr>) {
    let _ = Command::new("explorer.exe").arg(target).spawn();
}

fn notepad(path: &std::path::Path) {
    let _ = Command::new("notepad.exe").arg(path).spawn();
}

impl Backend for WinBackend {
    fn snapshot(&mut self) -> Snapshot {
        let has_tray = self.tray.is_some();
        let snap = self.build_snapshot(has_tray);
        if let Some(t) = &mut self.tray {
            t.update(&snap);
        }
        snap
    }

    fn command(&mut self, cmd: UiCmd) {
        match cmd {
            UiCmd::SetPreset(p) => self.ctl.send(Cmd::SetPreset(p)),
            UiCmd::SetInput(id) => self.ctl.send(Cmd::SetInput(id)),
            UiCmd::SetBypass(b) => self.ctl.send(Cmd::SetBypass(b)),
            UiCmd::SetGuard(g) => self.ctl.send(Cmd::SetGuard(g)),
            UiCmd::SetAutostart(a) => self.ctl.send(Cmd::SetAutostart(a)),
            UiCmd::RefreshMics => self.mics = list_mics(),
            UiCmd::Recalibrate => self.ctl.send(Cmd::Recalibrate),
            UiCmd::OpenReport => notepad(&self.dir.join("performance_report.txt")),
            UiCmd::OpenLog => notepad(&logging::path(&self.dir)),
            UiCmd::OpenVbCable => open(VB_CABLE_URL),
            UiCmd::HideToTray => self.pending.push_back(WindowCmd::Hide),
            UiCmd::Quit => self.pending.push_back(WindowCmd::Quit),
        }
    }

    fn poll_window_cmd(&mut self) -> Option<WindowCmd> {
        if let Some(t) = &mut self.tray {
            for out in t.poll() {
                match out {
                    TrayOut::Cmd(c) => self.command(c),
                    TrayOut::Window(w) => self.pending.push_back(w),
                }
            }
        }
        self.pending.pop_front()
    }
}

/// Fallback when the window cannot be created (for example no OpenGL): keep the tray icon and menu working.
pub fn run_tray_only(mut backend: WinBackend) {
    loop {
        if !tray::pump_messages() {
            return;
        }
        let _ = backend.snapshot();
        while let Some(w) = backend.poll_window_cmd() {
            if w == WindowCmd::Quit {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}
