//! Typical CT densities (Hounsfield units), shown as a reading reference
//! while measuring point densities.

use crate::i18n::Tr;

pub struct Tissue {
    pub name: Tr,
    pub min: f32,
    pub max: f32,
    pub typical: f32,
}

/// Unenhanced values from common radiology references; real values vary with
/// the scanner, the kVp and the patient.
pub const TISSUES: &[Tissue] = &[
    Tissue { name: Tr("Ar", "Air"), min: -1100.0, max: -950.0, typical: -1000.0 },
    Tissue { name: Tr("Pulmão", "Lung"), min: -950.0, max: -500.0, typical: -750.0 },
    Tissue { name: Tr("Gordura", "Fat"), min: -150.0, max: -50.0, typical: -100.0 },
    Tissue { name: Tr("Água, LCR, bile, urina", "Water, CSF, bile, urine"), min: -10.0, max: 20.0, typical: 5.0 },
    Tissue { name: Tr("Substância branca", "White matter"), min: 20.0, max: 34.0, typical: 28.0 },
    Tissue { name: Tr("Rim", "Kidney"), min: 30.0, max: 50.0, typical: 40.0 },
    Tissue { name: Tr("Sangue", "Blood"), min: 30.0, max: 45.0, typical: 40.0 },
    Tissue { name: Tr("Substância cinzenta", "Gray matter"), min: 35.0, max: 45.0, typical: 40.0 },
    Tissue { name: Tr("Músculo", "Muscle"), min: 35.0, max: 55.0, typical: 45.0 },
    Tissue { name: Tr("Baço", "Spleen"), min: 40.0, max: 60.0, typical: 50.0 },
    Tissue { name: Tr("Fígado", "Liver"), min: 50.0, max: 70.0, typical: 60.0 },
    Tissue { name: Tr("Hematoma agudo", "Acute hematoma"), min: 50.0, max: 90.0, typical: 70.0 },
    Tissue { name: Tr("Contraste nos vasos", "Contrast in vessels"), min: 150.0, max: 500.0, typical: 300.0 },
    Tissue { name: Tr("Osso esponjoso", "Cancellous bone"), min: 200.0, max: 700.0, typical: 400.0 },
    Tissue { name: Tr("Osso cortical", "Cortical bone"), min: 700.0, max: 2000.0, typical: 1200.0 },
    Tissue { name: Tr("Metal", "Metal"), min: 2000.0, max: f32::MAX, typical: 3000.0 },
];

impl Tissue {
    pub fn contains(&self, hu: f32) -> bool {
        (self.min..=self.max).contains(&hu)
    }

    /// Range as text, e.g. "−950 a −500" ("−950 to −500") or "> 2000".
    pub fn range_text(&self) -> String {
        let n = |v: f32| format!("{v:.0}").replace('-', "−");
        if self.max == f32::MAX {
            format!("> {}", n(self.min))
        } else if self.min <= -1100.0 {
            format!("< {}", n(self.max))
        } else {
            trf!("{} a {}", "{} to {}", n(self.min), n(self.max))
        }
    }
}

/// Tissue whose range holds `hu` and whose typical value is closest
/// (or simply the closest typical value when no range holds it).
pub fn classify(hu: f32) -> &'static Tissue {
    let dist = |t: &Tissue| (t.typical - hu).abs();
    TISSUES
        .iter()
        .filter(|t| t.contains(hu))
        .min_by(|a, b| dist(a).total_cmp(&dist(b)))
        .or_else(|| TISSUES.iter().min_by(|a, b| dist(a).total_cmp(&dist(b))))
        .expect("tissue table is not empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_densities() {
        assert_eq!(classify(-1000.0).name.pt(), "Ar");
        assert_eq!(classify(-98.0).name.pt(), "Gordura");
        assert_eq!(classify(3.0).name.pt(), "Água, LCR, bile, urina");
        assert_eq!(classify(62.0).name.pt(), "Fígado");
        assert_eq!(classify(1400.0).name.pt(), "Osso cortical");
        // Between ranges: the nearest typical value.
        assert_eq!(classify(-460.0).name.pt(), "Pulmão");
        assert_eq!(TISSUES[1].range_text(), "−950 a −500");
        assert_eq!(TISSUES[0].range_text(), "< −950");
    }
}
