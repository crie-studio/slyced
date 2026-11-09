//! Curved planar reformation (CPR): a straightened image along a path clicked
//! on the MPR planes, to follow a vessel, the ureter, the appendix or a bowel
//! loop in a single image.

use super::volume::{next_id, Quant, Volume, OUTSIDE};
use glam::DVec3;
use rayon::prelude::*;
use std::sync::Arc;

pub struct CprImage {
    pub id: u64,
    pub data: Arc<Vec<i16>>,
    pub quant: Quant,
    /// Columns: across the path.
    pub width: usize,
    /// Rows: along the path, from the first clicked point.
    pub height: usize,
    /// Pixel size in mm (both directions).
    pub pixel: f64,
    /// Path point and across-direction of each row.
    pub centers: Vec<DVec3>,
    pub across: Vec<DVec3>,
    pub length_mm: f64,
}

impl CprImage {
    /// Patient position of image point `(x, y)` (pixel `i` spans `[i, i+1)`).
    pub fn patient(&self, x: f64, y: f64) -> Option<DVec3> {
        if y < 0.0 || self.height == 0 {
            return None;
        }
        let i = (y as usize).min(self.height - 1);
        let offset = (x - 0.5 - (self.width as f64 - 1.0) * 0.5) * self.pixel;
        Some(self.centers[i] + self.across[i] * offset)
    }

    /// Row closest to a patient point and its distance from the path (mm).
    pub fn nearest_row(&self, p: DVec3) -> Option<(usize, f64)> {
        self.centers
            .iter()
            .enumerate()
            .map(|(i, c)| (i, (*c - p).length()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

/// Uniform Catmull-Rom spline through `points`, `per_segment` samples each.
fn spline(points: &[DVec3], per_segment: usize) -> Vec<DVec3> {
    let n = points.len();
    let at = |i: isize| points[i.clamp(0, n as isize - 1) as usize];
    let mut out = Vec::with_capacity((n - 1) * per_segment + 1);
    for i in 0..n as isize - 1 {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        for s in 0..per_segment {
            let t = s as f64 / per_segment as f64;
            let (t2, t3) = (t * t, t * t * t);
            out.push(
                (p1 * 2.0 + (p2 - p0) * t + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * t2 + (p1 * 3.0 - p0 - p2 * 3.0 + p3) * t3) * 0.5,
            );
        }
    }
    out.push(points[n - 1]);
    out
}

/// Points every `step` mm along a polyline, with unit tangents.
fn resample(poly: &[DVec3], step: f64) -> (Vec<DVec3>, Vec<DVec3>, f64) {
    let mut cum = vec![0.0];
    for w in poly.windows(2) {
        cum.push(cum.last().unwrap() + (w[1] - w[0]).length());
    }
    let total = *cum.last().unwrap();
    let count = (total / step).floor() as usize + 1;
    let mut centers = Vec::with_capacity(count);
    let mut seg = 0;
    for k in 0..count {
        let s = k as f64 * step;
        while seg + 1 < cum.len() - 1 && cum[seg + 1] < s {
            seg += 1;
        }
        let len = (cum[seg + 1] - cum[seg]).max(1e-12);
        let t = ((s - cum[seg]) / len).clamp(0.0, 1.0);
        centers.push(poly[seg].lerp(poly[seg + 1], t));
    }
    let tangents = (0..centers.len())
        .map(|i| {
            let a = centers[i.saturating_sub(1)];
            let b = centers[(i + 1).min(centers.len() - 1)];
            (b - a).try_normalize().unwrap_or(DVec3::Z)
        })
        .collect();
    (centers, tangents, total)
}

/// Straightened CPR of `vol` along `points`. The image spans `half_width_mm`
/// on each side of the path, in the direction that lies in the plane where
/// the path was drawn (`plane_normal`), rotated by `angle_deg` about the path.
pub fn build(vol: &Volume, points: &[DVec3], plane_normal: DVec3, half_width_mm: f64, angle_deg: f64) -> Option<CprImage> {
    if points.len() < 2 {
        return None;
    }
    let pixel = vol.spacing.min_element().clamp(0.25, 1.5);
    let (centers, tangents, length_mm) = resample(&spline(points, 24), pixel);
    if centers.len() < 2 {
        return None;
    }
    // Across-direction transported along the path (no sudden flips).
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    let mut across = Vec::with_capacity(centers.len());
    // A path drawn through the slices (e.g. clicked on axial images) runs along the
    // plane normal: then the image spreads left-right, like a coronal view.
    let main = (points[points.len() - 1] - points[0]).try_normalize().unwrap_or(tangents[0]);
    let reference = if plane_normal.dot(main).abs() > 0.7 {
        [DVec3::Y, DVec3::X, DVec3::Z].into_iter().min_by(|a, b| a.dot(main).abs().total_cmp(&b.dot(main).abs())).unwrap_or(plane_normal)
    } else {
        plane_normal
    };
    let mut e = reference.cross(tangents[0]).try_normalize().unwrap_or_else(|| tangents[0].any_orthonormal_vector());
    for t in &tangents {
        e = (e - *t * e.dot(*t)).try_normalize().unwrap_or(e);
        across.push((e * cos + t.cross(e) * sin).normalize());
    }
    let width = (2.0 * half_width_mm / pixel).round() as usize | 1;
    let height = centers.len();
    let m = vol.to_voxel_matrix();
    let mut data = vec![OUTSIDE; width * height];
    data.par_chunks_mut(width).enumerate().for_each(|(i, row)| {
        let base = m * (centers[i] - vol.origin);
        let d = m * (across[i] * pixel);
        for (j, px) in row.iter_mut().enumerate() {
            let v = base + d * (j as f64 - (width as f64 - 1.0) * 0.5);
            if let Some(s) = vol.sample(v.x as f32, v.y as f32, v.z as f32) {
                *px = s.round().clamp(-32767.0, 32767.0) as i16;
            }
        }
    });
    Some(CprImage { id: next_id(), data: Arc::new(data), quant: vol.quant, width, height, pixel, centers, across, length_mm })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_path_samples_the_line() {
        // Value = 10 × x index; path along x through the middle of the volume.
        let dims = [40, 20, 20];
        let data = (0..dims[0] * dims[1] * dims[2]).map(|i| (i % dims[0]) as i16 * 10).collect();
        let vol = Volume::new(dims, DVec3::ONE, DVec3::ZERO, [DVec3::X, DVec3::Y, DVec3::Z], data, Quant::IDENTITY);
        let pts = [DVec3::new(5.0, 10.0, 10.0), DVec3::new(20.0, 10.0, 10.0), DVec3::new(35.0, 10.0, 10.0)];
        let cpr = build(&vol, &pts, DVec3::Z, 5.0, 0.0).unwrap();
        assert!((cpr.length_mm - 30.0).abs() < 1e-6);
        let mid = cpr.width / 2;
        for (i, c) in cpr.centers.iter().enumerate() {
            assert!((c.y - 10.0).abs() < 1e-9 && (c.z - 10.0).abs() < 1e-9);
            let v = cpr.data[i * cpr.width + mid];
            assert!((v as f64 - c.x * 10.0).abs() < 1.0, "linha {i}: {v} vs {}", c.x * 10.0);
        }
        // Across the path lies in the drawing plane (z constant).
        assert!(cpr.across[3].z.abs() < 1e-9);
        let p = cpr.patient(mid as f64 + 0.5, 4.5).unwrap();
        assert!((p - cpr.centers[4]).length() < 1e-9);
    }
}
