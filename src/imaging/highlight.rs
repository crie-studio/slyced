//! Color overlays that make specific CT findings stand out on a slice: gas
//! inside the body (free air), dense foci (stones, appendicoliths) and
//! densified fat (inflammatory stranding).

use super::volume::{Quant, OUTSIDE};
use egui::Color32;
use rayon::prelude::*;

/// Which findings are highlighted. Values are Hounsfield units.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Highlights {
    /// Gas inside the body (below −800 HU, not connected to the air around the patient).
    pub gas: bool,
    /// Everything at or above this density.
    pub dense: Option<f32>,
    /// Fat that became denser (−70 to −10 HU) away from organ borders.
    pub fat: bool,
}

impl Highlights {
    pub fn any(&self) -> bool {
        self.gas || self.dense.is_some() || self.fat
    }
}

pub const GAS: Color32 = Color32::from_rgb(40, 230, 255);
pub const DENSE: Color32 = Color32::from_rgb(255, 60, 190);
pub const FAT: Color32 = Color32::from_rgb(255, 150, 40);

/// Gas threshold and the (looser) air level used to find the outside of the body.
const GAS_HU: f32 = -800.0;
const AIR_HU: f32 = -400.0;

/// Overlay colors (premultiplied, transparent where nothing is highlighted),
/// or `None` when nothing on this slice is highlighted.
pub fn overlay(data: &[i16], q: Quant, w: usize, h: usize, set: &Highlights) -> Option<Vec<Color32>> {
    if data.len() != w * h || w == 0 || h == 0 || !set.any() {
        return None;
    }
    let real = |s: i16| if s == OUTSIDE { f32::NAN } else { q.real(s) };
    let mut out = vec![Color32::TRANSPARENT; w * h];
    let mut found = false;

    if set.fat {
        let fat = densified_fat(data, q, w, h);
        for (o, &a) in out.iter_mut().zip(&fat) {
            if a > 0 {
                *o = Color32::from_rgba_unmultiplied(FAT.r(), FAT.g(), FAT.b(), a);
                found = true;
            }
        }
    }
    if set.gas {
        let outside = outside_air(data, q, w, h);
        for (i, o) in out.iter_mut().enumerate() {
            let v = real(data[i]);
            if v < GAS_HU && !outside[i] {
                *o = Color32::from_rgba_unmultiplied(GAS.r(), GAS.g(), GAS.b(), 215);
                found = true;
            }
        }
    }
    if let Some(t) = set.dense {
        for (o, &s) in out.iter_mut().zip(data) {
            if real(s) >= t {
                *o = Color32::from_rgba_unmultiplied(DENSE.r(), DENSE.g(), DENSE.b(), 220);
                found = true;
            }
        }
    }
    found.then_some(out)
}

/// Air connected to the image border: the room around the patient.
fn outside_air(data: &[i16], q: Quant, w: usize, h: usize) -> Vec<bool> {
    let air = |s: i16| s == OUTSIDE || q.real(s) < AIR_HU;
    let mut seen = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let push = |i: usize, seen: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if !seen[i] && air(data[i]) {
            seen[i] = true;
            stack.push(i);
        }
    };
    for x in 0..w {
        push(x, &mut seen, &mut stack);
        push((h - 1) * w + x, &mut seen, &mut stack);
    }
    for y in 0..h {
        push(y * w, &mut seen, &mut stack);
        push(y * w + w - 1, &mut seen, &mut stack);
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        if x > 0 {
            push(i - 1, &mut seen, &mut stack);
        }
        if x + 1 < w {
            push(i + 1, &mut seen, &mut stack);
        }
        if y > 0 {
            push(i - w, &mut seen, &mut stack);
        }
        if y + 1 < h {
            push(i + w, &mut seen, &mut stack);
        }
    }
    seen
}

/// Alpha (0 = none) of fat that is denser than normal fat (about −100 HU).
/// Only regions that are mostly fat count, so the partial-volume rim along
/// organs and bowel walls is not colored.
fn densified_fat(data: &[i16], q: Quant, w: usize, h: usize) -> Vec<u8> {
    let r = 2isize;
    let vals: Vec<f32> = data.iter().map(|&s| if s == OUTSIDE { -1000.0 } else { q.real(s) }).collect();
    let mut alpha = vec![0u8; w * h];
    alpha.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, a) in row.iter_mut().enumerate() {
            let v = vals[y * w + x];
            if !(-70.0..=-10.0).contains(&v) {
                continue;
            }
            let (mut sum, mut n, mut max, mut fatty) = (0.0f32, 0u32, f32::MIN, 0u32);
            for dy in -r..=r {
                for dx in -r..=r {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                        continue;
                    }
                    let u = vals[ny as usize * w + nx as usize];
                    sum += u;
                    n += 1;
                    max = max.max(u);
                    if (-150.0..=-10.0).contains(&u) {
                        fatty += 1;
                    }
                }
            }
            let mean = sum / n.max(1) as f32;
            // Mostly (densified) fat around, no soft tissue or air nearby.
            if max < 20.0 && (-95.0..=-20.0).contains(&mean) && fatty * 5 >= n * 4 && vals_min_ok(&vals, w, h, x, y) {
                let t = ((v + 80.0) / 70.0).clamp(0.2, 1.0);
                *a = (90.0 + 130.0 * t) as u8;
            }
        }
    });
    alpha
}

/// No gas next to the pixel (bowel content and lung borders are not fat).
fn vals_min_ok(vals: &[f32], w: usize, h: usize, x: usize, y: usize) -> bool {
    let (x0, x1) = (x.saturating_sub(1), (x + 1).min(w - 1));
    let (y0, y1) = (y.saturating_sub(1), (y + 1).min(h - 1));
    (y0..=y1).all(|yy| (x0..=x1).all(|xx| vals[yy * w + xx] > -300.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 40×40 slice: outside air, a body of soft tissue (40 HU) containing a
    /// fat region (−100 HU) with a densified patch (−40 HU), a bubble of gas
    /// and a dense stone.
    fn slice() -> Vec<i16> {
        let (w, h) = (40usize, 40usize);
        let mut d = vec![-1000i16; w * h];
        for y in 4..36 {
            for x in 4..36 {
                d[y * w + x] = 40;
            }
        }
        for y in 8..30 {
            for x in 8..30 {
                d[y * w + x] = -100;
            }
        }
        for y in 16..20 {
            for x in 16..20 {
                d[y * w + x] = -40;
            }
        }
        d[33 * w + 10] = -950; // free gas bubble inside the wall
        d[33 * w + 30] = 600; // stone
        d
    }

    #[test]
    fn highlights_internal_gas_dense_and_fat() {
        let d = slice();
        let set = Highlights { gas: true, dense: Some(200.0), fat: true };
        let o = overlay(&d, Quant::IDENTITY, 40, 40, &set).unwrap();
        assert_eq!(o[0], Color32::TRANSPARENT, "ar externo não é gás livre");
        assert!(o[33 * 40 + 10].a() > 0 && o[33 * 40 + 10].b() > 0, "bolha interna");
        assert!(o[33 * 40 + 30].a() > 0, "cálculo");
        assert!(o[18 * 40 + 18].a() > 0, "gordura densificada");
        assert_eq!(o[12 * 40 + 12], Color32::TRANSPARENT, "gordura normal");
        assert_eq!(o[6 * 40 + 20], Color32::TRANSPARENT, "partes moles");
        assert!(overlay(&d, Quant::IDENTITY, 40, 40, &Highlights::default()).is_none());
    }
}
