//! Encapsulated JPEG pixel data, including the 12-bit and lossless variants
//! the default decoder rejects.

use dicom_core::value::{PixelFragmentSequence, Value};
use dicom_core::{DataElement, PrimitiveValue, VR};
use dicom_dictionary_std::{tags, uids};
use dicom_object::{FileMetaTableBuilder, InMemDicomObject};
use libjpeg_turbo_rs as lj;
use slyced::dicom::decode::{decode_file, FramePixels, GrayData};

const W: usize = 32;
const H: usize = 24;

/// Stored value of pixel `i` in frame `f`: a smooth ramp (JPEG-friendly).
fn stored(f: usize, i: usize) -> u16 {
    (200 + f * 300 + (i % W) * 40 + (i / W) * 20) as u16
}

fn write(path: &std::path::Path, ts: &str, frames: Vec<Vec<Vec<u8>>>, bits_stored: u16) {
    let mut o = InMemDicomObject::new_empty();
    let mut put = |tag, vr, v: PrimitiveValue| {
        o.put(DataElement::new(tag, vr, v));
    };
    put(tags::SOP_CLASS_UID, VR::UI, uids::CT_IMAGE_STORAGE.into());
    put(tags::SOP_INSTANCE_UID, VR::UI, "2.25.777".into());
    put(tags::STUDY_INSTANCE_UID, VR::UI, "2.25.70".into());
    put(tags::SERIES_INSTANCE_UID, VR::UI, "2.25.71".into());
    put(tags::MODALITY, VR::CS, "CT".into());
    put(tags::SAMPLES_PER_PIXEL, VR::US, PrimitiveValue::from(1u16));
    put(tags::PHOTOMETRIC_INTERPRETATION, VR::CS, "MONOCHROME2".into());
    put(tags::NUMBER_OF_FRAMES, VR::IS, frames.len().to_string().into());
    put(tags::ROWS, VR::US, PrimitiveValue::from(H as u16));
    put(tags::COLUMNS, VR::US, PrimitiveValue::from(W as u16));
    put(tags::BITS_ALLOCATED, VR::US, PrimitiveValue::from(16u16));
    put(tags::BITS_STORED, VR::US, PrimitiveValue::from(bits_stored));
    put(tags::HIGH_BIT, VR::US, PrimitiveValue::from(bits_stored - 1));
    put(tags::PIXEL_REPRESENTATION, VR::US, PrimitiveValue::from(0u16));
    put(tags::RESCALE_INTERCEPT, VR::DS, "-1024".into());
    put(tags::RESCALE_SLOPE, VR::DS, "1".into());
    let fragments: Vec<Vec<u8>> = frames
        .into_iter()
        .flatten()
        .map(|mut f| {
            if f.len() % 2 == 1 {
                f.push(0);
            }
            f
        })
        .collect();
    o.put(DataElement::new(
        tags::PIXEL_DATA,
        VR::OB,
        Value::PixelSequence(PixelFragmentSequence::new(Vec::<u32>::new(), fragments)),
    ));
    o.with_meta(
        FileMetaTableBuilder::new()
            .transfer_syntax(ts)
            .media_storage_sop_class_uid(uids::CT_IMAGE_STORAGE)
            .media_storage_sop_instance_uid("2.25.777"),
    )
    .unwrap()
    .write_to_file(path)
    .unwrap();
}

/// Splits a JPEG stream in two fragments, as some PACS do.
fn two_fragments(jpg: Vec<u8>) -> Vec<Vec<u8>> {
    let mid = (jpg.len() / 2) & !1;
    vec![jpg[..mid].to_vec(), jpg[mid..].to_vec()]
}

fn hu(frame: &slyced::dicom::decode::DecodedFrame) -> Vec<i32> {
    match &frame.pixels {
        FramePixels::Gray(GrayData::I16(v)) => v.iter().map(|&x| x as i32).collect(),
        FramePixels::Gray(GrayData::F32(v)) => v.iter().map(|&x| x as i32).collect(),
        FramePixels::Rgb(_) => panic!("esperado tons de cinza"),
    }
}

#[test]
fn jpeg_extended_12bit_multiframe() {
    let dir = std::env::temp_dir().join(format!("slyced-jpeg12-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ext12.dcm");
    let frames = (0..3)
        .map(|f| {
            let px: Vec<i16> = (0..W * H).map(|i| stored(f, i) as i16).collect();
            two_fragments(lj::compress_12bit(&px, W, H, 1, 98, lj::Subsampling::S444).unwrap())
        })
        .collect();
    write(&path, uids::JPEG_EXTENDED12_BIT, frames, 12);

    let decoded = decode_file(&path).expect("JPEG 12 bits deve decodificar");
    assert_eq!(decoded.len(), 3);
    for (f, frame) in decoded.iter().enumerate() {
        assert_eq!((frame.width, frame.height, frame.frame_index as usize), (W, H, f));
        let worst = hu(frame)
            .iter()
            .enumerate()
            .map(|(i, &v)| (v - (stored(f, i) as i32 - 1024)).abs())
            .max()
            .unwrap();
        assert!(worst <= 24, "quadro {f}: erro {worst} HU");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn jpeg_lossless_multiframe_is_exact() {
    let dir = std::env::temp_dir().join(format!("slyced-jpegll-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ll.dcm");
    let frames = (0..2)
        .map(|f| {
            let px: Vec<u16> = (0..W * H).map(|i| stored(f, i)).collect();
            two_fragments(lj::precision::compress_lossless_arbitrary(&px, W, H, 1, 16, 1, 0).unwrap())
        })
        .collect();
    write(&path, uids::JPEG_LOSSLESS_SV1, frames, 16);

    let decoded = decode_file(&path).expect("JPEG lossless deve decodificar");
    assert_eq!(decoded.len(), 2);
    for (f, frame) in decoded.iter().enumerate() {
        let expected: Vec<i32> = (0..W * H).map(|i| stored(f, i) as i32 - 1024).collect();
        assert_eq!(hu(frame), expected, "quadro {f}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
