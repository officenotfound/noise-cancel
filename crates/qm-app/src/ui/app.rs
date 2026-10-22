use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{self, Align2, Layout, Pos2, Rect, Sense, Vec2};
use qm_engine::Preset;

use super::model::{preset_info, Backend, Phase, Snapshot, UiCmd, WindowCmd};
use super::theme::*;
use super::widgets::*;

/// Builds the backend once the window exists (the tray icon needs the running event loop and the egui context).
pub type BackendFactory = Box<dyn FnOnce(&egui::Context) -> Box<dyn Backend>>;

pub const WINDOW_SIZE: [f32; 2] = [620.0, 480.0];

#[derive(Default)]
pub struct WindowOptions {
    /// Save a PNG of the window shortly after start, then exit (used to preview the design).
    pub screenshot: Option<PathBuf>,
    /// Start hidden (tray only) until the tray icon asks to show the window.
    pub start_hidden: bool,
    /// Preview helper: pre-open "file", "options", "help" or "about".
    pub open: Option<String>,
}

struct RetroApp {
    backend: Box<dyn Backend>,
    snap: Snapshot,
    help: String,
    about: bool,
    shot: Option<(PathBuf, u32)>,
    open: Option<String>,
    quitting: bool,
}

fn level_frac(level: f32) -> (f32, String) {
    let db = 20.0 * level.max(1e-5).log10();
    let text = if db < -80.0 { "  -- dB".to_string() } else { format!("{db:>5.0} dB") };
    (((db + 60.0) / 60.0).clamp(0.0, 1.0), text)
}

impl RetroApp {
    fn new(cc: &eframe::CreationContext<'_>, make: BackendFactory, opts: &WindowOptions) -> Self {
        let mut backend = make(&cc.egui_ctx);
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.spacing.item_spacing = Vec2::new(6.0, 4.0);
        style.visuals.override_text_color = Some(DARK);
        style.visuals.panel_fill = FACE;
        style.visuals.window_fill = FACE;
        style.visuals.widgets.noninteractive.bg_fill = FACE;
        cc.egui_ctx.set_global_style(style);
        let snap = backend.snapshot();
        Self { backend, snap, help: "Ready".into(), about: opts.open.as_deref() == Some("about"), shot: opts.screenshot.clone().map(|p| (p, 0)), open: opts.open.clone(), quitting: false }
    }

    fn cmd(&mut self, c: UiCmd) {
        self.backend.command(c);
    }

    fn phase_text(&self) -> (&'static str, egui::Color32, bool) {
        match self.snap.phase {
            Phase::Running => ("Working", LED_GREEN, true),
            Phase::Bypass => ("Bypass - microphone passes through", LED_YELLOW, true),
            Phase::Calibrating => ("Measuring this PC...", LED_YELLOW, true),
            Phase::Starting => ("Starting...", LED_YELLOW, true),
            Phase::Error => ("Not running", LED_RED, true),
        }
    }

    fn menus(&self) -> Vec<(&'static str, Vec<MenuEntry>)> {
        let mut file = Vec::new();
        if self.snap.has_tray {
            file.push(MenuEntry::item("Hide to tray"));
        }
        file.push(MenuEntry::item("Exit").sep());
        vec![
            ("File", file),
            (
                "Options",
                vec![
                    MenuEntry::check("Extra noise guard (+13 ms)", self.snap.guard),
                    MenuEntry::check("Start with Windows", self.snap.autostart),
                    MenuEntry::item("Refresh microphone list").sep(),
                    MenuEntry::item("Re-measure this PC"),
                ],
            ),
            (
                "Help",
                vec![
                    MenuEntry::item("Performance report..."),
                    MenuEntry::item("Open log..."),
                    MenuEntry::item("Get VB-Cable (virtual microphone)..."),
                    MenuEntry::item("About QuietMic").sep(),
                ],
            ),
        ]
    }

    fn handle_menu(&mut self, (menu, entry): (usize, usize)) {
        let has_tray = self.snap.has_tray;
        match (menu, entry) {
            (0, 0) if has_tray => self.cmd(UiCmd::HideToTray),
            (0, _) => self.cmd(UiCmd::Quit),
            (1, 0) => self.cmd(UiCmd::SetGuard(!self.snap.guard)),
            (1, 1) => self.cmd(UiCmd::SetAutostart(!self.snap.autostart)),
            (1, 2) => self.cmd(UiCmd::RefreshMics),
            (1, _) => self.cmd(UiCmd::Recalibrate),
            (2, 0) => self.cmd(UiCmd::OpenReport),
            (2, 1) => self.cmd(UiCmd::OpenLog),
            (2, 2) => self.cmd(UiCmd::OpenVbCable),
            _ => self.about = true,
        }
    }

