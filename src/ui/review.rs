//! Reading aids: findings (bookmarks), hanging protocols, curved reformation,
//! thick slab, noise filter and magnifier.

use super::compare::CompareDisplay;
use super::icons;
use super::panels::{chip, full_slider, labeled_slider};
use super::theme::*;
use super::viewport::{Change, Content, Hit, Pin, Tool, Viewport};
use super::widgets::*;
use crate::app::{SlycedApp, Target, ToastKind, Workspace};
use crate::bookmarks::{self, Bookmark};
use crate::dicom::series::LoadedSeries;
use crate::imaging::cpr;
use crate::imaging::highlight;
use crate::imaging::lut::{Colormap, Window};
use crate::imaging::reslice::SlabMode;
use crate::protocols::{self, PROTOCOLS};
use egui::{Align, Color32, RichText, Ui};
use glam::DVec3;
use std::sync::Arc;

/// Color of the findings markers.
pub const FINDING: Color32 = Color32::from_rgb(255, 92, 150);

impl SlycedApp {
    fn bookmarks_path(&self) -> std::path::PathBuf {
        self.paths.root.join("achados.json")
    }

    pub(crate) fn load_bookmarks(&mut self) {
        self.bookmarks = bookmarks::load(&self.bookmarks_path());
    }

    fn save_bookmarks(&self) {
        bookmarks::save(&self.bookmarks_path(), &self.bookmarks);
    }

    /// Findings of `series`' study that can be shown on its images.
    pub(crate) fn bookmark_pins(&self, series: &LoadedSeries) -> Vec<Pin> {
        let fr = &series.info.frame_of_reference;
        self.bookmarks
            .iter()
            .filter(|b| b.study_uid == series.study.uid)
            .filter_map(|b| {
                let same_frame = b.series_uid == series.uid || (!fr.is_empty() && *fr == b.frame_of_ref);
                match b.point() {
                    Some(p) if same_frame => Some(Pin { id: b.id, label: b.label.clone(), color: FINDING, point: Some(p), image: None }),
                    None if b.series_uid == series.uid => Some(Pin {
                        id: b.id,
                        label: b.label.clone(),
                        color: FINDING,
                        point: None,
                        image: Some((b.slice, egui::pos2(b.pixel[0], b.pixel[1]))),
                    }),
                    _ => None,
                }
            })
            .collect()
    }

    pub(crate) fn add_bookmark(&mut self, series: &Arc<LoadedSeries>, hit: Hit, window: Window, zoom: Option<f32>, mpr: bool) {
        let n = self.bookmarks.iter().filter(|b| b.study_uid == series.study.uid).count() + 1;
        let id = self.bookmarks.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        self.bookmarks.push(Bookmark {
            id,
            label: trf!("Achado {n}", "Finding {n}"),
            note: String::new(),
            study_uid: series.study.uid.clone(),
            series_uid: series.uid.clone(),
            frame_of_ref: series.info.frame_of_reference.clone(),
            point: hit.patient.map(|p| p.to_array()),
            slice: hit.slice,
            pixel: [hit.pixel.x, hit.pixel.y],
            window: [window.center, window.width],
            zoom,
            mpr,
            created: bookmarks::now(),
        });
        self.bookmark_edit = Some(id);
        self.bookmark_focus = true;
        self.right_open = true;
        self.save_bookmarks();
    }

    /// Opens the series of a finding at its position, window and zoom.
    pub(crate) fn go_to_bookmark(&mut self, id: u64) {
        self.push_nav();
        let Some(b) = self.bookmarks.iter().find(|b| b.id == id).cloned() else { return };
        if self.library.find_series(&b.series_uid).is_none() {
            self.toast(
                ToastKind::Info,
                tr!("A série deste achado não está aberta. Abra o exame novamente.", "The series of this finding is not open. Open the study again."),
            );
            return;
        }
        self.pending_bookmark = Some(id);
        let target = if b.mpr && b.point.is_some() {
            Target::Mpr
        } else {
            self.workspace = Workspace::Viewer;
            let shown = self.views.iter().take(self.layout.count()).position(|v| v.content.series().is_some_and(|s| s.uid == b.series_uid));
            Target::View(shown.unwrap_or(self.active))
        };
        if let Target::View(i) = target {
            self.active = i;
        }
        self.open_series(&b.series_uid, target);
    }

    /// Applies a finding requested before its series finished loading.
    pub(crate) fn apply_pending_bookmark(&mut self, series: &Arc<LoadedSeries>) {
        let Some(id) = self.pending_bookmark else { return };
        let Some(b) = self.bookmarks.iter().find(|b| b.id == id && b.series_uid == series.uid).cloned() else { return };
        self.pending_bookmark = None;
        let window = Window::new(b.window[0], b.window[1]);
        match (self.workspace, b.point()) {
            (Workspace::Mpr, Some(p)) => {
                self.mpr.point = p;
                for v in self.mpr_views.iter_mut().take(3) {
                    v.window = window;
                }
            }
            _ => {
                let Some(v) = self.views.get_mut(self.active) else { return };
                match b.point() {
                    Some(p) => {
                        if !v.go_to_point(p) {
                            v.slice = b.slice;
                        }
                    }
                    None => v.slice = b.slice,
                }
                v.window = window;
                v.zoom_request = b.zoom;
                v.center_on = b.point();
            }
        }
        if let Some(p) = b.point() {
            self.cursor = Some((p, b.study_uid.clone(), b.frame_of_ref.clone()));
        }
    }

