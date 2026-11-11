//! Window/level and color lookup tables for 2D display.

use crate::i18n::Tr;

use super::volume::Quant;
use egui::Color32;
use rayon::prelude::*;

/// One entry per possible `i16` sample; index = `stored + 32768`.
pub const LUT_SIZE: usize = 1 << 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum Colormap {
    #[default]
    Gray,
    HotIron,
    Rainbow,
    Bone,
}

impl Colormap {
    pub const ALL: [Colormap; 4] = [Colormap::Gray, Colormap::HotIron, Colormap::Rainbow, Colormap::Bone];

    pub fn label(self) -> &'static str {
        match self {
            Colormap::Gray => tr!("Escala de cinza", "Grayscale"),
            Colormap::HotIron => "Hot iron",
            Colormap::Rainbow => tr!("Arco-íris", "Rainbow"),
            Colormap::Bone => "Bone",
        }
    }

    fn stops(self) -> &'static [(f32, [u8; 3])] {
        match self {
            Colormap::Gray => &[(0.0, [0, 0, 0]), (1.0, [255, 255, 255])],
            Colormap::HotIron => &[
                (0.0, [0, 0, 0]),
                (0.35, [160, 20, 10]),
                (0.65, [250, 130, 20]),
                (0.85, [255, 220, 90]),
                (1.0, [255, 255, 255]),
            ],
            Colormap::Rainbow => &[
                (0.0, [0, 0, 130]),
                (0.2, [0, 60, 255]),
                (0.4, [0, 220, 255]),
                (0.6, [120, 255, 120]),
                (0.8, [255, 200, 0]),
                (1.0, [220, 20, 20]),
            ],
            Colormap::Bone => &[(0.0, [0, 0, 0]), (0.375, [84, 84, 116]), (0.75, [166, 199, 199]), (1.0, [255, 255, 255])],
        }
    }

    pub fn map(self, t: f32) -> Color32 {
        let stops = self.stops();
        let t = t.clamp(0.0, 1.0);
        let i = stops.iter().position(|s| s.0 >= t).unwrap_or(stops.len() - 1).max(1);
        let (t0, c0) = stops[i - 1];
        let (t1, c1) = stops[i];
        let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f).round() as u8;
        Color32::from_rgb(mix(c0[0], c1[0]), mix(c0[1], c1[1]), mix(c0[2], c1[2]))
    }
}

/// Display window in real units.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Window {
    pub center: f32,
    pub width: f32,
}

impl Window {
    pub fn new(center: f32, width: f32) -> Self {
        Self { center, width: width.max(1e-3) }
    }
}

pub struct WindowPreset {
    pub name: Tr,
    pub key: &'static str,
    pub window: Option<Window>,
}

/// Presets for CT (Hounsfield units). `None` means "full dynamic range".
pub const CT_PRESETS: &[WindowPreset] = &[
    WindowPreset { name: Tr("Padrão DICOM", "DICOM default"), key: "1", window: None },
    WindowPreset { name: Tr("Cérebro", "Brain"), key: "2", window: Some(Window { center: 40.0, width: 80.0 }) },
    WindowPreset { name: Tr("Partes moles", "Soft tissue"), key: "3", window: Some(Window { center: 50.0, width: 350.0 }) },
    WindowPreset { name: Tr("Pulmão", "Lung"), key: "4", window: Some(Window { center: -600.0, width: 1500.0 }) },
    WindowPreset { name: Tr("Osso", "Bone"), key: "5", window: Some(Window { center: 400.0, width: 1800.0 }) },
    WindowPreset { name: Tr("Abdome", "Abdomen"), key: "6", window: Some(Window { center: 60.0, width: 400.0 }) },
    WindowPreset { name: Tr("Angio", "Angio"), key: "7", window: Some(Window { center: 300.0, width: 600.0 }) },
    WindowPreset { name: Tr("Mediastino", "Mediastinum"), key: "8", window: Some(Window { center: 40.0, width: 400.0 }) },
    WindowPreset { name: Tr("AVC", "Stroke"), key: "9", window: Some(Window { center: 35.0, width: 35.0 }) },
    // Narrow window centered on fat: inflamed (densified) mesenteric fat stands out.
    WindowPreset { name: Tr("Gordura mesent.", "Mesenteric fat"), key: "0", window: Some(Window { center: -50.0, width: 250.0 }) },
];

