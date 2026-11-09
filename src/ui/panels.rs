//! Main window chrome: top bar, study browser, inspector, status bar.

use super::icons;
use super::theme::*;
use super::viewport::{Content, MeasureKind, SeriesDrag, Tool};
use super::widgets::*;
use crate::app::{Layout, SlycedApp, Target, ToastAction, ToastKind, Workspace};
use crate::dicom::model::{format_date, format_person_name};
use crate::imaging::lut::{Colormap, Window, CT_PRESETS};
use crate::imaging::reslice::SlabMode;
use crate::render3d::camera::ViewPreset;
use crate::render3d::renderer::RenderMode;
use crate::render3d::tf::{TfAdjust, PRESETS};
use egui::{
    pos2, vec2, Align, Align2, Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui,
};
use std::sync::atomic::Ordering;

fn panel_frame() -> Frame {
    Frame::new().fill(pal().bg_panel).inner_margin(Margin::same(12))
}

fn vsep(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(9.0, 26.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0, pal().border));
}

pub fn logo(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    paint_logo(ui.painter(), rect);
}

/// The logo: three stacked slices (as in `logo_raster`, which draws the window and file icons).
pub fn paint_logo(p: &egui::Painter, rect: Rect) {
    use crate::logo_raster::{SLICES, SLICE_GAP, SLICE_HALF};
    let r = (rect.width() * 0.28) as u8;
    let (a, b) = (pal().accent, pal().accent_2);
    gradient_rect(p, rect, r, a, b);
    let k = rect.width() / 64.0;
    let diamond = |(cx, cy): (f32, f32)| {
        let (hw, hh) = SLICE_HALF;
        [(cx, cy - hh), (cx + hw, cy), (cx, cy + hh), (cx - hw, cy)].map(|(x, y)| rect.min + vec2(x, y) * k).to_vec()
    };
    // The gap around the upper slices takes the background color (middle of the gradient).
    let gap = Stroke::new(SLICE_GAP * 2.0 * k, lerp_color(a, b, 0.5));
    p.add(egui::Shape::convex_polygon(diamond(SLICES[2]), Color32::from_white_alpha(115), Stroke::NONE));
    p.add(egui::Shape::convex_polygon(diamond(SLICES[1]), Color32::from_rgb(207, 196, 255), gap));
    p.add(egui::Shape::convex_polygon(diamond(SLICES[0]), Color32::WHITE, gap));
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

impl SlycedApp {
    pub fn top_bar(&mut self, ui: &mut Ui) {
        egui::Panel::top("topbar")
            .exact_size(54.0)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_panel).inner_margin(Margin::symmetric(12, 0)))
            .show(ui, |ui| {
                let bar = ui.max_rect();
                ui.painter().hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, pal().border));
                // Narrow windows: text buttons become icons when they do not fit.
                let compact = bar.width() < self.topbar_full;
                let top_button = |ui: &mut Ui, icon: &str, text: &str, tip: &str| {
                    if compact {
                        icon_button(ui, icon, tip, false)
                    } else {
                        ghost_button(ui, icon, text).on_hover_text(tip)
                    }
                };
                let needed = ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    logo(ui, 28.0);
                    ui.add_space(4.0);
                    if bar.width() >= 1400.0 {
                        ui.vertical(|ui| {
                            ui.add_space(9.0);
                            ui.label(RichText::new("Slyced").font(font_bold(15.5)).color(pal().text));
                            ui.add_space(-6.0);
                            ui.label(RichText::new("DICOM VIEWER").font(font_bold(8.5)).color(pal().text_faint).extra_letter_spacing(1.2));
                        });
                    }
                    ui.add_space(10.0);
                    vsep(ui);

                    let open_x = ui.cursor().left();
                    if compact {
                        // Narrow bar: one icon each (the recent list is in the exams panel menu).
                        if icon_button(ui, icons::FOLDER_OPEN, "Abrir pasta com exames DICOM (Ctrl+O)", false).clicked() {
                            self.open_folder_dialog();
                        }
                        if icon_button(ui, icons::FILES, "Abrir arquivos DICOM, ZIP ou DICOMDIR (Ctrl+Shift+O)", false).clicked() {
                            self.open_files_dialog();
                        }
                    } else {
                        // Wide bar: "Abrir" opens a folder; its arrow shows files and recent exams.
                        let (main, arrow) = split_button(ui, icons::FOLDER_OPEN, "Abrir");
                        if main.on_hover_text("Abrir pasta com exames DICOM (Ctrl+O)").clicked() {
                            self.open_folder_dialog();
                        }
                        self.open_requested_menu(ui, "open", &arrow);
                        egui::Popup::menu(&arrow).width(290.0).show(|ui| self.open_menu(ui));
                    }
                    let row = bar.y_range().shrink(10.0);
                    self.tour_targets[0] = Rect::from_x_y_ranges(open_x..=ui.cursor().left() - 4.0, row);
                    let pacs = top_button(ui, icons::NETWORK, "PACS", "Pesquisar e baixar exames do PACS");
                    self.tour_targets[1] = pacs.rect;
                    if pacs.clicked() {
                        self.pacs.open = true;
                    }
                    let protocols = top_button(ui, icons::CLIPBOARD_TEXT, "Protocolos", "Protocolos de leitura");
                    egui::Popup::menu(&protocols).width(280.0).show(|ui| self.protocols_menu(ui));
                    vsep(ui);

                    let is3d = self.workspace == Workspace::Volume;
                    self.tour_targets[2] = Rect::NOTHING;
                    if !is3d {
                        let tools_x = ui.cursor().left();
                        self.tool_groups_bar(ui, compact);
                        self.tour_targets[2] = Rect::from_x_y_ranges(tools_x..=ui.cursor().left() - 4.0, row);
                        vsep(ui);
                        let inverted = self.active_view().is_some_and(|v| v.invert);
                        let transform = icon_button(ui, icons::FLIP_HORIZONTAL, "Girar, espelhar e inverter", inverted);
                        egui::Popup::menu(&transform).width(220.0).show(|ui| {
                            let Some(v) = self.active_view_mut() else { return };
                            if ui.button(format!("{}  Girar 90° à direita", icons::ARROW_CLOCKWISE)).clicked() {
                                v.rot = (v.rot + 1) % 4;
                            }
                            if ui.button(format!("{}  Girar 90° à esquerda", icons::ARROW_COUNTER_CLOCKWISE)).clicked() {
                                v.rot = (v.rot + 3) % 4;
                            }
                            if ui.button(format!("{}  Espelhar horizontal", icons::FLIP_HORIZONTAL)).clicked() {
                                v.flip_h = !v.flip_h;
                            }
                            if ui.button(format!("{}  Espelhar vertical", icons::FLIP_VERTICAL)).clicked() {
                                v.flip_v = !v.flip_v;
                            }
                            if ui.button(format!("{}  Inverter cores (I)", icons::DROP_HALF)).clicked() {
                                v.invert = !v.invert;
                            }
                        });
                        let playing = self.active_view().is_some_and(|v| v.cine);
                        let (icon, tip) = if playing { (icons::PAUSE, "Pausar cine (Espaço)") } else { (icons::PLAY, "Cine (Espaço)") };
                        if self.workspace == Workspace::Viewer && icon_button(ui, icon, tip, playing).clicked()
                            && let Some(v) = self.active_view_mut() {
                                v.cine = !v.cine;
                            }
                    } else {
                        ui.label(
                            RichText::new("Esquerdo: girar · Direito: ajustar · Shift/meio: mover · Roda, Ctrl+arrastar ou +/−: zoom")
                                .color(pal().text_faint)
                                .size(12.0),
                        );
                    }
                    let undo_x = ui.cursor().left();
                    ui.add_enabled_ui(self.history.can_undo(), |ui| {
                        if icon_button(ui, icons::ARROW_U_UP_LEFT, "Desfazer (Ctrl+Z)", false).clicked() {
                            self.undo();
                        }
                    });
                    ui.add_enabled_ui(self.history.can_redo(), |ui| {
                        if icon_button(ui, icons::ARROW_U_UP_RIGHT, "Refazer (Ctrl+Y)", false).clicked() {
                            self.redo();
                        }
                    });
                    let reset_tip = "Restaurar: visualização, medidas, realces… (R, Shift+R)";
                    let reset = if compact {
                        icon_button(ui, icons::ARROW_COUNTER_CLOCKWISE, reset_tip, false)
                    } else {
                        ghost_button(ui, icons::ARROW_COUNTER_CLOCKWISE, &format!("Restaurar  {}", icons::CARET_DOWN)).on_hover_text(reset_tip)
                    };
                    count_badge(ui, reset.rect, self.screen_changes().len());
                    self.tour_targets[3] = Rect::from_x_y_ranges(undo_x..=reset.rect.right(), row);
                    self.open_requested_menu(ui, "reset", &reset);
                    egui::Popup::menu(&reset).width(340.0).show(|ui| self.reset_menu(ui));
                    if icon_button(ui, icons::CAMERA, "Salvar imagem (PNG)", false).clicked() {
                        let ctx = ui.ctx().clone();
                        self.export_active(&ctx);
                    }

                    let left_end = ui.min_rect().right();
                    let right_width = ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let gear = icon_button(ui, icons::GEAR_SIX, "Configurações", self.show_settings);
                        self.tour_targets[4] = gear.rect;
                        if gear.clicked() {
                            self.show_settings = !self.show_settings;
                        }
                        if icon_button(ui, icons::QUESTION, "Atalhos (F1)", self.show_help).clicked() {
                            self.show_help = !self.show_help;
                        }
                        if icon_button(ui, icons::SIDEBAR_SIMPLE, "Painel lateral", self.right_open).clicked() {
                            self.right_open = !self.right_open;
                        }
                        self.view_mode_buttons(ui);
                        vsep(ui);
                        let mut ws = self.workspace;
                        if segmented(
                            ui,
                            &mut ws,
                            &if compact {
                                [(Workspace::Viewer, "", "2D"), (Workspace::Mpr, "", "MPR"), (Workspace::Volume, "", "3D"), (Workspace::Compare, icons::COLUMNS, "A|B")]
                            } else {
                                [
                                    (Workspace::Viewer, icons::IMAGE, "2D"),
                                    (Workspace::Mpr, icons::SQUARES_FOUR, "MPR"),
                                    (Workspace::Volume, icons::CUBE, "3D"),
                                    (Workspace::Compare, icons::COLUMNS, "Comparar"),
                                ]
                            },
                        ) {
                            self.switch_workspace(ws);
                        }
                        if self.workspace == Workspace::Viewer {
                            ui.add_space(6.0);
                            if self.layout.count() > 1
                                && icon_button(ui, icons::LINK, "Sincronizar navegação entre janelas do mesmo estudo", self.sync_scroll).clicked()
                            {
                                self.sync_scroll = !self.sync_scroll;
                            }
                            let resp = icon_button(ui, layout_icon(self.layout), "Layout", false);
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
                        ui.min_rect().width()
                    });
                    (left_end - bar.left()) + right_width.inner + 24.0
                });
                if !compact {
                    self.topbar_full = needed.inner;
                    if bar.width() < needed.inner {
                        ui.ctx().request_repaint();
                    }
                }
            });
    }

    /// Menu of the "Abrir" arrow (wide top bar): folder, files and recent exams.
    fn open_menu(&mut self, ui: &mut Ui) {
        let item = |ui: &mut Ui, icon: &str, text: &str, keys: &str| {
            ui.add(egui::Button::new(format!("{icon}  {text}")).shortcut_text(keys).min_size(vec2(280.0, 28.0))).clicked()
        };
        if item(ui, icons::FOLDER_OPEN, "Pasta…", "Ctrl+O") {
            self.open_folder_dialog();
            ui.close();
        }
        if item(ui, icons::FILES, "Arquivos ou ZIP…", "Ctrl+Shift+O") {
            self.open_files_dialog();
            ui.close();
        }
        if !self.settings.recent.is_empty() {
            ui.separator();
            ui.label(RichText::new("Recentes").small().color(pal().text_faint));
            for r in self.settings.recent.clone() {
                if recent_button(ui, &r).clicked() {
                    self.open_paths(vec![r], true);
                    ui.close();
                }
            }
        }
    }

    /// Tools of each toolbar group in the current workspace.
    fn tool_groups(&self) -> [(&'static str, &'static str, Vec<Tool>); 3] {
        let planes = self.workspace == Workspace::Mpr || (self.workspace == Workspace::Compare && self.compare_is_3d());
        let nav = Tool::NAV.into_iter().filter(|t| *t != Tool::Crosshair || planes).collect();
        let mut reading = Tool::FINDINGS.to_vec();
        match self.workspace {
            Workspace::Mpr | Workspace::Viewer => reading.push(Tool::Curve),
            Workspace::Compare => reading.push(Tool::Landmark),
            Workspace::Volume => {}
        }
        [("Navegar", "nav", nav), ("Medir", "measure", Tool::MEASURE.to_vec()), ("Leitura", "reading", reading)]
    }

    /// Navigation tools one by one, then Medir and Leitura: the main part picks the group's last tool,
    /// the arrow lists the others (and, in Leitura, the reading highlights).
    fn tool_groups_bar(&mut self, ui: &mut Ui, compact: bool) {
        for (g, (name, key, tools)) in self.tool_groups().into_iter().enumerate() {
            // Navigation tools are used all the time: one button each, always in view.
            if g == 0 {
                for t in &tools {
                    if icon_button(ui, t.icon(), t.label(), self.tool == *t).clicked() {
                        self.tool = *t;
                    }
                }
                vsep(ui);
                continue;
            }
            if tools.contains(&self.tool) {
                self.group_tool[g] = self.tool;
            }
            let shown = if tools.contains(&self.group_tool[g]) { self.group_tool[g] } else { tools[0] };
            let (main, arrow) = tool_split(ui, ("group", g), shown.icon(), (!compact).then_some(name), tools.contains(&self.tool));
            if main.on_hover_text(format!("{name}: {} ({}) — a seta mostra as outras ferramentas", shown.name(), shown.key())).clicked() {
                self.tool = shown;
            }
            self.open_requested_menu(ui, key, &arrow);
            egui::Popup::menu(&arrow).width(320.0).show(|ui| {
                ui.label(RichText::new(name).small().color(pal().text_faint));
                for t in &tools {
                    let button = egui::Button::new(format!("{}  {}", t.icon(), t.name())).shortcut_text(t.key()).selected(self.tool == *t).min_size(vec2(300.0, 28.0));
                    if ui.add(button).on_hover_text(t.hint()).clicked() {
                        self.tool = *t;
                        ui.close();
                    }
                }
                if g == 1 {
                    ui.separator();
                    let button = egui::Button::new(format!("{}  Volume por 3 diâmetros", icons::SPHERE)).min_size(vec2(300.0, 28.0));
                    if ui.add(button).on_hover_text("Elipsoide: meça largura, profundidade e altura; mostra o volume em mL e a densidade média").clicked() {
                        self.open_ellipsoid();
                        ui.close();
                    }
                }
                if g == 2 && self.workspace != Workspace::Volume {
                    ui.separator();
                    self.specials_menu(ui);
                }
            });
            ui.add_space(2.0);
        }
    }

    /// Opens the menu of `button` when automation asked for it.
    fn open_requested_menu(&mut self, ui: &Ui, name: &str, button: &egui::Response) {
        if self.open_menu.as_deref() == Some(name) {
            egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(button));
            self.open_menu = None;
        }
    }

    pub fn status_bar(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("statusbar")
            .exact_size(26.0)
            .show_separator_line(false)
            .frame(Frame::new().fill(pal().bg_app).inner_margin(Margin::symmetric(12, 0)))
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0, pal().border));
                ui.horizontal_centered(|ui| {
                    let small = |t: &str, c: Color32| RichText::new(t).size(11.5).color(c);
                    if let Some(scan) = &self.scan {
                        let (found, parsed) = (scan.found.load(Ordering::Relaxed), scan.parsed.load(Ordering::Relaxed));
                        let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        spinner(ui.painter(), rect.center(), 5.0, self.time, pal().accent);
                        ui.label(small(&format!("Lendo arquivos… {parsed} / {found}"), pal().text_dim));
                    } else {
                        // Pixel readout in a fixed slot, then how to use the active tool.
                        let start = ui.cursor().left();
                        if !self.status.is_empty() {
                            ui.label(small(&self.status, pal().text_dim));
                        }
                        let used = ui.cursor().left() - start;
                        let reading = self.workspace != Workspace::Volume && !self.library.studies.is_empty();
                        if reading {
                            ui.add_space((210.0 - used).max(12.0));
                            let t = self.tool;
                            ui.label(small(t.icon(), pal().accent));
                            ui.label(RichText::new(t.name()).size(11.5).color(pal().text));
                            if r.width() > 1300.0 {
                                ui.label(small(t.hint(), pal().text_faint));
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        super::credit::author_badge(ui, 20.0);
                        let version = ui
                            .add(egui::Label::new(small(&format!("v{}", crate::config::APP_VERSION), pal().text_faint)).sense(Sense::click()))
                            .on_hover_text("Sobre o Slyced");
                        if version.clicked() {
                            self.show_about = true;
                        }
                        ui.label(small(&format!("{}  {}", icons::CPU, self.adapter), pal().text_faint));
                        self.downloads_indicator(ui);
                        let mem = self.memory_used() as f64 / (1024.0 * 1024.0 * 1024.0);
                        ui.label(small(&format!("{}  {:.2} GB", icons::HARD_DRIVES, mem), pal().text_faint));
                        if let Some(scp) = &self.scp {
                            ui.label(small(&format!("● SCP {}:{}", scp.ae_title, scp.port), SUCCESS));
                        }
                        if self.paths.portable {
                            ui.label(small("modo portátil", pal().text_faint)).on_hover_text(format!("Dados em {}", self.paths.root.display()));
                        }
                    });
                });
            });
    }

    pub fn browser_panel(&mut self, ui: &mut Ui) {
        egui::Panel::left("browser")
            .default_size(320.0)
            .size_range(260.0..=520.0)
            .resizable(true)
            .show_separator_line(true)
            .frame(panel_frame())
            .show(ui, |ui| self.browser_contents(ui));
    }

    /// Exams of the open patients and their series (left panel, and the
    /// drawer of the reading full screen).
    pub(crate) fn browser_contents(&mut self, ui: &mut Ui) {
        let mut action: Option<BrowserAction> = None;
        let mut timeline_action = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Exames").font(font_bold(15.0)).color(pal().text));
            if !self.library.studies.is_empty() {
                badge(ui, &self.library.studies.len().to_string(), pal().text_dim);
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let menu = icon_button_sized(ui, icons::DOTS_THREE, "Mais opções", false, 28.0);
                self.open_requested_menu(ui, "exams", &menu);
                egui::Popup::menu(&menu).width(250.0).show(|ui| {
                    if ui.button(format!("{}  Expandir todos", icons::CARET_DOWN)).clicked() {
                        action = Some(BrowserAction::ExpandAll);
                    }
                    if ui.button(format!("{}  Recolher todos", icons::CARET_RIGHT)).clicked() {
                        action = Some(BrowserAction::CollapseAll);
                    }
                    if !self.settings.recent.is_empty() {
                        ui.separator();
                        ui.label(RichText::new("Abertos recentemente").small().color(pal().text_faint));
                        for r in &self.settings.recent {
                            if recent_button(ui, r).clicked() {
                                action = Some(BrowserAction::OpenPath(r.clone()));
                            }
                        }
                    }
                    ui.separator();
                    if ui.button(format!("{}  Fechar todos os exames", icons::X_CIRCLE)).clicked() {
                        action = Some(BrowserAction::CloseAll);
                    }
                });
                let (icon, tip) = if self.browser_grid { (icons::LIST_BULLETS, "Mostrar séries em lista") } else { (icons::SQUARES_FOUR, "Mostrar séries em grade") };
                if icon_button_sized(ui, icon, tip, false, 28.0).clicked() {
                    self.browser_grid = !self.browser_grid;
                }
                if icon_button_sized(ui, icons::FILES, "Abrir arquivo (Ctrl+Shift+O)", false, 28.0).clicked() {
                    self.open_files_dialog();
                }
                if icon_button_sized(ui, icons::FOLDER_OPEN, "Abrir pasta (Ctrl+O)", false, 28.0).clicked() {
                    self.open_folder_dialog();
                }
            });
        });
        ui.add_space(6.0);
        search_box(ui, &mut self.search, "Buscar paciente, ID, descrição…");
        ui.add_space(8.0);

        let q = self.search.trim().to_lowercase();
        let active = self.active_series();
        let active_uid = active.as_ref().map(|s| s.uid.clone());
        let active_study = active.as_ref().map(|s| s.study.uid.clone());
        let target_for_click = match self.workspace {
            Workspace::Viewer => Target::View(self.active),
            Workspace::Mpr => Target::Mpr,
            Workspace::Volume => Target::Volume,
            Workspace::Compare => Target::Compare(self.active.min(1)),
        };
        let shown = self.shown_series();

        // Studies grouped by patient, newest first (the library is sorted by date).
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for (i, st) in self.library.studies.iter().enumerate() {
            let h = &st.header;
            if !q.is_empty() {
                let hay = format!("{} {} {} {} {}", h.patient_name, h.patient_id, h.description, h.accession, st.modalities().join(" ")).to_lowercase();
                if !hay.contains(&q) {
                    continue;
                }
            }
            let key = if h.patient_id.trim().is_empty() { h.patient_name.clone() } else { h.patient_id.clone() };
            match groups.iter_mut().find(|g| g.0 == key) {
                Some(g) => g.1.push(i),
                None => groups.push((key, vec![i])),
            }
        }

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.library.studies.is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(icons::FOLDER_OPEN).size(30.0).color(pal().text_faint));
                    ui.label(RichText::new("Nenhum exame aberto").color(pal().text_dim));
                    ui.add_space(8.0);
                    if ghost_button(ui, icons::FOLDER_OPEN, "Abrir pasta").clicked() {
                        self.open_folder_dialog();
                    }
                    ui.add_space(4.0);
                    if ghost_button(ui, icons::FILES, "Abrir arquivo").clicked() {
                        self.open_files_dialog();
                    }
                    ui.add_space(4.0);
                    if ghost_button(ui, icons::NETWORK, "Buscar no PACS").clicked() {
                        self.pacs.open = true;
                    }
                });
            } else if groups.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(format!("Nada encontrado para \"{}\"", self.search.trim())).color(pal().text_faint));
                });
            }
            for (gi, (_, studies)) in groups.iter().enumerate() {
                if gi > 0 {
                    ui.add_space(10.0);
                }
                // Patient timeline (their exams here and on the PACS), on the first card;
                // the dots are compared with the exam being read when it is this patient's.
                let uid_of = |i: &usize| self.library.studies[*i].header.uid.clone();
                let reading_this = studies.iter().map(uid_of).find(|u| active_study.as_deref() == Some(u.as_str()));
                let current = reading_this.clone().or_else(|| studies.first().map(uid_of)).unwrap_or_default();
                let (items, pending) = self.timeline_items(studies, &current);
                let timeline = (items.len() >= 2).then_some((items, pending, reading_this.is_some()));
                for (k, &si) in studies.iter().enumerate() {
                    let st = &self.library.studies[si];
                    let uid = st.header.uid.clone();
                    let expanded = self.expanded.contains(&uid);
                    let is_active = active_study.as_deref() == Some(uid.as_str());
                    // Prior exams of the same patient are labelled to ease comparison.
                    let order = (k > 0).then_some("anterior");
                    let line_h = if k == 0 && timeline.is_some() { 26.0 } else { 0.0 };
                    let (resp, close, line_rect) = study_card(ui, st, (k == 0).then_some(studies.len()), expanded, is_active, order, line_h);
                    if let (0, Some((items, pending, can_compare))) = (k, &timeline)
                        && let Some(a) = crate::ui::timeline::timeline_strip(ui, line_rect, items, *pending, *can_compare, self.time) {
                            timeline_action = Some(a);
                        }
                    if close {
                        action = Some(BrowserAction::Close(uid.clone()));
                    } else if resp.clicked() {
                        action = Some(BrowserAction::Toggle(uid.clone()));
                    }
                    resp.context_menu(|ui| {
                        if active_study.as_deref().is_some_and(|a| a != uid) && ui.button(format!("{}  Comparar com o exame aberto", icons::COLUMNS)).clicked() {
                            action = Some(BrowserAction::Compare(uid.clone()));
                        }
                        if ui.button(format!("{}  Fechar exame", icons::X)).clicked() {
                            action = Some(BrowserAction::Close(uid.clone()));
                        }
                    });
                    if !expanded {
                        ui.add_space(4.0);
                        continue;
                    }
                    ui.add_space(2.0);
                    let avail = ui.available_width();
                    let series_clicked = |action: &mut Option<BrowserAction>, resp: &egui::Response, s: &crate::dicom::model::SeriesInfo| {
                        if resp.drag_started() || resp.dragged() {
                            resp.dnd_set_drag_payload(SeriesDrag(s.uid.clone()));
                        }
                        if resp.clicked() {
                            *action = Some(BrowserAction::Open(s.uid.clone(), target_for_click));
                        }
                        resp.context_menu(|ui| {
                            if ui.button(format!("{}  Abrir no visualizador 2D", icons::IMAGE)).clicked() {
                                *action = Some(BrowserAction::Open(s.uid.clone(), Target::View(self.active.min(5))));
                            }
                            if ui.button(format!("{}  Abrir em MPR", icons::SQUARES_FOUR)).clicked() {
                                *action = Some(BrowserAction::Open(s.uid.clone(), Target::Mpr));
                            }
                            if ui.button(format!("{}  Abrir em 3D", icons::CUBE)).clicked() {
                                *action = Some(BrowserAction::Open(s.uid.clone(), Target::Volume));
                            }
                            ui.separator();
                            if ui.button(format!("{}  Comparar como exame A", icons::COLUMNS)).clicked() {
                                *action = Some(BrowserAction::Open(s.uid.clone(), Target::Compare(0)));
                            }
                            if ui.button(format!("{}  Comparar como exame B", icons::COLUMNS)).clicked() {
                                *action = Some(BrowserAction::Open(s.uid.clone(), Target::Compare(1)));
                            }
                        });
                    };
                    if self.browser_grid {
                        let cols = if avail > 330.0 { 3 } else { 2 };
                        let gap = 6.0;
                        let cw = (avail - gap * (cols - 1) as f32) / cols as f32;
                        for chunk in st.series.chunks(cols) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = gap;
                                for s in chunk {
                                    let loading = self.loading.get(&s.uid).map(|p| p.done.load(Ordering::Relaxed) as f32 / p.total.max(1) as f32);
                                    let thumb = self.thumbs.get(&s.uid).map(|t| t.id());
                                    let resp = series_card(ui, s, thumb, cw, active_uid.as_deref() == Some(&s.uid), loading, self.time);
                                    series_clicked(&mut action, &resp, s);
                                }
                            });
                            ui.add_space(gap);
                        }
                    } else {
                        for s in &st.series {
                            let loading = self.loading.get(&s.uid).map(|p| p.done.load(Ordering::Relaxed) as f32 / p.total.max(1) as f32);
                            let thumb = self.thumbs.get(&s.uid).map(|t| t.id());
                            let places = shown.get(&s.uid).cloned().unwrap_or_default();
                            let resp = series_row(ui, s, thumb, avail, &places, active_uid.as_deref() == Some(&s.uid), loading, self.time);
                            series_clicked(&mut action, &resp, s);
                        }
                    }
                    ui.add_space(8.0);
                }
            }
        });

        let opened = matches!(action, Some(BrowserAction::Open(..)) | Some(BrowserAction::Compare(_))) || timeline_action.is_some();
        match action {
            Some(BrowserAction::Toggle(uid)) => {
                if !self.expanded.remove(&uid) {
                    self.expanded.insert(uid.clone());
                    self.expanded_study = Some(uid);
                }
            }
            Some(BrowserAction::Close(uid)) => {
                let name = self.library.studies.iter().find(|s| s.header.uid == uid).map(|s| format_date(&s.header.date)).unwrap_or_default();
                self.close_study(&uid);
                self.toast_action(ToastKind::Info, format!("Exame de {name} fechado."), ToastAction::ReopenClosed);
            }
            Some(BrowserAction::CloseAll) => self.confirm_close_all = true,
            Some(BrowserAction::ExpandAll) => self.expanded = self.library.studies.iter().map(|s| s.header.uid.clone()).collect(),
            Some(BrowserAction::CollapseAll) => self.expanded.clear(),
            Some(BrowserAction::Compare(uid)) => self.compare_with_study(&uid),
            Some(BrowserAction::OpenPath(path)) => self.open_paths(vec![path], true),
            Some(BrowserAction::Open(uid, target)) => {
                if matches!(target, Target::View(_)) && self.workspace != Workspace::Viewer {
                    self.workspace = Workspace::Viewer;
                }
                self.open_series(&uid, target);
            }
            None => {}
        }
        if let Some(a) = timeline_action {
            self.timeline_action(a);
        }
        // In the reading full screen the list gets out of the way once something opens.
        if self.reading && opened {
            self.pin_drawer(false);
        }
        // Floating label while dragging a series.
        if let Some(p) = egui::DragAndDrop::payload::<SeriesDrag>(ui.ctx())
            && let Some(pos) = ui.ctx().pointer_latest_pos() {
                let title = self.library.find_series(&p.0).map(|(_, s)| s.title()).unwrap_or_default();
                egui::Area::new(egui::Id::unique("drag-preview")).fixed_pos(pos + vec2(14.0, 10.0)).order(egui::Order::Tooltip).show(ui.ctx(), |ui| {
                    Frame::new().fill(pal().bg_active).corner_radius(8).inner_margin(Margin::symmetric(10, 6)).stroke(Stroke::new(1.0, pal().accent)).show(ui, |ui| {
                        ui.label(RichText::new(format!("{}  {}", icons::IMAGE, title)).color(pal().text));
                    });
                });
            }
    }

    /// Where each series is open: "2D", "2D·2", "MPR", "3D", "A", "B".
    fn shown_series(&self) -> std::collections::HashMap<String, Vec<String>> {
        let mut map: std::collections::HashMap<String, Vec<String>> = Default::default();
        let mut add = |c: &Content, label: String| {
            if let Some(s) = c.series() {
                let v = map.entry(s.uid.clone()).or_default();
                if !v.contains(&label) {
                    v.push(label);
                }
            }
        };
        let n = self.layout.count();
        for (i, v) in self.views.iter().take(n).enumerate() {
            add(&v.content, if n == 1 { "2D".into() } else { format!("2D·{}", i + 1) });
        }
        add(&self.mpr_views[0].content, "MPR".into());
        add(&self.vr_view.content, "3D".into());
        add(&self.cmp_views[0].content, "A".into());
        add(&self.cmp_views[1].content, "B".into());
        map
    }

    pub fn inspector_panel(&mut self, ui: &mut Ui) {
        egui::Panel::right("inspector")
            .default_size(292.0)
            .size_range(250.0..=420.0)
            .resizable(true)
            .frame(panel_frame())
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let Some(series) = self.active_series() else {
                        ui.add_space(20.0);
                        ui.label(RichText::new("Selecione uma série para ver os controles.").color(pal().text_faint));
                        return;
                    };
                    // Header
                    ui.horizontal(|ui| {
                        badge(ui, &series.info.modality, modality_color(&series.info.modality));
                        ui.label(RichText::new(series.info.title()).font(font_bold(14.0)).color(pal().text));
                    });
                    ui.label(RichText::new(format_person_name(&series.study.patient_name)).color(pal().text_dim).size(12.0));
                    ui.add_space(8.0);

                    let is3d = matches!(self.active_view().map(|v| &v.content), Some(Content::Volume(_)));
                    self.inspector_hernia(ui);
                    self.inspector_ellipsoid(ui);
                    if self.workspace == Workspace::Compare {
                        self.inspector_compare(ui);
                    }
                    if is3d {
                        self.inspector_3d(ui);
                    } else {
                        if self.workspace == Workspace::Mpr {
                            self.inspector_mpr(ui);
                            self.inspector_cpr(ui);
                        }
                        self.inspector_2d(ui, &series);
                        if self.workspace == Workspace::Viewer && series.volume.is_some() {
                            self.inspector_cpr(ui);
                        }
                        self.inspector_specials(ui, &series);
                        let stack = matches!(self.active_view().map(|v| &v.content), Some(Content::Stack(_)));
                        self.inspector_view_aids(ui, stack);
                    }
                    self.inspector_findings(ui, &series);
                    self.inspector_series(ui, &series);
                });
            });
    }

    fn inspector_mpr(&mut self, ui: &mut Ui) {
        caption(ui, "Reconstrução MPR");
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Espessura (slab)");
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(if self.mpr.slab_mm > 0.0 { format!("{:.0} mm", self.mpr.slab_mm) } else { "fina".into() }).color(pal().text_dim));
                });
            });
            full_slider(ui, &mut self.mpr.slab_mm, 0.0..=60.0);
            self.mpr.slab_mm = self.mpr.slab_mm.round();
            ui.add_space(4.0);
            let mut mode = self.mpr.slab_mode;
            ui.add_enabled_ui(self.mpr.slab_mm > 0.0, |ui| {
                if segmented(ui, &mut mode, &[(SlabMode::Mip, "", "MIP"), (SlabMode::MinIp, "", "MinIP"), (SlabMode::Avg, "", "Média")]) {
                    self.mpr.slab_mode = mode;
                }
            });
            ui.add_space(6.0);
            ui.checkbox(&mut self.mpr.crosshair, "Mostrar linhas de referência");
            if self.mpr.is_oblique() {
                if ui.button(format!("{}  Restaurar planos ortogonais", icons::ARROW_COUNTER_CLOCKWISE)).clicked() {
                    self.mpr.reset_axes();
                }
            } else {
                ui.label(RichText::new("Com a mira (C), arraste as alças coloridas para inclinar os planos.").size(11.5).color(pal().text_faint));
            }
            if ui.button(format!("{}  Centralizar mira", icons::CROSSHAIR)).clicked()
                && let Some(v) = self.mpr_views[0].content.series().and_then(|s| s.volume.clone()) {
                    self.mpr.point = v.center();
                }
        });
    }

    fn inspector_2d(&mut self, ui: &mut Ui, series: &crate::dicom::series::LoadedSeries) {
        let mpr = self.workspace == Workspace::Mpr;
        let ct = series.info.modality == "CT";
        let rgb = series.is_rgb();
        caption(ui, "Janela");
        let mut apply: Option<Window> = None;
        let mut preset_click = None;
        card(ui, |ui| {
            let Some(v) = self.active_view() else { return };
            let cur = v.window;
            if ct {
                egui::Grid::new("presets").num_columns(3).spacing(vec2(6.0, 6.0)).show(ui, |ui| {
                    for (i, p) in CT_PRESETS.iter().enumerate() {
                        let w = p.window.unwrap_or(series.default_window);
                        let sel = (w.center - cur.center).abs() < 0.5 && (w.width - cur.width).abs() < 0.5;
                        let resp = chip(ui, p.name, sel, 78.0).on_hover_text(format!("Tecla {} · W {:.0} L {:.0}", p.key, w.width, w.center));
                        if resp.clicked() {
                            preset_click = Some(i);
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
                ui.add_space(8.0);
            }
            let mut w = cur;
            ui.horizontal(|ui| {
                ui.label(RichText::new("W").color(pal().text_faint));
                ui.add(egui::DragValue::new(&mut w.width).speed(2.0).range(1.0..=100000.0).max_decimals(0));
                ui.add_space(8.0);
                ui.label(RichText::new("L").color(pal().text_faint));
                ui.add(egui::DragValue::new(&mut w.center).speed(2.0).max_decimals(0));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Auto").on_hover_text("Janela padrão da série").clicked() {
                        w = series.default_window;
                    }
                });
            });
            if w != cur {
                apply = Some(w);
            }
        });
        if let Some(i) = preset_click {
            self.apply_window_preset(i);
        }
        if let Some(w) = apply {
            if mpr {
                self.mpr_views.iter_mut().take(3).for_each(|v| v.window = w);
            } else if let Some(v) = self.active_view_mut() {
                v.window = w;
            }
        }

        caption(ui, "Exibição");
        card(ui, |ui| {
            let smooth = &mut self.settings.smooth;
            ui.checkbox(smooth, "Interpolação suave");
            ui.checkbox(&mut self.settings.overlays, "Informações sobre a imagem");
            let Some(v) = self.active_view_mut() else { return };
            ui.checkbox(&mut v.invert, "Inverter");
            if !rgb {
                ui.horizontal(|ui| {
                    ui.label("Cores");
                    egui::ComboBox::from_id_salt("cmap").selected_text(v.colormap.label()).show_ui(ui, |ui| {
                        for c in Colormap::ALL {
                            ui.selectable_value(&mut v.colormap, c, c.label());
                        }
                    });
                });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if icon_button_sized(ui, icons::ARROW_COUNTER_CLOCKWISE, "Girar à esquerda", false, 30.0).clicked() {
                    v.rot = (v.rot + 3) % 4;
                }
                if icon_button_sized(ui, icons::ARROW_CLOCKWISE, "Girar à direita", false, 30.0).clicked() {
                    v.rot = (v.rot + 1) % 4;
                }
                if icon_button_sized(ui, icons::FLIP_HORIZONTAL, "Espelhar horizontal", v.flip_h, 30.0).clicked() {
                    v.flip_h = !v.flip_h;
                }
                if icon_button_sized(ui, icons::FLIP_VERTICAL, "Espelhar vertical", v.flip_v, 30.0).clicked() {
                    v.flip_v = !v.flip_v;
                }
                if icon_button_sized(ui, icons::CORNERS_OUT, "Ajustar à janela", false, 30.0).clicked() {
                    v.zoom = None;
                    v.pan = egui::Vec2::ZERO;
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Zoom").color(pal().text_dim));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if icon_button_sized(ui, icons::MAGNIFYING_GLASS_PLUS, "Aproximar (+)", false, 30.0).clicked() {
                        v.zoom_by(1.25);
                    }
                    if icon_button_sized(ui, icons::MAGNIFYING_GLASS_MINUS, "Afastar (−)", false, 30.0).clicked() {
                        v.zoom_by(1.0 / 1.25);
                    }
                });
            });
        });

        if !mpr && series.len() > 1 {
            caption(ui, "Cine");
            card(ui, |ui| {
                let Some(v) = self.active_view_mut() else { return };
                ui.horizontal(|ui| {
                    let (icon, text) = if v.cine { (icons::PAUSE, "Pausar") } else { (icons::PLAY, "Reproduzir") };
                    if ui.button(format!("{icon}  {text}")).clicked() {
                        v.cine = !v.cine;
                    }
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(format!("{:.0} qps", v.cine_fps)).color(pal().text_dim));
                    });
                });
                full_slider(ui, &mut v.cine_fps, 1.0..=60.0);
            });
        }

        let view_key = self.active_view().map(|v| v.measurements.len()).unwrap_or(0);
        if view_key > 0 {
            caption(ui, "Medições");
            card(ui, |ui| {
                let Some(v) = self.active_view_mut() else { return };
                let mut remove = None;
                for (i, m) in v.measurements.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let (icon, name) = match m.kind {
                            MeasureKind::Length => (icons::RULER, "Distância"),
                            MeasureKind::Angle => (icons::ANGLE, "Ângulo"),
                            MeasureKind::Ellipse => (icons::CIRCLE, "ROI"),
                            MeasureKind::Probe => (icons::TARGET, "Ponto"),
                            MeasureKind::Arrow => (icons::ARROW_UP_RIGHT, "Seta"),
                        };
                        ui.label(RichText::new(icon).color(MEASURE));
                        if m.kind == MeasureKind::Arrow {
                            ui.add(egui::TextEdit::singleline(&mut m.text).hint_text("texto").desired_width(ui.available_width() - 40.0));
                        } else {
                            ui.label(format!("{name} · imagem {}", (m.key & 0xFFFF_FFFF) + 1));
                        }
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if icon_button_sized(ui, icons::X, "Remover", false, 22.0).clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = remove {
                    v.measurements.remove(i);
                }
                if ui.button(format!("{}  Limpar todas", icons::TRASH)).clicked() {
                    v.measurements.clear();
                }
            });
        }
    }

    fn inspector_3d(&mut self, ui: &mut Ui) {
        let Some(series) = self.active_series() else { return };
        let previews = self.vr_previews(ui.ctx(), &series);
        caption(ui, "Renderização");
        let quality = &mut self.settings.vr_quality;
        let lighting = &mut self.settings.vr_lighting;
        let v = match self.workspace {
            Workspace::Mpr => &mut self.mpr_views[3],
            _ => &mut self.vr_view,
        };
        let v3 = &mut v.v3d;
        card(ui, |ui| {
            let mut mode = v3.mode;
            if segmented(
                ui,
                &mut mode,
                &[(RenderMode::Composite, "", "VR"), (RenderMode::Mip, "", "MIP"), (RenderMode::MinIp, "", "MinIP"), (RenderMode::Iso, "", "Superfície")],
            ) {
                v3.mode = mode;
            }
            if matches!(v3.mode, RenderMode::Composite | RenderMode::Iso) {
                ui.add_space(8.0);
                ui.label(RichText::new("Iluminação").size(12.0).color(pal().text_dim));
                let options: Vec<(crate::config::Lighting, &str, &str)> = crate::config::Lighting::ALL.iter().map(|l| (*l, "", l.label())).collect();
                segmented(ui, lighting, &options);
                ui.label(RichText::new(lighting.help()).size(11.5).color(pal().text_faint));
            }
        });

        if v3.mode == RenderMode::Composite {
            caption(ui, "Predefinições");
            let ct = series.info.modality == "CT";
            egui::Grid::new("vr-presets").num_columns(2).spacing(vec2(6.0, 6.0)).show(ui, |ui| {
                let mut col = 0;
                for (i, p) in PRESETS.iter().enumerate() {
                    if p.ct != ct && p.ct {
                        continue;
                    }
                    let card = match previews.get(i).cloned().flatten() {
                        Some(tex) => super::previews::preset_preview_card(ui, p.name, &tex, v3.preset == i),
                        None => preset_card(ui, p.name, p.swatch, v3.preset == i),
                    };
                    if card.clicked() {
                        v3.preset = i;
                        v3.adjust = TfAdjust::default();
                        v3.opacity = 1.0;
                    }
                    col += 1;
                    if col % 2 == 0 {
                        ui.end_row();
                    }
                }
            });
        }

        caption(ui, "Ajustes");
        card(ui, |ui| {
            match v3.mode {
                RenderMode::Composite => {
                    labeled_slider(ui, "Opacidade", &mut v3.opacity, 0.05..=4.0, "×");
                    labeled_slider(ui, "Deslocamento", &mut v3.adjust.shift, -1000.0..=1000.0, "");
                    labeled_slider(ui, "Largura", &mut v3.adjust.width, 0.2..=3.0, "×");
                    ui.checkbox(&mut v3.shading, "Iluminação");
                }
                RenderMode::Iso => {
                    let mut iso = v3.iso.unwrap_or(if series.info.modality == "CT" { 250.0 } else { series.default_window.center });
                    let (lo, hi) = series.volume.as_ref().map(|v| v.real_range()).unwrap_or((-1000.0, 3000.0));
                    labeled_slider(ui, "Limiar", &mut iso, lo..=hi, "");
                    v3.iso = Some(iso);
                }
                _ => {
                    let mut w = v3.mip_window.unwrap_or(series.default_window);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("W").color(pal().text_faint));
                        ui.add(egui::DragValue::new(&mut w.width).speed(2.0).range(1.0..=100000.0).max_decimals(0));
                        ui.label(RichText::new("L").color(pal().text_faint));
                        ui.add(egui::DragValue::new(&mut w.center).speed(2.0).max_decimals(0));
                    });
                    v3.mip_window = Some(w);
                }
            }
            labeled_slider(ui, "Qualidade", quality, 0.5..=2.5, "×");
        });

        caption(ui, "Recorte");
        card(ui, |ui| {
            let names = [("D", "E", "Direita–Esquerda"), ("A", "P", "Anterior–Posterior"), ("I", "S", "Inferior–Superior")];
            for (axis, (a, b, title)) in names.iter().enumerate() {
                ui.label(RichText::new(*title).size(12.0).color(pal().text_dim));
                range_slider(ui, egui::Id::unique(("crop", axis)), &mut v3.crop[axis], a, b);
                ui.add_space(4.0);
            }
            if ui.button(format!("{}  Remover recorte", icons::CROP)).clicked() {
                v3.crop = [[0.0, 1.0]; 3];
            }
        });

        caption(ui, "Orientação");
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for p in ViewPreset::ALL {
                    if chip(ui, p.short(), false, 32.0).on_hover_text(p.label()).clicked() {
                        v3.camera.set_preset(p);
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button(format!("{}  90°", icons::ARROW_CLOCKWISE)).clicked() {
                    v3.camera.roll(-std::f32::consts::FRAC_PI_2);
                }
                if ui.button(format!("{}  Reiniciar", icons::ARROW_COUNTER_CLOCKWISE)).clicked() {
                    let d = crate::ui::viewport::View3D::default();
                    v3.camera = d.camera;
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Zoom").color(pal().text_dim));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if icon_button_sized(ui, icons::MAGNIFYING_GLASS_PLUS, "Aproximar (+)", false, 30.0).clicked() {
                        v3.camera.zoom(1.0 / 1.25);
                    }
                    if icon_button_sized(ui, icons::MAGNIFYING_GLASS_MINUS, "Afastar (−)", false, 30.0).clicked() {
                        v3.camera.zoom(1.25);
                    }
                });
            });
        });
    }

    fn inspector_series(&mut self, ui: &mut Ui, series: &crate::dicom::series::LoadedSeries) {
        caption(ui, "Série");
        let mut show_tags = false;
        card(ui, |ui| {
            let first = series.images.first();
            kv(ui, "Modalidade", &series.info.modality);
            kv(ui, "Imagens", &series.len().to_string());
            if let Some(f) = first {
                kv(ui, "Matriz", &format!("{} × {}", f.width, f.height));
                if f.spacing_known {
                    kv(ui, "Pixel", &format!("{:.3} × {:.3} mm", f.spacing.0, f.spacing.1));
                }
                if let Some(t) = f.thickness {
                    kv(ui, "Espessura", &format!("{t:.2} mm"));
                }
            }
            if let Some(v) = &series.volume {
                kv(ui, "Volume", &format!("{}×{}×{} · {:.2} mm", v.dims[0], v.dims[1], v.dims[2], v.spacing.z));
            } else {
                kv(ui, "Volume", "não disponível");
            }
            kv(ui, "Data", &format_date(&series.study.date));
            for w in &series.warnings {
                ui.add_space(4.0);
                ui.label(RichText::new(format!("{}  {w}", icons::WARNING)).color(WARNING).size(12.0));
            }
            ui.add_space(6.0);
            if ui.button(format!("{}  Tags DICOM", icons::TAG)).clicked() {
                show_tags = true;
            }
        });
        if show_tags {
            self.show_tags_for_active();
        }
    }

    pub fn welcome(&mut self, ui: &mut Ui) {
        let rect = ui.available_rect_before_wrap();
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::ZERO, pal().bg_app);
        // Soft glow behind the hero card.
        for i in 0..18 {
            let r = 420.0 - i as f32 * 20.0;
            p.circle_filled(rect.center() - vec2(0.0, 40.0), r, Color32::from_rgba_unmultiplied(70, 110, 255, 2));
        }
        // Two columns: the program on the left; the three ways to open (all alike,
        // each with a line saying what it opens) and the recent exams on the right.
        let recent: Vec<std::path::PathBuf> = self.settings.recent.iter().take(4).cloned().collect();
        let pad = 30.0;
        // Narrow windows: both columns shrink (texts that do not fit end with "…").
        let content_w = (rect.width() - 80.0 - pad * 4.0 - 1.0).clamp(380.0, 560.0);
        let (left_w, right_w) = (content_w * 0.41, content_w * 0.59);
        let recent_h = if recent.is_empty() { 0.0 } else { 30.0 + recent.len() as f32 * 26.0 };
        // Height of the right column as drawn on the previous frame (an estimate on the first one).
        let right_id = egui::Id::unique("welcome-right-height");
        let right_h = ui.ctx().data(|d| d.get_temp::<f32>(right_id)).unwrap_or(22.0 + 3.0 * 58.0 + recent_h);
        let inner_h = right_h.max(250.0);
        let card_size = vec2(left_w + 1.0 + right_w + pad * 4.0, inner_h + pad * 2.0);
        let card_rect = Rect::from_center_size(rect.center() - vec2(0.0, 10.0), card_size);
        let p = ui.painter();
        p.add(egui::Shadow { offset: [0, 18], blur: 48, spread: 0, color: Color32::from_black_alpha(pal().shadow) }.as_shape(card_rect, CornerRadius::same(18)));
        p.rect_filled(card_rect, CornerRadius::same(18), with_alpha(pal().bg_raised, 235));
        p.rect_stroke(card_rect, CornerRadius::same(18), Stroke::new(1.0, pal().border_strong), StrokeKind::Inside);
        let left = Rect::from_min_size(card_rect.min + vec2(pad, pad), vec2(left_w, inner_h));
        let divider = left.right() + pad;
        p.vline(divider, card_rect.y_range().shrink(pad), Stroke::new(1.0, pal().border));
        let right = Rect::from_min_size(pos2(divider + 1.0 + pad, left.top()), vec2(right_w, inner_h));

        // Left, as one block centered on the column's height: the logo with the
        // name and version beside it, what the program does and the drag hint.
        let logo_side = if left.width() < 210.0 { 52.0 } else { 64.0 };
        let name = p.layout_no_wrap("Slyced".into(), font_bold(28.0), pal().text);
        let version = p.layout_no_wrap(format!("versão {}", crate::config::APP_VERSION), font(13.5), pal().text_faint);
        let about = p.layout("Visualizador DICOM portátil\n2D · MPR · 3D · PACS".into(), font(13.0), pal().text_dim, left.width());
        let hint = p.layout("Ou arraste pastas, arquivos ou um CD/DVD com DICOMDIR para esta janela.".into(), font(12.0), pal().text_faint, left.width());
        let (gap_about, gap_hint) = (24.0, 18.0);
        let block_h = logo_side + gap_about + about.size().y + gap_hint + hint.size().y;
        let logo_rect = Rect::from_min_size(pos2(left.left(), left.center().y - block_h * 0.5), vec2(logo_side, logo_side));
        paint_logo(p, logo_rect);
        let name_h = name.size().y - 4.0;
        let top = logo_rect.center().y - (name_h + version.size().y) * 0.5;
        let x = logo_rect.right() + 14.0;
        p.galley(pos2(x, top), name, pal().text);
        p.galley(pos2(x, top + name_h), version, pal().text_faint);
        let mut y = logo_rect.bottom() + gap_about;
        let about_h = about.size().y;
        p.galley(pos2(left.left(), y), about, pal().text_dim);
        y += about_h + gap_hint;
        p.galley(pos2(left.left(), y), hint, pal().text_faint);

        // Right, also centered on the height of the card: the three ways to open, then the recent exams.
        let right = Rect::from_min_max(pos2(right.left(), right.top() + ((inner_h - right_h) * 0.5).max(0.0)), right.max);
        let drawn = ui.scope_builder(egui::UiBuilder::new().max_rect(right), |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(RichText::new("ABRIR").font(font_bold(10.5)).color(pal().text_faint).extra_letter_spacing(0.8));
            ui.add_space(-2.0);
            if open_choice(ui, icons::FOLDER_OPEN, "Pasta", "Exame numa pasta, pen drive ou CD/DVD", "Ctrl+O").clicked() {
                self.open_folder_dialog();
            }
            if open_choice(ui, icons::FILES, "Arquivos", "Arquivos DICOM soltos ou um ZIP", "Ctrl+Shift+O").clicked() {
                self.open_files_dialog();
            }
            if open_choice(ui, icons::NETWORK, "PACS", "Buscar e baixar exames do servidor", "").clicked() {
                self.pacs.open = true;
            }
            if !recent.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("RECENTES").font(font_bold(10.5)).color(pal().text_faint).extra_letter_spacing(0.8));
                ui.spacing_mut().item_spacing.y = 2.0;
                for r in recent {
                    // Folder name and its parent: full paths are too long to read here.
                    let name = r.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| r.display().to_string());
                    let parent = r.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let text = if parent.is_empty() { format!("{}  {name}", icons::CLOCK) } else { format!("{}  {name}   ·  {parent}", icons::CLOCK) };
                    let resp = ui.add(egui::Button::new(RichText::new(text).size(12.5).color(pal().text_dim)).frame(false).truncate());
                    if resp.on_hover_text(r.display().to_string()).clicked() {
                        self.open_paths(vec![r], true);
                    }
                }
            }
        });
        let drawn_h = drawn.response.rect.height();
        if (drawn_h - right_h).abs() > 0.5 {
            ui.ctx().data_mut(|d| d.insert_temp(right_id, drawn_h));
            ui.ctx().request_repaint();
        }
        // Author credit centered at the bottom of the welcome screen.
        let footer = Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 32.0), vec2(rect.width(), 30.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(footer), |ui| {
            ui.vertical_centered(|ui| {
                super::credit::author_badge(ui, 28.0);
            });
        });
    }

    /// PACS downloads in the status bar; a click lists them (also with the PACS window closed).
    fn downloads_indicator(&mut self, ui: &mut Ui) {
        let t = &self.pacs.transfers;
        if t.is_empty() {
            return;
        }
        let active: Vec<_> = t.iter().filter(|t| !t.done).collect();
        let failed = t.iter().filter(|t| t.error.is_some()).count();
        let text = if !active.is_empty() {
            let frac = active.iter().map(|t| t.fraction()).sum::<f32>() / active.len() as f32;
            format!("{}  {} baixando · {:.0}%", icons::DOWNLOAD_SIMPLE, active.len(), frac * 100.0)
        } else if failed > 0 {
            format!("{}  {failed} download(s) com erro", icons::WARNING_CIRCLE)
        } else {
            format!("{}  downloads concluídos", icons::CHECK_CIRCLE)
        };
        let color = if failed > 0 { DANGER } else if !active.is_empty() { pal().accent } else { SUCCESS };
        let resp = ui.add(egui::Label::new(RichText::new(text).size(11.5).color(color)).sense(Sense::click())).on_hover_text("Downloads do PACS");
        self.open_requested_menu(ui, "downloads", &resp);
        egui::Popup::menu(&resp).width(460.0).show(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Downloads do PACS").font(font_bold(13.0)).color(pal().text));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if self.pacs.transfers.iter().any(|t| t.done) && ui.small_button("Limpar concluídos").clicked() {
                        self.pacs.transfers.retain(|t| !t.done);
                    }
                    if ui.small_button(format!("{}  Janela PACS", icons::NETWORK)).clicked() {
                        self.pacs.open = true;
                        ui.close();
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| self.transfer_rows(ui));
        });
    }

    /// Asks before removing the downloaded exams ("Apagar agora" in the settings).
    pub fn confirm_clear_store_ui(&mut self, ctx: &egui::Context) {
        if !self.confirm_clear_store {
            return;
        }
        let what = self.store_usage.map(|u| u.describe()).unwrap_or_else(|| "os exames".into());
        let mut decision = None;
        egui::Modal::new(egui::Id::unique("confirm-clear-store")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Apagar os exames baixados do PACS?").font(font_bold(15.0)).color(pal().text));
            ui.add_space(4.0);
            ui.label(RichText::new(format!("{what} serão apagados deste computador. Os exames abertos agora ficam; os demais podem ser baixados de novo do PACS.")).size(12.5).color(pal().text_dim));
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new("Apagar").color(Color32::WHITE)).fill(DANGER)).clicked() {
                    decision = Some(true);
                }
                if ui.button("Cancelar").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    decision = Some(false);
                }
            });
        });
        if let Some(yes) = decision {
            self.confirm_clear_store = false;
            if yes {
                self.clean_store_now(None);
            }
        }
    }

    /// Asks before closing every open exam.
    pub fn confirm_close_all_ui(&mut self, ctx: &egui::Context) {
        if !self.confirm_close_all {
            return;
        }
        let n = self.library.studies.len();
        let mut decision = None;
        egui::Modal::new(egui::Id::unique("confirm-close-all")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(if n == 1 { "Fechar o exame aberto?".to_string() } else { format!("Fechar os {n} exames abertos?") }).font(font_bold(15.0)).color(pal().text));
            ui.add_space(4.0);
            ui.label(RichText::new("As medidas que não foram salvas como achado se perdem. Logo depois dá para desfazer.").size(12.5).color(pal().text_dim));
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                let close = egui::Button::new(RichText::new("Fechar").color(Color32::WHITE)).fill(DANGER);
                if ui.add(close).clicked() {
                    decision = Some(true);
                }
                if ui.button("Cancelar").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    decision = Some(false);
                }
            });
        });
        match decision {
            Some(true) => self.close_all_studies(),
            Some(false) => self.confirm_close_all = false,
            None => {}
        }
    }

    pub(crate) fn close_all_studies(&mut self) {
        self.confirm_close_all = false;
        let all: Vec<String> = self.library.studies.iter().map(|s| s.header.uid.clone()).collect();
        let n = all.len();
        for uid in all {
            self.close_study(&uid);
        }
        if n > 0 {
            self.toast_action(ToastKind::Info, if n == 1 { "Exame fechado.".to_string() } else { format!("{n} exames fechados.") }, ToastAction::ReopenClosed);
        }
    }

    pub fn toasts_ui(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        // Reading full screen: a slim strip at the top instead of cards at the corner.
        let slim = self.reading;
        let mut y = if slim { screen.top() + 12.0 } else { screen.bottom() - 40.0 };
        let mut clicked = None;
        for (i, t) in self.toasts.iter().enumerate().rev().take(if slim { 2 } else { 4 }) {
            let (icon, color) = match t.kind {
                ToastKind::Info => (icons::INFO, pal().accent),
                ToastKind::Success => (icons::CHECK_CIRCLE, SUCCESS),
                ToastKind::Error => (icons::WARNING_CIRCLE, DANGER),
            };
            let fade = ((t.until - self.time) as f32 / 0.4).clamp(0.0, 1.0);
            let (align, offset) = if slim { (Align2::CENTER_TOP, vec2(0.0, y - screen.top())) } else { (Align2::RIGHT_BOTTOM, vec2(-16.0, y - screen.bottom())) };
            let area = egui::Area::new(egui::Id::unique(("toast", i))).order(egui::Order::Foreground).anchor(align, offset).interactable(t.action.is_some());
            let resp = area.show(ctx, |ui| {
                ui.set_max_width(if slim { 560.0 } else { 380.0 });
                ui.multiply_opacity(fade);
                Frame::new()
                    .fill(if slim { with_alpha(pal().bg_raised, 230) } else { pal().bg_raised })
                    .stroke(Stroke::new(1.0, pal().border_strong))
                    .corner_radius(if slim { 16 } else { 10 })
                    .inner_margin(if slim { Margin::symmetric(12, 5) } else { Margin::symmetric(14, 10) })
                    .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(pal().shadow) })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icon).size(if slim { 13.0 } else { 16.0 }).color(color));
                            let text = RichText::new(&t.text).color(pal().text);
                            ui.add(egui::Label::new(if slim { text.size(12.5) } else { text }).wrap());
                            if let Some(action) = t.action {
                                ui.add_space(8.0);
                                let undo = egui::Button::new(RichText::new("Desfazer").font(font_bold(13.0)).color(pal().accent)).frame(false);
                                if ui.add(undo).clicked() {
                                    clicked = Some(action);
                                }
                            }
                        });
                    });
            });
            if slim {
                y += resp.response.rect.height() + 6.0;
            } else {
                y -= resp.response.rect.height() + 8.0;
            }
        }
        match clicked {
            Some(ToastAction::ReopenClosed) => self.reopen_closed(),
            None => {}
        }
    }

    pub fn drop_overlay(&self, ctx: &egui::Context) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if !hovering {
            return;
        }
        let screen = ctx.content_rect();
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop")));
        p.rect_filled(screen, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(8, 12, 22, 210));
        let r = screen.shrink(24.0);
        p.rect_stroke(r, CornerRadius::same(16), Stroke::new(2.0, pal().accent), StrokeKind::Inside);
        p.text(r.center() - vec2(0.0, 18.0), Align2::CENTER_CENTER, icons::DOWNLOAD_SIMPLE, font(40.0), pal().accent);
        p.text(r.center() + vec2(0.0, 24.0), Align2::CENTER_CENTER, "Solte para abrir os exames", font_bold(18.0), pal().text);
    }
}

