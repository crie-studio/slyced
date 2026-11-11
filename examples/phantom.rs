//! Generates a synthetic head CT (plus an RGB secondary capture) for testing
//! without real patient data:
//!
//!     cargo run --release --example phantom -- <output-folder> [--anterior | --sem-contraste] [--english]
//!
//! `--anterior` writes a "prior" exam of the same patient instead: another
//! study and Frame of Reference, the head shifted and rotated, and a smaller
//! lesion (for testing the comparison mode). `--sem-contraste` writes the same
//! exam's phase without contrast (vessels and lesion not enhanced).
//! `--english` writes the descriptions, institution and patient name in English
//! (screenshots for the English project page); UIDs and images are the same.

use dicom_core::{dicom_value, DataElement, PrimitiveValue, VR};
use dicom_dictionary_std::{tags, uids};
use dicom_object::{FileMetaTableBuilder, InMemDicomObject};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// `--english`: texts written into the files in English.
static ENGLISH: AtomicBool = AtomicBool::new(false);

fn text(pt: &'static str, en: &'static str) -> &'static str {
    if ENGLISH.load(Ordering::Relaxed) { en } else { pt }
}

const NX: usize = 256;
const NY: usize = 256;
const NZ: usize = 160;
const PIXEL: f64 = 0.9;
const SLICE: f64 = 1.25;

/// Current exam or prior exam of the same patient.
struct Variant {
    study: &'static str,
    series: &'static str,
    sop: &'static str,
    frame_of_ref: &'static str,
    date: &'static str,
    lesion_mm: f64,
    series_number: &'static str,
    description: &'static str,
    description_en: &'static str,
    /// Vessels and lesion enhanced.
    contrast: bool,
    /// Rotation about the patient axis (degrees) and shift (mm) of the head.
    pose: Option<(f64, [f64; 3])>,
}

const CURRENT: Variant = Variant {
    study: "2.25.11111111111111111111111111111111111",
    series: "2.25.22222222222222222222222222222222222",
    sop: "2.25.3333",
    frame_of_ref: "2.25.6666666666",
    date: "20261031",
    lesion_mm: 6.0,
    series_number: "2",
    description: "Cranio Angio 1.25mm",
    description_en: "Head Angio 1.25mm",
    contrast: true,
    pose: None,
};

const PLAIN: Variant = Variant {
    series: "2.25.22222222222222222222222222222222224",
    sop: "2.25.3335",
    series_number: "3",
    description: "Cranio sem contraste 1.25mm",
    description_en: "Head without contrast 1.25mm",
    contrast: false,
    ..CURRENT
};

const PRIOR: Variant = Variant {
    study: "2.25.11111111111111111111111111111111112",
    series: "2.25.22222222222222222222222222222222223",
    sop: "2.25.3334",
    frame_of_ref: "2.25.6666666667",
    date: "20250310",
    lesion_mm: 4.5,
    series_number: "2",
    description: "Cranio Angio 1.25mm",
    description_en: "Head Angio 1.25mm",
    contrast: true,
    pose: Some((5.0, [7.0, -9.0, 14.0])),
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let prior = args.iter().any(|a| a == "--anterior");
    let plain = args.iter().any(|a| a == "--sem-contraste");
    ENGLISH.store(args.iter().any(|a| a == "--english"), Ordering::Relaxed);
    let out = PathBuf::from(args.iter().find(|a| !a.starts_with("--")).cloned().unwrap_or_else(|| "phantom".into()));
    std::fs::create_dir_all(&out).unwrap();
    let v = if prior {
        &PRIOR
    } else if plain {
        &PLAIN
    } else {
        &CURRENT
    };
    (0..NZ).into_par_iter().for_each(|k| write_slice(&out, v, k));
    if prior || plain {
        println!("{} cortes ({}) gravados em {}", NZ, text(v.description, v.description_en), out.display());
        return;
    }
    write_rgb(&out, v.study);
    write_scout(&out, v.study);
    println!("{} cortes + localizador + 1 imagem colorida gravados em {}", NZ, out.display());
}

