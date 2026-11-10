//! File discovery and fast header parsing (pixel data is not read here).

use super::model::{InstanceInfo, ScannedFile, StudyHeader};
use super::source;
use super::tagutil::*;
use dicom_dictionary_std::tags;
use dicom_object::InMemDicomObject;
use glam::DVec3;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Extensions that are certainly not DICOM; everything else is probed.
const SKIP_EXT: &[&str] = &[
    "exe", "dll", "sys", "inf", "ini", "txt", "htm", "html", "xml", "json", "pdf", "jpg", "jpeg", "png", "gif",
    "bmp", "ico", "rar", "7z", "msi", "cab", "js", "css", "lnk", "url", "log", "bat", "cmd", "ps1", "mp4",
    "avi", "wmv", "doc", "docx", "xls", "xlsx", "rtf", "chm", "hlp", "manifest", "cfg", "dat", "db",
];

#[derive(Default)]
pub struct ScanProgress {
    pub found: AtomicUsize,
    pub parsed: AtomicUsize,
    pub cancel: AtomicBool,
    /// Password-protected ZIP entries that had to be skipped.
    pub encrypted: AtomicUsize,
}

/// Names that are never DICOM images (also applied to entries inside ZIPs).
fn skip_name(name: &str) -> bool {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    if base.is_empty() || base.eq_ignore_ascii_case("DICOMDIR") || base.starts_with('.') {
        return true;
    }
    // macOS resource forks inside ZIPs made on a Mac.
    if name.starts_with("__MACOSX/") {
        return true;
    }
    let ext = base.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    SKIP_EXT.contains(&ext.as_str())
}

/// Adds every candidate entry of a ZIP as a virtual path (see [`source`]).
fn expand_zip(path: &Path, out: &mut Vec<PathBuf>, progress: &ScanProgress) {
    match source::list_zip(path) {
        Ok(listing) => {
            progress.encrypted.fetch_add(listing.encrypted, Ordering::Relaxed);
            for name in listing.entries.iter().filter(|n| !skip_name(n) && !n.to_ascii_lowercase().ends_with(".zip")) {
                out.push(source::entry_path(path, name));
                progress.found.fetch_add(1, Ordering::Relaxed);
            }
        }
        Err(e) => log::warn!("{e:#}"),
    }
}

pub fn collect_files(inputs: &[PathBuf], progress: &ScanProgress) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for input in inputs {
        // Files opened in the browser (web version).
        if let Some(files) = super::memfs::files_under(input) {
            for p in files {
                if source::is_zip(&p) {
                    expand_zip(&p, &mut out, progress);
                } else if !skip_name(&p.to_string_lossy()) {
                    out.push(p);
                    progress.found.fetch_add(1, Ordering::Relaxed);
                }
            }
            continue;
        }
        if input.is_file() {
            if source::is_zip(input) {
                expand_zip(input, &mut out, progress);
            } else {
                out.push(input.clone());
            }
            continue;
        }
        for entry in walkdir::WalkDir::new(input).follow_links(false).into_iter().filter_map(Result::ok) {
            if progress.cancel.load(Ordering::Relaxed) {
                return out;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path();
            if source::is_zip(p) {
                expand_zip(p, &mut out, progress);
                continue;
            }
            if skip_name(&p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()) {
                continue;
            }
            out.push(p.to_path_buf());
            progress.found.fetch_add(1, Ordering::Relaxed);
        }
    }
    out
}

pub fn scan_files(files: &[PathBuf], progress: &ScanProgress) -> Vec<ScannedFile> {
    files
        .par_iter()
        .filter_map(|p| {
            if progress.cancel.load(Ordering::Relaxed) {
                return None;
            }
            let r = read_header(p);
            progress.parsed.fetch_add(1, Ordering::Relaxed);
            r
        })
        .collect()
}