pub(crate) fn layout_icon(l: Layout) -> &'static str {
    match l {
        Layout::One => icons::SQUARE,
        Layout::Cols2 => icons::COLUMNS,
        Layout::Rows2 => icons::ROWS,
        Layout::Grid4 => icons::SQUARES_FOUR,
        Layout::Grid6 => icons::GRID_FOUR,
    }
}

pub fn search_box(ui: &mut Ui, text: &mut String, hint: &str) {
    Frame::new()
        .fill(pal().bg_input)
        .stroke(Stroke::new(1.0, pal().border))
        .corner_radius(9)
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icons::MAGNIFYING_GLASS).color(pal().text_faint));
                ui.add(egui::TextEdit::singleline(text).hint_text(hint).frame(egui::Frame::NONE).desired_width(f32::INFINITY));
            });
        });
}

enum BrowserAction {
    Toggle(String),
    Close(String),
    CloseAll,
    ExpandAll,
    CollapseAll,
    Compare(String),
    Open(String, Target),
    OpenPath(std::path::PathBuf),
}

/// One way to open exams on the welcome card: icon, name, a line saying what it
/// opens and the shortcut. The three look alike, none is highlighted.
fn open_choice(ui: &mut Ui, icon: &str, title: &str, detail: &str, keys: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
    let p = ui.painter();
    let hot = resp.hovered();
    p.rect_filled(rect, CornerRadius::same(10), if hot { pal().bg_hover } else { pal().bg_input });
    p.rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0, if hot { pal().border_strong } else { pal().border }), StrokeKind::Inside);
    p.text(pos2(rect.left() + 22.0, rect.center().y), Align2::CENTER_CENTER, icon, font(19.0), if hot { pal().text } else { pal().text_dim });
    let x = rect.left() + 44.0;
    p.text(pos2(x, rect.top() + 9.0), Align2::LEFT_TOP, title, font_bold(13.5), pal().text);
    let keys_w = if keys.is_empty() { 0.0 } else { p.layout_no_wrap(keys.to_string(), font(11.0), pal().text_faint).size().x + 14.0 };
    let line = fit_text(p, detail.to_string(), font(11.5), pal().text_faint, rect.right() - 12.0 - keys_w - x);
    p.galley(pos2(x, rect.bottom() - 9.0 - line.size().y), line, pal().text_faint);
    if !keys.is_empty() {
        p.text(pos2(rect.right() - 12.0, rect.top() + 11.0), Align2::RIGHT_TOP, keys, font(11.0), pal().text_faint);
    }
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Menu entry of a recently opened folder or file: its name, the full path on hover.
fn recent_button(ui: &mut Ui, path: &std::path::Path) -> egui::Response {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
    ui.add(egui::Button::new(format!("{}  {}", icons::FOLDER_OPEN, name)).truncate()).on_hover_text(path.display().to_string())
}

