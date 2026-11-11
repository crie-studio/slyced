//! A single image viewport: native stack, MPR plane or 3D volume.

use super::icons;
use super::theme::*;
use super::widgets::*;
use crate::dicom::model::{format_date, format_person_name};
use crate::dicom::series::{LoadedSeries, Slice};
use crate::imaging::cpr::CprImage;
use crate::imaging::highlight::{self, Highlights};
use crate::imaging::lut::{self, Colormap, Window};
use crate::imaging::register::{Rigid, Sim2};
use crate::imaging::reslice::{self, orientation_label, PlaneGrid, PlaneKind, SlabMode};
use crate::imaging::tissues;
use crate::imaging::volume::{next_id, Quant, OUTSIDE};
use crate::render3d::camera::OrbitCamera;
use crate::render3d::renderer::{GpuVolumeData, RenderMode, RenderParams, VolumeCallback, ACCUM_FRAMES};
use crate::render3d::tf::{self, TfAdjust};
use egui::{
    pos2, vec2, Align2, Color32, ColorImage, CornerRadius, Event, Id, Modifiers, MouseWheelUnit, PointerButton, Pos2,
    Rect, Response, Sense, Shape, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, Vec2,
};
use glam::{DVec3, Vec3};
use half::f16;
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    WindowLevel,
    Pan,
    Zoom,
    Scroll,
    Crosshair,
    Locate,
    Length,
    Angle,
    Ellipse,
    Probe,
    Arrow,
    Bookmark,
    Magnifier,
    Curve,
    Landmark,
}

impl Tool {
    pub const NAV: [Tool; 6] = [Tool::WindowLevel, Tool::Pan, Tool::Zoom, Tool::Scroll, Tool::Crosshair, Tool::Locate];
    pub const MEASURE: [Tool; 5] = [Tool::Length, Tool::Angle, Tool::Ellipse, Tool::Probe, Tool::Arrow];
    pub const FINDINGS: [Tool; 2] = [Tool::Bookmark, Tool::Magnifier];

    pub fn icon(self) -> &'static str {
        match self {
            Tool::WindowLevel => icons::CIRCLE_HALF,
            Tool::Pan => icons::HAND,
            Tool::Zoom => icons::MAGNIFYING_GLASS_PLUS,
            Tool::Scroll => icons::STACK,
            Tool::Crosshair => icons::CROSSHAIR,
            Tool::Locate => icons::CROSSHAIR_SIMPLE,
            Tool::Length => icons::RULER,
            Tool::Angle => icons::ANGLE,
            Tool::Ellipse => icons::CIRCLE,
            Tool::Probe => icons::TARGET,
            Tool::Arrow => icons::ARROW_UP_RIGHT,
            Tool::Bookmark => icons::MAP_PIN,
            Tool::Magnifier => icons::MAGNIFYING_GLASS,
            Tool::Curve => icons::BEZIER_CURVE,
            Tool::Landmark => icons::ANCHOR,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::WindowLevel => tr!("Janela/Nível (W)", "Window/Level (W)"),
            Tool::Pan => tr!("Mover (P)", "Pan (P)"),
            Tool::Zoom => "Zoom (Z)",
            Tool::Scroll => tr!("Navegar cortes (S)", "Scroll slices (S)"),
            Tool::Crosshair => tr!("Mira MPR (C) — arraste as alças para inclinar os planos", "MPR crosshair (C) — drag the handles to tilt the planes"),
            Tool::Locate => tr!(
                "Cursor 3D (L ou Ctrl+clique): leva todas as séries ao mesmo ponto",
                "3D cursor (L or Ctrl+click): brings every series to the same point"
            ),
            Tool::Length => tr!("Distância (D)", "Distance (D)"),
            Tool::Angle => tr!("Ângulo (A)", "Angle (A)"),
            Tool::Ellipse => tr!("ROI elíptica (E)", "Elliptical ROI (E)"),
            Tool::Probe => tr!("Densidade pontual (H)", "Point density (H)"),
            Tool::Arrow => tr!("Seta com texto (T)", "Arrow with text (T)"),
            Tool::Bookmark => tr!("Marcar achado (B)", "Mark finding (B)"),
            Tool::Magnifier => tr!("Lupa (M)", "Magnifier (M)"),
            Tool::Curve => tr!(
                "Curva CPR (V): clique ao longo da estrutura; botão direito conclui",
                "CPR curve (V): click along the structure; right-click finishes"
            ),
            Tool::Landmark => tr!("Ponto de referência (G): marque o mesmo ponto nos dois exames", "Landmark (G): mark the same point on both studies"),
        }
    }

    /// Short name (menus, status bar).
    pub fn name(self) -> &'static str {
        match self {
            Tool::WindowLevel => tr!("Janela/Nível", "Window/Level"),
            Tool::Pan => tr!("Mover", "Pan"),
            Tool::Zoom => "Zoom",
            Tool::Scroll => tr!("Navegar cortes", "Scroll slices"),
            Tool::Crosshair => tr!("Mira MPR", "MPR crosshair"),
            Tool::Locate => tr!("Cursor 3D", "3D cursor"),
            Tool::Length => tr!("Distância", "Distance"),
            Tool::Angle => tr!("Ângulo", "Angle"),
            Tool::Ellipse => tr!("ROI elíptica", "Elliptical ROI"),
            Tool::Probe => tr!("Densidade pontual", "Point density"),
            Tool::Arrow => tr!("Seta com texto", "Arrow with text"),
            Tool::Bookmark => tr!("Marcar achado", "Mark finding"),
            Tool::Magnifier => tr!("Lupa", "Magnifier"),
            Tool::Curve => tr!("Rastreador tubular", "Tube tracker"),
            Tool::Landmark => tr!("Ponto de referência", "Landmark"),
        }
    }

    /// Keyboard shortcut.
    pub fn key(self) -> &'static str {
        match self {
            Tool::WindowLevel => "W",
            Tool::Pan => "P",
            Tool::Zoom => "Z",
            Tool::Scroll => "S",
            Tool::Crosshair => "C",
            Tool::Locate => "L",
            Tool::Length => "D",
            Tool::Angle => "A",
            Tool::Ellipse => "E",
            Tool::Probe => "H",
            Tool::Arrow => "T",
            Tool::Bookmark => "B",
            Tool::Magnifier => "M",
            Tool::Curve => "V",
            Tool::Landmark => "G",
        }
    }

    /// How to use the tool, shown in the status bar while it is active.
    pub fn hint(self) -> &'static str {
        match self {
            Tool::WindowLevel => tr!(
                "arraste: horizontal muda o contraste, vertical o brilho · teclas 1–9 e 0: predefinições",
                "drag: horizontal changes the contrast, vertical the brightness · keys 1–9 and 0: presets"
            ),
            Tool::Pan => tr!(
                "arraste para mover a imagem · também botão do meio ou Shift+arrastar",
                "drag to move the image · also the middle button or Shift+drag"
            ),
            Tool::Zoom => tr!("arraste para cima ou para baixo · também Ctrl+roda ou teclas + e −", "drag up or down · also Ctrl+wheel or the + and − keys"),
            Tool::Scroll => tr!(
                "arraste para cima ou para baixo para passar os cortes · também roda do mouse",
                "drag up or down to go through the slices · also the mouse wheel"
            ),
            Tool::Crosshair => tr!(
                "clique para levar os planos ao ponto · arraste as alças coloridas para inclinar",
                "click to bring the planes to the point · drag the colored handles to tilt"
            ),
            Tool::Locate => tr!(
                "clique para levar todas as séries ao mesmo ponto · Esc apaga o cursor",
                "click to bring every series to the same point · Esc clears the cursor"
            ),
            Tool::Length => tr!(
                "clique e arraste · Del apaga a última · Shift+Del apaga todas",
                "click and drag · Del deletes the last one · Shift+Del deletes all"
            ),
            Tool::Angle => tr!("arraste o primeiro lado e clique onde termina o segundo", "drag the first side and click where the second one ends"),
            Tool::Ellipse => tr!("arraste para desenhar · mostra média, DP, mínimo, máximo e área", "drag to draw · shows mean, SD, minimum, maximum and area"),
            Tool::Probe => tr!(
                "clique para ler a densidade · a legenda mostra os valores típicos",
                "click to read the density · the legend shows the typical values"
            ),
            Tool::Arrow => tr!(
                "arraste da ponta até a cauda e digite o texto · Enter confirma",
                "drag from the tip to the tail and type the text · Enter confirms"
            ),
            Tool::Bookmark => tr!("clique no ponto do achado · a lista fica no painel lateral", "click on the finding · the list is in the side panel"),
            Tool::Magnifier => tr!("passe o mouse sobre a imagem para ampliar", "move the mouse over the image to magnify"),
            Tool::Curve => tr!(
                "clique ao longo da estrutura · Enter ou botão direito conclui · Backspace desfaz",
                "click along the structure · Enter or right-click finishes · Backspace undoes"
            ),
            Tool::Landmark => tr!(
                "marque o mesmo ponto em A e em B · com 3 pontos os exames se alinham",
                "mark the same point on A and B · with 3 points the studies align"
            ),
        }
    }

    /// Tools whose clicks place something, so double-click must not maximize.
    pub fn places_points(self) -> bool {
        matches!(self, Tool::Curve | Tool::Landmark | Tool::Bookmark | Tool::Arrow | Tool::Angle | Tool::Probe)
    }
}

#[derive(Clone)]
pub enum Content {
    Empty,
    Stack(Arc<LoadedSeries>),
    Mpr(Arc<LoadedSeries>, PlaneKind),
    Volume(Arc<LoadedSeries>),
    Cpr(Arc<LoadedSeries>, Arc<CprImage>),
}

impl Content {
    pub fn series(&self) -> Option<&Arc<LoadedSeries>> {
        match self {
            Content::Empty => None,
            Content::Stack(s) | Content::Mpr(s, _) | Content::Volume(s) | Content::Cpr(s, _) => Some(s),
        }
    }
}

/// How a 2D viewport builds its image.
#[derive(Clone)]
enum View2D {
    Stack,
    Plane(PlaneKind),
    Cpr(Arc<CprImage>),
}

/// A marker drawn on the views: the 3D cursor, findings, landmarks.
#[derive(Clone, Debug)]
pub struct Pin {
    pub id: u64,
    pub label: String,
    pub color: Color32,
    /// Patient position (same space as the view's series).
    pub point: Option<DVec3>,
    /// Image position for images without geometry: `(slice, pixel)`.
    pub image: Option<(usize, Pos2)>,
}

/// Exam B shown in exam A's plane (comparison with synchronized position).
#[derive(Clone)]
pub enum Linked {
    Plane { grid: PlaneGrid, k: f64, slab_mm: f32, slab_mode: SlabMode, map: Rigid, key: u64, version: u64 },
    Image { size: (usize, usize), spacing: (f64, f64), map: Sim2, slice: usize, key: u64, version: u64 },
}

/// Exam B drawn over exam A in the same viewport.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Fusion {
    Blend(f32),
    Swipe(f32),
    Checker(f32),
}

pub struct FusionLayer {
    pub tex: egui::TextureId,
    pub size: [usize; 2],
    pub style: Fusion,
}

/// Value of a measurement, for lists and before/after comparisons.
#[derive(Clone, Debug)]
pub struct MeasureValue {
    pub index: usize,
    pub kind: MeasureKind,
    pub center: Pos2,
    pub value: f32,
    pub unit: &'static str,
    /// Mean value inside an ROI.
    pub mean: Option<f32>,
}

/// Something placed with a click, in the clicked view's coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub patient: Option<DVec3>,
    pub pixel: Pos2,
    /// Pixel position in mm from the image corner.
    pub mm: (f64, f64),
    pub slice: usize,
    pub normal: Option<DVec3>,
}

/// Chosen in the menu of the right mouse button (handled by the app).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ContextAction {
    Tool(Tool),
    OpenMpr,
    OpenVolume,
    Copy,
    Save,
    ResetView,
    ResetAll,
    Maximize,
}

/// What a stack view keeps of a series when another one is opened in it.
#[derive(Clone)]
struct SavedState {
    slice: usize,
    window: Window,
    invert: bool,
    colormap: Colormap,
    zoom: Option<f32>,
    pan: Vec2,
    rot: u8,
    flip: (bool, bool),
    measurements: Vec<Measurement>,
}

/// Drag-and-drop payload carrying a series UID from the browser.
pub struct SeriesDrag(pub String);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MeasureKind {
    Length,
    Angle,
    Ellipse,
    Probe,
    Arrow,
}

#[derive(Clone, Debug)]
pub struct Measurement {
    pub kind: MeasureKind,
    /// Image coordinates (pixel `i` spans `[i, i+1)`). Arrows: tip, then tail.
    pub pts: Vec<Pos2>,
    pub key: u64,
    /// Annotation text (arrows).
    pub text: String,
}

/// State shared by the three planes of the MPR workspace.
pub struct MprShared {
    pub point: DVec3,
    pub slab_mm: f32,
    pub slab_mode: SlabMode,
    pub crosshair: bool,
    /// Screen-right and screen-down directions of the axial, coronal and
    /// sagittal planes; rotated by the crosshair handles (oblique MPR).
    pub axes: [(DVec3, DVec3); 3],
}

impl MprShared {
    pub fn new() -> Self {
        Self {
            point: DVec3::ZERO,
            slab_mm: 0.0,
            slab_mode: SlabMode::Mip,
            crosshair: true,
            axes: PlaneKind::ALL.map(|k| k.axes()),
        }
    }

    pub fn is_oblique(&self) -> bool {
        PlaneKind::ALL.iter().any(|k| {
            let (u, v) = self.axes[k.index()];
            let (su, sv) = k.axes();
            u.dot(su) < 0.9999 || v.dot(sv) < 0.9999
        })
    }

    pub fn reset_axes(&mut self) {
        self.axes = PlaneKind::ALL.map(|k| k.axes());
    }
}

impl Default for MprShared {
    fn default() -> Self {
        Self::new()
    }
}

/// Plane of the active image, drawn as a reference line on other viewports.
#[derive(Clone, Debug)]
pub struct RefPlane {
    pub study_uid: String,
    pub series: usize,
    pub point: DVec3,
    pub normal: DVec3,
}

pub struct ViewCtx<'a> {
    pub tool: Tool,
    pub reference: Option<RefPlane>,
    pub smooth: bool,
    pub overlays: bool,
    pub active: bool,
    pub mpr: Option<&'a mut MprShared>,
    pub gpu: Option<Arc<GpuVolumeData>>,
    pub gpu_pending: bool,
    /// Set when 3D rendering is unavailable on this computer (shown instead of the volume).
    pub gpu_issue: Option<String>,
    pub loading: Option<(String, f32)>,
    pub time: f64,
    pub quality: f32,
    /// 3D cursor in this view's patient space.
    pub cursor: Option<DVec3>,
    pub pins: Vec<Pin>,
    /// Path being drawn for a curved reformation.
    pub curve: Option<&'a [DVec3]>,
    pub linked: Option<Linked>,
    pub fusion: Option<FusionLayer>,
    pub magnifier: f32,
    /// Corner label identifying the exam (comparison).
    pub tag: Option<(String, Color32)>,
    /// Shows a close button while the pointer is over the viewport.
    pub closable: bool,
    /// What the tools changed on screen, shown as removable labels on this view.
    pub changes: Vec<Change>,
    /// Shows the small front view of the volume (axial stacks).
    pub localizer: bool,
    /// Opens the right-button menu now (automation).
    pub open_context: bool,
    /// Light of the 3D view.
    pub lighting: crate::config::Lighting,
    /// Reading full screen: no frame, less text, a thinner slice bar.
    pub reading: bool,
    /// Text over the image in the reading full screen.
    pub info: crate::config::InfoLevel,
    /// The reading controls are showing (the mouse moved a moment ago).
    pub chrome: bool,
    /// More than one view is on screen (the active one gets a corner mark in reading).
    pub several: bool,
}

impl<'a> ViewCtx<'a> {
    pub fn new(tool: Tool, time: f64) -> Self {
        Self {
            tool,
            reference: None,
            smooth: true,
            overlays: true,
            active: false,
            mpr: None,
            gpu: None,
            gpu_pending: false,
            gpu_issue: None,
            loading: None,
            time,
            quality: 1.0,
            cursor: None,
            pins: Vec::new(),
            curve: None,
            linked: None,
            fusion: None,
            magnifier: 3.0,
            tag: None,
            closable: false,
            changes: Vec::new(),
            localizer: false,
            open_context: false,
            lighting: crate::config::Lighting::Realistic,
            reading: false,
            info: crate::config::InfoLevel::Full,
            chrome: false,
            several: false,
        }
    }
}

/// Something the reading tools changed on screen, shown as a removable label.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Change {
    Measures(usize),
    Gas,
    Fat,
    Dense,
    Slab(f32),
    Denoise,
    Colormap,
    Orientation,
    Inverted,
    Cursor,
    Calculi(usize),
    Path,
    Oblique,
}

impl Change {
    pub fn label(self) -> String {
        match self {
            Change::Measures(1) => tr!("1 medida", "1 measurement").into(),
            Change::Measures(n) => trf!("{n} medidas", "{n} measurements"),
            Change::Gas => tr!("Gás livre", "Free air").into(),
            Change::Fat => tr!("Gordura mesentérica", "Mesenteric fat").into(),
            Change::Dense => tr!("Cálculos", "Stones").into(),
            Change::Slab(mm) => format!("Slab {mm:.0} mm"),
            Change::Denoise => tr!("Filtro de ruído", "Noise reduction").into(),
            Change::Colormap => tr!("Mapa de cores", "Color map").into(),
            Change::Orientation => tr!("Girada/espelhada", "Rotated/flipped").into(),
            Change::Inverted => tr!("Cores invertidas", "Inverted colors").into(),
            Change::Cursor => tr!("Cursor 3D", "3D cursor").into(),
            Change::Calculi(1) => tr!("1 foco marcado", "1 focus marked").into(),
            Change::Calculi(n) => trf!("{n} focos marcados", "{n} foci marked"),
            Change::Path => tr!("Trajeto do rastreador", "Tracker path").into(),
            Change::Oblique => tr!("Planos oblíquos", "Oblique planes").into(),
        }
    }

    fn color(self) -> Color32 {
        match self {
            Change::Gas => highlight::GAS,
            Change::Fat => highlight::FAT,
            Change::Dense | Change::Calculi(_) => highlight::DENSE,
            Change::Measures(_) => MEASURE,
            _ => ACCENT,
        }
    }
}

#[derive(Default)]
pub struct ViewEvents {
    pub activated: bool,
    pub double_clicked: bool,
    pub dropped_series: Option<String>,
    pub need_gpu: Option<Arc<LoadedSeries>>,
    pub status: Option<String>,
    /// Ctrl+click or Cursor 3D tool.
    pub cursor: Option<Hit>,
    pub bookmark: Option<Hit>,
    pub landmark: Option<Hit>,
    pub curve_point: Option<Hit>,
    pub curve_finish: bool,
    pub swipe: Option<f32>,
    /// Degrees to rotate the straightened (CPR) image (mouse wheel over it).
    pub cpr_rotate: f32,
    /// The close button was clicked.
    pub close: bool,
    /// A change label was removed.
    pub undo: Option<Change>,
    /// Chosen in the right-button menu.
    pub context: Option<ContextAction>,
    /// The view jumped from this slice (slice bar, localizer): kept for "voltar".
    pub jump_from: Option<usize>,
    /// "Restaurar tudo" was clicked.
    pub reset_all: bool,
}

