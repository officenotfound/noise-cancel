#![cfg_attr(windows, windows_subsystem = "windows")]
//! QuietMic-Setup: a 90s scene-style installer. Rasterbars, a wobbling logo, an NFO box, a scroller and a chiptune.
//! (Joke only: QuietMic is free, open source and needs no crack or keygen.)

mod install;
mod music;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui::{self, Color32, FontId, Pos2, Rect, RichText, Stroke, Vec2};

const W: f32 = 600.0;
const H: f32 = 460.0;
const GREEN: Color32 = Color32::from_rgb(0x40, 0xff, 0x60);
const DIM: Color32 = Color32::from_rgb(0x20, 0x90, 0x38);
const CYAN: Color32 = Color32::from_rgb(0x40, 0xe0, 0xff);
const AMBER: Color32 = Color32::from_rgb(0xff, 0xc0, 0x30);

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
    launch: bool,
    progress: Arc<Mutex<install::Progress>>,
    music: music::Player,
    shot: Option<(PathBuf, u32)>,
    shot_state: Option<String>,
}

fn install_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "pixel".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!("../../../assets/fonts/PixelOperator.ttf"))),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().insert(0, "pixel".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let (i, f) = (h.floor() as i32, h.fract());
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

impl Setup {
    fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>, shot_state: Option<String>) -> Self {
        install_font(&cc.egui_ctx);
        let mut v = egui::Visuals::dark();
        v.panel_fill = Color32::BLACK;
        v.override_text_color = Some(GREEN);
        v.widgets.inactive.bg_fill = Color32::from_rgb(0x08, 0x30, 0x10);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x08, 0x30, 0x10);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x10, 0x60, 0x20);
        v.widgets.hovered.bg_fill = Color32::from_rgb(0x10, 0x60, 0x20);
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, DIM);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, GREEN);
        v.extreme_bg_color = Color32::from_rgb(0x00, 0x14, 0x06);
        v.selection.bg_fill = DIM;
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        let mut s = (*cc.egui_ctx.global_style()).clone();
        s.visuals = v;
        {
            s.spacing.item_spacing = Vec2::new(8.0, 6.0);
            s.visuals.window_corner_radius = 0.0.into();
            for w in [&mut s.visuals.widgets.inactive, &mut s.visuals.widgets.hovered, &mut s.visuals.widgets.active, &mut s.visuals.widgets.noninteractive] {
                w.corner_radius = 0.0.into();
            }
        }
        cc.egui_ctx.set_global_style(s);
        let mut me = Self {
            t0: Instant::now(),
            screen: Screen::Welcome,
            dir: install::default_dir().display().to_string(),
            desktop_icon: true,
            autostart: false,
            open_cable: false,
            vbcable: !install::cable_installed(),
            launch: true,
            progress: Arc::new(Mutex::new(install::Progress::default())),
            music: if shot.is_some() { music::Player::silent() } else { music::Player::start() },
            shot: shot.map(|p| (p, 0)),
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
        let opts = install::Options { dir: PathBuf::from(&self.dir), desktop_icon: self.desktop_icon, autostart: self.autostart, launch: self.launch, vbcable: self.vbcable };
        let progress = self.progress.clone();
        std::thread::spawn(move || install::run(opts, progress));
    }

    fn background(&self, ui: &egui::Ui, t: f32) {
        let p = ui.painter();
        let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(W, H));
        p.rect_filled(full, 0.0, Color32::BLACK);
        // starfield: three parallax layers
        for i in 0..90u32 {
            let h = i.wrapping_mul(2654435761).rotate_left(7);
            let layer = 1 + (h % 3);
            let x = ((h >> 3) % 1000) as f32 / 1000.0 * W;
            let y = ((h >> 13) % 1000) as f32 / 1000.0 * H;
            let sx = (x - t * 14.0 * layer as f32).rem_euclid(W);
            let b = 60 + layer * 55;
            p.rect_filled(Rect::from_min_size(Pos2::new(sx, y), Vec2::splat(layer as f32)), 0.0, Color32::from_gray(b as u8));
        }
        // rasterbars behind the logo
        for k in 0..3 {
            let phase = t * 1.6 + k as f32 * 1.05;
            let cy = 52.0 + (phase).sin() * 34.0;
            let col = hsv(0.55 + k as f32 * 0.16 + t * 0.03, 0.9, 1.0);
            for j in -7..=7i32 {
                let a = 1.0 - (j.abs() as f32 / 8.0);
                let c = Color32::from_rgb((col.r() as f32 * a) as u8, (col.g() as f32 * a) as u8, (col.b() as f32 * a) as u8);
                p.rect_filled(Rect::from_min_size(Pos2::new(0.0, cy + j as f32 * 2.0), Vec2::new(W, 2.0)), 0.0, c);
            }
        }
    }

    fn logo(&self, ui: &egui::Ui, t: f32) {
        let p = ui.painter();
        let font = FontId::proportional(64.0);
        let text = "QUIETMIC";
        let widths: Vec<f32> = text.chars().map(|c| p.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE).size().x + 3.0).collect();
        let total: f32 = widths.iter().sum();
        let mut x = (W - total) / 2.0;
        for (i, c) in text.chars().enumerate() {
            let y = 14.0 + (t * 3.0 + i as f32 * 0.6).sin() * 7.0;
            p.text(Pos2::new(x + 3.0, y + 3.0), egui::Align2::LEFT_TOP, c, font.clone(), Color32::from_black_alpha(220));
            p.text(Pos2::new(x, y), egui::Align2::LEFT_TOP, c, font.clone(), hsv(t * 0.25 + i as f32 * 0.07, 0.55, 1.0));
            x += widths[i];
        }
        p.text(Pos2::new(W / 2.0, 92.0), egui::Align2::CENTER_TOP, "v0.1  //  AI NOISE CANCELLER  //  FREE FOREVER  //  NO KEYGEN NEEDED", FontId::proportional(16.0), CYAN);
    }

    fn scroller(&self, ui: &egui::Ui, t: f32) {
        let p = ui.painter();
        let y0 = H - 34.0;
        p.rect_filled(Rect::from_min_size(Pos2::new(0.0, y0 - 6.0), Vec2::new(W, 40.0)), 0.0, Color32::from_rgb(0x00, 0x10, 0x20));
        p.line_segment([Pos2::new(0.0, y0 - 6.0), Pos2::new(W, y0 - 6.0)], Stroke::new(1.0, CYAN));
        let font = FontId::proportional(24.0);
        let space = 7.0;
        let mut x = W - (t * 130.0) % (SCROLLER.len() as f32 * 11.0 + W);
        for (i, c) in SCROLLER.chars().enumerate() {
            let w = if c == ' ' { space } else { p.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE).size().x + 1.0 };
            if x > -20.0 && x < W {
                let y = y0 + (t * 4.0 + x * 0.025).sin() * 5.0;
                p.text(Pos2::new(x, y), egui::Align2::LEFT_TOP, c, font.clone(), hsv(i as f32 * 0.02 + t * 0.2, 0.5, 1.0));
            }
            x += w;
        }
    }
}

