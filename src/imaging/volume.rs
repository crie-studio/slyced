//! Scalar volume stored as compact `i16` samples plus a linear mapping to real
//! (modality) units such as Hounsfield.

use glam::{DMat3, DVec3};
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Process-unique id, used as a cache key for GPU textures and derived images.
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Stored sample value used to mark pixels that fall outside the volume.
pub const OUTSIDE: i16 = i16::MIN;

/// Linear mapping between the compact `i16` storage and real values:
/// `real = stored * scale + offset`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quant {
    pub scale: f32,
    pub offset: f32,
}

impl Quant {
    pub const IDENTITY: Self = Self { scale: 1.0, offset: 0.0 };

    #[inline]
    pub fn real(self, s: i16) -> f32 {
        s as f32 * self.scale + self.offset
    }

    #[inline]
    pub fn real_f(self, s: f32) -> f32 {
        s * self.scale + self.offset
    }

    #[inline]
    pub fn stored(self, v: f32) -> f32 {
        (v - self.offset) / self.scale
    }

    #[inline]
    pub fn quantize(self, v: f32) -> i16 {
        ((v - self.offset) / self.scale).round().clamp(-32767.0, 32767.0) as i16
    }

    /// Picks a mapping able to hold `[min, max]`. Exact for integer data whose
    /// span fits in 16 bits (CT, most MR); otherwise uniformly quantized.
    /// `-32768` is never produced so it can serve as the [`OUTSIDE`] marker.
    pub fn for_range(min: f64, max: f64, integral: bool) -> Self {
        if integral && min >= -32767.0 && max <= 32767.0 {
            return Self::IDENTITY;
        }
        if integral && max - min <= 65534.0 {
            return Self { scale: 1.0, offset: (min + 32767.0) as f32 };
        }
        let scale = ((max - min) / 65534.0).max(1e-12);
        Self { scale: scale as f32, offset: (min + 32767.0 * scale) as f32 }
    }
}

/// A regular grid of samples in patient (LPS) space.
pub struct Volume {
    pub id: u64,
    /// Number of samples along the column (x), row (y) and slice (z) axes.
    pub dims: [usize; 3],
    /// Distance in mm between sample centers along each axis.
    pub spacing: DVec3,
    /// Patient position of the center of voxel (0, 0, 0).
    pub origin: DVec3,
    /// Unit direction of each voxel axis in patient space.
    pub axes: [DVec3; 3],
    pub data: Vec<i16>,
    pub quant: Quant,
    /// Minimum and maximum stored sample.
    pub range: (i16, i16),
    to_voxel: DMat3,
}

