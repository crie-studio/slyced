//! Secondary windows: settings, DICOM tags, keyboard help.

use super::icons;
use super::panels::search_box;
use super::theme::*;
use super::widgets::*;
use crate::app::{SlycedApp, ToastKind};
use crate::config::Theme;
use egui::{vec2, Align, RichText, Stroke, Ui};
use egui_extras::{Column, TableBuilder};

/// Section of the settings window (list on the left).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum SettingsTab {
    #[default]
    Appearance,
    Images,
    Network,
    Downloads,
    Data,
    About,
}

impl SettingsTab {
    const ALL: [SettingsTab; 6] = [SettingsTab::Appearance, SettingsTab::Images, SettingsTab::Network, SettingsTab::Downloads, SettingsTab::Data, SettingsTab::About];

    fn icon(self) -> &'static str {
        match self {
            SettingsTab::Appearance => icons::PALETTE,
            SettingsTab::Images => icons::SLIDERS_HORIZONTAL,
            SettingsTab::Network => icons::NETWORK,
            SettingsTab::Downloads => icons::TRAY_ARROW_DOWN,
            SettingsTab::Data => icons::HARD_DRIVES,
            SettingsTab::About => icons::INFO,
        }
    }

    fn label(self) -> &'static str {
        match self {
            SettingsTab::Appearance => "Aparência",
            SettingsTab::Images => "Imagens e desempenho",
            SettingsTab::Network => "Rede DICOM",
            SettingsTab::Downloads => "Exames baixados",
            SettingsTab::Data => "Dados e modo portátil",
            SettingsTab::About => "Sobre",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            SettingsTab::Appearance => "Tema e tamanho da interface.",
            SettingsTab::Images => "Como as imagens são mostradas e quanto da memória e da placa de vídeo o Slyced usa.",
            SettingsTab::Network => "Como este computador aparece para o PACS.",
            SettingsTab::Downloads => "Cópias de exames baixadas do PACS para este computador.",
            SettingsTab::Data => "Onde ficam configurações, achados e exames baixados.",
            SettingsTab::About => "Versão, placa de vídeo e autoria.",
        }
    }

    /// Section named in automation (`--do settings:rede`).
    pub fn from_name(name: &str) -> Self {
        match name {
            "imagens" => SettingsTab::Images,
            "rede" => SettingsTab::Network,
            "downloads" => SettingsTab::Downloads,
            "dados" => SettingsTab::Data,
            "sobre" => SettingsTab::About,
            _ => SettingsTab::Appearance,
        }
    }
}

/// Setting with its name and explanation on the left and the control on the right.
fn setting_row(ui: &mut Ui, title: &str, help: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let text_w = (ui.available_width() - 210.0).max(180.0);
        ui.allocate_ui_with_layout(vec2(text_w, 0.0), egui::Layout::top_down(Align::LEFT), |ui| setting_text(ui, title, help));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), control);
    });
    row_divider(ui);
}

/// Setting whose control is too wide for the side: it goes under the text.
fn setting_block(ui: &mut Ui, title: &str, help: &str, control: impl FnOnce(&mut Ui)) {
    setting_text(ui, title, help);
    ui.add_space(6.0);
    control(ui);
    row_divider(ui);
}

fn setting_text(ui: &mut Ui, title: &str, help: &str) {
    ui.label(RichText::new(title).size(13.5).color(pal().text));
    if !help.is_empty() {
        ui.add(egui::Label::new(RichText::new(help).size(11.5).color(pal().text_faint)).wrap());
    }
}

fn row_divider(ui: &mut Ui) {
    ui.add_space(9.0);
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, pal().border));
    ui.add_space(10.0);
}