    pub(crate) fn inspector_findings(&mut self, ui: &mut Ui, series: &LoadedSeries) {
        let study = series.study.uid.clone();
        let items: Vec<Bookmark> = self.bookmarks.iter().filter(|b| b.study_uid == study).cloned().collect();
        if items.is_empty() && self.tool != Tool::Bookmark {
            return;
        }
        caption(ui, tr!("Achados", "Findings"));
        let mut go = None;
        let mut remove = None;
        let mut changed = false;
        card(ui, |ui| {
            if items.is_empty() {
                ui.label(RichText::new(tr!("Clique na imagem para marcar um achado.", "Click the image to mark a finding.")).color(pal().text_faint).size(12.0));
            }
            for b in &items {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icons::MAP_PIN).color(FINDING));
                    if self.bookmark_edit == Some(b.id) {
                        if let Some(item) = self.bookmarks.iter_mut().find(|x| x.id == b.id) {
                            let r = ui.add(egui::TextEdit::singleline(&mut item.label).desired_width(ui.available_width() - 60.0));
                            if self.bookmark_focus {
                                r.request_focus();
                                self.bookmark_focus = false;
                            }
                            if r.lost_focus() {
                                self.bookmark_edit = None;
                                changed = true;
                            }
                        }
                    } else {
                        let label = ui.add(egui::Label::new(RichText::new(&b.label).color(pal().text)).sense(egui::Sense::click()).truncate());
                        if label.clicked() {
                            go = Some(b.id);
                        }
                        if label.double_clicked() {
                            self.bookmark_edit = Some(b.id);
                            self.bookmark_focus = true;
                        }
                        label.on_hover_text(tr!("Clique para ir ao achado · duplo clique para renomear", "Click to go to the finding · double-click to rename"));
                    }
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if icon_button_sized(ui, icons::X, tr!("Remover", "Remove"), false, 22.0).clicked() {
                            remove = Some(b.id);
                        }
                        if icon_button_sized(ui, icons::CROSSHAIR_SIMPLE, tr!("Ir ao achado", "Go to finding"), false, 22.0).clicked() {
                            go = Some(b.id);
                        }
                    });
                });
            }
        });
        if let Some(id) = remove {
            self.bookmarks.retain(|b| b.id != id);
            changed = true;
        }
        if changed {
            self.save_bookmarks();
        }
        if let Some(id) = go {
            self.go_to_bookmark(id);
        }
    }

    // ------------------------------------------------------------------ protocols

    pub(crate) fn protocols_menu(&mut self, ui: &mut Ui) {
        ui.label(RichText::new(tr!("Protocolos de leitura", "Reading protocols")).small().color(pal().text_faint));
        let text = trf!("{}  Hérnia incisional (índice de Tanaka)", "{}  Incisional hernia (Tanaka index)", icons::CLIPBOARD_TEXT);
        let r = ui.add(egui::Button::new(RichText::new(text).color(pal().text)).min_size(egui::vec2(260.0, 26.0)));
        let clicked = r.clicked();
        r.on_hover_text(tr!(
            "Volumes do saco herniário e da cavidade abdominal, índice de Tanaka, razão de Sabbagh e largura do defeito (EHS).",
            "Hernia sac and abdominal cavity volumes, Tanaka index, Sabbagh ratio and fascial defect width (EHS)."
        ));
        if clicked {
            self.open_hernia();
            ui.close();
        }
        let text = trf!("{}  Fases: sem × com contraste (ΔHU)", "{}  Phases: non-contrast × contrast (ΔHU)", icons::COLUMNS);
        let r = ui.add(egui::Button::new(RichText::new(text).color(pal().text)).min_size(egui::vec2(260.0, 26.0)));
        let clicked = r.clicked();
        r.on_hover_text(tr!(
            "Abre as fases sem e com contraste lado a lado, alinhadas: a densidade (H) mostra o ΔHU e há mapa de realce e subtração.",
            "Opens the non-contrast and contrast phases side by side, aligned: point density (H) shows the ΔHU, plus an enhancement map and subtraction."
        ));
        if clicked {
            self.compare_phases();
            ui.close();
        }
        for (i, p) in PROTOCOLS.iter().enumerate() {
            let r = ui.add(egui::Button::new(RichText::new(format!("{}  {}", icons::CLIPBOARD_TEXT, p.name)).color(pal().text)).min_size(egui::vec2(260.0, 26.0)));
            let clicked = r.clicked();
            r.on_hover_text(p.detail.get());
            if clicked {
                self.apply_protocol(i);
                ui.close();
            }
        }
    }

    /// Opens the best series of the active study in MPR with the protocol's settings.
    pub(crate) fn apply_protocol(&mut self, i: usize) {
        let Some(p) = PROTOCOLS.get(i) else { return };
        let study_uid = self.active_series().map(|s| s.study.uid.clone()).or_else(|| self.expanded_study.clone()).or_else(|| self.library.studies.first().map(|s| s.header.uid.clone()));
        let Some(study) = study_uid.and_then(|u| self.library.studies.iter().find(|s| s.header.uid == u)) else {
            self.toast(ToastKind::Info, tr!("Abra um exame primeiro.", "Open a study first."));
            return;
        };
        let best = study.series.iter().filter_map(|s| protocols::score(p, s).map(|sc| (sc, s.uid.clone()))).max_by_key(|x| x.0);
        let Some((_, uid)) = best else {
            self.toast(
                ToastKind::Error,
                trf!("Protocolo {}: nenhuma série de TC com cortes suficientes neste exame.", "Protocol {}: no CT series with enough slices in this study.", p.name),
            );
            return;
        };
        self.pending_protocol = Some(i);
        self.open_series(&uid, Target::Mpr);
    }

    /// Called once the MPR is set up for a protocol's series.
    pub(crate) fn finish_protocol(&mut self) {
        let Some(i) = self.pending_protocol.take() else { return };
        let p = &PROTOCOLS[i];
        self.mpr.slab_mm = p.slab_mm;
        self.mpr.slab_mode = p.slab_mode;
        self.mpr.crosshair = true;
        self.mpr.reset_axes();
        for v in self.mpr_views.iter_mut().take(3) {
            v.window = p.window;
            v.zoom = None;
            v.pan = egui::Vec2::ZERO;
        }
        self.tool = Tool::Crosshair;
        self.maximized = None;
        self.active = 0;
        self.toast(ToastKind::Info, format!("{}: {}", p.name, p.detail));
    }

    // ------------------------------------------------------------------ tubular tracker (CPR)

    /// Adds a point of the path, clicked on an axial image (2D) or an MPR plane.
    pub(crate) fn curve_add(&mut self, hit: Hit, series: &Arc<LoadedSeries>) {
        let Some(p) = hit.patient else { return };
        if series.volume.is_none() {
            self.toast(ToastKind::Info, tr!("O rastreador precisa de uma série em cortes (com volume).", "The tube tracker needs a series of slices (with a volume)."));
            return;
        }
        if self.curve_series.as_ref().is_some_and(|s| !Arc::ptr_eq(s, series)) {
            self.curve_clear();
        }
        self.curve_series = Some(series.clone());
        if self.curve.last().is_some_and(|q| (*q - p).length() < 0.5) {
            return;
        }
        if self.curve.is_empty() {
            self.curve_normal = hit.normal.unwrap_or(DVec3::Y);
        }
        self.curve.push(p);
        if self.curve.len() >= 2 {
            self.rebuild_cpr();
        }
    }

    pub(crate) fn curve_undo(&mut self) {
        self.curve.pop();
        if self.curve.len() >= 2 {
            self.rebuild_cpr();
        } else {
            self.cpr_close();
        }
    }

    pub(crate) fn curve_clear(&mut self) {
        self.curve.clear();
        self.cpr_close();
        self.curve_series = None;
    }

    /// Removes the straightened image: the 3D view comes back in MPR, the
    /// previous content in the 2D viewer.
    pub(crate) fn cpr_close(&mut self) {
        if let Content::Cpr(s, _) = self.mpr_views[3].content.clone() {
            self.mpr_views[3].set_content(Content::Volume(s));
        }
        if let Some((i, prev)) = self.cpr_view.take()
            && matches!(self.views[i].content, Content::Cpr(..)) {
                self.views[i].set_content(prev);
            }
    }

    pub(crate) fn rebuild_cpr(&mut self) {
        let Some(series) = self.curve_series.clone().or_else(|| self.mpr_views[0].content.series().cloned()) else { return };
        let Some(vol) = series.volume.clone() else { return };
        let Some(img) = cpr::build(&vol, &self.curve, self.curve_normal, self.cpr_width as f64 * 0.5, self.cpr_angle as f64) else { return };
        let window = self.cpr_kind.window();
        let content = Content::Cpr(series, Arc::new(img));
        let v = if self.workspace == Workspace::Mpr {
            &mut self.mpr_views[3]
        } else {
            // 2D viewer: the straightened image goes to a second window.
            if self.cpr_view.as_ref().is_some_and(|(i, _)| !matches!(self.views[*i].content, Content::Cpr(..))) {
                self.cpr_view = None;
            }
            let i = match &self.cpr_view {
                Some((i, _)) => *i,
                None => {
                    if self.layout.count() == 1 {
                        self.layout = crate::app::Layout::Cols2;
                    }
                    let n = self.layout.count();
                    let i = (0..n)
                        .find(|&j| j != self.active && matches!(self.views[j].content, Content::Empty))
                        .unwrap_or((self.active + 1) % n);
                    self.cpr_view = Some((i, self.views[i].content.clone()));
                    i
                }
            };
            &mut self.views[i]
        };
        let was_cpr = matches!(v.content, Content::Cpr(..));
        let (zoom, pan) = (v.zoom, v.pan);
        v.set_content(content);
        if was_cpr {
            (v.zoom, v.pan) = (zoom, pan);
        }
        v.window = window;
    }

    /// Rotates the straightened image about the path (mouse wheel over it).
    pub(crate) fn rotate_cpr(&mut self, degrees: f32) {
        self.cpr_angle = (self.cpr_angle + degrees + 180.0).rem_euclid(360.0) - 180.0;
        if self.curve.len() >= 2 {
            self.rebuild_cpr();
        }
    }

    pub(crate) fn inspector_cpr(&mut self, ui: &mut Ui) {
        let active = self.tool == Tool::Curve || !self.curve.is_empty();
        caption(ui, tr!("Rastreador tubular (CPR)", "Tube tracker (CPR)"));
        card(ui, |ui| {
            if !active {
                let text = tr!(
                    "Dê 3 ou 4 cliques ao longo do apêndice, ureter, colédoco ou vaso e veja a estrutura inteira esticada em uma imagem.",
                    "Click 3 or 4 times along the appendix, ureter, common bile duct or vessel to see the whole structure straightened in one image."
                );
                ui.label(RichText::new(text).color(pal().text_faint).size(12.0));
                if ui.button(trf!("{}  Rastrear estrutura (V)", "{}  Trace structure (V)", icons::BEZIER_CURVE)).clicked() {
                    self.tool = Tool::Curve;
                }
                return;
            }
            ui.label(
                RichText::new(trf!(
                    "{} ponto(s). Clique ao longo da estrutura, rolando os cortes entre os cliques. Botão direito ou Enter conclui; a roda do mouse sobre a imagem esticada a gira.",
                    "{} point(s). Click along the structure, scrolling through the slices between clicks. Right-click or Enter finishes; the mouse wheel over the straightened image rotates it.",
                    self.curve.len()
                ))
                .color(pal().text_faint)
                .size(12.0),
            );
            let mut rebuild = false;
            let mut kind = self.cpr_kind;
            ui.horizontal_wrapped(|ui| {
                for k in TubeKind::ALL {
                    if chip(ui, k.label(), kind == k, 54.0).clicked() {
                        kind = k;
                    }
                }
            });
            if kind != self.cpr_kind {
                self.cpr_kind = kind;
                self.cpr_width = kind.width_mm();
                rebuild = true;
            }
            let before = (self.cpr_angle, self.cpr_width);
            labeled_slider(ui, tr!("Rotação", "Rotation"), &mut self.cpr_angle, -180.0..=180.0, "°");
            labeled_slider(ui, tr!("Largura", "Width"), &mut self.cpr_width, 20.0..=160.0, " mm");
            if before != (self.cpr_angle, self.cpr_width) {
                rebuild = true;
            }
            ui.horizontal_wrapped(|ui| {
                if self.tool == Tool::Curve {
                    if ui.button(trf!("{}  Concluir", "{}  Finish", icons::CHECK)).clicked() {
                        self.tool = Tool::WindowLevel;
                    }
                } else if ui.button(trf!("{}  Continuar", "{}  Continue", icons::BEZIER_CURVE)).clicked() {
                    self.tool = Tool::Curve;
                }
                if !self.curve.is_empty() && ui.button(trf!("{}  Desfazer", "{}  Undo", icons::ARROW_COUNTER_CLOCKWISE)).clicked() {
                    self.curve_undo();
                }
                if ui.button(trf!("{}  Limpar", "{}  Clear", icons::TRASH)).clicked() {
                    self.curve_clear();
                    self.tool = Tool::WindowLevel;
                }
            });
            let shown = matches!(self.mpr_views[3].content, Content::Cpr(..)) || self.cpr_view.is_some();
            if shown {
                if ui.button(trf!("{}  Fechar imagem esticada", "{}  Close straightened image", icons::X)).clicked() {
                    self.cpr_close();
                }
            } else if self.curve.len() >= 2 && ui.button(trf!("{}  Mostrar imagem esticada", "{}  Show straightened image", icons::PATH)).clicked() {
                rebuild = true;
            }
            if rebuild && self.curve.len() >= 2 {
                self.rebuild_cpr();
            }
        });
    }

    // ------------------------------------------------------------------ 2D aids

    /// Thick slab over native images, noise filter and magnifier.
    pub(crate) fn inspector_view_aids(&mut self, ui: &mut Ui, stack: bool) {
        caption(ui, tr!("Leitura", "Reading"));
        let mut zoom = self.magnifier;
        let Some(v) = self.active_view_mut() else { return };
        card(ui, |ui| {
            if stack {
                ui.horizontal(|ui| {
                    ui.label(tr!("Espessura (slab)", "Slab thickness"));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(if v.slab_mm > 0.0 { format!("{:.0} mm", v.slab_mm) } else { tr!("nativa", "native").into() }).color(pal().text_dim));
                    });
                });
                full_slider(ui, &mut v.slab_mm, 0.0..=40.0);
                v.slab_mm = v.slab_mm.round();
                let mut mode = v.slab_mode;
                ui.add_enabled_ui(v.slab_mm > 0.0, |ui| {
                    if segmented(ui, &mut mode, &[(SlabMode::Mip, "", "MIP"), (SlabMode::MinIp, "", "MinIP"), (SlabMode::Avg, "", tr!("Média", "Average"))]) {
                        v.slab_mode = mode;
                    }
                });
                ui.add_space(6.0);
            }
            ui.label(tr!("Filtro de ruído (só exibição)", "Noise reduction (display only)"));
            let mut level = v.denoise;
            let levels = [(0u8, "", tr!("Desligado", "Off")), (1, "", tr!("Leve", "Light")), (2, "", tr!("Médio", "Medium")), (3, "", tr!("Forte", "Strong"))];
            if segmented(ui, &mut level, &levels) {
                v.denoise = level;
            }
            ui.add_space(6.0);
            labeled_slider(ui, tr!("Ampliação da lupa (M)", "Magnifier zoom (M)"), &mut zoom, 2.0..=8.0, "×");
        });
        self.magnifier = zoom.round();
    }
}