/// Patient name with an initials avatar, sex, age and ID.
/// Initials of the patient in a circle, always the same color for the same patient.
fn patient_avatar(p: &egui::Painter, center: egui::Pos2, radius: f32, h: &crate::dicom::model::StudyHeader, name: &str) {
    let palette = [pal().accent, pal().accent_2, SUCCESS, WARNING, Color32::from_rgb(236, 112, 160), Color32::from_rgb(64, 196, 220)];
    let seed = h.patient_id.bytes().chain(h.patient_name.bytes()).fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let color = palette[seed as usize % palette.len()];
    p.circle_filled(center, radius, with_alpha(color, 50));
    p.circle_stroke(center, radius, Stroke::new(1.0, with_alpha(color, 170)));
    let initials: String = name
        .split([',', ' '])
        .filter(|w| !w.is_empty())
        .take(2)
        .filter_map(|w| w.chars().next())
        .collect::<String>()
        .to_uppercase();
    // Light letters on the dark themes, dark ones on the light theme.
    let ink = if pal().dark { lighten(color, 0.35) } else { Color32::from_rgb(color.r() / 2, color.g() / 2, color.b() / 2) };
    p.text(center, Align2::CENTER_CENTER, initials, font_bold(radius * 0.9), ink);
}

/// One line of text cut with "…" to fit `width`.
fn fit_text(p: &egui::Painter, text: String, font_id: egui::FontId, color: Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text, font_id, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(1.0));
    p.layout_job(job)
}

