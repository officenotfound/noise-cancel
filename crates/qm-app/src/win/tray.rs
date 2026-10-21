//! System tray icon and menu. Runs the Win32 message pump on the main thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Result;
use qm_engine::Preset;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIconBuilder};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use super::controller::{Cmd, Controller, Phase};
use super::logging::{self, log};

const VB_CABLE_URL: &str = "https://vb-audio.com/Cable/";

#[derive(Clone, Debug)]
enum Action {
    Preset(Preset),
    Input(Option<String>),
    RefreshMics,
    Bypass,
    Autostart,
    Recalibrate,
    OpenReport,
    OpenLog,
    OpenVbCable,
    Quit,
}

pub fn set_autostart(on: bool, exe: &Path) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")?;
    if on {
        key.set_value("QuietMic", &format!("\"{}\"", exe.display()))?;
    } else {
        let _ = key.delete_value("QuietMic");
    }
    Ok(())
}

fn autostart_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .and_then(|k| k.get_value::<String, _>("QuietMic"))
        .is_ok()
}

/// 32x32 filled circle with a darker ring.
fn icon(rgb: (u8, u8, u8)) -> Icon {
    let n = 32usize;
    let mut px = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let d = (dx * dx + dy * dy).sqrt();
            let i = (y * n + x) * 4;
            if d <= 14.0 {
                let ring = d > 11.0;
                let k = if ring { 0.6 } else { 1.0 };
                px[i] = (rgb.0 as f32 * k) as u8;
                px[i + 1] = (rgb.1 as f32 * k) as u8;
                px[i + 2] = (rgb.2 as f32 * k) as u8;
                px[i + 3] = 255;
            }
        }
    }
    Icon::from_rgba(px, n as u32, n as u32).expect("valid icon")
}

fn open(path: impl AsRef<std::ffi::OsStr>) {
    let _ = Command::new("explorer.exe").arg(path).spawn();
}

fn notepad(path: &Path) {
    let _ = Command::new("notepad.exe").arg(path).spawn();
}

struct Ui {
    actions: HashMap<MenuId, Action>,
    status: MenuItem,
    detail: MenuItem,
    bypass: CheckMenuItem,
    presets: Vec<(Preset, CheckMenuItem)>,
    mics_menu: Submenu,
    mics: Vec<(Option<String>, CheckMenuItem)>,
    autostart: CheckMenuItem,
}

impl Ui {
    fn rebuild_mics(&mut self, selected: &Option<String>) {
        while self.mics_menu.remove_at(0).is_some() {}
        for (id, _) in self.mics.drain(..) {
            self.actions.retain(|_, a| !matches!(a, Action::Input(i) if *i == id));
        }
        let mut entries: Vec<(Option<String>, String)> = vec![(None, "Windows default microphone".into())];
        match qm_win::list_inputs() {
            Ok(list) => entries.extend(list.into_iter().map(|d| (Some(d.id), d.name))),
            Err(e) => log(format!("listing microphones failed: {e}")),
        }
        for (id, name) in entries {
            let item = CheckMenuItem::new(name, true, id == *selected, None);
            self.actions.insert(item.id().clone(), Action::Input(id.clone()));
            let _ = self.mics_menu.append(&item);
            self.mics.push((id, item));
        }
        let refresh = MenuItem::new("Refresh list", true, None);
        self.actions.insert(refresh.id().clone(), Action::RefreshMics);
        let _ = self.mics_menu.append(&PredefinedMenuItem::separator());
        let _ = self.mics_menu.append(&refresh);
    }
}

