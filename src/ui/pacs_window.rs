//! PACS: server list, study query (C-FIND / QIDO-RS), retrieval (C-GET / C-MOVE / WADO-RS).

use super::icons;
use super::panels::{chip, search_box};
use super::theme::*;
use super::widgets::*;
use crate::app::{SlycedApp, Target, ToastKind, Workspace};
use crate::config::{PacsNode, PacsProtocol, RetrieveMethod, Settings, WebAuth};
use crate::dicom::model::{format_date, format_person_name, format_time};
use crate::pacs::scu::{RetrieveProgress, SeriesResult, StudyQuery, StudyResult};
use egui::{vec2, Align, Frame, Margin, RichText, Sense, Stroke, Ui};
use egui_extras::{Column, TableBuilder};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DateRange {
    Today,
    Yesterday,
    Week,
    Month,
    Year,
    All,
    Custom,
}

impl DateRange {
    const ALL: [DateRange; 7] = [DateRange::Today, DateRange::Yesterday, DateRange::Week, DateRange::Month, DateRange::Year, DateRange::All, DateRange::Custom];

    fn label(self) -> &'static str {
        match self {
            DateRange::Today => "Hoje",
            DateRange::Yesterday => "Ontem",
            DateRange::Week => "7 dias",
            DateRange::Month => "30 dias",
            DateRange::Year => "1 ano",
            DateRange::All => "Todos",
            DateRange::Custom => "Período…",
        }
    }
}

const MODALITIES: [&str; 10] = ["", "CT", "MR", "CR", "DX", "US", "MG", "NM", "PT", "XA"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Patient,
    Date,
    Modality,
    Description,
    Images,
}

/// Downloads that run at the same time; the others wait in the queue.
const PARALLEL_TRANSFERS: usize = 2;

pub struct Transfer {
    pub id: u64,
    pub label: String,
    pub study_uid: String,
    pub progress: RetrieveProgress,
    pub done: bool,
    pub error: Option<String>,
    pub cancel: Arc<AtomicBool>,
    pub open_when_done: bool,
    /// Waiting for a free slot.
    pub queued: bool,
    /// Server and series asked for, to send the request (again).
    node: PacsNode,
    series_uid: Option<String>,
}

impl Transfer {
    pub fn running(&self) -> bool {
        !self.done && !self.queued
    }

    pub fn fraction(&self) -> f32 {
        let p = self.progress;
        let total = (p.completed + p.remaining + p.failed).max(p.received);
        if self.done {
            1.0
        } else if total > 0 {
            p.completed.max(p.received) as f32 / total as f32
        } else {
            0.0
        }
    }
}

pub struct PacsUi {
    pub open: bool,
    pub node: usize,
    pub query: StudyQuery,
    pub range: DateRange,
    pub custom_from: String,
    pub custom_to: String,
    pub results: Vec<StudyResult>,
    /// Study UID of the selected result.
    pub selected: Option<String>,
    pub series: HashMap<String, Vec<SeriesResult>>,
    /// Series ticked for download in the selected study.
    pub checked: HashSet<String>,
    pub searching: bool,
    pub search_started: f64,
    /// Number of results and seconds taken by the last search.
    pub last_search: Option<(usize, f64)>,
    pub error: Option<String>,
    pub filter: String,
    pub sort: (SortKey, bool),
    /// Last connection test of each server (by name): time in ms or the error.
    pub node_status: HashMap<String, Result<u128, String>>,
    pub transfers: Vec<Transfer>,
    pub editing: Option<(Option<usize>, PacsNode)>,
    next_id: u64,
}

impl PacsUi {
    pub fn new(_settings: &Settings) -> Self {
        Self {
            open: false,
            node: 0,
            query: StudyQuery::default(),
            range: DateRange::Week,
            custom_from: String::new(),
            custom_to: String::new(),
            results: Vec::new(),
            selected: None,
            series: HashMap::new(),
            checked: HashSet::new(),
            searching: false,
            search_started: 0.0,
            last_search: None,
            error: None,
            filter: String::new(),
            sort: (SortKey::Date, true),
            node_status: HashMap::new(),
            transfers: Vec::new(),
            editing: None,
            next_id: 1,
        }
    }

    /// Orders the results by the chosen column (descending = newest/largest first).
    pub fn sort_results(&mut self) {
        let (key, desc) = self.sort;
        self.results.sort_by(|a, b| {
            let o = match key {
                SortKey::Patient => format_person_name(&a.patient_name).to_lowercase().cmp(&format_person_name(&b.patient_name).to_lowercase()),
                SortKey::Date => (a.date.as_str(), a.time.as_str()).cmp(&(b.date.as_str(), b.time.as_str())),
                SortKey::Modality => a.modalities.cmp(&b.modalities),
                SortKey::Description => a.description.to_lowercase().cmp(&b.description.to_lowercase()),
                SortKey::Images => a.instance_count.unwrap_or(0).cmp(&b.instance_count.unwrap_or(0)),
            };
            if desc { o.reverse() } else { o }
        });
    }
}

/// Civil date `days` before today as `AAAAMMDD` (UTC based, adequate for date filters).
fn date_days_ago(days: i64) -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let z = secs / 86_400 - days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}")
}