/// Modality label ending at `right`, centered on `cy`; returns its left edge.
fn paint_modality(p: &egui::Painter, modality: &str, right: f32, cy: f32) -> f32 {
    let c = modality_color(modality);
    let g = p.layout_no_wrap(modality.to_string(), font_bold(9.5), c);
    let r = Rect::from_min_size(pos2(right - g.size().x - 8.0, cy - g.size().y * 0.5 - 1.5), g.size() + vec2(8.0, 3.0));
    p.rect_filled(r, CornerRadius::same(4), with_alpha(c, 30));
    p.galley(r.min + vec2(4.0, 1.5), g, c);
    r.left()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Card of one exam: the patient on the first line (only on the patient's first exam,
/// `patient` = how many exams they have) and the exam on the next; ID, series, images
/// and hospital are in the tooltip. `timeline` reserves that height under the patient
/// for the timeline. Returns the card response, whether its close button was clicked
/// and the space reserved for the timeline.
fn study_card(
    ui: &mut Ui,
    st: &crate::dicom::model::StudyInfo,
    patient: Option<usize>,
    expanded: bool,
    active: bool,
    order: Option<&str>,
    timeline: f32,
) -> (egui::Response, bool, Rect) {
    let h = &st.header;
    let (line, pad) = (20.0, 5.0);
    let timeline = if patient.is_some() { timeline } else { 0.0 };
    let height = pad * 2.0 + line * if patient.is_some() { 2.0 } else { 1.0 } + timeline;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let p = ui.painter();
    let bg = if active {
        pal().selected_bg
    } else if resp.hovered() {
        pal().bg_hover
    } else {
        pal().bg_raised
    };
    p.rect_filled(rect, CornerRadius::same(8), bg);
    p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, if active { with_alpha(pal().accent, 170) } else { pal().border }), StrokeKind::Inside);
    let (left, right) = (rect.left() + 8.0, rect.right() - 6.0);
    let mut cy = rect.top() + pad + line * 0.5;

    let name = format_person_name(&h.patient_name);
    let name = if name.is_empty() { "Paciente sem nome".to_string() } else { name };
    let mut who: Vec<String> = Vec::new();
    if !h.sex.trim().is_empty() {
        who.push(h.sex.trim().to_string());
    }
    if !h.age.trim().is_empty() {
        who.push(crate::dicom::model::format_age(&h.age));
    } else if !h.birth_date.trim().is_empty() {
        who.push(format!("nasc. {}", format_date(&h.birth_date)));
    }
    if patient.is_some() {
        patient_avatar(p, pos2(left + 9.0, cy), 10.0, h, &name);
        let info = p.layout_no_wrap(who.join(" · "), font(11.5), pal().text_faint);
        let info_x = right - 2.0 - info.size().x;
        p.galley(pos2(info_x, cy - info.size().y * 0.5), info, pal().text_faint);
        let g = fit_text(p, name.clone(), font_bold(13.0), pal().text, info_x - 8.0 - (left + 24.0));
        p.galley(pos2(left + 24.0, cy - g.size().y * 0.5), g, pal().text);
        cy += line;
    }
    let timeline_rect = Rect::from_min_size(pos2(left + 2.0, cy - line * 0.5 + 2.0), vec2(right - left - 4.0, timeline));
    cy += timeline;

    // Exam: caret, date, order and description, modalities and the close button.
    let caret = if expanded { icons::CARET_DOWN } else { icons::CARET_RIGHT };
    p.text(pos2(left + 5.0, cy), Align2::CENTER_CENTER, caret, font(11.0), pal().text_faint);
    let close_x = right - 10.0;
    let mut mods_left = close_x - 14.0;
    for m in st.modalities().iter().rev() {
        mods_left = paint_modality(p, m, mods_left, cy) - 3.0;
    }
    let x = left + 16.0;
    let date = p.layout_no_wrap(format_date(&h.date), font_bold(12.5), pal().text);
    let date_w = date.size().x;
    p.galley(pos2(x, cy - date.size().y * 0.5), date, pal().text);
    let desc = if h.description.trim().is_empty() { "Exame sem descrição" } else { h.description.trim() };
    let rest = match order {
        Some(o) => format!("· {o} · {desc}"),
        None => format!("· {desc}"),
    };
    let color = if active { pal().text } else { pal().text_dim };
    let g = fit_text(p, rest, font(12.0), color, mods_left - 6.0 - (x + date_w + 5.0));
    p.galley(pos2(x + date_w + 5.0, cy - g.size().y * 0.5), g, color);

    // Close button, registered after the card so it receives its own clicks.
    let xr = Rect::from_center_size(pos2(close_x, cy), vec2(20.0, 20.0));
    let close = ui.interact(xr, ui.scope_id().with(("close-study", &h.uid)), Sense::click());
    let hot = close.hovered();
    if hot || resp.hovered() || active {
        p.circle_filled(xr.center(), 9.0, if hot { DANGER } else { pal().bg_hover });
        p.text(xr.center(), Align2::CENTER_CENTER, icons::X, font(11.0), if hot { pal().strong } else { pal().text_dim });
    }
    let clicked = close.on_hover_text("Fechar exame").clicked();

    let mut ids = who;
    if !h.patient_id.trim().is_empty() {
        ids.push(format!("ID {}", h.patient_id.trim()));
    }
    if let Some(n) = patient.filter(|n| *n > 1) {
        ids.push(format!("{n} exames"));
    }
    let mut tip = vec![name];
    if !ids.is_empty() {
        tip.push(ids.join(" · "));
    }
    let rel = crate::dicom::model::relative_date(&h.date);
    let when = if rel.is_empty() { format_date(&h.date) } else { format!("{} ({rel})", format_date(&h.date)) };
    tip.push(format!("{when} · {desc}"));
    let images: usize = st.series.iter().map(|s| s.image_count()).sum();
    tip.push(format!("{} · {}", plural(st.series.len(), "série", "séries"), plural(images, "imagem", "imagens")));
    if !h.institution.trim().is_empty() {
        tip.push(h.institution.trim().to_string());
    }
    tip.push("Clique para mostrar ou esconder as séries.".into());
    (resp.on_hover_text(tip.join("\n")), clicked, timeline_rect)
}