/// Quick check for the `DICM` magic so that random files are rejected cheaply.
fn looks_like_dicom(path: &Path) -> bool {
    use std::io::Read;
    let mut buf = [0u8; 132];
    let read = match super::memfs::get(path) {
        Some(bytes) => (&bytes[..]).read(&mut buf),
        None => {
            let Ok(mut f) = std::fs::File::open(path) else { return false };
            f.read(&mut buf)
        }
    };
    match read {
        Ok(n) if n >= 132 => &buf[128..132] == b"DICM" || source::plausible_raw_dataset(&buf),
        Ok(n) if n >= 8 => source::plausible_raw_dataset(&buf[..n]),
        _ => false,
    }
}

/// Header of a file on disk or inside a ZIP; `None` when it is not DICOM.
pub fn open_header(path: &Path) -> Option<dicom_object::DefaultDicomObject> {
    // ZIP entries are sniffed while streaming (see `source::open_dicom`).
    if source::split_entry(path).is_none() && !looks_like_dicom(path) {
        return None;
    }
    source::open_dicom(path, true).ok()
}

pub fn read_header(path: &Path) -> Option<ScannedFile> {
    let obj = open_header(path)?;
    let study_uid = text(&obj, tags::STUDY_INSTANCE_UID);
    let series_uid = text(&obj, tags::SERIES_INSTANCE_UID);
    if study_uid.is_empty() || series_uid.is_empty() {
        return None;
    }
    let rows = int(&obj, tags::ROWS).unwrap_or(0).max(0) as u32;
    let cols = int(&obj, tags::COLUMNS).unwrap_or(0).max(0) as u32;
    let geo = frame_geometry(&obj, 0);
    let study = StudyHeader {
        uid: study_uid,
        patient_name: text(&obj, tags::PATIENT_NAME),
        patient_id: text(&obj, tags::PATIENT_ID),
        birth_date: text(&obj, tags::PATIENT_BIRTH_DATE),
        sex: text(&obj, tags::PATIENT_SEX),
        age: text(&obj, tags::PATIENT_AGE),
        date: text(&obj, tags::STUDY_DATE),
        time: text(&obj, tags::STUDY_TIME),
        description: text(&obj, tags::STUDY_DESCRIPTION),
        accession: text(&obj, tags::ACCESSION_NUMBER),
        institution: text(&obj, tags::INSTITUTION_NAME),
        referring: text(&obj, tags::REFERRING_PHYSICIAN_NAME),
    };
    Some(ScannedFile {
        study,
        series_uid,
        series_number: int(&obj, tags::SERIES_NUMBER).unwrap_or(0),
        series_description: text(&obj, tags::SERIES_DESCRIPTION),
        modality: text(&obj, tags::MODALITY),
        body_part: text(&obj, tags::BODY_PART_EXAMINED),
        frame_of_reference: text(&obj, tags::FRAME_OF_REFERENCE_UID),
        contrast: text(&obj, tags::CONTRAST_BOLUS_AGENT),
        series_date: text(&obj, tags::SERIES_DATE),
        series_time: text(&obj, tags::SERIES_TIME),
        instance: InstanceInfo {
            path: path.to_path_buf(),
            sop_instance_uid: text(&obj, tags::SOP_INSTANCE_UID),
            instance_number: int(&obj, tags::INSTANCE_NUMBER).unwrap_or(0),
            frames: int(&obj, tags::NUMBER_OF_FRAMES).unwrap_or(1).max(1) as u32,
            rows,
            cols,
            position: geo.position,
            orientation: geo.orientation,
            pixel_spacing: geo.spacing,
            slice_thickness: geo.thickness,
            has_pixels: rows > 0 && cols > 0,
        },
    })
}

/// Geometry of one frame, looking first at the (enhanced multi-frame)
/// functional groups and then at the classic top-level attributes.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameGeometry {
    pub position: Option<DVec3>,
    pub orientation: Option<(DVec3, DVec3)>,
    pub spacing: Option<(f64, f64)>,
    pub thickness: Option<f64>,
    pub rescale: Option<(f64, f64)>,
    pub window: Option<(f64, f64)>,
}

