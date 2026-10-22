//! Classic Windows 95 controls drawn by hand: buttons, check boxes, radio buttons, group boxes, drop-downs,
//! menu bar, title bar and the blocky progress meter.

use eframe::egui::{self, Align2, Color32, FontId, Id, Order, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

use super::theme::*;

pub const FONT_SIZE: f32 = 16.0;

pub fn font() -> FontId {
    FontId::proportional(FONT_SIZE)
}

pub fn text_size(ui: &Ui, s: &str) -> Vec2 {
    ui.painter().layout_no_wrap(s.to_owned(), font(), DARK).size()
}

pub fn label(ui: &mut Ui, text: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(text_size(ui, text) + Vec2::new(0.0, 4.0), Sense::hover());
    ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, text, font(), DARK);
    resp
}

/// Label in a fixed-width column so following controls line up.
pub fn label_w(ui: &mut Ui, text: &str, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::hover());
    ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, text, font(), DARK);
}

fn embossed(ui: &Ui, pos: Pos2, align: Align2, text: &str) {
    let p = ui.painter();
    p.text(pos + Vec2::new(1.0, 1.0), align, text, font(), HILITE);
    p.text(pos, align, text, font(), SHADOW);
}

/// Push button. `down` draws it latched (toggle buttons).
pub fn button(ui: &mut Ui, text: &str, enabled: bool, down: bool, min_w: f32) -> Response {
    let size = Vec2::new((text_size(ui, text).x + 24.0).max(min_w), 23.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let held = enabled && resp.is_pointer_button_down_on();
    let p = ui.painter();
    if held || down {
        pressed(p, rect);
    } else {
        raised(p, rect);
    }
    let center = rect.center() + if held || down { Vec2::new(1.0, 1.0) } else { Vec2::ZERO };
    if enabled {
        p.text(center, Align2::CENTER_CENTER, text, font(), DARK);
        if resp.has_focus() {
            dotted(p, rect.shrink(4.0));
        }
    } else {
        embossed(ui, center, Align2::CENTER_CENTER, text);
    }
    resp
}

pub fn checkbox(ui: &mut Ui, checked: bool, text: &str, enabled: bool) -> Response {
    let size = Vec2::new(19.0 + text_size(ui, text).x + 2.0, 16.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let b = Rect::from_min_size(rect.min + Vec2::new(0.0, 1.5), Vec2::splat(13.0));
    sunken(p, b, if enabled { FIELD } else { FACE });
    if checked {
        let c = if enabled { DARK } else { SHADOW };
        let pts = [Pos2::new(3.0, 6.0), Pos2::new(5.0, 8.5), Pos2::new(10.0, 3.0)].map(|q| b.min + q.to_vec2());
        p.line_segment([pts[0], pts[1]], Stroke::new(2.0, c));
        p.line_segment([pts[1], pts[2]], Stroke::new(2.0, c));
    }
    let tp = Pos2::new(rect.min.x + 19.0, rect.center().y);
    if enabled {
        p.text(tp, Align2::LEFT_CENTER, text, font(), DARK);
        if resp.has_focus() {
            dotted(p, Rect::from_min_max(Pos2::new(tp.x - 2.0, rect.min.y), rect.max));
        }
    } else {
        embossed(ui, tp, Align2::LEFT_CENTER, text);
    }
    resp
}

pub fn radio(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let size = Vec2::new(19.0 + text_size(ui, text).x + 2.0, 16.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let c = Pos2::new(rect.min.x + 6.5, rect.center().y);
    p.circle_filled(c, 6.0, FIELD);
    p.circle_stroke(c, 6.0, Stroke::new(1.0, SHADOW));
    p.circle_stroke(c, 5.0, Stroke::new(1.0, DARK));
    if selected {
        p.circle_filled(c, 2.5, DARK);
    }
    p.text(Pos2::new(rect.min.x + 19.0, rect.center().y), Align2::LEFT_CENTER, text, font(), DARK);
    resp
}

/// Etched group box with its title sitting on the border.
pub fn group_box<R>(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let w = ui.available_width();
    let tsz = text_size(ui, title);
    let out = egui::Frame::NONE
        .inner_margin(egui::Margin { left: 10, right: 10, top: (tsz.y + 6.0) as i8, bottom: 8 })
        .show(ui, |ui| {
            ui.set_width(w - 20.0);
            add(ui)
        });
    let rect = out.response.rect;
    let p = ui.painter();
    etched(p, Rect::from_min_max(Pos2::new(rect.min.x, rect.min.y + (tsz.y / 2.0).floor()), Pos2::new(rect.max.x - 1.0, rect.max.y - 1.0)));
    let tr = Rect::from_min_size(Pos2::new(rect.min.x + 8.0, rect.min.y), Vec2::new(tsz.x + 6.0, tsz.y));
    p.rect_filled(tr, 0.0, FACE);
    p.text(tr.center(), Align2::CENTER_CENTER, title, font(), DARK);
    ui.add_space(4.0);
    out.inner
}

/// Segmented blue progress meter. `level` is 0.0..=1.0.
pub fn meter(ui: &mut Ui, level: f32, width: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
    let p = ui.painter();
    sunken_thin(p, rect);
    let inner = rect.shrink(3.0);
    let (bw, gap) = (8.0, 2.0);
    let n = ((inner.width() + gap) / (bw + gap)).floor() as usize;
    let lit = (level.clamp(0.0, 1.0) * n as f32).round() as usize;
    for i in 0..lit {
        let x = inner.min.x + i as f32 * (bw + gap);
        p.rect_filled(Rect::from_min_size(Pos2::new(x, inner.min.y), Vec2::new(bw, inner.height())), 0.0, NAVY);
    }
    resp
}

/// Sunken single-line text field showing a value.
pub fn value_field(ui: &mut Ui, text: &str, width: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::hover());
    let p = ui.painter();
    sunken(p, rect, FIELD);
    p.with_clip_rect(rect.shrink(3.0)).text(rect.left_center() + Vec2::new(4.0, 0.0), Align2::LEFT_CENTER, text, font(), DARK);
}

fn triangle_down(p: &egui::Painter, c: Pos2, enabled: bool) {
    let col = if enabled { DARK } else { SHADOW };
    for i in 0..4 {
        let half = 3.5 - i as f32;
        p.rect_filled(Rect::from_center_size(Pos2::new(c.x, c.y - 1.5 + i as f32), Vec2::new(half * 2.0, 1.0)), 0.0, col);
    }
}

/// Drop-down list. Returns the newly chosen index, if any.
pub fn combo(ui: &mut Ui, key: &str, width: f32, shown: &str, options: &[String], selected: usize) -> Option<usize> {
    let id = ui.make_persistent_id(key);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 21.0), Sense::click());
    let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    if resp.clicked() {
        open = !open;
    }
    let p = ui.painter();
    sunken(p, rect, FIELD);
    p.with_clip_rect(Rect::from_min_max(rect.min, Pos2::new(rect.max.x - 18.0, rect.max.y)))
        .text(rect.left_center() + Vec2::new(5.0, 0.0), Align2::LEFT_CENTER, shown, font(), DARK);
    let arrow = Rect::from_min_size(Pos2::new(rect.max.x - 18.0, rect.min.y + 2.0), Vec2::new(16.0, 17.0));
    if open { pressed(p, arrow) } else { raised(p, arrow) }
    triangle_down(p, arrow.center() + if open { Vec2::new(1.0, 1.0) } else { Vec2::ZERO }, true);

    let mut chosen = None;
    if open {
        let row_h = 16.0;
        let list_h = (options.len() as f32 * row_h + 4.0).min(220.0);
        let area = egui::Area::new(id.with("list")).order(Order::Foreground).fade_in(false).fixed_pos(Pos2::new(rect.min.x, rect.max.y - 1.0)).show(ui.ctx(), |ui| {
            let (list, _) = ui.allocate_exact_size(Vec2::new(width, list_h), Sense::hover());
            let p = ui.painter();
            p.rect_filled(list, 0.0, FIELD);
            p.rect_stroke(list, 0.0, Stroke::new(1.0, DARK), egui::StrokeKind::Inside);
            for (i, opt) in options.iter().enumerate() {
                let r = Rect::from_min_size(list.min + Vec2::new(2.0, 2.0 + i as f32 * row_h), Vec2::new(width - 4.0, row_h));
                if r.max.y > list.max.y {
                    break;
                }
                let hot = ui.rect_contains_pointer(r);
                let (bg, fg) = if hot || i == selected && !ui.rect_contains_pointer(list) { (NAVY, HILITE) } else { (FIELD, DARK) };
                p.rect_filled(r, 0.0, bg);
                p.with_clip_rect(r).text(r.left_center() + Vec2::new(3.0, 0.0), Align2::LEFT_CENTER, opt, font(), fg);
                if hot && ui.input(|i| i.pointer.primary_released()) {
                    chosen = Some(i);
                }
            }
        });
        let pointer_down_outside = ui.input(|i| i.pointer.any_pressed())
            && !ui.input(|i| i.pointer.interact_pos()).map_or(false, |pos| area.response.rect.contains(pos) || rect.contains(pos));
        if chosen.is_some() || pointer_down_outside {
            open = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, open));
    chosen
}

