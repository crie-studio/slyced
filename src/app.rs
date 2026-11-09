//! Application state and frame loop.

use crate::bookmarks::Bookmark;
use crate::config::{Paths, Settings};
use crate::dicom::model::Library;
use crate::dicom::scan::ScanProgress;
use crate::dicom::series::LoadedSeries;
use crate::dicom::tags::TagRow;
use crate::imaging::reslice::{PlaneKind, SlabMode};
use crate::jobs::{Jobs, LoadProgress, Msg};
use crate::pacs::scp::{ScpEvent, StorageScp};
use crate::render3d::renderer::GpuVolumeData;
use crate::ui::compare::Compare;
use crate::ui::pacs_window::PacsUi;
use crate::ui::theme;
use crate::ui::viewport::{Content, MprShared, Tool, ViewCtx, ViewEvents, Viewport};
use egui::{Rect, TextureHandle, Vec2};
use glam::DVec3;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Workspace {
    Viewer,
    Mpr,
    Volume,
    Compare,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    One,
    Cols2,
    Rows2,
    Grid4,
    Grid6,
}

impl Layout {
    pub const ALL: [Layout; 5] = [Layout::One, Layout::Cols2, Layout::Rows2, Layout::Grid4, Layout::Grid6];

    pub fn dims(self) -> (usize, usize) {
        match self {
            Layout::One => (1, 1),
            Layout::Cols2 => (2, 1),
            Layout::Rows2 => (1, 2),
            Layout::Grid4 => (2, 2),
            Layout::Grid6 => (3, 2),
        }
    }

    pub fn count(self) -> usize {
        let (c, r) = self.dims();
        c * r
    }

    pub fn label(self) -> &'static str {
        match self {
            Layout::One => "1 × 1",
            Layout::Cols2 => "2 colunas",
            Layout::Rows2 => "2 linhas",
            Layout::Grid4 => "2 × 2",
            Layout::Grid6 => "3 × 2",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    View(usize),
    Mpr,
    Volume,
    /// Side A (0) or B (1) of the comparison.
    Compare(usize),
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub until: f64,
    /// Button shown in the notice (e.g. "Desfazer").
    pub action: Option<ToastAction>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastAction {
    /// Puts back the exams closed last.
    ReopenClosed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// Automation for documentation/testing: `--screenshot out.png`.
pub struct Automation {
    pub screenshot: Option<PathBuf>,
    pub workspace: Option<Workspace>,
    pub layout: Option<Layout>,
    /// Opens the PACS window and queries the first server.
    pub pacs_query: bool,
    /// Opens the "Sobre o Slyced" window.
    pub about: bool,
    /// Zoom applied to every viewport before the screenshot (same path as the +/− keys).
    pub zoom: Option<f32>,
    /// Scripted steps (`--do`), run one at a time once the app is idle.
    pub actions: Vec<String>,
    pub requested: bool,
    pub settle_since: f64,
}

pub struct SlycedApp {
    pub(crate) paths: Paths,
    pub(crate) settings: Settings,
    pub(crate) jobs: Jobs,
    pub(crate) library: Library,
    pub(crate) thumbs: HashMap<String, TextureHandle>,
    pub(crate) thumbs_requested: std::collections::HashSet<String>,
    pub(crate) loaded: HashMap<String, (Arc<LoadedSeries>, f64)>,
    pub(crate) loading: HashMap<String, Arc<LoadProgress>>,
    pub(crate) pending: Vec<(Target, String)>,
    pub(crate) scan: Option<Arc<ScanProgress>>,
    pub(crate) workspace: Workspace,
    pub(crate) layout: Layout,
    pub(crate) views: Vec<Viewport>,
    pub(crate) mpr_views: Vec<Viewport>,
    pub(crate) vr_view: Viewport,
    pub(crate) active: usize,
    pub(crate) maximized: Option<usize>,
    pub(crate) tool: Tool,
    pub(crate) mpr: MprShared,
    pub(crate) gpu: Option<Arc<GpuVolumeData>>,
    pub(crate) gpu_pending: Option<u64>,
    pub(crate) gpu_limits: (u32, usize),
    pub(crate) adapter: String,
    /// Why the 3D view cannot run on this computer, if it cannot.
    pub(crate) gpu_3d_issue: Option<String>,
    pub(crate) pacs: PacsUi,
    pub(crate) scp: Option<StorageScp>,
    pub(crate) incoming: Vec<PathBuf>,
    pub(crate) incoming_since: f64,
    pub(crate) scans_active: usize,
    pub(crate) open_after_scan: Option<String>,
    pub(crate) toasts: Vec<Toast>,
    pub(crate) status: String,
    pub(crate) tags: Option<(String, Vec<TagRow>)>,
    pub(crate) tag_filter: String,
    pub(crate) show_settings: bool,
    pub(crate) show_help: bool,
    pub(crate) show_about: bool,
    pub(crate) left_open: bool,
    pub(crate) right_open: bool,
    pub(crate) search: String,
    pub(crate) expanded_study: Option<String>,
    /// Studies whose series are listed in the browser.
    pub(crate) expanded: std::collections::HashSet<String>,
    /// Series as thumbnails grid (true) or as a list.
    pub(crate) browser_grid: bool,
    pub(crate) automation: Automation,
    pub(crate) export_request: Option<Rect>,
    pub(crate) view_rects: Vec<Rect>,
    pub(crate) time: f64,
    pub(crate) sync_scroll: bool,
    last_sync: Option<(usize, usize, usize)>,
    /// 3D cursor: point, study and Frame of Reference it belongs to.
    pub(crate) cursor: Option<(DVec3, String, String)>,
    pub(crate) bookmarks: Vec<Bookmark>,
    pub(crate) bookmark_edit: Option<u64>,
    pub(crate) bookmark_focus: bool,
    pub(crate) pending_bookmark: Option<u64>,
    pub(crate) pending_protocol: Option<usize>,
    /// Path clicked for the curved reformation, and the plane it started on.
    pub(crate) curve: Vec<DVec3>,
    pub(crate) curve_normal: DVec3,
    /// Series the path was clicked on.
    pub(crate) curve_series: Option<Arc<LoadedSeries>>,
    /// 2D viewer window showing the straightened image, and what it showed before.
    pub(crate) cpr_view: Option<(usize, Content)>,
    pub(crate) cpr_kind: crate::ui::review::TubeKind,
    pub(crate) cpr_angle: f32,
    pub(crate) cpr_width: f32,
    pub(crate) magnifier: f32,
    pub(crate) cmp: Compare,
    pub(crate) cmp_views: Vec<Viewport>,
    /// Threshold of the dense-foci highlight and search (HU).
    pub(crate) dense_threshold: f32,
    pub(crate) calculi: Option<crate::ui::review::CalculiResult>,
    pub(crate) calculi_busy: bool,
    pub(crate) hernia: crate::ui::hernia::Hernia,
    pub(crate) ellipsoid: crate::ui::ellipsoid::EllipsoidCalc,
    pub(crate) previews: crate::ui::previews::Previews,
    /// Step of the welcome tour being shown.
    pub(crate) tour: Option<usize>,
    /// Controls the tour points at (open, PACS, tools, undo/restore, settings), from the last frame.
    pub(crate) tour_targets: [Rect; 5],
    pub(crate) show_news: bool,
    /// Size of the downloaded exams (measured in the background).
    pub(crate) store_usage: Option<crate::store::StoreUsage>,
    /// Back and forward through reading positions (Alt+← / Alt+→).
    pub(crate) nav: crate::ui::navigation::NavHistory,
    /// Focus mode: panels hidden; holds whether they were open.
    pub(crate) focus: Option<(bool, bool)>,
    /// Tab was pressed outside a text field (taken before egui moves the keyboard focus).
    tab_pressed: bool,
    /// The image being exported goes to the clipboard instead of a file.
    export_clipboard: bool,
    /// Section shown in the settings window.
    pub(crate) settings_tab: crate::ui::dialogs::SettingsTab,
    /// "Apagar agora" is waiting for confirmation.
    pub(crate) confirm_clear_store: bool,
    /// Top-bar menu to open on the next frame (automation: `--do menu:reset`).
    pub(crate) open_menu: Option<String>,
    /// Exams closed recently, in batches closed together: `(time, studies)`.
    pub(crate) closed: Vec<(f64, Vec<crate::dicom::model::StudyInfo>)>,
    /// "Fechar todos os exames" is waiting for confirmation.
    pub(crate) confirm_close_all: bool,
    /// Width the top bar needs with text buttons (measured when they are shown).
    pub(crate) topbar_full: f32,
    /// Undo and redo of what the tools changed.
    pub(crate) history: crate::ui::history::History,
    /// Tool last used in each toolbar group (Navegar, Medir, Leitura).
    pub(crate) group_tool: [Tool; 3],
    /// Theme and interface size currently applied.
    applied_look: Option<(crate::config::Theme, f32)>,
    /// Each patient's exams on the PACS (timeline), by patient ID.
    pub(crate) priors: HashMap<String, crate::ui::timeline::Priors>,
    /// Exams being downloaded from the timeline to be compared when read.
    pub(crate) compare_after_retrieve: std::collections::HashSet<String>,
    /// "Ir para" box (Ctrl+G).
    pub(crate) goto: Option<crate::ui::navigation::GoTo>,
    /// Reading full screen (F11): only the images and small controls.
    pub(crate) reading: bool,
    /// The reading full screen also asked Windows for full screen.
    pub(crate) reading_os: bool,
    pub(crate) reading_since: f64,
    /// Exams list opened from the left edge of the reading full screen.
    pub(crate) reading_drawer: bool,
    /// Keeps the reading controls visible (automation: `reading:bar`, `reading:drawer`).
    pub(crate) reading_force: Option<String>,
}

impl SlycedApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        paths: Paths,
        open: Vec<PathBuf>,
        automation: Automation,
        graphics: crate::Graphics,
    ) -> Self {
        theme::install(&cc.egui_ctx);
        #[cfg(windows)]
        theme::style_title_bar(cc);

        let mut gpu_limits = (2048u32, 256 * 1024 * 1024usize);
        let mut adapter = String::from("GPU indisponível");
        let mut gpu_3d_issue = None;
        if let Some(rs) = &cc.wgpu_render_state {
            use crate::render3d::renderer::{VolumeRenderer, GPU_FAULT};
            use eframe::wgpu;
            let limits = rs.device.limits();
            gpu_limits.0 = limits.max_texture_dimension_3d.min(2048);
            let info = rs.adapter.get_info();
            adapter = format!("{} ({:?})", info.name, info.backend);
            // wgpu panics on unhandled errors by default; a bad driver must only cost the 3D view.
            rs.device.on_uncaptured_error(std::sync::Arc::new(|e: wgpu::Error| {
                log::error!("erro da GPU: {e}");
                GPU_FAULT.store(true, Ordering::Relaxed);
            }));
            let scope = rs.device.push_error_scope(wgpu::ErrorFilter::Validation);
            let renderer = VolumeRenderer::new(&rs.device, rs.target_format);
            match pollster::block_on(scope.pop()) {
                None => {
                    rs.renderer.write().callback_resources.insert(renderer);
                }
                Some(e) => {
                    log::error!("renderizador 3D recusado pelo driver: {e}");
                    gpu_3d_issue = Some("O driver de vídeo não aceitou o renderizador 3D.".to_string());
                }
            }
        } else if graphics == crate::Graphics::OpenGl {
            if let Some(gl) = &cc.gl {
                use eframe::glow::HasContext;
                adapter = format!("{} (OpenGL)", unsafe { gl.get_parameter_string(eframe::glow::RENDERER) });
            }
            gpu_3d_issue = Some(
                "Modo de compatibilidade: a placa de vídeo ou o driver deste computador não permite a reconstrução 3D (ela precisa de DirectX 12, Vulkan ou OpenGL 3.3). 2D e MPR funcionam normalmente."
                    .to_string(),
            );
        }
        log::info!("GPU: {adapter}");

        let mut settings = Settings::load(&paths.config);
        let renamed = settings.drop_old_name();
        if renamed.is_some() {
            settings.save(&paths.config);
        }
        let jobs = Jobs::new(cc.egui_ctx.clone());
        let mut app = Self {
            pacs: PacsUi::new(&settings),
            paths,
            settings,
            jobs,
            library: Library::default(),
            thumbs: HashMap::new(),
            thumbs_requested: Default::default(),
            loaded: HashMap::new(),
            loading: HashMap::new(),
            pending: Vec::new(),
            scan: None,
            workspace: Workspace::Viewer,
            layout: Layout::One,
            views: (0..6).map(|_| Viewport::new()).collect(),
            mpr_views: (0..4).map(|_| Viewport::new()).collect(),
            vr_view: Viewport::new(),
            active: 0,
            maximized: None,
            tool: Tool::WindowLevel,
            mpr: MprShared::new(),
            gpu: None,
            gpu_pending: None,
            gpu_limits,
            adapter,
            gpu_3d_issue,
            scp: None,
            incoming: Vec::new(),
            incoming_since: 0.0,
            scans_active: 0,
            open_after_scan: None,
            toasts: Vec::new(),
            status: String::new(),
            tags: None,
            tag_filter: String::new(),
            show_settings: false,
            show_help: false,
            show_about: false,
            left_open: true,
            right_open: true,
            search: String::new(),
            expanded_study: None,
            expanded: Default::default(),
            browser_grid: false,
            automation,
            export_request: None,
            view_rects: Vec::new(),
            time: 0.0,
            sync_scroll: true,
            last_sync: None,
            cursor: None,
            bookmarks: Vec::new(),
            bookmark_edit: None,
            bookmark_focus: false,
            pending_bookmark: None,
            pending_protocol: None,
            curve: Vec::new(),
            curve_normal: DVec3::Y,
            curve_series: None,
            cpr_view: None,
            cpr_kind: Default::default(),
            cpr_angle: 0.0,
            cpr_width: 40.0,
            magnifier: 3.0,
            cmp: Compare::default(),
            cmp_views: (0..2).map(|_| Viewport::new()).collect(),
            dense_threshold: 200.0,
            calculi: None,
            calculi_busy: false,
            hernia: Default::default(),
            open_menu: None,
            closed: Vec::new(),
            confirm_close_all: false,
            topbar_full: 0.0,
            applied_look: None,
            priors: HashMap::new(),
            compare_after_retrieve: Default::default(),
            goto: None,
            reading: false,
            reading_os: false,
            reading_since: 0.0,
            reading_drawer: false,
            reading_force: None,
            group_tool: [Tool::WindowLevel, Tool::Length, Tool::Bookmark],
            history: Default::default(),
            ellipsoid: Default::default(),
            previews: Default::default(),
            tour: None,
            tour_targets: [Rect::NOTHING; 5],
            show_news: false,
            store_usage: None,
            settings_tab: Default::default(),
            nav: Default::default(),
            focus: None,
            tab_pressed: false,
            export_clipboard: false,
            confirm_clear_store: false,
        };
        app.load_bookmarks();
        // Old downloads go away at startup (nothing is open yet).
        match app.settings.store_keep_days {
            0 => app.jobs.measure_store(app.paths.store.clone()),
            days => app.jobs.clean_store(app.paths.store.clone(), Some(days), Default::default()),
        }
        if app.automation.screenshot.is_none() && app.automation.actions.is_empty() {
            app.greet();
        }
        if let Some(l) = app.automation.layout {
            app.layout = l;
        }
        app.show_about = app.automation.about;
        if app.automation.pacs_query && !app.settings.nodes.is_empty() {
            app.pacs.open = true;
            app.pacs.range = crate::ui::pacs_window::DateRange::All;
            app.pacs.searching = true;
            app.jobs.find_studies(app.settings.nodes[0].clone(), app.settings.local_ae.clone(), Default::default());
        }
        if let Some(notice) = app.paths.notice.clone() {
            app.toast(ToastKind::Info, notice);
        }
        if let Some(notice) = renamed {
            app.toast(ToastKind::Info, notice);
        }
        if app.settings.scp_autostart {
            app.start_scp();
        }
        if !open.is_empty() {
            app.open_paths(open, true);
        }
        app
    }

    // ------------------------------------------------------------------ actions

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        let text = text.into();
        match kind {
            ToastKind::Error => log::warn!("{text}"),
            _ => log::info!("{text}"),
        }
        self.toasts.push(Toast { text, kind, until: self.time + if kind == ToastKind::Error { 7.0 } else { 4.0 }, action: None });
    }