/// Reading modes that recolor the image to make one finding stand out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Special {
    /// Mesenteric fat window with densified fat in orange (inflammation).
    Fat,
    /// Negative wide window with internal gas in cyan (pneumoperitoneum).
    Gas,
    /// Dense foci in magenta (stones, appendicoliths).
    Dense,
}

impl Special {
    pub const ALL: [Special; 3] = [Special::Fat, Special::Gas, Special::Dense];

    pub fn label(self) -> &'static str {
        match self {
            Special::Fat => tr!("Gordura mesentérica", "Mesenteric fat"),
            Special::Gas => tr!("Gás livre", "Free air"),
            Special::Dense => tr!("Cálculos", "Stones"),
        }
    }

    /// Name that fits under a preview.
    pub fn short(self) -> &'static str {
        match self {
            Special::Fat => tr!("Gordura", "Fat"),
            Special::Gas => tr!("Gás livre", "Free air"),
            Special::Dense => tr!("Cálculos", "Stones"),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Special::Fat => icons::DROP,
            Special::Gas => icons::CIRCLES_THREE,
            Special::Dense => icons::DIAMOND,
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Special::Fat => tr!(
                "Janela 250/−50 para gordura e realce em laranja da gordura densificada (borramento inflamatório: apendicite, diverticulite, pancreatite).",
                "Window 250/−50 for fat, with fat stranding highlighted in orange (inflammation: appendicitis, diverticulitis, pancreatitis)."
            ),
            Special::Gas => tr!(
                "Janela 1000/−600 em negativo e todo gás dentro do corpo (< −800 HU) em ciano: microbolhas de pneumoperitônio e retropneumoperitônio saltam aos olhos.",
                "Inverted window 1000/−600 with all gas inside the body (< −800 HU) in cyan: tiny bubbles of pneumoperitoneum and retropneumoperitoneum stand out."
            ),
            Special::Dense => tr!(
                "Tudo acima do limiar em magenta (cálculos, apendicolitos, calcificações). Procure no volume para listar os focos pequenos.",
                "Everything above the threshold in magenta (stones, appendicoliths, calcifications). Search the volume to list the small foci."
            ),
        }
    }
}