pub struct MenuEntry {
    pub label: String,
    pub checked: Option<bool>,
    pub enabled: bool,
    pub separator_above: bool,
}

impl MenuEntry {
    pub fn item(label: &str) -> Self {
        Self { label: label.into(), checked: None, enabled: true, separator_above: false }
    }
    pub fn check(label: &str, on: bool) -> Self {
        Self { label: label.into(), checked: Some(on), enabled: true, separator_above: false }
    }
    pub fn sep(mut self) -> Self {
        self.separator_above = true;
        self
    }
}

/// Classic menu bar. Returns (menu index, entry index) when an entry is chosen.
pub fn menu_bar(ui: &mut Ui, menus: &[(&str, Vec<MenuEntry>)]) -> Option<(usize, usize)> {
    let id = Id::new("qm_menu_open");
    let mut open: Option<usize> = ui.data(|d| d.get_temp(id)).unwrap_or(None);
    let bar_w = ui.available_width();
    let (bar, _) = ui.allocate_exact_size(Vec2::new(bar_w, 20.0), Sense::hover());
    let mut x = bar.min.x + 2.0;
    let mut chosen = None;
    let mut open_rect: Option<Rect> = None;
    for (mi, (name, entries)) in menus.iter().enumerate() {
        let tw = text_size(ui, name).x;
        let r = Rect::from_min_size(Pos2::new(x, bar.min.y + 1.0), Vec2::new(tw + 14.0, 18.0));
        x += r.width();
        let resp = ui.interact(r, id.with(("item", mi)), Sense::click());
        if resp.clicked() {
            open = if open == Some(mi) { None } else { Some(mi) };
        } else if resp.hovered() && open.is_some() {
            open = Some(mi);
        }
        let p = ui.painter();
        if open == Some(mi) {
            pressed(p, r);
        } else if resp.hovered() {
            raised(p, r);
        }
        let off = if open == Some(mi) { Vec2::new(1.0, 1.0) } else { Vec2::ZERO };
        p.text(r.center() + off, Align2::CENTER_CENTER, name, font(), DARK);

        if open == Some(mi) {
            let w = entries.iter().map(|e| text_size(ui, &e.label).x).fold(80.0, f32::max) + 40.0;
            let h: f32 = entries.iter().map(|e| 18.0 + if e.separator_above { 8.0 } else { 0.0 }).sum::<f32>() + 6.0;
            let area = egui::Area::new(id.with(("drop", mi))).order(Order::Foreground).fade_in(false).fixed_pos(Pos2::new(r.min.x, r.max.y)).show(ui.ctx(), |ui| {
                let (list, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
                raised(ui.painter(), list);
                let mut y = list.min.y + 3.0;
                for (ei, e) in entries.iter().enumerate() {
                    if e.separator_above {
                        let p = ui.painter();
                        p.rect_filled(Rect::from_min_size(Pos2::new(list.min.x + 3.0, y + 3.0), Vec2::new(w - 6.0, 1.0)), 0.0, SHADOW);
                        p.rect_filled(Rect::from_min_size(Pos2::new(list.min.x + 3.0, y + 4.0), Vec2::new(w - 6.0, 1.0)), 0.0, HILITE);
                        y += 8.0;
                    }
                    let er = Rect::from_min_size(Pos2::new(list.min.x + 3.0, y), Vec2::new(w - 6.0, 18.0));
                    let hot = e.enabled && ui.rect_contains_pointer(er);
                    let p = ui.painter();
                    if hot {
                        p.rect_filled(er, 0.0, NAVY);
                    }
                    let fg = if hot { HILITE } else { DARK };
                    if e.checked == Some(true) {
                        let c = er.min + Vec2::new(11.0, 9.0);
                        p.line_segment([c + Vec2::new(-3.0, 0.0), c + Vec2::new(-1.0, 3.0)], Stroke::new(2.0, fg));
                        p.line_segment([c + Vec2::new(-1.0, 3.0), c + Vec2::new(4.0, -3.0)], Stroke::new(2.0, fg));
                    }
                    if e.enabled {
                        p.text(er.left_center() + Vec2::new(22.0, 0.0), Align2::LEFT_CENTER, &e.label, font(), fg);
                    } else {
                        embossed(ui, er.left_center() + Vec2::new(22.0, 0.0), Align2::LEFT_CENTER, &e.label);
                    }
                    if hot && ui.input(|i| i.pointer.primary_released()) {
                        chosen = Some((mi, ei));
                    }
                    y += 18.0;
                }
            });
            open_rect = Some(area.response.rect);
        }
    }
    if chosen.is_some() {
        open = None;
    } else if let Some(m) = open {
        let _ = m;
        let outside = ui.input(|i| i.pointer.any_pressed())
            && !ui.input(|i| i.pointer.interact_pos()).map_or(false, |pos| bar.contains(pos) || open_rect.map_or(false, |r| r.contains(pos)));
        if outside {
            open = None;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, open));
    chosen
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TitleAction {
    None,
    Minimize,
    Close,
}

/// Navy title bar with icon, bold white caption, and the minimise / (disabled) maximise / close buttons.
pub fn title_bar(ui: &mut Ui, title: &str) -> TitleAction {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 20.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, NAVY);

    // little speaker-and-waves icon
    let ic = Rect::from_min_size(rect.min + Vec2::new(3.0, 2.0), Vec2::splat(16.0));
    p.rect_filled(ic.shrink(1.0), 0.0, FACE);
    let cy = ic.center().y;
    p.rect_filled(Rect::from_min_size(Pos2::new(ic.min.x + 3.0, cy - 2.0), Vec2::new(3.0, 4.0)), 0.0, DARK);
    p.add(egui::Shape::convex_polygon(
        vec![Pos2::new(ic.min.x + 6.0, cy - 2.0), Pos2::new(ic.min.x + 10.0, cy - 5.0), Pos2::new(ic.min.x + 10.0, cy + 5.0), Pos2::new(ic.min.x + 6.0, cy + 2.0)],
        DARK,
        Stroke::NONE,
    ));
    p.line_segment([Pos2::new(ic.min.x + 12.0, cy - 3.0), Pos2::new(ic.min.x + 12.0, cy + 3.0)], Stroke::new(1.0, TEAL));

    let tp = Pos2::new(rect.min.x + 24.0, rect.center().y);
    p.text(tp, Align2::LEFT_CENTER, title, font(), HILITE);
    p.text(tp + Vec2::new(1.0, 0.0), Align2::LEFT_CENTER, title, font(), HILITE);

    let mut action = TitleAction::None;
    let bw = Vec2::new(16.0, 14.0);
    let y = rect.min.y + 3.0;
    let close = Rect::from_min_size(Pos2::new(rect.max.x - 19.0, y), bw);
    let maxi = Rect::from_min_size(Pos2::new(close.min.x - 18.0, y), bw);
    let mini = Rect::from_min_size(Pos2::new(maxi.min.x - 16.0, y), bw);
    for (r, kind) in [(mini, 0), (maxi, 1), (close, 2)] {
        let resp = ui.interact(r, Id::new(("title_btn", kind)), if kind == 1 { Sense::hover() } else { Sense::click() });
        let held = kind != 1 && resp.is_pointer_button_down_on();
        let p = ui.painter();
        if held { pressed(p, r) } else { raised(p, r) }
        let c = r.center() + if held { Vec2::new(1.0, 1.0) } else { Vec2::ZERO };
        match kind {
            0 => {
                p.rect_filled(Rect::from_min_size(c + Vec2::new(-4.0, 2.0), Vec2::new(6.0, 2.0)), 0.0, DARK);
            }
            1 => {
                p.rect_stroke(Rect::from_center_size(c, Vec2::new(9.0, 8.0)), 0.0, Stroke::new(1.0, SHADOW), egui::StrokeKind::Inside);
            }
            _ => {
                p.line_segment([c + Vec2::new(-3.0, -3.0), c + Vec2::new(3.0, 3.0)], Stroke::new(2.0, DARK));
                p.line_segment([c + Vec2::new(-3.0, 3.0), c + Vec2::new(3.0, -3.0)], Stroke::new(2.0, DARK));
            }
        }
        if resp.clicked() {
            action = if kind == 0 { TitleAction::Minimize } else if kind == 2 { TitleAction::Close } else { action };
        }
    }

    // drag the window by the caption
    let drag_area = Rect::from_min_max(rect.min, Pos2::new(mini.min.x - 2.0, rect.max.y));
    let resp = ui.interact(drag_area, Id::new("title_drag"), Sense::click_and_drag());
    if resp.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    action
}

/// One sunken cell of the status bar.
pub fn status_cell(ui: &mut Ui, width: f32, text: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::hover());
    let p = ui.painter();
    sunken_thin(p, rect);
    p.with_clip_rect(rect.shrink(2.0)).text(rect.left_center() + Vec2::new(4.0, 0.0), Align2::LEFT_CENTER, text, font(), DARK);
}

/// A coloured status lamp.
pub fn led(ui: &mut Ui, color: Color32, on: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    let p = ui.painter();
    p.circle_filled(rect.center(), 6.0, if on { color } else { SHADOW });
    p.circle_stroke(rect.center(), 6.0, Stroke::new(1.0, DARK));
    if on {
        p.circle_filled(rect.center() + Vec2::new(-2.0, -2.0), 1.5, HILITE);
    }
}

/// Wrapped paragraph of text (used for status lines).
pub fn paragraph(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout(text.to_owned(), font(), color, ui.available_width());
    let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
}