    /// A notice with a button; it stays a little longer so the button can be reached.
    pub fn toast_action(&mut self, kind: ToastKind, text: impl Into<String>, action: ToastAction) {
        let text = text.into();
        log::info!("{text}");
        self.toasts.push(Toast { text, kind, until: self.time + 9.0, action: Some(action) });
    }

    pub fn open_paths(&mut self, paths: Vec<PathBuf>, open_first: bool) {
        if paths.is_empty() {
            return;
        }
        for p in &paths {
            if p.is_dir() || crate::dicom::source::is_zip(p) {
                self.settings.push_recent(p.clone());
            }
        }
        self.settings.save(&self.paths.config);
        self.start_scan(paths, open_first);
    }

    fn start_scan(&mut self, paths: Vec<PathBuf>, open_first: bool) {
        self.scans_active += 1;
        self.scan = Some(self.jobs.scan(paths, open_first));
    }

    pub fn open_folder_dialog(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Abrir pasta DICOM").pick_folder() {
            self.open_paths(vec![dir], true);
        }
    }

    pub fn open_files_dialog(&mut self) {
        if let Some(files) = rfd::FileDialog::new().set_title("Abrir arquivos DICOM").pick_files() {
            self.open_paths(files, true);
        }
    }

    /// Opens a series in the given target, loading it first when needed.
    pub fn open_series(&mut self, uid: &str, target: Target) {
        if let Some((s, used)) = self.loaded.get_mut(uid) {
            *used = self.time;
            let s = s.clone();
            self.assign(target, s);
            return;
        }
        self.pending.retain(|(t, _)| *t != target);
        self.pending.push((target, uid.to_string()));
        if !self.loading.contains_key(uid)
            && let Some((study, series)) = self.library.find_series(uid) {
                let progress = self.jobs.load_series(series.clone(), study.header.clone());
                self.loading.insert(uid.to_string(), progress);
            }
    }

    fn assign(&mut self, target: Target, series: Arc<LoadedSeries>) {
        match target {
            Target::View(i) => {
                if let Some(v) = self.views.get_mut(i) {
                    v.set_content(Content::Stack(series.clone()));
                }
                self.apply_pending_bookmark(&series);
            }
            Target::Mpr => {
                self.setup_mpr(series.clone());
                self.apply_pending_bookmark(&series);
            }
            Target::Compare(side) => {
                if let Some(v) = self.cmp_views.get_mut(side) {
                    v.set_content(Content::Stack(series));
                }
                self.workspace = Workspace::Compare;
                self.refresh_compare();
            }
            Target::Volume => {
                if series.volume.is_none() {
                    self.toast(ToastKind::Error, "Esta série não forma um volume 3D (geometria insuficiente).");
                    return;
                }
                self.vr_view.set_content(Content::Volume(series));
                self.workspace = Workspace::Volume;
            }
        }
    }

    fn setup_mpr(&mut self, series: Arc<LoadedSeries>) {
        let Some(vol) = series.volume.clone() else {
            self.toast(ToastKind::Error, "Esta série não forma um volume (MPR indisponível).");
            return;
        };
        let same = self.mpr_views[0].content.series().is_some_and(|s| Arc::ptr_eq(s, &series));
        let kinds = [PlaneKind::Axial, PlaneKind::Coronal, PlaneKind::Sagittal];
        for (v, k) in self.mpr_views.iter_mut().zip(kinds) {
            v.set_content(Content::Mpr(series.clone(), k));
            v.window = self.views.get(self.active).map(|a| a.window).unwrap_or(series.default_window);
        }
        self.mpr_views[3].set_content(Content::Volume(series.clone()));
        if !same {
            self.mpr.point = vol.center();
            self.mpr.reset_axes();
            self.curve.clear();
            self.curve_series = None;
        }
        // Entering MPR starts with the crosshair, which drives the three planes.
        let entering = self.workspace != Workspace::Mpr;
        self.workspace = Workspace::Mpr;
        if entering || self.tool == Tool::Scroll {
            self.tool = Tool::Crosshair;
        }
        self.finish_protocol();
    }

    /// Closes what viewport `i` of the current workspace shows (close button, Ctrl+W):
    /// a 2D window is emptied, MPR/3D return to 2D, a comparison side is removed.
    pub(crate) fn close_view(&mut self, i: usize) {
        match self.workspace {
            Workspace::Viewer => {
                if let Some(v) = self.views.get_mut(i) {
                    v.set_content(Content::Empty);
                }
                self.pending.retain(|(t, _)| *t != Target::View(i));
            }
            Workspace::Mpr | Workspace::Volume => self.close_reconstruction(),
            Workspace::Compare => self.close_compare_side(i.min(1)),
        }
        self.maximized = None;
        self.last_sync = None;
        self.evict_memory();
    }

    /// Empties MPR/3D and goes back to the 2D viewer.
    fn close_reconstruction(&mut self) {
        for v in self.mpr_views.iter_mut().chain(std::iter::once(&mut self.vr_view)) {
            v.set_content(Content::Empty);
        }
        self.curve.clear();
        self.curve_series = None;
        self.gpu = None;
        self.pending.retain(|(t, _)| !matches!(t, Target::Mpr | Target::Volume));
        if matches!(self.workspace, Workspace::Mpr | Workspace::Volume) {
            self.workspace = Workspace::Viewer;
            self.active = 0;
        }
        if matches!(self.tool, Tool::Crosshair | Tool::Curve) {
            self.tool = Tool::WindowLevel;
        }
    }

    /// Closes a whole exam: its windows, its memory and its entry in the list.
    /// Puts back in the list the exams closed last; the first one is opened again.
    pub(crate) fn reopen_closed(&mut self) {
        let Some((_, batch)) = self.closed.pop() else { return };
        let n = batch.len();
        let first = batch.first().and_then(|s| s.series.first()).map(|s| s.uid.clone());
        for study in batch {
            self.library.restore_study(study);
        }
        if let Some(uid) = first
            && self.workspace == Workspace::Viewer && self.views.get(self.active).is_some_and(|v| matches!(v.content, Content::Empty)) {
                self.open_series(&uid, Target::View(self.active));
            }
        self.toasts.retain(|t| t.action != Some(ToastAction::ReopenClosed));
        self.toast(ToastKind::Success, if n == 1 { "Exame reaberto.".to_string() } else { format!("{n} exames reabertos.") });
    }

    pub(crate) fn close_study(&mut self, uid: &str) {
        let series_uids: Vec<String> = self
            .library
            .studies
            .iter()
            .find(|s| s.header.uid == uid)
            .map(|s| s.series.iter().map(|x| x.uid.clone()).collect())
            .unwrap_or_default();
        let in_study = |v: &Viewport| v.content.series().is_some_and(|s| s.study.uid == uid);
        for v in self.views.iter_mut().filter(|v| in_study(v)) {
            v.set_content(Content::Empty);
        }
        if self.mpr_views.iter().chain(std::iter::once(&self.vr_view)).any(&in_study) {
            self.close_reconstruction();
        }
        for side in [1, 0] {
            if in_study(&self.cmp_views[side]) {
                self.close_compare_side(side);
            }
        }
        self.loaded.retain(|k, _| !series_uids.contains(k));
        self.loading.retain(|k, _| !series_uids.contains(k));
        self.pending.retain(|(_, u)| !series_uids.contains(u));
        for u in &series_uids {
            self.thumbs.remove(u);
            self.thumbs_requested.remove(u);
        }
        if self.cursor.as_ref().is_some_and(|c| c.1 == uid) {
            self.cursor = None;
        }
        if self.curve_series.as_ref().is_some_and(|s| s.study.uid == uid) {
            self.curve_clear();
        }
        if let Some(study) = self.library.take_study(uid) {
            // Kept so the close can be undone (exams closed together come back together).
            match self.closed.last_mut() {
                Some((t, batch)) if *t == self.time => batch.push(study),
                _ => self.closed.push((self.time, vec![study])),
            }
            if self.closed.len() > 5 {
                self.closed.remove(0);
            }
        }
        self.expanded.remove(uid);
        if self.expanded_study.as_deref() == Some(uid) {
            self.expanded_study = None;
        }
        self.maximized = None;
        self.last_sync = None;
    }

    /// Stops adding points to the tubular tracker path.
    pub(crate) fn finish_curve(&mut self) {
        self.tool = if self.workspace == Workspace::Mpr { Tool::Crosshair } else { Tool::WindowLevel };
    }

    /// Series shown by the active viewport of the current workspace.
    pub fn active_series(&self) -> Option<Arc<LoadedSeries>> {
        self.active_view().and_then(|v| v.content.series().cloned())
    }

    pub fn active_view(&self) -> Option<&Viewport> {
        match self.workspace {
            Workspace::Viewer => self.views.get(self.active),
            Workspace::Mpr => self.mpr_views.get(self.active.min(3)),
            Workspace::Volume => Some(&self.vr_view),
            Workspace::Compare => self.cmp_views.get(self.active.min(1)),
        }
    }

    pub fn active_view_mut(&mut self) -> Option<&mut Viewport> {
        match self.workspace {
            Workspace::Viewer => self.views.get_mut(self.active),
            Workspace::Mpr => self.mpr_views.get_mut(self.active.min(3)),
            Workspace::Volume => Some(&mut self.vr_view),
            Workspace::Compare => self.cmp_views.get_mut(self.active.min(1)),
        }
    }

    /// Viewport that keyboard navigation moves, with the MPR state it uses.
    pub(crate) fn nav_view(&mut self) -> Option<(&mut Viewport, &mut MprShared)> {
        match self.workspace {
            Workspace::Viewer => self.views.get_mut(self.active).map(|v| (v, &mut self.mpr)),
            Workspace::Mpr => self.mpr_views.get_mut(self.active.min(3)).map(|v| (v, &mut self.mpr)),
            Workspace::Compare => {
                // B follows A while synchronized.
                let side = if self.cmp.sync_position && self.tool != Tool::Landmark { 0 } else { self.active.min(1) };
                let (views, mprs) = (&mut self.cmp_views, &mut self.cmp.mpr);
                views.get_mut(side).map(|v| (v, &mut mprs[side]))
            }
            Workspace::Volume => None,
        }
    }

    pub fn switch_workspace(&mut self, ws: Workspace) {
        if ws == self.workspace {
            return;
        }
        let series = self.active_series().or_else(|| self.views.iter().find_map(|v| v.content.series().cloned()));
        match ws {
            Workspace::Viewer => {
                self.workspace = ws;
                self.active = 0;
            }
            Workspace::Compare => {
                self.workspace = ws;
                self.active = 0;
                self.start_compare();
            }
            Workspace::Mpr | Workspace::Volume => {
                let Some(s) = series else {
                    self.toast(ToastKind::Info, "Abra uma série primeiro.");
                    return;
                };
                if s.volume.is_none() {
                    self.toast(ToastKind::Error, "Esta série não forma um volume 3D (é preciso uma série de cortes com geometria).");
                    return;
                }
                let target = if ws == Workspace::Mpr { Target::Mpr } else { Target::Volume };
                self.assign(target, s);
                self.active = 0;
            }
        }
        self.maximized = None;
    }

    pub fn start_scp(&mut self) {
        if self.scp.is_some() {
            return;
        }
        let (tx, ctx) = self.jobs.sender();
        match StorageScp::start(&self.settings.local_ae, self.settings.local_port, self.paths.store.clone(), move |e| {
            let _ = tx.send(match e {
                ScpEvent::Stored(p) => Msg::ScpStored(p),
                ScpEvent::Error(s) => Msg::ScpError(s),
            });
            ctx.request_repaint();
        }) {
            Ok(scp) => {
                let text = format!("Receptor DICOM ativo: {} na porta {}", scp.ae_title, scp.port);
                self.scp = Some(scp);
                self.toast(ToastKind::Success, text);
            }
            Err(e) => self.toast(ToastKind::Error, format!("Não foi possível iniciar o receptor DICOM: {e:#}")),
        }
    }

    pub fn stop_scp(&mut self) {
        self.scp = None;
    }

    pub fn show_tags_for_active(&mut self) {
        let Some(view) = self.active_view() else { return };
        let Some(series) = view.content.series() else { return };
        let idx = view.slice.min(series.images.len().saturating_sub(1));
        let Some(info) = series.images.get(idx) else { return };
        let path = info.path.clone();
        match crate::dicom::tags::dump(&path) {
            Ok(rows) => self.tags = Some((path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), rows)),
            Err(e) => self.toast(ToastKind::Error, format!("Não foi possível ler as tags: {e:#}")),
        }
    }

    pub fn export_active(&mut self, ctx: &egui::Context) {
        self.export_view(ctx, false);
    }

    /// Captures the active view: to a PNG file, or to the clipboard (Ctrl+C).
    pub fn export_view(&mut self, ctx: &egui::Context, clipboard: bool) {
        self.export_clipboard = clipboard;
        let rect = match self.workspace {
            Workspace::Volume => self.view_rects.first().copied(),
            _ => self.view_rects.get(self.active).copied(),
        };
        if let Some(r) = rect {
            self.export_request = Some(r);
            let c = ctx.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                if let Ok(mut slot) = EXPORT_SLOT.lock() {
                    *slot = Some(img);
                }
                c.request_repaint();
            })));
        }
    }

    fn finish_export(&mut self, ctx: &egui::Context) {
        let Some(rect) = self.export_request else { return };
        let Some(img) = EXPORT_SLOT.lock().ok().and_then(|mut s| s.take()) else { return };
        self.export_request = None;
        let ppp = ctx.pixels_per_point();
        let (x0, y0) = ((rect.left() * ppp).round().max(0.0) as usize, (rect.top() * ppp).round().max(0.0) as usize);
        let (x1, y1) = (((rect.right() * ppp).round() as usize).min(img.size[0]), ((rect.bottom() * ppp).round() as usize).min(img.size[1]));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let mut buf = image::RgbaImage::new((x1 - x0) as u32, (y1 - y0) as u32);
        for y in y0..y1 {
            for x in x0..x1 {
                let c = img.pixels[y * img.size[0] + x];
                buf.put_pixel((x - x0) as u32, (y - y0) as u32, image::Rgba([c.r(), c.g(), c.b(), 255]));
            }
        }
        if self.export_clipboard {
            let size = [buf.width() as usize, buf.height() as usize];
            ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied(size, buf.as_raw()));
            self.toast(ToastKind::Success, "Imagem copiada: cole no laudo ou em outro programa (Ctrl+V).");
            return;
        }
        let default = format!("slyced-{}.png", chrono_like_stamp());
        if let Some(path) = rfd::FileDialog::new().set_title("Salvar imagem").set_file_name(&default).add_filter("PNG", &["png"]).save_file() {
            match buf.save(&path) {
                Ok(()) => self.toast(ToastKind::Success, format!("Imagem salva em {}", path.display())),
                Err(e) => self.toast(ToastKind::Error, format!("Falha ao salvar: {e}")),
            }
        }
    }

    // ------------------------------------------------------------------ messages

    fn drain_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.jobs.rx.try_recv() {
            match msg {
                Msg::Scanned { files, open, total } => {
                    self.scans_active = self.scans_active.saturating_sub(1);
                    let encrypted = self.scan.as_ref().map(|s| s.encrypted.swap(0, Ordering::Relaxed)).unwrap_or(0);
                    if encrypted > 0 {
                        self.toast(
                            ToastKind::Error,
                            format!("{encrypted} arquivo(s) dentro do ZIP estão protegidos por senha e foram ignorados."),
                        );
                    }
                    if self.scans_active == 0 {
                        self.scan = None;
                    }
                    let n_files = files.len();
                    let added = self.library.merge(files);
                    if n_files == 0 {
                        self.toast(ToastKind::Error, format!("Nenhuma imagem DICOM encontrada ({total} arquivo(s) verificado(s))."));
                        continue;
                    }
                    self.toast(ToastKind::Success, format!("{added} imagem(ns) adicionada(s) · {} estudo(s)", self.library.studies.len()));
                    self.request_thumbnails();
                    if open {
                        self.open_first_series();
                    } else if let Some(uid) = self.open_after_scan.take() {
                        // A PACS retrieval just finished: show what was downloaded.
                        match self.library.studies.iter().find(|st| st.header.uid == uid) {
                            Some(st) => {
                                let best = st.series.iter().max_by_key(|s| s.image_count()).map(|s| s.uid.clone());
                                if let Some(series) = best {
                                    self.expanded.insert(uid.clone());
                                    self.expanded_study = Some(uid);
                                    self.workspace = Workspace::Viewer;
                                    self.open_series(&series, Target::View(self.active));
                                }
                            }
                            None => self.open_after_scan = Some(uid),
                        }
                    }
                    self.compare_downloaded();
                }
                Msg::SeriesLoaded { uid, result } => {
                    self.loading.remove(&uid);
                    if self.library.find_series(&uid).is_none() {
                        continue; // its exam was closed while loading
                    }
                    match result {
                        Ok(series) => {
                            for w in &series.warnings {
                                self.toast(ToastKind::Info, w.clone());
                            }
                            self.loaded.insert(uid.clone(), (series.clone(), self.time));
                            let targets: Vec<Target> = self.pending.iter().filter(|(_, u)| *u == uid).map(|(t, _)| *t).collect();
                            self.pending.retain(|(_, u)| *u != uid);
                            for t in targets {
                                self.assign(t, series.clone());
                            }
                            self.evict_memory();
                        }
                        Err(e) => {
                            self.pending.retain(|(_, u)| *u != uid);
                            self.pending_protocol = None;
                            self.pending_bookmark = None;
                            self.toast(ToastKind::Error, e);
                        }
                    }
                }
                Msg::Thumb { uid, image } => {
                    let tex = ctx.load_texture(format!("thumb-{uid}"), image, egui::TextureOptions::LINEAR);
                    self.thumbs.insert(uid, tex);
                }
                Msg::GpuReady(data) => {
                    if self.gpu_pending == Some(data.source_id) {
                        self.gpu_pending = None;
                    }
                    self.gpu = Some(data);
                }
                Msg::ScpStored(p) => {
                    if self.incoming.is_empty() {
                        self.incoming_since = self.time;
                    }
                    self.incoming.push(p);
                }
                Msg::ScpError(e) => log::warn!("SCP: {e}"),
                Msg::Registered { version, result } => self.on_registered(version, result),
                Msg::Calculi { series, threshold, items } => self.on_calculi(series, threshold, items),
                Msg::StoreCleaned { removed, left, days } => {
                    self.store_usage = Some(left);
                    if removed.studies > 0 {
                        let text = match days {
                            Some(days) => format!("Limpeza automática: {} baixado(s) há mais de {days} dias foram apagados.", removed.describe()),
                            None => format!("{} baixado(s) do PACS foram apagados.", removed.describe()),
                        };
                        self.toast(ToastKind::Info, text);
                    }
                }
                other => self.handle_pacs_msg(other),
            }
        }
        // Files pushed to the Storage SCP are indexed in batches.
        if !self.incoming.is_empty() && self.time - self.incoming_since > 1.0 {
            let files = std::mem::take(&mut self.incoming);
            self.start_scan(files, false);
        }
        if !self.incoming.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    fn handle_pacs_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Echo { node, result } => {
                match &result {
                    Ok(d) => self.toast(ToastKind::Success, format!("Conexão com {node}: OK ({} ms)", d.as_millis())),
                    Err(e) => self.toast(ToastKind::Error, format!("Teste de conexão com {node} falhou: {e}")),
                }
                self.pacs.node_status.insert(node, result.map(|d| d.as_millis()));
            }
            Msg::Studies(r) => {
                self.pacs.searching = false;
                let elapsed = self.time - self.pacs.search_started;
                match r {
                    Ok(list) => {
                        self.pacs.error = None;
                        self.pacs.last_search = Some((list.len(), elapsed));
                        self.pacs.results = list;
                        self.pacs.sort_results();
                        self.pacs.selected = None;
                    }
                    Err(e) => {
                        self.pacs.error = Some(e.clone());
                        self.pacs.last_search = None;
                        self.toast(ToastKind::Error, e);
                    }
                }
            }
            Msg::Priors { patient_id, result } => self.on_priors(patient_id, result),
            Msg::Series { study_uid, result } => match result {
                Ok(mut list) => {
                    list.sort_by_key(|s| s.number.unwrap_or(i32::MAX));
                    self.pacs.series.insert(study_uid, list);
                }
                Err(e) => {
                    self.pacs.series.remove(&study_uid);
                    self.toast(ToastKind::Error, e);
                }
            },
            Msg::Retrieve { id, progress } => {
                if let Some(t) = self.pacs.transfers.iter_mut().find(|t| t.id == id) {
                    t.progress = progress;
                }
            }
            Msg::RetrieveDone { id, files, result } => {
                // The downloads take more space now (shown in the settings).
                self.jobs.measure_store(self.paths.store.clone());
                let mut open_study = None;
                if let Some(t) = self.pacs.transfers.iter_mut().find(|t| t.id == id) {
                    t.done = true;
                    match &result {
                        Ok(p) => {
                            t.progress = *p;
                            t.error = None;
                            open_study = t.open_when_done.then(|| t.study_uid.clone());
                        }
                        Err(e) => {
                            t.error = Some(e.clone());
                            // Nothing to compare when the download failed.
                            self.compare_after_retrieve.remove(&t.study_uid);
                        }
                    }
                }
                match result {
                    Ok(p) => {
                        let n = files.len().max(p.completed as usize);
                        self.toast(ToastKind::Success, format!("Transferência concluída: {n} imagem(ns)."));
                        self.open_after_scan = open_study;
                        if !files.is_empty() {
                            self.start_scan(files, false);
                        } else if !self.incoming.is_empty() {
                            let files = std::mem::take(&mut self.incoming);
                            self.start_scan(files, false);
                        }
                    }
                    Err(e) => self.toast(ToastKind::Error, e),
                }
            }
            _ => {}
        }
    }

    fn open_first_series(&mut self) {
        let first = self.library.studies.first().and_then(|st| {
            let series = st.series.iter().max_by_key(|s| s.image_count())?;
            Some((st.header.uid.clone(), series.uid.clone()))
        });
        if let Some((study, series)) = first {
            self.expanded.insert(study.clone());
            self.expanded_study = Some(study);
            self.workspace = Workspace::Viewer;
            self.open_series(&series, Target::View(self.active));
            if self.layout.count() > 1 {
                self.fill_empty_views();
            }
        }
    }

    fn request_thumbnails(&mut self) {
        let mut items = Vec::new();
        for st in &self.library.studies {
            for s in &st.series {
                if !self.thumbs_requested.insert(s.uid.clone()) {
                    continue;
                }
                let imgs: Vec<_> = s.instances.iter().filter(|i| i.has_pixels).collect();
                if let Some(mid) = imgs.get(imgs.len() / 2) {
                    let frame = if imgs.len() == 1 { mid.frames / 2 } else { 0 };
                    items.push((s.uid.clone(), mid.path.clone(), frame));
                }
            }
        }
        self.jobs.thumbnails(items);
    }

    fn evict_memory(&mut self) {
        let budget = crate::platform::memory_budget_mb(self.settings.memory_budget_mb) * 1024 * 1024;
        let in_use: Vec<*const LoadedSeries> = self
            .views
            .iter()
            .chain(self.mpr_views.iter())
            .chain(self.cmp_views.iter())
            .chain(std::iter::once(&self.vr_view))
            .filter_map(|v| v.content.series().map(Arc::as_ptr))
            .collect();
        loop {
            let total: usize = self.loaded.values().map(|(s, _)| s.memory_bytes()).sum();
            if total <= budget {
                break;
            }
            let victim = self
                .loaded
                .iter()
                .filter(|(_, (s, _))| !in_use.contains(&Arc::as_ptr(s)))
                .min_by(|a, b| a.1 .1.total_cmp(&b.1 .1))
                .map(|(k, _)| k.clone());
            match victim {
                Some(k) => {
                    self.loaded.remove(&k);
                }
                None => break,
            }
        }
    }

    pub fn memory_used(&self) -> usize {
        self.loaded.values().map(|(s, _)| s.memory_bytes()).sum()
    }

    // ------------------------------------------------------------------ input

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.tab_pressed) && self.tour.is_none() {
            // In the reading full screen there are no panels: Tab shows the exams list.
            if self.reading {
                let pinned = self.reading_force.as_deref() == Some("drawer");
                self.pin_drawer(!pinned);
            } else {
                self.toggle_focus();
            }
        }
        if ctx.egui_wants_keyboard_input() || self.tour.is_some() {
            return;
        }
        // Mouse side buttons: previous / next series.
        let (back, forward) = ctx.input(|i| (i.pointer.button_pressed(egui::PointerButton::Extra1), i.pointer.button_pressed(egui::PointerButton::Extra2)));
        if back {
            self.step_series(-1);
        }
        if forward {
            self.step_series(1);
        }
        use egui::Key;
        let (pressed, mods): (Vec<Key>, egui::Modifiers) = ctx.input(|i| {
            let keys = [
                Key::ArrowUp, Key::ArrowDown, Key::PageUp, Key::PageDown, Key::Home, Key::End, Key::Space, Key::R, Key::I,
                Key::W, Key::P, Key::Z, Key::S, Key::C, Key::D, Key::A, Key::E, Key::H, Key::O, Key::Delete, Key::F1,
                Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9,
                Key::Escape, Key::Plus, Key::Equals, Key::Minus, Key::L, Key::T, Key::B, Key::M, Key::V, Key::G,
                Key::Enter, Key::Backspace, Key::Y, Key::ArrowLeft, Key::ArrowRight, Key::F11,
            ];
            (keys.into_iter().filter(|k| i.key_pressed(*k)).collect(), i.modifiers)
        });
        for key in pressed {
            match key {
                Key::ArrowLeft if mods.alt => self.nav_step(true),
                Key::ArrowRight if mods.alt => self.nav_step(false),
                Key::ArrowLeft => self.step_series(-1),
                Key::ArrowRight => self.step_series(1),
                Key::F11 => self.toggle_reading(ctx),
                Key::G if mods.ctrl => self.open_goto(),
                Key::I if mods.ctrl => self.cycle_info(),
                Key::C if mods.ctrl => self.export_view(ctx, true),
                Key::Z if mods.ctrl && mods.shift => self.redo(),
                Key::Z if mods.ctrl => self.undo(),
                Key::Y if mods.ctrl => self.redo(),
                Key::Y => {}
                Key::O if mods.ctrl && mods.shift => self.open_files_dialog(),
                Key::O if mods.ctrl => self.open_folder_dialog(),
                Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown => {
                    let d = match key {
                        Key::ArrowUp => -1,
                        Key::ArrowDown => 1,
                        Key::PageUp => -10,
                        _ => 10,
                    };
                    if let Some((v, mpr)) = self.nav_view() {
                        v.step(d, Some(mpr));
                    }
                }
                Key::Home | Key::End => {
                    if let Some((v, mpr)) = self.nav_view() {
                        v.jump_to(key == Key::Home, Some(mpr));
                    }
                }
                Key::Space => {
                    if let Some(v) = self.active_view_mut() {
                        v.cine = !v.cine;
                    }
                }
                Key::R if mods.shift => self.reset_screen(),
                Key::R => {
                    if let Some(v) = self.active_view_mut() {
                        v.reset_view();
                    }
                }
                Key::I => {
                    if let Some(v) = self.active_view_mut() {
                        v.invert = !v.invert;
                    }
                }
                Key::Delete if mods.shift => self.clear_measurements(),
                Key::Delete => {
                    if let Some(v) = self.active_view_mut() {
                        v.measurements.pop();
                    }
                }
                Key::Escape => {
                    if self.goto.is_some() {
                        self.goto = None;
                    } else if self.tool == Tool::Curve {
                        self.finish_curve();
                    } else if self.reading_drawer || self.reading_force.as_deref() == Some("drawer") {
                        self.pin_drawer(false);
                    } else if self.maximized.is_some() {
                        self.maximized = None;
                    } else if self.reading {
                        self.leave_reading(ctx);
                    } else if ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                    } else {
                        self.cursor = None;
                        self.cmp.cursor = None;
                    }
                }
                Key::Enter if self.tool == Tool::Curve => self.finish_curve(),
                Key::Backspace if self.tool == Tool::Curve => self.curve_undo(),
                Key::Enter | Key::Backspace => {}
                Key::Plus | Key::Equals | Key::Minus => {
                    let factor = if key == Key::Minus { 1.0 / 1.25 } else { 1.25 };
                    if let Some(v) = self.active_view_mut() {
                        v.zoom_by(factor);
                    }
                }
                Key::F1 => self.show_help = !self.show_help,
                Key::W if mods.ctrl => self.close_view(self.active),
                Key::W => self.tool = Tool::WindowLevel,
                Key::P => self.tool = Tool::Pan,
                Key::Z => self.tool = Tool::Zoom,
                Key::S => self.tool = Tool::Scroll,
                Key::C => self.tool = Tool::Crosshair,
                Key::D => self.tool = Tool::Length,
                Key::A => self.tool = Tool::Angle,
                Key::E => self.tool = Tool::Ellipse,
                Key::H => self.tool = Tool::Probe,
                Key::L => self.tool = Tool::Locate,
                Key::T => self.tool = Tool::Arrow,
                Key::B => self.tool = Tool::Bookmark,
                Key::M => self.tool = Tool::Magnifier,
                Key::V if matches!(self.workspace, Workspace::Mpr | Workspace::Viewer) => self.tool = Tool::Curve,
                Key::G if self.workspace == Workspace::Compare => self.tool = Tool::Landmark,
                Key::V | Key::G => {}
                k => {
                    let n = match k {
                        Key::Num1 => 0,
                        Key::Num2 => 1,
                        Key::Num3 => 2,
                        Key::Num4 => 3,
                        Key::Num5 => 4,
                        Key::Num6 => 5,
                        Key::Num7 => 6,
                        Key::Num8 => 7,
                        Key::Num9 => 8,
                        Key::Num0 => 9,
                        _ => continue,
                    };
                    self.apply_window_preset(n);
                }
            }
        }
    }

    pub fn apply_window_preset(&mut self, n: usize) {
        let Some(p) = crate::imaging::lut::CT_PRESETS.get(n) else { return };
        let ws = self.workspace;
        let apply = |v: &mut Viewport| {
            if let Some(s) = v.content.series() {
                v.window = p.window.unwrap_or(s.default_window);
            }
        };
        match ws {
            Workspace::Mpr => self.mpr_views.iter_mut().take(3).for_each(apply),
            _ => {
                if let Some(v) = self.active_view_mut() {
                    apply(v);
                }
            }
        }
    }

    // ------------------------------------------------------------------ layout

    fn viewports_ui(&mut self, ui: &mut egui::Ui) {
        if self.workspace == Workspace::Compare {
            self.compare_views_ui(ui);
            return;
        }
        let full = ui.available_rect_before_wrap();
        let gap = if self.reading { 1.0 } else { 2.0 };
        if self.reading {
            ui.painter().rect_filled(full, egui::CornerRadius::ZERO, egui::Color32::from_gray(16));
        }
        let chrome = self.reading && self.reading_chrome(ui.ctx()) > 0.0;
        let (cols, rows, count) = match self.workspace {
            Workspace::Viewer => {
                let (c, r) = self.layout.dims();
                (c, r, self.layout.count())
            }
            Workspace::Mpr => (2, 2, 4),
            Workspace::Volume | Workspace::Compare => (1, 1, 1),
        };
        let mut rects = Vec::with_capacity(count);
        for i in 0..count {
            let (c, r) = (i % cols, i / cols);
            let w = (full.width() - gap * (cols - 1) as f32) / cols as f32;
            let h = (full.height() - gap * (rows - 1) as f32) / rows as f32;
            let min = full.min + Vec2::new(c as f32 * (w + gap), r as f32 * (h + gap));
            rects.push(Rect::from_min_size(min, Vec2::new(w, h)));
        }
        if let Some(m) = self.maximized.filter(|m| *m < count) {
            for (i, r) in rects.iter_mut().enumerate() {
                *r = if i == m { full } else { Rect::NOTHING };
            }
        }
        self.view_rects = rects.clone();
        ui.allocate_rect(full, egui::Sense::hover());

        let time = self.time;
        let mut status = None;
        // Plane of the active 2D image, drawn as a reference line on the other viewports.
        let reference = match self.workspace {
            Workspace::Viewer => self.views.get(self.active).and_then(|v| match &v.content {
                Content::Stack(s) => {
                    let im = s.images.get(v.slice)?;
                    let (p, (r, c)) = (im.position?, im.orientation?);
                    Some(crate::ui::viewport::RefPlane {
                        study_uid: s.study.uid.clone(),
                        series: Arc::as_ptr(s) as usize,
                        point: p,
                        normal: r.cross(c).normalize(),
                    })
                }
                _ => None,
            }),
            _ => None,
        };
        let gpu_issue = self.gpu_3d_issue.clone().or_else(|| {
            crate::render3d::renderer::GPU_FAULT
                .load(Ordering::Relaxed)
                .then(|| "A placa de vídeo relatou um erro; a reconstrução 3D foi desativada. Detalhes no log.".to_string())
        });
        // Labels of what the tools changed go on the active 2D view (never into exported images).
        let changes = if self.export_request.is_none() && !self.reading { self.screen_changes() } else { Vec::new() };
        let several = rects.iter().filter(|r| r.width() >= 2.0).count() > 1;
        let chip_view = (0..rects.len())
            .filter(|&i| rects[i].width() >= 2.0 && self.workspace_view(i).is_some_and(|v| matches!(v.content, Content::Stack(_) | Content::Mpr(..) | Content::Cpr(..))))
            .min_by_key(|&i| i != self.active);
        for (i, rect) in rects.into_iter().enumerate() {
            if rect.width() < 2.0 {
                continue;
            }
            let active = i == self.active || self.workspace == Workspace::Volume;
            let (loading_uid, is_target) = self
                .pending
                .iter()
                .find(|(t, _)| match (self.workspace, t) {
                    (Workspace::Viewer, Target::View(j)) => *j == i,
                    (Workspace::Mpr, Target::Mpr) => true,
                    (Workspace::Volume, Target::Volume) => true,
                    _ => false,
                })
                .map(|(_, u)| (Some(u.clone()), true))
                .unwrap_or((None, false));
            let loading = loading_uid.filter(|_| is_target).and_then(|u| {
                let p = self.loading.get(&u)?;
                let title = self.library.find_series(&u).map(|(_, s)| s.title()).unwrap_or_default();
                Some((format!("Carregando {title}"), p.done.load(Ordering::Relaxed) as f32 / p.total.max(1) as f32))
            });
            let gpu_series = match self.workspace {
                Workspace::Mpr if i == 3 => true,
                Workspace::Volume => true,
                _ => false,
            };
            let series = self.workspace_view(i).and_then(|v| v.content.series().cloned());
            let mut cx = ViewCtx::new(self.tool, time);
            cx.reference = if active { None } else { reference.clone() };
            cx.smooth = self.settings.smooth;
            cx.overlays = self.settings.overlays;
            cx.active = active;
            cx.gpu = if gpu_series { self.gpu.clone() } else { None };
            cx.gpu_pending = self.gpu_pending.is_some();
            cx.gpu_issue = gpu_issue.clone();
            cx.loading = loading;
            cx.quality = self.settings.vr_quality;
            cx.lighting = self.settings.vr_lighting;
            cx.magnifier = self.magnifier;
            cx.closable = !self.reading;
            cx.localizer = self.settings.localizer && (!self.reading || chrome);
            cx.reading = self.reading;
            cx.info = self.settings.reading_info;
            cx.chrome = chrome;
            cx.several = several;
            cx.open_context = i == self.active && self.open_menu.as_deref() == Some("context");
            if chip_view == Some(i) {
                cx.changes = changes.clone();
            }
            if let Some(s) = &series {
                cx.pins = self.bookmark_pins(s);
                cx.pins.extend(self.calculi_pins(s));
                cx.cursor = self.cursor_for(s);
            }
            let mpr_ws = self.workspace == Workspace::Mpr;
            let tracing = self.tool == Tool::Curve || !self.curve.is_empty();
            let plane_view = (mpr_ws && i < 3) || (self.workspace == Workspace::Viewer && matches!(self.views[i].content, Content::Stack(_)));
            if tracing && plane_view {
                cx.curve = Some(&self.curve);
            }
            cx.mpr = if mpr_ws { Some(&mut self.mpr) } else { None };
            let view = match self.workspace {
                Workspace::Mpr => &mut self.mpr_views[i],
                Workspace::Volume | Workspace::Compare => &mut self.vr_view,
                Workspace::Viewer => &mut self.views[i],
            };
            let window_before = view.window;
            let ev = view.show(ui, rect, &mut cx);
            // The three MPR planes share one window/level, as in most workstations.
            if self.workspace == Workspace::Mpr && i < 3 {
                let w = self.mpr_views[i].window;
                if w != window_before {
                    self.mpr_views.iter_mut().take(3).for_each(|v| v.window = w);
                }
            }
            if ev.activated {
                self.active = i;
            }
            if ev.double_clicked && !self.tool.places_points() {
                self.maximized = if self.maximized.is_some() { None } else { Some(i) };
            }
            if let Some(uid) = ev.dropped_series.clone() {
                self.active = i;
                let target = match self.workspace {
                    Workspace::Viewer => Target::View(i),
                    Workspace::Mpr => Target::Mpr,
                    Workspace::Volume | Workspace::Compare => Target::Volume,
                };
                self.open_series(&uid, target);
            }
            if let Some(series) = ev.need_gpu.clone()
                && let Some(vol) = series.volume.clone()
                    && self.gpu_pending != Some(vol.id) && self.gpu.as_ref().is_none_or(|g| g.source_id != vol.id) {
                        self.gpu_pending = Some(vol.id);
                        self.jobs.prepare_gpu(vol, self.gpu_limits.0, self.gpu_limits.1);
                    }
            if ev.status.is_some() {
                status = ev.status.clone();
            }
            if let Some(s) = series {
                self.view_event(i, &s, &ev);
            }
            if let Some(c) = ev.undo {
                self.undo_change(c);
            }
            if ev.reset_all {
                self.reset_screen();
            }
            if let Some(from) = ev.jump_from {
                self.push_nav_at(i, Some(from));
            }
            if let Some(a) = ev.context {
                let ctx = ui.ctx().clone();
                self.context_action(&ctx, i, a);
            }
            if ev.close {
                self.close_view(i);
                break;
            }
        }
        self.status = status.unwrap_or_default();
        if self.open_menu.as_deref() == Some("context") {
            self.open_menu = None;
        }

        if self.workspace == Workspace::Viewer && self.sync_scroll && self.layout.count() > 1 {
            let key = self.views.get(self.active).and_then(|v| match &v.content {
                Content::Stack(s) => Some((self.active, Arc::as_ptr(s) as usize, v.slice)),
                _ => None,
            });
            if key.is_some() && key != self.last_sync {
                self.last_sync = key;
                self.sync_from_active();
            }
        }
    }

    fn workspace_view(&self, i: usize) -> Option<&Viewport> {
        match self.workspace {
            Workspace::Viewer => self.views.get(i),
            Workspace::Mpr => self.mpr_views.get(i),
            Workspace::Volume => Some(&self.vr_view),
            Workspace::Compare => self.cmp_views.get(i),
        }
    }

    /// Cursor 3D, findings and CPR points placed on viewport `i`.
    fn view_event(&mut self, i: usize, series: &Arc<LoadedSeries>, ev: &ViewEvents) {
        if let Some(p) = ev.cursor.and_then(|h| h.patient) {
            self.set_cursor(p, series, i);
        }
        if let Some(hit) = ev.bookmark {
            let mpr = self.workspace == Workspace::Mpr;
            let (window, zoom) = self.workspace_view(i).map(|v| (v.window, v.zoom_factor())).unwrap_or((series.default_window, None));
            self.add_bookmark(series, hit, window, zoom, mpr);
        }
        if let Some(hit) = ev.curve_point.filter(|_| (self.workspace == Workspace::Mpr && i < 3) || self.workspace == Workspace::Viewer) {
            self.curve_add(hit, series);
        }
        if ev.curve_finish {
            self.finish_curve();
        }
        if ev.cpr_rotate != 0.0 {
            self.rotate_cpr(ev.cpr_rotate);
        }
    }

    /// Sends every view of the same patient space to `p` (Ctrl+click, Cursor 3D tool).
    fn set_cursor(&mut self, p: DVec3, series: &LoadedSeries, from: usize) {
        let (study, fr) = (series.study.uid.clone(), series.info.frame_of_reference.clone());
        self.cursor = Some((p, study, fr));
        let count = self.layout.count();
        let skip = (self.workspace == Workspace::Viewer).then_some(from);
        for j in 0..count {
            if Some(j) == skip {
                continue;
            }
            let same = self.views[j].content.series().is_some_and(|s| self.cursor_for(s).is_some());
            if same {
                self.views[j].go_to_point(p);
            }
        }
        if self.mpr_views[0].content.series().is_some_and(|s| self.cursor_for(s).is_some()) {
            self.mpr.point = p;
        }
    }

    /// The 3D cursor, if it belongs to `s`' patient space.
    pub(crate) fn cursor_for(&self, s: &LoadedSeries) -> Option<DVec3> {
        let (p, study, fr) = self.cursor.as_ref()?;
        let fs = &s.info.frame_of_reference;
        (s.study.uid == *study && (fr.is_empty() || fs.is_empty() || fs == fr)).then_some(*p)
    }

    /// Fills empty viewports of the current layout with the next series of the active study.
    pub fn fill_empty_views(&mut self) {
        let Some(study_uid) = self.active_series().map(|s| s.study.uid.clone()).or_else(|| self.expanded_study.clone()) else {
            return;
        };
        let Some(study) = self.library.studies.iter().find(|s| s.header.uid == study_uid) else { return };
        let shown: Vec<String> = self
            .views
            .iter()
            .filter_map(|v| v.content.series().map(|s| s.uid.clone()))
            .chain(self.pending.iter().map(|(_, u)| u.clone()))
            .collect();
        let mut candidates = study.series.iter().map(|s| s.uid.clone()).filter(|u| !shown.contains(u));
        let empty: Vec<usize> = (0..self.layout.count())
            .filter(|&i| matches!(self.views[i].content, Content::Empty) && !self.pending.iter().any(|(t, _)| *t == Target::View(i)))
            .collect();
        let plan: Vec<(usize, String)> = empty.into_iter().filter_map(|i| candidates.next().map(|u| (i, u))).collect();
        for (i, uid) in plan {
            self.open_series(&uid, Target::View(i));
        }
    }

    /// Moves the other viewports showing the same study to the image closest
    /// to the active image's center (works across orientations too).
    fn sync_from_active(&mut self) {
        let Some(src) = self.views.get(self.active) else { return };
        let Content::Stack(s) = &src.content else { return };
        let Some(info) = s.images.get(src.slice) else { return };
        let (Some(pos), Some((r, c))) = (info.position, info.orientation) else { return };
        let center = pos
            + r * (info.width as f64 * info.spacing.0 * 0.5)
            + c * (info.height as f64 * info.spacing.1 * 0.5);
        let (study, src_ptr) = (s.study.uid.clone(), Arc::as_ptr(s));
        let count = self.layout.count();
        let active = self.active;
        for (i, v) in self.views.iter_mut().enumerate().take(count) {
            let Content::Stack(o) = &v.content else { continue };
            if i == active || Arc::as_ptr(o) == src_ptr || o.study.uid != study {
                continue;
            }
            let best = o
                .images
                .iter()
                .enumerate()
                .filter_map(|(k, im)| {
                    let (p, (r, c)) = (im.position?, im.orientation?);
                    Some((k, (center - p).dot(r.cross(c)).abs(), im.thickness.unwrap_or(1.0)))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((k, dist, thick)) = best
                && dist <= thick.max(1.0) * 1.5 + 1.0 {
                    v.slice = k;
                }
        }
    }

    fn run_automation(&mut self, ctx: &egui::Context) {
        let Some(path) = self.automation.screenshot.clone() else { return };
        if let Some(ws) = self.automation.workspace
            && self.workspace != ws && self.active_series().is_some() {
                self.switch_workspace(ws);
            }
        let busy = self.scan.is_some()
            || !self.loading.is_empty()
            || !self.pending.is_empty()
            || self.gpu_pending.is_some()
            || self.cmp.registering.is_some()
            || self.calculi_busy
            || self.pacs.searching
            || self.priors.values().any(|p| matches!(p, crate::ui::timeline::Priors::Pending))
            || !self.compare_after_retrieve.is_empty()
            || match self.workspace {
                Workspace::Volume => self.vr_view.refining(),
                Workspace::Mpr => self.mpr_views.get(3).is_some_and(|v| v.refining()),
                _ => false,
            };
        if busy {
            self.automation.settle_since = self.time;
            return;
        }
        if self.time - self.automation.settle_since > 1.0 {
            if let Some(f) = self.automation.zoom.take() {
                let views = self.views.iter_mut().chain(self.mpr_views.iter_mut()).chain(std::iter::once(&mut self.vr_view));
                views.for_each(|v| v.zoom_by(f));
                self.automation.settle_since = self.time;
            } else if !self.automation.actions.is_empty() {
                let action = self.automation.actions.remove(0);
                self.run_action(&action);
                self.automation.settle_since = self.time;
            }
        }
        if self.automation.requested || !self.automation.actions.is_empty() || self.time - self.automation.settle_since < 2.0 {
            ctx.request_repaint();
            return;
        }
        self.automation.requested = true;
        let c = ctx.clone();
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
            let mut buf = image::RgbaImage::new(img.size[0] as u32, img.size[1] as u32);
            for (i, p) in img.pixels.iter().enumerate() {
                buf.put_pixel((i % img.size[0]) as u32, (i / img.size[0]) as u32, image::Rgba([p.r(), p.g(), p.b(), 255]));
            }
            if let Err(e) = buf.save(&path) {
                log::error!("screenshot: {e}");
            }
            c.send_viewport_cmd(egui::ViewportCommand::Close);
        })));
    }
}