impl Volume {
    pub fn new(
        dims: [usize; 3],
        spacing: DVec3,
        origin: DVec3,
        axes: [DVec3; 3],
        data: Vec<i16>,
        quant: Quant,
    ) -> Self {
        debug_assert_eq!(data.len(), dims[0] * dims[1] * dims[2]);
        let m = DMat3::from_cols(axes[0] * spacing.x, axes[1] * spacing.y, axes[2] * spacing.z);
        let range = data
            .par_chunks(1 << 16)
            .map(|c| c.iter().fold((i16::MAX, i16::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v))))
            .reduce(|| (i16::MAX, i16::MIN), |a, b| (a.0.min(b.0), a.1.max(b.1)));
        Self { id: next_id(), dims, spacing, origin, axes, data, quant, range, to_voxel: m.inverse() }
    }

    #[inline]
    pub fn slice_len(&self) -> usize {
        self.dims[0] * self.dims[1]
    }

    pub fn slice(&self, k: usize) -> &[i16] {
        let n = self.slice_len();
        &self.data[k * n..(k + 1) * n]
    }

    pub fn voxel_to_patient(&self, v: DVec3) -> DVec3 {
        self.origin
            + self.axes[0] * (v.x * self.spacing.x)
            + self.axes[1] * (v.y * self.spacing.y)
            + self.axes[2] * (v.z * self.spacing.z)
    }

    pub fn patient_to_voxel(&self, p: DVec3) -> DVec3 {
        self.to_voxel * (p - self.origin)
    }

    pub fn to_voxel_matrix(&self) -> DMat3 {
        self.to_voxel
    }

    /// Patient positions of the eight corner voxel centers.
    pub fn corners(&self) -> [DVec3; 8] {
        let [nx, ny, nz] = self.dims.map(|d| (d.max(1) - 1) as f64);
        let mut out = [DVec3::ZERO; 8];
        for (i, c) in out.iter_mut().enumerate() {
            let v = DVec3::new(
                if i & 1 != 0 { nx } else { 0.0 },
                if i & 2 != 0 { ny } else { 0.0 },
                if i & 4 != 0 { nz } else { 0.0 },
            );
            *c = self.voxel_to_patient(v);
        }
        out
    }

    pub fn center(&self) -> DVec3 {
        let c = DVec3::new(
            (self.dims[0] as f64 - 1.0) * 0.5,
            (self.dims[1] as f64 - 1.0) * 0.5,
            (self.dims[2] as f64 - 1.0) * 0.5,
        );
        self.voxel_to_patient(c)
    }

    /// Size of the volume box in mm, covering whole voxels.
    pub fn extent_mm(&self) -> DVec3 {
        DVec3::new(
            self.dims[0] as f64 * self.spacing.x,
            self.dims[1] as f64 * self.spacing.y,
            self.dims[2] as f64 * self.spacing.z,
        )
    }

    pub fn real_range(&self) -> (f32, f32) {
        (self.quant.real(self.range.0), self.quant.real(self.range.1))
    }

    pub fn memory_bytes(&self) -> usize {
        self.data.len() * 2
    }

    /// Trilinear sample in stored units at a continuous voxel coordinate.
    #[inline]
    pub fn sample(&self, x: f32, y: f32, z: f32) -> Option<f32> {
        let [nx, ny, nz] = self.dims;
        if !(x > -0.5 && y > -0.5 && z > -0.5) {
            return None;
        }
        if !(x < nx as f32 - 0.5 && y < ny as f32 - 0.5 && z < nz as f32 - 0.5) {
            return None;
        }
        let x = x.clamp(0.0, (nx - 1) as f32);
        let y = y.clamp(0.0, (ny - 1) as f32);
        let z = z.clamp(0.0, (nz - 1) as f32);
        let (x0, y0, z0) = (x as usize, y as usize, z as usize);
        let (fx, fy, fz) = (x - x0 as f32, y - y0 as f32, z - z0 as f32);
        let dx = usize::from(x0 + 1 < nx);
        let dy = if y0 + 1 < ny { nx } else { 0 };
        let dz = if z0 + 1 < nz { nx * ny } else { 0 };
        let i = z0 * nx * ny + y0 * nx + x0;
        let d = &self.data;
        let c00 = lerp(d[i] as f32, d[i + dx] as f32, fx);
        let c10 = lerp(d[i + dy] as f32, d[i + dy + dx] as f32, fx);
        let c01 = lerp(d[i + dz] as f32, d[i + dz + dx] as f32, fx);
        let c11 = lerp(d[i + dz + dy] as f32, d[i + dz + dy + dx] as f32, fx);
        Some(lerp(lerp(c00, c10, fy), lerp(c01, c11, fy), fz))
    }

    /// Nearest-neighbour value in real units, used by the HU probe.
    pub fn value_at(&self, p: DVec3) -> Option<f32> {
        let v = self.patient_to_voxel(p);
        self.sample(v.x as f32, v.y as f32, v.z as f32).map(|s| self.quant.real_f(s))
    }

    /// Mean value (real units) of the voxels within `r_mm` of patient point `p`.
    pub fn mean_in_sphere(&self, p: DVec3, r_mm: f64) -> Option<f32> {
        let c = self.patient_to_voxel(p);
        let reach = DVec3::new(r_mm / self.spacing.x, r_mm / self.spacing.y, r_mm / self.spacing.z);
        let lo = (c - reach).floor();
        let hi = (c + reach).ceil();
        let [nx, ny, nz] = self.dims;
        let (mut sum, mut n) = (0.0f64, 0usize);
        for z in lo.z.max(0.0) as usize..=(hi.z.max(0.0) as usize).min(nz.saturating_sub(1)) {
            for y in lo.y.max(0.0) as usize..=(hi.y.max(0.0) as usize).min(ny.saturating_sub(1)) {
                for x in lo.x.max(0.0) as usize..=(hi.x.max(0.0) as usize).min(nx.saturating_sub(1)) {
                    if (self.voxel_to_patient(DVec3::new(x as f64, y as f64, z as f64)) - p).length() > r_mm {
                        continue;
                    }
                    let s = self.data[(z * ny + y) * nx + x];
                    if s != OUTSIDE {
                        sum += self.quant.real(s) as f64;
                        n += 1;
                    }
                }
            }
        }
        (n > 0).then(|| (sum / n as f64) as f32)
    }

    /// Mean value (real units) inside the ellipsoid centered at `c` with
    /// perpendicular semi-axes `axes` (patient space, mm).
    pub fn mean_in_ellipsoid(&self, c: DVec3, axes: [DVec3; 3]) -> Option<f32> {
        let len2 = axes.map(|a| a.length_squared());
        if len2.iter().any(|l| *l <= 0.0) {
            return None;
        }
        // Voxel box around the ellipsoid: the corners of its bounding parallelepiped.
        let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let v = self.patient_to_voxel(c + axes[0] * sx + axes[1] * sy + axes[2] * sz);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
        }
        let [nx, ny, nz] = self.dims;
        let range = |l: f64, h: f64, n: usize| (l.floor().max(0.0) as usize)..=((h.ceil().max(0.0) as usize).min(n.saturating_sub(1)));
        let (mut sum, mut n) = (0.0f64, 0usize);
        for z in range(lo.z, hi.z, nz) {
            for y in range(lo.y, hi.y, ny) {
                for x in range(lo.x, hi.x, nx) {
                    let d = self.voxel_to_patient(DVec3::new(x as f64, y as f64, z as f64)) - c;
                    let inside: f64 = (0..3).map(|i| d.dot(axes[i]).powi(2) / (len2[i] * len2[i])).sum();
                    if inside > 1.0 {
                        continue;
                    }
                    let s = self.data[(z * ny + y) * nx + x];
                    if s != OUTSIDE {
                        sum += self.quant.real(s) as f64;
                        n += 1;
                    }
                }
            }
        }
        (n > 0).then(|| (sum / n as f64) as f32)
    }

    /// Returns a copy averaged down by `factor` along each axis (used to fit GPU limits).
    pub fn downsampled(&self, factor: [usize; 3]) -> Volume {
        let [fx, fy, fz] = factor.map(|f| f.max(1));
        let [nx, ny, nz] = self.dims;
        let dims = [nx.div_ceil(fx), ny.div_ceil(fy), nz.div_ceil(fz)];
        let mut data = vec![0i16; dims[0] * dims[1] * dims[2]];
        data.par_chunks_mut(dims[0] * dims[1]).enumerate().for_each(|(k, out)| {
            for j in 0..dims[1] {
                for i in 0..dims[0] {
                    let (mut sum, mut cnt) = (0i64, 0i64);
                    for z in k * fz..((k + 1) * fz).min(nz) {
                        for y in j * fy..((j + 1) * fy).min(ny) {
                            let row = z * nx * ny + y * nx;
                            for x in i * fx..((i + 1) * fx).min(nx) {
                                sum += self.data[row + x] as i64;
                                cnt += 1;
                            }
                        }
                    }
                    out[j * dims[0] + i] = (sum / cnt.max(1)) as i16;
                }
            }
        });
        let spacing = self.spacing * DVec3::new(fx as f64, fy as f64, fz as f64);
        // Keep the box center fixed: the first output voxel is centered over the first block.
        let shift = DVec3::new((fx - 1) as f64 * 0.5, (fy - 1) as f64 * 0.5, (fz - 1) as f64 * 0.5);
        let origin = self.voxel_to_patient(shift);
        Volume::new(dims, spacing, origin, self.axes, data, self.quant)
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quant_is_exact_for_ct_range() {
        let q = Quant::for_range(-1024.0, 3071.0, true);
        assert_eq!(q, Quant::IDENTITY);
        assert_eq!(q.real(q.quantize(-1000.0)), -1000.0);
    }

    #[test]
    fn quant_shifts_wide_integer_range() {
        let q = Quant::for_range(0.0, 60000.0, true);
        assert_eq!(q.scale, 1.0);
        for v in [0.0, 12345.0, 60000.0] {
            assert_eq!(q.real(q.quantize(v)), v);
        }
        assert_ne!(q.quantize(0.0), OUTSIDE);
    }

    #[test]
    fn quant_float_range_roundtrips_approximately() {
        let q = Quant::for_range(-0.5, 2.5, false);
        assert!((q.real(q.quantize(1.234)) - 1.234).abs() < 1e-3);
    }

    fn ramp(dims: [usize; 3]) -> Volume {
        let data = (0..dims[0] * dims[1] * dims[2]).map(|i| (i % dims[0]) as i16 * 10).collect();
        Volume::new(dims, DVec3::new(0.5, 0.5, 2.0), DVec3::ZERO, [DVec3::X, DVec3::Y, DVec3::Z], data, Quant::IDENTITY)
    }

    #[test]
    fn trilinear_interpolates_and_rejects_outside() {
        let v = ramp([8, 4, 3]);
        assert_eq!(v.sample(2.5, 1.0, 1.0), Some(25.0));
        assert_eq!(v.sample(-1.0, 0.0, 0.0), None);
        assert_eq!(v.sample(0.0, 0.0, 3.0), None);
    }

    #[test]
    fn geometry_roundtrip_and_downsample_keeps_center() {
        let v = ramp([8, 8, 4]);
        let p = v.voxel_to_patient(DVec3::new(3.0, 2.0, 1.0));
        assert!((v.patient_to_voxel(p) - DVec3::new(3.0, 2.0, 1.0)).length() < 1e-9);
        let d = v.downsampled([2, 2, 2]);
        assert_eq!(d.dims, [4, 4, 2]);
        assert!((d.center() - v.center()).length() < 1e-9);
    }
}
