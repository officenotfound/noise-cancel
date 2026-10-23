//! The shared scene look of the installer and the uninstaller: font, theme, animated background, wobbling logo,
//! scroller and screenshot helper. `Look::happy()` is the neon installer, `Look::sad()` the grey-blue rain of the uninstaller.

use std::path::Path;
use std::sync::Arc;

use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, Vec2};

pub const W: f32 = 600.0;
pub const H: f32 = 520.0;

#[derive(Clone, Copy)]
pub struct Look {
    pub sad: bool,
    /// Main text colour, dimmer text, highlight and accent (buttons, headings).
    pub fg: Color32,
    pub dim: Color32,
    pub hi: Color32,
    pub accent: Color32,
    pub panel: Color32,
}

impl Look {
    pub fn happy() -> Self {
        Self {
            sad: false,
            fg: Color32::from_rgb(0x40, 0xff, 0x60),
            dim: Color32::from_rgb(0x20, 0x90, 0x38),
            hi: Color32::from_rgb(0x40, 0xe0, 0xff),
            accent: Color32::from_rgb(0xff, 0xc0, 0x30),
            panel: Color32::from_rgb(0x00, 0x10, 0x20),
        }
    }

    pub fn sad() -> Self {
        Self {
            sad: true,
            fg: Color32::from_rgb(0x9a, 0xb0, 0xd0),
            dim: Color32::from_rgb(0x4c, 0x5a, 0x78),
            hi: Color32::from_rgb(0x7a, 0x8c, 0xc8),
            accent: Color32::from_rgb(0xc8, 0xa0, 0xb8),
            panel: Color32::from_rgb(0x08, 0x0c, 0x18),
        }
    }

