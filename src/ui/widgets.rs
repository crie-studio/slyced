//! Small custom-painted widgets that give the UI its look.

use super::icons;
use super::theme::*;
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

/// Square icon button used in toolbars.
pub fn icon_button(ui: &mut Ui, icon: &str, tooltip: &str, selected: bool) -> Response {
    icon_button_sized(ui, icon, tooltip, selected, 34.0)
}

pub fn icon_button_sized(ui: &mut Ui, icon: &str, tooltip: &str, selected: bool, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let hovered = resp.hovered();
        let (bg, fg) = if selected {
            (pal().selected_bg, pal().accent)
        } else if hovered {
            (pal().bg_hover, pal().text)
        } else {
            (Color32::TRANSPARENT, pal().text_dim)
        };
        p.rect_filled(rect, CornerRadius::same(8), bg);
        if selected {
            p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, pal().selected_border), StrokeKind::Inside);
        }
        p.text(rect.center(), Align2::CENTER_CENTER, icon, font(size * 0.53), fg);
    }
    if tooltip.is_empty() { resp } else { resp.on_hover_text(tooltip) }
}

/// Pill-shaped segmented control. Returns true when the value changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str, &str)]) -> bool {
    let h = 30.0;
    let pad = 3.0;
    let widths: Vec<f32> = options
        .iter()
        .map(|(_, icon, label)| {
            let text = seg_text(icon, label);
            let w = ui.painter().layout_no_wrap(text, font(13.0), pal().text).size().x;
            w + 22.0
        })
        .collect();
    let total = widths.iter().sum::<f32>() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(total, h), Sense::hover());
    let p = ui.painter().clone();
    p.rect_filled(rect, CornerRadius::same(9), pal().bg_input);
    p.rect_stroke(rect, CornerRadius::same(9), Stroke::new(1.0, pal().border), StrokeKind::Inside);
    let mut x = rect.left() + pad;
    let mut changed = false;
    for ((v, icon, label), w) in options.iter().zip(widths) {
        let r = Rect::from_min_size(Pos2::new(x, rect.top() + pad), Vec2::new(w, h - pad * 2.0));
        let id = ui.scope_id().with(("seg", label, x as i32));
        let resp = ui.interact(r, id, Sense::click());
        let sel = *v == *value;
        if sel {
            p.rect_filled(r, CornerRadius::same(7), pal().bg_active);
            p.rect_stroke(r, CornerRadius::same(7), Stroke::new(1.0, pal().border_strong), StrokeKind::Inside);
        } else if resp.hovered() {
            p.rect_filled(r, CornerRadius::same(7), pal().bg_raised);
        }
        let color = if sel { pal().strong } else if resp.hovered() { pal().text } else { pal().text_dim };
        p.text(r.center(), Align2::CENTER_CENTER, seg_text(icon, label), font(13.0), color);
        if resp.clicked() && !sel {
            *value = *v;
            changed = true;
        }
        x += w;
    }
    changed
}

fn seg_text(icon: &str, label: &str) -> String {
    match (icon.is_empty(), label.is_empty()) {
        (true, _) => label.to_string(),
        (_, true) => icon.to_string(),
        _ => format!("{icon}  {label}"),
    }
}

/// Accent-filled call-to-action button.
pub fn primary_button(ui: &mut Ui, icon: &str, text: &str) -> Response {
    let label = if icon.is_empty() { text.to_string() } else { format!("{icon}  {text}") };
    let galley = ui.painter().layout_no_wrap(label, font_bold(13.5), Color32::WHITE);
    let size = Vec2::new(galley.size().x + 28.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let (a, b) = if resp.hovered() { (lighten(pal().accent, 0.12), lighten(pal().accent_2, 0.12)) } else { (pal().accent, pal().accent_2) };
        gradient_rect(p, rect, 9, a, b);
        p.galley(rect.center() - galley.size() * 0.5, galley, Color32::WHITE);
    }
    resp
}

/// Low-emphasis button with border.
pub fn ghost_button(ui: &mut Ui, icon: &str, text: &str) -> Response {
    let label = if icon.is_empty() { text.to_string() } else { format!("{icon}  {text}") };
    let galley = ui.painter().layout_no_wrap(label, font(13.5), pal().text);
    let size = Vec2::new(galley.size().x + 26.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let bg = if resp.hovered() { pal().bg_hover } else { pal().bg_raised };
        p.rect_filled(rect, CornerRadius::same(9), bg);
        p.rect_stroke(rect, CornerRadius::same(9), Stroke::new(1.0, pal().border_strong), StrokeKind::Inside);
        p.galley(rect.center() - galley.size() * 0.5, galley, pal().text);
    }
    resp
}