impl SlycedApp {
    pub fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let mut changed = false;
        let mut portable_change: Option<bool> = None;
        let screen = ctx.content_rect();
        let size = vec2(800.0f32.min(screen.width() - 40.0), 560.0f32.min(screen.height() - 80.0));
        egui::Window::new(RichText::new(format!("{}  Configurações", icons::GEAR_SIX)).font(font_bold(15.0)))
            .open(&mut open)
            .fixed_size(size)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    // Sections.
                    ui.allocate_ui_with_layout(vec2(196.0, size.y), egui::Layout::top_down(Align::LEFT), |ui| {
                        // Same height for every section, so the window does not jump.
                        ui.set_min_height(size.y);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for tab in SettingsTab::ALL {
                            let selected = self.settings_tab == tab;
                            let (rect, resp) = ui.allocate_exact_size(vec2(188.0, 34.0), egui::Sense::click());
                            let p = ui.painter();
                            if selected {
                                p.rect_filled(rect, egui::CornerRadius::same(8), pal().selected_bg);
                            } else if resp.hovered() {
                                p.rect_filled(rect, egui::CornerRadius::same(8), pal().bg_hover);
                            }
                            let color = if selected { pal().accent } else { pal().text_dim };
                            p.text(rect.left_center() + vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, tab.icon(), font(16.0), color);
                            let text = if selected { pal().strong } else { pal().text };
                            p.text(rect.left_center() + vec2(38.0, 0.0), egui::Align2::LEFT_CENTER, tab.label(), font(13.0), text);
                            if resp.clicked() {
                                self.settings_tab = tab;
                            }
                        }
                    });
                    let r = ui.available_rect_before_wrap();
                    ui.painter().vline(r.left() - 2.0, r.top()..=r.top() + size.y, Stroke::new(1.0, pal().border));
                    ui.add_space(14.0);

