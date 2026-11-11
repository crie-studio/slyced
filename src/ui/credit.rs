//! Author credit ("Luiz Felipe Pitzer · lfp.app.br"): a monogram card for the
//! About window and a compact pill badge for footers and the status bar.

use super::icons;
use super::theme::*;
use super::widgets::{gradient_rect, lighten, with_alpha};
use crate::config::{AUTHOR, WEBSITE};
use egui::text::{LayoutJob, TextFormat};
use egui::{pos2, vec2, Align2, Color32, CornerRadius, FontId, Galley, Painter, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::sync::Arc;

pub const SITE_LABEL: &str = "lfp.app.br";
const INITIALS: &str = "LFP";
pub const CARD_SIZE: Vec2 = vec2(380.0, 236.0);

fn spaced(p: &Painter, text: &str, font: FontId, color: Color32, spacing: f32) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.append(text, 0.0, TextFormat { font_id: font, color, extra_letter_spacing: spacing, ..Default::default() });
    p.layout_job(job)
}

pub fn open_site(ctx: &egui::Context) {
    ctx.open_url(egui::OpenUrl::new_tab(WEBSITE));
}

/// Rounded square with the author's initials.
fn monogram(p: &Painter, rect: Rect, initials: &str) {
    let radius = (rect.width() * 0.28) as u8;
    gradient_rect(p, rect, radius, pal().accent, pal().accent_2);
    let top = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.center().y));
    p.rect_filled(top, CornerRadius { nw: radius, ne: radius, sw: 0, se: 0 }, Color32::from_white_alpha(16));
    let g = spaced(p, initials, font_bold(rect.height() * 0.3), Color32::WHITE, rect.height() * 0.02);
    p.galley(rect.center() - g.size() * 0.5, g, Color32::WHITE);
}

fn halo(p: &Painter, c: Pos2, radius: f32) {
    for i in 0..14 {
        let r = radius * (1.0 - i as f32 / 14.0);
        p.circle_filled(c, r, with_alpha(pal().accent, 4));
    }
}

/// Monogram card for the About window; the site link opens the browser.
pub fn author_card(ui: &mut Ui) -> Response {
    let (r, resp) = ui.allocate_exact_size(CARD_SIZE, Sense::hover());
    let p = ui.painter_at(r.expand(2.0));
    p.rect_filled(r, CornerRadius::same(16), pal().faint_bg);
    p.rect_stroke(r, CornerRadius::same(16), Stroke::new(1.0, pal().border), StrokeKind::Inside);
    gradient_rect(&p, Rect::from_min_max(pos2(r.left() + 28.0, r.top()), pos2(r.right() - 28.0, r.top() + 2.0)), 1, pal().accent, pal().accent_2);

    let m = Rect::from_min_size(pos2(r.left() + 30.0, r.top() + 46.0), vec2(76.0, 76.0));
    halo(&p, m.center(), 80.0);
    monogram(&p, m, INITIALS);

    let x = m.right() + 22.0;
    let cap = spaced(&p, tr!("CRIADO E DESENVOLVIDO POR", "CREATED AND DEVELOPED BY"), font_bold(10.0), pal().text_faint, 1.4);
    let mut y = m.top() + 6.0;
    let cap_h = cap.size().y;
    p.galley(pos2(x, y), cap, pal().text_faint);
    y += cap_h + 6.0;
    let name = p.layout_no_wrap(AUTHOR.to_string(), font_bold(21.0), pal().text);
    let name_h = name.size().y;
    p.galley(pos2(x, y), name, pal().text);
    y += name_h + 10.0;

    let link = p.layout_no_wrap(format!("{}  {}  {}", icons::GLOBE, SITE_LABEL, icons::ARROW_UP_RIGHT), font(13.0), pal().accent);
    let link_rect = Rect::from_min_size(pos2(x, y), link.size());
    let l = ui.interact(link_rect.expand(4.0), ui.scope_id().with("author-site"), Sense::click()).on_hover_text(WEBSITE);
    let c = if l.hovered() { lighten(pal().accent, 0.3) } else { pal().accent };
    p.galley_with_override_text_color(link_rect.min, link, c);
    if l.hovered() {
        p.hline(link_rect.x_range(), link_rect.bottom() + 1.0, Stroke::new(1.0, with_alpha(c, 110)));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if l.clicked() {
        open_site(ui.ctx());
    }

    let fy = r.bottom() - 46.0;
    p.hline(r.left() + 28.0..=r.right() - 28.0, fy, Stroke::new(1.0, pal().border));
    p.text(pos2(r.left() + 28.0, fy + 23.0), Align2::LEFT_CENTER, format!("Slyced {}", crate::config::APP_VERSION), font(12.0), pal().text_faint);
    p.text(pos2(r.right() - 28.0, fy + 23.0), Align2::RIGHT_CENTER, "© 2026", font(12.0), pal().text_faint);
    resp
}

/// Compact pill: mini monogram, name and site. `height` 28 for footers, ~20 for the status bar.
/// Clicking opens the site.
pub fn author_badge(ui: &mut Ui, height: f32) -> Response {
    let p = ui.painter().clone();
    let size_txt = (height * 0.45).clamp(10.5, 12.5);
    let name = p.layout_no_wrap(AUTHOR.to_string(), font(size_txt), pal().text_dim);
    let site = p.layout_no_wrap(format!("{}  {}", SITE_LABEL, icons::ARROW_UP_RIGHT), font(size_txt), pal().accent);
    let mono = height - 10.0;
    let size = vec2(5.0 + mono + 8.0 + name.size().x + 18.0 + site.size().x + 12.0, height);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = resp.on_hover_text(trf!("Abrir {WEBSITE}", "Open {WEBSITE}"));
    let hovered = resp.hovered();
    let radius = (height * 0.5) as u8;
    p.rect_filled(rect, CornerRadius::same(radius), if hovered { pal().bg_hover } else { pal().bg_raised });
    p.rect_stroke(
        rect,
        CornerRadius::same(radius),
        Stroke::new(1.0, if hovered { with_alpha(pal().accent, 140) } else { pal().border_strong }),
        StrokeKind::Inside,
    );
    let m = Rect::from_min_size(pos2(rect.left() + 5.0, rect.center().y - mono * 0.5), vec2(mono, mono));
    gradient_rect(&p, m, (mono * 0.5) as u8, pal().accent, pal().accent_2);
    let g = spaced(&p, "LF", font_bold(mono * 0.42), Color32::WHITE, 0.2);
    p.galley(m.center() - g.size() * 0.5, g, Color32::WHITE);
    let mut x = m.right() + 8.0;
    let nw = name.size().x;
    p.galley(pos2(x, rect.center().y - name.size().y * 0.5), name, pal().text_dim);
    x += nw + 8.0;
    p.circle_filled(pos2(x + 1.0, rect.center().y), 1.5, pal().text_faint);
    x += 10.0;
    p.galley_with_override_text_color(
        pos2(x, rect.center().y - site.size().y * 0.5),
        site,
        if hovered { lighten(pal().accent, 0.3) } else { pal().accent },
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        open_site(ui.ctx());
    }
    resp
}
