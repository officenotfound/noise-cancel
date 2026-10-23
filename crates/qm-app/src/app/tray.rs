//! System tray (Windows) / menu bar (macOS) icon and menu. It reports user actions as [`TrayOut`]; the backend
//! applies them. The creating thread needs an event loop: the main window's loop provides it (on Windows,
//! `pump_messages` does the same in tray-only mode).

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};

use eframe::egui;
use qm_engine::Preset;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

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

/// The app icon (64x64, same artwork as the desktop icon) with a small status dot in the corner.
fn icon(rgb: (u8, u8, u8)) -> Icon {
    let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!("../../../../assets/tray-64.png").as_slice()));
    let mut reader = decoder.read_info().expect("tray icon png");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("tray icon size")];
    let info = reader.next_frame(&mut buf).expect("tray icon frame");
    let n = info.width as usize;
    let mut px = match info.color_type {
        png::ColorType::Rgba => buf[..n * n * 4].to_vec(),
        png::ColorType::Rgb => buf[..n * n * 3].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        other => panic!("unexpected tray icon colour type {other:?}"),
    };
    // status dot, bottom right: a dark ring around the status colour
    let (cx, cy, r) = (n as f32 * 0.74, n as f32 * 0.74, n as f32 * 0.22);
    for y in 0..n {
        for x in 0..n {
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let i = (y * n + x) * 4;
            if d <= r {
                let (c, a) = if d > r - 3.0 { ((20, 20, 20), 255) } else { (rgb, 255) };
                px[i] = c.0;
                px[i + 1] = c.1;
                px[i + 2] = c.2;
                px[i + 3] = a;
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
        let autostart = CheckMenuItem::new(super::os::AUTOSTART_LABEL, true, snap.autostart, None);
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
        simple(&format!("Get {} (virtual microphone)...", super::os::CABLE_PRODUCT), Action::Cmd(UiCmd::OpenVbCable));
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
                Phase::Starting | Phase::Calibrating | Phase::NoCable => (230, 160, 30),
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

/// Run the Win32 message loop for the tray when no window is available. Returns false when the user quits.
#[cfg(windows)]
pub fn pump_messages() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT};
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