/// Head anatomy at patient point `p` for an exam variant.
fn hu_variant(p: [f64; 3], v: &Variant) -> f64 {
    let q = match v.pose {
        None => p,
        Some((deg, t)) => {
            // Patient coordinates of the prior exam -> anatomy coordinates.
            let (x, y, z) = (p[0] - t[0], p[1] - t[1], p[2] - t[2]);
            let (s, c) = (-deg.to_radians()).sin_cos();
            [c * x - s * y, s * x + c * y, z]
        }
    };
    let hu = hu_at(q, 0.0);
    // Hyperdense lesion in the right hemisphere.
    let r = ((q[0] + 24.0).powi(2) + (q[1] - 12.0).powi(2) + (q[2] - 38.0).powi(2)).sqrt();
    if r < v.lesion_mm && hu > 0.0 && hu < 100.0 {
        return if v.contrast { 78.0 } else { 50.0 };
    }
    // Contrast-filled vessels are plain blood without contrast.
    if !v.contrast && hu > 300.0 && hu < 350.0 {
        return 45.0;
    }
    hu
}

fn ellipsoid(p: [f64; 3], c: [f64; 3], r: [f64; 3]) -> f64 {
    ((p[0] - c[0]) / r[0]).powi(2) + ((p[1] - c[1]) / r[1]).powi(2) + ((p[2] - c[2]) / r[2]).powi(2)
}

fn hu_at(p: [f64; 3], noise: f64) -> f64 {
    let [x, y, z] = p;
    let mut hu = -1000.0;
    // Neck and cervical spine.
    if z < -35.0 && (x * x + (y - 5.0) * (y - 5.0)).sqrt() < 48.0 {
        hu = 42.0;
        if (x * x + (y - 28.0).powi(2)).sqrt() < 11.0 {
            hu = if (x * x + (y - 28.0).powi(2)).sqrt() < 8.0 { 280.0 } else { 900.0 };
        }
        if (x * x + (y + 18.0).powi(2)).sqrt() < 7.0 {
            hu = -950.0; // airway
        }
    }
    let e = ellipsoid(p, [0.0, 0.0, 15.0], [72.0, 92.0, 92.0]);
    if e < 1.0 {
        hu = if e > 0.93 {
            55.0 // scalp
        } else if e > 0.80 {
            if (0.84..0.89).contains(&e) { 650.0 } else { 1250.0 } // skull with diploe
        } else {
            let w = ellipsoid(p, [0.0, 2.0, 22.0], [48.0, 65.0, 55.0]);
            let mut b = if w < 1.0 { 29.0 } else { 38.0 };
            for s in [-1.0, 1.0] {
                if ellipsoid(p, [s * 11.0, -4.0, 26.0], [6.0, 22.0, 11.0]) < 1.0 {
                    b = 6.0;
                }
            }
            b
        };
    }
    // Eyes.
    for s in [-1.0, 1.0] {
        let d = ellipsoid(p, [s * 31.0, -70.0, 2.0], [12.0, 12.0, 12.0]);
        if d < 1.0 {
            hu = if ellipsoid(p, [s * 31.0, -80.0, 2.0], [5.0, 2.5, 5.0]) < 1.0 { 120.0 } else { 12.0 };
        }
    }
    // Contrast-filled vessels: carotids and a ring at the skull base.
    for s in [-1.0, 1.0] {
        if (-80.0..-2.0).contains(&z) && ((x - s * 21.0).powi(2) + (y - 6.0).powi(2)).sqrt() < 3.2 {
            hu = 340.0;
        }
    }
    let ring = ((x * x + (y - 4.0).powi(2)).sqrt() - 21.0).hypot(z + 2.0);
    if ring < 2.6 {
        hu = 340.0;
    }
    if ((x).powi(2) + (y + 30.0).powi(2)).sqrt() < 2.2 && (-2.0..40.0).contains(&z) {
        hu = 330.0;
    }
    if hu > -900.0 { hu + noise } else { hu + noise * 0.3 }
}

