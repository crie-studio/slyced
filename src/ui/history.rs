//! Undo and redo (Ctrl+Z / Ctrl+Y) of what the tools change on screen:
//! measurements, highlights, filters, window, orientation, the 3D cursor, dense
//! foci, the tracker path and oblique planes. Navigation (slice, zoom, pan) is
//! not undone. A state is recorded once the mouse is released, so a whole drag
//! is one step; opening other images starts a new history.

use super::review::CalculiResult;
use super::viewport::{Content, Measurement, Viewport};
use crate::app::{SlycedApp, ToastKind, Workspace};
use crate::dicom::series::LoadedSeries;
use crate::imaging::highlight::Highlights;
use crate::imaging::lut::{Colormap, Window};
use crate::imaging::reslice::SlabMode;
use glam::DVec3;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

const MAX_STEPS: usize = 100;

/// What undo restores on one viewport.
#[derive(Clone)]
struct ViewState {
    measurements: Vec<Measurement>,
    window: Window,
    invert: bool,
    colormap: Colormap,
    rot: u8,
    flip: (bool, bool),
    highlights: Highlights,
    restore: Option<(Window, bool)>,
    slab: (f32, SlabMode),
    denoise: u8,
}

impl ViewState {
    fn of(v: &Viewport) -> Self {
        Self {
            measurements: v.measurements.clone(),
            window: v.window,
            invert: v.invert,
            colormap: v.colormap,
            rot: v.rot,
            flip: (v.flip_h, v.flip_v),
            highlights: v.highlights,
            restore: v.restore,
            slab: (v.slab_mm, v.slab_mode),
            denoise: v.denoise,
        }
    }

    fn apply(&self, v: &mut Viewport) {
        v.measurements = self.measurements.clone();
        v.window = self.window;
        v.invert = self.invert;
        v.colormap = self.colormap;
        v.rot = self.rot;
        (v.flip_h, v.flip_v) = self.flip;
        v.highlights = self.highlights;
        v.restore = self.restore;
        (v.slab_mm, v.slab_mode) = self.slab;
        v.denoise = self.denoise;
    }
}

/// Shared MPR state of the MPR workspace and of the two comparison sides.
type PlanesState = [(f32, SlabMode, [(DVec3, DVec3); 3]); 3];

#[derive(Clone)]
pub struct Snapshot {
    /// Images on screen: a step only applies to the same images.
    contents: u64,
    fingerprint: u64,
    views: Vec<ViewState>,
    cursor: Option<(DVec3, String, String)>,
    cmp_cursor: Option<DVec3>,
    calculi: Option<CalculiResult>,
    curve: Vec<DVec3>,
    curve_series: Option<Arc<LoadedSeries>>,
    planes: PlanesState,
}

#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    current: Option<Snapshot>,
}

impl History {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

fn bits(p: DVec3) -> [u64; 3] {
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
}

impl SlycedApp {
    /// Viewports of the current workspace, in a fixed order.
    fn history_views(&self) -> Vec<&Viewport> {
        match self.workspace {
            Workspace::Viewer => self.views.iter().take(self.layout.count()).collect(),
            Workspace::Mpr => self.mpr_views.iter().collect(),
            Workspace::Volume => vec![&self.vr_view],
            Workspace::Compare => self.cmp_views.iter().collect(),
        }
    }

    fn history_views_mut(&mut self) -> Vec<&mut Viewport> {
        let n = self.layout.count();
        match self.workspace {
            Workspace::Viewer => self.views.iter_mut().take(n).collect(),
            Workspace::Mpr => self.mpr_views.iter_mut().collect(),
            Workspace::Volume => vec![&mut self.vr_view],
            Workspace::Compare => self.cmp_views.iter_mut().collect(),
        }
    }

    fn planes_state(&self) -> PlanesState {
        let m = |s: &super::viewport::MprShared| (s.slab_mm, s.slab_mode, s.axes);
        [m(&self.mpr), m(&self.cmp.mpr[0]), m(&self.cmp.mpr[1])]
    }

    /// Identity of the images on screen and a hash of everything undo restores.
    fn history_keys(&self) -> (u64, u64) {
        let mut c = DefaultHasher::new();
        let mut h = DefaultHasher::new();
        (self.workspace as u8).hash(&mut c);
        for v in self.history_views() {
            // The straightened image (CPR) replaces a view of the same series: same images.
            v.content.series().map(|s| Arc::as_ptr(s) as usize).hash(&mut c);
            matches!(v.content, Content::Volume(_)).hash(&mut c);
            for m in &v.measurements {
                (m.kind as u8, m.key, &m.text).hash(&mut h);
                m.pts.iter().for_each(|p| (p.x.to_bits(), p.y.to_bits()).hash(&mut h));
            }
            (v.window.center.to_bits(), v.window.width.to_bits(), v.invert, v.colormap as u8, v.rot, v.flip_h, v.flip_v).hash(&mut h);
            (v.highlights.gas, v.highlights.fat, v.highlights.dense.map(f32::to_bits)).hash(&mut h);
            (v.slab_mm.to_bits(), v.slab_mode as u8, v.denoise).hash(&mut h);
        }
        self.cursor.as_ref().map(|c| bits(c.0)).hash(&mut h);
        self.cmp.cursor.map(bits).hash(&mut h);
        self.calculi.as_ref().map(|r| (r.items.len(), r.threshold.to_bits(), &r.series_uid)).hash(&mut h);
        self.curve.iter().for_each(|p| bits(*p).hash(&mut h));
        for (slab, mode, axes) in self.planes_state() {
            (slab.to_bits(), mode as u8).hash(&mut h);
            axes.iter().for_each(|(u, v)| (bits(*u), bits(*v)).hash(&mut h));
        }
        (c.finish(), h.finish())
    }

