//! Multiplanar reformatting: planar resampling of a [`Volume`] on the CPU.

use super::register::{Rigid, Sim2};
use super::volume::{Volume, OUTSIDE};
use glam::{DVec2, DVec3, Vec3};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PlaneKind {
    Axial,
    Coronal,
    Sagittal,
}

impl PlaneKind {
    pub const ALL: [PlaneKind; 3] = [PlaneKind::Axial, PlaneKind::Coronal, PlaneKind::Sagittal];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            PlaneKind::Axial => "AXIAL",
            PlaneKind::Coronal => "CORONAL",
            PlaneKind::Sagittal => tr!("SAGITAL", "SAGITTAL"),
        }
    }

    /// Screen-right and screen-down directions in patient (LPS) space, following
    /// the radiological display convention.
    pub fn axes(self) -> (DVec3, DVec3) {
        match self {
            PlaneKind::Axial => (DVec3::X, DVec3::Y),
            PlaneKind::Coronal => (DVec3::X, -DVec3::Z),
            PlaneKind::Sagittal => (DVec3::Y, -DVec3::Z),
        }
    }

    /// Accent color used for crosshairs and labels (3D Slicer convention).
    pub fn color(self) -> egui::Color32 {
        match self {
            PlaneKind::Axial => egui::Color32::from_rgb(242, 92, 84),
            PlaneKind::Coronal => egui::Color32::from_rgb(92, 214, 120),
            PlaneKind::Sagittal => egui::Color32::from_rgb(246, 200, 76),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SlabMode {
    #[default]
    Mip,
    MinIp,
    Avg,
}

impl SlabMode {
    pub fn label(self) -> &'static str {
        match self {
            SlabMode::Mip => "MIP",
            SlabMode::MinIp => "MinIP",
            SlabMode::Avg => tr!("Média", "Average"),
        }
    }
}

/// A stack of parallel sampling planes covering a volume.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaneGrid {
    pub u: DVec3,
    pub v: DVec3,
    pub n: DVec3,
    /// Patient position of pixel (0, 0) on slice 0.
    pub origin: DVec3,
    /// In-plane pixel size (mm).
    pub pixel: f64,
    /// Distance between slices (mm).
    pub step: f64,
    pub width: usize,
    pub height: usize,
    pub slices: usize,
}

impl PlaneGrid {
    pub fn standard(vol: &Volume, kind: PlaneKind) -> Self {
        let (u, v) = kind.axes();
        Self::from_axes(vol, u, v)
    }

    pub fn from_axes(vol: &Volume, u: DVec3, v: DVec3) -> Self {
        let u = u.normalize();
        let v = (v - u * u.dot(v)).normalize();
        let n = u.cross(v);
        let corners = vol.corners();
        let span = |d: DVec3| {
            corners
                .iter()
                .map(|c| c.dot(d))
                .fold((f64::MAX, f64::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)))
        };
        let (u0, u1) = span(u);
        let (v0, v1) = span(v);
        let (n0, n1) = span(n);

        let mut pixel = vol.spacing.min_element().max(0.05);
        pixel = pixel.max((u1 - u0) / 2047.0).max((v1 - v0) / 2047.0);
        let best = (0..3)
            .max_by(|&a, &b| n.dot(vol.axes[a]).abs().total_cmp(&n.dot(vol.axes[b]).abs()))
            .unwrap_or(2);
        let step = vol.spacing[best].max(0.05);

        Self {
            u,
            v,
            n,
            origin: u * u0 + v * v0 + n * n0,
            pixel,
            step,
            width: ((u1 - u0) / pixel).round() as usize + 1,
            height: ((v1 - v0) / pixel).round() as usize + 1,
            slices: ((n1 - n0) / step).round() as usize + 1,
        }
    }

    /// Patient position of the (continuous) pixel center `(x, y)` on slice `k`.
    pub fn point(&self, x: f64, y: f64, k: f64) -> DVec3 {
        self.origin + self.u * (x * self.pixel) + self.v * (y * self.pixel) + self.n * (k * self.step)
    }

    /// Inverse of [`PlaneGrid::point`]: returns `(x, y, k)`.
    pub fn locate(&self, p: DVec3) -> DVec3 {
        let d = p - self.origin;
        DVec3::new(d.dot(self.u) / self.pixel, d.dot(self.v) / self.pixel, d.dot(self.n) / self.step)
    }
}

