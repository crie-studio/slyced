//! Secondary windows: settings, DICOM tags, keyboard help.

use super::icons;
use super::panels::search_box;
use super::theme::*;
use super::widgets::*;
use crate::app::{SlycedApp, ToastKind};
use crate::config::{Language, Theme};
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
            SettingsTab::Appearance => tr!("Aparência", "Appearance"),
            SettingsTab::Images => tr!("Imagens e desempenho", "Images and performance"),
            SettingsTab::Network => tr!("Rede DICOM", "DICOM network"),
            SettingsTab::Downloads => tr!("Exames baixados", "Downloaded studies"),
            SettingsTab::Data => tr!("Dados e modo portátil", "Data and portable mode"),
            SettingsTab::About => tr!("Sobre", "About"),
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            SettingsTab::Appearance => tr!("Idioma, tema e tamanho da interface.", "Language, theme and interface size."),
            SettingsTab::Images => tr!(
                "Como as imagens são mostradas e quanto da memória e da placa de vídeo o Slyced usa.",
                "How images are shown and how much memory and graphics card Slyced uses."
            ),
            SettingsTab::Network => tr!("Como este computador aparece para o PACS.", "How this computer presents itself to the PACS."),
            SettingsTab::Downloads => tr!("Cópias de exames baixadas do PACS para este computador.", "Copies of studies downloaded from the PACS to this computer."),
            SettingsTab::Data => tr!("Onde ficam configurações, achados e exames baixados.", "Where settings, findings and downloaded studies are kept."),
            SettingsTab::About => tr!("Versão, placa de vídeo e autoria.", "Version, graphics card and authorship."),
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

