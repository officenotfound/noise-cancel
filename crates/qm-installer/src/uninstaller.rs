#![cfg_attr(windows, windows_subsystem = "windows")]
//! QuietMic-Uninstall: the farewell screen. Same scene look as the installer, in rain and blue, with a sad chiptune.

mod music;
mod retro;
mod uninstall;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Vec2};
use retro::{Look, Shot, H, W};

const SCROLLER: &str = "   *** QUIETMIC IS PACKING ITS BAGS *** THE NOISE WILL COME BACK, THE DOGS WILL BARK, THE RAIN WILL HAMMER THE TIN ROOF \
    *** THANKS FOR THE CALLS WE KEPT QUIET *** YOUR SETTINGS CAN STAY, IN CASE YOU CHANGE YOUR MIND *** NO HARD FEELINGS, WE ARE JUST SOFTWARE *** \
    SO LONG, AND THANKS FOR ALL THE SILENCE ***   ";

#[derive(PartialEq)]
enum Screen {
    Ask,
    Working,
    Done,
    Failed(String),
}

struct Farewell {
    t0: Instant,
    look: Look,
    screen: Screen,
    dir: PathBuf,
    remove_settings: bool,
    progress: Arc<Mutex<uninstall::Progress>>,
    music: music::Player,
    shot: Shot,
    quit_after: Option<Instant>,
}

impl Farewell {
    fn new(cc: &eframe::CreationContext<'_>, dir: PathBuf, shot: Option<PathBuf>, state: Option<String>) -> Self {
        let look = Look::sad();
        look.apply_theme(&cc.egui_ctx);
        let music = if shot.is_some() { music::Player::silent() } else { music::Player::start(music::Mood::Sad) };
        let mut me = Self {
            t0: Instant::now(),
            look,
            screen: Screen::Ask,
            dir,
            remove_settings: false,
            progress: Arc::new(Mutex::new(uninstall::Progress::default())),
            music,
            shot: Shot::new(shot),
            quit_after: None,
        };
        match state.as_deref() {
            Some("working") => {
                me.screen = Screen::Working;
                let mut p = me.progress.lock().unwrap();
                p.fraction = 0.55;
                p.current = "models/dpdfnet/dpdfnet2_48khz.onnx".into();
            }
            Some("done") => me.screen = Screen::Done,
            _ => {}
        }
        me
    }
}

