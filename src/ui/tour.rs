//! Welcome tour (first run: five steps pointing at the main controls) and the
//! news of a new version.

use super::icons;
use super::theme::*;
use super::widgets::*;
use crate::app::SlycedApp;
use crate::config::APP_VERSION;
use egui::{pos2, vec2, Color32, CornerRadius, Frame, Id, LayerId, Margin, Order, Rect, RichText, Sense, Stroke, StrokeKind};

/// Title and text of each step; `SlycedApp::tour_targets` holds what each one points at.
const STEPS: [(&str, &str); 5] = [
    ("Abra seus exames", "Pasta, arquivos, ZIP ou CD/DVD — ou arraste para a janela. Os abertos recentemente ficam no menu ⋯ da lista de exames."),
    ("Busque no PACS", "Cadastre o servidor do hospital e baixe exames sem sair do Slyced. O andamento dos downloads aparece na barra de status."),
    (
        "Ferramentas",
        "As de navegação ficam à mostra; Medir e Leitura agrupam as demais (o botão usa a última, a seta mostra todas). Como usar a ferramenta ativa aparece na barra de status.",
    ),
    ("Desfazer e restaurar", "Ctrl+Z desfaz e Ctrl+Y refaz. O que mudou na imagem aparece em etiquetas no canto, e Restaurar volta tudo ao padrão."),
    ("Do seu jeito", "Nas Configurações: tema escuro, claro ou de alto contraste e o tamanho da interface. F1 mostra todos os atalhos."),
];

/// What changed in this version (shown once after an update).
const NEWS: &[&str] = &[
    "Versão para Windows 7 (64 e 32 bits), com o 3D por OpenGL ou Vulkan.",
    "Tela de abertura nova: Pasta, Arquivos e PACS lado a lado, cada um com o que abre e o atalho.",
    "A barra de título mostra só \"Slyced - por Luiz Felipe Pitzer\", sem o nome do paciente.",
    "Na 1.4: tela cheia de leitura (F11), prévia na barra de cortes, Ctrl+G e linha do tempo do paciente com os exames do PACS.",
];

enum TourAction {
    Back,
    Next,
    Skip,
}

impl SlycedApp {
    /// First run: the tour; after an update: the news.
    pub(crate) fn greet(&mut self) {
        let new_user = self.settings.recent.is_empty() && self.settings.nodes.is_empty();
        if !self.settings.tour_done && new_user {
            self.tour = Some(0);
        } else if self.settings.seen_version != APP_VERSION {
            self.show_news = true;
        }
    }

    pub(crate) fn start_tour(&mut self) {
        self.show_news = false;
        self.show_help = false;
        self.tour = Some(0);
    }

    fn end_tour(&mut self) {
        self.tour = None;
        self.settings.tour_done = true;
        self.settings.seen_version = APP_VERSION.to_string();
        self.settings.save(&self.paths.config);
    }

    /// Dims the window around the control of the current step and explains it.
    pub(crate) fn tour_ui(&mut self, ctx: &egui::Context) {
        let Some(step) = self.tour else { return };
        let step = step.min(STEPS.len() - 1);
        let screen = ctx.content_rect();
        let target = self.tour_targets[step];
        let has_target = target.is_positive();

        // Clicks outside the card do nothing while the tour is open.
        egui::Area::new(Id::unique("tour-block")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            ui.allocate_rect(screen, Sense::click_and_drag());
        });
        let p = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("tour-dim")));
        let dim = Color32::from_black_alpha(150);
        if has_target {
            let t = target.expand(6.0);
            for r in [
                Rect::from_min_max(screen.min, pos2(screen.right(), t.top())),
                Rect::from_min_max(pos2(screen.left(), t.bottom()), screen.max),
                Rect::from_min_max(pos2(screen.left(), t.top()), pos2(t.left(), t.bottom())),
                Rect::from_min_max(pos2(t.right(), t.top()), pos2(screen.right(), t.bottom())),
            ] {
                p.rect_filled(r, CornerRadius::ZERO, dim);
            }
            p.rect_stroke(t, CornerRadius::same(10), Stroke::new(2.0, pal().accent), StrokeKind::Outside);
        } else {
            p.rect_filled(screen, CornerRadius::ZERO, dim);
        }

        let card_w = 360.0;
        let pos = if has_target {
            pos2((target.center().x - card_w * 0.5).clamp(screen.left() + 12.0, screen.right() - card_w - 12.0), target.bottom() + 18.0)
        } else {
            screen.center() - vec2(card_w * 0.5, 90.0)
        };
        let mut action = None;
        egui::Area::new(Id::unique("tour-card")).order(Order::Tooltip).fixed_pos(pos).show(ctx, |ui| {
            Frame::new()
                .fill(pal().bg_raised)
                .stroke(Stroke::new(1.0, pal().accent))
                .corner_radius(12)
                .inner_margin(Margin::same(16))
                .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(pal().shadow) })
                .show(ui, |ui| {
                    ui.set_width(card_w - 32.0);
                    ui.label(RichText::new(format!("{} DE {}", step + 1, STEPS.len())).font(font_bold(10.5)).color(pal().text_faint).extra_letter_spacing(0.8));
                    ui.label(RichText::new(STEPS[step].0).font(font_bold(16.0)).color(pal().text));
                    ui.add_space(2.0);
                    ui.add(egui::Label::new(RichText::new(STEPS[step].1).size(12.5).color(pal().text_dim)).wrap());
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new("Pular").color(pal().text_dim)).frame(false)).clicked() {
                            action = Some(TourAction::Skip);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let last = step + 1 == STEPS.len();
                            if primary_button(ui, "", if last { "Concluir" } else { "Próximo" }).clicked() {
                                action = Some(TourAction::Next);
                            }
                            if step > 0 && ui.button("Voltar").clicked() {
                                action = Some(TourAction::Back);
                            }
                        });
                    });
                });
        });
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                action = Some(TourAction::Skip);
            } else if i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::ArrowRight) {
                action = Some(TourAction::Next);
            } else if i.key_pressed(egui::Key::ArrowLeft) && step > 0 {
                action = Some(TourAction::Back);
            }
        });
        match action {
            Some(TourAction::Back) => self.tour = Some(step - 1),
            Some(TourAction::Next) if step + 1 < STEPS.len() => self.tour = Some(step + 1),
            Some(TourAction::Next) | Some(TourAction::Skip) => self.end_tour(),
            None => {}
        }
    }

    /// What is new in this version (once after an update).
    pub(crate) fn news_ui(&mut self, ctx: &egui::Context) {
        if !self.show_news {
            return;
        }
        let mut close = false;
        let mut tour = false;
        egui::Modal::new(Id::unique("news")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(RichText::new(format!("{}  Novidades da versão {APP_VERSION}", icons::ROCKET_LAUNCH)).font(font_bold(16.0)).color(pal().text));
            ui.add_space(8.0);
            for n in NEWS {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new(icons::CHECK).color(pal().accent));
                    ui.add(egui::Label::new(RichText::new(*n).size(12.5).color(pal().text)).wrap());
                });
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ghost_button(ui, icons::LIGHTBULB, "Fazer o tour").clicked() {
                    tour = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if primary_button(ui, "", "Entendi").clicked() {
                        close = true;
                    }
                });
            });
        });
        if close || tour {
            self.show_news = false;
            self.settings.seen_version = APP_VERSION.to_string();
            self.settings.save(&self.paths.config);
        }
        if tour {
            self.start_tour();
        }
    }
}
