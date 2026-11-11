//! Background work. Every job runs off the UI thread and reports back through
//! a channel; the UI drains it once per frame.

use crate::config::{PacsNode, PacsProtocol, RetrieveMethod};
use crate::dicom::decode::{decode_file, decode_file_frames, FramePixels, GrayData};
use crate::dicom::model::{ScannedFile, SeriesInfo, StudyHeader};
use crate::dicom::scan::{self, ScanProgress};
use crate::dicom::series::LoadedSeries;
use crate::imaging::lut::{auto_window, Window};
use crate::imaging::volume::{Quant, Volume};
use crate::pacs::dicomweb;
use crate::pacs::scu::{self, RetrieveProgress, SeriesResult, StudyQuery, StudyResult};
use crate::render3d::renderer::GpuVolumeData;
use egui::{Color32, ColorImage};
use rayon::prelude::*;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

pub enum Msg {
    Scanned { files: Vec<ScannedFile>, open: bool, total: usize },
    SeriesLoaded { uid: String, result: Result<Arc<LoadedSeries>, String> },
    Thumb { uid: String, image: ColorImage },
    GpuReady(Arc<GpuVolumeData>),
    Echo { node: String, result: Result<Duration, String> },
    Studies(Result<Vec<StudyResult>, String>),
    /// The patient's exams on the PACS, for the timeline.
    Priors { patient_id: String, result: Result<Vec<StudyResult>, String> },
    Series { study_uid: String, result: Result<Vec<SeriesResult>, String> },
    Retrieve { id: u64, progress: RetrieveProgress },
    RetrieveDone { id: u64, files: Vec<PathBuf>, result: Result<RetrieveProgress, String> },
    ScpStored(PathBuf),
    ScpError(String),
    Registered { version: u64, result: Option<crate::imaging::register::AutoResult> },
    Calculi { series: Arc<LoadedSeries>, threshold: f32, items: Vec<crate::imaging::calculi::Candidate> },
    /// Downloaded exams removed (`days`: age limit, `None` when all were removed on request) and what is left.
    StoreCleaned { removed: crate::store::StoreUsage, left: crate::store::StoreUsage, days: Option<u32> },
}

pub struct LoadProgress {
    pub done: AtomicUsize,
    pub total: usize,
}

pub struct Jobs {
    pub tx: Sender<Msg>,
    pub rx: Receiver<Msg>,
    ctx: egui::Context,
}

