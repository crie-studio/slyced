//! Comparison of two exams (e.g. the current one and a prior): side by side
//! or fused, with navigation, cursor and measurements linked once aligned.

use super::icons;
use super::panels::{full_slider, labeled_slider};
use super::theme::*;
use super::viewport::{Content, Fusion, FusionLayer, Hit, Linked, MeasureKind, MprShared, Pin, Tool, ViewCtx, ViewEvents};
use super::widgets::*;
use crate::app::{SlycedApp, Target, ToastKind};
use crate::dicom::model::format_date;
use crate::dicom::series::LoadedSeries;
use crate::imaging::lut::Colormap;
use crate::imaging::register::{self, AutoResult, Rigid, Sim2};
use crate::imaging::reslice::{PlaneKind, SlabMode};
use egui::{pos2, vec2, Align, Color32, Pos2, Rect, RichText, Sense, Ui};
use glam::{DVec2, DVec3};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Colors identifying exam A and exam B.
pub const COLOR_A: Color32 = Color32::from_rgb(77, 141, 255);
pub const COLOR_B: Color32 = Color32::from_rgb(255, 138, 76);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompareDisplay {
    SideBySide,
    Blend,
    Swipe,
    Checker,
    /// Where B (with contrast) is denser than A (without): B − A ≥ 15 HU in color over A.
    Enhancement,
    /// B − A in gray (subtraction image).
    Subtraction,
}

/// Where the current alignment between the exams comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum RegSource {
    None,
    SameFrame,
    Centers,
    Landmarks(usize),
    Auto(f64),
}