pub struct View3D {
    pub camera: OrbitCamera,
    pub mode: RenderMode,
    pub preset: usize,
    pub adjust: TfAdjust,
    pub opacity: f32,
    pub shading: bool,
    /// Crop range along the patient axes (R→L, A→P, I→S), normalized.
    pub crop: [[f32; 2]; 3],
    pub mip_window: Option<Window>,
    pub iso: Option<f32>,
    tf: Option<(TfKey, Arc<Vec<f16>>, (f32, f32), u64)>,
    interacting_until: f64,
    /// Frames in the cinematic image (written by the renderer).
    progress: Arc<std::sync::atomic::AtomicU32>,
    /// What was last sent to the renderer: a change restarts the cinematic refinement.
    last_params: Option<(RenderParams, [u32; 2])>,
    refining: bool,
    refine_start: f64,
}

#[derive(Clone, Copy, PartialEq)]
struct TfKey {
    preset: usize,
    adjust: TfAdjust,
    range: (f32, f32),
}

static TF_VERSION: AtomicU64 = AtomicU64::new(1);

impl View3D {
    /// Initial 3D settings for a series: VR for CT, MIP for other modalities.
    fn for_series(s: &LoadedSeries) -> Self {
        if s.info.modality == "CT" {
            Self { preset: 0, ..Self::default() }
        } else {
            Self { preset: tf::PRESETS.len() - 1, mode: RenderMode::Mip, ..Self::default() }
        }
    }
}

impl Default for View3D {
    fn default() -> Self {
        let mut camera = OrbitCamera::default();
        camera.distance = 4.0;
        Self {
            camera,
            mode: RenderMode::Composite,
            preset: 0,
            adjust: TfAdjust::default(),
            opacity: 1.0,
            shading: true,
            crop: [[0.0, 1.0]; 3],
            mip_window: None,
            iso: None,
            tf: None,
            interacting_until: 0.0,
            progress: Arc::default(),
            last_params: None,
            refining: false,
            refine_start: 0.0,
        }
    }
}

pub struct Viewport {
    pub id: u64,
    pub content: Content,
    pub slice: usize,
    pub window: Window,
    pub invert: bool,
    pub colormap: Colormap,
    /// Screen points per mm; `None` = fit to viewport.
    pub zoom: Option<f32>,
    pub pan: Vec2,
    pub rot: u8,
    pub flip_h: bool,
    pub flip_v: bool,
    pub measurements: Vec<Measurement>,
    drawing: Option<Measurement>,
    /// Arrow whose text is being typed.
    editing: Option<(usize, bool)>,
    /// Values of the measurements on the displayed image (updated every frame).
    pub values: Vec<MeasureValue>,
    pub cine: bool,
    pub cine_fps: f32,
    cine_acc: f64,
    wheel_acc: f32,
    tex: Option<TextureHandle>,
    tex_key: Option<u64>,
    lut: Vec<Color32>,
    lut_key: Option<(u32, u32, u32, u32, bool, Colormap)>,
    grid: Option<(u64, PlaneKind, (DVec3, DVec3), PlaneGrid)>,
    /// Grid of exam A while this view shows exam B in A's planes.
    link_grid: Option<PlaneGrid>,
    mpr_buf: Option<(u64, Arc<Vec<i16>>, Quant)>,
    filtered: Option<(u64, Arc<Vec<i16>>)>,
    /// Thick slab over neighbouring native images (2D stack).
    pub slab_mm: f32,
    pub slab_mode: SlabMode,
    /// Edge-preserving noise filter for display: 0 off, 1–3 strength.
    pub denoise: u8,
    /// Pans to this patient point if it is out of view (cursor, findings).
    pub center_on: Option<DVec3>,
    /// Zoom to apply relative to "fit" on the next frame.
    pub zoom_request: Option<f32>,
    rotating: bool,
    /// Geometry of the last drawn 2D image: origin, axes, pixel size, key.
    last_geom: Option<(DVec3, DVec3, DVec3, f32, f32, u64)>,
    /// Space kept free at the top right for the close button.
    top_inset: f32,
    /// Color highlights over the image (free gas, dense foci, densified fat).
    pub highlights: Highlights,
    /// Window and inversion to restore when a special reading mode is turned off.
    pub restore: Option<(Window, bool)>,
    overlay_tex: Option<TextureHandle>,
    overlay_key: Option<u64>,
    overlay_on: bool,
    pub v3d: View3D,
    /// Fit-to-viewport scale of the last drawn 2D frame (points per mm).
    last_fit: f32,
    /// State of each series shown here before (by UID), restored when it comes back.
    saved: Vec<(String, SavedState)>,
    /// Slice when a press on the slice bar or the localizer began.
    jump_start: Option<usize>,
    /// Point of the last right click (context menu).
    context_hit: Option<Hit>,
    /// Front projection of the volume for the localizer: `(volume id, texture)`.
    scout: Option<(u64, TextureHandle)>,
    /// Small image of the slice under the pointer on the slice bar.
    preview: Option<SlicePreview>,
    /// Shows the slice bar preview of this slice without the pointer (automation).
    pub force_preview: Option<usize>,
}

/// Slice bar preview: key, texture, size in pixels and pixel size in mm.
type SlicePreview = (u64, TextureHandle, [usize; 2], (f32, f32));

fn hash_of(v: impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

fn grid_hash(g: &PlaneGrid) -> u64 {
    let bits = |d: DVec3| [d.x.to_bits(), d.y.to_bits(), d.z.to_bits()];
    hash_of((bits(g.u), bits(g.v), bits(g.origin), g.pixel.to_bits(), g.step.to_bits(), g.width, g.height))
}

/// Image currently displayed by a 2D viewport.
struct Frame {
    w: usize,
    h: usize,
    src: FrameSrc,
    /// Identifies the pixel source for texture/filter caches.
    src_key: u64,
    /// Filtered copy used for display only (noise filter).
    display: Option<Arc<Vec<i16>>>,
    display_key: u64,
    dx: f32,
    dy: f32,
    spacing_known: bool,
    origin: Option<DVec3>,
    u: DVec3,
    v: DVec3,
    key: u64,
    invert: bool,
    thickness: Option<f64>,
    cpr: Option<Arc<CprImage>>,
}

enum FrameSrc {
    Stack(Arc<LoadedSeries>, usize),
    Buffer(Arc<Vec<i16>>, Quant),
}

impl Frame {
    fn slice(&self) -> Option<Slice<'_>> {
        match &self.src {
            FrameSrc::Stack(s, k) => s.slice(*k),
            FrameSrc::Buffer(d, q) => Some(Slice::Gray(d, *q)),
        }
    }

    fn normal(&self) -> DVec3 {
        self.u.cross(self.v)
    }

    /// Image position of a patient point and its distance from the image plane (mm).
    fn image_of(&self, p: DVec3) -> Option<(Pos2, f64)> {
        if let Some(c) = &self.cpr {
            let (row, _) = c.nearest_row(p)?;
            let d = p - c.centers[row];
            let x = d.dot(c.across[row]) / c.pixel + (c.width as f64 - 1.0) * 0.5 + 0.5;
            let off = (d - c.across[row] * d.dot(c.across[row])).length();
            return Some((pos2(x as f32, row as f32 + 0.5), off));
        }
        let o = self.origin?;
        let d = p - o;
        Some((
            pos2((d.dot(self.u) / self.dx as f64) as f32 + 0.5, (d.dot(self.v) / self.dy as f64) as f32 + 0.5),
            d.dot(self.normal()),
        ))
    }

    fn hit(&self, p: Pos2, slice: usize) -> Hit {
        Hit {
            patient: self.patient(p),
            pixel: p,
            mm: (p.x as f64 * self.dx as f64, p.y as f64 * self.dy as f64),
            slice,
            normal: self.origin.map(|_| self.normal()),
        }
    }

    fn real_at(&self, p: Pos2) -> Option<f32> {
        if p.x < 0.0 || p.y < 0.0 {
            return None;
        }
        let (x, y) = (p.x as usize, p.y as usize);
        if x >= self.w || y >= self.h {
            return None;
        }
        match self.slice()? {
            Slice::Gray(d, q) => {
                let s = *d.get(y * self.w + x)?;
                (s != OUTSIDE).then(|| q.real(s))
            }
            Slice::Rgb(_) => None,
        }
    }

    fn patient(&self, p: Pos2) -> Option<DVec3> {
        if let Some(c) = &self.cpr {
            return c.patient(p.x as f64, p.y as f64);
        }
        self.origin.map(|o| {
            o + self.u * ((p.x as f64 - 0.5) * self.dx as f64) + self.v * ((p.y as f64 - 0.5) * self.dy as f64)
        })
    }
}

/// Image <-> screen mapping with zoom, pan, quarter rotations and flips.
#[derive(Clone, Copy)]
struct Xf {
    c: Pos2,
    s: f32,
    dx: f32,
    dy: f32,
    w_mm: f32,
    h_mm: f32,
    rot: u8,
    fh: bool,
    fv: bool,
}

impl Xf {
    fn to_screen(&self, p: Pos2) -> Pos2 {
        let (mut a, mut b) = (p.x * self.dx - self.w_mm * 0.5, p.y * self.dy - self.h_mm * 0.5);
        if self.fh {
            a = -a;
        }
        if self.fv {
            b = -b;
        }
        for _ in 0..self.rot {
            (a, b) = (-b, a);
        }
        self.c + vec2(a, b) * self.s
    }

    fn to_image(&self, q: Pos2) -> Pos2 {
        let v = (q - self.c) / self.s;
        let (mut a, mut b) = (v.x, v.y);
        for _ in 0..self.rot {
            (a, b) = (b, -a);
        }
        if self.fh {
            a = -a;
        }
        if self.fv {
            b = -b;
        }
        pos2((a + self.w_mm * 0.5) / self.dx, (b + self.h_mm * 0.5) / self.dy)
    }

    /// Screen direction expressed in image millimetres.
    fn dir_to_image_mm(&self, d: Vec2) -> Vec2 {
        let (mut a, mut b) = (d.x, d.y);
        for _ in 0..self.rot {
            (a, b) = (b, -a);
        }
        if self.fh {
            a = -a;
        }
        if self.fv {
            b = -b;
        }
        vec2(a, b)
    }
}

/// Wheel notches this frame (positive = towards the user / next slice).
fn wheel_steps(ui: &Ui) -> (f32, Modifiers) {
    ui.input(|i| {
        let mut steps = 0.0;
        let mut mods = i.modifiers;
        for e in &i.events {
            if let Event::MouseWheel { unit, delta, modifiers, .. } = e {
                mods = *modifiers;
                steps += match unit {
                    MouseWheelUnit::Line => delta.y,
                    MouseWheelUnit::Point => delta.y / 50.0,
                    MouseWheelUnit::Page => delta.y * 10.0,
                };
            }
        }
        (steps, mods)
    })
}

impl Viewport {
    pub fn new() -> Self {
        Self {
            id: next_id(),
            content: Content::Empty,
            slice: 0,
            window: Window::new(40.0, 400.0),
            invert: false,
            colormap: Colormap::Gray,
            zoom: None,
            pan: Vec2::ZERO,
            rot: 0,
            flip_h: false,
            flip_v: false,
            measurements: Vec::new(),
            drawing: None,
            editing: None,
            values: Vec::new(),
            cine: false,
            cine_fps: 15.0,
            cine_acc: 0.0,
            wheel_acc: 0.0,
            tex: None,
            tex_key: None,
            lut: Vec::new(),
            lut_key: None,
            grid: None,
            link_grid: None,
            mpr_buf: None,
            filtered: None,
            slab_mm: 0.0,
            slab_mode: SlabMode::Mip,
            denoise: 0,
            center_on: None,
            zoom_request: None,
            rotating: false,
            last_geom: None,
            top_inset: 0.0,
            highlights: Highlights::default(),
            restore: None,
            overlay_tex: None,
            overlay_key: None,
            overlay_on: false,
            v3d: View3D::default(),
            last_fit: 0.0,
            saved: Vec::new(),
            jump_start: None,
            context_hit: None,
            scout: None,
            preview: None,
            force_preview: None,
        }
    }

    /// Plane displayed by an MPR view (its grid and slice position), used to
    /// show another exam in the same plane.
    pub fn plane_state(&mut self, mpr: &MprShared) -> Option<(PlaneGrid, f64, u64)> {
        let Content::Mpr(s, kind) = &self.content else { return None };
        let (vol, kind) = (s.volume.clone()?, *kind);
        let g = self.grid_for(&vol, kind, mpr.axes[kind.index()]);
        let k = g.locate(mpr.point).z.round().clamp(0.0, (g.slices - 1) as f64);
        let key = ((kind as u64) << 32) | k as u64;
        Some((g, k, key))
    }