/// One series in list mode: small thumbnail, title, where it is open and how many images.
#[allow(clippy::too_many_arguments)]
fn series_row(
    ui: &mut Ui,
    s: &crate::dicom::model::SeriesInfo,
    thumb: Option<egui::TextureId>,
    width: f32,
    places: &[String],
    shown: bool,
    loading: Option<f32>,
    time: f64,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click_and_drag());
    let p = ui.painter();
    if shown {
        p.rect_filled(rect, CornerRadius::same(7), pal().selected_bg);
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(7), pal().bg_hover);
    }
    let img = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(32.0, 32.0));
    p.rect_filled(img, CornerRadius::same(5), Color32::BLACK);
    match thumb {
        Some(tex) => {
            let size = ui.ctx().tex_manager().read().meta(tex).map(|m| m.size).unwrap_or([1, 1]);
            let aspect = size[0] as f32 / size[1].max(1) as f32;
            let r = if aspect > 1.0 { vec2(32.0, 32.0 / aspect) } else { vec2(32.0 * aspect, 32.0) };
            p.image(tex, Rect::from_center_size(img.center(), r), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            p.text(img.center(), Align2::CENTER_CENTER, icons::IMAGE, font(14.0), pal().text_faint);
        }
    }
    if shown {
        p.rect_stroke(img, CornerRadius::same(5), Stroke::new(1.5, pal().accent), StrokeKind::Inside);
    }
    let x = img.right() + 9.0;
    if let Some(f) = loading {
        p.rect_filled(img, CornerRadius::same(5), Color32::from_black_alpha(150));
        spinner(p, img.center(), 7.0, time, pal().accent);
        progress_bar(p, Rect::from_min_size(pos2(x, rect.bottom() - 6.0), vec2(rect.right() - x - 8.0, 2.5)), f);
    }
    let cy = rect.center().y;
    let n = s.image_count();
    let count = p.layout_no_wrap(n.to_string(), font(11.5), pal().text_faint);
    let mut right = rect.right() - 8.0 - count.size().x;
    p.galley(pos2(right, cy - count.size().y * 0.5), count, pal().text_faint);
    // Where the series is open.
    if !places.is_empty() {
        let g = p.layout_no_wrap(places.join(" · "), font_bold(9.5), Color32::WHITE);
        let r = Rect::from_min_size(pos2(right - 6.0 - g.size().x - 10.0, cy - g.size().y * 0.5 - 1.5), g.size() + vec2(10.0, 3.0));
        p.rect_filled(r, CornerRadius::same(7), with_alpha(pal().accent, 160));
        p.galley(r.min + vec2(5.0, 1.5), g, Color32::WHITE);
        right = r.left();
    }
    let color = if shown { pal().strong } else { pal().text };
    let title = fit_text(p, format!("{} · {}", s.number, s.title()), font_bold(12.5), color, right - 8.0 - x);
    p.galley(pos2(x, cy - title.size().y * 0.5), title, color);

    let mut details = vec![s.modality.clone(), plural(n, "imagem", "imagens")];
    if let Some(t) = s.instances.iter().find_map(|i| i.slice_thickness) {
        details.push(format!("{} mm", format!("{t:.2}").trim_end_matches('0').trim_end_matches('.').replace('.', ",")));
    }
    let mut tip = format!("Série {} · {}\n{}", s.number, s.title(), details.join(" · "));
    if !places.is_empty() {
        tip.push_str(&format!("\nAberta em: {}", places.join(", ")));
    }
    tip.push_str("\nClique para abrir na janela ativa ou arraste para outra janela.\nBotão direito: abrir em MPR, 3D ou comparação.");
    resp.on_hover_text(tip)
}

