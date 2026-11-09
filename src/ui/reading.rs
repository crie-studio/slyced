//! Reading full screen (F11): only the images. A small bar with the main
//! controls shows up when the mouse moves and fades when it stops (the pointer
//! hides too); the exams list slides in from the left edge.

use super::icons;
use super::panels::layout_icon;
use super::theme::*;
use super::viewport::Tool;
use super::widgets::*;
use crate::app::{Layout, SlycedApp, ToastKind, Workspace};
use crate::config::InfoLevel;
use crate::imaging::lut::CT_PRESETS;
use egui::{pos2, vec2, Align2, Color32, CornerRadius, Frame, Id, Margin, Sense, Stroke, Ui};

/// Seconds without moving the mouse before the controls start to fade.
const IDLE: f64 = 2.5;
/// Seconds the fade takes.
const FADE: f64 = 0.35;
/// Width of the exams drawer.
const DRAWER_W: f32 = 330.0;

impl SlycedApp {
    /// F11: enters or leaves the reading full screen.
    pub(crate) fn toggle_reading(&mut self, ctx: &egui::Context) {
        if self.reading {
            self.leave_reading(ctx);
            return;
        }
        if self.library.studies.is_empty() {
            self.toast(ToastKind::Info, "Abra um exame para usar a tela cheia de leitura.");
            return;
        }
        self.reading = true;
        self.reading_os = true;
        self.reading_since = self.time;
        self.reading_drawer = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        self.toast(ToastKind::Info, "Tela cheia de leitura: mexa o mouse para ver os controles; Esc ou F11 sai.");
    }

    pub(crate) fn leave_reading(&mut self, ctx: &egui::Context) {
        self.reading = false;
        self.reading_drawer = false;
        self.reading_force = None;
        if std::mem::take(&mut self.reading_os) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
    }

    /// Tab or "⋯ → Exames": the exams drawer stays open until Tab, Esc or a
    /// series is opened from it.
    pub(crate) fn pin_drawer(&mut self, on: bool) {
        self.reading_drawer = on;
        self.reading_force = on.then(|| "drawer".to_string());
    }

    /// How visible the reading controls are: 1 while the mouse moves (or a
    /// menu, the drawer or "Ir para" is open), fading to 0 when it stops.
    pub(crate) fn reading_chrome(&self, ctx: &egui::Context) -> f32 {
        let over_bar = ctx.pointer_latest_pos().zip(ctx.memory(|m| m.area_rect(Id::unique("reading-bar")))).is_some_and(|(p, r)| r.expand(8.0).contains(p));
        if self.reading_force.is_some() || self.reading_drawer || self.goto.is_some() || over_bar || egui::Popup::is_any_open(ctx) {
            return 1.0;
        }
        let idle = ctx.input(|i| i.pointer.time_since_last_movement()) as f64;
        ((IDLE + FADE - idle) / FADE).clamp(0.0, 1.0) as f32
    }

    /// Controls of the reading full screen, drawn over the images.
    pub(crate) fn reading_ui(&mut self, ctx: &egui::Context) {
        // Full screen left some other way (Windows key, another program).
        let full = ctx.input(|i| i.viewport().fullscreen.unwrap_or(true));
        if self.reading_os && !full && self.time - self.reading_since > 1.5 {
            self.leave_reading(ctx);
            return;
        }
        let screen = ctx.content_rect();
        let pointer = ctx.pointer_latest_pos();

        // Exams drawer: opens at the left edge, closes when the mouse goes away.
        let forced_drawer = self.reading_force.as_deref() == Some("drawer");
        if !self.reading_drawer && pointer.is_some_and(|p| p.x <= screen.left() + 3.0 && screen.contains(p)) && !ctx.input(|i| i.pointer.any_down()) {
            self.reading_drawer = true;
        }
        if self.reading_drawer && !forced_drawer && pointer.is_some_and(|p| p.x > screen.left() + DRAWER_W + 40.0) && !egui::Popup::is_any_open(ctx) {
            self.reading_drawer = false;
        }
        if self.reading_drawer || forced_drawer {
            egui::Area::new(Id::unique("reading-drawer")).order(egui::Order::Foreground).fixed_pos(screen.left_top()).show(ctx, |ui| {
                Frame::new()
                    .fill(pal().bg_panel)
                    .stroke(Stroke::new(1.0, pal().border))
                    .inner_margin(Margin::same(12))
                    .shadow(egui::Shadow { offset: [8, 0], blur: 28, spread: 0, color: Color32::from_black_alpha(pal().shadow) })
                    .show(ui, |ui| {
                        ui.set_width(DRAWER_W - 24.0);
                        ui.set_height(screen.height() - 24.0);
                        self.browser_contents(ui);
                    });
            });
        }

        let alpha = self.reading_chrome(ctx);
        let idle = ctx.input(|i| i.pointer.time_since_last_movement()) as f64;
        if alpha > 0.0 && alpha < 1.0 {
            ctx.request_repaint();
        } else if alpha >= 1.0 && idle < IDLE {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(IDLE - idle + 0.02));
        }
        if alpha <= 0.0 {
            // Nothing over the images, not even the pointer.
            if pointer.is_some_and(|p| screen.contains(p)) {
                ctx.set_cursor_icon(egui::CursorIcon::None);
            }
            return;
        }

