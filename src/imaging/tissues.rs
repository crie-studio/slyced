//! Typical CT densities (Hounsfield units), shown as a reading reference
//! while measuring point densities.

pub struct Tissue {
    pub name: &'static str,
    pub min: f32,
    pub max: f32,
    pub typical: f32,
}

/// Unenhanced values from common radiology references; real values vary with
/// the scanner, the kVp and the patient.
pub const TISSUES: &[Tissue] = &[
    Tissue { name: "Ar", min: -1100.0, max: -950.0, typical: -1000.0 },
    Tissue { name: "Pulmão", min: -950.0, max: -500.0, typical: -750.0 },
    Tissue { name: "Gordura", min: -150.0, max: -50.0, typical: -100.0 },
    Tissue { name: "Água, LCR, bile, urina", min: -10.0, max: 20.0, typical: 5.0 },
    Tissue { name: "Substância branca", min: 20.0, max: 34.0, typical: 28.0 },
    Tissue { name: "Rim", min: 30.0, max: 50.0, typical: 40.0 },
    Tissue { name: "Sangue", min: 30.0, max: 45.0, typical: 40.0 },
    Tissue { name: "Substância cinzenta", min: 35.0, max: 45.0, typical: 40.0 },
    Tissue { name: "Músculo", min: 35.0, max: 55.0, typical: 45.0 },
    Tissue { name: "Baço", min: 40.0, max: 60.0, typical: 50.0 },
    Tissue { name: "Fígado", min: 50.0, max: 70.0, typical: 60.0 },
    Tissue { name: "Hematoma agudo", min: 50.0, max: 90.0, typical: 70.0 },
    Tissue { name: "Contraste nos vasos", min: 150.0, max: 500.0, typical: 300.0 },
    Tissue { name: "Osso esponjoso", min: 200.0, max: 700.0, typical: 400.0 },
    Tissue { name: "Osso cortical", min: 700.0, max: 2000.0, typical: 1200.0 },
    Tissue { name: "Metal", min: 2000.0, max: f32::MAX, typical: 3000.0 },
];

impl Tissue {
    pub fn contains(&self, hu: f32) -> bool {
        (self.min..=self.max).contains(&hu)
    }

    /// Range as text, e.g. "−950 a −500" or "> 2000".
    pub fn range_text(&self) -> String {
        let n = |v: f32| format!("{v:.0}").replace('-', "−");
        if self.max == f32::MAX {
            format!("> {}", n(self.min))
        } else if self.min <= -1100.0 {
            format!("< {}", n(self.max))
        } else {
            format!("{} a {}", n(self.min), n(self.max))
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
        assert_eq!(classify(-1000.0).name, "Ar");
        assert_eq!(classify(-98.0).name, "Gordura");
        assert_eq!(classify(3.0).name, "Água, LCR, bile, urina");
        assert_eq!(classify(62.0).name, "Fígado");
        assert_eq!(classify(1400.0).name, "Osso cortical");
        // Between ranges: the nearest typical value.
        assert_eq!(classify(-460.0).name, "Pulmão");
        assert_eq!(TISSUES[1].range_text(), "−950 a −500");
        assert_eq!(TISSUES[0].range_text(), "< −950");
    }
}
