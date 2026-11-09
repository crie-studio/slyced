//! Lenient accessors: malformed or missing attributes become `None`/empty
//! instead of errors, since real-world files are often imperfect.

use dicom_core::Tag;
use dicom_object::InMemDicomObject;

pub fn text(obj: &InMemDicomObject, tag: Tag) -> String {
    obj.element_opt(tag)
        .ok()
        .flatten()
        .and_then(|e| e.to_str().ok())
        .map(|s| s.trim_matches(|c: char| c.is_whitespace() || c == '\0').to_string())
        .unwrap_or_default()
}

pub fn int(obj: &InMemDicomObject, tag: Tag) -> Option<i32> {
    let e = obj.element_opt(tag).ok().flatten()?;
    e.to_int::<i32>().ok().or_else(|| e.to_float64().ok().map(|f| f.round() as i32))
}

pub fn float(obj: &InMemDicomObject, tag: Tag) -> Option<f64> {
    obj.element_opt(tag).ok().flatten()?.to_float64().ok().filter(|v| v.is_finite())
}

pub fn floats(obj: &InMemDicomObject, tag: Tag) -> Option<Vec<f64>> {
    let v = obj.element_opt(tag).ok().flatten()?.to_multi_float64().ok()?;
    (!v.is_empty() && v.iter().all(|x| x.is_finite())).then_some(v)
}

pub fn seq_item(obj: &InMemDicomObject, tag: Tag, index: usize) -> Option<&InMemDicomObject> {
    obj.element_opt(tag).ok().flatten()?.items()?.get(index)
}