impl eframe::App for Setup {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        let t = self.t0.elapsed().as_secs_f32();
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
            self.background(ui, t);
            self.logo(ui, t);

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
            ui.painter().rect_stroke(nfo, 0.0, Stroke::new(1.0, DIM), egui::StrokeKind::Inside);
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
                ui.painter().text(Pos2::new(nfo.left() + 12.0, y), egui::Align2::LEFT_TOP, format!("{k}"), FontId::monospace(16.0), DIM);
                ui.painter().text(Pos2::new(nfo.left() + 118.0, y), egui::Align2::LEFT_TOP, ":", FontId::monospace(16.0), DIM);
                ui.painter().text(Pos2::new(nfo.left() + 134.0, y), egui::Align2::LEFT_TOP, v, FontId::monospace(16.0), GREEN);
                y += 19.0;
            }

            // controls
            let area = Rect::from_min_size(Pos2::new(24.0, 262.0), Vec2::new(W - 48.0, 150.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| match &self.screen {
                Screen::Welcome => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("INSTALL TO >").color(AMBER));
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
                        ui.checkbox(&mut self.vbcable, if install::cable_installed() { "VB-CABLE (ALREADY INSTALLED)" } else { "INSTALL VB-CABLE (FREE MIC DRIVER, ASKS FOR ADMIN)" });
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.add_sized([210.0, 34.0], egui::Button::new(RichText::new("[ INSTALL ]").size(24.0).color(AMBER))).clicked() {
                            self.start_install();
                        }
                        if ui.add_sized([110.0, 34.0], egui::Button::new(RichText::new("EXIT").size(20.0))).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                }
                Screen::Installing => {
                    let p = self.progress.lock().unwrap().clone();
                    ui.label(RichText::new("DECRUNCHING ...").color(AMBER).size(20.0));
                    ui.label(RichText::new(p.current.chars().rev().take(58).collect::<Vec<_>>().into_iter().rev().collect::<String>()).color(DIM).size(15.0));
                    let n = 40usize;
                    let filled = (p.fraction * n as f32) as usize;
                    let bar: String = (0..n).map(|i| if i < filled { '#' } else { '.' }).collect();
                    ui.label(RichText::new(format!("[{bar}] {:>3}%", (p.fraction * 100.0) as u32)).font(FontId::monospace(18.0)).color(CYAN));
                }
                Screen::Done => {
                    ui.label(RichText::new("INSTALL COMPLETE.  LAMER-FREE ZONE.").color(AMBER).size(20.0));
                    ui.label("In QuietMic pick your real microphone, then choose CABLE Output as the microphone in Zoom / Teams / your softphone.");
                    if self.vbcable {
                        ui.label(RichText::new("If CABLE Output is not listed yet, restart Windows once.").color(DIM));
                    }
                    if let Some(w) = self.progress.lock().unwrap().warning.clone() {
                        ui.label(RichText::new(w).color(Color32::from_rgb(0xff, 0x90, 0x50)));
                    }
                    ui.add_space(8.0);
                    if ui.add_sized([210.0, 34.0], egui::Button::new(RichText::new("[ FINISH ]").size(24.0).color(AMBER))).clicked() {
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
            self.scroller(ui, t);
        });

        if let Some((path, frames)) = &mut self.shot {
            *frames += 1;
            if *frames == 10 {
                let path = path.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                    let [w, h] = img.size;
                    if let Ok(file) = std::fs::File::create(&path) {
                        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
                        enc.set_color(png::ColorType::Rgba);
                        enc.set_depth(png::BitDepth::Eight);
                        if let Ok(mut wr) = enc.write_header() {
                            let data: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                            let _ = wr.write_image_data(&data);
                        }
                    }
                })));
            }
            if *frames >= 24 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        let _ = &self.shot_state;
    }
}

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let shot = flag("--screenshot").map(PathBuf::from);
    let state = flag("--state");
    // headless install for CI smoke tests: QuietMic-Setup --silent [--dir PATH]
    if args.iter().any(|a| a == "--silent") {
        let dir = flag("--dir").map(PathBuf::from).unwrap_or_else(install::default_dir);
        let progress = Arc::new(Mutex::new(install::Progress::default()));
        install::run(install::Options { dir, desktop_icon: false, autostart: false, launch: false, vbcable: args.iter().any(|a| a == "--vbcable") }, progress.clone());
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
