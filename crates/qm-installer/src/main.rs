#![cfg_attr(windows, windows_subsystem = "windows")]
//! QuietMic-Setup: a 90s scene-style installer. Rasterbars, a wobbling logo, an NFO box, a scroller and a chiptune.
//! (Joke only: QuietMic is free, open source and needs no crack or keygen.)

mod install;
mod music;
mod retro;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Stroke, Vec2};

use retro::{Look, Shot, H, W};

const SCROLLER: &str = "   *** QUIETMIC 0.1 *** PROUDLY RELEASED INTO THE WILD *** THE AI NOISE CANCELLER THAT DOES NOT COST A DIME \
    *** NO CRACK, NO KEYGEN, NO SERIAL, NO NAG, JUST CLEAN AUDIO *** SENDS YOUR CLEANED MIC TO A VIRTUAL CABLE FOR ZOOM, TEAMS AND SOFTPHONES \
    *** RAIN ON A TIN ROOF? DOGS? HONKING? THE GUY SHOUTING OUTSIDE? NOT ON OUR WATCH *** GREETZ TO THE OPEN SOURCE CREW: DEEPFILTERNET, \
    RNNOISE, FASTENHANCER, DPDFNET, RUST AND EGUI *** MUSIC: ORIGINAL 4-CHANNEL CHIPTUNE, SYNTHESISED IN CODE *** TURN YOUR SPEAKERS UP, \
    CLICK INSTALL, AND TAKE YOUR NEXT CALL IN PEACE ***   ";

#[derive(PartialEq)]
enum Screen {
    Welcome,
    Installing,
    Done,
    Failed(String),
}

struct Setup {
    t0: Instant,
    screen: Screen,
    dir: String,
    desktop_icon: bool,
    autostart: bool,
    open_cable: bool,
    vbcable: bool,
    auto_update: bool,
    launch: bool,
    progress: Arc<Mutex<install::Progress>>,
    music: music::Player,
    look: Look,
    shot: Shot,
    shot_state: Option<String>,
}

impl Setup {
    fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>, shot_state: Option<String>) -> Self {
        let shot_active = shot.is_some();
        let look = Look::happy();
        look.apply_theme(&cc.egui_ctx);
        let mut me = Self {
            t0: Instant::now(),
            screen: Screen::Welcome,
            dir: install::default_dir().display().to_string(),
            desktop_icon: true,
            autostart: false,
            open_cable: false,
            vbcable: !install::cable_installed(),
            auto_update: false,
            launch: true,
            progress: Arc::new(Mutex::new(install::Progress::default())),
            music: if shot_active { music::Player::silent() } else { music::Player::start(music::Mood::Happy) },
            look,
            shot: Shot::new(shot),
            shot_state: shot_state.clone(),
        };
        match shot_state.as_deref() {
            Some("installing") => {
                me.screen = Screen::Installing;
                let mut p = me.progress.lock().unwrap();
                p.fraction = 0.62;
                p.current = "models/dpdfnet/dpdfnet2_48khz.onnx".into();
            }
            Some("done") => me.screen = Screen::Done,
            _ => {}
        }
        me
    }

    fn start_install(&mut self) {
        self.screen = Screen::Installing;
        *self.progress.lock().unwrap() = install::Progress::default();
        let opts = install::Options { dir: PathBuf::from(&self.dir), desktop_icon: self.desktop_icon, autostart: self.autostart, launch: self.launch, vbcable: self.vbcable, auto_update: Some(self.auto_update), staged: false };
        let progress = self.progress.clone();
        std::thread::spawn(move || install::run(opts, progress));
    }
}

