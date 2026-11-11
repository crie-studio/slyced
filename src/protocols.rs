//! Hanging protocols: one click arranges the reconstruction, window and slab
//! used for a clinical question.

use crate::i18n::Tr;

use crate::dicom::model::SeriesInfo;
use crate::imaging::lut::Window;
use crate::imaging::reslice::SlabMode;

pub struct Protocol {
    pub name: Tr,
    pub detail: Tr,
    pub window: Window,
    pub slab_mm: f32,
    pub slab_mode: SlabMode,
    /// Words in the series description that suggest the right series.
    pub prefer: &'static [&'static str],
    pub avoid: &'static [&'static str],
}

pub const PROTOCOLS: &[Protocol] = &[
    Protocol {
        name: Tr("Abdome agudo", "Acute abdomen"),
        detail: Tr(
            "Axial, coronal e sagital sincronizados, janela de abdome e média de 4 mm (apêndice, diverticulite, obstrução, coleções).",
            "Synchronized axial, coronal and sagittal, abdomen window and 4 mm average (appendix, diverticulitis, obstruction, collections).",
        ),
        window: Window { center: 60.0, width: 400.0 },
        slab_mm: 4.0,
        slab_mode: SlabMode::Avg,
        prefer: &["abd", "pelv", "portal", "venos", "abdomen", "fase"],
        avoid: &["pulm", "lung", "torax", "tórax", "chest", "osso", "bone", "scout", "topo", "local"],
    },
    Protocol {
        name: Tr("Tórax — nódulos", "Chest — nodules"),
        detail: Tr("Janela de pulmão e MIP de 8 mm, que destaca nódulos entre os vasos.", "Lung window and 8 mm MIP, which makes nodules stand out among the vessels."),
        window: Window { center: -600.0, width: 1500.0 },
        slab_mm: 8.0,
        slab_mode: SlabMode::Mip,
        prefer: &["pulm", "lung", "torax", "tórax", "chest", "parenq"],
        avoid: &["mediast", "partes moles", "scout", "topo"],
    },
    Protocol {
        name: Tr("Angio / TEP", "Angio / PE"),
        detail: Tr("Janela angio e MIP de 12 mm para seguir vasos e procurar falhas de enchimento.", "Angio window and 12 mm MIP to follow vessels and look for filling defects."),
        window: Window { center: 300.0, width: 600.0 },
        slab_mm: 12.0,
        slab_mode: SlabMode::Mip,
        prefer: &["angio", "arter", "tep", "cta", "aort", "pulmonar"],
        avoid: &["scout", "topo", "sem contraste"],
    },
    Protocol {
        name: Tr("Osso / coluna", "Bone / spine"),
        detail: Tr("Janela óssea com cortes finos, para fraturas e alinhamento.", "Bone window with thin slices, for fractures and alignment."),
        window: Window { center: 400.0, width: 1800.0 },
        slab_mm: 0.0,
        slab_mode: SlabMode::Avg,
        prefer: &["osso", "bone", "colun", "spine", "osse"],
        avoid: &["pulm", "scout", "topo"],
    },
    Protocol {
        name: Tr("Crânio / AVC", "Head / stroke"),
        detail: Tr("Janela estreita e média de 5 mm, que realça hipodensidades sutis.", "Narrow window and 5 mm average, which brings out subtle hypodensities."),
        window: Window { center: 35.0, width: 40.0 },
        slab_mm: 5.0,
        slab_mode: SlabMode::Avg,
        prefer: &["cran", "head", "brain", "encef", "cereb"],
        avoid: &["osso", "bone", "scout", "topo"],
    },
];

/// How well a series suits a protocol (higher is better); `None` if unusable.
pub fn score(p: &Protocol, s: &SeriesInfo) -> Option<i64> {
    let images = s.image_count() as i64;
    if s.modality != "CT" || images < 20 {
        return None;
    }
    let desc = format!("{} {}", s.description, s.body_part).to_lowercase();
    let mut score = images.min(2000);
    if p.prefer.iter().any(|w| desc.contains(w)) {
        score += 100_000;
    }
    if p.avoid.iter().any(|w| desc.contains(w)) {
        score -= 50_000;
    }
    // Thin slices reformat better.
    let thickness = s.instances.iter().filter_map(|i| i.slice_thickness).fold(f64::MAX, f64::min);
    if thickness <= 1.5 {
        score += 10_000;
    } else if thickness <= 3.0 {
        score += 4_000;
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dicom::model::InstanceInfo;

    fn series(desc: &str, n: usize, thick: f64) -> SeriesInfo {
        SeriesInfo {
            description: desc.into(),
            modality: "CT".into(),
            instances: (0..n).map(|_| InstanceInfo { has_pixels: true, frames: 1, slice_thickness: Some(thick), ..Default::default() }).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn acute_abdomen_prefers_thin_abdominal_series() {
        let p = &PROTOCOLS[0];
        let lung = series("Pulmão 1.0", 400, 1.0);
        let abd = series("ABDOME FASE PORTAL", 300, 1.25);
        let thick = series("ABDOME 5mm", 80, 5.0);
        let scout = series("Scout", 2, 1.0);
        assert!(score(p, &abd) > score(p, &lung));
        assert!(score(p, &abd) > score(p, &thick));
        assert_eq!(score(p, &scout), None);
    }
}