/// Last search for dense foci.
#[derive(Clone)]
pub struct CalculiResult {
    pub series_uid: String,
    pub study_uid: String,
    pub frame_of_ref: String,
    pub threshold: f32,
    pub items: Vec<crate::imaging::calculi::Candidate>,
}

impl SlycedApp {
    /// 2D viewports a reading mode applies to in the current workspace.
    fn for_special_views(&mut self, mut f: impl FnMut(&mut super::viewport::Viewport)) {
        let views: Vec<&mut super::viewport::Viewport> = match self.workspace {
            Workspace::Viewer => self.views.get_mut(self.active).into_iter().collect(),
            Workspace::Mpr => self.mpr_views.iter_mut().collect(),
            Workspace::Compare => self.cmp_views.iter_mut().collect(),
            Workspace::Volume => Vec::new(),
        };
        for v in views {
            if !matches!(v.content, Content::Empty | Content::Volume(_)) {
                f(v);
            }
        }
    }

    pub(crate) fn special_on(&self, s: Special) -> bool {
        self.active_view().is_some_and(|v| match s {
            Special::Fat => v.highlights.fat,
            Special::Gas => v.highlights.gas,
            Special::Dense => v.highlights.dense.is_some(),
        })
    }

    pub(crate) fn toggle_special(&mut self, s: Special) {
        let on = !self.special_on(s);
        let threshold = self.dense_threshold;
        self.for_special_views(|v| match s {
            Special::Dense => v.highlights.dense = on.then_some(threshold),
            Special::Gas | Special::Fat => {
                if on {
                    // The two modes set their own window; the original one comes back when turned off.
                    if v.restore.is_none() {
                        v.restore = Some((v.window, v.invert));
                    }
                    v.highlights.gas = s == Special::Gas;
                    v.highlights.fat = s == Special::Fat;
                    if s == Special::Gas {
                        v.window = Window::new(-600.0, 1000.0);
                        v.invert = true;
                    } else {
                        v.window = Window::new(-50.0, 250.0);
                        v.invert = false;
                    }
                } else {
                    v.highlights.gas = false;
                    v.highlights.fat = false;
                    if let Some((w, inv)) = v.restore.take() {
                        v.window = w;
                        v.invert = inv;
                    }
                }
            }
        });
    }

