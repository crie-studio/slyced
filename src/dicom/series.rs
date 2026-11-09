//! A series with its pixels in memory, assembled into a 3D volume when the
//! geometry allows it.

use super::decode::{DecodedFrame, FramePixels, GrayData};
use super::model::{SeriesInfo, StudyHeader};
use crate::imaging::lut::{auto_window, Window};
use crate::imaging::volume::{Quant, Volume};
use glam::DVec3;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ImageInfo {
    pub path: PathBuf,
    pub instance_number: i32,
    pub frame: u32,
    pub width: usize,
    pub height: usize,
    pub position: Option<DVec3>,
    pub orientation: Option<(DVec3, DVec3)>,
    /// `(dx, dy)` in mm.
    pub spacing: (f64, f64),
    pub spacing_known: bool,
    pub thickness: Option<f64>,
    pub window: Option<Window>,
    pub invert: bool,
    pub rgb: bool,
}

pub enum FrameData {
    Gray(Vec<i16>, Quant),
    Rgb(Vec<u8>),
}

pub struct LoadedSeries {
    pub uid: String,
    pub info: SeriesInfo,
    pub study: StudyHeader,
    pub images: Vec<ImageInfo>,
    /// `None` when every image is a slice of `volume` (same order).
    pub frames: Option<Vec<FrameData>>,
    pub volume: Option<Arc<Volume>>,
    pub default_window: Window,
    pub warnings: Vec<String>,
}

pub enum Slice<'a> {
    Gray(&'a [i16], Quant),
    Rgb(&'a [u8]),
}

impl LoadedSeries {
    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn slice(&self, k: usize) -> Option<Slice<'_>> {
        match &self.frames {
            Some(frames) => frames.get(k).map(|f| match f {
                FrameData::Gray(d, q) => Slice::Gray(d, *q),
                FrameData::Rgb(d) => Slice::Rgb(d),
            }),
            None => {
                let v = self.volume.as_ref()?;
                (k < v.dims[2]).then(|| Slice::Gray(v.slice(k), v.quant))
            }
        }
    }

    pub fn memory_bytes(&self) -> usize {
        let frames: usize = self
            .frames
            .iter()
            .flatten()
            .map(|f| match f {
                FrameData::Gray(d, _) => d.len() * 2,
                FrameData::Rgb(d) => d.len(),
            })
            .sum();
        frames + self.volume.as_ref().map(|v| v.memory_bytes()).unwrap_or(0)
    }

    pub fn is_rgb(&self) -> bool {
        self.images.first().map(|i| i.rgb).unwrap_or(false)
    }

    pub fn build(info: SeriesInfo, study: StudyHeader, mut decoded: Vec<DecodedFrame>) -> Self {
        decoded.sort_by(|a, b| {
            a.instance_number
                .cmp(&b.instance_number)
                .then(a.frame_index.cmp(&b.frame_index))
                .then_with(|| a.path.cmp(&b.path))
        });
        let mut warnings = Vec::new();
        let plan = plan_volume(&decoded, &mut warnings);
        let modality = info.modality.clone();

        let mut series = match plan {
            Some(plan) if plan.order.len() == decoded.len() => {
                let images: Vec<ImageInfo> = plan.order.iter().map(|&i| image_info(&decoded[i])).collect();
                let volume = assemble_volume(&plan, decoded);
                LoadedSeries {
                    uid: info.uid.clone(),
                    info,
                    study,
                    images,
                    frames: None,
                    volume: Some(Arc::new(volume)),
                    default_window: Window::new(40.0, 400.0),
                    warnings,
                }
            }
            plan => {
                // Mixed series (e.g. a scout among axial slices): keep every image for
                // 2D review and build the volume from the largest consistent group.
                let volume = plan.map(|p| {
                    warnings.push(format!(
                        "Volume 3D montado com {} de {} imagens (as demais têm geometria diferente).",
                        p.order.len(),
                        decoded.len()
                    ));
                    Arc::new(assemble_volume_borrowed(&p, &decoded))
                });
                let images = decoded.iter().map(image_info).collect();
                let frames = decoded
                    .into_iter()
                    .map(|f| match f.pixels {
                        FramePixels::Rgb(d) => FrameData::Rgb(d),
                        FramePixels::Gray(GrayData::I16(d)) => FrameData::Gray(d, Quant::IDENTITY),
                        FramePixels::Gray(GrayData::F32(d)) => {
                            let (q, data) = quantize(&[&d]);
                            FrameData::Gray(data.into_iter().next().unwrap_or_default(), q)
                        }
                    })
                    .collect();
                LoadedSeries {
                    uid: info.uid.clone(),
                    info,
                    study,
                    images,
                    frames: Some(frames),
                    volume,
                    default_window: Window::new(40.0, 400.0),
                    warnings,
                }
            }
        };
        series.default_window = series.compute_default_window(&modality);
        series
    }

    fn compute_default_window(&self, modality: &str) -> Window {
        if self.is_rgb() {
            return Window::new(127.5, 255.0);
        }
        let mid = self.len() / 2;
        if let Some(w) = self.images.get(mid).and_then(|i| i.window) {
            return w;
        }
        if modality == "CT" {
            return Window::new(40.0, 400.0);
        }
        match self.slice(mid) {
            Some(Slice::Gray(d, q)) => auto_window(d, q),
            _ => Window::new(127.5, 255.0),
        }
    }
}

