//! Moving around the open exams: back and forward through positions
//! (Alt+← / Alt+→), the next or previous series (← / →, mouse side buttons, the
//! strip of series under the images), the right-button menu, focus mode and
//! full screen.

use super::icons;
use super::theme::*;
use super::viewport::{ContextAction, Viewport};
use crate::app::{SlycedApp, Target, ToastKind, Workspace};
use crate::imaging::lut::Window;
use egui::{pos2, vec2, Align2, Color32, CornerRadius, Frame, Margin, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use glam::DVec3;

/// Where the reading was: series, image and how it was displayed.
#[derive(Clone)]
pub struct NavPos {
    workspace: Workspace,
    view: usize,
    series: String,
    slice: usize,
    window: Window,
    zoom: Option<f32>,
    pan: Vec2,
    point: DVec3,
}

#[derive(Default)]
pub struct NavHistory {
    back: Vec<NavPos>,
    forward: Vec<NavPos>,
}

/// "Ir para" (Ctrl+G): a slice number or a position in mm on the active view.
#[derive(Default)]
pub struct GoTo {
    pub text: String,
    /// The number is a position in mm (otherwise a slice number).
    pub mm: bool,
    pub error: Option<String>,
    focus: bool,
}

const MAX_POSITIONS: usize = 50;

impl SlycedApp {
    fn view_at(&self, i: usize) -> Option<&Viewport> {
        match self.workspace {
            Workspace::Viewer => self.views.get(i),
            Workspace::Mpr => self.mpr_views.get(i),
            Workspace::Volume => Some(&self.vr_view),
            Workspace::Compare => self.cmp_views.get(i),
        }
    }

    fn view_at_mut(&mut self, i: usize) -> Option<&mut Viewport> {
        match self.workspace {
            Workspace::Viewer => self.views.get_mut(i),
            Workspace::Mpr => self.mpr_views.get_mut(i),
            Workspace::Volume => Some(&mut self.vr_view),
            Workspace::Compare => self.cmp_views.get_mut(i),
        }
    }

    /// Where a series picked for the active view opens in this workspace.
    pub(crate) fn series_target(&self) -> Target {
        match self.workspace {
            Workspace::Viewer => Target::View(self.active),
            Workspace::Mpr => Target::Mpr,
            Workspace::Volume => Target::Volume,
            Workspace::Compare => Target::Compare(self.active.min(1)),
        }
    }

    fn position(&self, view: usize, slice: Option<usize>) -> Option<NavPos> {
        let v = self.view_at(view)?;
        let series = v.content.series()?.uid.clone();
        let point = match self.workspace {
            Workspace::Compare => self.cmp.mpr[0].point,
            _ => self.mpr.point,
        };
        Some(NavPos { workspace: self.workspace, view, series, slice: slice.unwrap_or(v.slice), window: v.window, zoom: v.zoom, pan: v.pan, point })
    }

    /// Remembers the current position (or `slice` of `view`) before a jump.
    pub(crate) fn push_nav_at(&mut self, view: usize, slice: Option<usize>) {
        let Some(pos) = self.position(view, slice) else { return };
        if let Some(last) = self.nav.back.last()
            && last.series == pos.series && last.slice == pos.slice && last.workspace == pos.workspace {
                return;
            }
        self.nav.back.push(pos);
        if self.nav.back.len() > MAX_POSITIONS {
            self.nav.back.remove(0);
        }
        self.nav.forward.clear();
    }

    pub(crate) fn push_nav(&mut self) {
        self.push_nav_at(self.active, None);
    }

    /// Alt+← (`back`) or Alt+→.
    pub(crate) fn nav_step(&mut self, back: bool) {
        let target = if back { self.nav.back.pop() } else { self.nav.forward.pop() };
        let Some(target) = target else {
            self.toast(ToastKind::Info, if back { "Nada para voltar." } else { "Nada para avançar." });
            return;
        };
        if let Some(here) = self.position(self.active, None) {
            if back {
                self.nav.forward.push(here);
            } else {
                self.nav.back.push(here);
            }
        }
        if self.workspace != target.workspace {
            self.switch_workspace(target.workspace);
        }
        self.active = target.view;
        let showing = self.view_at(target.view).and_then(|v| v.content.series()).is_some_and(|s| s.uid == target.series);
        if !showing {
            self.open_series(&target.series, self.series_target());
        }
        match self.workspace {
            Workspace::Mpr => self.mpr.point = target.point,
            Workspace::Compare => self.cmp.mpr[0].point = target.point,
            _ => {}
        }
        if let Some(v) = self.view_at_mut(target.view).filter(|v| v.content.series().is_some_and(|s| s.uid == target.series)) {
            v.slice = target.slice.min(v.slice_count().saturating_sub(1));
            v.window = target.window;
            v.zoom = target.zoom;
            v.pan = target.pan;
            v.cine = false;
        }
    }

    /// The study's other series in the active view (← / →, mouse side buttons).
    pub(crate) fn step_series(&mut self, dir: i32) {
        let Some(cur) = self.active_series() else { return };
        let Some(study) = self.library.studies.iter().find(|s| s.header.uid == cur.study.uid) else { return };
        // MPR and 3D need a volume: series of a few images (localizers) are skipped there.
        let volumes = matches!(self.workspace, Workspace::Mpr | Workspace::Volume);
        let list: Vec<String> = study.series.iter().filter(|s| !volumes || s.image_count() >= 3 || s.uid == cur.uid).map(|s| s.uid.clone()).collect();
        let Some(i) = list.iter().position(|u| *u == cur.uid) else { return };
        let j = i as i32 + dir;
        if j < 0 || j >= list.len() as i32 {
            self.toast(ToastKind::Info, if dir < 0 { "Esta é a primeira série do exame." } else { "Esta é a última série do exame." });
            return;
        }
        self.push_nav();
        let target = self.series_target();
        self.open_series(&list[j as usize], target);
    }

    /// Strip of the study's series under the images (when it has more than one).
    pub(crate) fn filmstrip(&mut self, ui: &mut Ui) {
        if !self.settings.filmstrip || self.focus.is_some() || !matches!(self.workspace, Workspace::Viewer | Workspace::Mpr) {
            return;
        }
        let Some(cur) = self.active_series() else { return };
        let Some(study) = self.library.studies.iter().find(|s| s.header.uid == cur.study.uid) else { return };
        if study.series.len() < 2 {
            return;
        }
        let items: Vec<(String, i32, String, usize)> = study.series.iter().map(|s| (s.uid.clone(), s.number, s.description.clone(), s.image_count())).collect();
        let mut open = None;
        egui::Panel::bottom("filmstrip")
            .exact_size(74.0)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_panel).inner_margin(Margin::symmetric(10, 6)))
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().hline(r.x_range(), r.top() - 6.0, Stroke::new(1.0, pal().border));
                egui::ScrollArea::horizontal().id_salt("filmstrip").show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (uid, number, desc, count) in &items {
                            let current = *uid == cur.uid;
                            let (rect, resp) = ui.allocate_exact_size(vec2(60.0, 60.0), Sense::click());
                            let p = ui.painter();
                            p.rect_filled(rect, CornerRadius::same(7), Color32::BLACK);
                            if let Some(tex) = self.thumbs.get(uid) {
                                let size = tex.size_vec2();
                                let k = (rect.width() - 6.0) / size.x.max(size.y);
                                p.image(tex.id(), Rect::from_center_size(rect.center(), size * k), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                            }
                            let tag = Rect::from_min_size(rect.left_bottom() + vec2(3.0, -17.0), vec2(26.0, 14.0));
                            p.rect_filled(tag, CornerRadius::same(4), Color32::from_black_alpha(190));
                            p.text(tag.center(), Align2::CENTER_CENTER, number.to_string(), font_bold(10.0), Color32::WHITE);
                            let border = if current {
                                Stroke::new(2.0, pal().accent)
                            } else if resp.hovered() {
                                Stroke::new(1.0, pal().border_strong)
                            } else {
                                Stroke::new(1.0, pal().border)
                            };
                            p.rect_stroke(rect, CornerRadius::same(7), border, StrokeKind::Inside);
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            let tip = format!("Série {number} · {desc}\n{count} imagem(ns) · ← / → troca de série");
                            if resp.on_hover_text(tip).clicked() && !current {
                                open = Some(uid.clone());
                            }
                        }
                    });
                });
            });
        if let Some(uid) = open {
            self.push_nav();
            let target = self.series_target();
            self.open_series(&uid, target);
        }
    }

    /// Something chosen in the right-button menu of view `i`.
    pub(crate) fn context_action(&mut self, ctx: &egui::Context, i: usize, action: ContextAction) {
        self.active = i;
        let uid = self.view_at(i).and_then(|v| v.content.series()).map(|s| s.uid.clone());
        match action {
            ContextAction::Tool(t) => self.tool = t,
            ContextAction::OpenMpr => {
                if let Some(uid) = uid {
                    self.open_series(&uid, Target::Mpr);
                }
            }
            ContextAction::OpenVolume => {
                if let Some(uid) = uid {
                    self.open_series(&uid, Target::Volume);
                }
            }
            ContextAction::Copy => self.export_view(ctx, true),
            ContextAction::Save => self.export_view(ctx, false),
            ContextAction::ResetView => {
                if let Some(v) = self.view_at_mut(i) {
                    v.reset_view();
                }
            }
            ContextAction::ResetAll => self.reset_screen(),
            ContextAction::Maximize => {
                if self.workspace != Workspace::Compare {
                    self.maximized = if self.maximized == Some(i) { None } else { Some(i) };
                }
            }
        }
    }

    /// Ctrl+G: opens the "Ir para" box over the active view.
    pub(crate) fn open_goto(&mut self) {
        if self.workspace == Workspace::Volume || self.active_view().is_none_or(|v| v.slice_count() < 2) {
            self.toast(ToastKind::Info, "Ir para: a janela ativa precisa mostrar uma série de cortes (2D ou MPR).");
            return;
        }
        self.goto = Some(GoTo { focus: true, ..Default::default() });
    }

    /// The "Ir para" box: number, slice or mm, and where it can go.
    pub(crate) fn goto_ui(&mut self, ctx: &egui::Context) {
        if self.goto.is_none() {
            return;
        }
        let Some(view) = self.active_view() else {
            self.goto = None;
            return;
        };
        let n = view.slice_count();
        let range = (view.slice_position(0).zip(view.slice_position(n.saturating_sub(1)))).map(|(a, b)| (a.min(b), a.max(b)));
        let area = self.view_rects.get(self.active).copied().filter(|r| r.width() > 2.0).unwrap_or(ctx.content_rect());
        let Some(g) = self.goto.as_mut() else { return };
        let (mut go, mut close) = (false, false);
        egui::Area::new(egui::Id::unique("goto"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(area.center().x - 170.0, area.top() + 48.0))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(pal().bg_raised)
                    .stroke(Stroke::new(1.0, pal().border_strong))
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(12, 10))
                    .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(pal().shadow) })
                    .show(ui, |ui| {
                        ui.set_width(316.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("{}  Ir para", icons::ARROWS_VERTICAL)).font(font_bold(13.5)).color(pal().text));
                            let edit = ui.add(egui::TextEdit::singleline(&mut g.text).desired_width(78.0).hint_text(if g.mm { "0,0" } else { "1" }));
                            if std::mem::take(&mut g.focus) {
                                edit.request_focus();
                            }
                            if edit.changed() {
                                g.error = None;
                                if g.text.to_lowercase().contains("mm") {
                                    g.mm = true;
                                }
                            }
                            go |= edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            close |= ui.input(|i| i.key_pressed(egui::Key::Escape));
                            ui.selectable_value(&mut g.mm, false, "Corte");
                            ui.selectable_value(&mut g.mm, true, "mm");
                            go |= ui.button("Ir").clicked();
                            close |= super::widgets::icon_button_sized(ui, icons::X, "Fechar (Esc)", false, 24.0).clicked();
                        });
                        let hint = match (g.mm, range) {
                            (false, _) => format!("De 1 a {n} · Enter vai, Esc fecha"),
                            (true, Some((a, b))) => format!("De {} a {} mm · Enter vai, Esc fecha", format!("{a:.1}").replace('.', ","), format!("{b:.1}").replace('.', ",")),
                            (true, None) => "Esta série não tem posição em mm; use o número do corte.".to_string(),
                        };
                        ui.label(egui::RichText::new(hint).small().color(pal().text_faint));
                        if let Some(e) = &g.error {
                            ui.label(egui::RichText::new(e).color(DANGER));
                        }
                    });
            });
        if close {
            self.goto = None;
        } else if go {
            self.apply_goto();
        }
    }

    /// Goes to the number typed in "Ir para"; the place left is kept for Alt+←.
    pub(crate) fn apply_goto(&mut self) {
        let Some(g) = self.goto.as_mut() else { return };
        let mm = g.mm;
        let raw = g.text.to_lowercase().replace("mm", "").replace(',', ".");
        let Ok(x) = raw.trim().parse::<f64>() else {
            g.error = Some("Digite um número.".into());
            return;
        };
        let Some(view) = self.active_view() else { return };
        let n = view.slice_count();
        let target = if mm {
            view.nearest_slice(x)
        } else {
            let k = x.round() as i64;
            (1..=n as i64).contains(&k).then(|| k as usize - 1)
        };
        let Some(k) = target else {
            if let Some(g) = self.goto.as_mut() {
                g.error = Some(if mm { "Esta série não tem posição em mm.".to_string() } else { format!("Escolha um corte de 1 a {n}.") });
            }
            return;
        };
        self.push_nav();
        if let Some((v, mpr)) = self.nav_view() {
            v.cine = false;
            let d = k as i32 - v.slice as i32;
            v.step(d, Some(mpr));
        }
        self.goto = None;
    }

    /// Tab: only the images (and the toolbar); Tab again brings the panels back.
    pub(crate) fn toggle_focus(&mut self) {
        match self.focus.take() {
            Some((left, right)) => {
                self.left_open = left;
                self.right_open = right;
            }
            None => {
                self.focus = Some((self.left_open, self.right_open));
                self.left_open = false;
                self.right_open = false;
                self.toast(ToastKind::Info, "Modo foco: Tab traz os painéis de volta.");
            }
        }
    }

    /// Focus mode and full screen buttons (top bar, right side).
    pub(crate) fn view_mode_buttons(&mut self, ui: &mut Ui) {
        if super::widgets::icon_button(ui, icons::CORNERS_OUT, "Tela cheia de leitura: só as imagens (F11)", self.reading).clicked() {
            let ctx = ui.ctx().clone();
            self.toggle_reading(&ctx);
        }
        if super::widgets::icon_button(ui, icons::FRAME_CORNERS, "Modo foco: só as imagens (Tab)", self.focus.is_some()).clicked() {
            self.toggle_focus();
        }
    }
}