/// Builds a lookup table from stored samples to display colors.
/// The [`OUTSIDE`](super::volume::OUTSIDE) marker (index 0) always maps to black.
pub fn build_lut(quant: Quant, window: Window, invert: bool, cmap: Colormap, lut: &mut Vec<Color32>) {
    lut.resize(LUT_SIZE, Color32::BLACK);
    let lo = window.center - window.width * 0.5;
    let inv_w = 1.0 / window.width.max(1e-3);
    // Precompute 1024 colormap entries so the per-sample work is just a lookup.
    let ramp: Vec<Color32> = (0..1024).map(|i| cmap.map(i as f32 / 1023.0)).collect();
    lut.par_chunks_mut(4096).enumerate().for_each(|(c, chunk)| {
        for (j, out) in chunk.iter_mut().enumerate() {
            let stored = (c * 4096 + j) as i32 - 32768;
            let v = quant.real(stored as i16);
            let mut t = ((v - lo) * inv_w).clamp(0.0, 1.0);
            if invert {
                t = 1.0 - t;
            }
            *out = ramp[(t * 1023.0).round() as usize];
        }
    });
    lut[0] = Color32::BLACK;
}

pub fn apply_lut(src: &[i16], lut: &[Color32], out: &mut Vec<Color32>) {
    out.resize(src.len(), Color32::BLACK);
    out.par_chunks_mut(8192).zip(src.par_chunks(8192)).for_each(|(o, s)| {
        for (o, &s) in o.iter_mut().zip(s) {
            *o = lut[(s as i32 + 32768) as usize];
        }
    });
}

/// Brightness/contrast for RGB images, expressed as a window over 0..255.
pub fn apply_rgb_window(src: &[u8], window: Window, invert: bool, out: &mut Vec<Color32>) {
    let lo = window.center - window.width * 0.5;
    let inv_w = 255.0 / window.width.max(1e-3);
    let table: Vec<u8> = (0..256)
        .map(|v| {
            let t = ((v as f32 - lo) * inv_w).clamp(0.0, 255.0) as u8;
            if invert { 255 - t } else { t }
        })
        .collect();
    out.resize(src.len() / 3, Color32::BLACK);
    out.par_chunks_mut(4096).zip(src.par_chunks(4096 * 3)).for_each(|(o, s)| {
        for (o, p) in o.iter_mut().zip(s.chunks_exact(3)) {
            *o = Color32::from_rgb(table[p[0] as usize], table[p[1] as usize], table[p[2] as usize]);
        }
    });
}

/// Window covering the central 98% of the samples, a robust default when the
/// file carries no window and the modality has no standard one.
pub fn auto_window(samples: &[i16], quant: Quant) -> Window {
    let mut hist = vec![0u32; LUT_SIZE];
    let mut n = 0u64;
    for &s in samples.iter().step_by((samples.len() / 200_000).max(1)) {
        if s != super::volume::OUTSIDE {
            hist[(s as i32 + 32768) as usize] += 1;
            n += 1;
        }
    }
    if n == 0 {
        return Window::new(0.0, 1.0);
    }
    let pick = |frac: f64| {
        let target = (n as f64 * frac) as u64;
        let mut acc = 0u64;
        for (i, &h) in hist.iter().enumerate() {
            acc += h as u64;
            if acc > target {
                return quant.real((i as i32 - 32768) as i16);
            }
        }
        quant.real(i16::MAX)
    };
    let (lo, hi) = (pick(0.01), pick(0.99));
    Window::new((lo + hi) * 0.5, (hi - lo).max(1.0))
}