/// Ghost button with a separate arrow part that opens a menu: returns the
/// responses of the main part and of the arrow.
pub fn split_button(ui: &mut Ui, icon: &str, text: &str) -> (Response, Response) {
    tool_split(ui, ("split", text), icon, Some(text), false)
}

/// Split button of a toolbar group: icon (and name when there is room), plus
/// an arrow that opens the group's menu. `selected` when its tool is active.
pub fn tool_split(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug + Copy, icon: &str, text: Option<&str>, selected: bool) -> (Response, Response) {
    let label = match text {
        Some(t) => format!("{icon}  {t}"),
        None => icon.to_string(),
    };
    let size = if text.is_some() { 13.5 } else { 18.0 };
    let galley = ui.painter().layout_no_wrap(label, font(size), pal().text);
    let main_w = if text.is_some() { galley.size().x + 22.0 } else { 34.0 };
    let arrow_w = if text.is_some() { 26.0 } else { 18.0 };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(main_w + arrow_w, 34.0), Sense::hover());
    let main_rect = Rect::from_min_size(rect.min, Vec2::new(main_w, rect.height()));
    let arrow_rect = Rect::from_min_max(Pos2::new(main_rect.right(), rect.top()), rect.max);
    let main = ui.interact(main_rect, egui::Id::unique(("tool-split", id)), Sense::click());
    let arrow = ui.interact(arrow_rect, egui::Id::unique(("tool-split-arrow", id)), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let (bg, border, fg) = if selected {
            (pal().selected_bg, pal().selected_border, pal().accent)
        } else if text.is_some() {
            (pal().bg_raised, pal().border_strong, pal().text)
        } else {
            (Color32::TRANSPARENT, Color32::TRANSPARENT, pal().text_dim)
        };
        p.rect_filled(rect, CornerRadius::same(9), bg);
        let left = CornerRadius { nw: 9, sw: 9, ne: 0, se: 0 };
        let right = CornerRadius { nw: 0, sw: 0, ne: 9, se: 9 };
        for (r, resp, corners) in [(main_rect, &main, left), (arrow_rect, &arrow, right)] {
            if resp.hovered() {
                p.rect_filled(r, corners, if selected { with_alpha(pal().accent, 40) } else { pal().bg_hover });
            }
        }
        if border != Color32::TRANSPARENT {
            p.rect_stroke(rect, CornerRadius::same(9), Stroke::new(1.0, border), StrokeKind::Inside);
            p.vline(main_rect.right(), rect.y_range().shrink(8.0), Stroke::new(1.0, border));
        }
        p.galley(main_rect.center() - galley.size() * 0.5, galley, fg);
        p.text(arrow_rect.center(), Align2::CENTER_CENTER, icons::CARET_DOWN, font(11.0), if selected { pal().accent } else { pal().text_dim });
    }
    (main, arrow)
}

/// On/off switch (settings).
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(40.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let (a, b) = (pal().widget_bg, pal().accent);
        let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
        p.rect_filled(rect, CornerRadius::same(11), Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b())));
        p.circle_filled(Pos2::new(rect.left() + 11.0 + (rect.width() - 22.0) * t, rect.center().y), 8.0, Color32::WHITE);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Small counter at the top right corner of a button.
pub fn count_badge(ui: &Ui, button: Rect, n: usize) {
    if n == 0 {
        return;
    }
    let p = ui.painter();
    let galley = p.layout_no_wrap(if n > 9 { "9+".to_string() } else { n.to_string() }, font_bold(10.0), Color32::WHITE);
    let r = Rect::from_center_size(button.right_top() + Vec2::new(-3.0, 5.0), Vec2::new((galley.size().x + 8.0).max(16.0), 16.0));
    p.rect_filled(r, CornerRadius::same(8), pal().accent);
    p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.5, pal().bg_panel), StrokeKind::Outside);
    p.galley(r.center() - galley.size() * 0.5, galley, Color32::WHITE);
}