                    // Content of the section.
                    ui.vertical(|ui| {
                        let tab = self.settings_tab;
                        ui.label(RichText::new(tab.label()).font(font_bold(18.0)).color(pal().text));
                        ui.label(RichText::new(tab.subtitle()).size(12.0).color(pal().text_dim));
                        ui.add_space(14.0);
                        egui::ScrollArea::vertical().id_salt("settings-page").max_height(size.y - 60.0).show(ui, |ui| {
                            ui.set_width(ui.available_width() - 8.0);
                            match tab {
                                SettingsTab::Appearance => self.settings_appearance(ui, &mut changed),
                                SettingsTab::Images => self.settings_images(ui, &mut changed),
                                SettingsTab::Network => self.settings_network(ui, &mut changed),
                                SettingsTab::Downloads => self.settings_downloads(ui, &mut changed),
                                SettingsTab::Data => self.settings_data(ui, &mut portable_change),
                                SettingsTab::About => self.settings_about(ui),
                            }
                        });
                    });
                });
            });
        if changed {
            self.settings.save(&self.paths.config);
        }
        if let Some(on) = portable_change {
            match self.paths.set_portable(on) {
                Ok(()) if on => self.toast(
                    ToastKind::Info,
                    "Modo portátil ligado: feche e abra o Slyced. As configurações atuais não são copiadas para o pendrive.",
                ),
                Ok(()) => self.toast(ToastKind::Info, "Modo portátil desligado: feche e abra o Slyced para usar a pasta do usuário."),
                Err(e) => self.toast(ToastKind::Error, format!("Não foi possível alterar o modo (a pasta do executável é somente leitura?): {e}")),
            }
        }
        self.show_settings = open;
    }

    fn settings_appearance(&mut self, ui: &mut Ui, changed: &mut bool) {
        setting_block(ui, "Tema", "As imagens continuam sobre fundo escuro em todos os temas.", |ui| {
            let mut theme = self.settings.theme;
            let options: Vec<(Theme, &str, &str)> = Theme::ALL
                .iter()
                .map(|t| (*t, match t { Theme::Dark => icons::MOON, Theme::Light => icons::SUN, Theme::HighContrast => icons::CIRCLE_HALF }, t.label()))
                .collect();
            if segmented(ui, &mut theme, &options) {
                self.settings.theme = theme;
                *changed = true;
            }
        });
        setting_block(ui, "Tamanho da interface", "Aumente em monitores grandes ou de alta resolução; diminua em telas pequenas.", |ui| {
            let mut pct = (self.settings.ui_scale * 100.0).round() as u32;
            let sizes: Vec<(u32, &str, String)> = [80, 90, 100, 115, 130, 150].into_iter().map(|p| (p, "", format!("{p}%"))).collect();
            let options: Vec<(u32, &str, &str)> = sizes.iter().map(|(p, i, l)| (*p, *i, l.as_str())).collect();
            if segmented(ui, &mut pct, &options) {
                self.settings.ui_scale = pct as f32 / 100.0;
                *changed = true;
            }
        });
        setting_row(ui, "Tour de boas-vindas", "Mostra de novo os controles principais, passo a passo.", |ui| {
            if ui.button(format!("{}  Fazer o tour", icons::LIGHTBULB)).clicked() {
                self.show_settings = false;
                self.start_tour();
            }
        });
    }

    fn settings_images(&mut self, ui: &mut Ui, changed: &mut bool) {
        setting_row(ui, "Interpolação suave", "Suaviza os pixels ao aproximar a imagem. Desligue para ver os pixels exatos.", |ui| {
            *changed |= toggle(ui, &mut self.settings.smooth).changed();
        });
        setting_row(ui, "Informações sobre a imagem", "Paciente, série, corte, janela e régua nos cantos da imagem.", |ui| {
            *changed |= toggle(ui, &mut self.settings.overlays).changed();
        });
        setting_row(ui, "Localizador", "Vista frontal do volume no canto das imagens axiais, com o nível do corte; clique nela para ir a outro nível.", |ui| {
            *changed |= toggle(ui, &mut self.settings.localizer).changed();
        });
        setting_row(ui, "Tira de séries", "Miniaturas das séries do exame embaixo das imagens; ← e → também trocam de série.", |ui| {
            *changed |= toggle(ui, &mut self.settings.filmstrip).changed();
        });
        setting_row(ui, "Tela cheia de leitura (F11)", "Texto sobre as imagens: completo, mínimo (plano, corte e janela) ou nenhum (só as letras de orientação). Ctrl+I troca durante a leitura.", |ui| {
            let options: Vec<(crate::config::InfoLevel, &str, &str)> = crate::config::InfoLevel::ALL.iter().map(|l| (*l, "", l.label())).collect();
            *changed |= segmented(ui, &mut self.settings.reading_info, &options);
        });
        setting_row(ui, "Iluminação 3D", self.settings.vr_lighting.help(), |ui| {
            let options: Vec<(crate::config::Lighting, &str, &str)> = crate::config::Lighting::ALL.iter().map(|l| (*l, "", l.label())).collect();
            *changed |= segmented(ui, &mut self.settings.vr_lighting, &options);
        });
        setting_row(ui, "Qualidade 3D", "Mais amostras por raio: imagem 3D mais fina e mais trabalho para a placa de vídeo.", |ui| {
            *changed |= ui.add(egui::Slider::new(&mut self.settings.vr_quality, 0.5..=2.5).max_decimals(1)).changed();
        });
        let effective = crate::platform::memory_budget_mb(self.settings.memory_budget_mb);
        let help = if effective < self.settings.memory_budget_mb {
            format!("Acima disso, as séries usadas há mais tempo saem da memória (continuam no disco). Neste computador o limite é {effective} MB (no máximo metade da memória instalada).")
        } else {
            "Acima disso, as séries usadas há mais tempo saem da memória (continuam no disco).".to_string()
        };
        setting_row(ui, "Memória para séries", &help, |ui| {
            *changed |= ui.add(egui::DragValue::new(&mut self.settings.memory_budget_mb).range(512..=65536).suffix(" MB").speed(64.0)).changed();
        });
    }

    fn settings_network(&mut self, ui: &mut Ui, changed: &mut bool) {
        setting_row(ui, "AE Title deste computador", "Nome com que o Slyced se apresenta ao PACS (até 16 letras).", |ui| {
            let mut ae = self.settings.local_ae.clone();
            if ui.add(egui::TextEdit::singleline(&mut ae).char_limit(16).desired_width(170.0)).changed() {
                self.settings.local_ae = ae.trim().to_uppercase();
                *changed = true;
            }
        });
        setting_row(ui, "Porta do receptor", "Porta em que o receptor recebe exames enviados pelo PACS (C-MOVE).", |ui| {
            *changed |= ui.add(egui::DragValue::new(&mut self.settings.local_port).range(1..=65535)).changed();
        });
        let status = match &self.scp {
            Some(scp) => format!("Ligado em {}:{}.", scp.ae_title, scp.port),
            None => "Desligado.".to_string(),
        };
        let help = format!("{status} Só é necessário para C-MOVE ou envio a partir do PACS; o Windows pode pedir liberação no firewall.");
        setting_row(ui, "Receptor (Storage SCP)", &help, |ui| {
            if self.scp.is_some() {
                if ui.button("Desligar").clicked() {
                    self.stop_scp();
                }
            } else if ui.button("Ligar").clicked() {
                self.start_scp();
            }
        });
        setting_row(ui, "Ligar o receptor ao abrir", "", |ui| {
            *changed |= toggle(ui, &mut self.settings.scp_autostart).changed();
        });
        let n = self.settings.nodes.len();
        let servers = match n {
            0 => "Nenhum servidor cadastrado.".to_string(),
            1 => "1 servidor cadastrado.".to_string(),
            n => format!("{n} servidores cadastrados."),
        };
        setting_row(ui, "Servidores PACS", &servers, |ui| {
            if ui.button(format!("{}  Gerenciar", icons::NETWORK)).clicked() {
                self.show_settings = false;
                self.pacs.open = true;
            }
        });
        setting_row(
            ui,
            "Exames anteriores na linha do tempo",
            "Ao abrir um exame, pergunta ao PACS selecionado pelos outros exames do mesmo paciente (mesmo ID e data de nascimento) e os mostra na linha do tempo da lista de exames.",
            |ui| {
                if toggle(ui, &mut self.settings.timeline_pacs).changed() {
                    *changed = true;
                    self.priors.clear();
                }
            },
        );
    }

    fn settings_downloads(&mut self, ui: &mut Ui, changed: &mut bool) {
        let usage = match self.store_usage {
            Some(u) if u.studies > 0 => u.describe(),
            Some(_) => "Nenhum exame baixado.".to_string(),
            None => "Medindo…".to_string(),
        };
        setting_row(ui, "Neste computador agora", &usage, |ui| {
            let can = self.store_usage.is_some_and(|u| u.studies > 0);
            let button = egui::Button::new(RichText::new(format!("{}  Apagar agora", icons::TRASH)).color(if can { DANGER } else { pal().text_faint }));
            if ui.add_enabled(can, button).clicked() {
                self.confirm_clear_store = true;
            }
            if ui.button(format!("{}  Abrir a pasta", icons::FOLDER_OPEN)).clicked() {
                let _ = std::fs::create_dir_all(&self.paths.store);
                let _ = std::process::Command::new("explorer").arg(&self.paths.store).spawn();
            }
        });
        setting_block(ui, "Apagar automaticamente", "Exames baixados há mais tempo que isso são apagados ao abrir o Slyced.", |ui| {
            let mut days = self.settings.store_keep_days;
            if segmented(ui, &mut days, &[(7, "", "7 dias"), (30, "", "30 dias"), (90, "", "90 dias"), (0, "", "Nunca")]) {
                self.settings.store_keep_days = days;
                *changed = true;
                if days > 0 {
                    self.clean_store_now(Some(days));
                }
            }
        });
        setting_row(ui, "Apagar todos ao fechar o Slyced", "Para computadores compartilhados: nada fica no disco depois do uso.", |ui| {
            *changed |= toggle(ui, &mut self.settings.store_clear_on_exit).changed();
        });
        ui.label(
            RichText::new(format!(
                "{}  Só as cópias baixadas do PACS são apagadas. Exames abertos de pastas, CDs ou pendrives nunca são tocados, e os exames abertos no momento ficam. Dá para baixá-los de novo do PACS quando precisar.",
                icons::INFO
            ))
            .size(11.5)
            .color(pal().text_faint),
        );
    }

    fn settings_data(&mut self, ui: &mut Ui, portable_change: &mut Option<bool>) {
        let mode = if self.paths.portable { "Portátil: ao lado do executável." } else { "Pasta do usuário do Windows: cada pessoa tem a sua." };
        setting_row(ui, "Pasta de dados", mode, |ui| {
            if ui.button(format!("{}  Abrir", icons::FOLDER_OPEN)).clicked() {
                let _ = std::process::Command::new("explorer").arg(&self.paths.root).spawn();
            }
        });
        ui.add(egui::Label::new(RichText::new(self.paths.root.display().to_string()).font(mono(11.5)).color(pal().text_dim)).wrap());
        row_divider(ui);
        setting_row(
            ui,
            "Modo portátil",
            "Salva os dados junto ao executável, para levar o Slyced num pendrive. Vale na próxima vez que o Slyced for aberto.",
            |ui| {
                let mut portable = self.paths.portable_requested();
                if toggle(ui, &mut portable).changed() {
                    *portable_change = Some(portable);
                }
            },
        );
    }

    fn settings_about(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            super::panels::logo(ui, 48.0);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("Slyced").font(font_bold(20.0)).color(pal().text));
                ui.label(RichText::new(format!("versão {}", crate::config::APP_VERSION)).size(12.5).color(pal().text_faint));
            });
        });
        ui.add_space(12.0);
        setting_row(ui, "Placa de vídeo", &self.adapter, |_| {});
        setting_row(ui, "Novidades desta versão", "", |ui| {
            if ui.button(format!("{}  Ver", icons::ROCKET_LAUNCH)).clicked() {
                self.show_settings = false;
                self.show_news = true;
            }
        });
        setting_row(ui, "Créditos e licenças", "", |ui| {
            if ui.button(format!("{}  Sobre o Slyced", icons::INFO)).clicked() {
                self.show_about = true;
            }
        });
        super::credit::author_badge(ui, 28.0);
        ui.add_space(10.0);
        ui.label(
            RichText::new("Software de visualização sem certificação como dispositivo médico. Não utilize como única base para diagnóstico.")
                .size(11.5)
                .color(WARNING),
        );
    }

    pub fn tags_window(&mut self, ctx: &egui::Context) {
        let Some((title, rows)) = &self.tags else { return };
        let mut open = true;
        let filter = self.tag_filter.to_lowercase();
        let title = title.clone();
        let visible: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                filter.is_empty()
                    || r.name.to_lowercase().contains(&filter)
                    || r.value.to_lowercase().contains(&filter)
                    || format!("{:04x},{:04x}", r.tag.group(), r.tag.element()).contains(&filter)
            })
            .map(|(i, _)| i)
            .collect();
        let mut filter_text = self.tag_filter.clone();
        egui::Window::new(RichText::new(format!("{}  Tags DICOM · {title}", icons::TAG)).font(font_bold(15.0)))
            .open(&mut open)
            .default_size(vec2(760.0, 600.0))
            .collapsible(false)
            .show(ctx, |ui| {
                search_box(ui, &mut filter_text, "Filtrar por nome, valor ou tag (ex.: 0010,0010)");
                ui.add_space(6.0);
                let rows = &self.tags.as_ref().unwrap().1;
                TableBuilder::new(ui)
                    .striped(true)
                    .cell_layout(egui::Layout::left_to_right(Align::Center))
                    .column(Column::exact(96.0))
                    .column(Column::exact(34.0))
                    .column(Column::initial(240.0).clip(true))
                    .column(Column::remainder().clip(true))
                    .header(22.0, |mut h| {
                        for t in ["Tag", "VR", "Nome", "Valor"] {
                            h.col(|ui| {
                                ui.label(RichText::new(t).font(font_bold(12.0)).color(pal().text_faint));
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(22.0, visible.len(), |mut row| {
                            let r = &rows[visible[row.index()]];
                            let indent = "    ".repeat(r.depth);
                            row.col(|ui| {
                                ui.label(RichText::new(format!("{indent}{:04X},{:04X}", r.tag.group(), r.tag.element())).font(mono(12.0)).color(pal().text_dim));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&r.vr).font(mono(12.0)).color(pal().text_faint));
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&r.name).color(pal().text));
                            });
                            row.col(|ui| {
                                let resp = ui.add(egui::Label::new(RichText::new(&r.value).color(pal().text_dim)).truncate().sense(egui::Sense::click()));
                                if resp.clicked() {
                                    ui.ctx().copy_text(r.value.clone());
                                }
                                resp.on_hover_text("Clique para copiar");
                            });
                        });
                    });
            });
        self.tag_filter = filter_text;
        if !open {
            self.tags = None;
        }
    }

    pub fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        egui::Window::new(RichText::new(format!("{}  Sobre o Slyced", icons::INFO)).font(font_bold(15.0)))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                // Auto-sized window as wide as the card, so the title bar matches the body.
                ui.set_width(super::credit::CARD_SIZE.x);
                ui.horizontal(|ui| {
                    super::panels::logo(ui, 44.0);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Slyced").font(font_bold(22.0)).color(pal().text));
                        ui.label(
                            RichText::new(format!("Versão {} · Visualizador DICOM portátil", crate::config::APP_VERSION))
                                .size(12.5)
                                .color(pal().text_dim),
                        );
                    });
                });
                ui.add_space(14.0);
                super::credit::author_card(ui);
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    let page = crate::config::PROJECT_PAGE;
                    let text = format!("{}  Conheça o Slyced: página do projeto  {}", icons::GLOBE, icons::ARROW_UP_RIGHT);
                    if ui.link(RichText::new(text).size(12.5)).on_hover_text(page).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(page));
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Feito com dicom-rs, egui, wgpu e Phosphor Icons (licenças MIT/Apache-2.0).")
                            .size(11.5)
                            .color(pal().text_faint),
                    );
                    ui.label(
                        RichText::new("Sem certificação como dispositivo médico: não utilize como única base para diagnóstico.")
                            .size(11.5)
                            .color(WARNING),
                    );
                    ui.add_space(4.0);
                });
            });
        self.show_about = open;
    }

    pub fn help_window(&mut self, ctx: &egui::Context) {
        if !self.show_help {
            return;
        }
        let mut open = true;
        egui::Window::new(RichText::new(format!("{}  Atalhos", icons::QUESTION)).font(font_bold(15.0)))
            .open(&mut open)
            .default_width(520.0)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ghost_button(ui, icons::LIGHTBULB, "Fazer o tour").clicked() {
                        self.start_tour();
                    }
                    if ghost_button(ui, icons::ROCKET_LAUNCH, "Novidades desta versão").clicked() {
                        self.show_news = true;
                    }
                });
                ui.add_space(8.0);
                let rows: &[(&str, &str)] = &[
                    ("Roda do mouse", "Navegar cortes"),
                    ("Barra à direita da imagem", "Passar o mouse mostra o corte; clicar/arrastar vai até ele"),
                    ("Ctrl+G", "Ir para um corte (número) ou posição (mm)"),
                    ("Ctrl + roda", "Zoom no cursor"),
                    ("Ctrl + arrastar (esquerdo)", "Zoom (2D e 3D), sem precisar de roda"),
                    ("+ / −", "Aproximar / afastar a janela ativa"),
                    ("Botão direito (arrastar)", "Janela / nível"),
                    ("Botão do meio / Shift + esquerdo", "Mover imagem"),
                    ("Duplo clique", "Maximizar / restaurar janela"),
                    ("↑ ↓  PgUp PgDn  Home End", "Navegar cortes"),
                    ("1 … 9, 0", "Predefinições de janela (TC); 0 = gordura mesentérica"),
                    ("W P Z S C", "Janela, mover, zoom, navegar, mira MPR"),
                    ("Ctrl + clique  ·  L", "Cursor 3D: leva todas as séries e planos ao mesmo ponto"),
                    ("MPR: alças da mira (C)", "Inclinar os planos (MPR oblíquo)"),
                    ("D A E H T", "Distância, ângulo, ROI elíptica, ponto, seta com texto"),
                    ("B", "Marcar achado (lista em Achados, reabre no mesmo ponto)"),
                    ("M", "Lupa"),
                    ("V", "Rastreador tubular (CPR) no 2D ou MPR: cliques ao longo da estrutura; direito/Enter conclui, Backspace desfaz, roda gira a imagem esticada"),
                    ("Realces (barra superior)", "Gordura mesentérica, Gás livre e Cálculos"),
                    ("H na comparação de fases", "ΔHU entre as fases sem e com contraste no mesmo ponto"),
                    ("G (Comparar)", "Pontos de referência para alinhar dois exames"),
                    ("Ctrl+Z  ·  Ctrl+Y", "Desfazer  ·  refazer (medidas, realces, janela, cursor, planos)"),
                    ("Botão direito (clique)", "Menu com o que dá para fazer naquele ponto (arrastar continua ajustando a janela)"),
                    ("← / →  ·  botões laterais do mouse", "Série anterior / seguinte do exame"),
                    ("Alt+← / Alt+→", "Voltar / avançar para onde você estava (antes de pular para um achado, cálculo ou outra série)"),
                    ("Tab", "Modo foco: esconde os painéis laterais (na tela cheia de leitura, abre e fecha a lista de exames)"),
                    ("F11", "Tela cheia de leitura: só as imagens; mexa o mouse para os controles, encoste à esquerda para os exames"),
                    ("Ctrl+I", "Informações sobre a imagem: completa, mínima ou nenhuma (na tela cheia); liga e desliga (tela normal)"),
                    ("Ctrl+C", "Copiar a imagem da janela ativa"),
                    ("Del  ·  Shift+Del", "Remover a última medição  ·  apagar todas as medidas da tela"),
                    ("Espaço", "Cine"),
                    ("I / R", "Inverter / restaurar visualização"),
                    ("Shift+R  ·  botão Restaurar na barra", "Restaurar tudo: medidas, realces, filtros, cursor e planos voltam ao padrão (achados salvos ficam)"),
                    ("Esc", "Fechar \"Ir para\" ou a lista de exames da tela cheia / sair da tela cheia / apagar o cursor 3D"),
                    ("Ctrl+W  ·  X no canto da janela", "Fechar a série da janela (no MPR/3D volta ao 2D; na comparação, fecha o lado)"),
                    ("H", "Densidade pontual, com legenda dos valores típicos de cada tecido"),
                    ("Ctrl+O / Ctrl+Shift+O", "Abrir pasta / arquivos"),
                    ("3D: esquerdo / direito / roda", "Girar / ajustar função de transferência / zoom"),
                    ("3D: Shift + esquerdo", "Mover (mouse sem botão do meio)"),
                ];
                egui::ScrollArea::vertical().max_height(ctx.content_rect().height() - 160.0).show(ui, |ui| {
                    egui::Grid::new("help").num_columns(2).striped(true).spacing(vec2(18.0, 8.0)).show(ui, |ui| {
                        for (k, v) in rows {
                            ui.label(RichText::new(*k).font(mono(12.5)).color(pal().accent));
                            ui.label(RichText::new(*v).color(pal().text));
                            ui.end_row();
                        }
                    });
                });
            });
        self.show_help = open;
    }
}
