//! Helpers shared by the integration tests.

use dicom_core::{dicom_value, DataElement, PrimitiveValue, VR};
use dicom_dictionary_std::{tags, uids};
use dicom_object::{FileMetaTableBuilder, InMemDicomObject};

/// Writes slice `k` of a tiny 4×3 CT series (study 2.25.1, series 2.25.2) at height `z`.
/// The stored value encodes the slice index: HU of pixel 0 is `k * 100`.
pub fn write_slice(dir: &std::path::Path, k: usize, z: f64) {
    let (w, h) = (4usize, 3usize);
    let mut o = InMemDicomObject::new_empty();
    let mut put = |tag, vr, v: PrimitiveValue| {
        o.put(DataElement::new(tag, vr, v));
    };
    let sop = format!("2.25.900{k}");
    put(tags::SOP_CLASS_UID, VR::UI, uids::CT_IMAGE_STORAGE.into());
    put(tags::SOP_INSTANCE_UID, VR::UI, sop.as_str().into());
    put(tags::STUDY_INSTANCE_UID, VR::UI, "2.25.1".into());
    put(tags::SERIES_INSTANCE_UID, VR::UI, "2.25.2".into());
    put(tags::MODALITY, VR::CS, "CT".into());
    put(tags::PATIENT_NAME, VR::PN, "TESTE^UNIT".into());
    put(tags::INSTANCE_NUMBER, VR::IS, (k + 1).to_string().into());
    put(tags::IMAGE_POSITION_PATIENT, VR::DS, dicom_value!(Strs, ["0", "0", format!("{z}")]));
    put(tags::IMAGE_ORIENTATION_PATIENT, VR::DS, dicom_value!(Strs, ["1", "0", "0", "0", "1", "0"]));
    put(tags::PIXEL_SPACING, VR::DS, dicom_value!(Strs, ["0.5", "0.7"]));
    put(tags::SAMPLES_PER_PIXEL, VR::US, PrimitiveValue::from(1u16));
    put(tags::PHOTOMETRIC_INTERPRETATION, VR::CS, "MONOCHROME2".into());
    put(tags::ROWS, VR::US, PrimitiveValue::from(h as u16));
    put(tags::COLUMNS, VR::US, PrimitiveValue::from(w as u16));
    put(tags::BITS_ALLOCATED, VR::US, PrimitiveValue::from(16u16));
    put(tags::BITS_STORED, VR::US, PrimitiveValue::from(12u16));
    put(tags::HIGH_BIT, VR::US, PrimitiveValue::from(11u16));
    put(tags::PIXEL_REPRESENTATION, VR::US, PrimitiveValue::from(0u16));
    put(tags::RESCALE_INTERCEPT, VR::DS, "-1024".into());
    put(tags::RESCALE_SLOPE, VR::DS, "1".into());
    // The stored value encodes the slice index so the order can be verified.
    let px: Vec<u16> = (0..w * h).map(|i| (1024 + k * 100 + i) as u16).collect();
    put(tags::PIXEL_DATA, VR::OW, PrimitiveValue::U16(px.into()));
    let file = o
        .with_meta(
            FileMetaTableBuilder::new()
                .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                .media_storage_sop_class_uid(uids::CT_IMAGE_STORAGE)
                .media_storage_sop_instance_uid(sop.as_str()),
        )
        .unwrap();
    // No extension, as on most CDs.
    file.write_to_file(dir.join(format!("IM{k}"))).unwrap();
}