    fn set_dense_threshold(&mut self, t: f32) {
        self.dense_threshold = t;
        self.for_special_views(|v| {
            if v.highlights.dense.is_some() {
                v.highlights.dense = Some(t);
            }
        });
    }

    /// Looks for small dense foci in the volume of the active series (background job).
    pub(crate) fn search_calculi(&mut self) {
        let Some(series) = self.active_series().filter(|s| s.volume.is_some()) else {
            self.toast(
                ToastKind::Info,
                tr!("Abra uma série de TC em cortes (com volume) para procurar cálculos.", "Open a CT series of slices (with a volume) to search for stones."),
            );
            return;
        };
        if !self.special_on(Special::Dense) {
            self.toggle_special(Special::Dense);
        }
        self.calculi_busy = true;
        self.jobs.find_calculi(series, self.dense_threshold);
    }

    pub(crate) fn on_calculi(&mut self, series: Arc<LoadedSeries>, threshold: f32, items: Vec<crate::imaging::calculi::Candidate>) {
        self.calculi_busy = false;
        let n = items.len();
        self.calculi = Some(CalculiResult {
            series_uid: series.uid.clone(),
            study_uid: series.study.uid.clone(),
            frame_of_ref: series.info.frame_of_reference.clone(),
            threshold,
            items,
        });
        self.right_open = true;
        self.toast(
            ToastKind::Info,
            match n {
                0 => trf!("Nenhum foco denso pequeno acima de {threshold:.0} HU.", "No small dense focus above {threshold:.0} HU."),
                1 => tr!("1 foco denso encontrado (lista no painel).", "1 dense focus found (listed in the panel).").to_string(),
                n => trf!("{n} focos densos encontrados (lista no painel).", "{n} dense foci found (listed in the panel)."),
            },
        );
    }

    /// Markers of the dense foci found in `series`' patient space.
    pub(crate) fn calculi_pins(&self, series: &LoadedSeries) -> Vec<Pin> {
        let Some(r) = &self.calculi else { return Vec::new() };
        let fr = &series.info.frame_of_reference;
        let same = series.uid == r.series_uid || (series.study.uid == r.study_uid && !fr.is_empty() && *fr == r.frame_of_ref);
        if !same {
            return Vec::new();
        }
        r.items
            .iter()
            .enumerate()
            .map(|(i, c)| Pin { id: 10_000 + i as u64, label: format!("C{}", i + 1), color: highlight::DENSE, point: Some(c.center), image: None })
            .collect()
    }