    fn snapshot(&self, contents: u64, fingerprint: u64) -> Snapshot {
        Snapshot {
            contents,
            fingerprint,
            views: self.history_views().into_iter().map(ViewState::of).collect(),
            cursor: self.cursor.clone(),
            cmp_cursor: self.cmp.cursor,
            calculi: self.calculi.clone(),
            curve: self.curve.clone(),
            curve_series: self.curve_series.clone(),
            planes: self.planes_state(),
        }
    }

    /// Records the state once per change (called at the end of every frame).
    pub(crate) fn track_history(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.any_down()) || ctx.egui_wants_keyboard_input() {
            return;
        }
        let (contents, fingerprint) = self.history_keys();
        match &self.history.current {
            Some(cur) if cur.contents == contents => {
                if cur.fingerprint != fingerprint {
                    let snap = self.snapshot(contents, fingerprint);
                    if let Some(prev) = self.history.current.replace(snap) {
                        self.history.undo.push(prev);
                        if self.history.undo.len() > MAX_STEPS {
                            self.history.undo.remove(0);
                        }
                    }
                    self.history.redo.clear();
                }
            }
            _ => {
                self.history = History { current: Some(self.snapshot(contents, fingerprint)), ..Default::default() };
            }
        }
    }

    fn apply_snapshot(&mut self, s: &Snapshot) {
        for (v, state) in self.history_views_mut().into_iter().zip(&s.views) {
            state.apply(v);
        }
        self.cursor = s.cursor.clone();
        self.cmp.cursor = s.cmp_cursor;
        self.calculi = s.calculi.clone();
        let [mpr, a, b] = s.planes;
        for (m, (slab, mode, axes)) in [&mut self.mpr, &mut self.cmp.mpr[0]].into_iter().zip([mpr, a]) {
            (m.slab_mm, m.slab_mode, m.axes) = (slab, mode, axes);
        }
        (self.cmp.mpr[1].slab_mm, self.cmp.mpr[1].slab_mode, self.cmp.mpr[1].axes) = b;
        if self.curve != s.curve {
            self.curve = s.curve.clone();
            self.curve_series = s.curve_series.clone();
            if self.curve.len() >= 2 {
                self.rebuild_cpr();
            } else {
                self.cpr_close();
            }
        }
    }

    pub(crate) fn undo(&mut self) {
        let Some(prev) = self.history.undo.pop() else {
            self.toast(ToastKind::Info, tr!("Nada para desfazer.", "Nothing to undo."));
            return;
        };
        let what = self.history.current.as_ref().map(|cur| describe(cur, &prev)).unwrap_or(tr!("alteração", "change"));
        if let Some(cur) = self.history.current.take() {
            self.history.redo.push(cur);
        }
        self.apply_snapshot(&prev);
        self.history.current = Some(prev);
        self.toast(ToastKind::Info, trf!("Desfeito: {what} · Ctrl+Y refaz", "Undone: {what} · Ctrl+Y to redo"));
    }

    pub(crate) fn redo(&mut self) {
        let Some(next) = self.history.redo.pop() else {
            self.toast(ToastKind::Info, tr!("Nada para refazer.", "Nothing to redo."));
            return;
        };
        let what = self.history.current.as_ref().map(|cur| describe(cur, &next)).unwrap_or(tr!("alteração", "change"));
        if let Some(cur) = self.history.current.take() {
            self.history.undo.push(cur);
        }
        self.apply_snapshot(&next);
        self.history.current = Some(next);
        self.toast(ToastKind::Info, trf!("Refeito: {what}", "Redone: {what}"));
    }
}

/// What differs between two states, in words (first difference found).
fn describe(a: &Snapshot, b: &Snapshot) -> &'static str {
    for (x, y) in a.views.iter().zip(&b.views) {
        if x.measurements.len() != y.measurements.len() {
            return tr!("medida", "measurement");
        }
        if x.highlights != y.highlights {
            return tr!("realce", "highlight");
        }
        if x.slab != y.slab {
            return "slab";
        }
        if x.denoise != y.denoise {
            return tr!("filtro de ruído", "noise reduction");
        }
        if x.colormap != y.colormap {
            return tr!("mapa de cores", "color map");
        }
        if x.rot != y.rot || x.flip != y.flip {
            return tr!("rotação", "rotation");
        }
        if x.invert != y.invert {
            return tr!("inversão de cores", "color inversion");
        }
        if x.window != y.window {
            return tr!("janela", "window");
        }
        if x.measurements.iter().zip(&y.measurements).any(|(m, n)| m.pts != n.pts || m.text != n.text) {
            return tr!("medida", "measurement");
        }
    }
    if a.cursor.as_ref().map(|c| c.0) != b.cursor.as_ref().map(|c| c.0) || a.cmp_cursor != b.cmp_cursor {
        return tr!("cursor 3D", "3D cursor");
    }
    if a.calculi.is_some() != b.calculi.is_some() {
        return tr!("focos de cálculo", "stone foci");
    }
    if a.curve != b.curve {
        return tr!("trajeto do rastreador", "tracker path");
    }
    if a.planes.iter().zip(&b.planes).any(|(x, y)| x.2 != y.2) {
        return tr!("planos oblíquos", "oblique planes");
    }
    if a.planes.iter().zip(&b.planes).any(|(x, y)| x.0 != y.0) {
        return "slab";
    }
    tr!("alteração", "change")
}