/// "🌐 Português · English" pill of the welcome screen: the language in use is
/// highlighted, a click on the other one switches (and saves) it.
pub(crate) fn language_chip(ui: &mut Ui) -> Option<Language> {
    let english = crate::i18n::en();
    let mut picked = None;
    egui::Frame::new()
        .stroke(Stroke::new(1.0, pal().border_strong))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            // Globe, Português, dot, English from left to right, also when the chip
            // sits in a right-to-left row (items are then added from the right).
            let mut items = [None, Some((Language::Portuguese, "Português", !english)), None, Some((Language::English, "English", english))];
            if ui.layout().prefer_right_to_left() {
                items.reverse();
            }
            for (i, item) in items.into_iter().enumerate() {
                let Some((language, name, current)) = item else {
                    let globe_slot = if ui.layout().prefer_right_to_left() { i == 3 } else { i == 0 };
                    let text = if globe_slot { icons::GLOBE } else { "·" };
                    ui.label(RichText::new(text).size(if globe_slot { 12.0 } else { 11.5 }).color(pal().text_faint));
                    continue;
                };
                let color = if current { pal().accent } else { pal().text_dim };
                let resp = ui.add(egui::Label::new(RichText::new(name).size(11.5).color(color)).sense(egui::Sense::click()));
                if !current {
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    if resp.hovered() {
                        let r = resp.rect;
                        ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, color));
                    }
                    if resp.clicked() {
                        picked = Some(language);
                    }
                }
            }
        })
        .response
        .on_hover_text(tr!("Idioma da interface (também em Configurações → Aparência)", "Interface language (also in Settings → Appearance)"));
    picked
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
        egui::Window::new(RichText::new(trf!("{}  Configurações", "{}  Settings", icons::GEAR_SIX)).font(font_bold(15.0)))
            .id(egui::Id::unique("settings-window"))
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
                    tr!(
                        "Modo portátil ligado: feche e abra o Slyced. As configurações atuais não são copiadas para o pendrive.",
                        "Portable mode on: close and reopen Slyced. Current settings are not copied to the USB drive."
                    ),
                ),
                Ok(()) => self.toast(
                    ToastKind::Info,
                    tr!(
                        "Modo portátil desligado: feche e abra o Slyced para usar a pasta do usuário.",
                        "Portable mode off: close and reopen Slyced to use the user folder."
                    ),
                ),
                Err(e) => self.toast(
                    ToastKind::Error,
                    trf!(
                        "Não foi possível alterar o modo (a pasta do executável é somente leitura?): {e}",
                        "Could not change the mode (is the program folder read-only?): {e}"
                    ),
                ),
            }
        }
        self.show_settings = open;
    }

    /// Changes the interface language (settings and the welcome screen chip).
    pub(crate) fn set_language(&mut self, language: Language) {
        self.settings.language = language;
        crate::i18n::set(language.resolve());
        self.settings.save(&self.paths.config);
        #[cfg(target_arch = "wasm32")]
        crate::web::remember_language(language);
    }

    fn settings_appearance(&mut self, ui: &mut Ui, changed: &mut bool) {
        // Title in both languages, so it can be found in either.
        let title = if crate::i18n::en() { "Language · Idioma" } else { "Idioma · Language" };
        let system = if crate::i18n::system() == crate::i18n::Lang::English { "English" } else { "Português" };
        let help = match (self.settings.language == Language::Auto, cfg!(target_arch = "wasm32")) {
            (true, true) => trf!(
                "Automático segue o idioma do navegador (agora: {system}). Muda na hora, sem reiniciar.",
                "Automatic follows the browser language (now: {system}). Changes right away, no restart."
            ),
            (true, false) => trf!(
                "Automático segue o idioma do Windows (agora: {system}). Muda na hora, sem reiniciar.",
                "Automatic follows the Windows language (now: {system}). Changes right away, no restart."
            ),
            (false, true) => tr!(
                "Automático segue o idioma do navegador. Muda na hora, sem reiniciar.",
                "Automatic follows the browser language. Changes right away, no restart."
            )
            .to_string(),
            (false, false) => tr!(
                "Automático segue o idioma do Windows. Muda na hora, sem reiniciar.",
                "Automatic follows the Windows language. Changes right away, no restart."
            )
            .to_string(),
        };
        setting_block(ui, title, &help, |ui| {
            let mut language = self.settings.language;
            let options: Vec<(Language, &str, &str)> =
                Language::ALL.iter().map(|l| (*l, if *l == Language::Auto { icons::GLOBE } else { "" }, l.label())).collect();
            if segmented(ui, &mut language, &options) {
                self.set_language(language);
            }
        });
        setting_block(ui, tr!("Tema", "Theme"), tr!("As imagens continuam sobre fundo escuro em todos os temas.", "Images stay on a dark background in every theme."), |ui| {
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
        let help = tr!("Aumente em monitores grandes ou de alta resolução; diminua em telas pequenas.", "Increase it on large or high-resolution monitors; decrease it on small screens.");
        setting_block(ui, tr!("Tamanho da interface", "Interface size"), help, |ui| {
            let mut pct = (self.settings.ui_scale * 100.0).round() as u32;
            let sizes: Vec<(u32, &str, String)> = [80, 90, 100, 115, 130, 150].into_iter().map(|p| (p, "", format!("{p}%"))).collect();
            let options: Vec<(u32, &str, &str)> = sizes.iter().map(|(p, i, l)| (*p, *i, l.as_str())).collect();
            if segmented(ui, &mut pct, &options) {
                self.settings.ui_scale = pct as f32 / 100.0;
                *changed = true;
            }
        });
        setting_row(ui, tr!("Tour de boas-vindas", "Welcome tour"), tr!("Mostra de novo os controles principais, passo a passo.", "Shows the main controls again, step by step."), |ui| {
            if ui.button(trf!("{}  Fazer o tour", "{}  Take the tour", icons::LIGHTBULB)).clicked() {
                self.show_settings = false;
                self.start_tour();
            }
        });
    }

    fn settings_images(&mut self, ui: &mut Ui, changed: &mut bool) {
        let help = tr!("Suaviza os pixels ao aproximar a imagem. Desligue para ver os pixels exatos.", "Smooths pixels when zooming in. Turn it off to see the exact pixels.");
        setting_row(ui, tr!("Interpolação suave", "Smooth interpolation"), help, |ui| {
            *changed |= toggle(ui, &mut self.settings.smooth).changed();
        });
        let help = tr!("Paciente, série, corte, janela e régua nos cantos da imagem.", "Patient, series, slice, window and scale in the image corners.");
        setting_row(ui, tr!("Informações sobre a imagem", "Image information"), help, |ui| {
            *changed |= toggle(ui, &mut self.settings.overlays).changed();
        });
        let help = tr!(
            "Vista frontal do volume no canto das imagens axiais, com o nível do corte; clique nela para ir a outro nível.",
            "Frontal view of the volume in the corner of axial images, showing the slice level; click it to go to another level."
        );
        setting_row(ui, tr!("Localizador", "Localizer"), help, |ui| {
            *changed |= toggle(ui, &mut self.settings.localizer).changed();
        });
        let help = tr!(
            "Miniaturas das séries do exame embaixo das imagens; ← e → também trocam de série.",
            "Thumbnails of the study's series below the images; ← and → also switch series."
        );
        setting_row(ui, tr!("Tira de séries", "Series strip"), help, |ui| {
            *changed |= toggle(ui, &mut self.settings.filmstrip).changed();
        });
        let help = tr!(
            "Texto sobre as imagens: completo, mínimo (plano, corte e janela) ou nenhum (só as letras de orientação). Ctrl+I troca durante a leitura.",
            "Text over the images: full, minimal (plane, slice and window) or none (orientation letters only). Ctrl+I switches while reading."
        );
        setting_row(ui, tr!("Tela cheia de leitura (F11)", "Reading full screen (F11)"), help, |ui| {
            let options: Vec<(crate::config::InfoLevel, &str, &str)> = crate::config::InfoLevel::ALL.iter().map(|l| (*l, "", l.label())).collect();
            *changed |= segmented(ui, &mut self.settings.reading_info, &options);
        });
        setting_row(ui, tr!("Iluminação 3D", "3D lighting"), self.settings.vr_lighting.help(), |ui| {
            let options: Vec<(crate::config::Lighting, &str, &str)> = crate::config::Lighting::ALL.iter().map(|l| (*l, "", l.label())).collect();
            *changed |= segmented(ui, &mut self.settings.vr_lighting, &options);
        });
        let help = tr!(
            "Mais amostras por raio: imagem 3D mais fina e mais trabalho para a placa de vídeo.",
            "More samples per ray: a finer 3D image and more work for the graphics card."
        );
        setting_row(ui, tr!("Qualidade 3D", "3D quality"), help, |ui| {
            let slider = egui::Slider::new(&mut self.settings.vr_quality, 0.5..=2.5).max_decimals(1).custom_formatter(|v, _| crate::i18n::num(v, 1));
            *changed |= ui.add(slider).changed();
        });
        let effective = crate::platform::memory_budget_mb(self.settings.memory_budget_mb);
        let help = if effective < self.settings.memory_budget_mb {
            trf!(
                "Acima disso, as séries usadas há mais tempo saem da memória (continuam no disco). Neste computador o limite é {effective} MB (no máximo metade da memória instalada).",
                "Above this, the least recently used series leave memory (they stay on disk). On this computer the limit is {effective} MB (at most half of the installed memory)."
            )
        } else {
            tr!(
                "Acima disso, as séries usadas há mais tempo saem da memória (continuam no disco).",
                "Above this, the least recently used series leave memory (they stay on disk)."
            )
            .to_string()
        };
        setting_row(ui, tr!("Memória para séries", "Memory for series"), &help, |ui| {
            *changed |= ui.add(egui::DragValue::new(&mut self.settings.memory_budget_mb).range(512..=65536).suffix(" MB").speed(64.0)).changed();
        });
    }

    fn settings_network(&mut self, ui: &mut Ui, changed: &mut bool) {
        let help = tr!("Nome com que o Slyced se apresenta ao PACS (até 16 letras).", "Name Slyced uses to present itself to the PACS (up to 16 characters).");
        setting_row(ui, tr!("AE Title deste computador", "AE Title of this computer"), help, |ui| {
            let mut ae = self.settings.local_ae.clone();
            if ui.add(egui::TextEdit::singleline(&mut ae).char_limit(16).desired_width(170.0)).changed() {
                self.settings.local_ae = ae.trim().to_uppercase();
                *changed = true;
            }
        });
        let help = tr!("Porta em que o receptor recebe exames enviados pelo PACS (C-MOVE).", "Port where the receiver gets studies sent by the PACS (C-MOVE).");
        setting_row(ui, tr!("Porta do receptor", "Receiver port"), help, |ui| {
            *changed |= ui.add(egui::DragValue::new(&mut self.settings.local_port).range(1..=65535)).changed();
        });
        let status = match &self.scp {
            Some(scp) => trf!("Ligado em {}:{}.", "On at {}:{}.", scp.ae_title, scp.port),
            None => tr!("Desligado.", "Off.").to_string(),
        };
        let help = trf!(
            "{status} Só é necessário para C-MOVE ou envio a partir do PACS; o Windows pode pedir liberação no firewall.",
            "{status} Only needed for C-MOVE or for sending from the PACS; Windows may ask to allow it through the firewall."
        );
        setting_row(ui, tr!("Receptor (Storage SCP)", "Receiver (Storage SCP)"), &help, |ui| {
            if self.scp.is_some() {
                if ui.button(tr!("Desligar", "Turn off")).clicked() {
                    self.stop_scp();
                }
            } else if ui.button(tr!("Ligar", "Turn on")).clicked() {
                self.start_scp();
            }
        });
        setting_row(ui, tr!("Ligar o receptor ao abrir", "Start the receiver on launch"), "", |ui| {
            *changed |= toggle(ui, &mut self.settings.scp_autostart).changed();
        });
        let n = self.settings.nodes.len();
        let servers = match n {
            0 => tr!("Nenhum servidor cadastrado.", "No servers added.").to_string(),
            1 => tr!("1 servidor cadastrado.", "1 server added.").to_string(),
            n => trf!("{n} servidores cadastrados.", "{n} servers added."),
        };
        setting_row(ui, tr!("Servidores PACS", "PACS servers"), &servers, |ui| {
            if ui.button(trf!("{}  Gerenciar", "{}  Manage", icons::NETWORK)).clicked() {
                self.show_settings = false;
                self.pacs.open = true;
            }
        });
        setting_row(
            ui,
            tr!("Exames anteriores na linha do tempo", "Prior studies on the timeline"),
            tr!(
                "Ao abrir um exame, pergunta ao PACS selecionado pelos outros exames do mesmo paciente (mesmo ID e data de nascimento) e os mostra na linha do tempo da lista de exames.",
                "When a study opens, asks the selected PACS for the patient's other studies (same ID and date of birth) and shows them on the timeline of the study list."
            ),
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
            Some(_) => tr!("Nenhum exame baixado.", "No downloaded studies.").to_string(),
            None => tr!("Medindo…", "Measuring…").to_string(),
        };
        setting_row(ui, tr!("Neste computador agora", "On this computer now"), &usage, |ui| {
            let can = self.store_usage.is_some_and(|u| u.studies > 0);
            let label = trf!("{}  Apagar agora", "{}  Delete now", icons::TRASH);
            let button = egui::Button::new(RichText::new(label).color(if can { DANGER } else { pal().text_faint }));
            if ui.add_enabled(can, button).clicked() {
                self.confirm_clear_store = true;
            }
            if ui.button(trf!("{}  Abrir a pasta", "{}  Open the folder", icons::FOLDER_OPEN)).clicked() {
                let _ = std::fs::create_dir_all(&self.paths.store);
                let _ = std::process::Command::new("explorer").arg(&self.paths.store).spawn();
            }
        });
        let help = tr!("Exames baixados há mais tempo que isso são apagados ao abrir o Slyced.", "Studies downloaded longer ago than this are deleted when Slyced opens.");
        setting_block(ui, tr!("Apagar automaticamente", "Delete automatically"), help, |ui| {
            let mut days = self.settings.store_keep_days;
            let options = [(7, "", tr!("7 dias", "7 days")), (30, "", tr!("30 dias", "30 days")), (90, "", tr!("90 dias", "90 days")), (0, "", tr!("Nunca", "Never"))];
            if segmented(ui, &mut days, &options) {
                self.settings.store_keep_days = days;
                *changed = true;
                if days > 0 {
                    self.clean_store_now(Some(days));
                }
            }
        });
        let help = tr!("Para computadores compartilhados: nada fica no disco depois do uso.", "For shared computers: nothing stays on disk after use.");
        setting_row(ui, tr!("Apagar todos ao fechar o Slyced", "Delete all when Slyced closes"), help, |ui| {
            *changed |= toggle(ui, &mut self.settings.store_clear_on_exit).changed();
        });
        ui.label(
            RichText::new(trf!(
                "{}  Só as cópias baixadas do PACS são apagadas. Exames abertos de pastas, CDs ou pendrives nunca são tocados, e os exames abertos no momento ficam. Dá para baixá-los de novo do PACS quando precisar.",
                "{}  Only copies downloaded from the PACS are deleted. Studies opened from folders, CDs or USB drives are never touched, and the studies open right now stay. You can download them again from the PACS when needed.",
                icons::INFO
            ))
            .size(11.5)
            .color(pal().text_faint),
        );
    }

    fn settings_data(&mut self, ui: &mut Ui, portable_change: &mut Option<bool>) {
        let mode = if self.paths.portable {
            tr!("Portátil: ao lado do executável.", "Portable: next to the program file.")
        } else {
            tr!("Pasta do usuário do Windows: cada pessoa tem a sua.", "Windows user folder: each person has their own.")
        };
        setting_row(ui, tr!("Pasta de dados", "Data folder"), mode, |ui| {
            if ui.button(trf!("{}  Abrir", "{}  Open", icons::FOLDER_OPEN)).clicked() {
                let _ = std::process::Command::new("explorer").arg(&self.paths.root).spawn();
            }
        });
        ui.add(egui::Label::new(RichText::new(self.paths.root.display().to_string()).font(mono(11.5)).color(pal().text_dim)).wrap());
        row_divider(ui);
        setting_row(
            ui,
            tr!("Modo portátil", "Portable mode"),
            tr!(
                "Salva os dados junto ao executável, para levar o Slyced num pendrive. Vale na próxima vez que o Slyced for aberto.",
                "Saves data next to the program file, to carry Slyced on a USB drive. Takes effect the next time Slyced opens."
            ),
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
                ui.label(RichText::new(trf!("versão {}", "version {}", crate::config::APP_VERSION)).size(12.5).color(pal().text_faint));
            });
        });
        ui.add_space(12.0);
        setting_row(ui, tr!("Placa de vídeo", "Graphics card"), &self.adapter, |_| {});
        setting_row(ui, tr!("Novidades desta versão", "What's new in this version"), "", |ui| {
            if ui.button(trf!("{}  Ver", "{}  View", icons::ROCKET_LAUNCH)).clicked() {
                self.show_settings = false;
                self.show_news = true;
            }
        });
        setting_row(ui, tr!("Créditos e licenças", "Credits and licenses"), "", |ui| {
            if ui.button(trf!("{}  Sobre o Slyced", "{}  About Slyced", icons::INFO)).clicked() {
                self.show_about = true;
            }
        });
        super::credit::author_badge(ui, 28.0);
        ui.add_space(10.0);
        ui.label(
            RichText::new(tr!(
                "Software de visualização sem certificação como dispositivo médico. Não utilize como única base para diagnóstico.",
                "Viewing software not certified as a medical device. Do not use it as the sole basis for diagnosis."
            ))
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
        egui::Window::new(RichText::new(trf!("{}  Tags DICOM · {title}", "{}  DICOM tags · {title}", icons::TAG)).font(font_bold(15.0)))
            .id(egui::Id::unique("tags-window"))
            .open(&mut open)
            .default_size(vec2(760.0, 600.0))
            .collapsible(false)
            .show(ctx, |ui| {
                search_box(ui, &mut filter_text, tr!("Filtrar por nome, valor ou tag (ex.: 0010,0010)", "Filter by name, value or tag (e.g. 0010,0010)"));
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
                        for t in ["Tag", "VR", tr!("Nome", "Name"), tr!("Valor", "Value")] {
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
                                resp.on_hover_text(tr!("Clique para copiar", "Click to copy"));
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
        egui::Window::new(RichText::new(trf!("{}  Sobre o Slyced", "{}  About Slyced", icons::INFO)).font(font_bold(15.0)))
            .id(egui::Id::unique("about-window"))
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
                            RichText::new(trf!("Versão {} · Visualizador DICOM portátil", "Version {} · Portable DICOM viewer", crate::config::APP_VERSION))
                                .size(12.5)
                                .color(pal().text_dim),
                        );
                    });
                });
                ui.add_space(14.0);
                super::credit::author_card(ui);
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    // The English project page is at en/ below the Portuguese one.
                    let page = if crate::i18n::en() { format!("{}en/", crate::config::PROJECT_PAGE) } else { crate::config::PROJECT_PAGE.to_string() };
                    let text = trf!("{}  Conheça o Slyced: página do projeto  {}", "{}  Discover Slyced: project page  {}", icons::GLOBE, icons::ARROW_UP_RIGHT);
                    if ui.link(RichText::new(text).size(12.5)).on_hover_text(&page).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(page));
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(tr!(
                            "Feito com dicom-rs, egui, wgpu e Phosphor Icons (licenças MIT/Apache-2.0).",
                            "Made with dicom-rs, egui, wgpu and Phosphor Icons (MIT/Apache-2.0 licenses)."
                        ))
                            .size(11.5)
                            .color(pal().text_faint),
                    );
                    ui.label(
                        RichText::new(tr!(
                            "Sem certificação como dispositivo médico: não utilize como única base para diagnóstico.",
                            "Not certified as a medical device: do not use it as the sole basis for diagnosis."
                        ))
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
        egui::Window::new(RichText::new(trf!("{}  Atalhos", "{}  Shortcuts", icons::QUESTION)).font(font_bold(15.0)))
            .id(egui::Id::unique("help-window"))
            .open(&mut open)
            .default_width(520.0)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ghost_button(ui, icons::LIGHTBULB, tr!("Fazer o tour", "Take the tour")).clicked() {
                        self.start_tour();
                    }
                    if ghost_button(ui, icons::ROCKET_LAUNCH, tr!("Novidades desta versão", "What's new in this version")).clicked() {
                        self.show_news = true;
                    }
                });
                ui.add_space(8.0);
                let rows: &[(&str, &str)] = &[
                    (tr!("Roda do mouse", "Mouse wheel"), tr!("Navegar cortes", "Scroll through slices")),
                    (
                        tr!("Barra à direita da imagem", "Bar to the right of the image"),
                        tr!("Passar o mouse mostra o corte; clicar/arrastar vai até ele", "Hovering previews the slice; click or drag to go to it"),
                    ),
                    ("Ctrl+G", tr!("Ir para um corte (número) ou posição (mm)", "Go to a slice (number) or position (mm)")),
                    (tr!("Ctrl + roda", "Ctrl + wheel"), tr!("Zoom no cursor", "Zoom at the cursor")),
                    (tr!("Ctrl + arrastar (esquerdo)", "Ctrl + drag (left)"), tr!("Zoom (2D e 3D), sem precisar de roda", "Zoom (2D and 3D) without a wheel")),
                    ("+ / −", tr!("Aproximar / afastar a janela ativa", "Zoom in / out in the active viewport")),
                    (tr!("Botão direito (arrastar)", "Right button (drag)"), tr!("Janela / nível", "Window / level")),
                    (tr!("Botão do meio / Shift + esquerdo", "Middle button / Shift + left"), tr!("Mover imagem", "Pan image")),
                    (tr!("Duplo clique", "Double-click"), tr!("Maximizar / restaurar janela", "Maximize / restore viewport")),
                    ("↑ ↓  PgUp PgDn  Home End", tr!("Navegar cortes", "Scroll through slices")),
                    ("1 … 9, 0", tr!("Predefinições de janela (TC); 0 = gordura mesentérica", "Window presets (CT); 0 = mesenteric fat")),
                    ("W P Z S C", tr!("Janela, mover, zoom, navegar, mira MPR", "Window, pan, zoom, scroll, MPR crosshair")),
                    (tr!("Ctrl + clique  ·  L", "Ctrl + click  ·  L"), tr!("Cursor 3D: leva todas as séries e planos ao mesmo ponto", "3D cursor: brings every series and plane to the same point")),
                    (tr!("MPR: alças da mira (C)", "MPR: crosshair handles (C)"), tr!("Inclinar os planos (MPR oblíquo)", "Tilt the planes (oblique MPR)")),
                    ("D A E H T", tr!("Distância, ângulo, ROI elíptica, ponto, seta com texto", "Distance, angle, elliptical ROI, point, arrow with text")),
                    ("B", tr!("Marcar achado (lista em Achados, reabre no mesmo ponto)", "Mark a finding (listed under Findings, reopens at the same point)")),
                    ("M", tr!("Lupa", "Magnifier")),
                    (
                        "V",
                        tr!(
                            "Rastreador tubular (CPR) no 2D ou MPR: cliques ao longo da estrutura; direito/Enter conclui, Backspace desfaz, roda gira a imagem esticada",
                            "Tube tracker (CPR) in 2D or MPR: click along the structure; right-click/Enter finishes, Backspace undoes, the wheel rotates the straightened image"
                        ),
                    ),
                    (tr!("Realces (barra superior)", "Highlights (top bar)"), tr!("Gordura mesentérica, Gás livre e Cálculos", "Mesenteric fat, Free air and Stones")),
                    (
                        tr!("H na comparação de fases", "H in phase comparison"),
                        tr!("ΔHU entre as fases sem e com contraste no mesmo ponto", "ΔHU between the non-contrast and contrast phases at the same point"),
                    ),
                    (tr!("G (Comparar)", "G (Compare)"), tr!("Pontos de referência para alinhar dois exames", "Landmarks to align two studies")),
                    ("Ctrl+Z  ·  Ctrl+Y", tr!("Desfazer  ·  refazer (medidas, realces, janela, cursor, planos)", "Undo  ·  redo (measurements, highlights, window, cursor, planes)")),
                    (
                        tr!("Botão direito (clique)", "Right button (click)"),
                        tr!(
                            "Menu com o que dá para fazer naquele ponto (arrastar continua ajustando a janela)",
                            "Menu of what can be done at that point (dragging still adjusts the window)"
                        ),
                    ),
                    (tr!("← / →  ·  botões laterais do mouse", "← / →  ·  mouse side buttons"), tr!("Série anterior / seguinte do exame", "Previous / next series of the study")),
                    (
                        "Alt+← / Alt+→",
                        tr!(
                            "Voltar / avançar para onde você estava (antes de pular para um achado, cálculo ou outra série)",
                            "Back / forward to where you were (before jumping to a finding, a stone or another series)"
                        ),
                    ),
                    (
                        "Tab",
                        tr!(
                            "Modo foco: esconde os painéis laterais (na tela cheia de leitura, abre e fecha a lista de exames)",
                            "Focus mode: hides the side panels (in reading full screen, opens and closes the study list)"
                        ),
                    ),
                    (
                        "F11",
                        tr!(
                            "Tela cheia de leitura: só as imagens; mexa o mouse para os controles, encoste à esquerda para os exames",
                            "Reading full screen: images only; move the mouse for the controls, touch the left edge for the studies"
                        ),
                    ),
                    (
                        "Ctrl+I",
                        tr!(
                            "Informações sobre a imagem: completa, mínima ou nenhuma (na tela cheia); liga e desliga (tela normal)",
                            "Image information: full, minimal or none (full screen); on and off (normal screen)"
                        ),
                    ),
                    ("Ctrl+C", tr!("Copiar a imagem da janela ativa", "Copy the image of the active viewport")),
                    ("Del  ·  Shift+Del", tr!("Remover a última medição  ·  apagar todas as medidas da tela", "Remove the last measurement  ·  delete every measurement on screen")),
                    (tr!("Espaço", "Space"), "Cine"),
                    ("I / R", tr!("Inverter / restaurar visualização", "Invert / reset view")),
                    (
                        tr!("Shift+R  ·  botão Restaurar na barra", "Shift+R  ·  Reset button on the bar"),
                        tr!(
                            "Restaurar tudo: medidas, realces, filtros, cursor e planos voltam ao padrão (achados salvos ficam)",
                            "Reset all: measurements, highlights, filters, cursor and planes go back to default (saved findings stay)"
                        ),
                    ),
                    (
                        "Esc",
                        tr!(
                            "Fechar \"Ir para\" ou a lista de exames da tela cheia / sair da tela cheia / apagar o cursor 3D",
                            "Close \"Go to\" or the full-screen study list / leave full screen / clear the 3D cursor"
                        ),
                    ),
                    (
                        tr!("Ctrl+W  ·  X no canto da janela", "Ctrl+W  ·  X in the viewport corner"),
                        tr!(
                            "Fechar a série da janela (no MPR/3D volta ao 2D; na comparação, fecha o lado)",
                            "Close the viewport's series (MPR/3D goes back to 2D; in comparison, closes that side)"
                        ),
                    ),
                    ("H", tr!("Densidade pontual, com legenda dos valores típicos de cada tecido", "Point density (HU), with a legend of typical values for each tissue")),
                    ("Ctrl+O / Ctrl+Shift+O", tr!("Abrir pasta / arquivos", "Open folder / files")),
                    (tr!("3D: esquerdo / direito / roda", "3D: left / right / wheel"), tr!("Girar / ajustar função de transferência / zoom", "Rotate / adjust transfer function / zoom")),
                    (tr!("3D: Shift + esquerdo", "3D: Shift + left"), tr!("Mover (mouse sem botão do meio)", "Pan (mouse without a middle button)")),
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