    fn body(&mut self, ui: &mut egui::Ui) {
        let mut hover_help: Option<String> = None;
        ui.columns(2, |cols| {
            // ---------------- left column
            let ui = &mut cols[0];
            group_box(ui, "Status", |ui| {
                let (text, color, on) = self.phase_text();
                ui.horizontal(|ui| {
                    led(ui, color, on);
                    label(ui, text);
                });
                paragraph(ui, &self.snap.headline, DARK);
                if !self.snap.detail.is_empty() {
                    paragraph(ui, &self.snap.detail, egui::Color32::from_rgb(64, 64, 64));
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if button(ui, "Bypass", true, self.snap.bypass, 90.0).clicked() {
                        self.cmd(UiCmd::SetBypass(!self.snap.bypass));
                    }
                    let (txt, c) = if self.snap.has_tray { ("Hide to tray", UiCmd::HideToTray) } else { ("Quit", UiCmd::Quit) };
                    if button(ui, txt, true, false, 90.0).clicked() {
                        self.cmd(c);
                    }
                });
            });
            group_box(ui, "Levels", |ui| {
                for (name, lvl) in [("Mic", self.snap.input_level), ("Out", self.snap.output_level)] {
                    let (f, txt) = level_frac(lvl);
                    ui.horizontal(|ui| {
                        label_w(ui, name, 34.0);
                        meter(ui, f, 150.0);
                        label(ui, &txt);
                    });
                }
            });
            group_box(ui, "Performance", |ui| {
                let rows: [(&str, String); 4] = [
                    ("Model", if self.snap.model.is_empty() { "-".into() } else { self.snap.model.clone() }),
                    ("Delay", if self.snap.est_delay_ms > 0.0 { format!("about {:.0} ms", self.snap.est_delay_ms) } else { "-".into() }),
                    ("Dropouts", self.snap.dropouts.to_string()),
                    ("Output", if self.snap.output_name.is_empty() { "-".into() } else { self.snap.output_name.clone() }),
                ];
                for (k, v) in rows {
                    ui.horizontal(|ui| {
                        label_w(ui, k, 62.0);
                        value_field(ui, &v, ui.available_width());
                    });
                }
                ui.horizontal(|ui| {
                    label_w(ui, "CPU", 62.0);
                    meter(ui, (self.snap.cpu_pct / 100.0).clamp(0.0, 1.0), 130.0);
                    label(ui, &format!("{:.0}%", self.snap.cpu_pct));
                });
            });

            // ---------------- right column
            let ui = &mut cols[1];
            group_box(ui, "Preset", |ui| {
                for p in Preset::ALL {
                    let (name, help) = preset_info(p);
                    let r = radio(ui, self.snap.preset == p, name);
                    if r.hovered() {
                        hover_help = Some(help.to_owned());
                    }
                    if r.clicked() {
                        self.cmd(UiCmd::SetPreset(p));
                    }
                }
                ui.add_space(4.0);
                let r = checkbox(ui, self.snap.guard, "Extra noise guard (+13 ms)", true);
                if r.hovered() {
                    hover_help = Some("A second small model mutes the output whenever nobody is speaking.".into());
                }
                if r.clicked() {
                    self.cmd(UiCmd::SetGuard(!self.snap.guard));
                }
            });
            group_box(ui, "Microphone", |ui| {
                let mut names = vec!["Windows default microphone".to_string()];
                names.extend(self.snap.mics.iter().filter(|(id, _)| id.is_some()).map(|(_, n)| n.clone()));
                let idx = self.snap.mics.iter().position(|(id, _)| *id == self.snap.selected_mic).unwrap_or(0);
                let shown = names.get(idx).cloned().unwrap_or_default();
                if let Some(i) = combo(ui, "mic_combo", ui.available_width(), &shown, &names, idx) {
                    if let Some((id, _)) = self.snap.mics.get(i) {
                        self.cmd(UiCmd::SetInput(id.clone()));
                    }
                }
            });
            group_box(ui, "Tools", |ui| {
                let w = ui.available_width();
                for (txt, c) in [
                    ("Re-measure this PC", UiCmd::Recalibrate),
                    ("Performance report...", UiCmd::OpenReport),
                    ("Open log...", UiCmd::OpenLog),
                    ("Get VB-Cable...", UiCmd::OpenVbCable),
                ] {
                    if button(ui, txt, true, false, w).clicked() {
                        self.cmd(c);
                    }
                }
            });
        });
        if let Some(h) = hover_help {
            self.help = h;
        } else {
            self.help = if self.snap.vb_cable_found || self.snap.phase != Phase::Error { "Ready".into() } else { "Install VB-Cable, then pick CABLE Output as the microphone in your calls.".into() };
        }
    }

    fn about_box(&mut self, ctx: &egui::Context, window: Rect) {
        let size = Vec2::new(300.0, 150.0);
        let pos = window.center() - size / 2.0;
        egui::Area::new(egui::Id::new("about")).order(egui::Order::Tooltip).fade_in(false).fixed_pos(pos).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(size, Sense::click());
            raised(ui.painter(), r);
            let tb = Rect::from_min_size(r.min + Vec2::splat(3.0), Vec2::new(size.x - 6.0, 18.0));
            ui.painter().rect_filled(tb, 0.0, NAVY);
            ui.painter().text(tb.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, "About QuietMic", font(), HILITE);
            ui.painter().text(r.center() + Vec2::new(0.0, -14.0), Align2::CENTER_CENTER, "QuietMic - noise-cancelling microphone", font(), DARK);
            ui.painter().text(r.center() + Vec2::new(0.0, 4.0), Align2::CENTER_CENTER, "Early test build. Free and runs on your PC.", font(), DARK);
            let b = Rect::from_center_size(Pos2::new(r.center().x, r.max.y - 24.0), Vec2::new(75.0, 23.0));
            let resp = ui.interact(b, egui::Id::new("about_ok"), Sense::click());
            if resp.is_pointer_button_down_on() { pressed(ui.painter(), b) } else { raised(ui.painter(), b) }
            ui.painter().text(b.center(), Align2::CENTER_CENTER, "OK", font(), DARK);
            if resp.clicked() {
                self.about = false;
            }
        });
    }
}

