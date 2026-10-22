//! Windows 95 look: the classic 16-colour-era palette and 1-pixel bevel drawing.

use eframe::egui::{Color32, Painter, Pos2, Rect, Vec2};

pub const FACE: Color32 = Color32::from_rgb(192, 192, 192);
pub const HILITE: Color32 = Color32::from_rgb(255, 255, 255);
pub const LIGHT: Color32 = Color32::from_rgb(223, 223, 223);
pub const SHADOW: Color32 = Color32::from_rgb(128, 128, 128);
pub const DARK: Color32 = Color32::from_rgb(0, 0, 0);
pub const NAVY: Color32 = Color32::from_rgb(0, 0, 128);
pub const TEAL: Color32 = Color32::from_rgb(0, 128, 128);
pub const FIELD: Color32 = Color32::from_rgb(255, 255, 255);
pub const LED_GREEN: Color32 = Color32::from_rgb(0, 255, 0);
pub const LED_RED: Color32 = Color32::from_rgb(255, 0, 0);
pub const LED_YELLOW: Color32 = Color32::from_rgb(255, 255, 0);

fn strip(p: &Painter, min: Pos2, size: Vec2, c: Color32) {
    p.rect_filled(Rect::from_min_size(min, size), 0.0, c);
}

/// One 1px frame: `tl` on the top and left edges, `br` on the bottom and right edges, inset by `inset` px.
fn frame(p: &Painter, r: Rect, inset: f32, tl: Color32, br: Color32) {
    let r = r.shrink(inset);
    let (w, h) = (r.width(), r.height());
    strip(p, r.min, Vec2::new(w, 1.0), tl);
    strip(p, r.min, Vec2::new(1.0, h), tl);
    strip(p, Pos2::new(r.min.x, r.max.y - 1.0), Vec2::new(w, 1.0), br);
    strip(p, Pos2::new(r.max.x - 1.0, r.min.y), Vec2::new(1.0, h), br);
}

/// Raised button / window face.
pub fn raised(p: &Painter, r: Rect) {
    p.rect_filled(r, 0.0, FACE);
    frame(p, r, 0.0, HILITE, DARK);
    frame(p, r, 1.0, LIGHT, SHADOW);
}

/// Button while it is held down.
pub fn pressed(p: &Painter, r: Rect) {
    p.rect_filled(r, 0.0, FACE);
    frame(p, r, 0.0, DARK, DARK);
    frame(p, r, 1.0, SHADOW, SHADOW);
}

/// Sunken text field / list / status panel interior.
pub fn sunken(p: &Painter, r: Rect, fill: Color32) {
    p.rect_filled(r, 0.0, fill);
    frame(p, r, 0.0, SHADOW, HILITE);
    frame(p, r, 1.0, DARK, LIGHT);
}

/// Thin sunken panel (status bar cells).
pub fn sunken_thin(p: &Painter, r: Rect) {
    p.rect_filled(r, 0.0, FACE);
    frame(p, r, 0.0, SHADOW, HILITE);
}

/// Etched group-box outline.
pub fn etched(p: &Painter, r: Rect) {
    frame(p, r, 0.0, SHADOW, HILITE);
    frame(p, r.translate(Vec2::new(1.0, 1.0)), 0.0, HILITE, SHADOW);
}

/// Dotted focus rectangle.
pub fn dotted(p: &Painter, r: Rect) {
    let mut x = r.min.x;
    while x < r.max.x {
        strip(p, Pos2::new(x, r.min.y), Vec2::new(1.0, 1.0), DARK);
        strip(p, Pos2::new(x, r.max.y - 1.0), Vec2::new(1.0, 1.0), DARK);
        x += 2.0;
    }
    let mut y = r.min.y;
    while y < r.max.y {
        strip(p, Pos2::new(r.min.x, y), Vec2::new(1.0, 1.0), DARK);
        strip(p, Pos2::new(r.max.x - 1.0, y), Vec2::new(1.0, 1.0), DARK);
        y += 2.0;
    }
}