pub fn frame_geometry(obj: &InMemDicomObject, frame: usize) -> FrameGeometry {
    let mut g = FrameGeometry {
        position: vec3(obj, tags::IMAGE_POSITION_PATIENT),
        orientation: orientation(obj),
        spacing: spacing(obj, tags::PIXEL_SPACING).or_else(|| spacing(obj, tags::IMAGER_PIXEL_SPACING)),
        thickness: float(obj, tags::SLICE_THICKNESS),
        rescale: rescale(obj),
        window: window(obj),
    };
    let groups = [
        seq_item(obj, tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE, 0),
        seq_item(obj, tags::PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE, frame),
    ];
    for grp in groups.into_iter().flatten() {
        if let Some(m) = seq_item(grp, tags::PIXEL_MEASURES_SEQUENCE, 0) {
            g.spacing = spacing(m, tags::PIXEL_SPACING).or(g.spacing);
            g.thickness = float(m, tags::SLICE_THICKNESS).or(g.thickness);
        }
        if let Some(p) = seq_item(grp, tags::PLANE_POSITION_SEQUENCE, 0) {
            g.position = vec3(p, tags::IMAGE_POSITION_PATIENT).or(g.position);
        }
        if let Some(o) = seq_item(grp, tags::PLANE_ORIENTATION_SEQUENCE, 0) {
            g.orientation = orientation(o).or(g.orientation);
        }
        if let Some(t) = seq_item(grp, tags::PIXEL_VALUE_TRANSFORMATION_SEQUENCE, 0) {
            g.rescale = rescale(t).or(g.rescale);
        }
        if let Some(w) = seq_item(grp, tags::FRAME_VOILUT_SEQUENCE, 0) {
            g.window = window(w).or(g.window);
        }
    }
    g
}

fn vec3(obj: &InMemDicomObject, tag: dicom_core::Tag) -> Option<DVec3> {
    let v = floats(obj, tag)?;
    (v.len() >= 3).then(|| DVec3::new(v[0], v[1], v[2]))
}

fn orientation(obj: &InMemDicomObject) -> Option<(DVec3, DVec3)> {
    let v = floats(obj, tags::IMAGE_ORIENTATION_PATIENT)?;
    if v.len() < 6 {
        return None;
    }
    let r = DVec3::new(v[0], v[1], v[2]);
    let c = DVec3::new(v[3], v[4], v[5]);
    (r.length() > 0.5 && c.length() > 0.5).then(|| (r.normalize(), c.normalize()))
}

/// DICOM stores (row spacing, column spacing); we return (dx, dy).
fn spacing(obj: &InMemDicomObject, tag: dicom_core::Tag) -> Option<(f64, f64)> {
    let v = floats(obj, tag)?;
    match v.as_slice() {
        [dy, dx, ..] if *dx > 0.0 && *dy > 0.0 => Some((*dx, *dy)),
        [d] if *d > 0.0 => Some((*d, *d)),
        _ => None,
    }
}

fn rescale(obj: &InMemDicomObject) -> Option<(f64, f64)> {
    let slope = float(obj, tags::RESCALE_SLOPE);
    let intercept = float(obj, tags::RESCALE_INTERCEPT);
    match (slope, intercept) {
        (None, None) => None,
        (s, i) => Some((s.filter(|s| *s != 0.0).unwrap_or(1.0), i.unwrap_or(0.0))),
    }
}

fn window(obj: &InMemDicomObject) -> Option<(f64, f64)> {
    let c = floats(obj, tags::WINDOW_CENTER)?.first().copied()?;
    let w = floats(obj, tags::WINDOW_WIDTH)?.first().copied()?;
    (w > 0.0).then_some((c, w))
}