impl eframe::App for RetroApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.snap = self.backend.snapshot();
        // Alt+F4 / taskbar close: hide to the tray instead of quitting, unless we are really quitting.
        if !self.quitting && self.snap.has_tray && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        while let Some(c) = self.backend.poll_window_cmd() {
            match c {
                WindowCmd::Show => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                WindowCmd::Hide => ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false)),
                WindowCmd::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(which) = self.open.take() {
            let idx = ["file", "options", "help"].iter().position(|m| *m == which);
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("qm_menu_open"), idx));
        }
        let full = ui.max_rect();
        raised(ui.painter(), full);
        let inner = full.shrink(3.0);
        let mut window_cmd = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(Layout::top_down(egui::Align::Min)), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
            match title_bar(ui, "QuietMic - Noise Canceller") {
                TitleAction::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                TitleAction::Close => window_cmd = Some(if self.snap.has_tray { UiCmd::HideToTray } else { UiCmd::Quit }),
                TitleAction::None => {}
            }
            let menus = self.menus();
            if let Some(choice) = menu_bar(ui, &menus) {
                self.handle_menu(choice);
            }
            ui.add_space(2.0);
            ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_max(ui.cursor().min + Vec2::new(6.0, 0.0), inner.max - Vec2::new(6.0, 30.0))), |ui| {
                self.body(ui);
            });
        });

        // status bar
        let bar = Rect::from_min_max(Pos2::new(inner.min.x + 4.0, inner.max.y - 24.0), Pos2::new(inner.max.x - 4.0, inner.max.y - 3.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(bar).layout(Layout::left_to_right(egui::Align::Center)), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
            let rest = bar.width() - 2.0 * 118.0 - 4.0;
            status_cell(ui, rest, &self.help);
            status_cell(ui, 116.0, &format!("Delay ~{:.0} ms", self.snap.est_delay_ms));
            status_cell(ui, 116.0, &format!("CPU {:.0}%", self.snap.cpu_pct));
        });

        if self.about {
            self.about_box(&ctx, full);
        }
        if let Some(c) = window_cmd {
            self.backend.command(c);
        }
        if matches!(self.snap.phase, Phase::Running | Phase::Calibrating | Phase::Starting) || true {
            ctx.request_repaint_after(Duration::from_millis(60));
        }

        if let Some((path, frames)) = &mut self.shot {
            *frames += 1;
            if *frames == 8 {
                let path = path.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                    let _ = save_png(&path, &img);
                })));
            }
            if *frames >= 20 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn save_png(path: &std::path::Path, img: &egui::ColorImage) -> std::io::Result<()> {
    let [w, h] = img.size;
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(std::io::Error::other)?;
    let data: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    writer.write_image_data(&data).map_err(std::io::Error::other)
}

/// Open the main window and run until it is closed.
pub fn run(make: BackendFactory, opts: WindowOptions) -> Result<(), eframe::Error> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("QuietMic")
        .with_inner_size(WINDOW_SIZE)
        .with_resizable(false)
        .with_decorations(false)
        .with_visible(!opts.start_hidden);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("QuietMic", options, Box::new(move |cc| Ok(Box::new(RetroApp::new(cc, make, &opts)))))
}
