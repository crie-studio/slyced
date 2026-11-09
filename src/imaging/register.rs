//! Alignment between two exams (e.g. a follow-up and its prior): rigid
//! transforms from paired landmarks or found automatically by mutual
//! information, and 2D similarity transforms for radiographs.

use super::volume::Volume;
use glam::{DMat3, DQuat, DVec2, DVec3, EulerRot};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Rigid transform `p ↦ r·p + t` mapping patient coordinates of exam A to the
/// patient coordinates of the same anatomy in exam B.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rigid {
    pub r: DMat3,
    pub t: DVec3,
}

impl Default for Rigid {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Rigid {
    pub const IDENTITY: Self = Self { r: DMat3::IDENTITY, t: DVec3::ZERO };

    pub fn translation(t: DVec3) -> Self {
        Self { r: DMat3::IDENTITY, t }
    }

    #[inline]
    pub fn apply(&self, p: DVec3) -> DVec3 {
        self.r * p + self.t
    }

    #[inline]
    pub fn apply_dir(&self, d: DVec3) -> DVec3 {
        self.r * d
    }

    pub fn inverse(&self) -> Self {
        let ri = self.r.transpose();
        Self { r: ri, t: -(ri * self.t) }
    }

    /// `self ∘ other`: applies `other` first.
    pub fn compose(&self, other: &Rigid) -> Rigid {
        Rigid { r: self.r * other.r, t: self.r * other.t + self.t }
    }

    /// Rotation angle in degrees.
    pub fn angle_deg(&self) -> f64 {
        let c = ((self.r.x_axis.x + self.r.y_axis.y + self.r.z_axis.z - 1.0) * 0.5).clamp(-1.0, 1.0);
        c.acos().to_degrees()
    }

    /// Rotation by Euler angles (radians) about `center`, then translation.
    fn from_params(p: &[f64; 6], center: DVec3) -> Rigid {
        let r = DMat3::from_euler(EulerRot::XYZ, p[3], p[4], p[5]);
        Rigid { r, t: center - r * center + DVec3::new(p[0], p[1], p[2]) }
    }
}

fn mean(points: impl Iterator<Item = DVec3>) -> DVec3 {
    let (sum, n) = points.fold((DVec3::ZERO, 0usize), |(s, n), p| (s + p, n + 1));
    sum / n.max(1) as f64
}

/// Eigen-decomposition of a symmetric matrix (cyclic Jacobi). Returns the
/// eigenvalues and the eigenvectors as columns.
fn jacobi<const N: usize>(mut a: [[f64; N]; N]) -> ([f64; N], [[f64; N]; N]) {
    let mut v = [[0.0; N]; N];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..64 {
        let off: f64 = (0..N).flat_map(|i| (0..N).filter(move |&j| j != i).map(move |j| (i, j))).map(|(i, j)| a[i][j] * a[i][j]).sum();
        if off < 1e-22 {
            break;
        }
        for p in 0..N {
            for q in p + 1..N {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = if theta == 0.0 { 1.0 } else { theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt()) };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..N {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                for k in 0..N {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let (vkp, vkq) = (row[p], row[q]);
                    row[p] = c * vkp - s * vkq;
                    row[q] = s * vkp + c * vkq;
                }
            }
        }
    }
    let mut vals = [0.0; N];
    for (i, val) in vals.iter_mut().enumerate() {
        *val = a[i][i];
    }
    (vals, v)
}

/// Least-squares rigid transform taking each `a` onto its paired `b`.
/// One or two pairs (or points on a line) give a translation; three or more
/// spread-out pairs give rotation + translation (Horn's quaternion method).
pub fn from_landmarks(pairs: &[(DVec3, DVec3)]) -> Option<Rigid> {
    if pairs.is_empty() {
        return None;
    }
    let ca = mean(pairs.iter().map(|p| p.0));
    let cb = mean(pairs.iter().map(|p| p.1));
    let translation = Rigid::translation(cb - ca);
    if pairs.len() < 3 {
        return Some(translation);
    }
    // Collinear landmarks leave the rotation about their line undetermined.
    let mut scatter = [[0.0; 3]; 3];
    let mut s = [[0.0; 3]; 3];
    for (a, b) in pairs {
        let (da, db) = ((*a - ca).to_array(), (*b - cb).to_array());
        for i in 0..3 {
            for j in 0..3 {
                scatter[i][j] += da[i] * da[j];
                s[i][j] += da[i] * db[j];
            }
        }
    }
    let (ev, _) = jacobi(scatter);
    let mut ev = ev;
    ev.sort_by(|x, y| y.total_cmp(x));
    if ev[1] < 1.0 || ev[1] < ev[0] * 1e-4 {
        return Some(translation);
    }
    let [[sxx, sxy, sxz], [syx, syy, syz], [szx, szy, szz]] = s;
    let n = [
        [sxx + syy + szz, syz - szy, szx - sxz, sxy - syx],
        [syz - szy, sxx - syy - szz, sxy + syx, szx + sxz],
        [szx - sxz, sxy + syx, -sxx + syy - szz, syz + szy],
        [sxy - syx, szx + sxz, syz + szy, -sxx - syy + szz],
    ];
    let (vals, vecs) = jacobi(n);
    let best = (0..4).max_by(|&i, &j| vals[i].total_cmp(&vals[j]))?;
    let q = DQuat::from_xyzw(vecs[1][best], vecs[2][best], vecs[3][best], vecs[0][best]).normalize();
    let r = DMat3::from_quat(q);
    Some(Rigid { r, t: cb - r * ca })
}

/// 2D similarity `p ↦ [a −b; b a]·p + t` (uniform scale, rotation, translation),
/// in millimetres of the image plane. Maps image A onto image B.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sim2 {
    pub a: f64,
    pub b: f64,
    pub t: DVec2,
}

