//! Findings marked on the images ("achados"): a labelled point plus the view
//! it was seen in, kept per study across sessions.

use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: u64,
    pub label: String,
    #[serde(default)]
    pub note: String,
    pub study_uid: String,
    pub series_uid: String,
    #[serde(default)]
    pub frame_of_ref: String,
    /// Patient position (mm), when the image has geometry.
    pub point: Option<[f64; 3]>,
    /// Image and pixel, for images without geometry (radiographs).
    pub slice: usize,
    pub pixel: [f32; 2],
    /// Window center and width.
    pub window: [f32; 2],
    /// Zoom relative to "fit to window".
    pub zoom: Option<f32>,
    /// Marked on an MPR plane (reopens in MPR).
    pub mpr: bool,
    /// Seconds since the Unix epoch.
    pub created: u64,
}

impl Bookmark {
    pub fn point(&self) -> Option<DVec3> {
        self.point.map(DVec3::from_array)
    }
}

pub fn load(path: &Path) -> Vec<Bookmark> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(path: &Path, items: &[Bookmark]) {
    match serde_json::to_vec_pretty(items) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                log::warn!("não foi possível salvar os achados: {e}");
            }
        }
        Err(e) => log::warn!("achados: {e}"),
    }
}

pub fn now() -> u64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_through_json() {
        let dir = std::env::temp_dir().join(format!("slyced-bm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("achados.json");
        let b = Bookmark {
            id: 7,
            label: "Apêndice espessado".into(),
            note: String::new(),
            study_uid: "1.2".into(),
            series_uid: "1.2.3".into(),
            frame_of_ref: "1.2.9".into(),
            point: Some([10.0, -20.5, 300.0]),
            slice: 42,
            pixel: [100.0, 200.0],
            window: [60.0, 400.0],
            zoom: Some(2.0),
            mpr: true,
            created: now(),
        };
        save(&path, std::slice::from_ref(&b));
        let back = load(&path);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].label, b.label);
        assert_eq!(back[0].point(), Some(DVec3::new(10.0, -20.5, 300.0)));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