    fn go_to_calculus(&mut self, i: usize) {
        self.push_nav();
        let Some(r) = &self.calculi else { return };
        let Some(c) = r.items.get(i) else { return };
        let (p, study, fr, uid) = (c.center, r.study_uid.clone(), r.frame_of_ref.clone(), r.series_uid.clone());
        self.cursor = Some((p, study, fr));
        match self.workspace {
            Workspace::Mpr => self.mpr.point = p,
            Workspace::Compare => {
                self.cmp.mpr[0].point = p;
                self.cmp.cursor = Some(p);
            }
            _ => {
                let n = self.layout.count();
                if let Some(v) = self.views.iter_mut().take(n).find(|v| v.content.series().is_some_and(|s| s.uid == uid)) {
                    v.go_to_point(p);
                } else if let Some(v) = self.views.get_mut(self.active) {
                    v.go_to_point(p);
                }
            }
        }
    }

    /// Menu of the reading modes (top bar).
    pub(crate) fn specials_menu(&mut self, ui: &mut Ui) {
        ui.label(RichText::new(tr!("Realces de leitura (TC)", "Reading highlights (CT)")).small().color(pal().text_faint));
        if self.special_preview_row(ui) {
            ui.add_space(4.0);
        } else {
            self.special_buttons(ui);
        }
        ui.separator();
        if ui.button(trf!("{}  Procurar cálculos no volume", "{}  Search the volume for stones", icons::MAGNIFYING_GLASS)).clicked() {
            self.search_calculi();
            ui.close();
        }
    }

    /// Text buttons of the reading modes (when there is no image to preview).
    fn special_buttons(&mut self, ui: &mut Ui) {
        for s in Special::ALL {
            let on = self.special_on(s);
            let r = ui.add(egui::Button::new(RichText::new(format!("{}  {}{}", s.icon(), s.label(), if on { format!("   {}", icons::CHECK) } else { String::new() })).color(if on { pal().accent } else { pal().text })).min_size(egui::vec2(250.0, 26.0)));
            let clicked = r.clicked();
            r.on_hover_text(s.help());
            if clicked {
                self.toggle_special(s);
            }
        }
    }