impl SlycedApp {
    /// One scripted step (`--do name:arg`), for documentation screenshots and tests.
    fn run_action(&mut self, action: &str) {
        use crate::ui::compare::CompareDisplay;
        let (name, arg) = action.split_once(':').unwrap_or((action, ""));
        let nums: Vec<f64> = arg.split(',').filter_map(|v| v.trim().parse().ok()).collect();
        let num = nums.first().copied();
        log::info!("automação: {action}");
        match name {
            "protocol" => self.apply_protocol(num.unwrap_or(0.0) as usize),
            "display" => self.set_compare_display(match arg {
                "blend" => CompareDisplay::Blend,
                "swipe" => CompareDisplay::Swipe,
                "checker" => CompareDisplay::Checker,
                "enhancement" => CompareDisplay::Enhancement,
                "subtraction" => CompareDisplay::Subtraction,
                _ => CompareDisplay::SideBySide,
            }),
            "register" => self.start_auto_registration(),
            "plane" => {
                self.cmp.plane = match arg {
                    "coronal" => PlaneKind::Coronal,
                    "sagittal" => PlaneKind::Sagittal,
                    _ => PlaneKind::Axial,
                };
                self.refresh_compare();
            }
            "oblique" => {
                let (u, v) = self.mpr.axes[0];
                let q = glam::DQuat::from_axis_angle(u.cross(v).normalize(), num.unwrap_or(20.0).to_radians());
                for i in 1..3 {
                    let (a, b) = self.mpr.axes[i];
                    self.mpr.axes[i] = (q * a, q * b);
                }
            }
            "cpr" => {
                // Demo path: right carotid, up into the vessel ring at the skull base.
                let ring = |deg: f64| DVec3::new(21.0 * deg.to_radians().cos(), 4.0 + 21.0 * deg.to_radians().sin(), -2.0);
                self.curve = vec![
                    DVec3::new(21.0, 6.0, -72.0),
                    DVec3::new(21.0, 6.0, -40.0),
                    DVec3::new(21.0, 6.0, -8.0),
                    ring(40.0),
                    ring(90.0),
                    ring(140.0),
                    DVec3::new(-21.0, 6.0, -8.0),
                    DVec3::new(-21.0, 6.0, -40.0),
                ];
                self.curve_normal = DVec3::Y;
                self.rebuild_cpr();
            }
            "denoise" => {
                if let Some(v) = self.active_view_mut() {
                    v.denoise = num.unwrap_or(2.0) as u8;
                }
            }
            "slab" => {
                if let Some(v) = self.active_view_mut() {
                    v.slab_mm = num.unwrap_or(10.0) as f32;
                    if let Some(mode) = arg.split(',').nth(1) {
                        v.slab_mode = if mode == "avg" { SlabMode::Avg } else { SlabMode::Mip };
                    }
                }
            }
            "volume" => self.open_ellipsoid(),
            "length" if nums.len() >= 6 => {
                let (a, b) = (DVec3::new(nums[0], nums[1], nums[2]), DVec3::new(nums[3], nums[4], nums[5]));
                if let Some(v) = self.active_view_mut() {
                    v.add_length_at(a, b);
                }
            }
            "cursor" | "bookmark" | "ellipse" | "arrow" | "probe" => {
                let p = DVec3::new(*nums.first().unwrap_or(&0.0), *nums.get(1).unwrap_or(&0.0), *nums.get(2).unwrap_or(&0.0));
                let Some(series) = self.active_series() else { return };
                match name {
                    "cursor" if self.workspace == Workspace::Compare => self.cmp.cursor = Some(p).inspect(|p| {
                        self.cmp.mpr[0].point = *p;
                        self.cmp.mpr[1].point = self.cmp.reg.apply(*p);
                    }),
                    "cursor" => {
                        self.set_cursor(p, &series, self.active);
                        if let Some(v) = self.active_view_mut() {
                            v.go_to_point(p);
                        }
                        self.mpr.point = p;
                    }
                    "probe" => {
                        if let Some(v) = self.active_view_mut() {
                            v.add_probe_at(p);
                        }
                    }
                    "arrow" => {
                        if let Some(v) = self.active_view_mut() {
                            v.add_arrow_at(p, "Lesão hiperdensa");
                        }
                    }
                    "bookmark" => {
                        let hit = crate::ui::viewport::Hit { patient: Some(p), pixel: egui::Pos2::ZERO, mm: (0.0, 0.0), slice: 0, normal: None };
                        let mpr = self.workspace == Workspace::Mpr;
                        let window = self.active_view().map(|v| v.window).unwrap_or(series.default_window);
                        self.add_bookmark(&series, hit, window, None, mpr);
                        self.bookmark_edit = None;
                    }
                    _ => {
                        let r = *nums.get(3).unwrap_or(&8.0);
                        if self.workspace == Workspace::Compare {
                            self.cmp_views.iter_mut().for_each(|v| v.add_ellipse_at(p, r));
                        } else if let Some(v) = self.active_view_mut() {
                            v.add_ellipse_at(p, r);
                        }
                    }
                }
            }
            "tool" => {
                self.tool = match arg {
                    "lupa" => Tool::Magnifier,
                    "seta" => Tool::Arrow,
                    "curva" => Tool::Curve,
                    "pontos" => Tool::Landmark,
                    "cursor" => Tool::Locate,
                    "densidade" => Tool::Probe,
                    _ => Tool::WindowLevel,
                }
            }
            "pacs_select" => {
                if let Some(r) = self.pacs.results.get(num.unwrap_or(0.0) as usize).cloned() {
                    self.pacs.selected = Some(r.study_uid.clone());
                    if let Some(node) = self.settings.nodes.get(self.pacs.node).cloned() {
                        self.jobs.find_series(node, self.settings.local_ae.clone(), r.study_uid);
                    }
                }
            }
            "pacs_get" => {
                if let Some(r) = self.pacs.results.get(num.unwrap_or(0.0) as usize).cloned() {
                    self.start_retrieve(&r, None);
                }
            }
            "close" => self.close_view(num.unwrap_or(0.0) as usize),
            "special" => {
                use crate::ui::review::Special;
                let s = match arg { "gas" => Special::Gas, "fat" => Special::Fat, _ => Special::Dense };
                self.toggle_special(s);
            }
            "calculi" => self.search_calculi(),
            "reset" => match arg {
                "view" => {
                    if let Some(v) = self.active_view_mut() {
                        v.reset_view();
                    }
                }
                "measures" => self.clear_measurements(),
                "enhancements" => self.clear_enhancements(),
                "markers" => self.clear_markers(),
                _ => self.reset_screen(),
            },
            "undo" => {
                use crate::ui::viewport::Change;
                let c = match arg {
                    "measures" => Change::Measures(0),
                    "gas" => Change::Gas,
                    "fat" => Change::Fat,
                    "dense" => Change::Dense,
                    "slab" => Change::Slab(0.0),
                    "denoise" => Change::Denoise,
                    "colormap" => Change::Colormap,
                    "orientation" => Change::Orientation,
                    "inverted" => Change::Inverted,
                    "cursor" => Change::Cursor,
                    "calculi" => Change::Calculi(0),
                    "path" => Change::Path,
                    _ => Change::Oblique,
                };
                self.undo_change(c);
            }
            "phases" => self.compare_phases(),
            "hernia" => {
                self.open_hernia();
                for (i, v) in nums.iter().enumerate().take(7) {
                    self.hernia.mm[i] = Some(*v as f32);
                }
            }
            "trace" => {
                // Tubular tracker on the 2D viewer, from scripted points (x,y,z triples).
                let Some(series) = self.active_series() else { return };
                for p in nums.as_chunks::<3>().0 {
                    let hit = crate::ui::viewport::Hit { patient: Some(DVec3::new(p[0], p[1], p[2])), pixel: egui::Pos2::ZERO, mm: (0.0, 0.0), slice: 0, normal: Some(DVec3::Z) };
                    self.curve_add(hit, &series);
                }
            }
            "close_all" => self.confirm_close_all = true,
            "confirm" => {
                if self.confirm_close_all {
                    self.close_all_studies();
                }
            }
            "reopen" => self.reopen_closed(),
            "close_study" => {
                if let Some(uid) = self.library.studies.get(num.unwrap_or(0.0) as usize).map(|s| s.header.uid.clone()) {
                    self.close_study(&uid);
                }
            }
            "active" => self.active = (num.unwrap_or(0.0) as usize).min(5),
            "ws" => self.switch_workspace(match arg {
                "mpr" => Workspace::Mpr,
                "3d" => Workspace::Volume,
                "compare" => Workspace::Compare,
                _ => Workspace::Viewer,
            }),
            "layout" => {
                self.layout = match arg {
                    "2" => Layout::Cols2,
                    "r2" => Layout::Rows2,
                    "4" => Layout::Grid4,
                    "6" => Layout::Grid6,
                    _ => Layout::One,
                };
                self.active = 0;
                self.fill_empty_views();
            }
            "help" => self.show_help = true,
            "settings" => {
                self.show_settings = true;
                self.settings_tab = crate::ui::dialogs::SettingsTab::from_name(arg);
            }
            "about" => self.show_about = true,
            "pacs" => self.pacs.open = true,
            "maximize" => self.maximized = num.map(|n| n as usize),
            "inspector" => self.right_open = arg != "off",
            "vr3d" | "orbit" | "crop" => {
                use crate::render3d::renderer::RenderMode;
                let v = match self.workspace {
                    Workspace::Mpr => &mut self.mpr_views[3],
                    _ => &mut self.vr_view,
                };
                let v3 = &mut v.v3d;
                match name {
                    "vr3d" => {
                        v3.mode = match arg {
                            "iso" => RenderMode::Iso,
                            "mip" => RenderMode::Mip,
                            "minip" => RenderMode::MinIp,
                            _ => RenderMode::Composite,
                        }
                    }
                    "orbit" if nums.len() >= 2 => v3.camera.rotate(glam::Vec2::new(nums[0] as f32, nums[1] as f32)),
                    "crop" if nums.len() >= 3 => v3.crop[(nums[0] as usize).min(2)] = [nums[1] as f32, nums[2] as f32],
                    _ => {}
                }
            }
            "lighting" => {
                use crate::config::Lighting;
                self.settings.vr_lighting = match arg {
                    "simple" => Lighting::Simple,
                    "cinema" => Lighting::Cinematic,
                    _ => Lighting::Realistic,
                };
            }
            "store" => match arg {
                "now" => self.clean_store_now(None),
                "measure" => self.jobs.measure_store(self.paths.store.clone()),
                _ => self.settings.store_keep_days = num.unwrap_or(30.0) as u32,
            },
            "menu" => self.open_menu = Some(arg.to_string()),
            "series" => self.step_series(if arg == "prev" { -1 } else { 1 }),
            "nav" => self.nav_step(arg != "forward"),
            "focus" => self.toggle_focus(),
            "slice" => {
                if let Some(v) = self.active_view_mut() {
                    v.slice = (num.unwrap_or(0.0) as usize).min(v.slice_count().saturating_sub(1));
                }
            }
            "tour" => self.tour = Some(num.unwrap_or(0.0) as usize),
            "news" => self.show_news = true,
            "history" => {
                if arg == "redo" {
                    self.redo();
                } else {
                    self.undo();
                }
            }
            "theme" => {
                use crate::config::Theme;
                self.settings.theme = match arg {
                    "light" => Theme::Light,
                    "contrast" => Theme::HighContrast,
                    _ => Theme::Dark,
                };
            }
            "scale" => self.settings.ui_scale = num.unwrap_or(100.0) as f32 / 100.0,
            // Reading full screen without asking Windows (same window size for screenshots);
            // `reading:full` as F11, `reading:bar` / `reading:drawer` keep the controls showing.
            "reading" => match arg {
                "off" => {
                    let ctx = self.jobs.ctx();
                    self.leave_reading(&ctx);
                }
                "full" => {
                    let ctx = self.jobs.ctx();
                    self.toggle_reading(&ctx);
                }
                "bar" | "drawer" => {
                    self.reading = true;
                    self.reading_force = Some(arg.to_string());
                }
                _ => {
                    self.reading = true;
                    self.reading_force = None;
                }
            },
            // Click on dot `n` of the active patient's timeline (oldest first).
            "timeline" => {
                use crate::ui::timeline::TimelineAction;
                let Some(cur) = self.active_series() else { return };
                let group: Vec<usize> = (0..self.library.studies.len()).filter(|&i| self.library.studies[i].header.patient_id == cur.study.patient_id).collect();
                let (items, _) = self.timeline_items(&group, &cur.study.uid);
                let Some(item) = items.into_iter().nth(num.unwrap_or(0.0) as usize).filter(|i| !i.current) else { return };
                let action = match (item.local, item.remote) {
                    (Some(uid), _) => TimelineAction::Compare(uid),
                    (None, Some(r)) => TimelineAction::Retrieve(r, true),
                    _ => return,
                };
                self.timeline_action(action);
            }
            "info" => {
                use crate::config::InfoLevel;
                self.settings.reading_info = match arg {
                    "full" => InfoLevel::Full,
                    "none" => InfoLevel::None,
                    _ => InfoLevel::Minimal,
                };
            }
            "preview" => {
                if let Some(v) = self.active_view_mut() {
                    v.force_preview = num.map(|n| n as usize);
                }
            }
            "goto" => {
                self.open_goto();
                // `goto:open` only opens the box; `goto:81` or `goto:-30mm` also goes.
                if arg.trim_end_matches("mm").parse::<f64>().is_ok() {
                    if let Some(g) = self.goto.as_mut() {
                        g.mm = arg.ends_with("mm");
                        g.text = arg.trim_end_matches("mm").to_string();
                    }
                    self.apply_goto();
                }
            }
            _ => log::warn!("ação de automação desconhecida: {action}"),
        }
    }
}