    /// Moves a stack to the image nearest to `p` (if the series covers it) and
    /// pans it into view. Returns whether the series covers the point.
    pub fn go_to_point(&mut self, p: DVec3) -> bool {
        let Content::Stack(s) = &self.content else { return false };
        let best = s
            .images
            .iter()
            .enumerate()
            .filter_map(|(k, im)| {
                let (o, (r, c)) = (im.position?, im.orientation?);
                Some((k, (p - o).dot(r.cross(c)).abs(), im.thickness.unwrap_or(1.0)))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((k, dist, thick)) if dist <= thick.max(1.0) * 1.5 + 1.0 => {
                self.slice = k;
                self.cine = false;
                self.center_on = Some(p);
                true
            }
            _ => false,
        }
    }

    pub fn set_content(&mut self, content: Content) {
        let same_series = match (self.content.series(), content.series()) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        if !same_series {
            self.save_state();
        }
        let is_3d = matches!(content, Content::Volume(_));
        self.content = content;
        self.tex_key = None;
        self.overlay_key = None;
        self.mpr_buf = None;
        self.filtered = None;
        self.link_grid = None;
        self.drawing = None;
        self.editing = None;
        if !same_series {
            self.measurements.clear();
            self.slab_mm = 0.0;
            self.reset_view();
            if let Some(s) = self.content.series() {
                self.slice = if matches!(self.content, Content::Stack(_)) { 0 } else { self.slice };
                self.window = s.default_window;
                if is_3d {
                    self.v3d = View3D::for_series(s);
                }
            }
            self.restore_state();
        }
    }

    /// Keeps where the series of this stack view was (slice, window, zoom, measurements).
    fn save_state(&mut self) {
        let Content::Stack(s) = &self.content else { return };
        let state = SavedState {
            slice: self.slice,
            window: self.window,
            invert: self.invert,
            colormap: self.colormap,
            zoom: self.zoom,
            pan: self.pan,
            rot: self.rot,
            flip: (self.flip_h, self.flip_v),
            measurements: self.measurements.clone(),
        };
        let uid = s.uid.clone();
        self.saved.retain(|(u, _)| *u != uid);
        self.saved.push((uid, state));
        if self.saved.len() > 30 {
            self.saved.remove(0);
        }
    }

    /// Opens the series where it was left in this view.
    fn restore_state(&mut self) {
        let Content::Stack(s) = &self.content else { return };
        let Some((_, st)) = self.saved.iter().find(|(u, _)| *u == s.uid).cloned() else { return };
        self.slice = st.slice.min(s.len().saturating_sub(1));
        self.window = st.window;
        self.invert = st.invert;
        self.colormap = st.colormap;
        self.zoom = st.zoom;
        self.pan = st.pan;
        self.rot = st.rot;
        (self.flip_h, self.flip_v) = st.flip;
        self.measurements = st.measurements;
    }

    /// Slice of this view nearest to patient point `p` (stack image or MPR plane), if the view covers it.
    pub fn slice_of_point(&self, p: DVec3) -> Option<usize> {
        match &self.content {
            Content::Stack(s) => s
                .images
                .iter()
                .enumerate()
                .filter_map(|(k, im)| {
                    let (o, (r, c)) = (im.position?, im.orientation?);
                    Some((k, (p - o).dot(r.cross(c)).abs(), im.thickness.unwrap_or(1.0)))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .filter(|(_, d, t)| *d <= t.max(1.0) * 1.5 + 1.0)
                .map(|(k, _, _)| k),
            Content::Mpr(..) => {
                let (_, _, _, g) = self.grid.as_ref()?;
                let z = g.locate(p).z.round();
                (z >= 0.0 && (z as usize) < g.slices).then_some(z as usize)
            }
            _ => None,
        }
    }

    /// Marks on the slice bar: measurements, findings, dense foci and the 3D
    /// cursor, with the name shown in the slice preview.
    fn slice_markers(&self, cx: &ViewCtx) -> Vec<(usize, Color32, String)> {
        let plane = match &self.content {
            Content::Stack(_) => None,
            Content::Mpr(_, k) => Some(*k as u64),
            _ => return Vec::new(),
        };
        let mut out: Vec<(usize, Color32, String)> = self
            .measurements
            .iter()
            .filter(|m| plane.is_none_or(|k| m.key >> 32 == k))
            .map(|m| ((m.key & 0xFFFF_FFFF) as usize, MEASURE, tr!("Medida", "Measurement").to_string()))
            .collect();
        for pin in &cx.pins {
            let k = match (pin.point, pin.image) {
                (Some(p), _) => self.slice_of_point(p),
                (None, Some((k, _))) => Some(k),
                _ => None,
            };
            let label = if pin.label.trim().is_empty() { tr!("Achado", "Finding").to_string() } else { pin.label.trim().to_string() };
            out.extend(k.map(|k| (k, pin.color, label)));
        }
        if let Some(k) = cx.cursor.and_then(|c| self.slice_of_point(c)) {
            out.push((k, Color32::from_rgb(40, 230, 255), tr!("Cursor 3D", "3D cursor").to_string()));
        }
        out
    }

    /// Position along the slice normal (mm) of slice `k`, as "Posição" in the
    /// image corner.
    pub fn slice_position(&self, k: usize) -> Option<f64> {
        match &self.content {
            Content::Stack(s) => {
                let im = s.images.get(k)?;
                let (p, (r, c)) = (im.position?, im.orientation?);
                Some(p.dot(r.cross(c).normalize()))
            }
            Content::Mpr(..) => {
                let g = self.link_grid.as_ref().or(self.grid.as_ref().map(|g| &g.3))?;
                (k < g.slices).then(|| g.point(0.0, 0.0, k as f64).dot(g.n))
            }
            _ => None,
        }
    }

    /// Slice closest to position `mm` along the normal (`None` without geometry).
    pub fn nearest_slice(&self, mm: f64) -> Option<usize> {
        (0..self.slice_count())
            .filter_map(|k| self.slice_position(k).map(|p| (k, (p - mm).abs())))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(k, _)| k)
    }

    /// Small image of slice `k` with the current window (slice bar preview):
    /// texture, size in pixels and pixel size in mm.
    fn slice_preview(&mut self, ui: &Ui, k: usize) -> Option<(egui::TextureId, [usize; 2], (f32, f32))> {
        const SIDE: f64 = 140.0;
        let window = (self.window.center.to_bits(), self.window.width.to_bits());
        let (key, size, spacing, pixels) = match &self.content {
            Content::Stack(s) => {
                let im = s.images.get(k)?;
                let invert = self.invert ^ im.invert;
                let key = hash_of((s.uid.as_str(), k, window, invert, self.colormap));
                if let Some(p) = self.preview.as_ref().filter(|p| p.0 == key) {
                    return Some((p.1.id(), p.2, p.3));
                }
                let step = ((im.width.max(im.height) as f64 / SIDE).ceil() as usize).max(1);
                let (w, h) = (im.width.div_ceil(step), im.height.div_ceil(step));
                let at = |x: usize, y: usize| (y * step).min(im.height - 1) * im.width + (x * step).min(im.width - 1);
                let mut pixels = Vec::new();
                match s.slice(k)? {
                    Slice::Gray(d, q) => {
                        let mut table = Vec::new();
                        lut::build_lut(q, self.window, invert, self.colormap, &mut table);
                        pixels = (0..w * h).map(|i| d.get(at(i % w, i / w)).map_or(Color32::BLACK, |&v| table[(v as i32 + 32768) as usize])).collect();
                    }
                    Slice::Rgb(d) => {
                        let rgb: Vec<u8> = (0..w * h).flat_map(|i| {
                            let j = at(i % w, i / w) * 3;
                            d.get(j..j + 3).map_or([0, 0, 0], |c| [c[0], c[1], c[2]])
                        }).collect();
                        lut::apply_rgb_window(&rgb, self.window, invert, &mut pixels);
                    }
                }
                (key, [w, h], (im.spacing.0 as f32 * step as f32, im.spacing.1 as f32 * step as f32), pixels)
            }
            Content::Mpr(s, _) if self.link_grid.is_none() => {
                let vol = s.volume.clone()?;
                let g = &self.grid.as_ref()?.3;
                let key = hash_of((vol.id, grid_hash(g), k, window, self.invert, self.colormap));
                if let Some(p) = self.preview.as_ref().filter(|p| p.0 == key) {
                    return Some((p.1.id(), p.2, p.3));
                }
                let f = (g.width.max(g.height) as f64 / SIDE).max(1.0);
                let small = PlaneGrid {
                    pixel: g.pixel * f,
                    width: ((g.width as f64 / f).ceil() as usize).max(1),
                    height: ((g.height as f64 / f).ceil() as usize).max(1),
                    ..g.clone()
                };
                let mut buf = Vec::new();
                reslice::reslice(&vol, &small, k as f64, 0.0, SlabMode::Avg, &mut buf);
                let mut table = Vec::new();
                lut::build_lut(vol.quant, self.window, self.invert, self.colormap, &mut table);
                let mut pixels = Vec::new();
                lut::apply_lut(&buf, &table, &mut pixels);
                (key, [small.width, small.height], (small.pixel as f32, small.pixel as f32), pixels)
            }
            _ => return None,
        };
        if pixels.len() != size[0] * size[1] {
            return None;
        }
        let img = ColorImage::new(size, pixels);
        match &mut self.preview {
            Some(p) => {
                p.1.set(img, TextureOptions::LINEAR);
                (p.0, p.2, p.3) = (key, size, spacing);
            }
            None => self.preview = Some((key, ui.ctx().load_texture(format!("preview-{}", self.id), img, TextureOptions::LINEAR), size, spacing)),
        }
        self.preview.as_ref().map(|p| (p.1.id(), p.2, p.3))
    }

    /// Small front view of the volume at the top right, with the level of the
    /// current image; clicking or dragging on it goes to that level.
    fn draw_localizer(&mut self, ui: &Ui, painter: &egui::Painter, rect: Rect, series: &LoadedSeries, ev: &mut ViewEvents) {
        let Some(vol) = series.volume.clone() else { return };
        if self.scout.as_ref().is_none_or(|s| s.0 != vol.id) {
            let image = crate::ui::previews::scout_image(&vol);
            self.scout = Some((vol.id, ui.ctx().load_texture(format!("scout-{}", self.id), image, TextureOptions::LINEAR)));
        }
        let Some((_, tex)) = &self.scout else { return };
        let size = tex.size_vec2();
        let h = (rect.height() * 0.2).clamp(90.0, 150.0);
        let w = (h * size.x / size.y).clamp(40.0, 170.0);
        let pad_r = if self.slice_count() > 1 { 26.0 } else { 10.0 };
        let r = Rect::from_min_size(pos2(rect.right() - pad_r - w, rect.top() + 66.0 + self.top_inset), vec2(w, h));
        painter.rect_filled(r.expand(3.0), CornerRadius::same(6), Color32::from_black_alpha(200));
        painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        painter.rect_stroke(r.expand(3.0), CornerRadius::same(6), Stroke::new(1.0, Color32::from_white_alpha(50)), StrokeKind::Inside);
        // Same orientation as `scout_image`: head at the top.
        let up = vol.axes[2].z >= 0.0;
        let nz = vol.dims[2] as f32;
        let y_of = |z: f32| {
            let t = ((z + 0.5) / nz).clamp(0.0, 1.0);
            r.top() + (if up { 1.0 - t } else { t }) * r.height()
        };
        if let Some(p) = series.images.get(self.slice).and_then(|im| im.position) {
            let y = y_of(vol.patient_to_voxel(p).z as f32);
            painter.line_segment([pos2(r.left() - 3.0, y), pos2(r.right() + 3.0, y)], Stroke::new(2.0, ACCENT));
        }
        let resp = ui.interact(r, Id::unique(("scout", self.id)), Sense::click_and_drag());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let down = resp.is_pointer_button_down_on();
        if down {
            ev.activated = true;
            self.jump_start.get_or_insert(self.slice);
            if let Some(pos) = resp.interact_pointer_pos() {
                let t = ((pos.y - r.top()) / r.height()).clamp(0.0, 1.0);
                let z = (if up { 1.0 - t } else { t }) * nz - 0.5;
                let p = vol.voxel_to_patient(DVec3::new(vol.dims[0] as f64 * 0.5, vol.dims[1] as f64 * 0.5, z as f64));
                if let Some(k) = self.slice_of_point(p) {
                    self.slice = k;
                    self.cine = false;
                }
            }
        } else if resp.drag_stopped() || resp.clicked() {
            self.finish_jump(ev);
        }
        resp.on_hover_text(tr!("Localizador: clique ou arraste para ir a esse nível", "Localizer: click or drag to go to that level"));
    }

    /// Reports a jump of 3 or more slices, for "voltar" (Alt+←).
    fn finish_jump(&mut self, ev: &mut ViewEvents) {
        if let Some(from) = self.jump_start.take()
            && from.abs_diff(self.slice) >= 3 {
                ev.jump_from = Some(from);
            }
    }

    /// Turns off highlights, slab, noise filter and color map; the window a
    /// special reading mode replaced comes back.
    pub fn clear_enhancements(&mut self) {
        if let Some((w, inv)) = self.restore.take() {
            self.window = w;
            self.invert = inv;
        }
        self.highlights = Highlights::default();
        self.slab_mm = 0.0;
        self.denoise = 0;
        self.colormap = Colormap::Gray;
    }

    /// Removes measurements and annotations, including one being drawn.
    pub fn clear_measurements(&mut self) {
        self.measurements.clear();
        self.drawing = None;
        self.editing = None;
    }

    /// Back to how the series opened: view, measurements, enhancements and 3D settings.
    pub fn reset_all(&mut self) {
        self.clear_enhancements();
        self.clear_measurements();
        self.cine = false;
        self.reset_view();
        if let Content::Volume(s) = &self.content {
            self.v3d = View3D::for_series(s);
        }
    }

    /// The cinematic 3D light is still being refined (automation waits for it).
    pub fn refining(&self) -> bool {
        self.v3d.refining
    }

    pub fn reset_view(&mut self) {
        self.zoom = None;
        self.pan = Vec2::ZERO;
        self.rot = 0;
        self.flip_h = false;
        self.flip_v = false;
        self.invert = false;
        if let Some(s) = self.content.series() {
            self.window = s.default_window;
        }
        self.v3d.camera = View3D::default().camera;
    }

    /// Adds a circular ROI of radius `r_mm` centered on patient point `p` to
    /// the image last drawn (scripted measurements).
    pub fn add_ellipse_at(&mut self, p: DVec3, r_mm: f64) {
        let Some((o, u, v, dx, dy, key)) = self.last_geom else { return };
        let d = p - o;
        let c = pos2((d.dot(u) / dx as f64) as f32 + 0.5, (d.dot(v) / dy as f64) as f32 + 0.5);
        let (rx, ry) = ((r_mm / dx as f64) as f32, (r_mm / dy as f64) as f32);
        self.measurements.push(Measurement { kind: MeasureKind::Ellipse, pts: vec![c - vec2(rx, ry), c + vec2(rx, ry)], key, text: String::new() });
    }

    /// Key of the image last drawn (slice or plane position).
    pub fn current_key(&self) -> Option<u64> {
        self.last_geom.map(|g| g.5)
    }

    /// Patient position of a pixel of the image last drawn.
    pub fn pixel_patient(&self, p: Pos2) -> Option<DVec3> {
        let (o, u, v, dx, dy, _) = self.last_geom?;
        Some(o + u * ((p.x as f64 - 0.5) * dx as f64) + v * ((p.y as f64 - 0.5) * dy as f64))
    }

    /// Adds a density probe at patient point `p` (scripted measurements).
    pub fn add_probe_at(&mut self, p: DVec3) {
        let Some((o, u, v, dx, dy, key)) = self.last_geom else { return };
        let d = p - o;
        let at = pos2((d.dot(u) / dx as f64) as f32 + 0.5, (d.dot(v) / dy as f64) as f32 + 0.5);
        self.measurements.push(Measurement { kind: MeasureKind::Probe, pts: vec![at], key, text: String::new() });
    }

    /// Adds a distance between patient points `a` and `b` on the image last drawn (scripted measurements).
    pub fn add_length_at(&mut self, a: DVec3, b: DVec3) {
        let Some((o, u, v, dx, dy, key)) = self.last_geom else { return };
        let px = |p: DVec3| {
            let d = p - o;
            pos2((d.dot(u) / dx as f64) as f32 + 0.5, (d.dot(v) / dy as f64) as f32 + 0.5)
        };
        self.measurements.push(Measurement { kind: MeasureKind::Length, pts: vec![px(a), px(b)], key, text: String::new() });
    }

    /// Adds an arrow pointing at patient point `p`, labelled `text` (scripted annotations).
    pub fn add_arrow_at(&mut self, p: DVec3, text: &str) {
        let Some((o, u, v, dx, dy, key)) = self.last_geom else { return };
        let d = p - o;
        let tip = pos2((d.dot(u) / dx as f64) as f32 + 0.5, (d.dot(v) / dy as f64) as f32 + 0.5);
        let tail = tip + vec2(-25.0 / dx, -25.0 / dy);
        self.measurements.push(Measurement { kind: MeasureKind::Arrow, pts: vec![tip, tail], key, text: text.to_string() });
    }

    /// Current zoom relative to "fit to window" (`None` = fit).
    pub fn zoom_factor(&self) -> Option<f32> {
        self.zoom.filter(|_| self.last_fit > 0.0).map(|z| z / self.last_fit)
    }

    /// Zooms around the viewport center (keyboard and buttons); `factor` > 1 enlarges.
    pub fn zoom_by(&mut self, factor: f32) {
        match self.content {
            Content::Empty => {}
            Content::Volume(_) => self.v3d.camera.zoom(1.0 / factor),
            Content::Stack(_) | Content::Mpr(..) | Content::Cpr(..) => {
                let fit = self.last_fit;
                if fit <= 0.0 {
                    return;
                }
                let s = self.zoom.unwrap_or(fit);
                let new = (s * factor).clamp(fit * 0.1, fit * 40.0);
                self.pan *= new / s;
                self.zoom = Some(new);
            }
        }
    }

    pub fn slice_count(&self) -> usize {
        if let Some(g) = &self.link_grid {
            return g.slices;
        }
        match &self.content {
            Content::Stack(s) => s.len(),
            Content::Mpr(s, kind) => s
                .volume
                .as_ref()
                .map(|v| match &self.grid {
                    Some((id, k, _, g)) if *id == v.id && k == kind => g.slices,
                    _ => PlaneGrid::standard(v, *kind).slices,
                })
                .unwrap_or(0),
            _ => 0,
        }
    }

    /// Moves `delta` slices (stack) or slice steps along the plane normal (MPR).
    pub fn step(&mut self, delta: i32, mpr: Option<&mut MprShared>) {
        match &self.content {
            Content::Stack(s) => {
                let n = s.len() as i32;
                if n > 0 {
                    self.slice = (self.slice as i32 + delta).clamp(0, n - 1) as usize;
                }
            }
            Content::Mpr(s, kind) => {
                if let (Some(v), Some(m)) = (s.volume.clone(), mpr) {
                    let g = match &self.link_grid {
                        Some(g) => g.clone(),
                        None => self.grid_for(&v, *kind, m.axes[kind.index()]),
                    };
                    let k = g.locate(m.point).z.round() + delta as f64;
                    let k = k.clamp(0.0, (g.slices - 1) as f64);
                    let cur = g.locate(m.point);
                    m.point = g.point(cur.x, cur.y, k);
                }
            }
            _ => {}
        }
    }

    pub fn jump_to(&mut self, first: bool, mpr: Option<&mut MprShared>) {
        let n = self.slice_count() as i32;
        let target = if first { 0 } else { n - 1 };
        match &self.content {
            Content::Stack(_) => self.slice = target.max(0) as usize,
            Content::Mpr(..) => {
                let cur = self.slice as i32;
                self.step(target - cur, mpr);
            }
            _ => {}
        }
    }

    fn grid_for(&mut self, vol: &crate::imaging::volume::Volume, kind: PlaneKind, axes: (DVec3, DVec3)) -> PlaneGrid {
        if let Some((id, k, a, g)) = &self.grid
            && *id == vol.id && *k == kind && *a == axes {
                return g.clone();
            }
        let g = PlaneGrid::from_axes(vol, axes.0, axes.1);
        self.grid = Some((vol.id, kind, axes, g.clone()));
        g
    }

    pub fn show(&mut self, ui: &mut Ui, rect: Rect, cx: &mut ViewCtx) -> ViewEvents {
        let mut ev = ViewEvents::default();
        let resp = ui.interact(rect, Id::unique(("viewport", self.id)), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::ZERO, Color32::BLACK);

        if let Some(p) = resp.dnd_release_payload::<SeriesDrag>() {
            ev.dropped_series = Some(p.0.clone());
        }
        let drop_hover = resp.dnd_hover_payload::<SeriesDrag>().is_some();
        if resp.is_pointer_button_down_on() {
            ev.activated = true;
        }
        if resp.double_clicked() {
            ev.double_clicked = true;
        }

        let has_content = !matches!(self.content, Content::Empty);
        self.top_inset = if cx.closable && has_content { 26.0 } else { 0.0 };
        match self.content.clone() {
            Content::Empty => draw_empty(&painter, rect),
            Content::Volume(s) => self.show_3d(ui, &resp, &painter, rect, &s, cx, &mut ev),
            Content::Stack(s) => self.show_2d(ui, &resp, &painter, rect, &s, View2D::Stack, cx, &mut ev),
            Content::Mpr(s, kind) => self.show_2d(ui, &resp, &painter, rect, &s, View2D::Plane(kind), cx, &mut ev),
            Content::Cpr(s, c) => self.show_2d(ui, &resp, &painter, rect, &s, View2D::Cpr(c), cx, &mut ev),
        }
        if let Some((text, color)) = cx.tag.as_ref().filter(|_| matches!(self.content, Content::Empty | Content::Volume(_))) {
            let galley = painter.layout_no_wrap(text.clone(), font_bold(11.0), Color32::WHITE);
            let r = Rect::from_min_size(pos2(rect.center().x - galley.size().x * 0.5 - 9.0, rect.top() + 8.0), galley.size() + vec2(18.0, 6.0));
            painter.rect_filled(r, CornerRadius::same(7), with_alpha(*color, 215));
            painter.galley(r.min + vec2(9.0, 3.0), galley, Color32::WHITE);
        }

        // Close button (top right), shown while the pointer is over the viewport.
        if cx.closable && has_content && rect.width() > 120.0 && ui.rect_contains_pointer(rect) {
            let r = Rect::from_center_size(rect.right_top() + vec2(-20.0, 19.0), vec2(24.0, 24.0));
            let b = ui.interact(r, Id::unique(("close-view", self.id)), Sense::click());
            let hot = b.hovered();
            painter.circle_filled(r.center(), 12.0, if hot { DANGER } else { Color32::from_rgba_unmultiplied(20, 23, 30, 230) });
            painter.circle_stroke(r.center(), 12.0, Stroke::new(1.0, if hot { DANGER } else { BORDER_STRONG }));
            painter.text(r.center(), Align2::CENTER_CENTER, icons::X, font(13.0), Color32::WHITE);
            if hot {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if b.on_hover_text(tr!("Fechar (Ctrl+W)", "Close (Ctrl+W)")).clicked() {
                ev.close = true;
            }
        }

        if let Some((title, frac)) = &cx.loading {
            let c = rect.center();
            painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_black_alpha(150));
            spinner(&painter, c - vec2(0.0, 18.0), 16.0, cx.time, ACCENT);
            painter.text(c + vec2(0.0, 14.0), Align2::CENTER_CENTER, title, font(13.0), TEXT);
            progress_bar(&painter, Rect::from_center_size(c + vec2(0.0, 36.0), vec2(180.0, 4.0)), *frac);
            ui.ctx().request_repaint();
        }

        if cx.reading && !drop_hover {
            // Reading: no frame; the active view is marked by a small corner.
            if cx.active && cx.several {
                let (c, l) = (rect.left_top() + vec2(5.0, 5.0), 14.0);
                let s = Stroke::new(2.0, with_alpha(ACCENT, 220));
                painter.line_segment([c, c + vec2(l, 0.0)], s);
                painter.line_segment([c, c + vec2(0.0, l)], s);
            }
            return ev;
        }
        let stroke = if drop_hover {
            Stroke::new(2.0, ACCENT)
        } else if cx.active {
            Stroke::new(1.5, with_alpha(ACCENT, 200))
        } else {
            Stroke::new(1.0, BORDER)
        };
        painter.rect_stroke(rect, CornerRadius::ZERO, stroke, StrokeKind::Inside);
        ev
    }

    // ---------------------------------------------------------------- 2D

    #[allow(clippy::too_many_arguments)]
    fn show_2d(
        &mut self,
        ui: &mut Ui,
        resp: &Response,
        painter: &egui::Painter,
        rect: Rect,
        series: &Arc<LoadedSeries>,
        mode: View2D,
        cx: &mut ViewCtx,
        ev: &mut ViewEvents,
    ) {
        let plane = match &mode {
            View2D::Plane(k) => Some(*k),
            _ => None,
        };
        let stack = matches!(mode, View2D::Stack);
        let Some(mut frame) = self.current_frame(series, &mode, cx) else {
            painter.text(rect.center(), Align2::CENTER_CENTER, tr!("Sem imagem", "No image"), font(13.0), TEXT_FAINT);
            return;
        };
        self.apply_denoise(&mut frame);
        self.last_geom = frame.origin.map(|o| (o, frame.u, frame.v, frame.dx, frame.dy, frame.key));
        let w_mm = frame.w as f32 * frame.dx;
        let h_mm = frame.h as f32 * frame.dy;
        let (vw, vh) = if self.rot % 2 == 1 { (h_mm, w_mm) } else { (w_mm, h_mm) };
        let fit = (rect.width() / vw).min(rect.height() / vh) * 0.96;
        self.last_fit = fit;
        if let Some(f) = self.zoom_request.take() {
            self.zoom = Some(fit * f);
        }
        let mut xf = Xf {
            c: rect.center() + self.pan,
            s: self.zoom.unwrap_or(fit),
            dx: frame.dx,
            dy: frame.dy,
            w_mm,
            h_mm,
            rot: self.rot,
            fh: self.flip_h,
            fv: self.flip_v,
        };
        if let Some(p) = self.center_on.take()
            && let Some((img, _)) = frame.image_of(p) {
                let s = xf.to_screen(img);
                if !rect.shrink(rect.size().min_elem() * 0.15).contains(s) {
                    let shift = rect.center() - s;
                    self.pan += shift;
                    xf.c += shift;
                }
            }

        // ---- interaction
        let hover = resp.hover_pos().filter(|p| rect.contains(*p));
        // Zoom keeping the image point under `anchor` in place.
        let zoom_at = |this: &mut Self, xf: &mut Xf, anchor: Pos2, factor: f32| {
            let before = xf.to_image(anchor);
            xf.s = (xf.s * factor).clamp(fit * 0.1, fit * 40.0);
            let shift = anchor - xf.to_screen(before);
            this.pan += shift;
            xf.c += shift;
            this.zoom = Some(xf.s);
        };
        let (wheel, mods) = if resp.contains_pointer() { wheel_steps(ui) } else { (0.0, Modifiers::NONE) };
        if wheel != 0.0 {
            if mods.ctrl {
                zoom_at(self, &mut xf, hover.unwrap_or(rect.center()), (wheel * 0.12).exp());
            } else if matches!(mode, View2D::Cpr(_)) {
                // No slices here: the wheel turns the straightened image about its path.
                self.wheel_acc += wheel;
                let steps = self.wheel_acc.trunc();
                self.wheel_acc -= steps;
                ev.cpr_rotate += steps * 10.0;
            } else {
                self.wheel_acc += wheel;
                let steps = self.wheel_acc.trunc();
                self.wheel_acc -= steps;
                if steps != 0.0 {
                    self.step(-steps as i32, cx.mpr.as_deref_mut());
                }
            }
        }

        let delta = resp.drag_delta();
        let primary = resp.dragged_by(PointerButton::Primary);
        let secondary = resp.dragged_by(PointerButton::Secondary);
        let middle = resp.dragged_by(PointerButton::Middle);
        let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.ctrl));
        // Ctrl + left drag zooms with any tool (for mice without a wheel).
        let tool_drag = primary && !shift && !ctrl;
        let wl_drag = secondary || (tool_drag && cx.tool == Tool::WindowLevel);
        if wl_drag && delta != Vec2::ZERO {
            let k = self.window.width.max(20.0) / 250.0;
            self.window.width = (self.window.width + delta.x * k).max(1.0);
            self.window.center += delta.y * k;
        }
        if middle || (primary && (cx.tool == Tool::Pan || shift)) {
            self.pan += delta;
            xf.c += delta;
        }
        if primary && !shift && (ctrl || cx.tool == Tool::Zoom) && delta.y != 0.0 {
            let anchor = ui.input(|i| i.pointer.press_origin()).unwrap_or(rect.center());
            zoom_at(self, &mut xf, anchor, (-delta.y * 0.01).exp());
        }
        if tool_drag && cx.tool == Tool::Scroll {
            self.wheel_acc += -delta.y / 6.0;
            let steps = self.wheel_acc.trunc();
            self.wheel_acc -= steps;
            if steps != 0.0 {
                self.step(steps as i32, cx.mpr.as_deref_mut());
            }
        }

        // Ctrl+click (any tool) or the Cursor 3D tool: same point on every series.
        if let Some(p) = resp.interact_pointer_pos().map(|p| xf.to_image(p)) {
            let locate = (ctrl && !shift && resp.clicked()) || (cx.tool == Tool::Locate && !shift && !ctrl && (resp.clicked() || primary));
            if locate {
                ev.cursor = Some(frame.hit(p, self.slice));
            }
            if !shift && !ctrl && resp.clicked_by(PointerButton::Primary) {
                match cx.tool {
                    Tool::Bookmark => ev.bookmark = Some(frame.hit(p, self.slice)),
                    Tool::Landmark => ev.landmark = Some(frame.hit(p, self.slice)),
                    Tool::Curve if plane.is_some() || (stack && frame.origin.is_some()) => ev.curve_point = Some(frame.hit(p, self.slice)),
                    _ => {}
                }
            }
        }
        if cx.tool == Tool::Curve && resp.clicked_by(PointerButton::Secondary) {
            ev.curve_finish = true;
        }

        // Right click without dragging (dragging adjusts the window): what can be done at that point.
        if cx.tool != Tool::Curve {
            if resp.secondary_clicked()
                && let Some(p) = resp.interact_pointer_pos() {
                    self.context_hit = Some(frame.hit(xf.to_image(p), self.slice));
                }
            let mut menu = egui::Popup::context_menu(resp);
            if cx.open_context {
                self.context_hit = Some(frame.hit(xf.to_image(rect.center()), self.slice));
                menu = menu.open_memory(egui::SetOpenCommand::Bool(true));
            }
            let hit = self.context_hit;
            let can_reconstruct = stack && series.volume.is_some();
            let mut probe = None;
            menu.show(|ui| {
                ui.set_min_width(260.0);
                let item = |ui: &mut Ui, icon: &str, text: &str, keys: &str| ui.add(egui::Button::new(format!("{icon}  {text}")).shortcut_text(keys)).clicked();
                if let Some(h) = hit {
                    if h.patient.is_some() && item(ui, icons::CROSSHAIR_SIMPLE, tr!("Cursor 3D aqui", "3D cursor here"), tr!("Ctrl+clique", "Ctrl+click")) {
                        ev.cursor = Some(h);
                        ui.close();
                    }
                    if item(ui, icons::MAP_PIN, tr!("Marcar achado aqui", "Mark finding here"), "B") {
                        ev.bookmark = Some(h);
                        ui.close();
                    }
                    if item(ui, icons::TARGET, tr!("Densidade neste ponto", "Density at this point"), "H") {
                        probe = Some(h.pixel);
                        ui.close();
                    }
                    ui.separator();
                }
                let tools = [
                    (Tool::Length, tr!("Medir distância", "Measure distance")),
                    (Tool::Ellipse, tr!("Medir ROI elíptica", "Measure elliptical ROI")),
                    (Tool::Arrow, tr!("Seta com texto", "Arrow with text")),
                ];
                for (t, text) in tools {
                    if item(ui, t.icon(), text, t.key()) {
                        ev.context = Some(ContextAction::Tool(t));
                        ui.close();
                    }
                }
                ui.separator();
                if can_reconstruct {
                    if item(ui, icons::SQUARES_FOUR, tr!("Abrir em MPR", "Open in MPR"), "") {
                        ev.context = Some(ContextAction::OpenMpr);
                        ui.close();
                    }
                    if item(ui, icons::CUBE, tr!("Abrir em 3D", "Open in 3D"), "") {
                        ev.context = Some(ContextAction::OpenVolume);
                        ui.close();
                    }
                }
                if item(ui, icons::COPY, tr!("Copiar imagem", "Copy image"), "Ctrl+C") {
                    ev.context = Some(ContextAction::Copy);
                    ui.close();
                }
                if item(ui, icons::CAMERA, tr!("Salvar imagem (PNG)", "Save image (PNG)"), "") {
                    ev.context = Some(ContextAction::Save);
                    ui.close();
                }
                ui.separator();
                if item(ui, icons::ARROW_COUNTER_CLOCKWISE, tr!("Restaurar visualização", "Reset view"), "R") {
                    ev.context = Some(ContextAction::ResetView);
                    ui.close();
                }
                if item(ui, icons::ARROWS_CLOCKWISE, tr!("Restaurar tudo", "Reset all"), "Shift+R") {
                    ev.context = Some(ContextAction::ResetAll);
                    ui.close();
                }
                if item(ui, icons::ARROWS_OUT_CARDINAL, tr!("Maximizar / voltar ao layout", "Maximize / back to layout"), tr!("duplo clique", "double-click")) {
                    ev.context = Some(ContextAction::Maximize);
                    ui.close();
                }
            });
            if let Some(px) = probe {
                self.measurements.push(Measurement { kind: MeasureKind::Probe, pts: vec![px], key: frame.key, text: String::new() });
            }
        }

        // Crosshair: click/drag moves the MPR point; dragging a handle tilts the other planes.
        let mut handle_hot = false;
        if cx.tool == Tool::Crosshair && !shift && !ctrl
            && let (Some(kind), Some(m)) = (plane, cx.mpr.as_deref_mut()) {
                handle_hot = hover.is_some_and(|h| near_handle(rect, &xf, &frame, kind, m, h));
                if resp.drag_started_by(PointerButton::Primary) {
                    self.rotating = ui.input(|i| i.pointer.press_origin()).is_some_and(|o| near_handle(rect, &xf, &frame, kind, m, o));
                }
                if self.rotating && primary {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let n = frame.normal().normalize();
                        let in_plane = |s: Pos2| frame.patient(xf.to_image(s)).map(|q| (q - m.point) - n * (q - m.point).dot(n));
                        if let (Some(a), Some(b)) = (in_plane(pos - delta), in_plane(pos)) {
                            let angle = a.cross(b).dot(n).atan2(a.dot(b));
                            let q = glam::DQuat::from_axis_angle(n, angle);
                            for other in PlaneKind::ALL.into_iter().filter(|k| *k != kind) {
                                let (u, v) = m.axes[other.index()];
                                m.axes[other.index()] = ((q * u).normalize(), (q * v).normalize());
                            }
                        }
                    }
                } else if (primary || resp.clicked())
                    && let Some(pt) = resp.interact_pointer_pos().and_then(|p| frame.patient(xf.to_image(p))) {
                        m.point = pt;
                    }
            }
        if !primary {
            self.rotating = false;
        }
        if !shift && !ctrl {
            self.handle_measure(resp, &xf, &frame, cx.tool);
        }