fn build_ui(ctl: &Controller) -> (Menu, Ui) {
    let settings = ctl.settings.lock().unwrap().clone();
    let menu = Menu::new();
    let mut actions = HashMap::new();

    let status = MenuItem::new("Starting...", false, None);
    let detail = MenuItem::new("", false, None);
    let _ = menu.append_items(&[&status, &detail, &PredefinedMenuItem::separator()]);

    let bypass = CheckMenuItem::new("Bypass (turn noise removal off)", true, settings.bypass, None);
    actions.insert(bypass.id().clone(), Action::Bypass);
    let _ = menu.append(&bypass);

    let presets_menu = Submenu::new("Preset", true);
    let mut presets = Vec::new();
    for p in Preset::ALL {
        let label = match p {
            Preset::Auto => "Auto (recommended)",
            Preset::LowLatency => "Low latency",
            Preset::Balanced => "Balanced",
            Preset::MaxNoiseRemoval => "Max noise removal (more delay)",
            Preset::MaxVoiceSeparation => "Max voice separation",
            Preset::Light => "Light (old or slow PCs)",
        };
        let item = CheckMenuItem::new(label, true, p == settings.preset, None);
        actions.insert(item.id().clone(), Action::Preset(p));
        let _ = presets_menu.append(&item);
        presets.push((p, item));
    }
    let _ = menu.append(&presets_menu);

    let mics_menu = Submenu::new("Microphone", true);
    let _ = menu.append(&mics_menu);

    let autostart = CheckMenuItem::new("Start with Windows", true, autostart_enabled(), None);
    actions.insert(autostart.id().clone(), Action::Autostart);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&autostart);

    let mut simple = |text: &str, a: Action| {
        let item = MenuItem::new(text, true, None);
        actions.insert(item.id().clone(), a);
        let _ = menu.append(&item);
    };
    simple("Re-measure this PC", Action::Recalibrate);
    simple("Performance report...", Action::OpenReport);
    simple("Open log...", Action::OpenLog);
    simple("Get VB-Cable (virtual microphone)...", Action::OpenVbCable);
    let _ = menu.append(&PredefinedMenuItem::separator());
    simple("Quit QuietMic", Action::Quit);

    let mut ui = Ui { actions, status, detail, bypass, presets, mics_menu, mics: Vec::new(), autostart };
    ui.rebuild_mics(&settings.input_id);
    (menu, ui)
}

pub fn run(ctl: Controller, dir: PathBuf) {
    let (menu, mut ui) = build_ui(&ctl);
    let tray = match TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("QuietMic").with_icon(icon((230, 160, 30))).build() {
        Ok(t) => t,
        Err(e) => {
            log(format!("could not create tray icon: {e}"));
            return;
        }
    };

    let mut last_ui = Instant::now() - Duration::from_secs(1);
    let mut last_phase = None;
    let mut last_tooltip = String::new();
    'outer: loop {
        // Win32 message pump (needed for the tray icon and its menu).
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            if msg.message == WM_QUIT {
                break 'outer;
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            let Some(action) = ui.actions.get(&ev.id).cloned() else { continue };
            match action {
                Action::Preset(p) => {
                    for (q, item) in &ui.presets {
                        item.set_checked(*q == p);
                    }
                    ctl.send(Cmd::SetPreset(p));
                }
                Action::Input(id) => {
                    for (q, item) in &ui.mics {
                        item.set_checked(*q == id);
                    }
                    ctl.send(Cmd::SetInput(id));
                }
                Action::RefreshMics => {
                    let sel = ctl.settings.lock().unwrap().input_id.clone();
                    ui.rebuild_mics(&sel);
                }
                Action::Bypass => ctl.send(Cmd::SetBypass(ui.bypass.is_checked())),
                Action::Autostart => ctl.send(Cmd::SetAutostart(ui.autostart.is_checked())),
                Action::Recalibrate => ctl.send(Cmd::Recalibrate),
                Action::OpenReport => notepad(&dir.join("performance_report.txt")),
                Action::OpenLog => notepad(&logging::path(&dir)),
                Action::OpenVbCable => open(VB_CABLE_URL),
                Action::Quit => break 'outer,
            }
        }

        if last_ui.elapsed() >= Duration::from_millis(500) {
            last_ui = Instant::now();
            let st = ctl.status.lock().unwrap().clone();
            ui.status.set_text(&st.headline);
            ui.detail.set_text(&st.detail);
            if last_phase != Some(st.phase) {
                last_phase = Some(st.phase);
                let rgb = match st.phase {
                    Phase::Running => (40, 190, 90),
                    Phase::Bypass => (150, 150, 150),
                    Phase::Error => (215, 60, 60),
                    Phase::Starting | Phase::Calibrating => (230, 160, 30),
                };
                let _ = tray.set_icon(Some(icon(rgb)));
            }
            let tip: String = format!("QuietMic: {}", st.headline).chars().take(120).collect();
            if tip != last_tooltip {
                let _ = tray.set_tooltip(Some(&tip));
                last_tooltip = tip;
            }
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    drop(tray);
    ctl.shutdown();
}