        egui::Area::new(Id::unique("reading-bar")).order(egui::Order::Foreground).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -18.0)).show(ctx, |ui| {
            ui.multiply_opacity(alpha);
            Frame::new()
                .fill(with_alpha(pal().bg_panel, 242))
                .stroke(Stroke::new(1.0, pal().border_strong))
                .corner_radius(22)
                .inner_margin(Margin::symmetric(8, 5))
                .shadow(egui::Shadow { offset: [0, 6], blur: 22, spread: 0, color: Color32::from_black_alpha(pal().shadow) })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        self.reading_bar(ui);
                    });
                });
        });
        egui::Area::new(Id::unique("reading-exit")).order(egui::Order::Foreground).anchor(Align2::RIGHT_TOP, vec2(-14.0, 14.0)).show(ctx, |ui| {
            ui.multiply_opacity(alpha);
            Frame::new().fill(with_alpha(pal().bg_panel, 235)).stroke(Stroke::new(1.0, pal().border_strong)).corner_radius(18).inner_margin(Margin::same(2)).show(ui, |ui| {
                if icon_button_sized(ui, icons::CORNERS_IN, "Sair da tela cheia (Esc ou F11)", false, 30.0).clicked() {
                    let ctx = ui.ctx().clone();
                    self.leave_reading(&ctx);
                }
            });
        });
    }

    /// Contents of the floating bar: five tools, window presets, layout,
    /// workspace and "more".
    fn reading_bar(&mut self, ui: &mut Ui) {
        for t in [Tool::WindowLevel, Tool::Pan, Tool::Zoom, Tool::Length, Tool::Bookmark] {
            if icon_button_sized(ui, t.icon(), &format!("{} ({})", t.name(), t.key()), self.tool == t, 32.0).clicked() {
                self.tool = t;
            }
        }
        bar_separator(ui);

        let series = self.active_series();
        if series.as_ref().is_some_and(|s| s.info.modality == "CT") && self.workspace != Workspace::Volume {
            let window = self.active_view().map(|v| v.window);
            let default = series.as_ref().map(|s| s.default_window);
            // A named window first; "Padrão DICOM" only when no other one matches.
            let name = CT_PRESETS
                .iter()
                .find(|p| p.window.is_some() && p.window == window)
                .or_else(|| CT_PRESETS.iter().find(|p| p.window.is_none() && default == window))
                .map_or("Janela", |p| p.name);
            let resp = text_button(ui, &format!("{name}  {}", icons::CARET_UP), "Predefinições de janela (teclas 1 … 0)");
            let mut pick = None;
            egui::Popup::menu(&resp).width(210.0).show(|ui| {
                for (i, p) in CT_PRESETS.iter().enumerate() {
                    if ui.add(egui::Button::new(p.name).shortcut_text(p.key).selected(p.name == name)).clicked() {
                        pick = Some(i);
                    }
                }
            });
            if let Some(i) = pick {
                self.apply_window_preset(i);
            }
        }
        if self.workspace == Workspace::Viewer {
            let resp = icon_button_sized(ui, layout_icon(self.layout), "Layout", false, 32.0);
            egui::Popup::menu(&resp).show(|ui| {
                for l in Layout::ALL {
                    if ui.selectable_label(self.layout == l, format!("{}  {}", layout_icon(l), l.label())).clicked() {
                        self.layout = l;
                        self.maximized = None;
                        if self.active >= l.count() {
                            self.active = 0;
                        }
                        self.fill_empty_views();
                    }
                }
            });
        }
        let spaces = [(Workspace::Viewer, "2D"), (Workspace::Mpr, "MPR"), (Workspace::Volume, "3D"), (Workspace::Compare, "A|B")];
        let current = spaces.iter().find(|s| s.0 == self.workspace).map_or("2D", |s| s.1);
        let resp = text_button(ui, &format!("{current}  {}", icons::CARET_UP), "Espaço de trabalho: 2D, MPR, 3D ou comparação");
        let mut switch = None;
        egui::Popup::menu(&resp).width(150.0).show(|ui| {
            for (ws, label) in spaces {
                if ui.selectable_label(self.workspace == ws, label).clicked() {
                    switch = Some(ws);
                }
            }
        });
        if let Some(ws) = switch {
            self.switch_workspace(ws);
        }
        bar_separator(ui);

        let resp = icon_button_sized(ui, icons::DOTS_THREE, "Mais", false, 32.0);
        egui::Popup::menu(&resp).width(290.0).show(|ui| {
            let item = |ui: &mut Ui, icon: &str, text: &str, keys: &str| ui.add(egui::Button::new(format!("{icon}  {text}")).shortcut_text(keys)).clicked();
            if item(ui, icons::ARROWS_VERTICAL, "Ir para corte ou posição…", "Ctrl+G") {
                self.open_goto();
            }
            if item(ui, icons::PLAY, "Cine", "Espaço")
                && let Some(v) = self.active_view_mut() {
                    v.cine = !v.cine;
                }
            if item(ui, icons::CIRCLE_HALF, "Inverter", "I")
                && let Some(v) = self.active_view_mut() {
                    v.invert = !v.invert;
                }
            if item(ui, icons::ARROW_COUNTER_CLOCKWISE, "Restaurar visualização", "R")
                && let Some(v) = self.active_view_mut() {
                    v.reset_view();
                }
            if item(ui, icons::ARROWS_CLOCKWISE, "Restaurar tudo", "Shift+R") {
                self.reset_screen();
            }
            if item(ui, icons::COPY, "Copiar imagem", "Ctrl+C") {
                let ctx = ui.ctx().clone();
                self.export_view(&ctx, true);
            }
            ui.separator();
            ui.label(egui::RichText::new("Informações sobre a imagem (Ctrl+I)").small().color(pal().text_faint));
            ui.horizontal(|ui| {
                for level in InfoLevel::ALL {
                    if ui.selectable_label(self.settings.reading_info == level, level.label()).clicked() {
                        self.settings.reading_info = level;
                    }
                }
            });
            ui.separator();
            if item(ui, icons::SIDEBAR_SIMPLE, "Lista de exames (ou encoste o mouse à esquerda)", "Tab") {
                self.pin_drawer(true);
            }
            if item(ui, icons::GEAR_SIX, "Configurações", "") {
                self.show_settings = true;
            }
            if item(ui, icons::CORNERS_IN, "Sair da tela cheia", "Esc") {
                let ctx = ui.ctx().clone();
                self.leave_reading(&ctx);
            }
        });
    }

    /// Ctrl+I: cycles the text over the images (reading full screen) or turns
    /// it on and off (normal screen).
    pub(crate) fn cycle_info(&mut self) {
        if self.reading {
            self.settings.reading_info = self.settings.reading_info.next();
            self.toast(ToastKind::Info, format!("Informações sobre a imagem: {}", self.settings.reading_info.label().to_lowercase()));
        } else {
            self.settings.overlays = !self.settings.overlays;
        }
    }
}

/// Thin vertical line between groups of the bar.
fn bar_separator(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(9.0, 22.0), Sense::hover());
    ui.painter().vline(r.center().x, r.y_range(), Stroke::new(1.0, pal().border_strong));
}

/// Text button of the bar ("Cérebro ▴", "2D ▴").
fn text_button(ui: &mut Ui, text: &str, tooltip: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), font(13.0), pal().text);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 18.0, 32.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(8), pal().bg_hover);
    }
    p.galley(pos2(rect.left() + 9.0, rect.center().y - galley.size().y * 0.5), galley, if resp.hovered() { pal().text } else { pal().text_dim });
    resp.on_hover_text(tooltip)
}