impl RegSource {
    fn describe(&self) -> String {
        match self {
            RegSource::None => "Sem alinhamento: posições DICOM originais.".into(),
            RegSource::SameFrame => "Mesmo referencial (mesma aquisição): alinhamento exato.".into(),
            RegSource::Centers => "Aproximado pelo centro do corpo. Refine com o alinhamento automático ou com pontos.".into(),
            RegSource::Landmarks(n) => format!("{n} par(es) de pontos de referência."),
            RegSource::Auto(nmi) => format!("Automático por informação mútua (NMI {nmi:.3})."),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LandmarkPos {
    pub patient: Option<DVec3>,
    pub mm: DVec2,
    pub slice: usize,
    pub pixel: Pos2,
}

#[derive(Clone, Debug, Default)]
pub struct Landmark {
    pub a: Option<LandmarkPos>,
    pub b: Option<LandmarkPos>,
}

pub struct Compare {
    pub plane: PlaneKind,
    pub display: CompareDisplay,
    pub opacity: f32,
    pub swipe: f32,
    pub checker_mm: f32,
    pub sync_position: bool,
    pub sync_view: bool,
    pub sync_window: bool,
    /// Exam A → exam B (3D).
    pub reg: Rigid,
    /// Image A (mm) → image B (mm), for series without volume.
    pub sim: Sim2,
    pub source: RegSource,
    /// Bumped on every alignment change (cache key of the resampled images).
    pub version: u64,
    pub landmarks: Vec<Landmark>,
    /// Navigation of A, and of B while it is not synchronized.
    pub mpr: [MprShared; 2],
    pub registering: Option<Arc<AtomicBool>>,
    /// Linked cursor, in exam A's coordinates.
    pub cursor: Option<DVec3>,
    /// Density probes read in both exams: label, A, B, B − A (HU).
    pub deltas: Vec<(String, f32, f32, f32)>,
    sub_cache: Option<(u64, egui::TextureHandle, [usize; 2])>,
    pair: (String, String),
}

impl Default for Compare {
    fn default() -> Self {
        Self {
            plane: PlaneKind::Axial,
            display: CompareDisplay::SideBySide,
            opacity: 0.5,
            swipe: 0.5,
            checker_mm: 20.0,
            sync_position: true,
            sync_view: true,
            sync_window: true,
            reg: Rigid::IDENTITY,
            sim: Sim2::IDENTITY,
            source: RegSource::None,
            version: 1,
            landmarks: Vec::new(),
            mpr: [MprShared::new(), MprShared::new()],
            registering: None,
            cursor: None,
            deltas: Vec::new(),
            sub_cache: None,
            pair: (String::new(), String::new()),
        }
    }
}

fn side_label(series: &LoadedSeries, side: usize) -> String {
    let date = format_date(&series.study.date);
    format!("{} · {}", if side == 0 { "A" } else { "B" }, if date.is_empty() { series.info.title() } else { date })
}

impl SlycedApp {
    /// Removes one exam from the comparison; the remaining one becomes A.
    pub(crate) fn close_compare_side(&mut self, side: usize) {
        self.cmp_views[side].set_content(Content::Empty);
        self.pending.retain(|(t, _)| *t != Target::Compare(side));
        if side == 0 && self.compare_series(1).is_some() {
            self.cmp_views.swap(0, 1);
        }
        self.cmp.cursor = None;
        self.active = 0;
        self.refresh_compare();
    }

    /// Opens `study_uid` as exam B next to the series being viewed.
    pub(crate) fn compare_with_study(&mut self, study_uid: &str) {
        let Some(a) = self.active_series().or_else(|| self.compare_series(0)) else { return };
        let Some(study) = self.library.studies.iter().find(|s| s.header.uid == study_uid) else { return };
        let words = |s: &str| s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(String::from).collect::<Vec<_>>();
        let aw = words(&a.info.description);
        let best = study
            .series
            .iter()
            .filter(|s| s.image_count() > 0)
            .max_by_key(|s| {
                let same_mod = (s.modality == a.info.modality) as i64 * 100_000;
                let common = words(&s.description).iter().filter(|w| aw.contains(w)).count() as i64 * 2000;
                same_mod + common + s.image_count().min(1000) as i64
            })
            .map(|s| s.uid.clone());
        let Some(b) = best else { return };
        if !self.compare_series(0).is_some_and(|s| Arc::ptr_eq(&s, &a)) {
            self.cmp_views[0].set_content(Content::Stack(a));
        }
        self.workspace = crate::app::Workspace::Compare;
        self.active = 0;
        self.open_series(&b, Target::Compare(1));
        self.refresh_compare();
    }

    /// Changes the display; blending colors exam B so the two can be told apart.
    pub(crate) fn set_compare_display(&mut self, d: CompareDisplay) {
        if d == CompareDisplay::Blend && self.cmp.display != d && self.cmp_views[1].colormap == Colormap::Gray {
            self.cmp_views[1].colormap = Colormap::HotIron;
        }
        self.cmp.display = d;
    }

    pub(crate) fn compare_series(&self, side: usize) -> Option<Arc<LoadedSeries>> {
        self.cmp_views.get(side).and_then(|v| v.content.series().cloned())
    }

    /// Both exams are shown as reformatted planes of their volumes.
    pub(crate) fn compare_is_3d(&self) -> bool {
        matches!(self.cmp_views[0].content, Content::Mpr(..)) && matches!(self.cmp_views[1].content, Content::Mpr(..))
    }

    fn compare_fused(&self) -> bool {
        self.cmp.display != CompareDisplay::SideBySide && self.compare_series(1).is_some()
    }

    /// B is drawn in A's planes (same image grid), so everything lines up.
    fn compare_linked(&self) -> bool {
        self.compare_series(1).is_some() && (self.compare_fused() || (self.cmp.sync_position && self.tool != Tool::Landmark))
    }

    /// Opens the comparison with the active series as A and, as B, the most
    /// similar series of another study of the same patient (if loaded).
    pub(crate) fn start_compare(&mut self) {
        let Some(a) = self.active_series().or_else(|| self.views.iter().find_map(|v| v.content.series().cloned())) else {
            self.toast(ToastKind::Info, "Abra uma série primeiro.");
            return;
        };
        if self.compare_series(0).is_none() || self.compare_series(0).is_some_and(|s| !Arc::ptr_eq(&s, &a) && self.compare_series(1).is_none()) {
            self.cmp_views[0].set_content(Content::Stack(a.clone()));
        }
        if self.compare_series(1).is_none()
            && let Some(uid) = self.prior_candidate(&a) {
                self.open_series(&uid, Target::Compare(1));
            }
        self.refresh_compare();
    }

    /// Series of another study of the same patient that best matches `a`.
    fn prior_candidate(&self, a: &LoadedSeries) -> Option<String> {
        let words = |s: &str| s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(String::from).collect::<Vec<_>>();
        let aw = words(&a.info.description);
        let same_patient = |pid: &str, name: &str| (!a.study.patient_id.is_empty() && pid == a.study.patient_id) || (!a.study.patient_name.is_empty() && name == a.study.patient_name);
        self.library
            .studies
            .iter()
            .filter(|st| st.header.uid != a.study.uid && same_patient(&st.header.patient_id, &st.header.patient_name))
            .flat_map(|st| st.series.iter())
            .filter(|s| s.modality == a.info.modality && s.image_count() > 0)
            .max_by_key(|s| {
                let common = words(&s.description).iter().filter(|w| aw.contains(w)).count() as i64;
                let count_match = -((s.image_count() as i64 - a.len() as i64).abs()).min(1000);
                common * 2000 + count_match + s.image_count().min(1000) as i64
            })
            .map(|s| s.uid.clone())
    }

    /// Chooses planes (both with volume) or images, and re-aligns when the pair changed.
    pub(crate) fn refresh_compare(&mut self) {
        let (a, b) = (self.compare_series(0), self.compare_series(1));
        let both_3d = a.as_ref().is_some_and(|s| s.volume.is_some()) && b.as_ref().is_some_and(|s| s.volume.is_some());
        for (side, s) in [a.clone(), b.clone()].into_iter().enumerate() {
            let Some(s) = s else { continue };
            let content = if both_3d { Content::Mpr(s, self.cmp.plane) } else { Content::Stack(s) };
            let same = match (&self.cmp_views[side].content, &content) {
                (Content::Mpr(x, k), Content::Mpr(y, l)) => Arc::ptr_eq(x, y) && k == l,
                (Content::Stack(x), Content::Stack(y)) => Arc::ptr_eq(x, y),
                _ => false,
            };
            if !same {
                self.cmp_views[side].set_content(content);
            }
        }
        let pair = (a.as_ref().map(|s| s.uid.clone()).unwrap_or_default(), b.as_ref().map(|s| s.uid.clone()).unwrap_or_default());
        if pair != self.cmp.pair {
            self.cmp.pair = pair;
            if let Some(a) = &a {
                if let Some(v) = &a.volume {
                    self.cmp.mpr[0].point = v.center();
                }
                if b.is_none() {
                    self.cmp_views[1].window = self.cmp_views[0].window;
                }
            }
            self.reset_registration();
        }
    }

    /// Initial alignment: exact when both share the Frame of Reference,
    /// otherwise the body centers are matched.
    pub(crate) fn reset_registration(&mut self) {
        if let Some(flag) = self.cmp.registering.take() {
            flag.store(true, Ordering::Relaxed);
        }
        self.cmp.landmarks.clear();
        self.cmp.version += 1;
        self.cmp.reg = Rigid::IDENTITY;
        self.cmp.sim = Sim2::IDENTITY;
        self.cmp.source = RegSource::None;
        let (Some(a), Some(b)) = (self.compare_series(0), self.compare_series(1)) else { return };
        // Images without volume: centers matched, same scale.
        if let (Some(ia), Some(ib)) = (a.images.get(self.cmp_views[0].slice.min(a.len().saturating_sub(1))), b.images.first()) {
            let ca = DVec2::new(ia.width as f64 * ia.spacing.0, ia.height as f64 * ia.spacing.1) * 0.5;
            let cb = DVec2::new(ib.width as f64 * ib.spacing.0, ib.height as f64 * ib.spacing.1) * 0.5;
            self.cmp.sim = Sim2 { t: cb - ca, ..Sim2::IDENTITY };
        }
        let same_frame = !a.info.frame_of_reference.is_empty() && a.info.frame_of_reference == b.info.frame_of_reference;
        if same_frame {
            self.cmp.source = RegSource::SameFrame;
        } else if let (Some(va), Some(vb)) = (&a.volume, &b.volume)
            && let (Some(ca), Some(cb)) = (register::body_center(va), register::body_center(vb)) {
                self.cmp.reg = Rigid::translation(cb - ca);
                self.cmp.source = RegSource::Centers;
            }
        self.cmp.mpr[1].point = self.cmp.reg.apply(self.cmp.mpr[0].point);
    }

    pub(crate) fn start_auto_registration(&mut self) {
        let (Some(a), Some(b)) = (self.compare_series(0), self.compare_series(1)) else { return };
        let (Some(va), Some(vb)) = (a.volume.clone(), b.volume.clone()) else {
            self.toast(ToastKind::Info, "O alinhamento automático precisa de duas séries com volume (cortes de TC/RM).");
            return;
        };
        if let Some(flag) = self.cmp.registering.take() {
            flag.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.cmp.registering = Some(cancel.clone());
        self.jobs.register(va, vb, self.cmp.reg, self.cmp.version, cancel);
    }

    pub(crate) fn on_registered(&mut self, version: u64, result: Option<AutoResult>) {
        if version != self.cmp.version {
            return; // the pair or the alignment changed meanwhile
        }
        self.cmp.registering = None;
        match result {
            Some(r) => {
                self.cmp.reg = r.transform;
                self.cmp.source = RegSource::Auto(r.nmi);
                self.cmp.version += 1;
                self.cmp.mpr[1].point = self.cmp.reg.apply(self.cmp.mpr[0].point);
                self.toast(ToastKind::Success, "Exames alinhados automaticamente.");
            }
            None => self.toast(ToastKind::Error, "Não foi possível alinhar automaticamente (pouca sobreposição entre os exames)."),
        }
    }

    fn add_landmark(&mut self, side: usize, hit: Hit, linked: bool) {
        let mut pos = LandmarkPos { patient: hit.patient, mm: DVec2::new(hit.mm.0, hit.mm.1), slice: hit.slice, pixel: hit.pixel };
        if side == 1 && linked {
            // Clicked on B drawn in A's grid: store B's own coordinates.
            pos.patient = pos.patient.map(|p| self.cmp.reg.apply(p));
            pos.mm = self.cmp.sim.apply(pos.mm);
            if let Some(ib) = self.compare_series(1).and_then(|b| b.images.get(hit.slice).cloned()) {
                pos.pixel = pos2((pos.mm.x / ib.spacing.0) as f32, (pos.mm.y / ib.spacing.1) as f32);
            }
        }
        let slot = self.cmp.landmarks.iter_mut().find(|l| if side == 0 { l.a.is_none() } else { l.b.is_none() });
        match slot {
            Some(l) if side == 0 => l.a = Some(pos),
            Some(l) => l.b = Some(pos),
            None => self.cmp.landmarks.push(if side == 0 { Landmark { a: Some(pos), b: None } } else { Landmark { a: None, b: Some(pos) } }),
        }
        self.solve_landmarks();
    }

    fn solve_landmarks(&mut self) {
        let complete: Vec<(LandmarkPos, LandmarkPos)> = self.cmp.landmarks.iter().filter_map(|l| Some((l.a?, l.b?))).collect();
        if complete.is_empty() {
            return;
        }
        if self.compare_is_3d() {
            let pairs: Vec<(DVec3, DVec3)> = complete.iter().filter_map(|(a, b)| Some((a.patient?, b.patient?))).collect();
            if let Some(r) = register::from_landmarks(&pairs) {
                self.cmp.reg = r;
                self.cmp.source = RegSource::Landmarks(pairs.len());
            }
        } else {
            let pairs: Vec<(DVec2, DVec2)> = complete.iter().map(|(a, b)| (a.mm, b.mm)).collect();
            if let Some(s) = register::similarity_2d(&pairs) {
                self.cmp.sim = s;
                self.cmp.source = RegSource::Landmarks(pairs.len());
            }
        }
        self.cmp.version += 1;
    }

    /// B in A's plane or image, for the synchronized display.
    fn compare_link(&mut self) -> Option<Linked> {
        if self.compare_is_3d() {
            let (grid, k, key) = self.cmp_views[0].plane_state(&self.cmp.mpr[0])?;
            let m = &self.cmp.mpr[0];
            return Some(Linked::Plane { grid, k, slab_mm: m.slab_mm, slab_mode: m.slab_mode, map: self.cmp.reg, key, version: self.cmp.version });
        }
        let (a, b) = (self.compare_series(0)?, self.compare_series(1)?);
        let ka = self.cmp_views[0].slice.min(a.len().checked_sub(1)?);
        let ia = a.images.get(ka)?;
        let slice = if a.len() == b.len() { ka } else { self.cmp_views[1].slice.min(b.len().checked_sub(1)?) };
        Some(Linked::Image { size: (ia.width, ia.height), spacing: ia.spacing, map: self.cmp.sim, slice, key: ka as u64, version: self.cmp.version })
    }

    fn compare_pins(&self, linked: bool) -> [Vec<Pin>; 2] {
        let inv = self.cmp.reg.inverse();
        let mut pins = [Vec::new(), Vec::new()];
        for (i, l) in self.cmp.landmarks.iter().enumerate() {
            let label = format!("{}", i + 1);
            if let Some(a) = l.a {
                pins[0].push(Pin { id: i as u64, label: label.clone(), color: COLOR_A, point: a.patient, image: Some((a.slice, a.pixel)) });
            }
            if let Some(b) = l.b {
                let pin = if linked {
                    Pin { id: i as u64, label, color: COLOR_B, point: b.patient.map(|p| inv.apply(p)), image: None }
                } else {
                    Pin { id: i as u64, label, color: COLOR_B, point: b.patient, image: Some((b.slice, b.pixel)) }
                };
                pins[1].push(pin);
            }
        }
        for side in 0..2 {
            if let Some(s) = self.compare_series(side) {
                let to_view = |p: DVec3| if side == 1 && linked { inv.apply(p) } else { p };
                pins[side].extend(self.bookmark_pins(&s).into_iter().map(|mut p| {
                    p.point = p.point.map(to_view);
                    p
                }));
            }
        }
        pins
    }

    /// The two viewports (side by side) or the fused view.
    pub(crate) fn compare_views_ui(&mut self, ui: &mut Ui) {
        let full = ui.available_rect_before_wrap();
        ui.allocate_rect(full, Sense::hover());
        let fused = self.compare_fused();
        let rects = if fused {
            vec![full]
        } else {
            let w = (full.width() - 2.0) * 0.5;
            vec![Rect::from_min_size(full.min, vec2(w, full.height())), Rect::from_min_size(full.min + vec2(w + 2.0, 0.0), vec2(w, full.height()))]
        };
        self.view_rects = rects.clone();
        let linked = self.compare_linked();
        let link = if linked { self.compare_link() } else { None };

        // Zoom, pan, rotation and window follow the side being used.
        let (src, dst) = if self.active == 1 && !fused { (1, 0) } else { (0, 1) };
        if self.cmp.sync_view && link.is_some() {
            let (zoom, pan, rot, fh, fv) = {
                let s = &self.cmp_views[src];
                (s.zoom, s.pan, s.rot, s.flip_h, s.flip_v)
            };
            let d = &mut self.cmp_views[dst];
            (d.zoom, d.pan, d.rot, d.flip_h, d.flip_v) = (zoom, pan, rot, fh, fv);
        }
        let same_modality = match (self.compare_series(0), self.compare_series(1)) {
            (Some(a), Some(b)) => a.info.modality == b.info.modality,
            _ => false,
        };
        if self.cmp.sync_window && same_modality {
            self.cmp_views[dst].window = self.cmp_views[src].window;
        }
        let pins = self.compare_pins(link.is_some());
        let tags: Vec<Option<(String, Color32)>> = (0..2).map(|s| self.compare_series(s).map(|x| (side_label(&x, s), if s == 0 { COLOR_A } else { COLOR_B }))).collect();
        let cursor = self.cmp.cursor;
        let changes = if self.export_request.is_none() && !self.reading { self.screen_changes() } else { Vec::new() };
        let chip_side = [self.active.min(1), 1 - self.active.min(1)].into_iter().find(|&s| !matches!(self.cmp_views[s].content, Content::Empty));
        let chrome = self.reading && self.reading_chrome(ui.ctx()) > 0.0;
        let base = |app: &SlycedApp| {
            let mut cx = ViewCtx::new(app.tool, app.time);
            cx.smooth = app.settings.smooth;
            cx.overlays = app.settings.overlays;
            cx.magnifier = app.magnifier;
            cx.closable = !app.reading;
            cx.reading = app.reading;
            cx.info = app.settings.reading_info;
            cx.chrome = chrome;
            cx.several = !fused;
            cx
        };
        let mut events: Vec<(usize, ViewEvents)> = Vec::new();
        if fused {
            let mut cxb = base(self);
            cxb.linked = link.clone();
            let maps = matches!(self.cmp.display, CompareDisplay::Enhancement | CompareDisplay::Subtraction);
            let layer = if maps {
                link.as_ref().and_then(|l| self.subtraction_layer(ui, l))
            } else {
                self.cmp_views[1].overlay_texture(ui, &cxb).map(|(tex, size)| FusionLayer {
                    tex,
                    size,
                    style: match self.cmp.display {
                        CompareDisplay::Blend => Fusion::Blend(self.cmp.opacity),
                        CompareDisplay::Swipe => Fusion::Swipe(self.cmp.swipe),
                        _ => Fusion::Checker(self.cmp.checker_mm),
                    },
                })
            };
            let tag = match self.cmp.display {
                CompareDisplay::Enhancement => "Mapa de realce (B − A ≥ 15 HU)",
                CompareDisplay::Subtraction => "Subtração B − A",
                _ => "A + B",
            };
            let mut cx = base(self);
            cx.active = true;
            cx.closable = false;
            cx.fusion = layer;
            cx.cursor = cursor;
            cx.pins = pins[0].clone();
            cx.tag = Some((tag.into(), pal().accent_2));
            cx.changes = changes;
            cx.mpr = Some(&mut self.cmp.mpr[0]);
            let ev = self.cmp_views[0].show(ui, full, &mut cx);
            events.push((0, ev));
        } else {
            for side in 0..2 {
                let mut cx = base(self);
                cx.active = self.active == side;
                cx.tag = tags[side].clone();
                cx.pins = pins[side].clone();
                if chip_side == Some(side) {
                    cx.changes = changes.clone();
                }
                let mapped = if side == 1 && link.is_none() { cursor.map(|p| self.cmp.reg.apply(p)) } else { cursor };
                cx.cursor = mapped;
                if side == 1 && self.compare_series(1).is_none() {
                    cx.tag = Some(("B · arraste aqui a série do exame anterior".into(), COLOR_B));
                }
                if side == 1 && link.is_some() {
                    cx.linked = link.clone();
                    cx.mpr = Some(&mut self.cmp.mpr[0]);
                } else {
                    cx.mpr = Some(&mut self.cmp.mpr[side]);
                }
                let ev = self.cmp_views[side].show(ui, rects[side], &mut cx);
                events.push((side, ev));
            }
        }
        let mut status = None;
        for (side, ev) in events {
            if ev.status.is_some() {
                status = ev.status.clone();
            }
            let (close, undo, reset_all, context) = (ev.close, ev.undo, ev.reset_all, ev.context);
            self.compare_event(side, ev, link.is_some());
            if let Some(c) = undo {
                self.undo_change(c);
            }
            if reset_all {
                self.reset_screen();
            }
            if let Some(a) = context {
                let ctx = ui.ctx().clone();
                self.context_action(&ctx, side, a);
            }
            if close {
                self.close_compare_side(side);
                break;
            }
        }
        self.status = status.unwrap_or_default();
        self.update_deltas(link.is_some());
    }

    /// Names of the two exams in density readouts: "Pré"/"Pós" when the phases are known.
    fn phase_names(&self) -> (&'static str, &'static str, bool) {
        use crate::phases::{phase, Phase};
        match (self.compare_series(0), self.compare_series(1)) {
            (Some(a), Some(b)) => match (phase(&a.info), phase(&b.info)) {
                (Phase::Post, Phase::Pre) => ("Pós", "Pré", true),
                (Phase::Pre, _) | (_, Phase::Post) => ("Pré", "Pós", false),
                _ => ("A", "B", false),
            },
            _ => ("A", "B", false),
        }
    }

    /// Density probes (H) read in both exams at the same anatomical point,
    /// averaged in a 2.5 mm sphere, with the change between them (ΔHU).
    fn update_deltas(&mut self, linked: bool) {
        self.cmp.deltas.clear();
        let (Some(a), Some(b)) = (self.compare_series(0), self.compare_series(1)) else { return };
        let (Some(va), Some(vb)) = (a.volume.clone(), b.volume.clone()) else { return };
        let (na, nb, reversed) = self.phase_names();
        let reg = self.cmp.reg;
        for side in 0..2 {
            let v = &self.cmp_views[side];
            let Some(key) = v.current_key() else { continue };
            let readings: Vec<(usize, f32, f32)> = v
                .measurements
                .iter()
                .enumerate()
                .filter(|(_, m)| m.kind == MeasureKind::Probe && m.key == key)
                .filter_map(|(i, m)| {
                    let p = v.pixel_patient(m.pts[0])?;
                    let pa = if side == 1 && !linked { reg.inverse().apply(p) } else { p };
                    Some((i, va.mean_in_sphere(pa, 2.5)?, vb.mean_in_sphere(reg.apply(pa), 2.5)?))
                })
                .collect();
            for (i, ha, hb) in readings {
                // Δ is always "with contrast − without contrast" when the phases are known.
                let delta = if reversed { ha - hb } else { hb - ha };
                let tail = if na == "A" { String::new() } else { format!(" · {}", crate::phases::enhancement_label(delta)) };
                let text = format!("{na} {ha:.0} | {nb} {hb:.0} | Δ {delta:+.0} HU{tail}");
                self.cmp_views[side].measurements[i].text = text;
                let n = self.cmp.deltas.len() + 1;
                self.cmp.deltas.push((format!("P{n}"), ha, hb, delta));
            }
        }
    }

    /// Enhancement map or subtraction image of the current plane (B − A).
    fn subtraction_layer(&mut self, ui: &Ui, link: &Linked) -> Option<FusionLayer> {
        let Linked::Plane { grid, k, slab_mm, slab_mode, map, version, .. } = link else { return None };
        let (va, vb) = (self.compare_series(0)?.volume.clone()?, self.compare_series(1)?.volume.clone()?);
        let (_, _, reversed) = self.phase_names();
        let display = self.cmp.display;
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (grid.origin.to_array().map(f64::to_bits), grid.u.to_array().map(f64::to_bits), grid.v.to_array().map(f64::to_bits), grid.width, grid.height).hash(&mut h);
            (k.to_bits(), slab_mm.to_bits(), *slab_mode as u8, *version, display as u8, va.id, vb.id, reversed).hash(&mut h);
            h.finish()
        };
        if self.cmp.sub_cache.as_ref().is_none_or(|c| c.0 != key) {
            let (mut pa, mut pb) = (Vec::new(), Vec::new());
            crate::imaging::reslice::reslice(&va, grid, *k, *slab_mm as f64, *slab_mode, &mut pa);
            crate::imaging::reslice::reslice_mapped(&vb, grid, *k, *slab_mm as f64, *slab_mode, map, &mut pb);
            let (w, h) = (grid.width, grid.height);
            let real = |s: i16, q: crate::imaging::volume::Quant| if s == crate::imaging::volume::OUTSIDE { f32::NAN } else { q.real(s) };
            let (ra, rb): (Vec<f32>, Vec<f32>) = pa.iter().zip(&pb).map(|(&x, &y)| (real(x, va.quant), real(y, vb.quant))).unzip();
            // Without- and with-contrast values, whichever side each one is on.
            let (pre, post) = if reversed { (&rb, &ra) } else { (&ra, &rb) };
            // 3×3 mean of the difference: subtraction doubles the noise.
            let raw: Vec<f32> = pre.iter().zip(post).map(|(a, b)| b - a).collect();
            let diff: Vec<f32> = (0..w * h)
                .map(|i| {
                    let (x, y) = (i % w, i / w);
                    let (mut s, mut n) = (0.0, 0);
                    for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                        for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                            let d = raw[yy * w + xx];
                            if d.is_finite() {
                                s += d;
                                n += 1;
                            }
                        }
                    }
                    if n > 0 && raw[i].is_finite() { s / n as f32 } else { f32::NAN }
                })
                .collect();
            let pixels: Vec<Color32> = (0..w * h)
                .map(|i| {
                    let d = diff[i];
                    if !d.is_finite() {
                        return if display == CompareDisplay::Subtraction { Color32::BLACK } else { Color32::TRANSPARENT };
                    }
                    match display {
                        CompareDisplay::Subtraction => Color32::from_gray((((d + 100.0) / 300.0).clamp(0.0, 1.0) * 255.0) as u8),
                        _ => {
                            // Soft tissue that gained at least 15 HU: yellow (15) to red (80+).
                            let base = pre[i];
                            if !(-60.0..=250.0).contains(&base) || d < 15.0 {
                                return Color32::TRANSPARENT;
                            }
                            let t = ((d - 15.0) / 65.0).clamp(0.0, 1.0);
                            Color32::from_rgba_unmultiplied(255, (230.0 - 190.0 * t) as u8, (60.0 - 40.0 * t) as u8, 175)
                        }
                    }
                })
                .collect();
            let img = egui::ColorImage::new([w, h], pixels);
            let tex = match self.cmp.sub_cache.take() {
                Some((_, mut t, _)) => {
                    t.set(img, egui::TextureOptions::LINEAR);
                    t
                }
                None => ui.ctx().load_texture("cmp-subtraction", img, egui::TextureOptions::LINEAR),
            };
            self.cmp.sub_cache = Some((key, tex, [w, h]));
        }
        let (_, tex, size) = self.cmp.sub_cache.as_ref()?;
        Some(FusionLayer { tex: tex.id(), size: *size, style: Fusion::Blend(1.0) })
    }

    /// Opens the exam's phases without and with contrast side by side, ready for ΔHU readings.
    pub(crate) fn compare_phases(&mut self) {
        let study_uid = self.active_series().map(|s| s.study.uid.clone()).or_else(|| self.expanded_study.clone()).or_else(|| self.library.studies.first().map(|s| s.header.uid.clone()));
        let pair = study_uid.and_then(|u| self.library.studies.iter().find(|s| s.header.uid == u)).and_then(crate::phases::pick_pair);
        let Some((pre, post)) = pair else {
            self.toast(ToastKind::Info, "Não encontrei fases sem e com contraste (TC) neste exame. Arraste as séries para A e B na comparação.");
            return;
        };
        self.workspace = crate::app::Workspace::Compare;
        self.cmp.display = CompareDisplay::SideBySide;
        self.cmp.sync_position = true;
        self.active = 0;
        self.open_series(&pre, Target::Compare(0));
        self.open_series(&post, Target::Compare(1));
        self.tool = Tool::Probe;
        self.toast(ToastKind::Info, "Fases sem (A) e com contraste (B). Clique com a densidade (H) para ver o ΔHU; o mapa de realce está em Exibição.");
    }

    fn compare_event(&mut self, side: usize, ev: ViewEvents, linked: bool) {
        if ev.activated {
            self.active = side;
        }
        if let Some(uid) = ev.dropped_series {
            self.active = side;
            self.open_series(&uid, Target::Compare(side));
        }
        if let Some(f) = ev.swipe {
            self.cmp.swipe = f;
        }
        if let Some(hit) = ev.cursor
            && let Some(p) = hit.patient {
                let pa = if side == 1 && !linked { self.cmp.reg.inverse().apply(p) } else { p };
                self.cmp.cursor = Some(pa);
                self.cmp.mpr[0].point = pa;
                self.cmp.mpr[1].point = self.cmp.reg.apply(pa);
                if !self.compare_is_3d() {
                    if side != 0 {
                        self.cmp_views[0].go_to_point(pa);
                    }
                    if side != 1 && !linked {
                        self.cmp_views[1].go_to_point(self.cmp.reg.apply(pa));
                    }
                }
            }
        if let Some(hit) = ev.landmark {
            self.add_landmark(side, hit, linked);
        }
        if let Some(mut hit) = ev.bookmark
            && let Some(series) = self.compare_series(side) {
                if side == 1 && linked {
                    hit.patient = hit.patient.map(|p| self.cmp.reg.apply(p));
                }
                let mpr = self.compare_is_3d();
                let (window, zoom) = (self.cmp_views[side].window, self.cmp_views[side].zoom_factor());
                self.add_bookmark(&series, hit, window, zoom, mpr);
            }
    }

    pub(crate) fn inspector_compare(&mut self, ui: &mut Ui) {
        caption(ui, "Comparação de exames");
        let (a, b) = (self.compare_series(0), self.compare_series(1));
        let mut swap = false;
        card(ui, |ui| {
            for (side, s) in [(0, &a), (1, &b)] {
                ui.horizontal(|ui| {
                    let color = if side == 0 { COLOR_A } else { COLOR_B };
                    ui.label(RichText::new(if side == 0 { "A" } else { "B" }).font(font_bold(13.0)).color(color));
                    match s {
                        Some(s) => {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(s.info.title()).color(pal().text).size(12.5));
                                ui.label(RichText::new(format!("{} · {} imagens", format_date(&s.study.date), s.len())).color(pal().text_faint).size(11.5));
                            });
                        }
                        None => {
                            ui.label(RichText::new("Arraste uma série para a janela B (exame anterior).").color(pal().text_faint).size(12.0));
                        }
                    }
                });
            }
            if b.is_some() && ui.button(format!("{}  Trocar A e B", icons::SWAP)).clicked() {
                swap = true;
            }
        });
        if swap {
            self.cmp_views.swap(0, 1);
            self.cmp.pair = (String::new(), String::new());
            self.refresh_compare();
        }
        if b.is_none() {
            return;
        }
        let is_3d = self.compare_is_3d();

        caption(ui, "Exibição");
        card(ui, |ui| {
            let mut d = self.cmp.display;
            if segmented(
                ui,
                &mut d,
                &[
                    (CompareDisplay::SideBySide, icons::COLUMNS, "Lado a lado"),
                    (CompareDisplay::Blend, icons::INTERSECT, "Sobrepor"),
                    (CompareDisplay::Swipe, icons::SPLIT_HORIZONTAL, "Cortina"),
                    (CompareDisplay::Checker, icons::CHECKERBOARD, "Xadrez"),
                ],
            ) {
                self.set_compare_display(d);
            }
            match self.cmp.display {
                CompareDisplay::Blend => labeled_slider(ui, "Opacidade de B", &mut self.cmp.opacity, 0.0..=1.0, ""),
                CompareDisplay::Checker => labeled_slider(ui, "Tamanho do quadro (mm)", &mut self.cmp.checker_mm, 4.0..=80.0, ""),
                CompareDisplay::Swipe => {
                    ui.label(RichText::new("Arraste a divisória: à esquerda A, à direita B.").color(pal().text_faint).size(12.0));
                }
                CompareDisplay::SideBySide => {}
                CompareDisplay::Enhancement => {
                    ui.label(RichText::new("Amarelo → vermelho: ganho de 15 a 80+ HU entre as fases, só em partes moles da fase sem contraste.").color(pal().text_faint).size(12.0));
                }
                CompareDisplay::Subtraction => {
                    ui.label(RichText::new("Imagem de subtração (com − sem contraste): cinza médio = sem mudança, claro = realce.").color(pal().text_faint).size(12.0));
                }
            }
            if is_3d {
                ui.horizontal_wrapped(|ui| {
                    for (d, label) in [(CompareDisplay::Enhancement, "Mapa de realce"), (CompareDisplay::Subtraction, "Subtração")] {
                        let on = self.cmp.display == d;
                        if super::panels::chip(ui, label, on, 90.0).on_hover_text("Diferença entre as fases (B − A), alinhadas").clicked() {
                            self.set_compare_display(if on { CompareDisplay::SideBySide } else { d });
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.label("Cores de B");
                let v = &mut self.cmp_views[1];
                egui::ComboBox::from_id_salt("cmp-cmap").selected_text(v.colormap.label()).show_ui(ui, |ui| {
                    for c in Colormap::ALL {
                        ui.selectable_value(&mut v.colormap, c, c.label());
                    }
                });
            });
            if is_3d {
                ui.add_space(4.0);
                let mut plane = self.cmp.plane;
                if segmented(ui, &mut plane, &[(PlaneKind::Axial, "", "Axial"), (PlaneKind::Coronal, "", "Coronal"), (PlaneKind::Sagittal, "", "Sagital")]) {
                    self.cmp.plane = plane;
                    self.refresh_compare();
                }
                ui.horizontal(|ui| {
                    ui.label("Espessura (slab)");
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let t = self.cmp.mpr[0].slab_mm;
                        ui.label(RichText::new(if t > 0.0 { format!("{t:.0} mm") } else { "fina".into() }).color(pal().text_dim));
                    });
                });
                let mut slab = self.cmp.mpr[0].slab_mm;
                full_slider(ui, &mut slab, 0.0..=30.0);
                let mode = if slab > 0.0 { SlabMode::Avg } else { self.cmp.mpr[0].slab_mode };
                for m in &mut self.cmp.mpr {
                    m.slab_mm = slab.round();
                    m.slab_mode = mode;
                }
            }
        });

        if is_3d {
            caption(ui, "Realce entre fases (ΔHU)");
            card(ui, |ui| {
                let (na, nb, _) = self.phase_names();
                if self.cmp.deltas.is_empty() {
                    ui.label(
                        RichText::new("Com a densidade (H), clique no mesmo ponto: o Slyced lê as duas fases (média em 2,5 mm) e mostra a diferença.")
                            .color(pal().text_faint)
                            .size(12.0),
                    );
                }
                for (name, ha, hb, d) in &self.cmp.deltas {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).font(font_bold(12.0)).color(MEASURE));
                        ui.label(RichText::new(format!("{na} {ha:.0} · {nb} {hb:.0}")).size(12.0).color(pal().text_dim));
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            let color = if *d >= 20.0 { SUCCESS } else if *d >= 10.0 { WARNING } else { pal().text };
                            ui.label(RichText::new(format!("Δ {d:+.0} HU")).font(font_bold(13.0)).color(color));
                        });
                    });
                    if na != "A" {
                        ui.label(RichText::new(format!("    {}", crate::phases::enhancement_label(*d))).size(11.5).color(pal().text_faint));
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Δ < 10 HU: sem realce · 10–20: duvidoso · ≥ 20: realce. Sem realce numa alça ou área hiperdensa sugere sofrimento de alça ou hematoma.").size(11.0).color(pal().text_faint));
            });
        }

        caption(ui, "Sincronização");
        card(ui, |ui| {
            let fused = self.cmp.display != CompareDisplay::SideBySide;
            ui.add_enabled_ui(!fused, |ui| {
                ui.checkbox(&mut self.cmp.sync_position, "Posição, cortes e cursor");
            });
            ui.checkbox(&mut self.cmp.sync_view, "Zoom, deslocamento e rotação");
            ui.checkbox(&mut self.cmp.sync_window, "Janela/nível");
            ui.label(RichText::new("Ctrl+clique marca o mesmo ponto nos dois exames.").color(pal().text_faint).size(11.5));
        });

        caption(ui, "Alinhamento");
        let mut action = None;
        card(ui, |ui| {
            ui.label(RichText::new(self.cmp.source.describe()).color(pal().text_dim).size(12.0));
            if is_3d {
                let t = self.cmp.reg.t;
                if self.cmp.source != RegSource::SameFrame {
                    ui.label(RichText::new(format!("Deslocamento {:.1} mm · rotação {:.1}°", t.length(), self.cmp.reg.angle_deg())).color(pal().text_faint).size(11.5));
                }
            } else {
                let s = self.cmp.sim;
                ui.label(RichText::new(format!("Escala {:.3}× · rotação {:.1}°", s.scale(), s.angle_deg())).color(pal().text_faint).size(11.5));
            }
            ui.add_space(6.0);
            if is_3d {
                if self.cmp.registering.is_some() {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        spinner(ui.painter(), r.center(), 6.0, self.time, pal().accent);
                        ui.label(RichText::new("Alinhando…").color(pal().text_dim));
                        if ui.small_button("Cancelar").clicked() {
                            action = Some("cancel");
                        }
                    });
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                } else if ui.button(format!("{}  Alinhar automaticamente", icons::MAGIC_WAND)).on_hover_text("Rotação e translação por informação mútua (funciona entre fases e modalidades).").clicked() {
                    action = Some("auto");
                }
            }
            ui.horizontal(|ui| {
                let on = self.tool == Tool::Landmark;
                if ui.selectable_label(on, format!("{}  Pontos de referência", icons::ANCHOR)).clicked() {
                    action = Some(if on { "tool-off" } else { "tool-on" });
                }
                if ui.button(format!("{}  Redefinir", icons::ARROW_COUNTER_CLOCKWISE)).clicked() {
                    action = Some("reset");
                }
            });
            if self.tool == Tool::Landmark {
                ui.label(
                    RichText::new(if is_3d {
                        "Clique no mesmo ponto anatômico em A e em B (cada lado navega sozinho enquanto a ferramenta está ativa). 1 par = translação; 3 ou mais = também rotação."
                    } else {
                        "Clique no mesmo ponto em A e em B. 1 par = translação; 2 ou mais = também escala e rotação."
                    })
                    .color(pal().text_faint)
                    .size(11.5),
                );
            }
            let mut remove = None;
            for (i, l) in self.cmp.landmarks.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}", i + 1)).font(font_bold(12.0)).color(pal().text));
                    let mark = |ok: bool, c: Color32| RichText::new(if ok { icons::CHECK_CIRCLE } else { icons::CIRCLE_DASHED }).color(if ok { c } else { pal().text_faint });
                    ui.label(RichText::new("A").color(COLOR_A).size(11.5));
                    ui.label(mark(l.a.is_some(), COLOR_A));
                    ui.label(RichText::new("B").color(COLOR_B).size(11.5));
                    ui.label(mark(l.b.is_some(), COLOR_B));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if icon_button_sized(ui, icons::X, "Remover", false, 22.0).clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = remove {
                self.cmp.landmarks.remove(i);
                self.solve_landmarks();
            }
        });
        match action {
            Some("auto") => self.start_auto_registration(),
            Some("cancel") => {
                if let Some(f) = self.cmp.registering.take() {
                    f.store(true, Ordering::Relaxed);
                }
            }
            Some("tool-on") => self.tool = Tool::Landmark,
            Some("tool-off") => self.tool = Tool::WindowLevel,
            Some("reset") => self.reset_registration(),
            _ => {}
        }

        caption(ui, "Medidas pareadas (imagem atual)");
        card(ui, |ui| {
            let linked = self.compare_linked() && !self.compare_fused();
            if !linked {
                ui.label(RichText::new("Ative a sincronização de posição para comparar medidas.").color(pal().text_faint).size(12.0));
                return;
            }
            let (va, vb) = (self.cmp_views[0].values.clone(), self.cmp_views[1].values.clone());
            let mut rows = 0;
            for a in &va {
                let best = vb
                    .iter()
                    .filter(|b| b.kind == a.kind)
                    .map(|b| (b, b.center.distance(a.center)))
                    .filter(|(_, d)| *d < 40.0)
                    .min_by(|x, y| x.1.total_cmp(&y.1));
                let Some((b, _)) = best else { continue };
                rows += 1;
                let (icon, name) = match a.kind {
                    MeasureKind::Length => (icons::RULER, "Distância"),
                    MeasureKind::Angle => (icons::ANGLE, "Ângulo"),
                    MeasureKind::Ellipse => (icons::CIRCLE, "Área"),
                    MeasureKind::Probe => (icons::TARGET, "Densidade"),
                    MeasureKind::Arrow => continue,
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon).color(MEASURE));
                    ui.label(RichText::new(name).size(12.0));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let change = if a.value.abs() > 1e-3 { (b.value - a.value) / a.value.abs() * 100.0 } else { 0.0 };
                        let delta = if a.kind == MeasureKind::Probe { format!("{:+.0}", b.value - a.value) } else { format!("{change:+.1}%") };
                        let color = if change.abs() < 5.0 { pal().text_dim } else if change > 0.0 { WARNING } else { SUCCESS };
                        ui.label(RichText::new(delta).font(font_bold(12.5)).color(color));
                        ui.label(RichText::new(format!("{:.1} → {:.1} {}", a.value, b.value, a.unit)).size(12.0).color(pal().text));
                    });
                });
                if let (Some(ma), Some(mb)) = (a.mean, b.mean) {
                    ui.label(RichText::new(format!("    média {ma:.0} → {mb:.0} ({:+.0})", mb - ma)).size(11.5).color(pal().text_faint));
                }
            }
            if rows == 0 {
                ui.label(RichText::new("Meça a mesma lesão em A e em B para ver a variação.").color(pal().text_faint).size(12.0));
            }
            if !self.cmp_views[0].measurements.is_empty() && ui.button(format!("{}  Copiar medidas de A para B", icons::COPY)).on_hover_text("Cria em B as mesmas medidas, para ajustar").clicked() {
                self.cmp_views[1].measurements = self.cmp_views[0].measurements.clone();
            }
        });
    }
}