fn write_slice(out: &Path, v: &Variant, k: usize) {
    // Instance 1 is the most superior slice.
    let z = 100.0 - k as f64 * SLICE;
    let x0 = -(NX as f64 - 1.0) * PIXEL * 0.5;
    let y0 = -(NY as f64 - 1.0) * PIXEL * 0.5;
    let mut seed = (k as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 10_000) as f64 / 10_000.0 - 0.5
    };
    let mut pixels = Vec::with_capacity(NX * NY);
    for j in 0..NY {
        for i in 0..NX {
            // 3×3×3 supersampling emulates the partial-volume blur of a real scanner.
            let mut hu = 0.0;
            for (a, b, c) in (0..27).map(|s| (s % 3, (s / 3) % 3, s / 9)) {
                let o = |v: usize, d: f64| (v as f64 - 1.0) * d / 3.0;
                hu += hu_variant([x0 + i as f64 * PIXEL + o(a, PIXEL), y0 + j as f64 * PIXEL + o(b, PIXEL), z + o(c, SLICE)], v);
            }
            let hu = hu / 27.0 + (rnd() + rnd() + rnd()) * 12.0;
            pixels.push((hu + 1024.0).round().clamp(0.0, 4095.0) as u16);
        }
    }
    let sop = format!("{}{:06}", v.sop, k + 1);
    let mut o = base(v.study, uids::CT_IMAGE_STORAGE, &sop, "CT");
    o.put(DataElement::new(tags::FRAME_OF_REFERENCE_UID, VR::UI, PrimitiveValue::from(v.frame_of_ref)));
    o.put(DataElement::new(tags::STUDY_DATE, VR::DA, PrimitiveValue::from(v.date)));
    let put = |o: &mut InMemDicomObject, tag, vr, v: PrimitiveValue| {
        o.put(DataElement::new(tag, vr, v));
    };
    put(&mut o, tags::SERIES_INSTANCE_UID, VR::UI, v.series.into());
    put(&mut o, tags::SERIES_NUMBER, VR::IS, v.series_number.into());
    put(&mut o, tags::SERIES_DESCRIPTION, VR::LO, text(v.description, v.description_en).into());
    if v.contrast {
        put(&mut o, tags::CONTRAST_BOLUS_AGENT, VR::LO, "IOPAMIDOL".into());
    }
    put(&mut o, tags::BODY_PART_EXAMINED, VR::CS, "HEAD".into());
    put(&mut o, tags::INSTANCE_NUMBER, VR::IS, (k + 1).to_string().into());
    put(&mut o, tags::IMAGE_POSITION_PATIENT, VR::DS, dicom_value!(Strs, [format!("{x0:.4}"), format!("{y0:.4}"), format!("{z:.4}")]));
    put(&mut o, tags::IMAGE_ORIENTATION_PATIENT, VR::DS, dicom_value!(Strs, ["1", "0", "0", "0", "1", "0"]));
    put(&mut o, tags::SLICE_LOCATION, VR::DS, format!("{z:.4}").into());
    put(&mut o, tags::SLICE_THICKNESS, VR::DS, format!("{SLICE}").into());
    put(&mut o, tags::PIXEL_SPACING, VR::DS, dicom_value!(Strs, [format!("{PIXEL}"), format!("{PIXEL}")]));
    put(&mut o, tags::SAMPLES_PER_PIXEL, VR::US, PrimitiveValue::from(1u16));
    put(&mut o, tags::PHOTOMETRIC_INTERPRETATION, VR::CS, "MONOCHROME2".into());
    put(&mut o, tags::ROWS, VR::US, PrimitiveValue::from(NY as u16));
    put(&mut o, tags::COLUMNS, VR::US, PrimitiveValue::from(NX as u16));
    put(&mut o, tags::BITS_ALLOCATED, VR::US, PrimitiveValue::from(16u16));
    put(&mut o, tags::BITS_STORED, VR::US, PrimitiveValue::from(12u16));
    put(&mut o, tags::HIGH_BIT, VR::US, PrimitiveValue::from(11u16));
    put(&mut o, tags::PIXEL_REPRESENTATION, VR::US, PrimitiveValue::from(0u16));
    put(&mut o, tags::RESCALE_INTERCEPT, VR::DS, "-1024".into());
    put(&mut o, tags::RESCALE_SLOPE, VR::DS, "1".into());
    put(&mut o, tags::WINDOW_CENTER, VR::DS, "40".into());
    put(&mut o, tags::WINDOW_WIDTH, VR::DS, "80".into());
    put(&mut o, tags::PIXEL_DATA, VR::OW, PrimitiveValue::U16(pixels.into()));
    save(o, uids::CT_IMAGE_STORAGE, &sop, &out.join(format!("CT{:04}.dcm", k + 1)));
}