fn image_info(f: &DecodedFrame) -> ImageInfo {
    let g = &f.geometry;
    ImageInfo {
        path: f.path.clone(),
        instance_number: f.instance_number,
        frame: f.frame_index,
        width: f.width,
        height: f.height,
        position: g.position,
        orientation: g.orientation,
        spacing: g.spacing.unwrap_or((1.0, 1.0)),
        spacing_known: g.spacing.is_some(),
        thickness: g.thickness,
        window: g.window.map(|(c, w)| Window::new(c as f32, w as f32)),
        invert: f.invert,
        rgb: matches!(f.pixels, FramePixels::Rgb(_)),
    }
}

struct VolumePlan {
    /// Indices into the decoded frames, in slice order.
    order: Vec<usize>,
    dims: [usize; 3],
    spacing: DVec3,
    origin: DVec3,
    axes: [DVec3; 3],
}

fn plan_volume(frames: &[DecodedFrame], warnings: &mut Vec<String>) -> Option<VolumePlan> {
    // Group gray frames with full geometry by size and orientation; use the largest group.
    let mut groups: Vec<(usize, usize, (DVec3, DVec3), Vec<usize>)> = Vec::new();
    for (i, f) in frames.iter().enumerate() {
        if !matches!(f.pixels, FramePixels::Gray(_)) || f.geometry.position.is_none() {
            continue;
        }
        let Some(o) = f.geometry.orientation else { continue };
        match groups.iter_mut().find(|g| {
            g.0 == f.width && g.1 == f.height && g.2.0.dot(o.0) > 0.9999 && g.2.1.dot(o.1) > 0.9999
        }) {
            Some(g) => g.3.push(i),
            None => groups.push((f.width, f.height, o, vec![i])),
        }
    }
    let (w, h, (row, col), members) = groups.into_iter().max_by_key(|g| g.3.len())?;
    if members.len() < 3 {
        return None;
    }
    let normal = row.cross(col).normalize();
    let mut ordered: Vec<(f64, usize)> =
        members.iter().map(|&i| (frames[i].geometry.position.unwrap().dot(normal), i)).collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0));

    let gaps: Vec<f64> = ordered.windows(2).map(|w| w[1].0 - w[0].0).collect();
    let min_gap = gaps.iter().cloned().fold(f64::MAX, f64::min);
    if min_gap < 1e-3 {
        // Repeated positions: multi-phase / multi-echo stacks are shown in 2D only.
        warnings.push("Posições de corte repetidas (múltiplas fases/ecos): reconstruções 3D/MPR indisponíveis.".into());
        return None;
    }
    let max_gap = gaps.iter().cloned().fold(0.0, f64::max);
    if max_gap / min_gap > 1.5 {
        warnings.push("Espaçamento irregular entre cortes: as reconstruções podem ficar distorcidas.".into());
    }

    // Keep acquisition order when instance numbers run against the spatial order,
    // so that image 1 stays first in the stack.
    let first_inst = frames[ordered[0].1].instance_number;
    let last_inst = frames[ordered[ordered.len() - 1].1].instance_number;
    if first_inst > last_inst {
        ordered.reverse();
    }
    let order: Vec<usize> = ordered.iter().map(|o| o.1).collect();
    let p0 = frames[order[0]].geometry.position.unwrap();
    let p1 = frames[*order.last().unwrap()].geometry.position.unwrap();
    let span = p1 - p0;
    let dz = span.length() / (order.len() - 1) as f64;
    if dz.abs() < 1e-6 {
        return None;
    }
    if span.normalize().dot(normal).abs() < 0.97 {
        warnings.push("Aquisição com inclinação do gantry: geometria aproximada.".into());
    }
    let (dx, dy) = frames[order[0]].geometry.spacing.unwrap_or((1.0, 1.0));
    Some(VolumePlan {
        dims: [w, h, order.len()],
        spacing: DVec3::new(dx, dy, dz),
        origin: p0,
        axes: [row, col, span.normalize()],
        order,
    })
}