/// Horizontal gradient fill with rounded corners (approximated by a mesh clipped to a rounded rect).
/// Rounded rectangle filled with a left-to-right gradient. One shape with the
/// same rounding on all four corners and a one-pixel soft edge, as egui's own
/// rounded rectangles.
pub fn gradient_rect(p: &egui::Painter, rect: Rect, radius: u8, left: Color32, right: Color32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let r = (radius as f32).min(rect.width() * 0.5).min(rect.height() * 0.5).max(0.0);
    let color_at = |x: f32| {
        let t = ((x - rect.left()) / rect.width().max(1e-3)).clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color32::from_rgba_premultiplied(mix(left.r(), right.r()), mix(left.g(), right.g()), mix(left.b(), right.b()), mix(left.a(), right.a()))
    };
    // Outline, clockwise from the top-left corner, with the outward normal of each point.
    let steps = ((r * 0.6).ceil() as usize).clamp(1, 16);
    let corners = [
        (Pos2::new(rect.left() + r, rect.top() + r), PI),
        (Pos2::new(rect.right() - r, rect.top() + r), PI + FRAC_PI_2),
        (Pos2::new(rect.right() - r, rect.bottom() - r), 0.0),
        (Pos2::new(rect.left() + r, rect.bottom() - r), FRAC_PI_2),
    ];
    let mut outline: Vec<(Pos2, Vec2)> = Vec::with_capacity(4 * (steps + 1));
    for (c, start) in corners {
        for i in 0..=steps {
            let a = start + FRAC_PI_2 * i as f32 / steps as f32;
            let n = Vec2::new(a.cos(), a.sin());
            outline.push((c + n * r, n));
        }
    }
    let half = 0.5 / p.pixels_per_point();
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.center(), color_at(rect.center().x));
    for (q, n) in &outline {
        let inner = *q - *n * half;
        mesh.colored_vertex(inner, color_at(inner.x));
        mesh.colored_vertex(*q + *n * half, Color32::TRANSPARENT);
    }
    let count = outline.len() as u32;
    for i in 0..count {
        let j = (i + 1) % count;
        let (a_in, a_out, b_in, b_out) = (1 + 2 * i, 2 + 2 * i, 1 + 2 * j, 2 + 2 * j);
        mesh.add_triangle(0, a_in, b_in);
        mesh.add_triangle(a_in, a_out, b_out);
        mesh.add_triangle(a_in, b_out, b_in);
    }
    p.add(egui::Shape::mesh(mesh));
}

pub fn lighten(c: Color32, t: f32) -> Color32 {
    let f = |v: u8| (v as f32 + (255.0 - v as f32) * t).round() as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Small rounded tag such as a modality ("CT", "MR").
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), font_bold(10.5), color);
    let size = galley.size() + Vec2::new(10.0, 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(5), with_alpha(color, 30));
    p.rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0, with_alpha(color, 70)), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() * 0.5, galley, color);
    resp
}

pub fn modality_color(modality: &str) -> Color32 {
    match modality {
        "CT" => Color32::from_rgb(92, 160, 255),
        "MR" => Color32::from_rgb(170, 128, 255),
        "PT" | "NM" => Color32::from_rgb(255, 140, 90),
        "US" => Color32::from_rgb(64, 214, 190),
        "CR" | "DX" | "MG" | "XA" | "RF" => Color32::from_rgb(240, 196, 92),
        _ => Color32::from_rgb(150, 160, 177),
    }
}

/// Uppercase section caption used in side panels.
pub fn caption(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text.to_uppercase()).font(font_bold(10.5)).color(pal().text_faint).extra_letter_spacing(0.8));
    ui.add_space(2.0);
}

/// Panel card with a subtle border.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(pal().bg_raised)
        .stroke(Stroke::new(1.0, pal().border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn kv(ui: &mut Ui, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(key).color(pal().text_faint).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value).color(pal().text).size(12.0));
        });
    });
}

/// Animated arc spinner.
pub fn spinner(p: &egui::Painter, center: Pos2, radius: f32, time: f64, color: Color32) {
    let start = (time * 4.0) as f32;
    let n = 24;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = start + i as f32 / n as f32 * std::f32::consts::PI * 1.4;
            center + Vec2::angled(a) * radius
        })
        .collect();
    p.circle_stroke(center, radius, Stroke::new(2.5, with_alpha(color, 35)));
    p.add(egui::Shape::line(pts, Stroke::new(2.5, color)));
}

/// Thin progress bar with rounded ends.
pub fn progress_bar(p: &egui::Painter, rect: Rect, fraction: f32) {
    p.rect_filled(rect, CornerRadius::same(3), pal().bg_hover);
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    if fill.width() > 1.0 {
        gradient_rect(p, fill, 3, pal().accent, pal().accent_2);
    }
}

/// Text with a soft dark outline for legibility over images.
pub fn shadow_text(p: &egui::Painter, pos: Pos2, align: Align2, text: impl ToString, font: egui::FontId, color: Color32) -> Rect {
    let text = text.to_string();
    p.text(pos + Vec2::new(1.0, 1.0), align, &text, font.clone(), Color32::from_black_alpha(200));
    p.text(pos, align, text, font, color)
}