fn series_card(
    ui: &mut Ui,
    s: &crate::dicom::model::SeriesInfo,
    thumb: Option<egui::TextureId>,
    width: f32,
    shown: bool,
    loading: Option<f32>,
    time: f64,
) -> egui::Response {
    let h = width * 0.78 + 40.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::click_and_drag());
    let p = ui.painter();
    let hovered = resp.hovered();
    p.rect_filled(rect, CornerRadius::same(9), if hovered { pal().bg_hover } else { pal().bg_input });
    let img_rect = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(width - 8.0, width * 0.78 - 4.0));
    p.rect_filled(img_rect, CornerRadius::same(6), Color32::BLACK);
    if let Some(tex) = thumb {
        // Letterbox the thumbnail into the image area (square thumbnails assumed max 128px).
        let size = ui.ctx().tex_manager().read().meta(tex).map(|m| m.size).unwrap_or([1, 1]);
        let aspect = size[0] as f32 / size[1].max(1) as f32;
        let mut r = img_rect;
        if aspect > img_rect.aspect_ratio() {
            r.set_height(img_rect.width() / aspect);
        } else {
            r.set_width(img_rect.height() * aspect);
        }
        r = Rect::from_center_size(img_rect.center(), r.size());
        p.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    } else {
        p.text(img_rect.center(), Align2::CENTER_CENTER, icons::IMAGE, font(20.0), pal().text_faint);
    }
    if let Some(f) = loading {
        p.rect_filled(img_rect, CornerRadius::same(6), Color32::from_black_alpha(150));
        spinner(p, img_rect.center(), 10.0, time, pal().accent);
        progress_bar(p, Rect::from_min_size(img_rect.left_bottom() + vec2(6.0, -10.0), vec2(img_rect.width() - 12.0, 3.0)), f);
    }
    // Count badge
    let count = s.image_count().to_string();
    let g = p.layout_no_wrap(count, font_bold(10.0), pal().text);
    let br = Rect::from_min_size(img_rect.right_top() + vec2(-g.size().x - 12.0, 4.0), g.size() + vec2(8.0, 2.0));
    p.rect_filled(br, CornerRadius::same(4), Color32::from_black_alpha(170));
    p.galley(br.min + vec2(4.0, 1.0), g, pal().text);
    let mc = modality_color(&s.modality);
    let mg = p.layout_no_wrap(s.modality.clone(), font_bold(9.5), mc);
    let mr = Rect::from_min_size(img_rect.left_top() + vec2(4.0, 4.0), mg.size() + vec2(8.0, 2.0));
    p.rect_filled(mr, CornerRadius::same(4), Color32::from_black_alpha(170));
    p.galley(mr.min + vec2(4.0, 1.0), mg, mc);

    let label_y = img_rect.bottom() + 6.0;
    let title = format!("{} · {}", s.number, s.title());
    let galley = p.layout(title, font(11.5), if shown { pal().text } else { pal().text_dim }, width - 10.0);
    let clipped = p.with_clip_rect(Rect::from_min_max(pos2(rect.left(), label_y), rect.right_bottom() - vec2(0.0, 2.0)));
    clipped.galley(pos2(rect.left() + 5.0, label_y), galley, pal().text);
    if shown {
        p.rect_stroke(rect, CornerRadius::same(9), Stroke::new(1.5, pal().accent), StrokeKind::Inside);
    }
    resp.on_hover_text(format!("Série {} · {}\n{} imagem(ns)\nArraste para uma janela ou clique para abrir", s.number, s.title(), s.image_count()))
}

