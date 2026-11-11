//! Transfer functions for volume rendering, inspired by the 3D Slicer presets.

use crate::i18n::Tr;

use half::f16;

/// Number of entries in the baked transfer function texture.
pub const TF_SIZE: usize = 4096;

#[derive(Clone, Copy, Debug)]
pub struct TfPoint {
    pub value: f32,
    pub color: [f32; 3],
    pub opacity: f32,
}

const fn p(value: f32, r: f32, g: f32, b: f32, opacity: f32) -> TfPoint {
    TfPoint { value, color: [r, g, b], opacity }
}

#[derive(Clone, Copy, Debug)]
pub struct Shading {
    pub ambient: f32,
    pub diffuse: f32,
    pub specular: f32,
    pub shininess: f32,
}

const SHADE_DEFAULT: Shading = Shading { ambient: 0.25, diffuse: 0.8, specular: 0.25, shininess: 24.0 };
const SHADE_SOFT: Shading = Shading { ambient: 0.35, diffuse: 0.7, specular: 0.15, shininess: 12.0 };

pub struct VrPreset {
    pub name: Tr,
    pub ct: bool,
    pub points: &'static [TfPoint],
    pub shading: Shading,
    /// Swatch color for the preset gallery.
    pub swatch: [u8; 3],
}

pub const PRESETS: &[VrPreset] = &[
    VrPreset {
        name: Tr("Osso", "Bone"),
        ct: true,
        points: &[
            p(-3024.0, 0.0, 0.0, 0.0, 0.0),
            p(-16.0, 0.73, 0.25, 0.30, 0.0),
            p(641.0, 0.91, 0.82, 0.55, 0.72),
            p(3071.0, 1.0, 1.0, 1.0, 0.71),
        ],
        shading: SHADE_DEFAULT,
        swatch: [232, 214, 170],
    },
    VrPreset {
        name: Tr("Angio", "Angio"),
        ct: true,
        points: &[
            p(-3024.0, 0.0, 0.0, 0.0, 0.0),
            p(143.0, 0.62, 0.36, 0.18, 0.0),
            p(166.0, 0.88, 0.60, 0.29, 0.69),
            p(214.0, 1.0, 1.0, 1.0, 0.70),
            p(420.0, 1.0, 0.94, 0.95, 0.83),
            p(3071.0, 0.83, 0.66, 1.0, 0.80),
        ],
        shading: SHADE_DEFAULT,
        swatch: [226, 120, 96],
    },
    VrPreset {
        name: Tr("Músculo", "Muscle"),
        ct: true,
        points: &[
            p(-3024.0, 0.0, 0.0, 0.0, 0.0),
            p(-155.0, 0.55, 0.25, 0.15, 0.0),
            p(218.0, 0.88, 0.60, 0.29, 0.68),
            p(420.0, 1.0, 0.94, 0.95, 0.83),
            p(3071.0, 0.83, 0.66, 1.0, 0.80),
        ],
        shading: SHADE_DEFAULT,
        swatch: [196, 92, 72],
    },
    VrPreset {
        name: Tr("Pele", "Skin"),
        ct: true,
        points: &[
            p(-3024.0, 0.0, 0.0, 0.0, 0.0),
            p(-1000.0, 0.0, 0.0, 0.0, 0.0),
            p(-500.0, 0.85, 0.60, 0.45, 0.0),
            p(-300.0, 0.95, 0.72, 0.58, 0.9),
            p(3071.0, 1.0, 0.85, 0.75, 1.0),
        ],
        shading: SHADE_SOFT,
        swatch: [238, 186, 150],
    },
    VrPreset {
        name: Tr("Pulmão", "Lung"),
        ct: true,
        points: &[
            p(-1024.0, 0.0, 0.0, 0.0, 0.0),
            p(-900.0, 0.30, 0.40, 0.85, 0.0),
            p(-760.0, 0.45, 0.60, 0.95, 0.06),
            p(-500.0, 0.85, 0.85, 0.95, 0.10),
            p(-350.0, 0.9, 0.7, 0.6, 0.0),
            p(150.0, 0.9, 0.7, 0.6, 0.0),
            p(250.0, 0.95, 0.92, 0.85, 0.6),
            p(3071.0, 1.0, 1.0, 1.0, 0.7),
        ],
        shading: SHADE_SOFT,
        swatch: [130, 170, 240],
    },
    VrPreset {
        name: Tr("Partes moles", "Soft tissue"),
        ct: true,
        points: &[
            p(-3024.0, 0.0, 0.0, 0.0, 0.0),
            p(-200.0, 0.0, 0.0, 0.0, 0.0),
            p(-100.0, 0.75, 0.45, 0.35, 0.04),
            p(60.0, 0.85, 0.40, 0.30, 0.25),
            p(200.0, 0.95, 0.80, 0.65, 0.6),
            p(3071.0, 1.0, 1.0, 1.0, 0.9),
        ],
        shading: SHADE_SOFT,
        swatch: [214, 132, 112],
    },
    VrPreset {
        name: Tr("MR padrão", "MR default"),
        ct: false,
        points: &[
            p(0.0, 0.0, 0.0, 0.0, 0.0),
            p(20.0, 0.17, 0.0, 0.0, 0.0),
            p(40.0, 0.40, 0.15, 0.08, 0.15),
            p(120.0, 0.78, 0.61, 0.38, 0.30),
            p(220.0, 0.85, 0.84, 0.79, 0.375),
            p(1024.0, 1.0, 1.0, 1.0, 0.5),
        ],
        shading: SHADE_SOFT,
        swatch: [200, 170, 130],
    },
];

