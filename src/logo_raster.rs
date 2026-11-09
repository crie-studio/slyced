// Shared by `build.rs` (via `include!`) and the app: rasterizes the logo so
// no binary image assets are needed for the window and executable icons.

/// The three stacked slices of the logo, in a 64-unit square: diamond centers,
/// the plate's half width and half height, and the gap drawn around the upper plates.
pub const SLICES: [(f32, f32); 3] = [(32.0, 23.0), (32.0, 34.0), (32.0, 44.0)];
pub const SLICE_HALF: (f32, f32) = (20.0, 10.0);
pub const SLICE_GAP: f32 = 1.5;

/// Signed distance (logo units) from point `(x, y)` to the edge of the slice centered at `c`.
fn slice_distance(x: f32, y: f32, c: (f32, f32)) -> f32 {
    let (a, b) = SLICE_HALF;
    let v = (x - c.0).abs() / a + (y - c.1).abs() / b;
    (v - 1.0) / (1.0 / (a * a) + 1.0 / (b * b)).sqrt()
}

/// Renders the logo as straight-alpha RGBA at `size`×`size` pixels.
#[allow(dead_code)]
pub fn render_logo(size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let n = size as f32;
    let radius = n * 0.26;
    let mut out = vec![0u8; (size * size * 4) as usize];
    let a = [77.0f32, 141.0, 255.0];
    let b = [132.0f32, 102.0, 255.0];
    for y in 0..size {
        for x in 0..size {
            let (mut rgb, mut cov) = ([0.0f32; 3], 0.0f32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    // Rounded square.
                    let qx = (px - n * 0.5).abs() - (n * 0.5 - radius);
                    let qy = (py - n * 0.5).abs() - (n * 0.5 - radius);
                    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
                    if outside > 0.0 {
                        continue;
                    }
                    let t = ((px + py) / (2.0 * n)).clamp(0.0, 1.0);
                    let bg = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
                    // Stacked slices, top first: white, lilac, then a translucent one.
                    let (ux, uy) = (px / n * 64.0, py / n * 64.0);
                    let (top, mid, low) = (slice_distance(ux, uy, SLICES[0]), slice_distance(ux, uy, SLICES[1]), slice_distance(ux, uy, SLICES[2]));
                    let c = if top < -SLICE_GAP {
                        [255.0, 255.0, 255.0]
                    } else if top <= SLICE_GAP {
                        bg
                    } else if mid < -SLICE_GAP {
                        [207.0, 196.0, 255.0]
                    } else if mid <= SLICE_GAP {
                        bg
                    } else if low <= 0.0 {
                        [bg[0] * 0.55 + 255.0 * 0.45, bg[1] * 0.55 + 255.0 * 0.45, bg[2] * 0.55 + 255.0 * 0.45]
                    } else {
                        bg
                    };
                    rgb = [rgb[0] + c[0], rgb[1] + c[1], rgb[2] + c[2]];
                    cov += 1.0;
                }
            }
            let i = ((y * size + x) * 4) as usize;
            if cov > 0.0 {
                out[i] = (rgb[0] / cov) as u8;
                out[i + 1] = (rgb[1] / cov) as u8;
                out[i + 2] = (rgb[2] / cov) as u8;
                out[i + 3] = (cov / (SS * SS) as f32 * 255.0) as u8;
            }
        }
    }
    out
}

/// Encodes several sizes into a Windows .ico (32-bit BMP entries).
#[allow(dead_code)]
pub fn encode_ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = sizes
        .iter()
        .map(|&s| {
            let rgba = render_logo(s);
            let mut bmp = Vec::new();
            // BITMAPINFOHEADER; height is doubled to account for the AND mask.
            for v in [40u32, s, s * 2] {
                bmp.extend_from_slice(&v.to_le_bytes());
            }
            bmp.extend_from_slice(&1u16.to_le_bytes());
            bmp.extend_from_slice(&32u16.to_le_bytes());
            for _ in 0..6 {
                bmp.extend_from_slice(&0u32.to_le_bytes());
            }
            for y in (0..s).rev() {
                for x in 0..s {
                    let i = ((y * s + x) * 4) as usize;
                    bmp.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
                }
            }
            let mask_row = s.div_ceil(32) * 4;
            bmp.extend(std::iter::repeat_n(0u8, (mask_row * s) as usize));
            (s, bmp)
        })
        .collect();
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (s, bmp) in &images {
        let dim = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend_from_slice(&[dim, dim, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(bmp.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += bmp.len() as u32;
    }
    for (_, bmp) in images {
        ico.extend(bmp);
    }
    ico
}