impl Jobs {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, ctx }
    }

    pub fn ctx(&self) -> egui::Context {
        self.ctx.clone()
    }

    fn spawn(&self, name: &str, f: impl FnOnce(Sender<Msg>) + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        // The browser has no threads here: the job runs once the current frame is on screen.
        #[cfg(target_arch = "wasm32")]
        {
            let _ = name;
            crate::web::defer(move || {
                f(tx);
                ctx.request_repaint();
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let name = name.to_string();
        #[cfg(not(target_arch = "wasm32"))]
        let _ = std::thread::Builder::new().name(name.clone()).spawn(move || {
            if std::panic::catch_unwind(AssertUnwindSafe(|| f(tx))).is_err() {
                log::error!("tarefa '{name}' falhou (panic)");
            }
            ctx.request_repaint();
        });
    }

    /// Removes downloaded exams older than `days` (all when `None`), except `keep`, and measures what is left.
    pub fn clean_store(&self, root: PathBuf, days: Option<u32>, keep: std::collections::HashSet<String>) {
        if cfg!(target_arch = "wasm32") {
            return; // no downloaded exams in the web version
        }
        self.spawn("store", move |tx| {
            let removed = crate::store::clean(&root, days.map(|d| Duration::from_secs(d as u64 * 86_400)), &keep);
            let left = crate::store::usage(&root);
            let _ = tx.send(Msg::StoreCleaned { removed, left, days });
        });
    }

    /// Measures the downloaded exams without removing any.
    pub fn measure_store(&self, root: PathBuf) {
        if cfg!(target_arch = "wasm32") {
            return;
        }
        self.spawn("store", move |tx| {
            let left = crate::store::usage(&root);
            let _ = tx.send(Msg::StoreCleaned { removed: Default::default(), left, days: None });
        });
    }

    /// Small dense foci (stones, appendicoliths) in a series' volume.
    pub fn find_calculi(&self, series: Arc<LoadedSeries>, threshold: f32) {
        self.spawn("calculi", move |tx| {
            let Some(vol) = series.volume.clone() else { return };
            let t = web_time::Instant::now();
            let items = crate::imaging::calculi::find(&vol, threshold, 6000.0);
            log::info!("busca de cálculos: {} foco(s) em {:.1} s", items.len(), t.elapsed().as_secs_f32());
            let _ = tx.send(Msg::Calculi { series, threshold, items });
        });
    }

    /// Automatic alignment of exam B to exam A (comparison).
    pub fn register(&self, fixed: Arc<Volume>, moving: Arc<Volume>, init: crate::imaging::register::Rigid, version: u64, cancel: Arc<AtomicBool>) {
        self.spawn("register", move |tx| {
            let t = web_time::Instant::now();
            let result = crate::imaging::register::auto_register(&fixed, &moving, init, &cancel);
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            log::info!("alinhamento automático em {:.1} s", t.elapsed().as_secs_f32());
            let _ = tx.send(Msg::Registered { version, result });
        });
    }

    pub fn sender(&self) -> (Sender<Msg>, egui::Context) {
        (self.tx.clone(), self.ctx.clone())
    }

    pub fn scan(&self, paths: Vec<PathBuf>, open: bool) -> Arc<ScanProgress> {
        let progress = Arc::new(ScanProgress::default());
        let p = progress.clone();
        let ctx = self.ctx.clone();
        self.spawn("scan", move |tx| {
            let files = scan::collect_files(&paths, &p);
            ctx.request_repaint();
            let scanned = scan::scan_files(&files, &p);
            let _ = tx.send(Msg::Scanned { files: scanned, open, total: files.len() });
        });
        progress
    }

    pub fn load_series(&self, info: SeriesInfo, study: StudyHeader) -> Arc<LoadProgress> {
        let paths: Vec<PathBuf> = info.instances.iter().filter(|i| i.has_pixels).map(|i| i.path.clone()).collect();
        let progress = Arc::new(LoadProgress { done: AtomicUsize::new(0), total: paths.len() });
        let p = progress.clone();
        let ctx = self.ctx.clone();
        self.spawn("load-series", move |tx| {
            let uid = info.uid.clone();
            let mut errors = Vec::new();
            let frames: Vec<_> = paths
                .par_iter()
                .map(|path| {
                    let r = std::panic::catch_unwind(AssertUnwindSafe(|| decode_file(path)))
                        .unwrap_or_else(|_| Err(anyhow::anyhow!(tr!("falha inesperada ao decodificar", "unexpected failure while decoding"))));
                    let done = p.done.fetch_add(1, Ordering::Relaxed);
                    if done.is_multiple_of(16) {
                        ctx.request_repaint();
                    }
                    r.map_err(|e| format!("{}: {e:#}", path.display()))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|r| r.map_err(|e| errors.push(e)).ok())
                .flatten()
                .collect();
            let result = if frames.is_empty() {
                let first = errors.first().cloned().unwrap_or_else(|| tr!("nenhuma imagem", "no images").into());
                Err(trf!("Não foi possível decodificar a série. {first}", "Could not decode the series. {first}"))
            } else {
                let mut series = LoadedSeries::build(info, study, frames);
                if !errors.is_empty() {
                    log::warn!("{} arquivo(s) com erro: {}", errors.len(), errors[0]);
                    series.warnings.push(trf!("{} arquivo(s) não puderam ser lidos.", "{} file(s) could not be read.", errors.len()));
                }
                Ok(Arc::new(series))
            };
            let _ = tx.send(Msg::SeriesLoaded { uid, result });
        });
        progress
    }

    pub fn thumbnails(&self, items: Vec<(String, PathBuf, u32)>) {
        if items.is_empty() {
            return;
        }
        let ctx = self.ctx.clone();
        self.spawn("thumbnails", move |tx| {
            items.par_iter().for_each(|(uid, path, frame)| {
                if let Some(image) = make_thumbnail(path, *frame) {
                    let _ = tx.send(Msg::Thumb { uid: uid.clone(), image });
                    ctx.request_repaint();
                }
            });
        });
    }

    pub fn prepare_gpu(&self, volume: Arc<Volume>, max_dim: u32, max_voxels: usize) {
        self.spawn("prepare-3d", move |tx| {
            let data = GpuVolumeData::prepare(&volume, max_dim, max_voxels);
            let _ = tx.send(Msg::GpuReady(Arc::new(data)));
        });
    }

    pub fn echo(&self, node: PacsNode, local_ae: String) {
        self.spawn("c-echo", move |tx| {
            let result = match node.protocol {
                PacsProtocol::Dimse => scu::echo(&node, &local_ae),
                PacsProtocol::DicomWeb => dicomweb::echo(&node),
            };
            let _ = tx.send(Msg::Echo { node: node.name.clone(), result: result.map_err(|e| format!("{e:#}")) });
        });
    }

    pub fn find_studies(&self, node: PacsNode, local_ae: String, q: StudyQuery) {
        self.spawn("c-find", move |tx| {
            let r = match node.protocol {
                PacsProtocol::Dimse => scu::find_studies(&node, &local_ae, &q),
                PacsProtocol::DicomWeb => dicomweb::find_studies(&node, &q),
            };
            let _ = tx.send(Msg::Studies(r.map_err(|e| format!("{e:#}"))));
        });
    }

    /// Every exam of a patient on the PACS (patient timeline).
    pub fn find_priors(&self, node: PacsNode, local_ae: String, patient_id: String) {
        self.spawn("c-find-anteriores", move |tx| {
            let q = StudyQuery { patient_id: patient_id.clone(), ..Default::default() };
            let r = match node.protocol {
                PacsProtocol::Dimse => scu::find_studies(&node, &local_ae, &q),
                PacsProtocol::DicomWeb => dicomweb::find_studies(&node, &q),
            };
            let _ = tx.send(Msg::Priors { patient_id, result: r.map_err(|e| format!("{e:#}")) });
        });
    }

    pub fn find_series(&self, node: PacsNode, local_ae: String, study_uid: String) {
        self.spawn("c-find-series", move |tx| {
            let result = match node.protocol {
                PacsProtocol::Dimse => scu::find_series(&node, &local_ae, &study_uid),
                PacsProtocol::DicomWeb => dicomweb::find_series(&node, &study_uid),
            };
            let _ = tx.send(Msg::Series { study_uid, result: result.map_err(|e| format!("{e:#}")) });
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn retrieve(
        &self,
        id: u64,
        node: PacsNode,
        local_ae: String,
        study_uid: String,
        series_uid: Option<String>,
        store: PathBuf,
        cancel: Arc<AtomicBool>,
    ) {
        let ctx = self.ctx.clone();
        self.spawn("retrieve", move |tx| {
            let tx2 = tx.clone();
            let progress = |p: RetrieveProgress| {
                let _ = tx2.send(Msg::Retrieve { id, progress: p });
                ctx.request_repaint();
            };
            let (files, result) = match (node.protocol, node.retrieve) {
                (PacsProtocol::DicomWeb, _) => {
                    match dicomweb::retrieve(&node, &study_uid, series_uid.as_deref(), &store, &cancel, progress) {
                        Ok((files, p)) => (files, Ok(p)),
                        Err(e) => (Vec::new(), Err(format!("{e:#}"))),
                    }
                }
                (PacsProtocol::Dimse, RetrieveMethod::CGet) => {
                    match scu::retrieve_get(&node, &local_ae, &study_uid, series_uid.as_deref(), &store, &cancel, progress) {
                        Ok((files, p)) => (files, Ok(p)),
                        Err(e) => (Vec::new(), Err(format!("{e:#}"))),
                    }
                }
                (PacsProtocol::Dimse, RetrieveMethod::CMove) => (
                    Vec::new(),
                    scu::retrieve_move(&node, &local_ae, &local_ae, &study_uid, series_uid.as_deref(), &cancel, progress)
                        .map_err(|e| format!("{e:#}")),
                ),
            };
            let _ = tx.send(Msg::RetrieveDone { id, files, result });
        });
    }
}

/// Small preview of one frame, windowed with the file's own window when present.
fn make_thumbnail(path: &std::path::Path, frame: u32) -> Option<ColorImage> {
    let frames = decode_file_frames(path, Some(frame)).ok()?;
    let f = frames.into_iter().next()?;
    let (w, h) = (f.width, f.height);
    let gray: Vec<u8> = match &f.pixels {
        FramePixels::Rgb(d) => d.as_chunks::<3>().0.iter().map(|p| ((p[0] as u16 + p[1] as u16 + p[2] as u16) / 3) as u8).collect(),
        FramePixels::Gray(g) => {
            let (vals, quant): (Vec<i16>, Quant) = match g {
                GrayData::I16(d) => (d.clone(), Quant::IDENTITY),
                GrayData::F32(d) => {
                    let (lo, hi) = d.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
                    let q = Quant::for_range(lo as f64, hi as f64, false);
                    (d.iter().map(|&v| q.quantize(v)).collect(), q)
                }
            };
            let win = f.geometry.window.map(|(c, w)| Window::new(c as f32, w as f32)).unwrap_or_else(|| auto_window(&vals, quant));
            let lo = win.center - win.width * 0.5;
            vals.iter()
                .map(|&s| {
                    let t = ((quant.real(s) - lo) / win.width).clamp(0.0, 1.0);
                    let t = if f.invert { 1.0 - t } else { t };
                    (t * 255.0) as u8
                })
                .collect()
        }
    };
    let rgb: Option<&[u8]> = match &f.pixels {
        FramePixels::Rgb(d) => Some(d),
        _ => None,
    };
    let max = 128usize;
    let scale = (w.max(h) as f32 / max as f32).max(1.0);
    let (tw, th) = (((w as f32 / scale) as usize).max(1), ((h as f32 / scale) as usize).max(1));
    let mut pixels = Vec::with_capacity(tw * th);
    for ty in 0..th {
        for tx in 0..tw {
            // Box filter over the source block.
            let (x0, x1) = ((tx as f32 * scale) as usize, (((tx + 1) as f32 * scale) as usize).min(w).max(1));
            let (y0, y1) = ((ty as f32 * scale) as usize, (((ty + 1) as f32 * scale) as usize).min(h).max(1));
            let (mut acc, mut n) = ([0u32; 3], 0u32);
            for y in y0..y1.max(y0 + 1).min(h) {
                for x in x0..x1.max(x0 + 1).min(w) {
                    let i = y * w + x;
                    match rgb {
                        Some(d) => {
                            acc[0] += d[i * 3] as u32;
                            acc[1] += d[i * 3 + 1] as u32;
                            acc[2] += d[i * 3 + 2] as u32;
                        }
                        None => {
                            let g = gray[i] as u32;
                            acc = [acc[0] + g, acc[1] + g, acc[2] + g];
                        }
                    }
                    n += 1;
                }
            }
            let n = n.max(1);
            pixels.push(Color32::from_rgb((acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8));
        }
    }
    Some(ColorImage::new([tw, th], pixels))
}