static EXPORT_SLOT: std::sync::Mutex<Option<Arc<egui::ColorImage>>> = std::sync::Mutex::new(None);

fn chrono_like_stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    secs.to_string()
}

impl SlycedApp {
    /// Removes downloaded exams now: older than `days` (all when `None`), keeping the exams that are open.
    pub(crate) fn clean_store_now(&mut self, days: Option<u32>) {
        let keep = self.library.studies.iter().map(|s| s.header.uid.clone()).collect();
        self.jobs.clean_store(self.paths.store.clone(), days, keep);
    }
}

impl eframe::App for SlycedApp {
    /// Tab toggles the focus mode, so it is taken before egui uses it to move
    /// the keyboard focus (unless a text field is being edited).
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if ctx.text_edit_focused() {
            return;
        }
        let before = raw_input.events.len();
        raw_input.events.retain(|e| !matches!(e, egui::Event::Key { key: egui::Key::Tab, pressed: true, modifiers, .. } if modifiers.is_none()));
        raw_input.events.retain(|e| !matches!(e, egui::Event::Key { key: egui::Key::Tab, pressed: false, .. }));
        if raw_input.events.len() < before {
            self.tab_pressed = true;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.time = ctx.input(|i| i.time);
        let look = (self.settings.theme, self.settings.ui_scale.clamp(0.8, 1.5));
        if self.applied_look != Some(look) {
            theme::set_theme(&ctx, look.0);
            ctx.set_zoom_factor(look.1);
            #[cfg(windows)]
            theme::style_title_bar(frame);
            self.applied_look = Some(look);
        }
        self.drain_messages(&ctx);
        self.pump_transfers();
        self.request_priors();
        self.finish_export(&ctx);

        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() {
            self.open_paths(dropped, true);
        }
        self.track_history(&ctx);
        self.handle_shortcuts(&ctx);

        if self.reading && self.library.studies.is_empty() && self.scan.is_none() {
            self.leave_reading(&ctx);
        }
        // Reading full screen: only the images (its controls float over them).
        if !self.reading {
            self.top_bar(ui);
            self.status_bar(ui);
            if self.left_open {
                self.browser_panel(ui);
            }
            if self.right_open && !self.library.studies.is_empty() {
                self.inspector_panel(ui);
            }
            if !self.library.studies.is_empty() {
                self.filmstrip(ui);
            }
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
            if self.library.studies.is_empty() && self.scan.is_none() {
                self.welcome(ui);
            } else {
                self.viewports_ui(ui);
                self.hernia_poll();
                self.ellipsoid_poll();
            }
        });
        if self.reading {
            self.reading_ui(&ctx);
        }
        self.goto_ui(&ctx);

        self.pacs_window(&ctx);
        self.settings_window(&ctx);
        self.tags_window(&ctx);
        self.help_window(&ctx);
        self.about_window(&ctx);
        self.confirm_close_all_ui(&ctx);
        self.confirm_clear_store_ui(&ctx);
        self.news_ui(&ctx);
        self.tour_ui(&ctx);
        self.toasts_ui(&ctx);
        self.drop_overlay(&ctx);

        self.toasts.retain(|t| t.until > self.time);
        if !self.toasts.is_empty() || self.scan.is_some() || !self.loading.is_empty() || self.gpu_pending.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.run_automation(&ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::pal().bg_app.to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.save(&self.paths.config);
        if self.settings.store_clear_on_exit {
            crate::store::clean(&self.paths.store, None, &Default::default());
        }
    }
}