    pub fn install_font(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert("pixel".to_owned(), Arc::new(egui::FontData::from_static(include_bytes!("../../../assets/fonts/PixelOperator.ttf"))));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().insert(0, "pixel".to_owned());
        }
        ctx.set_fonts(fonts);
    }

    pub fn apply_theme(&self, ctx: &egui::Context) {
        Self::install_font(ctx);
        let mut v = egui::Visuals::dark();
        v.panel_fill = Color32::BLACK;
        v.override_text_color = Some(self.fg);
        let (idle, hover) = if self.sad {
            (Color32::from_rgb(0x12, 0x1a, 0x2c), Color32::from_rgb(0x20, 0x2c, 0x48))
        } else {
            (Color32::from_rgb(0x08, 0x30, 0x10), Color32::from_rgb(0x10, 0x60, 0x20))
        };
        v.widgets.inactive.bg_fill = idle;
        v.widgets.inactive.weak_bg_fill = idle;
        v.widgets.hovered.weak_bg_fill = hover;
        v.widgets.hovered.bg_fill = hover;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, self.dim);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, self.fg);
        v.extreme_bg_color = if self.sad { Color32::from_rgb(0x04, 0x08, 0x14) } else { Color32::from_rgb(0x00, 0x14, 0x06) };
        v.selection.bg_fill = self.dim;
        ctx.set_theme(egui::Theme::Dark);
        let mut s = (*ctx.global_style()).clone();
        s.visuals = v;
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.visuals.window_corner_radius = 0.0.into();
        for w in [&mut s.visuals.widgets.inactive, &mut s.visuals.widgets.hovered, &mut s.visuals.widgets.active, &mut s.visuals.widgets.noninteractive] {
            w.corner_radius = 0.0.into();
        }
        ctx.set_global_style(s);
    }

    pub fn background(&self, ui: &egui::Ui, t: f32) {
        let p = ui.painter();
        p.rect_filled(Rect::from_min_size(Pos2::ZERO, Vec2::new(W, H)), 0.0, Color32::BLACK);
        if self.sad {
            // rain: slanted streaks falling at three speeds
            for i in 0..110u32 {
                let h = i.wrapping_mul(2654435761).rotate_left(9);
                let layer = 1.0 + (h % 3) as f32;
                let x0 = ((h >> 3) % 1000) as f32 / 1000.0 * (W + 120.0);
                let y0 = ((h >> 13) % 1000) as f32 / 1000.0 * H;
                let y = (y0 + t * 90.0 * layer).rem_euclid(H + 20.0) - 10.0;
                let x = x0 - 0.25 * (y - y0) - 60.0;
                let a = (30.0 + 25.0 * layer) as u8;
                p.line_segment([Pos2::new(x, y), Pos2::new(x - 2.0 * layer, y + 7.0 * layer)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x70, 0x88, 0xb8, a)));
            }
        } else {
            for i in 0..90u32 {
                let h = i.wrapping_mul(2654435761).rotate_left(7);
                let layer = 1 + (h % 3);
                let x = ((h >> 3) % 1000) as f32 / 1000.0 * W;
                let y = ((h >> 13) % 1000) as f32 / 1000.0 * H;
                let sx = (x - t * 14.0 * layer as f32).rem_euclid(W);
                p.rect_filled(Rect::from_min_size(Pos2::new(sx, y), Vec2::splat(layer as f32)), 0.0, Color32::from_gray((60 + layer * 55) as u8));
            }
        }
        // rasterbars behind the logo (slow, muted and sinking for the uninstaller)
        let (speed, sat, val, amp) = if self.sad { (0.5, 0.35, 0.55, 14.0) } else { (1.6, 0.9, 1.0, 34.0) };
        for k in 0..3 {
            let cy = 52.0 + (t * speed + k as f32 * 1.05).sin() * amp + if self.sad { 10.0 } else { 0.0 };
            let col = hsv(if self.sad { 0.60 + k as f32 * 0.05 } else { 0.55 + k as f32 * 0.16 + t * 0.03 }, sat, val);
            for j in -7..=7i32 {
                let a = 1.0 - (j.abs() as f32 / 8.0);
                let c = Color32::from_rgb((col.r() as f32 * a) as u8, (col.g() as f32 * a) as u8, (col.b() as f32 * a) as u8);
                p.rect_filled(Rect::from_min_size(Pos2::new(0.0, cy + j as f32 * 2.0), Vec2::new(W, 2.0)), 0.0, c);
            }
        }
    }

    pub fn logo(&self, ui: &egui::Ui, t: f32, subtitle: &str) {
        let p = ui.painter();
        let font = FontId::proportional(64.0);
        let text = "QUIETMIC";
        let widths: Vec<f32> = text.chars().map(|c| p.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE).size().x + 3.0).collect();
        let mut x = (W - widths.iter().sum::<f32>()) / 2.0;
        for (i, c) in text.chars().enumerate() {
            let (y, col) = if self.sad {
                // slow swell, letters drooping further to the right, washed-out blue
                (20.0 + i as f32 * 1.6 + (t * 1.1 + i as f32 * 0.5).sin() * 3.0, hsv(0.6 + i as f32 * 0.01, 0.28, 0.82))
            } else {
                (14.0 + (t * 3.0 + i as f32 * 0.6).sin() * 7.0, hsv(t * 0.25 + i as f32 * 0.07, 0.55, 1.0))
            };
            p.text(Pos2::new(x + 3.0, y + 3.0), egui::Align2::LEFT_TOP, c, font.clone(), Color32::from_black_alpha(220));
            p.text(Pos2::new(x, y), egui::Align2::LEFT_TOP, c, font.clone(), col);
            x += widths[i];
        }
        p.text(Pos2::new(W / 2.0, 92.0), egui::Align2::CENTER_TOP, subtitle, FontId::proportional(16.0), self.hi);
    }

    pub fn scroller(&self, ui: &egui::Ui, t: f32, text: &str) {
        let p = ui.painter();
        let y0 = H - 34.0;
        p.rect_filled(Rect::from_min_size(Pos2::new(0.0, y0 - 6.0), Vec2::new(W, 40.0)), 0.0, self.panel);
        p.line_segment([Pos2::new(0.0, y0 - 6.0), Pos2::new(W, y0 - 6.0)], Stroke::new(1.0, self.hi));
        let font = FontId::proportional(24.0);
        let speed = if self.sad { 80.0 } else { 130.0 };
        let wave = if self.sad { 2.0 } else { 5.0 };
        let mut x = W - (t * speed) % (text.len() as f32 * 11.0 + W);
        for (i, c) in text.chars().enumerate() {
            let w = if c == ' ' { 7.0 } else { p.layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE).size().x + 1.0 };
            if x > -20.0 && x < W {
                let y = y0 + (t * if self.sad { 2.0 } else { 4.0 } + x * 0.025).sin() * wave;
                let col = if self.sad { hsv(0.6 + (i as f32 * 0.01).sin() * 0.05, 0.25, 0.7) } else { hsv(i as f32 * 0.02 + t * 0.2, 0.5, 1.0) };
                p.text(Pos2::new(x, y), egui::Align2::LEFT_TOP, c, font.clone(), col);
            }
            x += w;
        }
    }
}

pub fn hsv(h: f32, s: f32, v: f32) -> Color32 {
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

/// Frame counter for `--screenshot out.png`: grabs the window on frame 10 and closes on frame 24.
pub struct Shot {
    path: Option<std::path::PathBuf>,
    frames: u32,
}

impl Shot {
    pub fn new(path: Option<std::path::PathBuf>) -> Self {
        Self { path, frames: 0 }
    }

    pub fn active(&self) -> bool {
        self.path.is_some()
    }

    pub fn tick(&mut self, ctx: &egui::Context) {
        let Some(path) = self.path.clone() else { return };
        self.frames += 1;
        if self.frames == 10 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| save_png(&path, &img))));
        }
        if self.frames >= 24 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn save_png(path: &Path, img: &egui::ColorImage) {
    let [w, h] = img.size;
    if let Ok(file) = std::fs::File::create(path) {
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        if let Ok(mut wr) = enc.write_header() {
            let data: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let _ = wr.write_image_data(&data);
        }
    }
}