/// Sagittal localizer through the midline (series 1), for reference lines.
fn write_scout(out: &Path, study: &str) {
    let (w, h) = (256usize, 280usize);
    let px = 1.0;
    let (y0, z0) = (-128.0, 150.0);
    let mut pixels = Vec::with_capacity(w * h);
    for j in 0..h {
        for i in 0..w {
            let hu = hu_at([0.0, y0 + i as f64 * px, z0 - j as f64 * px], 0.0);
            pixels.push((hu + 1024.0).round().clamp(0.0, 4095.0) as u16);
        }
    }
    let sop = "2.25.77770000001";
    let mut o = base(study, uids::CT_IMAGE_STORAGE, sop, "CT");
    let put = |o: &mut InMemDicomObject, tag, vr, v: PrimitiveValue| {
        o.put(DataElement::new(tag, vr, v));
    };
    put(&mut o, tags::SERIES_INSTANCE_UID, VR::UI, "2.25.7777777777".into());
    put(&mut o, tags::SERIES_NUMBER, VR::IS, "1".into());
    put(&mut o, tags::SERIES_DESCRIPTION, VR::LO, text("Localizador", "Localizer").into());
    put(&mut o, tags::INSTANCE_NUMBER, VR::IS, "1".into());
    put(&mut o, tags::IMAGE_POSITION_PATIENT, VR::DS, dicom_value!(Strs, ["0", format!("{y0}"), format!("{z0}")]));
    put(&mut o, tags::IMAGE_ORIENTATION_PATIENT, VR::DS, dicom_value!(Strs, ["0", "1", "0", "0", "0", "-1"]));
    put(&mut o, tags::PIXEL_SPACING, VR::DS, dicom_value!(Strs, ["1", "1"]));
    put(&mut o, tags::SAMPLES_PER_PIXEL, VR::US, PrimitiveValue::from(1u16));
    put(&mut o, tags::PHOTOMETRIC_INTERPRETATION, VR::CS, "MONOCHROME2".into());
    put(&mut o, tags::ROWS, VR::US, PrimitiveValue::from(h as u16));
    put(&mut o, tags::COLUMNS, VR::US, PrimitiveValue::from(w as u16));
    put(&mut o, tags::BITS_ALLOCATED, VR::US, PrimitiveValue::from(16u16));
    put(&mut o, tags::BITS_STORED, VR::US, PrimitiveValue::from(12u16));
    put(&mut o, tags::HIGH_BIT, VR::US, PrimitiveValue::from(11u16));
    put(&mut o, tags::PIXEL_REPRESENTATION, VR::US, PrimitiveValue::from(0u16));
    put(&mut o, tags::RESCALE_INTERCEPT, VR::DS, "-1024".into());
    put(&mut o, tags::RESCALE_SLOPE, VR::DS, "1".into());
    put(&mut o, tags::WINDOW_CENTER, VR::DS, "300".into());
    put(&mut o, tags::WINDOW_WIDTH, VR::DS, "1600".into());
    put(&mut o, tags::PIXEL_DATA, VR::OW, PrimitiveValue::U16(pixels.into()));
    save(o, uids::CT_IMAGE_STORAGE, sop, &out.join("SCOUT001.dcm"));
}

