//! Connects the retro window and the tray to the real audio controller.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

use super::controller::{Cmd, Controller};
use super::logging;
use super::os;
use super::tray::{Tray, TrayOut};
use qm_audio::player_core::PlayShared;
use qm_audio::Player;

use crate::recorder::{Recorder, SAMPLE_RATE};
use crate::ui::model::{Backend, PlayState, RecState, Snapshot, UiCmd, WindowCmd};

pub struct AppBackend {
    ctl: Arc<Controller>,
    dir: PathBuf,
    mics: Vec<(Option<String>, String)>,
    vb_found: bool,
    vb_checked: Option<Instant>,
    smooth_in: f32,
    smooth_out: f32,
    pending: VecDeque<WindowCmd>,
    tray: Option<Tray>,
    recorder: Arc<Recorder>,
    player: Option<(Player, Arc<PlayShared>)>,
    play_original: bool,
    was_recording: bool,
}

fn list_mics() -> Vec<(Option<String>, String)> {
    let mut v = vec![(None, os::DEFAULT_MIC_LABEL.to_string())];
    match qm_audio::list_inputs() {
        Ok(l) => v.extend(l.into_iter().map(|d| (Some(d.id), d.name))),
        Err(e) => logging::log(format!("listing microphones failed: {e}")),
    }
    v
}

impl AppBackend {
    /// `ctx` is the egui context when a window exists (so tray clicks can wake it), `None` in tray-only mode.
    pub fn new(ctl: Arc<Controller>, dir: PathBuf, ctx: Option<egui::Context>) -> Self {
        let ctl_recorder = ctl.recorder.clone();
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
            recorder: ctl_recorder,
            player: None,
            play_original: false,
            was_recording: false,
        };
        let snap = b.build_snapshot(false);
        b.tray = Tray::new(&snap, ctx);
        b
    }

    fn build_snapshot(&mut self, has_tray: bool) -> Snapshot {
        if self.vb_checked.map_or(true, |t| t.elapsed() > Duration::from_secs(3)) {
            self.vb_found = qm_audio::find_virtual_cable().ok().flatten().is_some();
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
            rec: self.rec_state(),
        }
    }

    fn rec_state(&mut self) -> RecState {
        // A recording that ended by itself (length cap) still has to be saved.
        let recording = self.recorder.is_recording();
        if self.was_recording && !recording {
            let _ = self.recorder.stop(&self.dir);
        }
        self.was_recording = recording;

        let mut play = PlayState::Stopped;
        let mut play_secs = 0.0;
        let finished = self.player.as_ref().map_or(false, |(_, s)| s.is_finished());
        if finished {
            self.stop_player();
        } else if let Some((_, shared)) = &self.player {
            play_secs = shared.position() as f32 / SAMPLE_RATE as f32;
            play = if shared.is_playing() { PlayState::Playing } else { PlayState::Paused };
        }
        RecState {
            recording,
            has_recording: self.recorder.has_recording(),
            play,
            rec_secs: self.recorder.seconds(),
            total_secs: self.recorder.seconds(),
            play_secs,
            play_original: self.play_original,
        }
    }

    fn stop_player(&mut self) {
        if let Some((p, _)) = self.player.take() {
            p.stop();
        }
    }

    fn toggle_record(&mut self) {
        if self.recorder.is_recording() {
            if let Err(e) = self.recorder.stop(&self.dir) {
                logging::log(format!("saving the recording failed: {e}"));
            }
        } else {
            self.stop_player();
            self.recorder.start(&self.dir); // also deletes the previous take
        }
    }

    fn toggle_play(&mut self) {
        if self.recorder.is_recording() || !self.recorder.has_recording() {
            return;
        }
        if let Some((_, shared)) = &self.player {
            if shared.is_playing() {
                shared.pause();
            } else {
                shared.play();
            }
            return;
        }
        let shared = PlayShared::new(self.recorder.buffer(self.play_original));
        match Player::start(shared.clone(), None) {
            Ok(p) => {
                shared.play();
                self.player = Some((p, shared));
            }
            Err(e) => logging::log(format!("could not start playback: {e:#}")),
        }
    }
}

impl Backend for AppBackend {
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
            UiCmd::OpenReport => os::open_text_file(&self.dir.join("performance_report.txt")),
            UiCmd::OpenLog => os::open_text_file(&logging::path(&self.dir)),
            UiCmd::OpenVbCable => os::open_url(os::CABLE_URL),
            UiCmd::HideToTray => self.pending.push_back(WindowCmd::Hide),
            UiCmd::RecordToggle => self.toggle_record(),
            UiCmd::PlayToggle => self.toggle_play(),
            UiCmd::SetPlayOriginal(o) => {
                self.play_original = o;
                if let Some((_, shared)) = &self.player {
                    shared.switch_samples(self.recorder.buffer(o));
                }
            }
            UiCmd::Quit => {
                self.stop_player();
                self.pending.push_back(WindowCmd::Quit);
            }
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
#[cfg(windows)]
pub fn run_tray_only(mut backend: AppBackend) {
    loop {
        if !super::tray::pump_messages() {
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
