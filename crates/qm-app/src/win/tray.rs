//! System tray icon and menu. The tray reports user actions as [`TrayOut`]; the window backend (or the tray-only
//! fallback loop) applies them. Windows needs a message loop on the creating thread: the main window's event loop
//! provides it, and `pump_messages` does the same in tray-only mode.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};

use anyhow::Result;
use eframe::egui;
use qm_engine::Preset;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use super::logging::log;
use crate::ui::model::{preset_info, Phase, Snapshot, UiCmd, WindowCmd};

#[derive(Clone, Debug)]
enum Action {
    Show,
    Cmd(UiCmd),
}

pub enum TrayOut {
    Cmd(UiCmd),
    Window(WindowCmd),
}

enum Input {
    Menu(MenuEvent),
    Icon(TrayIconEvent),
}

pub fn set_autostart(on: bool, exe: &Path) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")?;
    if on {
        // Start hidden in the tray; the window opens when the icon is clicked.
        key.set_value("QuietMic", &format!("\"{}\" --minimized", exe.display()))?;
    } else {
        let _ = key.delete_value("QuietMic");
    }
    Ok(())
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
                let k = if d > 11.0 { 0.6 } else { 1.0 };
                px[i] = (rgb.0 as f32 * k) as u8;
                px[i + 1] = (rgb.1 as f32 * k) as u8;
                px[i + 2] = (rgb.2 as f32 * k) as u8;
                px[i + 3] = 255;
            }
        }
    }
    Icon::from_rgba(px, n as u32, n as u32).expect("valid icon")
}

pub struct Tray {
    icon: TrayIcon,
    rx: Receiver<Input>,
    actions: HashMap<MenuId, Action>,
    status: MenuItem,
    detail: MenuItem,
    bypass: CheckMenuItem,
    presets: Vec<(Preset, CheckMenuItem)>,
    guard: CheckMenuItem,
    autostart: CheckMenuItem,
    mics_menu: Submenu,
    mics: Vec<(Option<String>, CheckMenuItem)>,
    mic_list: Vec<(Option<String>, String)>,
    last_phase: Option<Phase>,
    last_status: String,
    last_detail: String,
    last_tip: String,
}

impl Tray {
    /// `ctx` lets tray clicks wake the (possibly hidden) window so they are handled promptly.
    pub fn new(snap: &Snapshot, ctx: Option<egui::Context>) -> Option<Tray> {
        let (tx, rx) = mpsc::channel();
        {
            let (tx, ctx) = (tx.clone(), ctx.clone());
            MenuEvent::set_event_handler(Some(move |e| {
                let _ = tx.send(Input::Menu(e));
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            }));
        }
        TrayIconEvent::set_event_handler(Some(move |e| {
            let _ = tx.send(Input::Icon(e));
            if let Some(c) = &ctx {
                c.request_repaint();
            }
        }));

        let menu = Menu::new();
        let mut actions = HashMap::new();
        let status = MenuItem::new("Starting...", false, None);
        let detail = MenuItem::new("", false, None);
        let _ = menu.append_items(&[&status, &detail, &PredefinedMenuItem::separator()]);

        let show = MenuItem::new("Open QuietMic", true, None);
        actions.insert(show.id().clone(), Action::Show);
        let _ = menu.append(&show);

        let bypass = CheckMenuItem::new("Bypass (turn noise removal off)", true, snap.bypass, None);
        actions.insert(bypass.id().clone(), Action::Cmd(UiCmd::SetBypass(!snap.bypass)));
        let _ = menu.append(&bypass);

        let presets_menu = Submenu::new("Preset", true);
        let mut presets = Vec::new();
        for p in Preset::ALL {
            let item = CheckMenuItem::new(preset_info(p).0, true, p == snap.preset, None);
            actions.insert(item.id().clone(), Action::Cmd(UiCmd::SetPreset(p)));
            let _ = presets_menu.append(&item);
            presets.push((p, item));
        }
        let _ = menu.append(&presets_menu);

        let mics_menu = Submenu::new("Microphone", true);
        let _ = menu.append(&mics_menu);

        let guard = CheckMenuItem::new("Extra noise guard (about +13 ms delay)", true, snap.guard, None);
        actions.insert(guard.id().clone(), Action::Cmd(UiCmd::SetGuard(!snap.guard)));
        let _ = menu.append(&guard);
        let autostart = CheckMenuItem::new("Start with Windows", true, snap.autostart, None);
        actions.insert(autostart.id().clone(), Action::Cmd(UiCmd::SetAutostart(!snap.autostart)));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&autostart);