fn gray_of(f: &DecodedFrame) -> &GrayData {
    match &f.pixels {
        FramePixels::Gray(g) => g,
        FramePixels::Rgb(_) => unreachable!("volume plans only include gray frames"),
    }
}

fn assemble_volume(plan: &VolumePlan, decoded: Vec<DecodedFrame>) -> Volume {
    let all_i16 = plan.order.iter().all(|&i| matches!(gray_of(&decoded[i]), GrayData::I16(_)));
    if !all_i16 {
        return assemble_volume_borrowed(plan, &decoded);
    }
    let mut slots: Vec<Option<DecodedFrame>> = decoded.into_iter().map(Some).collect();
    let mut data = Vec::with_capacity(plan.dims[0] * plan.dims[1] * plan.dims[2]);
    for &i in &plan.order {
        if let Some(DecodedFrame { pixels: FramePixels::Gray(GrayData::I16(d)), .. }) = slots[i].take() {
            data.extend_from_slice(&d);
        }
    }
    Volume::new(plan.dims, plan.spacing, plan.origin, plan.axes, data, Quant::IDENTITY)
}

fn assemble_volume_borrowed(plan: &VolumePlan, decoded: &[DecodedFrame]) -> Volume {
    let f32_slices: Vec<Vec<f32>> = plan
        .order
        .iter()
        .map(|&i| match gray_of(&decoded[i]) {
            GrayData::I16(d) => d.iter().map(|&v| v as f32).collect(),
            GrayData::F32(d) => d.clone(),
        })
        .collect();
    let refs: Vec<&[f32]> = f32_slices.iter().map(|v| v.as_slice()).collect();
    let (quant, slices) = quantize(&refs);
    let data: Vec<i16> = slices.into_iter().flatten().collect();
    Volume::new(plan.dims, plan.spacing, plan.origin, plan.axes, data, quant)
}

/// Shared quantization for a set of real-valued slices.
fn quantize(slices: &[&[f32]]) -> (Quant, Vec<Vec<i16>>) {
    let (mut lo, mut hi, mut integral) = (f64::MAX, f64::MIN, true);
    for s in slices {
        for &v in s.iter() {
            let v = v as f64;
            lo = lo.min(v);
            hi = hi.max(v);
            integral &= v.fract() == 0.0;
        }
    }
    if lo > hi {
        return (Quant::IDENTITY, slices.iter().map(|s| vec![0; s.len()]).collect());
    }
    let q = Quant::for_range(lo, hi, integral);
    (q, slices.iter().map(|s| s.iter().map(|&v| q.quantize(v)).collect()).collect())
}