/// Resamples slice `k` of `grid` into `out` (stored units, [`OUTSIDE`] where no data).
/// With `slab_mm > 0` several parallel planes are combined with `mode`.
pub fn reslice(vol: &Volume, grid: &PlaneGrid, k: f64, slab_mm: f64, mode: SlabMode, out: &mut Vec<i16>) {
    reslice_mapped(vol, grid, k, slab_mm, mode, &Rigid::IDENTITY, out);
}

/// [`reslice`] of a grid defined in another exam's space: each grid point `p`
/// is sampled at `map(p)` in `vol` (used to show exam B in exam A's planes).
pub fn reslice_mapped(vol: &Volume, grid: &PlaneGrid, k: f64, slab_mm: f64, mode: SlabMode, map: &Rigid, out: &mut Vec<i16>) {
    out.clear();
    out.resize(grid.width * grid.height, OUTSIDE);
    let m = vol.to_voxel_matrix();
    let to_vec3 = |d: DVec3| Vec3::new(d.x as f32, d.y as f32, d.z as f32);
    let base = to_vec3(m * (map.apply(grid.point(0.0, 0.0, k)) - vol.origin));
    let dx = to_vec3(m * map.apply_dir(grid.u * grid.pixel));
    let dy = to_vec3(m * map.apply_dir(grid.v * grid.pixel));

    let fine = vol.spacing.min_element().max(0.05);
    let samples = if slab_mm > fine { ((slab_mm / fine).ceil() as usize + 1).clamp(2, 512) } else { 1 };
    let offsets: Vec<Vec3> = (0..samples)
        .map(|i| {
            let t = if samples == 1 { 0.0 } else { -slab_mm * 0.5 + slab_mm * i as f64 / (samples - 1) as f64 };
            to_vec3(m * map.apply_dir(grid.n * t))
        })
        .collect();

    out.par_chunks_mut(grid.width).enumerate().for_each(|(y, row)| {
        let rb = base + dy * y as f32;
        for (x, px) in row.iter_mut().enumerate() {
            let p = rb + dx * x as f32;
            let value = if samples == 1 {
                vol.sample(p.x, p.y, p.z)
            } else {
                combine(vol, p, &offsets, mode)
            };
            if let Some(s) = value {
                *px = s.round().clamp(-32767.0, 32767.0) as i16;
            }
        }
    });
}

/// Resamples a 2D image onto another pixel grid: destination pixel centers
/// (in mm) are mapped by `map` into source millimetres (bilinear, gray only).
pub fn resample_2d(src: &[i16], (sw, sh): (usize, usize), (sdx, sdy): (f64, f64), (dw, dh): (usize, usize), (ddx, ddy): (f64, f64), map: &Sim2) -> Vec<i16> {
    let mut out = vec![OUTSIDE; dw * dh];
    if src.len() != sw * sh || sw == 0 || sh == 0 {
        return out;
    }
    out.par_chunks_mut(dw).enumerate().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let q = map.apply(DVec2::new((x as f64 + 0.5) * ddx, (y as f64 + 0.5) * ddy));
            let (fx, fy) = (q.x / sdx - 0.5, q.y / sdy - 0.5);
            if fx < -0.5 || fy < -0.5 || fx > sw as f64 - 0.5 || fy > sh as f64 - 0.5 {
                continue;
            }
            let (fx, fy) = (fx.clamp(0.0, (sw - 1) as f64), fy.clamp(0.0, (sh - 1) as f64));
            let (x0, y0) = (fx as usize, fy as usize);
            let (x1, y1) = ((x0 + 1).min(sw - 1), (y0 + 1).min(sh - 1));
            let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
            let at = |xx: usize, yy: usize| src[yy * sw + xx];
            let corners = [at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1)];
            if corners.contains(&OUTSIDE) {
                *px = at(fx.round() as usize, fy.round() as usize);
                continue;
            }
            let top = corners[0] as f64 * (1.0 - tx) + corners[1] as f64 * tx;
            let bottom = corners[2] as f64 * (1.0 - tx) + corners[3] as f64 * tx;
            *px = (top * (1.0 - ty) + bottom * ty).round().clamp(-32767.0, 32767.0) as i16;
        }
    });
    out
}