pub(crate) fn chip(ui: &mut Ui, text: &str, selected: bool, width: f32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), font(12.0), pal().text);
    let w = width.max(galley.size().x + 14.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    let p = ui.painter();
    let (bg, fg, stroke) = if selected {
        (pal().selected_bg, pal().strong, pal().accent)
    } else if resp.hovered() {
        (pal().bg_hover, pal().text, pal().border_strong)
    } else {
        (pal().bg_input, pal().text_dim, pal().border)
    };
    p.rect_filled(rect, CornerRadius::same(7), bg);
    p.rect_stroke(rect, CornerRadius::same(7), Stroke::new(1.0, stroke), StrokeKind::Inside);
    p.text(rect.center(), Align2::CENTER_CENTER, text, font(12.0), fg);
    resp
}

fn preset_card(ui: &mut Ui, name: &str, swatch: [u8; 3], selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(124.0, 44.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(9), if resp.hovered() { pal().bg_hover } else { pal().bg_input });
    let sw = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(32.0, 32.0));
    let c = Color32::from_rgb(swatch[0], swatch[1], swatch[2]);
    gradient_rect(p, sw, 7, Color32::from_rgb(20, 14, 12), c);
    p.circle_filled(sw.center() + vec2(4.0, -3.0), 6.0, with_alpha(Color32::WHITE, 60));
    p.text(pos2(sw.right() + 8.0, rect.center().y), Align2::LEFT_CENTER, name, font(12.5), if selected { pal().strong } else { pal().text_dim });
    p.rect_stroke(rect, CornerRadius::same(9), Stroke::new(if selected { 1.5 } else { 1.0 }, if selected { pal().accent } else { pal().border }), StrokeKind::Inside);
    resp
}