/// `DD/MM/AAAA` (or already `AAAAMMDD`) -> `AAAAMMDD`.
fn parse_br_date(s: &str) -> String {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 8 && s.contains('/') {
        format!("{}{}{}", &digits[4..8], &digits[2..4], &digits[0..2])
    } else {
        digits
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Labelled text field of the search form.
fn field(ui: &mut Ui, label: &str, value: &mut String, hint: &str, width: f32) {
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.label(RichText::new(label).size(12.0).color(pal().text_dim));
        search_box(ui, value, hint);
    });
}

impl SlycedApp {
    pub fn pacs_window(&mut self, ctx: &egui::Context) {
        if !self.pacs.open {
            return;
        }
        let mut open = self.pacs.open;
        egui::Window::new(RichText::new(format!("{}  PACS — pesquisa e download de exames", icons::NETWORK)).font(font_bold(15.0)))
            .open(&mut open)
            .default_size(vec2(1240.0, 760.0))
            .min_width(900.0)
            .min_height(520.0)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(236.0);
                        self.pacs_nodes(ui);
                    });
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        self.pacs_query(ui);
                    });
                });
            });
        self.pacs.open = open;
        self.pacs_node_editor(ctx);
    }

    fn pacs_nodes(&mut self, ui: &mut Ui) {
        caption(ui, "Servidores");
        let mut echo = None;
        let mut edit = None;
        let mut remove = None;
        if self.settings.nodes.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("Nenhum servidor cadastrado.").color(pal().text_dim).size(12.5));
                ui.label(RichText::new("Adicione o PACS do hospital (AE Title, IP e porta) ou um endereço DICOMweb.").color(pal().text_faint).size(11.5));
            });
        }
        for (i, n) in self.settings.nodes.iter().enumerate() {
            let selected = self.pacs.node == i;
            let status = self.pacs.node_status.get(&n.name);
            let resp = Frame::new()
                .fill(if selected { pal().selected_bg } else { pal().bg_input })
                .stroke(Stroke::new(1.0, if selected { pal().accent } else { pal().border }))
                .corner_radius(9)
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icons::HARD_DRIVES).color(if selected { pal().accent } else { pal().text_dim }));
                        ui.add(egui::Label::new(RichText::new(&n.name).font(font_bold(13.0)).color(pal().text)).truncate());
                    });
                    ui.add(egui::Label::new(RichText::new(n.address()).size(11.5).color(pal().text_faint)).truncate());
                    ui.horizontal(|ui| {
                        badge(ui, n.badge(), pal().text_dim);
                        match status {
                            Some(Ok(ms)) => {
                                ui.label(RichText::new(format!("● {ms} ms")).size(11.0).color(SUCCESS)).on_hover_text("Último teste de conexão: OK");
                            }
                            Some(Err(e)) => {
                                ui.label(RichText::new("● falhou").size(11.0).color(DANGER)).on_hover_text(e.as_str());
                            }
                            None => {}
                        }
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if icon_button_sized(ui, icons::TRASH_SIMPLE, "Remover", false, 24.0).clicked() {
                                remove = Some(i);
                            }
                            if icon_button_sized(ui, icons::SLIDERS_HORIZONTAL, "Editar", false, 24.0).clicked() {
                                edit = Some(i);
                            }
                            let test = match n.protocol {
                                PacsProtocol::Dimse => "Testar conexão (C-ECHO)",
                                PacsProtocol::DicomWeb => "Testar conexão (QIDO-RS)",
                            };
                            if icon_button_sized(ui, icons::PLUGS_CONNECTED, test, false, 24.0).clicked() {
                                echo = Some(i);
                            }
                        });
                    });
                })
                .response;
            if ui.interact(resp.rect, ui.scope_id().with(("node", i)), egui::Sense::click()).clicked() && self.pacs.node != i {
                self.pacs.node = i;
                self.pacs.results.clear();
                self.pacs.selected = None;
                self.pacs.last_search = None;
                self.pacs.error = None;
            }
            ui.add_space(4.0);
        }
        ui.add_space(4.0);
        if ghost_button(ui, icons::PLUS, "Adicionar servidor").clicked() {
            self.pacs.editing = Some((None, PacsNode::default()));
        }
        if let Some(i) = echo {
            let node = self.settings.nodes[i].clone();
            self.toast(ToastKind::Info, format!("Testando {}…", node.name));
            self.jobs.echo(node, self.settings.local_ae.clone());
        }
        if let Some(i) = edit {
            self.pacs.editing = Some((Some(i), self.settings.nodes[i].clone()));
        }
        if let Some(i) = remove {
            self.settings.nodes.remove(i);
            self.pacs.node = self.pacs.node.min(self.settings.nodes.len().saturating_sub(1));
            self.settings.save(&self.paths.config);
        }

        ui.add_space(14.0);
        caption(ui, "Este computador");
        card(ui, |ui| {
            kv(ui, "AE Title", &self.settings.local_ae);
            kv(ui, "Porta", &self.settings.local_port.to_string());
            ui.add_space(4.0);
            let running = self.scp.is_some();
            let text = if running { "Receptor ativo" } else { "Receptor parado" };
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").color(if running { SUCCESS } else { pal().text_faint }));
                ui.label(RichText::new(text).size(12.0).color(pal().text_dim));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if running {
                        if ui.small_button("Parar").clicked() {
                            self.stop_scp();
                        }
                    } else if ui.small_button("Iniciar").clicked() {
                        self.start_scp();
                    }
                });
            });
            ui.label(RichText::new("Necessário só para C-MOVE. Cadastre este AE e a porta no PACS.").size(11.0).color(pal().text_faint));
        });
    }

    fn start_search(&mut self) {
        let Some(node) = self.settings.nodes.get(self.pacs.node).cloned() else { return };
        let mut q = self.pacs.query.clone();
        let (from, to) = match self.pacs.range {
            DateRange::Today => (date_days_ago(0), date_days_ago(0)),
            DateRange::Yesterday => (date_days_ago(1), date_days_ago(1)),
            DateRange::Week => (date_days_ago(7), String::new()),
            DateRange::Month => (date_days_ago(30), String::new()),
            DateRange::Year => (date_days_ago(365), String::new()),
            DateRange::All => (String::new(), String::new()),
            DateRange::Custom => (parse_br_date(&self.pacs.custom_from), parse_br_date(&self.pacs.custom_to)),
        };
        q.date_from = from;
        q.date_to = to;
        self.pacs.searching = true;
        self.pacs.search_started = self.time;
        self.pacs.error = None;
        self.pacs.series.clear();
        self.pacs.selected = None;
        self.jobs.find_studies(node, self.settings.local_ae.clone(), q);
    }

    /// Shows a study that is already on this computer.
    pub(crate) fn open_local_study(&mut self, uid: &str) {
        let Some(st) = self.library.studies.iter().find(|s| s.header.uid == uid) else { return };
        let Some(best) = st.series.iter().max_by_key(|s| s.image_count()).map(|s| s.uid.clone()) else { return };
        self.expanded.insert(uid.to_string());
        self.expanded_study = Some(uid.to_string());
        self.workspace = Workspace::Viewer;
        self.open_series(&best, Target::View(self.active.min(5)));
        self.pacs.open = false;
    }

    fn pacs_query(&mut self, ui: &mut Ui) {
        let has_node = self.settings.nodes.get(self.pacs.node).is_some();
        let node_name = self.settings.nodes.get(self.pacs.node).map(|n| n.name.clone()).unwrap_or_default();
        let mut search = false;

        // ---- search form
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 3.0 * 10.0) / 4.0;
                ui.spacing_mut().item_spacing.x = 10.0;
                field(ui, "Paciente", &mut self.pacs.query.patient_name, "Nome (parte do nome)", w);
                field(ui, "ID do paciente", &mut self.pacs.query.patient_id, "Prontuário / ID", w);
                field(ui, "Nº de acesso", &mut self.pacs.query.accession, "Accession", w);
                field(ui, "Descrição do estudo", &mut self.pacs.query.description, "Ex.: abdome", w);
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{}  Período", icons::CALENDAR_BLANK)).size(12.0).color(pal().text_dim));
                ui.add_space(4.0);
                for r in DateRange::ALL {
                    if chip(ui, r.label(), self.pacs.range == r, 54.0).clicked() {
                        self.pacs.range = r;
                    }
                }
                if self.pacs.range == DateRange::Custom {
                    ui.add_space(6.0);
                    ui.label(RichText::new("de").size(12.0).color(pal().text_dim));
                    ui.add(egui::TextEdit::singleline(&mut self.pacs.custom_from).hint_text("DD/MM/AAAA").desired_width(92.0));
                    ui.label(RichText::new("até").size(12.0).color(pal().text_dim));
                    ui.add(egui::TextEdit::singleline(&mut self.pacs.custom_to).hint_text("DD/MM/AAAA").desired_width(92.0));
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{}  Modalidade", icons::STACK)).size(12.0).color(pal().text_dim));
                ui.add_space(4.0);
                for m in MODALITIES {
                    let label = if m.is_empty() { "Todas" } else { m };
                    if chip(ui, label, self.pacs.query.modality == m, 40.0).clicked() {
                        self.pacs.query.modality = m.to_string();
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.add_enabled_ui(has_node && !self.pacs.searching, |ui| {
                        if primary_button(ui, icons::MAGNIFYING_GLASS, "Pesquisar").clicked() {
                            search = true;
                        }
                    });
                    if ghost_button(ui, icons::ARROW_COUNTER_CLOCKWISE, "Limpar").on_hover_text("Limpar os campos da pesquisa").clicked() {
                        self.pacs.query = StudyQuery::default();
                        self.pacs.range = DateRange::Week;
                        self.pacs.custom_from.clear();
                        self.pacs.custom_to.clear();
                    }
                });
            });
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter && has_node && !self.pacs.searching {
                search = true;
            }
        });
        if search {
            self.start_search();
        }

        // ---- status line
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if !has_node {
                ui.label(RichText::new(format!("{}  Cadastre um servidor PACS à esquerda para pesquisar.", icons::INFO)).color(WARNING).size(12.5));
            } else if self.pacs.searching {
                let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                spinner(ui.painter(), rect.center(), 6.0, self.time, pal().accent);
                ui.label(RichText::new(format!("Consultando {node_name}…")).color(pal().text_dim).size(12.5));
                ui.ctx().request_repaint();
            } else if let Some(e) = &self.pacs.error {
                ui.label(RichText::new(format!("{}  {e}", icons::WARNING_CIRCLE)).color(DANGER).size(12.5));
            } else if let Some((n, secs)) = self.pacs.last_search {
                ui.label(RichText::new(format!("{} em {node_name} · {secs:.1} s", plural(n, "estudo encontrado", "estudos encontrados"))).color(pal().text_dim).size(12.5));
            } else {
                ui.label(RichText::new("Preencha os filtros e pressione Enter ou Pesquisar.").color(pal().text_faint).size(12.5));
            }
            if !self.pacs.results.is_empty() {
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.scope(|ui| {
                        ui.set_width(240.0);
                        search_box(ui, &mut self.pacs.filter, "Filtrar resultados…");
                    });
                });
            }
        });
        ui.add_space(4.0);

        // ---- results and details
        let transfers_h = if self.pacs.transfers.is_empty() { 0.0 } else { (34.0 + 26.0 * self.pacs.transfers.len().min(4) as f32).min(140.0) };
        let area_h = (ui.available_height() - transfers_h - 8.0).max(220.0);
        let detail_w = 320.0;
        let filter = self.pacs.filter.trim().to_lowercase();
        let rows: Vec<usize> = (0..self.pacs.results.len())
            .filter(|&i| {
                let r = &self.pacs.results[i];
                filter.is_empty() || format!("{} {} {} {}", r.patient_name, r.patient_id, r.description, r.accession).to_lowercase().contains(&filter)
            })
            .collect();
        let local: HashSet<&str> = self.library.studies.iter().map(|s| s.header.uid.as_str()).collect();
        let local: HashSet<String> = self.pacs.results.iter().filter(|r| local.contains(r.study_uid.as_str())).map(|r| r.study_uid.clone()).collect();
        let mut clicked_row = None;
        let mut retrieve_row = None;
        let mut open_row = None;
        let mut sort_by = None;
        ui.horizontal_top(|ui| {
            let table_w = (ui.available_width() - detail_w - 10.0).max(420.0);
            ui.vertical(|ui| {
                ui.set_width(table_w);
                Frame::new().fill(pal().bg_input).stroke(Stroke::new(1.0, pal().border)).corner_radius(10).inner_margin(Margin::same(6)).show(ui, |ui| {
                    ui.set_height(area_h - 14.0);
                    ui.set_width(table_w - 14.0);
                    if self.pacs.results.is_empty() {
                        ui.add_space(area_h * 0.3);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(icons::MAGNIFYING_GLASS).size(30.0).color(pal().text_faint));
                            let msg = if self.pacs.last_search.is_some() { "Nenhum estudo encontrado com esses filtros." } else { "Os estudos encontrados aparecem aqui." };
                            ui.label(RichText::new(msg).color(pal().text_dim));
                            if self.pacs.last_search.is_some() {
                                ui.label(RichText::new("Tente ampliar o período ou usar só parte do nome.").size(12.0).color(pal().text_faint));
                            }
                        });
                        return;
                    }
                    let (key, desc) = self.pacs.sort;
                    let arrow = |k: SortKey| if k == key { if desc { icons::SORT_DESCENDING } else { icons::SORT_ASCENDING } } else { "" };
                    TableBuilder::new(ui)
                        .id_salt("pacs-results")
                        .striped(true)
                        .sense(egui::Sense::click())
                        .cell_layout(egui::Layout::left_to_right(Align::Center))
                        .column(Column::initial(210.0).at_least(140.0).clip(true))
                        .column(Column::initial(96.0))
                        .column(Column::initial(76.0).clip(true))
                        .column(Column::remainder().at_least(120.0).clip(true))
                        .column(Column::exact(70.0))
                        .column(Column::exact(96.0))
                        .max_scroll_height(area_h - 50.0)
                        .header(26.0, |mut h| {
                            for (title, k) in [
                                ("Paciente", Some(SortKey::Patient)),
                                ("Data", Some(SortKey::Date)),
                                ("Mod.", Some(SortKey::Modality)),
                                ("Descrição", Some(SortKey::Description)),
                                ("Imagens", Some(SortKey::Images)),
                                ("", None),
                            ] {
                                h.col(|ui| {
                                    let text = match k {
                                        Some(k) => format!("{title} {}", arrow(k)),
                                        None => title.to_string(),
                                    };
                                    let r = ui.add(egui::Label::new(RichText::new(text).font(font_bold(12.0)).color(pal().text_faint)).sense(Sense::click()));
                                    if let Some(k) = k
                                        && r.on_hover_text("Ordenar").clicked() {
                                            sort_by = Some(k);
                                        }
                                });
                            }
                        })
                        .body(|body| {
                            body.rows(44.0, rows.len(), |mut row| {
                                let i = rows[row.index()];
                                let r = &self.pacs.results[i];
                                let is_local = local.contains(&r.study_uid);
                                let transfer = self.pacs.transfers.iter().rev().find(|t| t.study_uid == r.study_uid);
                                row.set_selected(self.pacs.selected.as_deref() == Some(r.study_uid.as_str()));
                                row.col(|ui| {
                                    ui.vertical(|ui| {
                                        ui.add_space(3.0);
                                        ui.add(egui::Label::new(RichText::new(format_person_name(&r.patient_name)).font(font_bold(12.5)).color(pal().text)).truncate());
                                        let mut info = vec![];
                                        if !r.patient_id.is_empty() {
                                            info.push(format!("ID {}", r.patient_id));
                                        }
                                        if !r.sex.is_empty() {
                                            info.push(r.sex.clone());
                                        }
                                        if !r.birth_date.is_empty() {
                                            info.push(format!("nasc. {}", format_date(&r.birth_date)));
                                        }
                                        ui.add(egui::Label::new(RichText::new(info.join(" · ")).size(11.0).color(pal().text_faint)).truncate());
                                    });
                                });
                                row.col(|ui| {
                                    ui.vertical(|ui| {
                                        ui.add_space(3.0);
                                        ui.label(RichText::new(format_date(&r.date)).size(12.5).color(pal().text));
                                        ui.label(RichText::new(format_time(&r.time)).size(11.0).color(pal().text_faint));
                                    });
                                });
                                row.col(|ui| {
                                    for m in r.modalities.split(['\\', ' ']).filter(|m| !m.is_empty()).take(2) {
                                        badge(ui, m, modality_color(m));
                                    }
                                });
                                row.col(|ui| {
                                    ui.vertical(|ui| {
                                        ui.add_space(3.0);
                                        let d = if r.description.is_empty() { "—" } else { &r.description };
                                        ui.add(egui::Label::new(RichText::new(d).size(12.5).color(pal().text_dim)).truncate());
                                        if !r.accession.is_empty() {
                                            ui.label(RichText::new(format!("Acesso {}", r.accession)).size(11.0).color(pal().text_faint));
                                        }
                                    });
                                });
                                row.col(|ui| {
                                    let n = r.instance_count.map(|n| n.to_string()).unwrap_or_else(|| "–".into());
                                    let s = r.series_count.map(|n| format!("{n} sér.")).unwrap_or_default();
                                    ui.vertical(|ui| {
                                        ui.add_space(3.0);
                                        ui.label(RichText::new(n).size(12.5).color(pal().text_dim));
                                        ui.label(RichText::new(s).size(11.0).color(pal().text_faint));
                                    });
                                });
                                row.col(|ui| match transfer.filter(|t| !t.done) {
                                    Some(t) => {
                                        let (rect, _) = ui.allocate_exact_size(vec2(80.0, 6.0), Sense::hover());
                                        progress_bar(ui.painter(), rect, t.fraction());
                                    }
                                    None if is_local => {
                                        if ui.add(egui::Button::new(RichText::new(format!("{}  Abrir", icons::ARROW_SQUARE_OUT)).size(12.0))).on_hover_text("Já está neste computador").clicked() {
                                            open_row = Some(i);
                                        }
                                    }
                                    None => {
                                        if ui.add(egui::Button::new(RichText::new(format!("{}  Baixar", icons::DOWNLOAD_SIMPLE)).size(12.0))).on_hover_text("Baixar e abrir o estudo").clicked() {
                                            retrieve_row = Some(i);
                                        }
                                    }
                                });
                                if row.response().clicked() {
                                    clicked_row = Some(i);
                                }
                                if row.response().double_clicked() {
                                    if is_local {
                                        open_row = Some(i);
                                    } else {
                                        retrieve_row = Some(i);
                                    }
                                }
                            });
                        });
                });
            });
            ui.vertical(|ui| {
                ui.set_width(detail_w);
                self.pacs_details(ui, area_h, &local);
            });
        });
        if let Some(k) = sort_by {
            self.pacs.sort = if self.pacs.sort.0 == k { (k, !self.pacs.sort.1) } else { (k, matches!(k, SortKey::Date | SortKey::Images)) };
            self.pacs.sort_results();
        }
        if let Some(i) = clicked_row {
            let uid = self.pacs.results[i].study_uid.clone();
            if self.pacs.selected.as_deref() != Some(uid.as_str()) {
                self.pacs.checked.clear();
            }
            self.pacs.selected = Some(uid.clone());
            if !self.pacs.series.contains_key(&uid)
                && let Some(node) = self.settings.nodes.get(self.pacs.node).cloned() {
                    self.pacs.series.insert(uid.clone(), Vec::new());
                    self.jobs.find_series(node, self.settings.local_ae.clone(), uid);
                }
        }
        if let Some(i) = retrieve_row {
            let r = self.pacs.results[i].clone();
            self.start_retrieve(&r, None);
        }
        if let Some(i) = open_row {
            let uid = self.pacs.results[i].study_uid.clone();
            self.open_local_study(&uid);
        }
        self.pacs_transfers(ui, transfers_h);
    }

    /// Selected study: patient data, its series (tick to download only some) and actions.
    fn pacs_details(&mut self, ui: &mut Ui, height: f32, local: &HashSet<String>) {
        Frame::new().fill(pal().bg_raised).stroke(Stroke::new(1.0, pal().border)).corner_radius(10).inner_margin(Margin::same(12)).show(ui, |ui| {
            ui.set_height(height - 26.0);
            ui.set_width(ui.available_width());
            let Some(sel) = self.pacs.selected.clone().and_then(|u| self.pacs.results.iter().find(|r| r.study_uid == u).cloned()) else {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(icons::CLIPBOARD_TEXT).size(26.0).color(pal().text_faint));
                    ui.label(RichText::new("Selecione um estudo para ver as séries.").color(pal().text_faint).size(12.5));
                    ui.label(RichText::new("Duplo clique baixa e abre.").color(pal().text_faint).size(11.5));
                });
                return;
            };
            ui.label(RichText::new(format_person_name(&sel.patient_name)).font(font_bold(14.0)).color(pal().text));
            ui.label(RichText::new(format!("{} {}", format_date(&sel.date), format_time(&sel.time))).size(12.0).color(pal().text_dim));
            ui.add_space(6.0);
            for (k, v) in [
                ("ID", sel.patient_id.clone()),
                ("Nascimento", format_date(&sel.birth_date)),
                ("Sexo", sel.sex.clone()),
                ("Acesso", sel.accession.clone()),
                ("Descrição", sel.description.clone()),
                ("Solicitante", format_person_name(&sel.referring)),
            ] {
                if !v.trim().is_empty() {
                    kv(ui, k, &v);
                }
            }
            ui.add_space(8.0);
            let list = self.pacs.series.get(&sel.study_uid).cloned().unwrap_or_default();
            ui.horizontal(|ui| {
                caption(ui, &format!("Séries ({})", list.len()));
                if !list.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let all = list.iter().all(|s| self.pacs.checked.contains(&s.series_uid));
                        if ui.small_button(if all { "Nenhuma" } else { "Todas" }).clicked() {
                            if all {
                                self.pacs.checked.clear();
                            } else {
                                self.pacs.checked = list.iter().map(|s| s.series_uid.clone()).collect();
                            }
                        }
                    });
                }
            });
            let is_local = local.contains(&sel.study_uid);
            let buttons_h = if is_local { 86.0 } else { 46.0 };
            egui::ScrollArea::vertical().id_salt("pacs-series").max_height((ui.available_height() - buttons_h).max(60.0)).show(ui, |ui| {
                if list.is_empty() {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        spinner(ui.painter(), rect.center(), 5.0, self.time, pal().accent);
                        ui.label(RichText::new("Carregando séries…").size(12.0).color(pal().text_faint));
                    });
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                }
                for s in &list {
                    let mut on = self.pacs.checked.contains(&s.series_uid);
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut on, "").changed() {
                            if on {
                                self.pacs.checked.insert(s.series_uid.clone());
                            } else {
                                self.pacs.checked.remove(&s.series_uid);
                            }
                        }
                        badge(ui, &s.modality, modality_color(&s.modality));
                        let title = format!("{} {}", s.number.map(|n| format!("{n} ·")).unwrap_or_default(), if s.description.is_empty() { "Sem descrição" } else { &s.description });
                        ui.add(egui::Label::new(RichText::new(title).size(12.0).color(pal().text)).truncate());
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(s.instance_count.map(|n| n.to_string()).unwrap_or_default()).size(11.5).color(pal().text_faint));
                        });
                    });
                }
            });
            ui.add_space(6.0);
            if is_local && primary_button(ui, icons::ARROW_SQUARE_OUT, "Abrir no Slyced").clicked() {
                self.open_local_study(&sel.study_uid);
            }
            ui.horizontal(|ui| {
                let label = if is_local { "Baixar de novo" } else { "Baixar estudo" };
                if !is_local {
                    if primary_button(ui, icons::DOWNLOAD_SIMPLE, label).clicked() {
                        self.start_retrieve(&sel, None);
                    }
                } else if ghost_button(ui, icons::DOWNLOAD_SIMPLE, label).clicked() {
                    self.start_retrieve(&sel, None);
                }
                let n = list.iter().filter(|s| self.pacs.checked.contains(&s.series_uid)).count();
                ui.add_enabled_ui(n > 0, |ui| {
                    if ghost_button(ui, icons::CHECK_SQUARE, &format!("Só as marcadas ({n})")).clicked() {
                        let picked: Vec<SeriesResult> = list.iter().filter(|s| self.pacs.checked.contains(&s.series_uid)).cloned().collect();
                        for s in &picked {
                            self.start_retrieve(&sel, Some(s));
                        }
                        self.pacs.checked.clear();
                    }
                });
            });
        });
    }

    fn pacs_transfers(&mut self, ui: &mut Ui, height: f32) {
        if self.pacs.transfers.is_empty() {
            return;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            caption(ui, "Transferências");
            if self.pacs.transfers.iter().any(|t| t.done) {
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Limpar concluídas").clicked() {
                        self.pacs.transfers.retain(|t| !t.done);
                    }
                });
            }
        });
        egui::ScrollArea::vertical().id_salt("transfers").max_height(height - 30.0).show(ui, |ui| {
            self.transfer_rows(ui);
        });
    }

    pub(crate) fn start_retrieve(&mut self, study: &StudyResult, series: Option<&SeriesResult>) {
        let Some(node) = self.settings.nodes.get(self.pacs.node).cloned() else { return };
        if node.protocol == PacsProtocol::Dimse && node.retrieve == RetrieveMethod::CMove && self.scp.is_none() {
            self.start_scp();
            if self.scp.is_none() {
                return;
            }
        }
        let id = self.pacs.next_id;
        self.pacs.next_id += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let mut label = format_person_name(&study.patient_name);
        if let Some(s) = series {
            label.push_str(&format!(" · série {}", s.description));
        } else if !study.description.is_empty() {
            label.push_str(&format!(" · {}", study.description));
        }
        self.pacs.transfers.push(Transfer {
            id,
            label,
            study_uid: study.study_uid.clone(),
            progress: RetrieveProgress::default(),
            done: false,
            error: None,
            cancel,
            open_when_done: true,
            queued: true,
            node,
            series_uid: series.map(|s| s.series_uid.clone()),
        });
        self.pump_transfers();
    }

    /// Starts queued downloads, oldest first, while there are free slots.
    pub(crate) fn pump_transfers(&mut self) {
        let mut running = self.pacs.transfers.iter().filter(|t| t.running()).count();
        for i in 0..self.pacs.transfers.len() {
            if running >= PARALLEL_TRANSFERS {
                break;
            }
            let t = &mut self.pacs.transfers[i];
            if !t.queued {
                continue;
            }
            t.queued = false;
            running += 1;
            let (id, node, study, series, cancel) = (t.id, t.node.clone(), t.study_uid.clone(), t.series_uid.clone(), t.cancel.clone());
            self.jobs.retrieve(id, node, self.settings.local_ae.clone(), study, series, self.paths.store.clone(), cancel);
        }
    }

    /// Sends a failed or cancelled download again (it goes back to the queue).
    fn retry_transfer(&mut self, id: u64) {
        let new_id = self.pacs.next_id;
        let Some(t) = self.pacs.transfers.iter_mut().find(|t| t.id == id) else { return };
        self.pacs.next_id += 1;
        t.id = new_id;
        t.progress = RetrieveProgress::default();
        t.done = false;
        t.error = None;
        t.queued = true;
        t.cancel = Arc::new(AtomicBool::new(false));
        self.pump_transfers();
    }

    fn cancel_transfer(&mut self, id: u64) {
        let Some(t) = self.pacs.transfers.iter_mut().find(|t| t.id == id) else { return };
        if t.queued {
            t.queued = false;
            t.done = true;
            t.error = Some("cancelada".into());
            self.compare_after_retrieve.remove(&t.study_uid);
        } else {
            t.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Opens a downloaded exam (or as soon as its files are read).
    fn open_transferred(&mut self, study_uid: &str) {
        let best = self
            .library
            .studies
            .iter()
            .find(|st| st.header.uid == study_uid)
            .and_then(|st| st.series.iter().max_by_key(|s| s.image_count()).map(|s| s.uid.clone()));
        match best {
            Some(series) => {
                self.workspace = Workspace::Viewer;
                self.expanded.insert(study_uid.to_string());
                self.open_series(&series, Target::View(self.active));
                self.pacs.open = false;
            }
            None => {
                self.open_after_scan = Some(study_uid.to_string());
                self.toast(ToastKind::Info, "O exame abre assim que terminar de ser lido.");
            }
        }
    }

    /// Rows of the download list (PACS window and status bar menu).
    pub(crate) fn transfer_rows(&mut self, ui: &mut Ui) {
        enum Act {
            Cancel(u64),
            Retry(u64),
            Open(String),
        }
        let mut act = None;
        for t in self.pacs.transfers.iter().rev() {
            ui.horizontal(|ui| {
                let p = t.progress;
                let total = (p.completed + p.remaining + p.failed).max(p.received);
                let (icon, color) = match (&t.error, t.done) {
                    (Some(_), _) => (icons::WARNING_CIRCLE, DANGER),
                    (None, true) => (icons::CHECK_CIRCLE, SUCCESS),
                    _ if t.queued => (icons::HOURGLASS, pal().text_faint),
                    _ => (icons::CLOUD_ARROW_DOWN, pal().accent),
                };
                ui.label(RichText::new(icon).color(color));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    match (&t.error, t.done) {
                        (Some(_), _) => {
                            if ui.small_button(format!("{}  Tentar de novo", icons::ARROWS_CLOCKWISE)).clicked() {
                                act = Some(Act::Retry(t.id));
                            }
                        }
                        (None, true) => {
                            if ui.small_button(format!("{}  Abrir", icons::ARROW_SQUARE_OUT)).clicked() {
                                act = Some(Act::Open(t.study_uid.clone()));
                            }
                        }
                        _ => {
                            if icon_button_sized(ui, icons::X, "Cancelar", false, 22.0).clicked() {
                                act = Some(Act::Cancel(t.id));
                            }
                        }
                    }
                    let status = match &t.error {
                        Some(e) => e.clone(),
                        None if t.done => format!("concluída · {}", plural(p.completed.max(p.received) as usize, "imagem", "imagens")),
                        None if t.queued => "na fila".to_string(),
                        None => format!("{} / {}", p.completed.max(p.received), total),
                    };
                    ui.add(egui::Label::new(RichText::new(status).size(11.5).color(pal().text_dim)).truncate());
                    if t.running() {
                        let (rect, _) = ui.allocate_exact_size(vec2(110.0, 6.0), Sense::hover());
                        progress_bar(ui.painter(), rect, t.fraction());
                    }
                    // The name fills what is left, aligned next to the icon.
                    ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(&t.label).size(12.5).color(pal().text)).truncate());
                    });
                });
            });
        }
        match act {
            Some(Act::Cancel(id)) => self.cancel_transfer(id),
            Some(Act::Retry(id)) => self.retry_transfer(id),
            Some(Act::Open(uid)) => self.open_transferred(&uid),
            None => {}
        }
    }

    fn pacs_node_editor(&mut self, ctx: &egui::Context) {
        let Some((index, mut node)) = self.pacs.editing.take() else { return };
        let mut keep = true;
        let mut save = false;
        egui::Modal::new(egui::Id::unique("node-editor")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new(if index.is_some() { "Editar servidor" } else { "Novo servidor PACS" }).font(font_bold(16.0)));
            ui.add_space(10.0);
            egui::Grid::new("node-form").num_columns(2).spacing(vec2(12.0, 10.0)).show(ui, |ui| {
                ui.label("Nome");
                ui.add(egui::TextEdit::singleline(&mut node.name).desired_width(240.0));
                ui.end_row();
                ui.label("Protocolo");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut node.protocol, PacsProtocol::Dimse, "DICOM (DIMSE)");
                    ui.selectable_value(&mut node.protocol, PacsProtocol::DicomWeb, "DICOMweb");
                });
                ui.end_row();
                match node.protocol {
                    PacsProtocol::Dimse => {
                        ui.label("AE Title");
                        ui.add(egui::TextEdit::singleline(&mut node.ae_title).desired_width(240.0).char_limit(16));
                        ui.end_row();
                        ui.label("Endereço");
                        ui.add(egui::TextEdit::singleline(&mut node.host).desired_width(240.0).hint_text("IP ou nome"));
                        ui.end_row();
                        ui.label("Porta");
                        ui.add(egui::DragValue::new(&mut node.port).range(1..=65535));
                        ui.end_row();
                        ui.label("Recuperação");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut node.retrieve, RetrieveMethod::CGet, "C-GET");
                            ui.selectable_value(&mut node.retrieve, RetrieveMethod::CMove, "C-MOVE");
                        });
                        ui.end_row();
                    }
                    PacsProtocol::DicomWeb => {
                        ui.label("URL base");
                        ui.add(
                            egui::TextEdit::singleline(&mut node.base_url)
                                .desired_width(240.0)
                                .hint_text("http://servidor:8042/dicom-web"),
                        );
                        ui.end_row();
                        ui.label("Autenticação");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut node.auth, WebAuth::None, "Nenhuma");
                            ui.selectable_value(&mut node.auth, WebAuth::Basic, "Usuário e senha");
                            ui.selectable_value(&mut node.auth, WebAuth::Bearer, "Token");
                        });
                        ui.end_row();
                        match node.auth {
                            WebAuth::None => {}
                            WebAuth::Basic => {
                                ui.label("Usuário");
                                ui.add(egui::TextEdit::singleline(&mut node.username).desired_width(240.0));
                                ui.end_row();
                                ui.label("Senha");
                                ui.add(egui::TextEdit::singleline(&mut node.secret).desired_width(240.0).password(true));
                                ui.end_row();
                            }
                            WebAuth::Bearer => {
                                ui.label("Token");
                                ui.add(egui::TextEdit::singleline(&mut node.secret).desired_width(240.0).password(true));
                                ui.end_row();
                            }
                        }
                    }
                }
            });
            ui.add_space(6.0);
            let help = match (node.protocol, node.retrieve) {
                (PacsProtocol::Dimse, RetrieveMethod::CGet) => {
                    "C-GET recebe as imagens pela mesma conexão: não exige porta de entrada nem liberação no firewall."
                }
                (PacsProtocol::Dimse, RetrieveMethod::CMove) => {
                    "C-MOVE: o PACS envia as imagens para este computador. Cadastre o AE local e a porta no servidor."
                }
                (PacsProtocol::DicomWeb, _) => {
                    "DICOMweb usa HTTP(S): pesquisa por QIDO-RS e download por WADO-RS, sem porta de entrada. No Orthanc, a URL costuma terminar em /dicom-web."
                }
            };
            ui.label(RichText::new(help).size(11.5).color(pal().text_faint));
            if node.protocol == PacsProtocol::DicomWeb && node.auth != WebAuth::None {
                ui.label(RichText::new("A senha/token fica gravada em config.json, na pasta de dados do Slyced.").size(11.5).color(pal().text_faint));
            }
            let url = node.base_url.trim().to_ascii_lowercase();
            let valid = node.protocol == PacsProtocol::Dimse || url.starts_with("http://") || url.starts_with("https://");
            if !valid {
                ui.label(RichText::new("A URL deve começar com http:// ou https://").size(11.5).color(WARNING));
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.add_enabled_ui(valid, |ui| primary_button(ui, icons::CHECK, "Salvar")).inner.clicked() {
                    save = true;
                    keep = false;
                }
                if ghost_button(ui, "", "Cancelar").clicked() {
                    keep = false;
                }
            });
        });
        node.ae_title = node.ae_title.trim().to_uppercase();
        if save {
            node.base_url = node.base_url.trim().trim_end_matches('/').to_string();
            match index {
                Some(i) if i < self.settings.nodes.len() => self.settings.nodes[i] = node,
                _ => {
                    self.settings.nodes.push(node);
                    self.pacs.node = self.settings.nodes.len() - 1;
                }
            }
            self.settings.save(&self.paths.config);
        } else if keep {
            self.pacs.editing = Some((index, node));
        }
    }
}