#[inline]
fn combine(vol: &Volume, p: Vec3, offsets: &[Vec3], mode: SlabMode) -> Option<f32> {
    let mut acc: Option<f32> = None;
    let mut count = 0u32;
    for o in offsets {
        let q = p + *o;
        if let Some(s) = vol.sample(q.x, q.y, q.z) {
            count += 1;
            acc = Some(match (acc, mode) {
                (None, _) => s,
                (Some(a), SlabMode::Mip) => a.max(s),
                (Some(a), SlabMode::MinIp) => a.min(s),
                (Some(a), SlabMode::Avg) => a + s,
            });
        }
    }
    match mode {
        SlabMode::Avg => acc.map(|a| a / count as f32),
        _ => acc,
    }
}

/// Anatomical label (R/L, A/P, S/I) for a direction in patient space; oblique
/// directions get up to two letters ordered by dominance.
pub fn orientation_label(d: DVec3) -> String {
    let parts = [
        (d.x, if d.x > 0.0 { 'L' } else { 'R' }),
        (d.y, if d.y > 0.0 { 'P' } else { 'A' }),
        (d.z, if d.z > 0.0 { 'S' } else { 'I' }),
    ];
    let mut parts: Vec<_> = parts.into_iter().filter(|(c, _)| c.abs() > 0.25).collect();
    parts.sort_by(|a, b| b.0.abs().total_cmp(&a.0.abs()));
    parts.into_iter().take(2).map(|(_, c)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imaging::volume::{Quant, Volume};

    fn volume() -> Volume {
        let dims = [6, 5, 4];
        let data = (0..dims[0] * dims[1] * dims[2]).map(|i| i as i16).collect();
        Volume::new(dims, DVec3::ONE, DVec3::ZERO, [DVec3::X, DVec3::Y, DVec3::Z], data, Quant::IDENTITY)
    }

    #[test]
    fn axial_reslice_matches_native_slice() {
        let v = volume();
        let g = PlaneGrid::standard(&v, PlaneKind::Axial);
        assert_eq!((g.width, g.height, g.slices), (6, 5, 4));
        let mut out = Vec::new();
        reslice(&v, &g, 2.0, 0.0, SlabMode::Mip, &mut out);
        assert_eq!(out, v.slice(2));
    }

    #[test]
    fn coronal_grid_covers_volume_and_slab_mip_is_max() {
        let v = volume();
        let g = PlaneGrid::standard(&v, PlaneKind::Coronal);
        assert_eq!((g.width, g.height, g.slices), (6, 4, 5));
        let (mut thin, mut slab) = (Vec::new(), Vec::new());
        reslice(&v, &g, 2.0, 0.0, SlabMode::Mip, &mut thin);
        reslice(&v, &g, 2.0, 2.0, SlabMode::Mip, &mut slab);
        assert!(slab.iter().zip(&thin).all(|(s, t)| s >= t));
        let p = DVec3::new(2.0, 3.0, 1.0);
        let l = g.locate(p);
        assert!((g.point(l.x, l.y, l.z) - p).length() < 1e-9);
    }

    #[test]
    fn mapped_reslice_follows_transform() {
        let v = volume();
        let g = PlaneGrid::standard(&v, PlaneKind::Axial);
        // Grid in a space shifted by one voxel along x: sampling there equals x + 1.
        let map = Rigid::translation(DVec3::X);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        reslice(&v, &g, 1.0, 0.0, SlabMode::Mip, &mut a);
        reslice_mapped(&v, &g, 1.0, 0.0, SlabMode::Mip, &map, &mut b);
        assert_eq!(b[0], a[1]);
        assert_eq!(b[5], OUTSIDE);
    }

    #[test]
    fn resample_2d_identity_and_scale() {
        let src: Vec<i16> = (0..12).collect();
        let same = resample_2d(&src, (4, 3), (1.0, 1.0), (4, 3), (1.0, 1.0), &Sim2::IDENTITY);
        assert_eq!(same, src);
        // Destination with half-size pixels: every second pixel center hits a source edge.
        let fine = resample_2d(&src, (4, 3), (1.0, 1.0), (8, 6), (0.5, 0.5), &Sim2::IDENTITY);
        assert_eq!(fine.len(), 48);
        assert_eq!(fine[0], 0);
    }

    #[test]
    fn orientation_labels() {
        assert_eq!(orientation_label(DVec3::X), "L");
        assert_eq!(orientation_label(-DVec3::Y), "A");
        assert_eq!(orientation_label(DVec3::new(0.7, 0.0, -0.7)), "LI");
    }
}
