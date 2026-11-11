//! Flattened attribute listing for the "DICOM tags" inspector.

use dicom_core::dictionary::{DataDictionary, DataDictionaryEntry};
use dicom_core::header::Header;
use dicom_core::{Tag, VR};
use dicom_dictionary_std::StandardDataDictionary;
use dicom_object::InMemDicomObject;
use std::path::Path;

pub struct TagRow {
    pub depth: usize,
    pub tag: Tag,
    pub vr: String,
    pub name: String,
    pub value: String,
}

pub fn dump(path: &Path) -> anyhow::Result<Vec<TagRow>> {
    let obj = super::source::open_dicom(path, true)?;
    let mut rows = Vec::new();
    for e in obj.meta().to_element_iter() {
        rows.push(TagRow {
            depth: 0,
            tag: e.tag(),
            vr: e.vr().to_string().to_string(),
            name: name_of(e.tag()),
            value: e.to_str().map(|s| clean(&s)).unwrap_or_default(),
        });
    }
    walk(&obj, 0, &mut rows);
    Ok(rows)
}

fn walk(obj: &InMemDicomObject, depth: usize, rows: &mut Vec<TagRow>) {
    for e in obj.iter() {
        let tag = e.tag();
        let vr = e.vr();
        let value = if let Some(items) = e.items() {
            format!("{} item(s)", items.len())
        } else if matches!(vr, VR::OB | VR::OW | VR::OF | VR::OD | VR::OL | VR::OV | VR::UN) {
            trf!("<binário, {} bytes>", "<binary, {} bytes>", e.to_bytes().map(|b| b.len()).unwrap_or(0))
        } else {
            e.to_str().map(|s| clean(&s)).unwrap_or_else(|_| "<?>".into())
        };
        rows.push(TagRow { depth, tag, vr: vr.to_string().to_string(), name: name_of(tag), value });
        if let Some(items) = e.items() {
            for (i, item) in items.iter().enumerate() {
                rows.push(TagRow {
                    depth: depth + 1,
                    tag: Tag(0xFFFE, 0xE000),
                    vr: String::new(),
                    name: format!("Item #{}", i + 1),
                    value: String::new(),
                });
                walk(item, depth + 2, rows);
            }
        }
    }
}

fn name_of(tag: Tag) -> String {
    if tag.group() % 2 == 1 {
        return tr!("Privado", "Private").into();
    }
    StandardDataDictionary.by_tag(tag).map(|e| e.alias().to_string()).unwrap_or_else(|| tr!("Desconhecido", "Unknown").into())
}

fn clean(s: &str) -> String {
    let s = s.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    if s.chars().count() > 160 {
        let cut: String = s.chars().take(160).collect();
        format!("{cut}…")
    } else {
        s.to_string()
    }
}
