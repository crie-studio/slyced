//! Second JPEG decoder for the streams the default one (jpeg-decoder, used by
//! dicom-pixeldata) rejects: 12-bit DCT ("JPEG Extended", common in CT/MR),
//! arithmetic coding and some lossless variants.

use anyhow::{anyhow, bail, Result};
use dicom_dictionary_std::tags;
use dicom_object::InMemDicomObject;
use libjpeg_turbo_rs as lj;

/// JPEG baseline, extended, lossless (process 14) and lossless SV1.
pub fn is_jpeg(ts: &str) -> bool {
    matches!(
        ts,
        "1.2.840.10008.1.2.4.50" | "1.2.840.10008.1.2.4.51" | "1.2.840.10008.1.2.4.57" | "1.2.840.10008.1.2.4.70"
    )
}

pub enum Samples {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

pub struct Image {
    pub width: usize,
    pub height: usize,
    /// 1 (gray) or 3 (RGB, interleaved).
    pub components: usize,
    pub samples: Samples,
}

/// The encapsulated JPEG stream of each frame.
pub fn frame_streams(obj: &InMemDicomObject, frames: usize) -> Result<Vec<Vec<u8>>> {
    let value = obj.element(tags::PIXEL_DATA).map_err(|_| anyhow!(tr!("sem PixelData", "no PixelData")))?.value();
    let (Some(fragments), Some(offsets)) = (value.fragments(), value.offset_table()) else {
        bail!(tr!("PixelData não está encapsulado", "PixelData is not encapsulated"));
    };
    Ok(split_frames(fragments, offsets, frames.max(1)))
}

/// Groups fragments into frames: one frame takes them all, one fragment per
/// frame is the common case, otherwise the Basic Offset Table and, without it,
/// fragments that start with an SOI marker mark the frame starts.
fn split_frames(fragments: &[Vec<u8>], offsets: &[u32], frames: usize) -> Vec<Vec<u8>> {
    if frames == 1 {
        return vec![fragments.concat()];
    }
    if fragments.len() == frames {
        return fragments.to_vec();
    }
    let starts: Vec<usize> = if offsets.len() == frames {
        // Offsets count from the first fragment's item tag (8 header bytes each).
        let mut pos = 0u64;
        let positions: Vec<u64> = fragments
            .iter()
            .map(|f| {
                let p = pos;
                pos += 8 + f.len() as u64;
                p
            })
            .collect();
        offsets.iter().filter_map(|&o| positions.iter().position(|&p| p == o as u64)).collect()
    } else {
        fragments.iter().enumerate().filter(|(_, f)| f.starts_with(&[0xFF, 0xD8])).map(|(i, _)| i).collect()
    };
    if starts.is_empty() || starts[0] != 0 {
        return vec![fragments.concat()];
    }
    starts
        .iter()
        .enumerate()
        .map(|(k, &s)| {
            let end = starts.get(k + 1).copied().unwrap_or(fragments.len());
            fragments[s..end].concat()
        })
        .collect()
}

/// Start-of-frame marker and sample precision of a JPEG stream.
fn frame_header(data: &[u8]) -> Option<(u8, u8)> {
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        if marker == 0xFF || marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            i += if marker == 0xFF { 1 } else { 2 };
            continue;
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            return data.get(i + 4).map(|&p| (marker, p));
        }
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        i += 2 + len;
    }
    None
}

pub fn decode(data: &[u8]) -> Result<Image> {
    let start = memchr::memmem::find(data, &[0xFF, 0xD8]).ok_or_else(|| anyhow!(tr!("marcador SOI ausente", "SOI marker missing")))?;
    let data = &data[start..];
    let (sof, precision) = frame_header(data).ok_or_else(|| anyhow!(tr!("cabeçalho SOF ausente", "SOF header missing")))?;
    let lossless = matches!(sof, 0xC3 | 0xC7 | 0xCB | 0xCF);
    if lossless || precision > 12 {
        let img = lj::precision::decompress_lossless_arbitrary_lenient(data)?.0;
        return Ok(Image {
            width: img.width,
            height: img.height,
            components: img.num_components,
            samples: Samples::U16(img.data),
        });
    }
    if precision > 8 {
        let img = lj::precision::decompress_12bit_lenient(data)?.0;
        return Ok(Image {
            width: img.width,
            height: img.height,
            components: img.num_components,
            samples: Samples::U16(img.data.into_iter().map(|v| v.clamp(0, 4095) as u16).collect()),
        });
    }
    let probe = lj::decompress_lenient(data)?;
    let format = if probe.pixel_format.bytes_per_pixel() == 1 { lj::PixelFormat::Grayscale } else { lj::PixelFormat::Rgb };
    let img = if probe.pixel_format == format { probe } else { lj::decompress_to(data, format)? };
    Ok(Image {
        width: img.width,
        height: img.height,
        components: format.bytes_per_pixel(),
        samples: Samples::U8(img.data),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: usize, h: usize, max: u16) -> Vec<u16> {
        (0..w * h).map(|i| ((i % w) * max as usize / (w - 1)) as u16 ^ ((i / w) as u16 & 7)).collect()
    }

    #[test]
    fn lossless_16bit_roundtrip_is_exact() {
        let px = ramp(64, 48, 65535);
        let jpg = lj::precision::compress_lossless_arbitrary(&px, 64, 48, 1, 16, 1, 0).unwrap();
        let img = decode(&jpg).unwrap();
        assert_eq!((img.width, img.height, img.components), (64, 48, 1));
        let Samples::U16(out) = img.samples else { panic!("esperado 16 bits") };
        assert_eq!(out, px);
    }

    #[test]
    fn lossless_12bit_all_predictors() {
        let px = ramp(40, 30, 4095);
        for predictor in 1..=7 {
            let jpg = lj::precision::compress_lossless_arbitrary(&px, 40, 30, 1, 12, predictor, 0).unwrap();
            let Samples::U16(out) = decode(&jpg).unwrap().samples else { panic!() };
            assert_eq!(out, px, "preditor {predictor}");
        }
    }

    #[test]
    fn extended_12bit_is_close() {
        let px: Vec<i16> = ramp(64, 64, 4095).into_iter().map(|v| v as i16).collect();
        let jpg = lj::compress_12bit(&px, 64, 64, 1, 95, lj::Subsampling::S444).unwrap();
        assert_eq!(frame_header(&jpg).map(|h| h.1), Some(12));
        let Samples::U16(out) = decode(&jpg).unwrap().samples else { panic!() };
        let err = px.iter().zip(&out).map(|(&a, &b)| (a as i32 - b as i32).abs()).max().unwrap();
        assert!(err < 120, "erro máximo {err}");
    }

    #[test]
    fn frames_split_by_offset_table_and_soi() {
        let a = vec![0xFF, 0xD8, 1, 2];
        let b = vec![3, 4];
        let c = vec![0xFF, 0xD8, 5, 6];
        let frags = vec![a.clone(), b.clone(), c.clone()];
        let by_soi = split_frames(&frags, &[], 2);
        assert_eq!(by_soi, vec![[a.clone(), b.clone()].concat(), c.clone()]);
        let by_bot = split_frames(&frags, &[0, (8 + 4 + 8 + 2) as u32], 2);
        assert_eq!(by_bot, by_soi);
        assert_eq!(split_frames(&frags, &[], 1), vec![[a, b, c].concat()]);
    }
}