        // ---- image
        self.update_texture(ui, &frame, cx.smooth);
        self.update_overlay(ui, &frame);
        if let Some(tex) = &self.tex {
            painter.add(Shape::mesh(image_mesh(tex.id(), &xf, frame.w, frame.h, Color32::WHITE)));
        }
        let overlay = self.overlay_tex.as_ref().filter(|_| self.overlay_on).map(|t| t.id());
        if let Some(tex) = overlay {
            painter.add(Shape::mesh(image_mesh(tex, &xf, frame.w, frame.h, Color32::WHITE)));
        }
        if let Some(layer) = &cx.fusion {
            draw_fusion(ui, painter, rect, &xf, layer, self.id, ev);
        }

        if let Some(r) = cx.reference.as_ref().filter(|r| stack && r.study_uid == series.study.uid && r.series != Arc::as_ptr(series) as usize) {
            draw_reference_line(painter, &xf, &frame, r);
        }
        if let (Some(kind), Some(m)) = (plane, cx.mpr.as_deref())
            && m.crosshair {
                draw_crosshair(painter, rect, &xf, &frame, kind, m, hover, self.rotating);
            }
        if let Some(curve) = cx.curve {
            draw_curve(painter, &xf, &frame, curve);
            if cx.tool == Tool::Curve && cx.active {
                let text = match curve.len() {
                    0 => tr!("Rastreador: clique no início da estrutura", "Tracer: click at the start of the structure").to_string(),
                    1 => tr!(
                        "Clique no próximo ponto (role os cortes para seguir a estrutura)",
                        "Click the next point (scroll the slices to follow the structure)"
                    )
                    .to_string(),
                    n => trf!("{n} pontos · continue clicando · botão direito ou Enter conclui", "{n} points · keep clicking · right-click or Enter finishes"),
                };
                let galley = painter.layout_no_wrap(text, font_bold(12.0), Color32::WHITE);
                let r = Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 40.0), galley.size() + vec2(24.0, 12.0));
                painter.rect_filled(r, CornerRadius::same(8), with_alpha(CURVE, 225));
                painter.galley(r.min + vec2(12.0, 6.0), galley, Color32::WHITE);
            }
        }
        draw_pins(painter, &xf, &frame, &cx.pins, self.slice);
        if let Some(p) = cx.cursor.filter(|_| plane.is_none()) {
            draw_cursor(painter, &xf, &frame, p);
        }
        let ct = series.info.modality == "CT";
        self.draw_measurements(painter, &xf, &frame, ct);
        self.values = measure_values(&self.measurements, &frame);
        self.arrow_text_editor(ui, &xf, rect);

        // ---- cine
        if self.cine && stack && series.len() > 1 && cx.linked.is_none() {
            let dt = ui.input(|i| i.stable_dt) as f64;
            self.cine_acc += dt * self.cine_fps as f64;
            let adv = self.cine_acc.floor() as usize;
            if adv > 0 {
                self.cine_acc -= adv as f64;
                self.slice = (self.slice + adv) % series.len();
            }
            ui.ctx().request_repaint();
        }

        // ---- overlays
        let hu = hover.and_then(|h| {
            let p = xf.to_image(h);
            frame.real_at(p).map(|v| (p, v))
        });
        if let Some((p, v)) = hu {
            ev.status = Some(trf!("x {:.0}  y {:.0}   valor {}", "x {:.0}  y {:.0}   value {}", p.x.floor(), p.y.floor(), crate::i18n::num(v as f64, 1)));
        }
        use crate::config::InfoLevel;
        let level = if cx.reading { cx.info } else if cx.overlays { InfoLevel::Full } else { InfoLevel::None };
        let info_top = match level {
            InfoLevel::Full => self.draw_overlays(painter, rect, series, &frame, &xf, &mode, hu.map(|h| h.1), cx.mpr.as_deref(), cx.tag.is_some()),
            InfoLevel::Minimal => self.draw_overlays_min(painter, rect, series, &frame, &xf, &mode, hu.map(|h| h.1)),
            InfoLevel::None => {
                if cx.reading {
                    let pad_r = if self.slice_count() > 1 { 24.0 } else { 10.0 };
                    draw_orientation(painter, rect, &frame, &xf, 10.0, pad_r, with_alpha(WARNING, 200));
                }
                rect.bottom() - 6.0
            }
        };
        draw_change_chips(ui, painter, rect, info_top, self.id, &cx.changes, ev);
        let axial = series.images.get(self.slice).and_then(|im| im.orientation).is_some_and(|(r, c)| r.cross(c).z.abs() > 0.8);
        if stack && axial && cx.localizer && rect.width() > 480.0 && rect.height() > 380.0 {
            self.draw_localizer(ui, painter, rect, series, ev);
        }
        // Reference densities while measuring them (CT).
        if cx.tool == Tool::Probe && cx.active && ct {
            let last_probe = self.values.iter().rev().find(|v| v.kind == MeasureKind::Probe).map(|v| v.value);
            let pad_r = if self.slice_count() > 1 { 24.0 } else { 10.0 };
            draw_tissue_legend(painter, rect, hu.map(|h| h.1).or(last_probe), rect.top() + 70.0 + self.top_inset, rect.right() - pad_r);
        }
        // In the reading full screen the short text already names the plane.
        let chip = match &mode {
            _ if cx.reading && level != InfoLevel::Full => None,
            View2D::Plane(kind) => {
                let oblique = cx.mpr.as_deref().is_some_and(|m| {
                    let ((u, v), (su, sv)) = (m.axes[kind.index()], kind.axes());
                    u.dot(su) < 0.9999 || v.dot(sv) < 0.9999
                });
                let label = if oblique { trf!("{} OBLÍQUO", "{} OBLIQUE", kind.label()) } else { kind.label().to_string() };
                Some(plane_chip(painter, rect, &label, kind.color()))
            }
            View2D::Cpr(c) => Some(plane_chip(painter, rect, &format!("CPR · {:.0} mm", c.length_mm), CURVE)),
            View2D::Stack => None,
        };
        // Exam label (comparison), next to the plane chip.
        if let Some((text, color)) = &cx.tag {
            let galley = painter.layout_no_wrap(text.clone(), font_bold(11.0), Color32::WHITE);
            let x = chip.map(|c| c.right() + 6.0).unwrap_or(rect.left() + 10.0);
            let r = Rect::from_min_size(pos2(x, rect.top() + 10.0), galley.size() + vec2(18.0, 8.0));
            painter.rect_filled(r, CornerRadius::same(6), with_alpha(*color, 225));
            painter.galley(r.min + vec2(9.0, 4.0), galley, Color32::WHITE);
        }
        if let (Some(h), Some(tex)) = (hover.filter(|_| cx.tool == Tool::Magnifier), &self.tex) {
            let unit = if series.info.modality == "CT" { " HU" } else { "" };
            draw_magnifier(painter, rect, &xf, tex.id(), overlay, [frame.w, frame.h], h, cx.magnifier, hu.map(|v| format!("{:.0}{unit}", v.1)));
        }
        if hover.is_some() {
            let icon = match cx.tool {
                _ if self.rotating => egui::CursorIcon::Grabbing,
                _ if handle_hot => egui::CursorIcon::Grab,
                Tool::Pan => egui::CursorIcon::Grab,
                Tool::Zoom => egui::CursorIcon::ZoomIn,
                Tool::Scroll => egui::CursorIcon::ResizeVertical,
                Tool::Magnifier => egui::CursorIcon::None,
                Tool::Bookmark | Tool::Landmark | Tool::Curve => egui::CursorIcon::PointingHand,
                _ => egui::CursorIcon::Crosshair,
            };
            ui.ctx().set_cursor_icon(icon);
            // The active tool next to the pointer, so it is clear what a click will do.
            if let Some(h) = hover.filter(|_| !self.rotating && !handle_hot && !matches!(cx.tool, Tool::Pan | Tool::Zoom | Tool::Scroll | Tool::Magnifier)) {
                shadow_text(painter, h + vec2(13.0, 11.0), Align2::LEFT_TOP, cx.tool.icon(), font(15.0), Color32::WHITE);
            }
        }
        self.slice_scrollbar(ui, painter, rect, cx, ev);
    }

    /// Vertical scrollbar on the right edge: click or drag to jump to any slice.
    /// Over the bar, a small image of the slice under the pointer shows where a
    /// click would go.
    fn slice_scrollbar(&mut self, ui: &mut Ui, painter: &egui::Painter, rect: Rect, cx: &mut ViewCtx, ev: &mut ViewEvents) {
        let n = self.slice_count();
        if n < 2 || rect.height() < 120.0 || matches!(cx.linked, Some(Linked::Image { .. })) {
            return;
        }
        let track = Rect::from_min_max(pos2(rect.right() - 15.0, rect.top() + 44.0), pos2(rect.right() - 5.0, rect.bottom() - 44.0));
        let resp = ui.interact(track.expand2(vec2(3.0, 6.0)), Id::unique(("slice-scrollbar", self.id)), Sense::click_and_drag());
        let thumb_h = (track.height() / n as f32).clamp(18.0, 48.0);
        let y_of = |k: usize| track.top() + thumb_h * 0.5 + k.min(n - 1) as f32 / (n - 1) as f32 * (track.height() - thumb_h);
        let markers = self.slice_markers(cx);
        // Slice under the pointer; a mark within a few points wins, so its image is exact.
        let slice_at = |y: f32| {
            let t = ((y - track.top()) / track.height()).clamp(0.0, 1.0);
            markers
                .iter()
                .filter(|m| (y_of(m.0) - y).abs() < 5.0)
                .min_by(|a, b| (y_of(a.0) - y).abs().total_cmp(&(y_of(b.0) - y).abs()))
                .map_or((t * (n - 1) as f32).round() as usize, |m| m.0)
        };
        let pressed = resp.is_pointer_button_down_on();
        if pressed {
            ev.activated = true;
            self.jump_start.get_or_insert(self.slice);
            if let Some(p) = resp.interact_pointer_pos() {
                let target = slice_at(p.y) as i32;
                if target != self.slice as i32 {
                    self.cine = false;
                    self.step(target - self.slice as i32, cx.mpr.as_deref_mut());
                }
            }
        } else if resp.drag_stopped() || resp.clicked() {
            self.finish_jump(ev);
        }
        let busy = resp.hovered() || resp.dragged();
        let preview = if pressed || resp.dragged() { None } else { resp.hover_pos().filter(|_| resp.hovered()).map(|p| slice_at(p.y)) }
            .or(self.force_preview.map(|k| k.min(n - 1)));
        // Reading full screen: a hairline until the pointer comes close or the controls show.
        let slim = cx.reading && !busy && !cx.chrome && preview.is_none();
        let bar = if slim { Rect::from_min_max(pos2(track.right() - 3.0, track.top()), track.max) } else { track };
        // Dark backing keeps the bar visible over bright image regions.
        painter.rect_filled(bar, CornerRadius::same(5), Color32::from_rgba_unmultiplied(10, 12, 16, if busy { 220 } else { 150 }));
        if !slim {
            painter.rect_stroke(bar, CornerRadius::same(5), Stroke::new(1.0, Color32::from_white_alpha(if busy { 40 } else { 18 })), StrokeKind::Inside);
        }
        let reach = if slim { 1.0 } else { 3.0 };
        for (k, color, _) in &markers {
            let y = y_of(*k);
            painter.line_segment([pos2(bar.left() - reach, y), pos2(bar.right() + reach, y)], Stroke::new(if slim { 2.0 } else { 2.5 }, *color));
        }
        let t = self.slice.min(n - 1) as f32 / (n - 1) as f32;
        let thumb = Rect::from_center_size(
            pos2(bar.center().x, bar.top() + thumb_h * 0.5 + t * (bar.height() - thumb_h)),
            vec2((bar.width() - 4.0).max(3.0), thumb_h - 4.0),
        );
        let color = if resp.dragged() {
            ACCENT
        } else if busy {
            Color32::from_white_alpha(230)
        } else {
            Color32::from_white_alpha(150)
        };
        painter.rect_filled(thumb, CornerRadius::same(5), color);
        if busy {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if let Some(k) = preview {
            let y = y_of(k);
            painter.line_segment([pos2(track.left() - 4.0, y), pos2(track.right() + 4.0, y)], Stroke::new(2.0, Color32::WHITE));
            let marks: Vec<(Color32, String)> = markers.iter().filter(|m| m.0 == k).map(|m| (m.1, m.2.clone())).collect();
            self.draw_slice_preview(ui, painter, rect, track.left() - 10.0, y, k, n, &marks);
        } else if busy {
            let label = format!("{} / {}", self.slice.min(n - 1) + 1, n);
            let galley = painter.layout_no_wrap(label, font_bold(11.5), TEXT);
            let r = Rect::from_min_size(
                pos2(thumb.left() - galley.size().x - 18.0, thumb.center().y - galley.size().y * 0.5 - 3.0),
                galley.size() + vec2(10.0, 6.0),
            );
            painter.rect_filled(r, CornerRadius::same(5), Color32::from_rgba_unmultiplied(12, 14, 19, 230));
            painter.galley(r.min + vec2(5.0, 3.0), galley, TEXT);
        }
    }

    /// Box left of the slice bar with slice `k` as it would look (same window,
    /// rotation and flips), its number, position and the marks on it.
    #[allow(clippy::too_many_arguments)]
    fn draw_slice_preview(&mut self, ui: &Ui, painter: &egui::Painter, rect: Rect, right: f32, y: f32, k: usize, n: usize, marks: &[(Color32, String)]) {
        let plane = matches!(self.content, Content::Mpr(..));
        let image = self.slice_preview(ui, k);
        let side = (rect.height() * 0.3).clamp(80.0, 150.0).min(rect.width() * 0.4);
        let what = if plane { tr!("Corte", "Slice") } else { tr!("Imagem", "Image") };
        let mut lines = vec![(painter.layout_no_wrap(format!("{what} {} / {n}", k + 1), font_bold(12.0), TEXT), TEXT)];
        if let Some(mm) = self.slice_position(k) {
            lines.push((painter.layout_no_wrap(crate::i18n::dec(format!("{mm:.1} mm")), font(11.5), OVERLAY_TEXT), OVERLAY_TEXT));
        }
        for (color, label) in marks.iter().take(3) {
            lines.push((painter.layout_no_wrap(format!("● {label}"), font(11.5), *color), *color));
        }
        // Image size on screen, as the view shows it (a quarter turn swaps the sides).
        let shown = image.map(|(tex, size, (dx, dy))| {
            let (w_mm, h_mm) = (size[0] as f32 * dx, size[1] as f32 * dy);
            let (vw, vh) = if self.rot % 2 == 1 { (h_mm, w_mm) } else { (w_mm, h_mm) };
            let s = side / vw.max(vh).max(1e-3);
            (tex, size, (dx, dy), (w_mm, h_mm), s, vec2(vw * s, vh * s))
        });
        let text_w = lines.iter().map(|l| l.0.size().x).fold(0.0, f32::max);
        let text_h: f32 = lines.iter().map(|l| l.0.size().y + 1.0).sum();
        let img_h = shown.map_or(0.0, |s| s.5.y + 6.0);
        let size = vec2(shown.map_or(0.0, |s| s.5.x).max(text_w) + 12.0, img_h + text_h + 11.0);
        let left = (right - size.x).max(rect.left() + 4.0);
        let top = (y - size.y * 0.5).clamp(rect.top() + 4.0, (rect.bottom() - 4.0 - size.y).max(rect.top() + 4.0));
        let r = Rect::from_min_size(pos2(left, top), size);
        painter.rect_filled(r, CornerRadius::same(8), Color32::from_rgba_unmultiplied(14, 16, 22, 240));
        painter.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, Color32::from_white_alpha(45)), StrokeKind::Inside);
        let mut cy = r.top() + 6.0;
        if let Some((tex, px, (dx, dy), (w_mm, h_mm), s, on_screen)) = shown {
            let ir = Rect::from_min_size(pos2(r.center().x - on_screen.x * 0.5, cy), on_screen);
            painter.rect_filled(ir, CornerRadius::same(3), Color32::BLACK);
            let xf = Xf { c: ir.center(), s, dx, dy, w_mm, h_mm, rot: self.rot, fh: self.flip_h, fv: self.flip_v };
            painter.add(Shape::mesh(image_mesh(tex, &xf, px[0], px[1], Color32::WHITE)));
            cy += on_screen.y + 6.0;
        }
        for (g, color) in lines {
            let h = g.size().y;
            painter.galley(pos2(r.left() + 6.0, cy), g, color);
            cy += h + 1.0;
        }
    }

    /// Prepares the texture of a view that is drawn over another one (fusion)
    /// without drawing anything itself.
    pub fn overlay_texture(&mut self, ui: &Ui, cx: &ViewCtx) -> Option<(egui::TextureId, [usize; 2])> {
        let (series, mode) = match self.content.clone() {
            Content::Stack(s) => (s, View2D::Stack),
            Content::Mpr(s, k) => (s, View2D::Plane(k)),
            _ => return None,
        };
        let mut frame = self.current_frame(&series, &mode, cx)?;
        self.apply_denoise(&mut frame);
        self.update_texture(ui, &frame, cx.smooth);
        self.tex.as_ref().map(|t| (t.id(), [frame.w, frame.h]))
    }

    fn current_frame(&mut self, series: &Arc<LoadedSeries>, mode: &View2D, cx: &ViewCtx) -> Option<Frame> {
        match mode {
            View2D::Stack => self.stack_frame(series, cx),
            View2D::Plane(kind) => self.plane_frame(series, *kind, cx),
            View2D::Cpr(c) => {
                self.link_grid = None;
                Some(Frame {
                    w: c.width,
                    h: c.height,
                    src: FrameSrc::Buffer(c.data.clone(), c.quant),
                    src_key: c.id,
                    display: None,
                    display_key: 0,
                    dx: c.pixel as f32,
                    dy: c.pixel as f32,
                    spacing_known: true,
                    origin: None,
                    u: DVec3::X,
                    v: DVec3::Y,
                    key: c.id,
                    invert: false,
                    thickness: None,
                    cpr: Some(c.clone()),
                })
            }
        }
    }

    fn stack_frame(&mut self, series: &Arc<LoadedSeries>, cx: &ViewCtx) -> Option<Frame> {
        if series.len() == 0 {
            return None;
        }
        self.link_grid = None;
        // Exam B resampled onto exam A's image (2D comparison).
        if let Some(Linked::Image { size, spacing, map, slice, key, version }) = cx.linked.clone() {
            self.slice = slice.min(series.len() - 1);
            let info = &series.images[self.slice];
            if let Some(Slice::Gray(d, q)) = series.slice(self.slice) {
                let cache = hash_of((version, self.slice, key, size, series.uid.as_str()));
                if self.mpr_buf.as_ref().is_none_or(|b| b.0 != cache) {
                    let out = reslice::resample_2d(d, (info.width, info.height), info.spacing, size, spacing, &map);
                    self.mpr_buf = Some((cache, Arc::new(out), q));
                }
                let (cache, data, q) = self.mpr_buf.clone()?;
                return Some(Frame {
                    w: size.0,
                    h: size.1,
                    src: FrameSrc::Buffer(data, q),
                    src_key: cache,
                    display: None,
                    display_key: 0,
                    dx: spacing.0 as f32,
                    dy: spacing.1 as f32,
                    spacing_known: info.spacing_known,
                    origin: None,
                    u: DVec3::X,
                    v: DVec3::Y,
                    key,
                    invert: info.invert,
                    thickness: info.thickness,
                    cpr: None,
                });
            }
        }
        self.slice = self.slice.min(series.len() - 1);
        let info = &series.images[self.slice];
        let (u, v) = info.orientation.unwrap_or((DVec3::X, DVec3::Y));
        let mut src = FrameSrc::Stack(series.clone(), self.slice);
        let mut src_key = hash_of((series.uid.as_str(), self.slice));
        let mut thickness = info.thickness;
        if self.slab_mm > 0.0
            && let Some((cache, data, q)) = self.stack_slab(series) {
                src = FrameSrc::Buffer(data, q);
                src_key = cache;
                thickness = Some(self.slab_mm as f64);
            }
        Some(Frame {
            w: info.width,
            h: info.height,
            src,
            src_key,
            display: None,
            display_key: 0,
            dx: info.spacing.0 as f32,
            dy: info.spacing.1 as f32,
            spacing_known: info.spacing_known,
            origin: info.position.filter(|_| info.orientation.is_some()),
            u,
            v,
            key: self.slice as u64,
            invert: info.invert,
            thickness,
            cpr: None,
        })
    }

    /// Thick slab made from the native images around the current one (same
    /// size and orientation), combined with MIP, MinIP or average.
    fn stack_slab(&mut self, series: &LoadedSeries) -> Option<(u64, Arc<Vec<i16>>, Quant)> {
        let k = self.slice;
        let info = &series.images[k];
        let normal = info.orientation.map(|(r, c)| r.cross(c));
        let center = info.position.zip(normal).map(|(p, n)| p.dot(n));
        let half = self.slab_mm as f64 * 0.5;
        let members: Vec<usize> = (0..series.len())
            .filter(|&j| {
                let im = &series.images[j];
                if im.width != info.width || im.height != info.height || im.rgb {
                    return false;
                }
                match (center, normal, im.position) {
                    (Some(c), Some(n), Some(p)) => (p.dot(n) - c).abs() <= half + 1e-3,
                    _ => (j as f64 - k as f64).abs() * info.thickness.unwrap_or(1.0) <= half,
                }
            })
            .collect();
        if members.len() < 2 {
            return None;
        }
        let cache = hash_of((series.uid.as_str(), k, self.slab_mm.to_bits(), self.slab_mode as u8));
        if self.mpr_buf.as_ref().is_none_or(|b| b.0 != cache) {
            let Slice::Gray(_, q0) = series.slice(k)? else { return None };
            let slices: Vec<(&[i16], Quant)> = members
                .iter()
                .filter_map(|&j| match series.slice(j)? {
                    Slice::Gray(d, q) => Some((d, q)),
                    Slice::Rgb(_) => None,
                })
                .collect();
            let mode = self.slab_mode;
            let mut out = vec![OUTSIDE; info.width * info.height];
            out.par_chunks_mut(info.width).enumerate().for_each(|(y, row)| {
                for (x, px) in row.iter_mut().enumerate() {
                    let i = y * info.width + x;
                    let (mut acc, mut n) = (0.0f32, 0u32);
                    for (d, q) in &slices {
                        let s = d[i];
                        if s == OUTSIDE {
                            continue;
                        }
                        let v = q.real(s);
                        acc = match (n, mode) {
                            (0, _) => v,
                            (_, SlabMode::Mip) => acc.max(v),
                            (_, SlabMode::MinIp) => acc.min(v),
                            (_, SlabMode::Avg) => acc + v,
                        };
                        n += 1;
                    }
                    if n > 0 {
                        *px = q0.quantize(if mode == SlabMode::Avg { acc / n as f32 } else { acc });
                    }
                }
            });
            self.mpr_buf = Some((cache, Arc::new(out), q0));
        }
        self.mpr_buf.clone()
    }

    fn plane_frame(&mut self, series: &Arc<LoadedSeries>, kind: PlaneKind, cx: &ViewCtx) -> Option<Frame> {
        let vol = series.volume.clone()?;
        // Exam B in exam A's plane (comparison).
        let (g, k, slab_mm, slab_mode, map, key, version) = match cx.linked.clone() {
            Some(Linked::Plane { grid, k, slab_mm, slab_mode, map, key, version }) => {
                self.link_grid = Some(grid.clone());
                (grid, k, slab_mm, slab_mode, map, key, version)
            }
            _ => {
                self.link_grid = None;
                let m = cx.mpr.as_deref()?;
                let g = self.grid_for(&vol, kind, m.axes[kind.index()]);
                let k = g.locate(m.point).z.round().clamp(0.0, (g.slices - 1) as f64);
                (g, k, m.slab_mm, m.slab_mode, Rigid::IDENTITY, ((kind as u64) << 32) | k as u64, 0)
            }
        };
        self.slice = k as usize;
        let cache = hash_of((vol.id, version, grid_hash(&g), k.to_bits(), slab_mm.to_bits(), slab_mode as u8));
        if self.mpr_buf.as_ref().is_none_or(|b| b.0 != cache) {
            let mut buf = Vec::new();
            reslice::reslice_mapped(&vol, &g, k, slab_mm as f64, slab_mode, &map, &mut buf);
            self.mpr_buf = Some((cache, Arc::new(buf), vol.quant));
        }
        let data = self.mpr_buf.as_ref()?.1.clone();
        Some(Frame {
            w: g.width,
            h: g.height,
            src: FrameSrc::Buffer(data, vol.quant),
            src_key: cache,
            display: None,
            display_key: 0,
            dx: g.pixel as f32,
            dy: g.pixel as f32,
            spacing_known: true,
            origin: Some(g.point(0.0, 0.0, k)),
            u: g.u,
            v: g.v,
            key,
            invert: false,
            thickness: Some(if slab_mm > 0.0 { slab_mm as f64 } else { g.step }),
            cpr: None,
        })
    }

    /// Display-only noise filter; HU readings keep using the original pixels.
    fn apply_denoise(&mut self, frame: &mut Frame) {
        if self.denoise == 0 {
            return;
        }
        let Some(Slice::Gray(d, q)) = frame.slice() else { return };
        let level = self.denoise.min(3) as usize;
        // Strength follows the window width (in quarter-octave steps, so a
        // window/level drag does not refilter every frame).
        let wq = (self.window.width.max(1.0).log2() * 4.0).round();
        let key = hash_of((frame.src_key, frame.key, level, wq as i32));
        if self.filtered.as_ref().is_none_or(|f| f.0 != key) {
            let sigma_value = [0.0, 0.05, 0.08, 0.12][level] * 2f32.powf(wq / 4.0) / q.scale.abs().max(1e-6);
            let sigma_px = [0.0, 1.0, 1.4, 1.8][level];
            let out = crate::imaging::filter::bilateral(d, frame.w, frame.h, sigma_px, sigma_value.max(0.5));
            self.filtered = Some((key, Arc::new(out)));
        }
        if let Some((key, data)) = &self.filtered {
            frame.display = Some(data.clone());
            frame.display_key = *key;
        }
    }

    fn update_texture(&mut self, ui: &Ui, frame: &Frame, smooth: bool) {
        let invert = self.invert ^ frame.invert;
        let key = hash_of((
            frame.src_key,
            frame.key,
            frame.display_key,
            self.window.center.to_bits(),
            self.window.width.to_bits(),
            invert,
            self.colormap,
            smooth,
        ));
        if self.tex_key == Some(key) && self.tex.is_some() {
            return;
        }
        let Some(slice) = frame.slice() else { return };
        let mut pixels = Vec::new();
        match slice {
            Slice::Gray(d, q) => {
                let lk = (q.scale.to_bits(), q.offset.to_bits(), self.window.center.to_bits(), self.window.width.to_bits(), invert, self.colormap);
                if self.lut_key != Some(lk) {
                    lut::build_lut(q, self.window, invert, self.colormap, &mut self.lut);
                    self.lut_key = Some(lk);
                }
                let shown: &[i16] = frame.display.as_deref().map(|v| v.as_slice()).unwrap_or(d);
                lut::apply_lut(shown, &self.lut, &mut pixels);
            }
            Slice::Rgb(d) => lut::apply_rgb_window(d, self.window, invert, &mut pixels),
        }
        if pixels.len() != frame.w * frame.h {
            return;
        }
        let img = ColorImage::new([frame.w, frame.h], pixels);
        let opts = if smooth { TextureOptions::LINEAR } else { TextureOptions::NEAREST };
        match &mut self.tex {
            Some(t) => t.set(img, opts),
            None => self.tex = Some(ui.ctx().load_texture(format!("vp-{}", self.id), img, opts)),
        }
        self.tex_key = Some(key);
    }

    /// Color layer of the active highlights, rebuilt only when the image or the settings change.
    fn update_overlay(&mut self, ui: &Ui, frame: &Frame) {
        if !self.highlights.any() {
            self.overlay_on = false;
            return;
        }
        let h = self.highlights;
        let key = hash_of((frame.src_key, frame.key, h.gas, h.fat, h.dense.map(f32::to_bits)));
        if self.overlay_key == Some(key) {
            return;
        }
        self.overlay_key = Some(key);
        let Some(Slice::Gray(d, q)) = frame.slice() else {
            self.overlay_on = false;
            return;
        };
        match highlight::overlay(d, q, frame.w, frame.h, &h) {
            Some(px) => {
                let img = ColorImage::new([frame.w, frame.h], px);
                match &mut self.overlay_tex {
                    Some(t) => t.set(img, TextureOptions::NEAREST),
                    None => self.overlay_tex = Some(ui.ctx().load_texture(format!("vp-overlay-{}", self.id), img, TextureOptions::NEAREST)),
                }
                self.overlay_on = true;
            }
            None => self.overlay_on = false,
        }
    }

    fn handle_measure(&mut self, resp: &Response, xf: &Xf, frame: &Frame, tool: Tool) {
        let kind = match tool {
            Tool::Length => MeasureKind::Length,
            Tool::Angle => MeasureKind::Angle,
            Tool::Ellipse => MeasureKind::Ellipse,
            Tool::Probe => MeasureKind::Probe,
            Tool::Arrow => MeasureKind::Arrow,
            _ => {
                self.drawing = None;
                return;
            }
        };
        let pos = resp.interact_pointer_pos().or(resp.hover_pos()).map(|p| xf.to_image(p));
        let Some(p) = pos else { return };
        let new = |pts| Measurement { kind, pts, key: frame.key, text: String::new() };

        // Angle: the third point follows the pointer until the next click.
        if let Some(d) = &mut self.drawing
            && d.kind == MeasureKind::Angle && d.pts.len() == 3 {
                d.pts[2] = p;
                if resp.clicked() {
                    let done = self.drawing.take().unwrap();
                    self.measurements.push(done);
                }
                return;
            }
        if kind == MeasureKind::Probe {
            if resp.clicked_by(PointerButton::Primary) {
                self.measurements.push(new(vec![p]));
            }
            return;
        }
        if resp.drag_started_by(PointerButton::Primary) {
            self.drawing = Some(new(vec![p, p]));
        } else if resp.dragged_by(PointerButton::Primary) {
            if let Some(d) = &mut self.drawing {
                d.pts[1] = p;
            }
        } else if resp.drag_stopped_by(PointerButton::Primary)
            && let Some(mut d) = self.drawing.take() {
                let size = xf.to_screen(d.pts[0]).distance(xf.to_screen(d.pts[1]));
                if size < 3.0 {
                    return;
                }
                match d.kind {
                    MeasureKind::Angle => {
                        d.pts.push(p);
                        self.drawing = Some(d);
                    }
                    MeasureKind::Arrow => {
                        self.measurements.push(d);
                        self.editing = Some((self.measurements.len() - 1, true));
                    }
                    _ => self.measurements.push(d),
                }
            }
    }

    /// Text box for the arrow just drawn (Enter or a click elsewhere confirms).
    fn arrow_text_editor(&mut self, ui: &mut Ui, xf: &Xf, rect: Rect) {
        let Some((i, focus)) = self.editing else { return };
        let Some(m) = self.measurements.get_mut(i) else {
            self.editing = None;
            return;
        };
        let pos = (xf.to_screen(m.pts[1]) + vec2(8.0, -12.0)).clamp(rect.min, (rect.max - vec2(180.0, 30.0)).max(rect.min));
        let mut done = false;
        egui::Area::new(Id::unique(("arrow-text", self.id))).fixed_pos(pos).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(12, 14, 19, 235))
                .stroke(Stroke::new(1.0, MEASURE))
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    let hint = tr!("Texto da seta…", "Arrow text…");
                    let r = ui.add(egui::TextEdit::singleline(&mut m.text).hint_text(hint).desired_width(160.0).frame(egui::Frame::NONE));
                    if focus {
                        r.request_focus();
                    }
                    if r.lost_focus() {
                        done = true;
                    }
                });
        });
        self.editing = if done { None } else { Some((i, false)) };
    }

    fn draw_measurements(&self, painter: &egui::Painter, xf: &Xf, frame: &Frame, ct: bool) {
        let editing = self.editing.map(|e| e.0);
        let items = self
            .measurements
            .iter()
            .enumerate()
            .filter(|(_, m)| m.key == frame.key)
            .map(|(i, m)| (Some(i), m))
            .chain(self.drawing.iter().map(|m| (None, m)));
        for (index, m) in items {
            let pts: Vec<Pos2> = m.pts.iter().map(|p| xf.to_screen(*p)).collect();
            let stroke = Stroke::new(1.6, MEASURE);
            let label_pos;
            let label = match m.kind {
                MeasureKind::Length => {
                    painter.line_segment([pts[0], pts[1]], stroke);
                    label_pos = pts[1] + vec2(8.0, -8.0);
                    let mm = length_mm(m, frame);
                    let value = crate::i18n::dec(if frame.spacing_known { format!("{mm:.1} mm") } else { format!("{mm:.1} px") });
                    if m.text.is_empty() { value } else { format!("{}: {value}", m.text) }
                }
                MeasureKind::Angle => {
                    painter.line_segment([pts[0], pts[1]], stroke);
                    if pts.len() > 2 {
                        painter.line_segment([pts[1], pts[2]], stroke);
                        label_pos = pts[1] + vec2(10.0, 10.0);
                        crate::i18n::dec(format!("{:.1}°", angle_deg(m, frame)))
                    } else {
                        label_pos = pts[1];
                        String::new()
                    }
                }
                MeasureKind::Ellipse => {
                    let (center, ra, rb) = ellipse_axes(m);
                    let outline: Vec<Pos2> = (0..64)
                        .map(|i| {
                            let t = i as f32 / 64.0 * std::f32::consts::TAU;
                            xf.to_screen(pos2(center.x + ra * t.cos(), center.y + rb * t.sin()))
                        })
                        .collect();
                    painter.add(Shape::closed_line(outline.clone(), stroke));
                    label_pos = outline.iter().fold(pos2(f32::MIN, f32::MIN), |a, p| pos2(a.x.max(p.x), a.y.max(p.y))) + vec2(6.0, -40.0);
                    ellipse_stats(frame, center, ra, rb)
                }
                MeasureKind::Probe => {
                    painter.circle_stroke(pts[0], 4.0, stroke);
                    label_pos = pts[0] + vec2(8.0, -8.0);
                    match frame.real_at(m.pts[0]) {
                        // Reading prepared by the comparison (both phases).
                        _ if !m.text.is_empty() => m.text.clone(),
                        Some(v) if ct => format!("{v:.0} HU · {}", tissues::classify(v).name),
                        Some(v) => format!("{v:.0}"),
                        None => String::new(),
                    }
                }
                MeasureKind::Arrow => {
                    let (tip, tail) = (pts[0], pts[1]);
                    let s = Stroke::new(2.0, MEASURE);
                    painter.line_segment([tail, tip], s);
                    let dir = (tip - tail).normalized();
                    let side = vec2(-dir.y, dir.x);
                    painter.add(Shape::convex_polygon(vec![tip, tip - dir * 11.0 + side * 5.0, tip - dir * 11.0 - side * 5.0], MEASURE, Stroke::NONE));
                    label_pos = tail + vec2(if tail.x >= tip.x { 6.0 } else { -6.0 }, -8.0);
                    if index.is_some() && index == editing { String::new() } else { m.text.clone() }
                }
            };
            if m.kind != MeasureKind::Arrow {
                for p in &pts {
                    painter.circle_filled(*p, 2.5, MEASURE);
                }
            }
            if !label.is_empty() {
                let galley = painter.layout(label, font(12.0), MEASURE, 220.0);
                let left = m.kind == MeasureKind::Arrow && pts[1].x < pts[0].x;
                let at = if left { label_pos - vec2(galley.size().x, 0.0) } else { label_pos };
                let r = Rect::from_min_size(at, galley.size()).expand2(vec2(6.0, 3.0));
                painter.rect_filled(r, CornerRadius::same(5), Color32::from_black_alpha(170));
                painter.galley(at, galley, MEASURE);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_overlays(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        series: &LoadedSeries,
        frame: &Frame,
        xf: &Xf,
        mode: &View2D,
        hu: Option<f32>,
        mpr: Option<&MprShared>,
        chip_row: bool,
    ) -> f32 {
        let plane = matches!(mode, View2D::Plane(_));
        let st = &series.study;
        let pad = 10.0;
        // Keep right-aligned text clear of the slice scrollbar.
        let pad_r = if self.slice_count() > 1 { pad + 14.0 } else { pad };
        let line = 16.0;
        let f = font(12.0);
        let fb = font_bold(13.0);
        let tl = rect.left_top() + vec2(pad, pad + if matches!(mode, View2D::Stack) && !chip_row { 0.0 } else { 26.0 });
        shadow_text(painter, tl, Align2::LEFT_TOP, format_person_name(&st.patient_name), fb.clone(), Color32::WHITE);
        let mut y = tl.y + line + 2.0;
        let id_line = format!("ID {}", st.patient_id);
        shadow_text(painter, pos2(tl.x, y), Align2::LEFT_TOP, id_line, f.clone(), OVERLAY_TEXT);
        y += line;
        let birth = [format_date(&st.birth_date), st.sex.clone(), st.age.clone()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        if !birth.is_empty() {
            shadow_text(painter, pos2(tl.x, y), Align2::LEFT_TOP, birth, f.clone(), OVERLAY_TEXT);
        }

        let tr = rect.right_top() + vec2(-pad_r, pad + self.top_inset);
        // Lines that would run into the patient block (narrow views) are left out.
        let left_w = painter.layout_no_wrap(format_person_name(&st.patient_name), fb.clone(), Color32::WHITE).size().x;
        let mut y = tr.y;
        for t in [st.institution.clone(), st.description.clone(), format!("{}  {}", format_date(&st.date), crate::dicom::model::format_time(&st.time))] {
            let t = t.trim();
            if t.is_empty() || tr.x - painter.layout_no_wrap(t.to_string(), f.clone(), OVERLAY_TEXT).size().x < tl.x + left_w + 16.0 {
                continue;
            }
            shadow_text(painter, pos2(tr.x, y), Align2::RIGHT_TOP, t, f.clone(), OVERLAY_TEXT);
            y += line;
        }

        let bl = rect.left_bottom() + vec2(pad, -pad);
        let mut lines = vec![trf!("Série {}: {}", "Series {}: {}", series.info.number, series.info.title())];
        match mode {
            View2D::Cpr(c) => lines.push(trf!("Curva de {} pontos · {:.0} mm", "{}-point curve · {:.0} mm", c.centers.len().min(9999), c.length_mm)),
            _ => lines.push(format!(
                "{} {} / {}",
                if plane { tr!("Corte", "Slice") } else { tr!("Imagem", "Image") },
                self.slice + 1,
                if plane { self.slice_count() } else { series.len() }
            )),
        }
        if let Some(t) = frame.thickness {
            let slab = match mode {
                View2D::Plane(_) => mpr.filter(|m| m.slab_mm > 0.0).map(|m| format!(" ({})", m.slab_mode.label())).unwrap_or_default(),
                View2D::Stack if self.slab_mm > 0.0 && matches!(frame.src, FrameSrc::Buffer(..)) => format!(" ({})", self.slab_mode.label()),
                _ => String::new(),
            };
            let t = crate::i18n::num(t, 2);
            lines.push(trf!("Espessura {t} mm{slab}", "Thickness {t} mm{slab}"));
        }
        if self.denoise > 0 {
            let level = ["", tr!("leve", "light"), tr!("médio", "medium"), tr!("forte", "strong")][self.denoise.min(3) as usize];
            lines.push(trf!("Filtro de ruído: {level}", "Noise reduction: {level}"));
        }
        if self.highlights.any() {
            let h = self.highlights;
            let mut parts = Vec::new();
            if h.gas {
                parts.push(tr!("gás livre (ciano)", "free air (cyan)").to_string());
            }
            if let Some(t) = h.dense {
                parts.push(format!("≥ {t:.0} HU (magenta)"));
            }
            if h.fat {
                parts.push(tr!("gordura densificada (laranja)", "fat stranding (orange)").to_string());
            }
            lines.push(trf!("Realce: {}", "Highlight: {}", parts.join(" · ")));
        }
        if frame.cpr.is_none()
            && let Some(p) = frame.patient(pos2(frame.w as f32 * 0.5, frame.h as f32 * 0.5)) {
                lines.push(trf!("Posição {} mm", "Position {} mm", crate::i18n::num(p.dot(frame.normal()), 1)));
            }
        for (i, t) in lines.iter().rev().enumerate() {
            shadow_text(painter, bl - vec2(0.0, i as f32 * line), Align2::LEFT_BOTTOM, t, f.clone(), OVERLAY_TEXT);
        }
        let info_top = bl.y - lines.len() as f32 * line - 4.0;

        let br = rect.right_bottom() + vec2(-pad_r, -pad);
        let native = xf.s * frame.dx * painter.pixels_per_point();
        let mut lines = vec![
            format!("W {:.0}  L {:.0}", self.window.width, self.window.center),
            format!("Zoom {:.0}%", native * 100.0),
        ];
        if let Some(v) = hu {
            let unit = if series.info.modality == "CT" { " HU" } else { "" };
            lines.push(format!("{v:.0}{unit}"));
        }
        for (i, t) in lines.iter().rev().enumerate() {
            shadow_text(painter, br - vec2(0.0, i as f32 * line), Align2::RIGHT_BOTTOM, t, f.clone(), OVERLAY_TEXT);
        }

        draw_orientation(painter, rect, frame, xf, pad, pad_r, WARNING);

        // Scale bar.
        if frame.spacing_known {
            let target = 110.0 / xf.s;
            let mm = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0].into_iter().min_by(|a, b| (a - target).abs().total_cmp(&(b - target).abs())).unwrap_or(10.0);
            let len = mm * xf.s;
            let x0 = rect.right() - pad_r - 4.0;
            let bottom = rect.bottom() - 4.0 * line - pad;
            let top = bottom - len;
            if len < rect.height() * 0.45 && top > rect.center().y + 24.0 {
                let s = Stroke::new(1.2, Color32::from_white_alpha(170));
                painter.line_segment([pos2(x0, top), pos2(x0, bottom)], s);
                for i in 0..=10 {
                    let y = top + (bottom - top) * i as f32 / 10.0;
                    let w = if i % 5 == 0 { 7.0 } else { 4.0 };
                    painter.line_segment([pos2(x0 - w, y), pos2(x0, y)], s);
                }
                shadow_text(painter, pos2(x0 - 10.0, bottom), Align2::RIGHT_BOTTOM, format!("{mm:.0} mm"), font(11.0), OVERLAY_TEXT);
            }
        }
        info_top
    }

    /// Reading full screen, minimal text: plane and slice at the top left,
    /// window (and the value under the pointer) at the bottom right, and the
    /// orientation letters. Returns the top of the bottom text.
    #[allow(clippy::too_many_arguments)]
    fn draw_overlays_min(&self, painter: &egui::Painter, rect: Rect, series: &LoadedSeries, frame: &Frame, xf: &Xf, mode: &View2D, hu: Option<f32>) -> f32 {
        let pad = 10.0;
        let pad_r = if self.slice_count() > 1 { pad + 14.0 } else { pad };
        let f = font(12.0);
        let (name, color) = match mode {
            View2D::Plane(k) => (title_case(k.label()), k.color()),
            View2D::Cpr(_) => ("CPR".to_string(), CURVE),
            View2D::Stack => (stack_plane_name(frame).to_string(), OVERLAY_TEXT),
        };
        let text = match mode {
            View2D::Cpr(c) => format!("{name} · {:.0} mm", c.length_mm),
            View2D::Plane(_) => format!("{name} · {} / {}", self.slice + 1, self.slice_count()),
            View2D::Stack => format!("{name} · {} / {}", self.slice + 1, series.len()),
        };
        let tl = rect.left_top() + vec2(pad, pad);
        painter.circle_filled(tl + vec2(4.0, 8.0), 3.5, color);
        shadow_text(painter, tl + vec2(13.0, 0.0), Align2::LEFT_TOP, text, f.clone(), OVERLAY_TEXT);

        let mut lines = vec![format!("W {:.0}  L {:.0}", self.window.width, self.window.center)];
        if let Some(v) = hu {
            let unit = if series.info.modality == "CT" { " HU" } else { "" };
            lines.push(format!("{v:.0}{unit}"));
        }
        let br = rect.right_bottom() + vec2(-pad_r, -pad);
        for (i, t) in lines.iter().rev().enumerate() {
            shadow_text(painter, br - vec2(0.0, i as f32 * 16.0), Align2::RIGHT_BOTTOM, t, f.clone(), OVERLAY_TEXT);
        }
        draw_orientation(painter, rect, frame, xf, pad, pad_r, with_alpha(WARNING, 200));
        rect.bottom() - pad - 20.0
    }

    // ---------------------------------------------------------------- 3D

    #[allow(clippy::too_many_arguments)]
    fn show_3d(
        &mut self,
        ui: &mut Ui,
        resp: &Response,
        painter: &egui::Painter,
        rect: Rect,
        series: &Arc<LoadedSeries>,
        cx: &mut ViewCtx,
        ev: &mut ViewEvents,
    ) {
        self.v3d.refining = false;
        let Some(vol) = series.volume.clone() else {
            painter.text(rect.center(), Align2::CENTER_CENTER, tr!("Série sem volume 3D", "Series without a 3D volume"), font(13.0), TEXT_FAINT);
            return;
        };
        if let Some(issue) = &cx.gpu_issue {
            let c = rect.center();
            painter.text(c - vec2(0.0, 34.0), Align2::CENTER_CENTER, icons::CUBE, font(30.0), TEXT_FAINT);
            let text = tr!("Reconstrução 3D indisponível neste computador", "3D rendering unavailable on this computer");
            painter.text(c, Align2::CENTER_CENTER, text, font_bold(14.0), TEXT);
            let galley = painter.layout(issue.clone(), font(12.5), TEXT_DIM, (rect.width() - 60.0).clamp(120.0, 440.0));
            painter.galley(c + vec2(-galley.size().x * 0.5, 18.0), galley, TEXT_DIM);
            return;
        }
        let Some(gpu) = cx.gpu.clone().filter(|g| g.source_id == vol.id) else {
            if !cx.gpu_pending {
                ev.need_gpu = Some(series.clone());
            }
            spinner(painter, rect.center() - vec2(0.0, 12.0), 16.0, cx.time, ACCENT);
            painter.text(rect.center() + vec2(0.0, 20.0), Align2::CENTER_CENTER, tr!("Preparando volume 3D…", "Preparing 3D volume…"), font(13.0), TEXT_DIM);
            ui.ctx().request_repaint();
            return;
        };

        // ---- interaction
        let v = &mut self.v3d;
        let delta = resp.drag_delta();
        let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.ctrl));
        let mut interacting = false;
        if resp.dragged_by(PointerButton::Primary) && ctrl && !shift {
            // Ctrl + left drag: zoom (drag up = closer), for mice without a wheel.
            v.camera.zoom((delta.y * 0.01).exp());
            interacting = true;
        } else if resp.dragged_by(PointerButton::Primary) && !shift {
            v.camera.rotate(glam::Vec2::new(delta.x, delta.y));
            interacting = true;
        }
        if resp.dragged_by(PointerButton::Middle) || (resp.dragged_by(PointerButton::Primary) && shift) {
            v.camera.pan_by(glam::Vec2::new(delta.x, delta.y), rect.height());
            interacting = true;
        }
        if resp.dragged_by(PointerButton::Secondary) {
            match v.mode {
                RenderMode::Composite => {
                    v.adjust.shift += delta.x * 2.0;
                    v.opacity = (v.opacity * (-delta.y * 0.006).exp()).clamp(0.05, 4.0);
                }
                RenderMode::Iso => {
                    let iso = v.iso.get_or_insert(default_iso(series));
                    *iso += delta.x * 2.0;
                }
                _ => {
                    let w = v.mip_window.get_or_insert(series.default_window);
                    let k = w.width.max(20.0) / 250.0;
                    w.width = (w.width + delta.x * k).max(1.0);
                    w.center += delta.y * k;
                }
            }
            interacting = true;
        }
        let (wheel, _) = if resp.contains_pointer() { wheel_steps(ui) } else { (0.0, Modifiers::NONE) };
        if wheel != 0.0 {
            v.camera.zoom((-wheel * 0.1).exp());
            interacting = true;
        }
        if interacting {
            v.interacting_until = cx.time + 0.25;
        }
        let lod = cx.time < v.interacting_until;
        if lod {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
        }

        // ---- transfer function
        let range = (vol.real_range().0, vol.real_range().1);
        let key = TfKey { preset: v.preset, adjust: v.adjust, range };
        if v.tf.as_ref().is_none_or(|t| t.0 != key) {
            let preset = &tf::PRESETS[v.preset.min(tf::PRESETS.len() - 1)];
            let (table, lo, hi) = tf::bake(preset, v.adjust, range);
            v.tf = Some((key, Arc::new(table), (lo, hi), TF_VERSION.fetch_add(1, Ordering::Relaxed)));
        }
        let (_, table, tf_range, tf_version) = v.tf.clone().unwrap();
        let preset = &tf::PRESETS[v.preset.min(tf::PRESETS.len() - 1)];
        let window = v.mip_window.unwrap_or(series.default_window);

        let ppp = ui.ctx().pixels_per_point();
        let scale = if lod { 0.5 } else { 1.0 };
        let size_px = [
            ((rect.width() * ppp * scale) as u32).max(16),
            ((rect.height() * ppp * scale) as u32).max(16),
        ];
        let model = gpu.model_matrix();
        let mvp = v.camera.projection(rect.width() / rect.height()) * v.camera.view() * model;

        // Patient-axis crop -> texture-space crop.
        let mut cmin = Vec3::ZERO;
        let mut cmax = Vec3::ONE;
        for a in 0..3 {
            let ax = gpu.axes[a];
            let p = (0..3).max_by(|&i, &j| ax[i].abs().total_cmp(&ax[j].abs())).unwrap_or(a);
            let [lo, hi] = v.crop[p];
            if ax[p] >= 0.0 {
                cmin[a] = lo;
                cmax[a] = hi;
            } else {
                cmin[a] = 1.0 - hi;
                cmax[a] = 1.0 - lo;
            }
        }
        let params = RenderParams {
            inv_mvp: mvp.inverse(),
            crop_min: cmin,
            crop_max: cmax,
            mode: v.mode,
            range: match v.mode {
                RenderMode::Composite | RenderMode::Iso => tf_range,
                _ => (window.center - window.width * 0.5, window.center + window.width * 0.5),
            },
            iso: v.iso.unwrap_or_else(|| default_iso(series)),
            opacity: v.opacity,
            shading: v.shading,
            light: [preset.shading.ambient, preset.shading.diffuse, preset.shading.specular, preset.shading.shininess],
            samples_per_voxel: cx.quality.clamp(0.25, 4.0) * if lod { 0.6 } else { 1.0 },
            bg_top: [0.075, 0.09, 0.12],
            bg_bottom: [0.008, 0.01, 0.014],
            iso_color: [0.93, 0.87, 0.78],
            tf_version,
            lighting: cx.lighting,
        };
        // Cinematic: keep drawing while the renderer adds frames to the average.
        let cinematic = params.lighting() == crate::config::Lighting::Cinematic;
        let fresh = v.last_params.as_ref().is_none_or(|(p, s)| *p != params || *s != size_px);
        if fresh {
            v.progress.store(0, Ordering::Relaxed);
            v.last_params = Some((params.clone(), size_px));
            v.refine_start = if cinematic { cx.time } else { -1.0 };
        }
        let done = v.progress.load(Ordering::Relaxed);
        let refining = cinematic && !lod && done < ACCUM_FRAMES;
        if cinematic && done >= ACCUM_FRAMES && v.refine_start >= 0.0 {
            log::info!("3D cinemático: {ACCUM_FRAMES} quadros de {}×{} em {:.1} s", size_px[0], size_px[1], cx.time - v.refine_start);
            v.refine_start = -1.0;
        }
        if refining {
            ui.ctx().request_repaint();
        }
        v.refining = refining;
        painter.add(egui_wgpu::Callback::new_paint_callback(
            rect,
            VolumeCallback { view_id: self.id, volume: gpu.clone(), params, tf_table: table, size_px, progress: v.progress.clone() },
        ));

        // ---- overlays
        let pad = 10.0;
        let title = match v.mode {
            RenderMode::Composite => format!("VR · {}", preset.name),
            RenderMode::Iso => trf!("Superfície · {:.0}", "Surface · {:.0}", v.iso.unwrap_or_else(|| default_iso(series))),
            m => format!("{} · W {:.0} L {:.0}", m.label(), window.width, window.center),
        };
        let level = if cx.reading { cx.info } else if cx.overlays { crate::config::InfoLevel::Full } else { crate::config::InfoLevel::None };
        if !cx.reading || level != crate::config::InfoLevel::None {
            plane_chip(painter, rect, &title, ACCENT);
        }
        if level == crate::config::InfoLevel::Full {
            let st = &series.study;
            let top = pad + self.top_inset;
            shadow_text(painter, rect.right_top() + vec2(-pad, top), Align2::RIGHT_TOP, format_person_name(&st.patient_name), font_bold(13.0), Color32::WHITE);
            let series_line = trf!("Série {}: {}", "Series {}: {}", series.info.number, series.info.title());
            shadow_text(painter, rect.right_top() + vec2(-pad, top + 18.0), Align2::RIGHT_TOP, series_line, font(12.0), OVERLAY_TEXT);
            let mut info = format!("{} × {} × {}", gpu.dims[0], gpu.dims[1], gpu.dims[2]);
            if gpu.downsampled {
                info.push_str(tr!("  (reduzido para a GPU)", "  (downsampled for the GPU)"));
            }
            shadow_text(painter, rect.left_bottom() + vec2(pad, -pad), Align2::LEFT_BOTTOM, info, font(11.5), TEXT_DIM);
        }
        draw_gizmo(painter, rect.right_bottom() + vec2(-46.0, -46.0), &v.camera);
        if refining {
            let pct = (done * 100 / ACCUM_FRAMES).min(99);
            let pos = rect.center_bottom() + vec2(0.0, -16.0);
            let text = trf!("Refinando a luz… {pct}%", "Refining the lighting… {pct}%");
            let galley = painter.layout_no_wrap(text, font(11.5), OVERLAY_TEXT);
            let bg = Rect::from_center_size(pos, galley.size() + vec2(18.0, 8.0));
            painter.rect_filled(bg, CornerRadius::same(10), Color32::from_black_alpha(150));
            let bar = Rect::from_min_size(bg.left_bottom() + vec2(8.0, -3.0), vec2((bg.width() - 16.0) * pct as f32 / 100.0, 1.5));
            painter.rect_filled(bar, CornerRadius::ZERO, ACCENT);
            painter.galley(pos - galley.size() * 0.5, galley, OVERLAY_TEXT);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}

/// Intersection of the reference plane with the displayed image, clipped to its bounds.
fn draw_reference_line(painter: &egui::Painter, xf: &Xf, frame: &Frame, r: &RefPlane) {
    let Some(o) = frame.origin else { return };
    // Points of this image: x = o + u*a + v*b (a, b in mm from the first pixel center).
    let (nu, nv) = (frame.u.dot(r.normal), frame.v.dot(r.normal));
    if nu.abs() < 1e-3 && nv.abs() < 1e-3 {
        return; // parallel planes
    }
    let d0 = (o - r.point).dot(r.normal);
    let (w, h) = ((frame.w as f64 - 1.0) * frame.dx as f64, (frame.h as f64 - 1.0) * frame.dy as f64);
    // Solve d0 + a*nu + b*nv = 0 on the four borders.
    let mut pts: Vec<(f64, f64)> = Vec::new();
    if nv.abs() > 1e-9 {
        for a in [0.0, w] {
            let b = -(d0 + a * nu) / nv;
            if (0.0..=h).contains(&b) {
                pts.push((a, b));
            }
        }
    }
    if nu.abs() > 1e-9 {
        for b in [0.0, h] {
            let a = -(d0 + b * nv) / nu;
            if (0.0..=w).contains(&a) {
                pts.push((a, b));
            }
        }
    }
    if pts.len() < 2 {
        return;
    }
    let to_screen = |(a, b): (f64, f64)| xf.to_screen(pos2((a / frame.dx as f64) as f32 + 0.5, (b / frame.dy as f64) as f32 + 0.5));
    let (p0, p1) = (to_screen(pts[0]), to_screen(pts[pts.len() - 1]));
    painter.line_segment([p0, p1], Stroke::new(1.3, with_alpha(WARNING, 200)));
}

fn default_iso(series: &LoadedSeries) -> f32 {
    if series.info.modality == "CT" {
        250.0
    } else {
        let w = series.default_window;
        w.center
    }
}

/// Color of the CPR path and image label.
const CURVE: Color32 = Color32::from_rgb(255, 196, 64);
/// Color of the 3D cursor.
const CURSOR: Color32 = Color32::from_rgb(64, 224, 255);

fn image_mesh(tex: egui::TextureId, xf: &Xf, w: usize, h: usize, tint: Color32) -> egui::Mesh {
    let corners = [pos2(0.0, 0.0), pos2(w as f32, 0.0), pos2(w as f32, h as f32), pos2(0.0, h as f32)];
    let uvs = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
    let mut mesh = egui::Mesh::with_texture(tex);
    for (c, uv) in corners.iter().zip(uvs) {
        mesh.vertices.push(egui::epaint::Vertex { pos: xf.to_screen(*c), uv, color: tint });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    mesh
}

/// Exam B over exam A: transparency, a sliding curtain or a checkerboard.
/// Removable labels of what the tools changed on screen, then "Restaurar tudo",
/// stacked upwards from `bottom` at the left edge of the view.
fn draw_change_chips(ui: &Ui, painter: &egui::Painter, rect: Rect, bottom: f32, id: u64, changes: &[Change], ev: &mut ViewEvents) {
    if changes.is_empty() || rect.width() < 220.0 {
        return;
    }
    let (pad, h, gap) = (10.0, 22.0, 5.0);
    let mut items: Vec<(Option<Change>, Arc<egui::Galley>, f32)> = changes
        .iter()
        .map(|c| {
            let g = painter.layout_no_wrap(c.label(), font(11.5), Color32::WHITE);
            let w = g.size().x + 30.0;
            (Some(*c), g, w)
        })
        .collect();
    let g = painter.layout_no_wrap(tr!("Restaurar tudo", "Reset all").to_string(), font_bold(11.5), ACCENT);
    let w = g.size().x + 16.0;
    items.push((None, g, w));

    // Wrap into rows that fit the view, then stack the rows upwards.
    let max_w = rect.width() - 2.0 * pad - 24.0;
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut x = 0.0;
    for (i, item) in items.iter().enumerate() {
        if x > 0.0 && x + item.2 > max_w {
            rows.push(Vec::new());
            x = 0.0;
        }
        if let Some(row) = rows.last_mut() {
            row.push(i);
        }
        x += item.2 + gap;
    }
    for (ri, row) in rows.iter().enumerate() {
        let y = bottom - (rows.len() - ri) as f32 * (h + gap);
        let mut x = rect.left() + pad;
        for &i in row {
            let (change, galley, w) = &items[i];
            let r = Rect::from_min_size(pos2(x, y), vec2(*w, h));
            x += w + gap;
            let resp = ui.interact(r, Id::unique(("change-chip", id, i)), Sense::click());
            let hot = resp.hovered();
            if hot {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let text_at = |left: f32| pos2(r.left() + left, r.center().y - galley.size().y * 0.5);
            painter.rect_filled(r, CornerRadius::same(11), Color32::from_rgba_unmultiplied(16, 20, 30, if hot { 245 } else { 200 }));
            match change {
                Some(c) => {
                    painter.rect_stroke(r, CornerRadius::same(11), Stroke::new(1.0, with_alpha(c.color(), if hot { 230 } else { 140 })), StrokeKind::Inside);
                    painter.galley(text_at(10.0), galley.clone(), Color32::WHITE);
                    painter.text(pos2(r.right() - 11.0, r.center().y), Align2::CENTER_CENTER, icons::X, font(11.0), if hot { DANGER } else { TEXT_DIM });
                    if resp.on_hover_text(tr!("Remover", "Remove")).clicked() {
                        ev.undo = Some(*c);
                    }
                }
                None => {
                    painter.galley(text_at(8.0), galley.clone(), ACCENT);
                    let tip = tr!(
                        "Todas as janelas voltam ao padrão (Shift+R). Os achados salvos ficam.",
                        "All viewports go back to default (Shift+R). Saved findings are kept."
                    );
                    if resp.on_hover_text(tip).clicked() {
                        ev.reset_all = true;
                    }
                }
            }
        }
    }
}

fn draw_fusion(ui: &mut Ui, painter: &egui::Painter, rect: Rect, xf: &Xf, layer: &FusionLayer, id: u64, ev: &mut ViewEvents) {
    let mesh = |tint| Shape::mesh(image_mesh(layer.tex, xf, layer.size[0], layer.size[1], tint));
    match layer.style {
        Fusion::Blend(a) => {
            painter.add(mesh(Color32::WHITE.gamma_multiply(a.clamp(0.0, 1.0))));
        }
        Fusion::Swipe(t) => {
            let x = rect.left() + rect.width() * t;
            painter.with_clip_rect(Rect::from_min_max(pos2(x, rect.top()), rect.right_bottom())).add(mesh(Color32::WHITE));
            let r = ui.interact(Rect::from_min_max(pos2(x - 9.0, rect.top()), pos2(x + 9.0, rect.bottom())), Id::unique(("swipe", id)), Sense::drag());
            let hot = r.hovered() || r.dragged();
            painter.vline(x, rect.y_range(), Stroke::new(if hot { 2.5 } else { 1.5 }, Color32::WHITE));
            let knob = pos2(x, rect.center().y);
            painter.circle_filled(knob, 14.0, if hot { ACCENT } else { with_alpha(ACCENT, 210) });
            painter.text(knob, Align2::CENTER_CENTER, icons::ARROWS_LEFT_RIGHT, font(14.0), Color32::WHITE);
            for (label, pos, align) in [("A", pos2(x - 22.0, rect.center().y), Align2::RIGHT_CENTER), ("B", pos2(x + 22.0, rect.center().y), Align2::LEFT_CENTER)] {
                shadow_text(painter, pos, align, label, font_bold(13.0), Color32::WHITE);
            }
            if hot {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            if r.dragged()
                && let Some(p) = r.interact_pointer_pos() {
                    ev.swipe = Some(((p.x - rect.left()) / rect.width()).clamp(0.02, 0.98));
                }
        }
        Fusion::Checker(mm) => {
            let (cw, ch) = (mm / xf.dx, mm / xf.dy);
            if cw <= 0.0 || ch <= 0.0 {
                return;
            }
            let nx = (layer.size[0] as f32 / cw).ceil() as i32;
            let ny = (layer.size[1] as f32 / ch).ceil() as i32;
            let base = mesh(Color32::WHITE);
            for j in 0..ny {
                for i in (0..nx).filter(|i| (i + j) % 2 == 1) {
                    let a = xf.to_screen(pos2(i as f32 * cw, j as f32 * ch));
                    let b = xf.to_screen(pos2((i + 1) as f32 * cw, (j + 1) as f32 * ch));
                    let r = Rect::from_two_pos(a, b).intersect(rect);
                    if r.is_positive() {
                        painter.with_clip_rect(r).add(base.clone());
                    }
                }
            }
        }
    }
}

/// How far from the plane (mm) a marker still counts as "on" this image.
fn plane_tolerance(frame: &Frame) -> f64 {
    if frame.cpr.is_some() {
        return 3.0;
    }
    (frame.thickness.unwrap_or(1.0) * 0.5).max(1.0) + 0.5
}

/// Screen positions of the rotation handles on the crosshair lines of an MPR plane.
fn crosshair_lines(rect: Rect, xf: &Xf, frame: &Frame, kind: PlaneKind, m: &MprShared) -> Option<(Pos2, Vec<(PlaneKind, Vec2)>)> {
    let (img, _) = frame.image_of(m.point)?;
    let center = xf.to_screen(img);
    let n = frame.normal();
    let lines = PlaneKind::ALL
        .into_iter()
        .filter(|k| *k != kind)
        .filter_map(|other| {
            let (ou, ov) = m.axes[other.index()];
            let dir3 = n.cross(ou.cross(ov));
            if dir3.length() < 1e-6 {
                return None;
            }
            let img_dir = pos2(dir3.dot(frame.u) as f32 / frame.dx, dir3.dot(frame.v) as f32 / frame.dy);
            Some((other, (xf.to_screen(img_dir) - xf.to_screen(Pos2::ZERO)).normalized()))
        })
        .collect();
    let _ = rect;
    Some((center, lines))
}

fn handle_radius(rect: Rect) -> f32 {
    rect.size().min_elem() * 0.34
}

fn near_handle(rect: Rect, xf: &Xf, frame: &Frame, kind: PlaneKind, m: &MprShared, p: Pos2) -> bool {
    let Some((center, lines)) = crosshair_lines(rect, xf, frame, kind, m) else { return false };
    let r = handle_radius(rect);
    lines.iter().any(|(_, d)| [-1.0, 1.0].iter().any(|s| (center + *d * r * *s).distance(p) < 11.0))
}

#[allow(clippy::too_many_arguments)]
fn draw_crosshair(painter: &egui::Painter, rect: Rect, xf: &Xf, frame: &Frame, kind: PlaneKind, m: &MprShared, hover: Option<Pos2>, rotating: bool) {
    let Some((center, lines)) = crosshair_lines(rect, xf, frame, kind, m) else { return };
    let len = rect.size().length();
    let r = handle_radius(rect);
    let gap = 14.0;
    for (other, dir) in lines {
        let color = with_alpha(other.color(), 210);
        for s in [-1.0, 1.0] {
            painter.line_segment([center + dir * gap * s, center + dir * len * s], Stroke::new(1.2, color));
            let hp = center + dir * r * s;
            let hot = rotating || hover.is_some_and(|h| h.distance(hp) < 11.0);
            painter.circle_filled(hp, if hot { 6.5 } else { 4.5 }, other.color());
            painter.circle_stroke(hp, if hot { 6.5 } else { 4.5 }, Stroke::new(1.0, Color32::from_black_alpha(170)));
        }
    }
    painter.circle_stroke(center, 3.0, Stroke::new(1.2, Color32::from_white_alpha(180)));
}

/// Path clicked for a curved reformation, projected on the image.
fn draw_curve(painter: &egui::Painter, xf: &Xf, frame: &Frame, pts: &[DVec3]) {
    let tol = plane_tolerance(frame);
    let proj: Vec<(Pos2, bool)> = pts.iter().filter_map(|p| frame.image_of(*p).map(|(img, d)| (xf.to_screen(img), d.abs() <= tol))).collect();
    for w in proj.windows(2) {
        painter.extend(Shape::dashed_line(&[w[0].0, w[1].0], Stroke::new(1.6, with_alpha(CURVE, 210)), 7.0, 4.0));
    }
    for (i, (p, on)) in proj.iter().enumerate() {
        if *on {
            painter.circle_filled(*p, 4.5, CURVE);
        } else {
            painter.circle_stroke(*p, 4.0, Stroke::new(1.3, with_alpha(CURVE, 160)));
        }
        shadow_text(painter, *p + vec2(7.0, -7.0), Align2::LEFT_BOTTOM, (i + 1).to_string(), font_bold(10.5), CURVE);
    }
}

/// Findings and landmarks that lie on (or near) the displayed image.
fn draw_pins(painter: &egui::Painter, xf: &Xf, frame: &Frame, pins: &[Pin], slice: usize) {
    let tol = plane_tolerance(frame);
    for pin in pins {
        let at = match (pin.point, pin.image) {
            (Some(p), _) => match frame.image_of(p) {
                Some((img, d)) if d.abs() <= tol => img,
                _ => continue,
            },
            (None, Some((s, img))) if s == slice && frame.origin.is_none() && frame.cpr.is_none() => img,
            _ => continue,
        };
        let c = xf.to_screen(at);
        let head = c - vec2(0.0, 18.0);
        painter.line_segment([c, head], Stroke::new(2.0, pin.color));
        painter.circle_filled(c, 2.5, pin.color);
        painter.circle_filled(head, 7.5, pin.color);
        painter.circle_filled(head, 2.8, Color32::from_black_alpha(200));
        if !pin.label.is_empty() {
            let galley = painter.layout_no_wrap(pin.label.clone(), font_bold(11.5), Color32::WHITE);
            let r = Rect::from_min_size(head + vec2(11.0, -galley.size().y * 0.5 - 3.0), galley.size() + vec2(10.0, 6.0));
            painter.rect_filled(r, CornerRadius::same(5), Color32::from_rgba_unmultiplied(12, 14, 19, 220));
            painter.rect_stroke(r, CornerRadius::same(5), Stroke::new(1.0, with_alpha(pin.color, 180)), StrokeKind::Inside);
            painter.galley(r.min + vec2(5.0, 3.0), galley, Color32::WHITE);
        }
    }
}

fn draw_cursor(painter: &egui::Painter, xf: &Xf, frame: &Frame, p: DVec3) {
    let Some((img, d)) = frame.image_of(p) else { return };
    if d.abs() > plane_tolerance(frame) {
        return;
    }
    let c = xf.to_screen(img);
    let s = Stroke::new(1.8, CURSOR);
    painter.circle_stroke(c, 10.0, Stroke::new(3.5, Color32::from_black_alpha(120)));
    painter.circle_stroke(c, 10.0, s);
    for d in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
        painter.line_segment([c + d * 4.0, c + d * 17.0], s);
    }
}

/// Magnified inset following the pointer.
#[allow(clippy::too_many_arguments)]
fn draw_magnifier(painter: &egui::Painter, rect: Rect, xf: &Xf, tex: egui::TextureId, overlay: Option<egui::TextureId>, size: [usize; 2], at: Pos2, zoom: f32, value: Option<String>) {
    let side = (rect.size().min_elem() * 0.42).clamp(140.0, 340.0);
    let lens = Rect::from_center_size(at, vec2(side, side));
    let mut zx = *xf;
    zx.s = xf.s * zoom;
    zx.c = at - (at - xf.c) * zoom;
    painter.rect_filled(lens.expand(5.0), CornerRadius::same(14), Color32::from_black_alpha(110));
    let clip = painter.with_clip_rect(lens.intersect(rect));
    clip.rect_filled(lens, CornerRadius::same(10), Color32::BLACK);
    clip.add(Shape::mesh(image_mesh(tex, &zx, size[0], size[1], Color32::WHITE)));
    if let Some(o) = overlay {
        clip.add(Shape::mesh(image_mesh(o, &zx, size[0], size[1], Color32::WHITE)));
    }
    clip.circle_stroke(at, 2.0, Stroke::new(1.0, Color32::from_white_alpha(200)));
    clip.rect_stroke(lens, CornerRadius::same(10), Stroke::new(1.5, ACCENT), StrokeKind::Inside);
    let label = match value {
        Some(v) => format!("{zoom:.0}×  ·  {v}"),
        None => format!("{zoom:.0}×"),
    };
    let galley = painter.layout_no_wrap(label, font_bold(11.0), TEXT);
    let r = Rect::from_min_size(pos2(lens.center().x - galley.size().x * 0.5 - 6.0, lens.bottom() - galley.size().y - 12.0), galley.size() + vec2(12.0, 6.0));
    clip.rect_filled(r, CornerRadius::same(5), Color32::from_black_alpha(190));
    clip.galley(r.min + vec2(6.0, 3.0), galley, TEXT);
}

/// Card listing typical tissue densities; rows holding `value` are highlighted.
fn draw_tissue_legend(painter: &egui::Painter, rect: Rect, value: Option<f32>, top: f32, right: f32) {
    let row = 17.0;
    let width = 238.0;
    let all: Vec<&tissues::Tissue> = tissues::TISSUES.iter().collect();
    let room = rect.bottom() - 90.0 - top;
    let full_h = 30.0 + row * all.len() as f32 + 22.0;
    // Small viewports: only the tissues that match the value.
    let rows: Vec<&tissues::Tissue> = if full_h <= room {
        all
    } else {
        match value {
            Some(v) => vec![tissues::classify(v)],
            None => return,
        }
    };
    if rect.width() < width + 140.0 || room < 60.0 {
        return;
    }
    let h = 30.0 + row * rows.len() as f32 + if rows.len() > 1 { 22.0 } else { 6.0 };
    let card = Rect::from_min_size(pos2(right - width, top), vec2(width, h));
    painter.rect_filled(card, CornerRadius::same(9), Color32::from_rgba_unmultiplied(12, 14, 19, 225));
    painter.rect_stroke(card, CornerRadius::same(9), Stroke::new(1.0, BORDER_STRONG), StrokeKind::Inside);
    painter.text(card.left_top() + vec2(10.0, 8.0), Align2::LEFT_TOP, tr!("Densidades típicas (HU)", "Typical densities (HU)"), font_bold(11.5), TEXT);
    if let Some(v) = value {
        painter.text(card.right_top() + vec2(-10.0, 8.0), Align2::RIGHT_TOP, format!("{v:.0}").replace('-', "−"), font_bold(11.5), MEASURE);
    }
    let best = value.map(tissues::classify);
    for (i, t) in rows.iter().enumerate() {
        let y = card.top() + 28.0 + i as f32 * row;
        let hit = value.is_some_and(|v| t.contains(v));
        let r = Rect::from_min_size(pos2(card.left() + 4.0, y), vec2(width - 8.0, row));
        if hit {
            let strong = best.is_some_and(|b| std::ptr::eq(b, *t));
            painter.rect_filled(r, CornerRadius::same(4), if strong { with_alpha(ACCENT, 90) } else { with_alpha(ACCENT, 40) });
        }
        // Gray swatch: how the tissue looks in a wide window.
        let g = (((t.typical.clamp(-1000.0, 1000.0) + 1000.0) / 2000.0) * 235.0 + 20.0) as u8;
        painter.rect_filled(Rect::from_center_size(pos2(r.left() + 10.0, r.center().y), vec2(9.0, 9.0)), CornerRadius::same(2), Color32::from_gray(g));
        let color = if hit { Color32::WHITE } else { TEXT_DIM };
        painter.text(pos2(r.left() + 20.0, r.center().y), Align2::LEFT_CENTER, t.name, if hit { font_bold(11.5) } else { font(11.5) }, color);
        painter.text(pos2(r.right() - 6.0, r.center().y), Align2::RIGHT_CENTER, t.range_text(), font(11.0), if hit { TEXT } else { TEXT_FAINT });
    }
    if rows.len() > 1 {
        let note = tr!("Sem contraste; variam com aparelho e kV.", "Non-contrast; vary with scanner and kV.");
        painter.text(pos2(card.left() + 10.0, card.bottom() - 9.0), Align2::LEFT_CENTER, note, font(10.0), TEXT_FAINT);
    }
}

fn length_mm(m: &Measurement, frame: &Frame) -> f32 {
    let (dx, dy) = ((m.pts[1].x - m.pts[0].x) * frame.dx, (m.pts[1].y - m.pts[0].y) * frame.dy);
    (dx * dx + dy * dy).sqrt()
}

fn angle_deg(m: &Measurement, frame: &Frame) -> f32 {
    let a = vec2((m.pts[0].x - m.pts[1].x) * frame.dx, (m.pts[0].y - m.pts[1].y) * frame.dy);
    let b = vec2((m.pts[2].x - m.pts[1].x) * frame.dx, (m.pts[2].y - m.pts[1].y) * frame.dy);
    a.normalized().dot(b.normalized()).clamp(-1.0, 1.0).acos().to_degrees()
}

fn ellipse_axes(m: &Measurement) -> (Pos2, f32, f32) {
    let center = pos2((m.pts[0].x + m.pts[1].x) * 0.5, (m.pts[0].y + m.pts[1].y) * 0.5);
    (center, (m.pts[1].x - m.pts[0].x).abs() * 0.5, (m.pts[1].y - m.pts[0].y).abs() * 0.5)
}

/// Values of the measurements on the displayed image.
fn measure_values(measurements: &[Measurement], frame: &Frame) -> Vec<MeasureValue> {
    measurements
        .iter()
        .enumerate()
        .filter(|(_, m)| m.key == frame.key)
        .filter_map(|(index, m)| {
            let center = pos2(m.pts.iter().map(|p| p.x).sum::<f32>() / m.pts.len() as f32, m.pts.iter().map(|p| p.y).sum::<f32>() / m.pts.len() as f32);
            let unit_len = if frame.spacing_known { "mm" } else { "px" };
            let (value, unit, mean) = match m.kind {
                MeasureKind::Length => (length_mm(m, frame), unit_len, None),
                MeasureKind::Angle if m.pts.len() == 3 => (angle_deg(m, frame), "°", None),
                MeasureKind::Ellipse => {
                    let (c, ra, rb) = ellipse_axes(m);
                    let stats = ellipse_numbers(frame, c, ra, rb);
                    (std::f32::consts::PI * ra * frame.dx * rb * frame.dy, if frame.spacing_known { "mm²" } else { "px²" }, stats.map(|s| s.0 as f32))
                }
                MeasureKind::Probe => (frame.real_at(m.pts[0])?, "", None),
                _ => return None,
            };
            Some(MeasureValue { index, kind: m.kind, center, value, unit, mean })
        })
        .collect()
}

fn ellipse_numbers(frame: &Frame, c: Pos2, ra: f32, rb: f32) -> Option<(f64, f64, f32, f32)> {
    let Some(Slice::Gray(d, q)) = frame.slice() else { return None };
    if ra < 0.5 || rb < 0.5 {
        return None;
    }
    let (x0, x1) = ((c.x - ra).floor().max(0.0) as usize, ((c.x + ra).ceil() as usize).min(frame.w));
    let (y0, y1) = ((c.y - rb).floor().max(0.0) as usize, ((c.y + rb).ceil() as usize).min(frame.h));
    let (mut n, mut sum, mut sum2, mut lo, mut hi) = (0u64, 0f64, 0f64, f32::MAX, f32::MIN);
    for y in y0..y1 {
        for x in x0..x1 {
            let (px, py) = ((x as f32 + 0.5 - c.x) / ra, (y as f32 + 0.5 - c.y) / rb);
            if px * px + py * py > 1.0 {
                continue;
            }
            let s = d[y * frame.w + x];
            if s == OUTSIDE {
                continue;
            }
            let v = q.real(s);
            n += 1;
            sum += v as f64;
            sum2 += (v as f64) * (v as f64);
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    if n == 0 {
        return None;
    }
    let mean = sum / n as f64;
    Some((mean, (sum2 / n as f64 - mean * mean).max(0.0).sqrt(), lo, hi))
}

fn ellipse_stats(frame: &Frame, c: Pos2, ra: f32, rb: f32) -> String {
    let Some((mean, sd, lo, hi)) = ellipse_numbers(frame, c, ra, rb) else { return String::new() };
    let area = std::f32::consts::PI * ra * frame.dx * rb * frame.dy;
    let area = if !frame.spacing_known {
        format!("{area:.0} px²")
    } else if area > 100.0 {
        crate::i18n::dec(format!("{:.2} cm²", area / 100.0))
    } else {
        crate::i18n::dec(format!("{area:.1} mm²"))
    };
    let (mean, sd) = (crate::i18n::num(mean, 1), crate::i18n::num(sd, 1));
    trf!("Média {mean}  DP {sd}\nMín {lo:.0}  Máx {hi:.0}\nÁrea {area}", "Mean {mean}  SD {sd}\nMin {lo:.0}  Max {hi:.0}\nArea {area}")
}

/// Orientation letters (A, P, R, L, S, I) in the middle of each edge.
fn draw_orientation(painter: &egui::Painter, rect: Rect, frame: &Frame, xf: &Xf, pad: f32, pad_r: f32, color: Color32) {
    if frame.origin.is_none() {
        return;
    }
    let dir = |d: Vec2| {
        let m = xf.dir_to_image_mm(d);
        frame.u * m.x as f64 + frame.v * m.y as f64
    };
    let right = orientation_label(dir(vec2(1.0, 0.0)));
    let down = orientation_label(dir(vec2(0.0, 1.0)));
    let left = orientation_label(-dir(vec2(1.0, 0.0)));
    let up = orientation_label(-dir(vec2(0.0, 1.0)));
    let ff = font_bold(14.0);
    let c = rect.center();
    shadow_text(painter, pos2(rect.right() - pad_r, c.y), Align2::RIGHT_CENTER, right, ff.clone(), color);
    shadow_text(painter, pos2(rect.left() + pad, c.y), Align2::LEFT_CENTER, left, ff.clone(), color);
    shadow_text(painter, pos2(c.x, rect.bottom() - pad), Align2::CENTER_BOTTOM, down, ff.clone(), color);
    shadow_text(painter, pos2(c.x, rect.top() + pad), Align2::CENTER_TOP, up, ff, color);
}

/// "AXIAL" → "Axial".
fn title_case(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut chars = lower.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Plane of a native image from its orientation.
fn stack_plane_name(frame: &Frame) -> &'static str {
    if frame.origin.is_none() {
        return tr!("Imagem", "Image");
    }
    let n = frame.u.cross(frame.v).normalize().abs();
    match n.max_element() {
        m if m < 0.8 => tr!("Oblíquo", "Oblique"),
        _ if n.z >= n.x && n.z >= n.y => "Axial",
        _ if n.y >= n.x => "Coronal",
        _ => tr!("Sagital", "Sagittal"),
    }
}

fn plane_chip(painter: &egui::Painter, rect: Rect, label: &str, color: Color32) -> Rect {
    let galley = painter.layout_no_wrap(label.to_string(), font_bold(11.0), TEXT);
    let r = Rect::from_min_size(rect.left_top() + vec2(10.0, 10.0), galley.size() + vec2(26.0, 8.0));
    painter.rect_filled(r, CornerRadius::same(6), Color32::from_rgba_unmultiplied(12, 14, 19, 220));
    painter.rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, BORDER_STRONG), StrokeKind::Inside);
    painter.circle_filled(pos2(r.left() + 10.0, r.center().y), 3.5, color);
    painter.galley(pos2(r.left() + 19.0, r.top() + 4.0), galley, TEXT);
    r
}

fn draw_empty(painter: &egui::Painter, rect: Rect) {
    let inner = rect.shrink(18.0);
    let dash = Shape::dashed_line(
        &[inner.left_top(), inner.right_top(), inner.right_bottom(), inner.left_bottom(), inner.left_top()],
        Stroke::new(1.0, BORDER_STRONG),
        6.0,
        5.0,
    );
    painter.extend(dash);
    painter.text(rect.center() - vec2(0.0, 12.0), Align2::CENTER_CENTER, icons::IMAGE, font(26.0), TEXT_FAINT);
    painter.text(rect.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, tr!("Arraste uma série para cá", "Drag a series here"), font(12.5), TEXT_FAINT);
}

fn draw_gizmo(painter: &egui::Painter, c: Pos2, cam: &OrbitCamera) {
    let axes = [
        (Vec3::X, "L", "R", Color32::from_rgb(240, 96, 96)),
        (Vec3::Y, "P", "A", Color32::from_rgb(96, 214, 128)),
        (Vec3::Z, "S", "I", Color32::from_rgb(96, 150, 250)),
    ];
    let mut items: Vec<(f32, Pos2, &str, Color32, bool)> = Vec::new();
    for (a, pos_l, neg_l, col) in axes {
        for (sign, label, strong) in [(1.0, pos_l, true), (-1.0, neg_l, false)] {
            let v = cam.rotation * (a * sign);
            let p = c + vec2(v.x, -v.y) * 30.0;
            items.push((v.z, p, label, col, strong));
        }
    }
    items.sort_by(|a, b| a.0.total_cmp(&b.0));
    painter.circle_filled(c, 40.0, Color32::from_black_alpha(90));
    for (_, p, label, col, strong) in items {
        let col = if strong { col } else { with_alpha(col, 110) };
        painter.line_segment([c, p], Stroke::new(1.5, col));
        painter.circle_filled(p, 8.0, Color32::from_rgb(14, 16, 22));
        painter.circle_stroke(p, 8.0, Stroke::new(1.0, col));
        painter.text(p, Align2::CENTER_CENTER, label, font_bold(10.0), col);
    }
}
