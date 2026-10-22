//! Simulated backend: lets the window run (and be screenshotted) on any OS without audio.

use std::time::Instant;

use qm_engine::Preset;

use super::model::{Backend, Phase, PlayState, RecState, Snapshot, UiCmd};

pub struct DemoBackend {
    t0: Instant,
    snap: Snapshot,
    rec_started: Option<Instant>,
    play_started: Option<(Instant, f32)>,
}

impl DemoBackend {
    pub fn new(state: &str) -> Self {
        let phase = match state {
            "error" => Phase::Error,
            "bypass" => Phase::Bypass,
            "nocable" => Phase::NoCable,
            "calibrating" => Phase::Calibrating,
            _ => Phase::Running,
        };
        let snap = Snapshot {
            phase,
            headline: "ON - dfn3-ll - est. delay 41 ms - CPU 14%".into(),
            detail: "Headset Microphone -> CABLE Input - dropouts 0".into(),
            model: "dfn3-ll".into(),
            est_delay_ms: 41.0,
            cpu_pct: 14.0,
            dropouts: 0,
            input_level: 0.2,
            output_level: 0.1,
            input_name: "Headset Microphone".into(),
            output_name: if cfg!(target_os = "macos") { "BlackHole 2ch" } else { "CABLE Input (VB-Audio Virtual Cable)" }.into(),
            preset: Preset::Auto,
            bypass: phase == Phase::Bypass,
            guard: false,
            autostart: false,
            mics: vec![
                (None, crate::app::os::DEFAULT_MIC_LABEL.into()),
                (Some("mic1".into()), "Headset Microphone (USB Audio)".into()),
                (Some("mic2".into()), "Microphone Array (Realtek Audio)".into()),
            ],
            selected_mic: None,
            vb_cable_found: phase != Phase::Error,
            has_tray: false,
            rec: RecState::default(),
        };
        let mut d = Self { t0: Instant::now(), snap, rec_started: None, play_started: None };
        match state {
            "recording" => d.rec_started = Some(Instant::now()),
            "recorded" | "playing" => {
                d.snap.rec = RecState { has_recording: true, total_secs: 12.0, play_secs: 4.0, ..Default::default() };
                if state == "playing" {
                    d.snap.rec.play = PlayState::Playing;
                }
            }
            _ => {}
        }
        if phase == Phase::Error {
            d.snap.headline = format!("Not running: {} is not installed", crate::app::os::CABLE_PRODUCT).into();
            d.snap.detail = format!("Install {}, then choose {} as your microphone.", crate::app::os::CABLE_PRODUCT, crate::app::os::CABLE_PICK).into();
            d.snap.model.clear();
            d.snap.output_name.clear();
            d.snap.cpu_pct = 0.0;
            d.snap.est_delay_ms = 0.0;
        }
        if phase == Phase::NoCable {
            d.snap.headline = format!("TEST MODE: {} is not installed", crate::app::os::CABLE_PRODUCT);
            d.snap.detail = "Other apps cannot hear you yet. Try the recorder below.".into();
            d.snap.output_name.clear();
            d.snap.vb_cable_found = false;
        }
        if phase == Phase::Bypass {
            d.snap.headline = "BYPASS: microphone passes through unprocessed".into();
        }
        if phase == Phase::Calibrating {
            d.snap.headline = "Measuring this PC's speed (first run, a few seconds)...".into();
            d.snap.detail.clear();
        }
        d
    }
}

impl Backend for DemoBackend {
    fn snapshot(&mut self) -> Snapshot {
        let t = self.t0.elapsed().as_secs_f32();
        let mut s = self.snap.clone();
        if matches!(s.phase, Phase::Running | Phase::Bypass | Phase::NoCable) {
            let talk = ((t * 1.7).sin() * 0.5 + 0.5).powi(2);
            s.input_level = 0.04 + 0.5 * talk;
            s.output_level = if s.phase == Phase::Bypass { s.input_level } else { 0.01 + 0.35 * talk };
            if s.phase == Phase::Running {
                s.cpu_pct = 14.0 + 3.0 * (t * 0.7).sin();
            }
        } else {
            s.input_level = 0.0;
            s.output_level = 0.0;
        }
        if let Some(t) = self.rec_started {
            s.rec.recording = true;
            s.rec.rec_secs = if self.snap.rec.total_secs > 0.0 && false { 0.0 } else { t.elapsed().as_secs_f32() + 7.0 };
        }
        if s.rec.play == PlayState::Playing {
            s.rec.play_secs = match self.play_started {
                Some((t, from)) => (from + t.elapsed().as_secs_f32()).min(s.rec.total_secs),
                None => s.rec.play_secs,
            };
        }
        s
    }

    fn command(&mut self, cmd: UiCmd) {
        match cmd {
            UiCmd::SetPreset(p) => self.snap.preset = p,
            UiCmd::SetInput(id) => self.snap.selected_mic = id,
            UiCmd::SetBypass(b) => {
                self.snap.bypass = b;
                self.snap.phase = if b { Phase::Bypass } else { Phase::Running };
                self.snap.headline = if b { "BYPASS: microphone passes through unprocessed".into() } else { "ON - dfn3-ll - est. delay 41 ms - CPU 14%".into() };
            }
            UiCmd::SetGuard(g) => self.snap.guard = g,
            UiCmd::SetAutostart(a) => self.snap.autostart = a,
            UiCmd::RecordToggle => {
                if let Some(t) = self.rec_started.take() {
                    self.snap.rec = RecState { has_recording: true, total_secs: t.elapsed().as_secs_f32(), play_original: self.snap.rec.play_original, ..Default::default() };
                } else {
                    self.snap.rec = RecState { play_original: self.snap.rec.play_original, ..Default::default() };
                    self.rec_started = Some(Instant::now());
                }
            }
            UiCmd::PlayToggle => match self.snap.rec.play {
                PlayState::Playing => {
                    self.snap.rec.play = PlayState::Paused;
                    if let Some((t, from)) = self.play_started.take() {
                        self.snap.rec.play_secs = (from + t.elapsed().as_secs_f32()).min(self.snap.rec.total_secs);
                    }
                }
                _ => {
                    self.snap.rec.play = PlayState::Playing;
                    if self.snap.rec.play_secs >= self.snap.rec.total_secs {
                        self.snap.rec.play_secs = 0.0;
                    }
                    self.play_started = Some((Instant::now(), self.snap.rec.play_secs));
                }
            },
            UiCmd::SetPlayOriginal(o) => self.snap.rec.play_original = o,
            UiCmd::Quit => std::process::exit(0),
            _ => {}
        }
    }
}