impl eframe::App for Farewell {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        let t = self.t0.elapsed().as_secs_f32();
        let (fg, dim, hi, accent) = (self.look.fg, self.look.dim, self.look.hi, self.look.accent);
        ctx.request_repaint();
        if self.screen == Screen::Working {
            let p = self.progress.lock().unwrap().clone();
            if let Some(e) = p.error {
                self.screen = Screen::Failed(e);
            } else if p.done {
                self.screen = Screen::Done;
            }
        }
        root.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::ZERO, Vec2::new(W, H))), |ui| {
            let bg = ui.interact(Rect::from_min_size(Pos2::ZERO, Vec2::new(W, 120.0)), egui::Id::new("qm_drag"), egui::Sense::drag());
            if bg.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            self.look.background(ui, t);
            self.look.logo(ui, t, "v0.1  //  UNINSTALLER  //  GOODBYE FOR NOW");

            let mut close = false;
            ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(W - 118.0, 4.0), Vec2::new(114.0, 22.0))), |ui| {
                ui.horizontal(|ui| {
                    let muted = self.music.muted.load(Ordering::Relaxed);
                    if ui.small_button(if muted { "MUSIC: OFF" } else { "MUSIC: ON" }).clicked() {
                        self.music.muted.store(!muted, Ordering::Relaxed);
                    }
                    if ui.small_button("X").clicked() {
                        close = true;
                    }
                });
            });
            if close {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            // the "NFO" box: what goes and what stays
            let nfo = Rect::from_min_size(Pos2::new(24.0, 122.0), Vec2::new(W - 48.0, 128.0));
            ui.painter().rect_filled(nfo, 0.0, Color32::from_black_alpha(215));
            ui.painter().rect_stroke(nfo, 0.0, egui::Stroke::new(1.0, dim), egui::StrokeKind::Inside);
            let dir_text = self.dir.display().to_string();
            let lines = [
                ("REMOVES", "THE APP, MODELS, TOOLS AND SHORTCUTS"),
                ("ALSO", "THE AUTOSTART AND INSTALLED-APPS ENTRIES"),
                ("FROM", dir_text.as_str()),
                ("KEEPS", "VB-CABLE (OTHER APPS MAY USE IT)"),
                ("KEEPS", "YOUR SETTINGS, UNLESS YOU TICK THE BOX"),
                ("MOOD", "A LITTLE BLUE"),
            ];
            let mut y = nfo.top() + 8.0;
            for (k, v) in lines {
                ui.painter().text(Pos2::new(nfo.left() + 12.0, y), egui::Align2::LEFT_TOP, k, FontId::monospace(16.0), dim);
                ui.painter().text(Pos2::new(nfo.left() + 118.0, y), egui::Align2::LEFT_TOP, ":", FontId::monospace(16.0), dim);
                let shown: String = if v.len() > 52 { format!("...{}", &v[v.len() - 49..]) } else { v.to_string() };
                ui.painter().text(Pos2::new(nfo.left() + 134.0, y), egui::Align2::LEFT_TOP, shown, FontId::monospace(16.0), fg);
                y += 19.0;
            }

            let area = Rect::from_min_size(Pos2::new(24.0, 262.0), Vec2::new(W - 48.0, 150.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| match &self.screen {
                Screen::Ask => {
                    ui.label(RichText::new("REALLY LEAVE?").color(accent).size(20.0));
                    ui.checkbox(&mut self.remove_settings, "ALSO DELETE MY SETTINGS AND LAST RECORDING");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.add_sized([210.0, 34.0], egui::Button::new(RichText::new("[ UNINSTALL ]").size(24.0).color(accent))).clicked() {
                            self.screen = Screen::Working;
                            *self.progress.lock().unwrap() = uninstall::Progress::default();
                            let opts = uninstall::Options { dir: self.dir.clone(), remove_settings: self.remove_settings };
                            let progress = self.progress.clone();
                            std::thread::spawn(move || uninstall::run(opts, progress));
                        }
                        if ui.add_sized([150.0, 34.0], egui::Button::new(RichText::new("KEEP IT").size(20.0))).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                }
                Screen::Working => {
                    let p = self.progress.lock().unwrap().clone();
                    ui.label(RichText::new("SAYING GOODBYE ...").color(accent).size(20.0));
                    ui.label(RichText::new(p.current.chars().rev().take(58).collect::<Vec<_>>().into_iter().rev().collect::<String>()).color(dim).size(15.0));
                    let n = 40usize;
                    let filled = (p.fraction * n as f32) as usize;
                    let bar: String = (0..n).map(|i| if i < filled { '#' } else { '.' }).collect();
                    ui.label(RichText::new(format!("[{bar}] {:>3}%", (p.fraction * 100.0) as u32)).font(FontId::monospace(18.0)).color(hi));
                }
                Screen::Done => {
                    ui.label(RichText::new("QUIETMIC HAS LEFT THE BUILDING.").color(accent).size(20.0));
                    ui.label("The silence is yours now. Restart QuietMic any time from a fresh install.");
                    let leftovers = self.progress.lock().unwrap().leftovers.clone();
                    if !leftovers.is_empty() {
                        ui.label(RichText::new(format!("Could not remove {} file(s); delete the folder by hand once QuietMic is closed.", leftovers.len())).color(Color32::from_rgb(0xe0, 0x90, 0x70)));
                    }
                    ui.add_space(6.0);
                    if ui.add_sized([210.0, 34.0], egui::Button::new(RichText::new("[ CLOSE ]").size(24.0).color(accent))).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Screen::Failed(e) => {
                    ui.label(RichText::new("COULD NOT UNINSTALL").color(Color32::from_rgb(0xe0, 0x70, 0x70)).size(20.0));
                    ui.label(e.as_str());
                    if ui.button("CLOSE").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            });
            self.look.scroller(ui, t, SCROLLER);
        });
        self.shot.tick(ctx);
        let _ = self.quit_after;
    }
}

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let dir_arg = flag("--dir");
    let dir = uninstall::install_dir(dir_arg.clone());
    // headless removal for scripts and tests: QuietMic-Uninstall --silent --dir PATH [--settings]
    if args.iter().any(|a| a == "--silent") {
        // from inside the install folder: do the work from a temp copy (it removes itself afterwards)
        if dir_arg.is_none() {
            let mut extra = vec!["--silent"];
            if args.iter().any(|a| a == "--settings") {
                extra.push("--settings");
            }
            if uninstall::relaunch_from_temp(&dir, &extra, false) {
                return Ok(()); // the temp copy finishes the job and removes itself
            }
        }
        let progress = Arc::new(Mutex::new(uninstall::Progress::default()));
        uninstall::run(uninstall::Options { dir, remove_settings: args.iter().any(|a| a == "--settings") }, progress.clone());
        let p = progress.lock().unwrap();
        if let Some(e) = &p.error {
            eprintln!("uninstall failed: {e}");
            std::process::exit(1);
        }
        println!("removed");
        uninstall::delete_self_later();
        return Ok(());
    }
    // run from the install folder: continue from a temp copy so the folder (and this exe) can be deleted
    if dir_arg.is_none() && flag("--screenshot").is_none() && uninstall::relaunch_from_temp(&dir, &[], false) {
        return Ok(());
    }
    let shot = flag("--screenshot").map(PathBuf::from);
    let state = flag("--state");
    let viewport = egui::ViewportBuilder::default().with_title("QuietMic Uninstall").with_inner_size([W, H]).with_resizable(false).with_decorations(false);
    let result = eframe::run_native(
        "QuietMic Uninstall",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(move |cc| Ok(Box::new(Farewell::new(cc, dir, shot, state)))),
    );
    uninstall::delete_self_later();
    result
}