impl Default for Sim2 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Sim2 {
    pub const IDENTITY: Self = Self { a: 1.0, b: 0.0, t: DVec2::ZERO };

    #[inline]
    pub fn apply(&self, p: DVec2) -> DVec2 {
        DVec2::new(self.a * p.x - self.b * p.y, self.b * p.x + self.a * p.y) + self.t
    }

    pub fn scale(&self) -> f64 {
        self.a.hypot(self.b)
    }

    pub fn angle_deg(&self) -> f64 {
        self.b.atan2(self.a).to_degrees()
    }
}

/// Least-squares similarity from paired points (one pair: translation only).
pub fn similarity_2d(pairs: &[(DVec2, DVec2)]) -> Option<Sim2> {
    if pairs.is_empty() {
        return None;
    }
    let n = pairs.len() as f64;
    let ca = pairs.iter().map(|p| p.0).sum::<DVec2>() / n;
    let cb = pairs.iter().map(|p| p.1).sum::<DVec2>() / n;
    if pairs.len() == 1 {
        return Some(Sim2 { t: cb - ca, ..Sim2::IDENTITY });
    }
    let (mut num_a, mut num_b, mut den) = (0.0, 0.0, 0.0);
    for (a, b) in pairs {
        let (da, db) = (*a - ca, *b - cb);
        num_a += da.dot(db);
        num_b += da.x * db.y - da.y * db.x;
        den += da.length_squared();
    }
    if den < 1e-9 {
        return Some(Sim2 { t: cb - ca, ..Sim2::IDENTITY });
    }
    let (a, b) = (num_a / den, num_b / den);
    let m = Sim2 { a, b, t: DVec2::ZERO };
    Some(Sim2 { a, b, t: cb - m.apply(ca) })
}