    /// Reading modes, threshold and the list of dense foci.
    pub(crate) fn inspector_specials(&mut self, ui: &mut Ui, series: &LoadedSeries) {
        if series.info.modality != "CT" {
            return;
        }
        caption(ui, tr!("Realces", "Highlights"));
        let mut go = None;
        card(ui, |ui| {
            if !self.special_preview_row(ui) {
                ui.horizontal_wrapped(|ui| {
                    for s in Special::ALL {
                        let on = self.special_on(s);
                        if chip(ui, &format!("{}  {}", s.icon(), s.label()), on, 60.0).on_hover_text(s.help()).clicked() {
                            self.toggle_special(s);
                        }
                    }
                });
            }
            for s in Special::ALL.into_iter().filter(|s| self.special_on(*s)) {
                ui.add_space(4.0);
                ui.label(RichText::new(s.help()).size(11.5).color(pal().text_faint));
            }
            ui.add_space(6.0);
            let mut t = self.dense_threshold;
            labeled_slider(ui, tr!("Limiar de cálculo (HU)", "Stone threshold (HU)"), &mut t, 130.0..=800.0, "");
            let t = (t / 10.0).round() * 10.0;
            if t != self.dense_threshold {
                self.set_dense_threshold(t);
            }
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!self.calculi_busy && series.volume.is_some(), |ui| {
                    if ui.button(trf!("{}  Procurar cálculos no volume", "{}  Search the volume for stones", icons::MAGNIFYING_GLASS)).clicked() {
                        self.search_calculi();
                    }
                });
                if self.calculi_busy {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                    spinner(ui.painter(), rect.center(), 5.0, self.time, pal().accent);
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                }
            });
            if let Some(r) = self.calculi.as_ref().filter(|r| r.study_uid == series.study.uid) {
                ui.add_space(4.0);
                let n = r.items.len();
                let foci = if n == 1 { "focus" } else { "foci" };
                ui.label(
                    RichText::new(trf!(
                        "{n} foco(s) ≥ {:.0} HU, menores que osso e vasos. Clique para ir.",
                        "{n} {foci} ≥ {:.0} HU, smaller than bone and vessels. Click to go.",
                        r.threshold
                    ))
                    .size(11.5)
                    .color(pal().text_faint),
                );
                egui::ScrollArea::vertical().id_salt("calculi").max_height(220.0).show(ui, |ui| {
                    for (i, c) in r.items.iter().enumerate() {
                        let text = trf!(
                            "C{}   {:.0} HU (média {:.0}) · {} mm · {:.0} mm³",
                            "C{}   {:.0} HU (mean {:.0}) · {} mm · {:.0} mm³",
                            i + 1,
                            c.max_hu,
                            c.mean_hu,
                            crate::i18n::num(c.length_mm, 1),
                            c.volume_mm3
                        );
                        if ui.add(egui::Button::new(RichText::new(text).size(12.0)).frame(false)).on_hover_text(tr!("Ir até o foco", "Go to the focus")).clicked() {
                            go = Some(i);
                        }
                    }
                });
            }
        });
        if let Some(i) = go {
            self.go_to_calculus(i);
        }
    }

    // ------------------------------------------------------------------ restore

    /// Viewports with content in the current workspace.
    fn screen_views(&self) -> Vec<&Viewport> {
        let views: Vec<&Viewport> = match self.workspace {
            Workspace::Viewer => self.views.iter().take(self.layout.count()).collect(),
            Workspace::Mpr => self.mpr_views.iter().collect(),
            Workspace::Volume => vec![&self.vr_view],
            Workspace::Compare => self.cmp_views.iter().collect(),
        };
        views.into_iter().filter(|v| !matches!(v.content, Content::Empty)).collect()
    }

    fn for_screen_views(&mut self, mut f: impl FnMut(&mut Viewport)) {
        let n = self.layout.count();
        let views: Vec<&mut Viewport> = match self.workspace {
            Workspace::Viewer => self.views.iter_mut().take(n).collect(),
            Workspace::Mpr => self.mpr_views.iter_mut().collect(),
            Workspace::Volume => vec![&mut self.vr_view],
            Workspace::Compare => self.cmp_views.iter_mut().collect(),
        };
        for v in views.into_iter().filter(|v| !matches!(v.content, Content::Empty)) {
            f(v);
        }
    }

    /// Color map of exam B blended over A: part of the fusion, kept on restore.
    fn fused_colormap(&self) -> Option<Colormap> {
        (self.workspace == Workspace::Compare && self.cmp.display == CompareDisplay::Blend).then(|| self.cmp_views[1].colormap)
    }

    /// What the tools changed on screen (labels over the image, counter of the restore button).
    pub(crate) fn screen_changes(&self) -> Vec<Change> {
        let views: Vec<&Viewport> = self.screen_views().into_iter().filter(|v| !matches!(v.content, Content::Volume(_))).collect();
        let fused = self.fused_colormap().map(|_| &self.cmp_views[1]);
        let mut out = Vec::new();
        let measures: usize = views.iter().map(|v| v.measurements.len()).sum();
        if measures > 0 {
            out.push(Change::Measures(measures));
        }
        for (on, c) in [
            (views.iter().any(|v| v.highlights.gas), Change::Gas),
            (views.iter().any(|v| v.highlights.fat), Change::Fat),
            (views.iter().any(|v| v.highlights.dense.is_some()), Change::Dense),
        ] {
            if on {
                out.push(c);
            }
        }
        let shared_slab = match self.workspace {
            Workspace::Mpr => self.mpr.slab_mm,
            Workspace::Compare => self.cmp.mpr[0].slab_mm.max(self.cmp.mpr[1].slab_mm),
            _ => 0.0,
        };
        let slab = views.iter().map(|v| v.slab_mm).fold(shared_slab, f32::max);
        if slab > 0.0 {
            out.push(Change::Slab(slab));
        }
        if views.iter().any(|v| v.denoise > 0) {
            out.push(Change::Denoise);
        }
        if views.iter().any(|v| v.colormap != Colormap::Gray && !fused.is_some_and(|b| std::ptr::eq(*v, b))) {
            out.push(Change::Colormap);
        }
        if views.iter().any(|v| v.rot != 0 || v.flip_h || v.flip_v) {
            out.push(Change::Orientation);
        }
        if views.iter().any(|v| v.invert && v.restore.is_none()) {
            out.push(Change::Inverted);
        }
        if self.cursor.is_some() || self.cmp.cursor.is_some() {
            out.push(Change::Cursor);
        }
        if let Some(n) = self.calculi.as_ref().map(|r| r.items.len()).filter(|n| *n > 0) {
            out.push(Change::Calculi(n));
        }
        if !self.curve.is_empty() {
            out.push(Change::Path);
        }
        let oblique = match self.workspace {
            Workspace::Mpr => self.mpr.is_oblique(),
            Workspace::Compare => self.cmp.mpr.iter().any(|m| m.is_oblique()),
            _ => false,
        };
        if oblique {
            out.push(Change::Oblique);
        }
        out
    }

    /// Undoes one change on every image on screen (label removed).
    pub(crate) fn undo_change(&mut self, c: Change) {
        // Turning off the last of the two special modes brings their window back.
        let off = |v: &mut Viewport| {
            if !v.highlights.gas && !v.highlights.fat
                && let Some((w, inv)) = v.restore.take() {
                    v.window = w;
                    v.invert = inv;
                }
        };
        match c {
            Change::Measures(_) => self.clear_measurements(),
            Change::Gas => self.for_screen_views(|v| {
                v.highlights.gas = false;
                off(v);
            }),
            Change::Fat => self.for_screen_views(|v| {
                v.highlights.fat = false;
                off(v);
            }),
            Change::Dense => self.for_screen_views(|v| v.highlights.dense = None),
            Change::Slab(_) => {
                self.for_screen_views(|v| v.slab_mm = 0.0);
                self.mpr.slab_mm = 0.0;
                self.cmp.mpr.iter_mut().for_each(|m| m.slab_mm = 0.0);
            }
            Change::Denoise => self.for_screen_views(|v| v.denoise = 0),
            Change::Colormap => {
                let fused = self.fused_colormap();
                self.for_screen_views(|v| v.colormap = Colormap::Gray);
                if let Some(c) = fused {
                    self.cmp_views[1].colormap = c;
                }
            }
            Change::Orientation => self.for_screen_views(|v| {
                v.rot = 0;
                v.flip_h = false;
                v.flip_v = false;
            }),
            Change::Inverted => self.for_screen_views(|v| {
                if v.restore.is_none() {
                    v.invert = false;
                }
            }),
            Change::Cursor => {
                self.cursor = None;
                self.cmp.cursor = None;
            }
            Change::Calculi(_) => self.calculi = None,
            Change::Path => self.curve_clear(),
            Change::Oblique => {
                self.mpr.reset_axes();
                self.cmp.mpr.iter_mut().for_each(|m| m.reset_axes());
            }
        }
    }

    /// Removes measurements, arrows and density probes from every image on screen.
    pub(crate) fn clear_measurements(&mut self) {
        self.for_screen_views(|v| v.clear_measurements());
    }

    /// Turns off reading highlights, slab, noise filter and color maps on screen.
    pub(crate) fn clear_enhancements(&mut self) {
        let fused = self.fused_colormap();
        self.for_screen_views(|v| v.clear_enhancements());
        if let Some(c) = fused {
            self.cmp_views[1].colormap = c;
        }
        match self.workspace {
            Workspace::Mpr => self.mpr.slab_mm = 0.0,
            Workspace::Compare => self.cmp.mpr.iter_mut().for_each(|m| m.slab_mm = 0.0),
            _ => {}
        }
    }

    /// Removes the 3D cursor, the dense foci found and the tracker path.
    pub(crate) fn clear_markers(&mut self) {
        self.cursor = None;
        self.cmp.cursor = None;
        self.calculi = None;
        self.curve_clear();
    }

    /// Everything on screen back to how it opened; saved findings stay.
    pub(crate) fn reset_screen(&mut self) {
        let fused = self.fused_colormap();
        self.for_screen_views(|v| v.reset_all());
        if let Some(c) = fused {
            self.cmp_views[1].colormap = c;
        }
        self.clear_markers();
        match self.workspace {
            Workspace::Mpr => {
                self.mpr.slab_mm = 0.0;
                self.mpr.reset_axes();
            }
            Workspace::Compare => self.cmp.mpr.iter_mut().for_each(|m| {
                m.slab_mm = 0.0;
                m.reset_axes();
            }),
            _ => {}
        }
        self.toast(ToastKind::Info, tr!("Imagens restauradas ao padrão. Os achados salvos foram mantidos.", "Images reset to default. Saved findings were kept."));
    }

    /// Menu of the restore button (top bar).
    pub(crate) fn reset_menu(&mut self, ui: &mut Ui) {
        let item = |ui: &mut Ui, enabled: bool, icon: &str, text: &str, keys: &str, tip: &str| {
            let button = egui::Button::new(format!("{icon}  {text}")).shortcut_text(keys).min_size(egui::vec2(330.0, 28.0));
            let r = ui.add_enabled(enabled, button);
            let clicked = r.clicked();
            r.on_hover_text(tip).on_disabled_hover_text(tip);
            clicked
        };
        ui.label(RichText::new(tr!("Voltar ao padrão", "Reset to default")).small().color(pal().text_faint));
        let changes = self.screen_changes();
        let any = |f: fn(&Change) -> bool| changes.iter().any(f);
        let has_view = self.active_view().is_some_and(|v| !matches!(v.content, Content::Empty));
        let (text, tip) = (
            tr!("Visualização desta janela", "View of this viewport"),
            tr!("Janela/nível, zoom, posição, rotação, espelhamento e inversão de cores", "Window/level, zoom, position, rotation, flip and color inversion"),
        );
        if item(ui, has_view, icons::ARROW_COUNTER_CLOCKWISE, text, "R", tip) {
            if let Some(v) = self.active_view_mut() {
                v.reset_view();
            }
            ui.close();
        }
        let (text, tip) = (
            tr!("Apagar medidas e setas", "Delete measurements and arrows"),
            tr!(
                "Distâncias, ângulos, ROIs, pontos de densidade e setas de todas as imagens na tela",
                "Distances, angles, ROIs, density points and arrows on all images on screen"
            ),
        );
        if item(ui, any(|c| matches!(c, Change::Measures(_))), icons::TRASH, text, "Shift+Del", tip) {
            self.clear_measurements();
            ui.close();
        }
        let (text, tip) = (
            tr!("Desligar realces e filtros", "Turn off highlights and filters"),
            tr!(
                "Gordura mesentérica, gás livre, cálculos, slab, filtro de ruído e mapa de cores",
                "Mesenteric fat, free air, stones, slab, noise reduction and color map"
            ),
        );
        if item(ui, any(|c| matches!(c, Change::Gas | Change::Fat | Change::Dense | Change::Slab(_) | Change::Denoise | Change::Colormap)), icons::SPARKLE, text, "", tip) {
            self.clear_enhancements();
            ui.close();
        }
        let (text, tip) = (
            tr!("Limpar cursor e marcadores", "Clear cursor and markers"),
            tr!("Cursor 3D, focos de cálculo encontrados e trajeto do rastreador tubular", "3D cursor, stone foci found and the tube tracker path"),
        );
        if item(ui, any(|c| matches!(c, Change::Cursor | Change::Calculi(_) | Change::Path)), icons::X_CIRCLE, text, "", tip) {
            self.clear_markers();
            ui.close();
        }
        ui.separator();
        let tip = tr!(
            "Todas as janelas na tela voltam a como a série abriu: visualização, medidas, realces, filtros, cursor e planos oblíquos. Os achados salvos são mantidos.",
            "All viewports on screen go back to how the series opened: view, measurements, highlights, filters, cursor and oblique planes. Saved findings are kept."
        );
        if item(ui, true, icons::ARROWS_CLOCKWISE, tr!("Restaurar tudo", "Reset all"), "Shift+R", tip) {
            self.reset_screen();
            ui.close();
        }
    }
}

/// Structure followed by the tubular tracker: width of the straightened image and window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TubeKind {
    #[default]
    Appendix,
    Ureter,
    Bile,
    Vessel,
}

impl TubeKind {
    pub const ALL: [TubeKind; 4] = [TubeKind::Appendix, TubeKind::Ureter, TubeKind::Bile, TubeKind::Vessel];

    pub fn label(self) -> &'static str {
        match self {
            TubeKind::Appendix => tr!("Apêndice", "Appendix"),
            TubeKind::Ureter => "Ureter",
            TubeKind::Bile => tr!("Colédoco", "Bile duct"),
            TubeKind::Vessel => tr!("Vaso", "Vessel"),
        }
    }

    pub fn width_mm(self) -> f32 {
        match self {
            TubeKind::Appendix | TubeKind::Bile => 40.0,
            TubeKind::Ureter => 50.0,
            TubeKind::Vessel => 60.0,
        }
    }

    pub fn window(self) -> Window {
        match self {
            TubeKind::Appendix | TubeKind::Ureter => Window::new(50.0, 400.0),
            TubeKind::Bile => Window::new(60.0, 350.0),
            TubeKind::Vessel => Window::new(300.0, 600.0),
        }
    }
}