/// User adjustments applied on top of a preset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TfAdjust {
    /// Shift of every control point (real units).
    pub shift: f32,
    /// Multiplier of the value distance between control points around their center.
    pub width: f32,
}

impl Default for TfAdjust {
    fn default() -> Self {
        Self { shift: 0.0, width: 1.0 }
    }
}

/// Bakes the preset into an RGBA (f16) table covering `[lo, hi]` and returns `(table, lo, hi)`.
/// MR presets are rescaled to `data_range` since MR intensities are not standardized.
pub fn bake(preset: &VrPreset, adjust: TfAdjust, data_range: (f32, f32)) -> (Vec<f16>, f32, f32) {
    let mut pts: Vec<TfPoint> = preset.points.to_vec();
    if !preset.ct {
        let span = (data_range.1 - data_range.0).max(1.0) / 1024.0;
        for q in &mut pts {
            q.value = data_range.0 + q.value * span;
        }
    }
    let first = pts.first().map(|q| q.value).unwrap_or(0.0);
    let last = pts.last().map(|q| q.value).unwrap_or(1.0);
    // Scale point distances around the first point that has visible opacity.
    let pivot = pts.iter().find(|q| q.opacity > 0.0).map(|q| q.value).unwrap_or((first + last) * 0.5);
    for q in &mut pts {
        q.value = pivot + (q.value - pivot) * adjust.width + adjust.shift;
    }
    let lo = pts.first().map(|q| q.value).unwrap_or(0.0).max(data_range.0 - 1.0);
    let hi = pts.last().map(|q| q.value).unwrap_or(1.0).min(data_range.1 + 1.0).max(lo + 1.0);

    let mut table = Vec::with_capacity(TF_SIZE * 4);
    for i in 0..TF_SIZE {
        let v = lo + (hi - lo) * i as f32 / (TF_SIZE - 1) as f32;
        let (c, a) = eval(&pts, v);
        table.extend([c[0], c[1], c[2], a].map(f16::from_f32));
    }
    (table, lo, hi)
}

fn eval(pts: &[TfPoint], v: f32) -> ([f32; 3], f32) {
    if pts.is_empty() {
        return ([0.0; 3], 0.0);
    }
    if v <= pts[0].value {
        return (pts[0].color, pts[0].opacity);
    }
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if v <= b.value {
            let t = if b.value > a.value { (v - a.value) / (b.value - a.value) } else { 1.0 };
            let c = [0, 1, 2].map(|i| a.color[i] + (b.color[i] - a.color[i]) * t);
            return (c, a.opacity + (b.opacity - a.opacity) * t);
        }
    }
    let l = pts[pts.len() - 1];
    (l.color, l.opacity)
}