fn write_rgb(out: &Path, study: &str) {
    let (w, h) = (320usize, 240usize);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let d = ((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt();
            rgb.extend([(255.0 * fx) as u8, (255.0 * (1.0 - d * 1.6).max(0.0)) as u8, (255.0 * fy) as u8]);
        }
    }
    let sop = "2.25.44440000001";
    let mut o = base(study, uids::SECONDARY_CAPTURE_IMAGE_STORAGE, sop, "OT");
    o.put(DataElement::new(tags::SERIES_INSTANCE_UID, VR::UI, PrimitiveValue::from("2.25.5555555555")));
    o.put(DataElement::new(tags::SERIES_NUMBER, VR::IS, PrimitiveValue::from("90")));
    o.put(DataElement::new(tags::SERIES_DESCRIPTION, VR::LO, PrimitiveValue::from(text("Captura colorida", "Color capture"))));
    o.put(DataElement::new(tags::INSTANCE_NUMBER, VR::IS, PrimitiveValue::from("1")));
    o.put(DataElement::new(tags::SAMPLES_PER_PIXEL, VR::US, PrimitiveValue::from(3u16)));
    o.put(DataElement::new(tags::PHOTOMETRIC_INTERPRETATION, VR::CS, PrimitiveValue::from("RGB")));
    o.put(DataElement::new(tags::PLANAR_CONFIGURATION, VR::US, PrimitiveValue::from(0u16)));
    o.put(DataElement::new(tags::ROWS, VR::US, PrimitiveValue::from(h as u16)));
    o.put(DataElement::new(tags::COLUMNS, VR::US, PrimitiveValue::from(w as u16)));
    o.put(DataElement::new(tags::BITS_ALLOCATED, VR::US, PrimitiveValue::from(8u16)));
    o.put(DataElement::new(tags::BITS_STORED, VR::US, PrimitiveValue::from(8u16)));
    o.put(DataElement::new(tags::HIGH_BIT, VR::US, PrimitiveValue::from(7u16)));
    o.put(DataElement::new(tags::PIXEL_REPRESENTATION, VR::US, PrimitiveValue::from(0u16)));
    o.put(DataElement::new(tags::PIXEL_DATA, VR::OB, PrimitiveValue::U8(rgb.into())));
    save(o, uids::SECONDARY_CAPTURE_IMAGE_STORAGE, sop, &out.join("SC0001.dcm"));
}

fn base(study: &str, sop_class: &str, sop: &str, modality: &str) -> InMemDicomObject {
    let mut o = InMemDicomObject::new_empty();
    let s = |o: &mut InMemDicomObject, tag, vr, v: &str| {
        o.put(DataElement::new(tag, vr, PrimitiveValue::from(v)));
    };
    s(&mut o, tags::SPECIFIC_CHARACTER_SET, VR::CS, "ISO_IR 192");
    s(&mut o, tags::SOP_CLASS_UID, VR::UI, sop_class);
    s(&mut o, tags::SOP_INSTANCE_UID, VR::UI, sop);
    s(&mut o, tags::STUDY_DATE, VR::DA, "20261031");
    s(&mut o, tags::STUDY_TIME, VR::TM, "143000");
    s(&mut o, tags::ACCESSION_NUMBER, VR::SH, "A123456");
    s(&mut o, tags::MODALITY, VR::CS, modality);
    s(&mut o, tags::INSTITUTION_NAME, VR::LO, text("Hospital Demonstração", "Demo Hospital"));
    s(&mut o, tags::STUDY_DESCRIPTION, VR::LO, text("TC de crânio (fantoma sintético)", "Head CT (synthetic phantom)"));
    s(&mut o, tags::PATIENT_NAME, VR::PN, text("FANTOMA^JOÃO DA SILVA", "PHANTOM^JOHN DOE"));
    s(&mut o, tags::PATIENT_ID, VR::LO, "000123");
    s(&mut o, tags::PATIENT_BIRTH_DATE, VR::DA, "19800215");
    s(&mut o, tags::PATIENT_SEX, VR::CS, "M");
    s(&mut o, tags::PATIENT_AGE, VR::AS, "046Y");
    s(&mut o, tags::STUDY_INSTANCE_UID, VR::UI, study);
    s(&mut o, tags::FRAME_OF_REFERENCE_UID, VR::UI, "2.25.6666666666");
    o
}

fn save(o: InMemDicomObject, sop_class: &str, sop: &str, path: &Path) {
    let file = o
        .with_meta(
            FileMetaTableBuilder::new()
                .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                .media_storage_sop_class_uid(sop_class)
                .media_storage_sop_instance_uid(sop),
        )
        .expect("meta");
    file.write_to_file(path).expect("write");
}
