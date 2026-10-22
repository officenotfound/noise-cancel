//! What the window shows and what it can ask for. The window knows nothing about audio: it talks to a [`Backend`].

use qm_engine::Preset;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    Calibrating,
    Running,
    Bypass,
    Error,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub phase: Phase,
    /// Short status line, e.g. "ON | dfn3-ll | est. delay 41 ms | CPU 14%".
    pub headline: String,
    pub detail: String,
    pub model: String,
    pub est_delay_ms: f32,
    pub cpu_pct: f32,
    pub dropouts: u64,
    /// Linear peak levels 0.0..=1.0 of the microphone and the cleaned output.
    pub input_level: f32,
    pub output_level: f32,
    pub input_name: String,
    pub output_name: String,
    pub preset: Preset,
    pub bypass: bool,
    pub guard: bool,
    pub autostart: bool,
    pub mics: Vec<(Option<String>, String)>,
    pub selected_mic: Option<String>,
    pub vb_cable_found: bool,
    /// True when closing the window should hide it to the tray instead of quitting.
    pub has_tray: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UiCmd {
    SetPreset(Preset),
    SetInput(Option<String>),
    SetBypass(bool),
    SetGuard(bool),
    SetAutostart(bool),
    RefreshMics,
    Recalibrate,
    OpenReport,
    OpenLog,
    OpenVbCable,
    HideToTray,
    Quit,
}

/// Requests from outside the window (the tray icon) to change the window itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCmd {
    Show,
    Hide,
    Quit,
}

pub trait Backend {
    fn snapshot(&mut self) -> Snapshot;
    fn command(&mut self, cmd: UiCmd);
    /// Window requests coming from the tray icon, if any.
    fn poll_window_cmd(&mut self) -> Option<WindowCmd> {
        None
    }
}

/// (label, one-line help shown in the status bar) for each preset, in menu order.
pub fn preset_info(p: Preset) -> (&'static str, &'static str) {
    match p {
        Preset::Auto => ("Auto (recommended)", "Measures this PC and picks the strongest low-delay model it can run safely."),
        Preset::LowLatency => ("Low latency", "Lowest delay first, then noise removal. Best for live calls."),
        Preset::Balanced => ("Balanced", "Stronger noise removal for a little more delay and CPU."),
        Preset::MaxNoiseRemoval => ("Max noise removal", "Strongest suppression. Adds about 50 ms of delay and turns the noise guard on."),
        Preset::MaxVoiceSeparation => ("Max voice separation", "Tighter on other and distant voices. Noise guard on."),
        Preset::Light => ("Light", "For old or slow PCs. Smallest models, lowest CPU."),
    }
}
