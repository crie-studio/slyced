//! Display-only noise reduction. Measurements always read the original data.

use super::volume::OUTSIDE;
use rayon::prelude::*;

/// Edge-preserving smoothing (bilateral filter): neighbours are averaged with
/// weights that fall with distance (`sigma_px`, in pixels) and with the
/// difference in value (`sigma_value`, in stored units), so noise in uniform
/// tissue is reduced while the borders between tissues stay sharp.
pub fn bilateral(src: &[i16], w: usize, h: usize, sigma_px: f32, sigma_value: f32) -> Vec<i16> {
    if src.len() != w * h || sigma_px <= 0.0 || sigma_value <= 0.0 {
        return src.to_vec();
    }
    let r = (sigma_px * 2.0).ceil() as isize;
    let mut taps = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            let d2 = (dx * dx + dy * dy) as f32;
            if d2 <= (r * r) as f32 + 0.5 {
                taps.push((dx, dy, (-d2 / (2.0 * sigma_px * sigma_px)).exp()));
            }
        }
    }
    // Range weights over [0, 3σ), looked up instead of computing exp per tap.
    const STEPS: usize = 512;
    let max_d = 3.0 * sigma_value;
    let range: Vec<f32> = (0..STEPS)
        .map(|i| {
            let d = i as f32 / STEPS as f32 * max_d;
            (-d * d / (2.0 * sigma_value * sigma_value)).exp()
        })
        .collect();
    let to_index = STEPS as f32 / max_d;

    let mut out = vec![OUTSIDE; w * h];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let c = src[y * w + x];
            if c == OUTSIDE {
                continue;
            }
            let (mut acc, mut sum) = (0.0f32, 0.0f32);
            for &(dx, dy, ws) in &taps {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                    continue;
                }
                let v = src[ny as usize * w + nx as usize];
                if v == OUTSIDE {
                    continue;
                }
                let d = (v as f32 - c as f32).abs();
                if d >= max_d {
                    continue;
                }
                let wt = ws * range[(d * to_index) as usize];
                acc += wt * v as f32;
                sum += wt;
            }
            *px = if sum > 0.0 { (acc / sum).round().clamp(-32767.0, 32767.0) as i16 } else { c };
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(i: usize) -> i16 {
        // Deterministic pseudo-noise in [-20, 20].
        let x = (i as u32).wrapping_mul(2654435761).rotate_left(13);
        (x % 41) as i16 - 20
    }

    #[test]
    fn smooths_flat_regions_and_keeps_edges() {
        let (w, h) = (48, 32);
        // Left half 0 HU, right half 300 HU, both noisy.
        let src: Vec<i16> = (0..w * h).map(|i| if i % w < w / 2 { 0 } else { 300 } + noise(i)).collect();
        let out = bilateral(&src, w, h, 1.5, 40.0);
        let sd = |d: &[i16], x0: usize, x1: usize| {
            let v: Vec<f32> = (4..h - 4).flat_map(|y| (x0..x1).map(move |x| (y, x))).map(|(y, x)| d[y * w + x] as f32).collect();
            let m = v.iter().sum::<f32>() / v.len() as f32;
            (v.iter().map(|a| (a - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
        };
        assert!(sd(&out, 4, 18) < sd(&src, 4, 18) * 0.6, "ruído não reduzido");
        // The edge stays sharp: pixels next to it keep their side's level.
        for y in 4..h - 4 {
            assert!(out[y * w + w / 2 - 1].abs() < 40);
            assert!((out[y * w + w / 2] - 300).abs() < 40);
        }
    }

    #[test]
    fn outside_samples_are_preserved() {
        let mut src = vec![100i16; 16];
        src[5] = OUTSIDE;
        let out = bilateral(&src, 4, 4, 1.0, 30.0);
        assert_eq!(out[5], OUTSIDE);
        assert_eq!(out[0], 100);
    }
}