/// Center of the patient's body (voxels denser than air), used as a first
/// alignment between exams acquired in different positions.
pub fn body_center(vol: &Volume) -> Option<DVec3> {
    let (lo, _) = vol.real_range();
    // CT: air is about −1000 HU; other modalities: anything above the mean.
    let stride = 2;
    let threshold = if lo < -500.0 {
        -400.0
    } else {
        let (sum, n) = vol.data.iter().step_by(7).fold((0.0f64, 0usize), |(s, n), &v| (s + vol.quant.real(v) as f64, n + 1));
        (sum / n.max(1) as f64) as f32
    };
    let [nx, ny, nz] = vol.dims;
    let (sum, n) = (0..nz)
        .into_par_iter()
        .step_by(stride)
        .map(|z| {
            let mut acc = (DVec3::ZERO, 0usize);
            for y in (0..ny).step_by(stride) {
                for x in (0..nx).step_by(stride) {
                    let s = vol.data[z * nx * ny + y * nx + x];
                    if s != super::volume::OUTSIDE && vol.quant.real(s) > threshold {
                        acc.0 += DVec3::new(x as f64, y as f64, z as f64);
                        acc.1 += 1;
                    }
                }
            }
            acc
        })
        .reduce(|| (DVec3::ZERO, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    (n > 0).then(|| vol.voxel_to_patient(sum / n as f64))
}

pub struct AutoResult {
    pub transform: Rigid,
    /// Normalized mutual information of the result (1 = independent, 2 = identical).
    pub nmi: f64,
}

const BINS: usize = 32;

/// Intensity range (1st–99th percentile) mapped onto the histogram bins.
fn bin_range(vol: &Volume) -> (f32, f32) {
    let mut v: Vec<i16> = vol.data.iter().step_by(11).copied().filter(|&s| s != super::volume::OUTSIDE).collect();
    if v.is_empty() {
        return (0.0, 1.0);
    }
    v.sort_unstable();
    let lo = vol.quant.real(v[v.len() / 100]);
    let hi = vol.quant.real(v[v.len() * 99 / 100]);
    (lo, hi.max(lo + 1.0))
}

#[inline]
fn bin(v: f32, (lo, hi): (f32, f32)) -> usize {
    (((v - lo) / (hi - lo)) * BINS as f32).clamp(0.0, BINS as f32 - 1.0) as usize
}

fn coarse(vol: &Volume, mm: f64) -> Volume {
    let f = |s: f64| ((mm / s).round() as usize).max(1);
    vol.downsampled([f(vol.spacing.x), f(vol.spacing.y), f(vol.spacing.z)])
}

/// Finds the rigid transform aligning `moving` (exam B) to `fixed` (exam A)
/// by maximizing normalized mutual information, starting from `init`.
/// Works between modalities and window settings; returns `None` if cancelled.
pub fn auto_register(fixed: &Volume, moving: &Volume, init: Rigid, cancel: &AtomicBool) -> Option<AutoResult> {
    let extent = fixed.extent_mm().min_element().min(moving.extent_mm().min_element());
    let coarse_mm = (extent / 40.0).clamp(3.0, 8.0);
    let finest = fixed.spacing.min_element().max(moving.spacing.min_element());
    let levels = [coarse_mm, (coarse_mm * 0.5).max(finest)];
    let mut current = init;
    let mut nmi = 0.0;
    for (li, &mm) in levels.iter().enumerate() {
        let f = coarse(fixed, mm);
        let m = coarse(moving, mm);
        let (fr, mr) = (bin_range(&f), bin_range(&m));
        // Sample points: fixed voxels inside the body, at most ~40k.
        let body = fr.0 + (fr.1 - fr.0) * 0.05;
        let total = f.data.len();
        let stride = (total / 40_000).max(1);
        let points: Vec<(DVec3, u8)> = (0..total)
            .step_by(stride)
            .filter_map(|i| {
                let s = f.data[i];
                let v = f.quant.real(s);
                if s == super::volume::OUTSIDE || v <= body {
                    return None;
                }
                let [nx, ny, _] = f.dims;
                let p = f.voxel_to_patient(DVec3::new((i % nx) as f64, ((i / nx) % ny) as f64, (i / (nx * ny)) as f64));
                Some((p, bin(v, fr) as u8))
            })
            .collect();
        if points.len() < 200 {
            return None;
        }
        let center = mean(points.iter().map(|p| p.0));
        let to_voxel = m.to_voxel_matrix();
        let score = |params: &[f64; 6]| -> f64 {
            let t = current.compose(&Rigid::from_params(params, center));
            let (hist, inside) = points
                .par_chunks(2048)
                .map(|chunk| {
                    let mut h = vec![0u32; BINS * BINS];
                    let mut n = 0usize;
                    for (p, ba) in chunk {
                        let v = to_voxel * (t.apply(*p) - m.origin);
                        if let Some(s) = m.sample(v.x as f32, v.y as f32, v.z as f32) {
                            h[*ba as usize * BINS + bin(m.quant.real_f(s), mr)] += 1;
                            n += 1;
                        }
                    }
                    (h, n)
                })
                .reduce(
                    || (vec![0u32; BINS * BINS], 0),
                    |mut a, b| {
                        a.0.iter_mut().zip(&b.0).for_each(|(x, y)| *x += y);
                        (a.0, a.1 + b.1)
                    },
                );
            if inside < points.len() / 4 {
                return 0.0;
            }
            let n = inside as f64;
            let mut pa = [0u32; BINS];
            let mut pb = [0u32; BINS];
            for i in 0..BINS {
                for j in 0..BINS {
                    pa[i] += hist[i * BINS + j];
                    pb[j] += hist[i * BINS + j];
                }
            }
            let entropy = |counts: &[u32]| -> f64 {
                counts.iter().filter(|&&c| c > 0).map(|&c| c as f64 / n).map(|p| -p * p.ln()).sum()
            };
            let hab = entropy(&hist);
            if hab <= 0.0 {
                return 0.0;
            }
            (entropy(&pa) + entropy(&pb)) / hab
        };

        let scale = if li == 0 { 1.0 } else { 0.5 };
        let mut step = [8.0 * scale, 8.0 * scale, 8.0 * scale, 0.07 * scale, 0.07 * scale, 0.07 * scale];
        let min_step = [0.3, 0.3, 0.3, 0.003, 0.003, 0.003];
        let mut params = [0.0f64; 6];
        let mut best = score(&params);
        let mut evals = 0;
        while evals < 900 {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let mut improved = false;
            for i in 0..6 {
                for sign in [1.0, -1.0] {
                    let mut trial = params;
                    trial[i] += sign * step[i];
                    let s = score(&trial);
                    evals += 1;
                    if s > best + 1e-7 {
                        best = s;
                        params = trial;
                        improved = true;
                        break;
                    }
                }
            }
            if !improved {
                if step.iter().zip(&min_step).all(|(s, m)| s <= m) {
                    break;
                }
                step.iter_mut().for_each(|s| *s *= 0.5);
            }
        }
        current = current.compose(&Rigid::from_params(&params, center));
        nmi = best;
    }
    Some(AutoResult { transform: current, nmi })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imaging::volume::Quant;

    fn rot_z(deg: f64) -> DMat3 {
        DMat3::from_rotation_z(deg.to_radians())
    }

    #[test]
    fn landmarks_recover_rigid_transform() {
        let truth = Rigid { r: rot_z(12.0) * DMat3::from_rotation_x(0.1), t: DVec3::new(5.0, -3.0, 20.0) };
        let pts = [DVec3::new(0.0, 0.0, 0.0), DVec3::new(100.0, 0.0, 10.0), DVec3::new(0.0, 80.0, -5.0), DVec3::new(30.0, 40.0, 60.0)];
        let pairs: Vec<_> = pts.iter().map(|&p| (p, truth.apply(p))).collect();
        let r = from_landmarks(&pairs).unwrap();
        for p in pts {
            assert!((r.apply(p) - truth.apply(p)).length() < 1e-6);
        }
        assert!((r.inverse().apply(truth.apply(pts[1])) - pts[1]).length() < 1e-6);
        // One pair: translation only.
        let one = from_landmarks(&pairs[..1]).unwrap();
        assert_eq!(one.r, DMat3::IDENTITY);
        assert!((one.apply(pts[0]) - pairs[0].1).length() < 1e-9);
    }

    #[test]
    fn similarity_from_two_points() {
        let truth = Sim2 { a: 1.2 * 0.3f64.cos(), b: 1.2 * 0.3f64.sin(), t: DVec2::new(4.0, -7.0) };
        let a = [DVec2::new(10.0, 10.0), DVec2::new(60.0, 30.0)];
        let s = similarity_2d(&[(a[0], truth.apply(a[0])), (a[1], truth.apply(a[1]))]).unwrap();
        assert!((s.scale() - 1.2).abs() < 1e-9);
        assert!((s.apply(DVec2::new(-5.0, 8.0)) - truth.apply(DVec2::new(-5.0, 8.0))).length() < 1e-9);
    }

    /// Smooth phantom: a body ellipsoid with denser and lighter blobs.
    fn phantom(p: DVec3) -> f32 {
        let body = (p.x / 40.0).powi(2) + (p.y / 30.0).powi(2) + (p.z / 34.0).powi(2);
        let mut v = if body < 1.0 { 40.0 } else { -1000.0 };
        for (c, r, a) in [
            (DVec3::new(-15.0, 5.0, 8.0), 9.0, 600.0),
            (DVec3::new(18.0, -8.0, -10.0), 7.0, 900.0),
            (DVec3::new(5.0, 14.0, 15.0), 6.0, -300.0),
            (DVec3::new(-5.0, -15.0, -18.0), 8.0, 400.0),
        ] {
            v += a * (-(p - c).length_squared() / (2.0 * r * r)).exp() as f32;
        }
        v
    }

    fn sample_volume(dims: [usize; 3], spacing: f64, origin: DVec3, f: impl Fn(DVec3) -> f32) -> Volume {
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let mut data = Vec::with_capacity(dims[0] * dims[1] * dims[2]);
        for z in 0..dims[2] {
            for y in 0..dims[1] {
                for x in 0..dims[0] {
                    let p = origin + DVec3::new(x as f64, y as f64, z as f64) * spacing;
                    data.push(f(p).round() as i16);
                }
            }
        }
        Volume::new(dims, DVec3::splat(spacing), origin, axes, data, Quant::IDENTITY)
    }

    #[test]
    fn mutual_information_finds_shift_and_rotation() {
        let truth = Rigid { r: rot_z(6.0), t: DVec3::new(9.0, -6.0, 7.0) };
        let inv = truth.inverse();
        let a = sample_volume([48, 40, 40], 2.0, DVec3::new(-48.0, -40.0, -40.0), phantom);
        // B(q) = phantom(T⁻¹ q), so B(T p) = A(p).
        let b = sample_volume([52, 44, 44], 2.0, DVec3::new(-44.0, -50.0, -36.0), |q| phantom(inv.apply(q)));
        let init = Rigid::translation(body_center(&b).unwrap() - body_center(&a).unwrap());
        let r = auto_register(&a, &b, init, &AtomicBool::new(false)).unwrap();
        let err = [DVec3::ZERO, DVec3::new(30.0, 20.0, 20.0), DVec3::new(-25.0, 10.0, -20.0)]
            .iter()
            .map(|&p| (r.transform.apply(p) - truth.apply(p)).length())
            .fold(0.0, f64::max);
        assert!(err < 2.5, "erro {err:.2} mm, NMI {:.3}", r.nmi);
    }
}
