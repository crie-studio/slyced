//! Files handed over by the browser (web version), kept in memory.
//!
//! Each batch the user opens gets its own virtual folder (`/navegador/1`,
//! `/navegador/2`, ...) and is then scanned and read exactly like a folder on
//! disk (see [`super::source`] and [`super::scan`]). The Windows version never
//! adds anything, so every lookup there is a quick miss.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

pub const ROOT: &str = "/navegador";

static FILES: RwLock<BTreeMap<String, Arc<[u8]>>> = RwLock::new(BTreeMap::new());

/// Map key of a virtual path; `None` for anything else.
fn key(path: &Path) -> Option<String> {
    let s = path.to_str()?;
    (s == ROOT || s.starts_with(&format!("{ROOT}/"))).then(|| s.replace('\\', "/"))
}

/// A new, empty virtual folder for the next batch of files.
pub fn new_folder() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    format!("{ROOT}/{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Adds `relative` (e.g. `CD/DICOM/IM1`) to `folder`.
pub fn insert(folder: &str, relative: &str, bytes: Arc<[u8]>) {
    let path = format!("{folder}/{}", relative.replace('\\', "/").trim_start_matches('/'));
    if let Ok(mut files) = FILES.write() {
        files.insert(path, bytes);
    }
}

pub fn get(path: &Path) -> Option<Arc<[u8]>> {
    let k = key(path)?;
    FILES.read().ok()?.get(&k).cloned()
}

/// Every file at or below `path` when it is a virtual path (`None` otherwise).
pub fn files_under(path: &Path) -> Option<Vec<PathBuf>> {
    let k = key(path)?;
    let dir = format!("{}/", k.trim_end_matches('/'));
    let files = FILES.read().ok()?;
    Some(files.keys().filter(|p| **p == k || p.starts_with(&dir)).map(PathBuf::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_kept_apart() {
        let a = new_folder();
        let b = new_folder();
        insert(&a, "CD/IM1", Arc::from(&b"x"[..]));
        insert(&a, "/CD\\IM2", Arc::from(&b"y"[..]));
        insert(&b, "IM1", Arc::from(&b"z"[..]));
        assert_eq!(files_under(Path::new(&a)).unwrap().len(), 2);
        assert_eq!(&*get(Path::new(&format!("{a}/CD/IM2"))).unwrap(), b"y");
        assert_eq!(files_under(Path::new(&format!("{b}/IM1"))).unwrap().len(), 1);
        assert!(files_under(Path::new(r"C:\exames")).is_none());
        assert!(get(Path::new("/navegadorX/1")).is_none());
    }
}