impl eframe::App for Setup {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        let t = self.t0.elapsed().as_secs_f32();
        let (green, dim, cyan, amber) = (self.look.fg, self.look.dim, self.look.hi, self.look.accent);
        ctx.request_repaint();
        // finished?
        if self.screen == Screen::Installing {
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
            self.look.logo(ui, t, "v0.1  //  AI NOISE CANCELLER  //  FREE FOREVER  //  NO KEYGEN NEEDED");

            // window buttons
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

            // NFO box
            let nfo = Rect::from_min_size(Pos2::new(24.0, 122.0), Vec2::new(W - 48.0, 128.0));
            ui.painter().rect_filled(nfo, 0.0, Color32::from_black_alpha(215));
            ui.painter().rect_stroke(nfo, 0.0, Stroke::new(1.0, dim), egui::StrokeKind::Inside);
            let size_mb = install::payload_size() as f32 / 1.048_576e6;
            let lines = [
                ("RELEASE.DATE", "2026-10"),
                ("PLATFORM", "WINDOWS 10/11 x64   (macOS .dmg also out)"),
                ("PROTECTION", "NONE. IT IS FREE. YOU WIN."),
                ("ENGINE", "DFN3-LL > FASTENHANCER > DPDFNET, ON YOUR CPU"),
                ("SIZE", &format!("{size_mb:.0} MB PACKED")),
                ("NEEDS", "VB-CABLE (FREE) AS THE VIRTUAL MICROPHONE"),
            ];
            let mut y = nfo.top() + 8.0;
            for (k, v) in lines {
                ui.painter().text(Pos2::new(nfo.left() + 12.0, y), egui::Align2::LEFT_TOP, format!("{k}"), FontId::monospace(16.0), dim);
                ui.painter().text(Pos2::new(nfo.left() + 118.0, y), egui::Align2::LEFT_TOP, ":", FontId::monospace(16.0), dim);
                ui.painter().text(Pos2::new(nfo.left() + 134.0, y), egui::Align2::LEFT_TOP, v, FontId::monospace(16.0), green);
                y += 19.0;
            }

            // controls
            let area = Rect::from_min_size(Pos2::new(24.0, 262.0), Vec2::new(W - 48.0, 150.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| match &self.screen {
                Screen::Welcome => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("INSTALL TO >").color(amber));
                        ui.add(egui::TextEdit::singleline(&mut self.dir).desired_width(W - 190.0).font(FontId::monospace(15.0)));
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.desktop_icon, "DESKTOP ICON");
                        ui.checkbox(&mut self.autostart, "START WITH WINDOWS");
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.launch, "RUN WHEN DONE");
                        ui.checkbox(&mut self.open_cable, "OPEN VB-CABLE PAGE");
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.auto_update, "CHECK GITHUB FOR UPDATES (OFF = NEVER CONNECTS TO THE NET)");
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.vbcable, if install::cable_installed() { "VB-CABLE (ALREADY INSTALLED)" } else { "INSTALL VB-CABLE (FREE MIC DRIVER, ASKS FOR ADMIN)" });
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.add_sized([210.0, 34.0], egui::Button::new(RichText::new("[ INSTALL ]").size(24.0).color(amber))).clicked() {
                            self.start_install();
                        }
                        if ui.add_sized([110.0, 34.0], egui::Button::new(RichText::new("EXIT").size(20.0))).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                }
                Screen::Installing => {
                    let p = self.progress.lock().unwrap().clone();
                    ui.label(RichText::new("DECRUNCHING ...").color(amber).size(20.0));
                    ui.label(RichText::new(p.current.chars().rev().take(58).collect::<Vec<_>>().into_iter().rev().collect::<String>()).color(dim).size(15.0));
                    let n = 40usize;
                    let filled = (p.fraction * n as f32) as usize;
                    let bar: String = (0..n).map(|i| if i < filled { '#' } else { '.' }).collect();
                    ui.label(RichText::new(format!("[{bar}] {:>3}%", (p.fraction * 100.0) as u32)).font(FontId::monospace(18.0)).color(cyan));
                }
                Screen::Done => {
                    ui.label(RichText::new("INSTALL COMPLETE.  LAMER-FREE ZONE.").color(amber).size(20.0));
                    ui.label("In QuietMic pick your real microphone, then choose CABLE Output as the microphone in Zoom / Teams / your softphone.");
                    if self.vbcable {
                        ui.label(RichText::new("If CABLE Output is not listed yet, restart Windows once.").color(dim));
                    }
                    if let Some(w) = self.progress.lock().unwrap().warning.clone() {
                        ui.label(RichText::new(w).color(Color32::from_rgb(0xff, 0x90, 0x50)));
                    }
                    ui.add_space(8.0);
                    if ui.add_sized([210.0, 34.0], egui::Button::new(RichText::new("[ FINISH ]").size(24.0).color(amber))).clicked() {
                        if self.open_cable {
                            install::open_url("https://vb-audio.com/Cable/");
                        }
                        if self.launch {
                            install::launch(&PathBuf::from(&self.dir));
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Screen::Failed(e) => {
                    ui.label(RichText::new("INSTALL FAILED").color(Color32::from_rgb(0xff, 0x50, 0x50)).size(20.0));
                    ui.label(e.as_str());
                    if ui.button("EXIT").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            });
            self.look.scroller(ui, t, SCROLLER);
        });

        self.shot.tick(ctx);
        let _ = &self.shot_state;
    }
}

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let shot = flag("--screenshot").map(PathBuf::from);
    let state = flag("--state");
    // automatic update, started by QuietMic itself: no window; replace the app folder in one swap and relaunch it
    if args.iter().any(|a| a == "--update") {
        let dir = flag("--dir").map(PathBuf::from).unwrap_or_else(install::default_dir);
        std::process::exit(if install::update_in_place(&dir) { 0 } else { 1 });
    }
    // headless install for CI smoke tests: QuietMic-Setup --silent [--dir PATH]
    if args.iter().any(|a| a == "--silent") {
        let dir = flag("--dir").map(PathBuf::from).unwrap_or_else(install::default_dir);
        let progress = Arc::new(Mutex::new(install::Progress::default()));
        install::run(install::Options { dir, desktop_icon: args.iter().any(|a| a == "--desktop"), autostart: false, launch: false, vbcable: args.iter().any(|a| a == "--vbcable"), auto_update: None, staged: false }, progress.clone());
        let p = progress.lock().unwrap();
        if let Some(e) = &p.error {
            eprintln!("install failed: {e}");
            std::process::exit(1);
        }
        println!("installed");
        return Ok(());
    }
    let viewport = egui::ViewportBuilder::default().with_title("QuietMic Setup").with_inner_size([W, H]).with_resizable(false).with_decorations(false);
    eframe::run_native(
        "QuietMic Setup",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(move |cc| Ok(Box::new(Setup::new(cc, shot, state)))),
    )
}