        let mut simple = |text: &str, a: Action| {
            let item = MenuItem::new(text, true, None);
            actions.insert(item.id().clone(), a);
            let _ = menu.append(&item);
        };
        simple("Re-measure this PC", Action::Cmd(UiCmd::Recalibrate));
        simple("Performance report...", Action::Cmd(UiCmd::OpenReport));
        simple("Open log...", Action::Cmd(UiCmd::OpenLog));
        simple("Get VB-Cable (virtual microphone)...", Action::Cmd(UiCmd::OpenVbCable));
        let _ = menu.append(&PredefinedMenuItem::separator());
        simple("Quit QuietMic", Action::Cmd(UiCmd::Quit));

        let built = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("QuietMic")
            .with_icon(icon((230, 160, 30)))
            .build();
        let icon = match built {
            Ok(i) => i,
            Err(e) => {
                log(format!("could not create tray icon: {e}"));
                return None;
            }
        };
        let mut t = Tray {
            icon,
            rx,
            actions,
            status,
            detail,
            bypass,
            presets,
            guard,
            autostart,
            mics_menu,
            mics: Vec::new(),
            mic_list: Vec::new(),
            last_phase: None,
            last_status: String::new(),
            last_detail: String::new(),
            last_tip: String::new(),
        };
        t.update(snap);
        Some(t)
    }

    fn rebuild_mics(&mut self, snap: &Snapshot) {
        while self.mics_menu.remove_at(0).is_some() {}
        for (id, _) in self.mics.drain(..) {
            self.actions.retain(|_, a| !matches!(a, Action::Cmd(UiCmd::SetInput(i)) if *i == id));
        }
        for (id, name) in &snap.mics {
            let item = CheckMenuItem::new(name, true, *id == snap.selected_mic, None);
            self.actions.insert(item.id().clone(), Action::Cmd(UiCmd::SetInput(id.clone())));
            let _ = self.mics_menu.append(&item);
            self.mics.push((id.clone(), item));
        }
        let refresh = MenuItem::new("Refresh list", true, None);
        self.actions.insert(refresh.id().clone(), Action::Cmd(UiCmd::RefreshMics));
        let _ = self.mics_menu.append(&PredefinedMenuItem::separator());
        let _ = self.mics_menu.append(&refresh);
        self.mic_list = snap.mics.clone();
    }

    /// Bring the menu texts, check marks, icon colour and tooltip in line with `snap`.
    pub fn update(&mut self, snap: &Snapshot) {
        if snap.headline != self.last_status {
            self.status.set_text(&snap.headline);
            self.last_status = snap.headline.clone();
        }
        if snap.detail != self.last_detail {
            self.detail.set_text(&snap.detail);
            self.last_detail = snap.detail.clone();
        }
        self.bypass.set_checked(snap.bypass);
        self.guard.set_checked(snap.guard);
        self.autostart.set_checked(snap.autostart);
        for (p, item) in &self.presets {
            item.set_checked(*p == snap.preset);
        }
        if snap.mics != self.mic_list {
            self.rebuild_mics(snap);
        }
        for (id, item) in &self.mics {
            item.set_checked(*id == snap.selected_mic);
        }
        // The menu items hold the *next* value for toggles.
        self.actions.insert(self.bypass.id().clone(), Action::Cmd(UiCmd::SetBypass(!snap.bypass)));
        self.actions.insert(self.guard.id().clone(), Action::Cmd(UiCmd::SetGuard(!snap.guard)));
        self.actions.insert(self.autostart.id().clone(), Action::Cmd(UiCmd::SetAutostart(!snap.autostart)));

        if self.last_phase != Some(snap.phase) {
            self.last_phase = Some(snap.phase);
            let rgb = match snap.phase {
                Phase::Running => (40, 190, 90),
                Phase::Bypass => (150, 150, 150),
                Phase::Error => (215, 60, 60),
                Phase::Starting | Phase::Calibrating => (230, 160, 30),
            };
            let _ = self.icon.set_icon(Some(icon(rgb)));
        }
        let tip: String = format!("QuietMic: {}", snap.headline).chars().take(120).collect();
        if tip != self.last_tip {
            let _ = self.icon.set_tooltip(Some(&tip));
            self.last_tip = tip;
        }
    }

    /// Collect what the user did with the tray since the last call.
    pub fn poll(&mut self) -> Vec<TrayOut> {
        let mut out = Vec::new();
        while let Ok(input) = self.rx.try_recv() {
            match input {
                Input::Menu(ev) => match self.actions.get(&ev.id).cloned() {
                    Some(Action::Show) => out.push(TrayOut::Window(WindowCmd::Show)),
                    Some(Action::Cmd(c)) => out.push(TrayOut::Cmd(c)),
                    None => {}
                },
                Input::Icon(TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. })
                | Input::Icon(TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }) => out.push(TrayOut::Window(WindowCmd::Show)),
                Input::Icon(_) => {}
            }
        }
        out
    }
}

/// Run the Win32 message loop for the tray when no window is available. Returns when the user quits.
pub fn pump_messages() -> bool {
    let mut msg = MSG::default();
    while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
        if msg.message == WM_QUIT {
            return false;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}
