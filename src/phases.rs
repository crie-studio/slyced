//! Contrast phases of a CT exam (without and with contrast) and the reading of
//! enhancement as the density change between them (ΔHU).

use crate::dicom::model::{SeriesInfo, StudyInfo};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Without contrast (native, plain).
    Pre,
    /// After contrast: arterial, portal, venous, delayed…
    Post,
    Unknown,
}

const PRE: &[&str] = &["sem contr", "s/ contr", "s/contr", "s/c", "pre contr", "pré", "pre-contr", "nativ", "non con", "noncon", "plain", "basal", "sem iv", "without"];
const POST: &[&str] = &["arter", "portal", "venos", "com contr", "c/ contr", "c/contr", "c/c", "tardi", "equil", "nefro", "excret", "angio", "cta", "contrast", "fase", "phase", "realce"];

pub fn phase(s: &SeriesInfo) -> Phase {
    let d = s.description.to_lowercase();
    if PRE.iter().any(|w| d.contains(w)) {
        Phase::Pre
    } else if !s.contrast.trim().is_empty() || POST.iter().any(|w| d.contains(w)) {
        Phase::Post
    } else {
        Phase::Unknown
    }
}

/// Without-contrast and with-contrast CT series of a study (portal/venous
/// preferred, then arterial), as `(pre, post)` series UIDs.
pub fn pick_pair(study: &StudyInfo) -> Option<(String, String)> {
    let cts: Vec<&SeriesInfo> = study.series.iter().filter(|s| s.modality == "CT" && s.image_count() >= 20).collect();
    let thickness = |s: &SeriesInfo| s.instances.iter().find_map(|i| i.slice_thickness).unwrap_or(0.0);
    let pre = cts
        .iter()
        .filter(|s| phase(s) == Phase::Pre)
        .max_by_key(|s| s.image_count())
        .or_else(|| {
            // No explicit label: the first series without a contrast agent, before the contrast ones.
            cts.iter().filter(|s| phase(s) == Phase::Unknown).min_by_key(|s| s.number)
        })?;
    let score = |s: &SeriesInfo| {
        let d = s.description.to_lowercase();
        let kind = if d.contains("portal") || d.contains("venos") {
            3
        } else if d.contains("arter") || d.contains("angio") {
            2
        } else {
            1
        };
        let similar = ((thickness(s) - thickness(pre)).abs() < 0.3) as i64;
        kind * 10_000 + similar * 5_000 + s.image_count().min(4_000) as i64
    };
    let post = cts.iter().filter(|s| s.uid != pre.uid && phase(s) == Phase::Post).max_by_key(|s| score(s))?;
    Some((pre.uid.clone(), post.uid.clone()))
}

/// Reading of the density change between phases (renal-mass style limits).
pub fn enhancement_label(delta: f32) -> &'static str {
    if delta < -10.0 {
        "densidade menor na fase com contraste"
    } else if delta < 10.0 {
        "sem realce"
    } else if delta < 20.0 {
        "realce duvidoso"
    } else {
        "realce presente"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dicom::model::InstanceInfo;

    fn series(uid: &str, number: i32, desc: &str, contrast: &str) -> SeriesInfo {
        SeriesInfo {
            uid: uid.into(),
            number,
            description: desc.into(),
            modality: "CT".into(),
            contrast: contrast.into(),
            instances: (0..100).map(|_| InstanceInfo { has_pixels: true, frames: 1, slice_thickness: Some(1.0), ..Default::default() }).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn pairs_plain_with_portal_phase() {
        let study = StudyInfo {
            header: Default::default(),
            series: vec![
                series("pre", 2, "ABDOME SEM CONTRASTE", ""),
                series("art", 3, "Fase arterial", "IOPAMIDOL"),
                series("port", 4, "Fase portal", "IOPAMIDOL"),
            ],
        };
        assert_eq!(pick_pair(&study), Some(("pre".into(), "port".into())));
        assert_eq!(phase(&study.series[0]), Phase::Pre);
        assert_eq!(enhancement_label(3.0), "sem realce");
        assert_eq!(enhancement_label(28.0), "realce presente");
    }
}