pub(crate) fn labeled_slider(ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, suffix: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.5));
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(format!("{:.2}{suffix}", value)).size(12.0).color(pal().text_dim));
        });
    });
    full_slider(ui, value, range);
}

/// Slider without value text that spans the available width.
pub(crate) fn full_slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 4.0).max(60.0);
        ui.add(egui::Slider::new(value, range).show_value(false));
    });
}

/// Two-handle slider over [0, 1].
fn range_slider(ui: &mut Ui, id: egui::Id, v: &mut [f32; 2], lo_label: &str, hi_label: &str) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    let track = Rect::from_min_max(pos2(rect.left() + 16.0, rect.center().y - 2.0), pos2(rect.right() - 16.0, rect.center().y + 2.0));
    let p = ui.painter();
    p.text(pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, lo_label, font_bold(10.5), pal().text_faint);
    p.text(pos2(rect.right(), rect.center().y), Align2::RIGHT_CENTER, hi_label, font_bold(10.5), pal().text_faint);
    p.rect_filled(track, CornerRadius::same(2), pal().bg_hover);
    let x = |t: f32| track.left() + t * track.width();
    let sel = Rect::from_min_max(pos2(x(v[0]), track.top()), pos2(x(v[1]), track.bottom()));
    p.rect_filled(sel, CornerRadius::same(2), pal().accent);
    for i in 0..2 {
        let c = pos2(x(v[i]), track.center().y);
        let hr = Rect::from_center_size(c, vec2(16.0, 20.0));
        let r = ui.interact(hr, id.with(i), Sense::drag());
        if r.dragged()
            && let Some(pos) = r.interact_pointer_pos() {
                let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
                if i == 0 {
                    v[0] = t.min(v[1] - 0.02);
                } else {
                    v[1] = t.max(v[0] + 0.02);
                }
            }
        let p = ui.painter();
        p.circle_filled(c, 7.0, if r.hovered() || r.dragged() { pal().strong } else { pal().text });
        p.circle_stroke(c, 7.0, Stroke::new(2.0, pal().accent));
    }
}
