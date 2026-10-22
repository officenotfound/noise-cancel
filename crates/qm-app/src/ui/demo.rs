//! Simulated backend: lets the window run (and be screenshotted) on any OS without audio.

use std::time::Instant;

use qm_engine::Preset;

use super::model::{Backend, Phase, Snapshot, UiCmd};

pub struct DemoBackend {
    t0: Instant,
    snap: Snapshot,
}

impl DemoBackend {
    pub fn new(state: &str) -> Self {
        let phase = match state {
            "error" => Phase::Error,
            "bypass" => Phase::Bypass,
            "calibrating" => Phase::Calibrating,
            _ => Phase::Running,
        };
        let snap = Snapshot {
            phase,
            headline: "ON | dfn3-ll | est. delay 41 ms | CPU 14%".into(),
            detail: "Headset Microphone -> CABLE Input (VB-Audio Virtual Cable)".into(),
            model: "dfn3-ll".into(),
            est_delay_ms: 41.0,
            cpu_pct: 14.0,
            dropouts: 0,
            input_level: 0.2,
            output_level: 0.1,
            input_name: "Headset Microphone".into(),
            output_name: "CABLE Input (VB-Audio Virtual Cable)".into(),
            preset: Preset::Auto,
            bypass: phase == Phase::Bypass,
            guard: false,
            autostart: false,
            mics: vec![
                (None, "Windows default microphone".into()),
                (Some("mic1".into()), "Headset Microphone (USB Audio)".into()),
                (Some("mic2".into()), "Microphone Array (Realtek Audio)".into()),
            ],
            selected_mic: None,
            vb_cable_found: phase != Phase::Error,
            has_tray: false,
        };
        let mut d = Self { t0: Instant::now(), snap };
        if phase == Phase::Error {
            d.snap.headline = "Not running: VB-Audio Virtual Cable is not installed".into();
            d.snap.detail = "Install it (Tools > Get VB-Cable), then pick CABLE Output as the microphone in your calls.".into();
            d.snap.model.clear();
            d.snap.output_name.clear();
            d.snap.cpu_pct = 0.0;
            d.snap.est_delay_ms = 0.0;
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
        if matches!(s.phase, Phase::Running | Phase::Bypass) {
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
        s
    }

    fn command(&mut self, cmd: UiCmd) {
        match cmd {
            UiCmd::SetPreset(p) => self.snap.preset = p,
            UiCmd::SetInput(id) => self.snap.selected_mic = id,
            UiCmd::SetBypass(b) => {
                self.snap.bypass = b;
                self.snap.phase = if b { Phase::Bypass } else { Phase::Running };
                self.snap.headline = if b { "BYPASS: microphone passes through unprocessed".into() } else { "ON | dfn3-ll | est. delay 41 ms | CPU 14%".into() };
            }
            UiCmd::SetGuard(g) => self.snap.guard = g,
            UiCmd::SetAutostart(a) => self.snap.autostart = a,
            UiCmd::Quit => std::process::exit(0),
            _ => {}
        }
    }
}
