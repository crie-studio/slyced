//! Uniform access to DICOM files on disk and inside ZIP archives.
//!
//! A ZIP entry is addressed by a virtual path such as
//! `C:\exames\estudo.zip|DICOM/0001/IM1`: `|` cannot appear in Windows file
//! names, so the first one always separates the archive from the entry name.
//! Nothing is ever extracted to disk.

use anyhow::{bail, Context as _, Result};
use dicom_dictionary_std::tags;
use dicom_object::file::ReadPreamble;
use dicom_object::{DefaultDicomObject, OpenFileOptions};
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use zip::read::{ZipArchive, ZipArchiveMetadata};

pub const SEP: char = '|';

pub fn is_zip(path: &Path) -> bool {
    split_entry(path).is_none() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

/// Virtual path of `name` inside `archive`.
pub fn entry_path(archive: &Path, name: &str) -> PathBuf {
    let mut s = OsString::from(archive.as_os_str());
    s.push(SEP.to_string());
    s.push(name);
    PathBuf::from(s)
}

/// Splits a virtual path into `(archive, entry name)`; `None` for plain files.
pub fn split_entry(path: &Path) -> Option<(PathBuf, String)> {
    let s = path.to_string_lossy();
    let (archive, name) = s.split_once(SEP)?;
    Some((PathBuf::from(archive), name.to_string()))
}

/// Parsed central directories, keyed by archive path and validated against the
/// file's size and modification time so a changed archive is re-read.
struct CachedIndex {
    path: PathBuf,
    stamp: (u64, Option<SystemTime>),
    metadata: Arc<ZipArchiveMetadata>,
}

static INDEXES: Mutex<Vec<CachedIndex>> = Mutex::new(Vec::new());

fn stamp(path: &Path) -> Result<(u64, Option<SystemTime>)> {
    let m = std::fs::metadata(path).with_context(|| format!("abrindo {}", path.display()))?;
    Ok((m.len(), m.modified().ok()))
}

/// Opens the archive with a fresh file handle (closed when the result is dropped,
/// so the ZIP is never kept locked), reusing the cached central directory.
fn open_archive(path: &Path) -> Result<ZipArchive<BufReader<File>>> {
    let st = stamp(path)?;
    let file = BufReader::new(File::open(path).with_context(|| format!("abrindo {}", path.display()))?);
    let cached = INDEXES
        .lock()
        .ok()
        .and_then(|c| c.iter().find(|i| i.path == path && i.stamp == st).map(|i| i.metadata.clone()));
    if let Some(metadata) = cached {
        // SAFETY: the metadata was parsed from this same file, unchanged since (size and mtime match).
        return Ok(unsafe { ZipArchive::unsafe_new_with_metadata(file, metadata) });
    }
    let archive = ZipArchive::new(file).with_context(|| format!("ZIP inválido: {}", path.display()))?;
    if let Ok(mut c) = INDEXES.lock() {
        c.retain(|i| i.path != path);
        c.push(CachedIndex { path: path.to_path_buf(), stamp: st, metadata: archive.metadata() });
        if c.len() > 16 {
            c.remove(0);
        }
    }
    Ok(archive)
}

#[derive(Default, Debug)]
pub struct ZipListing {
    pub entries: Vec<String>,
    /// Entries skipped because they are password protected.
    pub encrypted: usize,
}

pub fn list_zip(path: &Path) -> Result<ZipListing> {
    let mut archive = open_archive(path)?;
    let mut listing = ZipListing::default();
    for i in 0..archive.len() {
        let Ok(f) = archive.by_index_raw(i) else { continue };
        if f.is_dir() {
            continue;
        }
        if f.encrypted() {
            listing.encrypted += 1;
            continue;
        }
        if let Ok(name) = f.name() {
            listing.entries.push(name.into_owned());
        }
    }
    Ok(listing)
}

/// Files written without the 128-byte preamble start directly with a group 0002 or 0008 tag.
pub fn plausible_raw_dataset(b: &[u8]) -> bool {
    b.len() >= 8 && (b[0..2] == [0x02, 0x00] || b[0..2] == [0x08, 0x00]) && b[2] < 0x80 && b[3] == 0
}

/// Opens a DICOM object from disk or from a ZIP entry. With `header_only`
/// reading stops before the pixel data, which also avoids decompressing the
/// rest of a ZIP entry.
pub fn open_dicom(path: &Path, header_only: bool) -> Result<DefaultDicomObject> {
    let mut opts = OpenFileOptions::new();
    if header_only {
        opts = opts.read_until(tags::PIXEL_DATA);
    }
    let Some((zip_path, name)) = split_entry(path) else {
        return opts.open_file(path).with_context(|| format!("abrindo {}", path.display()));
    };
    let mut archive = open_archive(&zip_path)?;
    let mut entry = archive.by_name(&name).with_context(|| format!("{name} em {}", zip_path.display()))?;
    let mut head = [0u8; 132];
    let mut n = 0;
    while n < head.len() {
        match entry.read(&mut head[n..])? {
            0 => break,
            k => n += k,
        }
    }
    // Strip the preamble ourselves: the stream cannot be rewound to detect it.
    let start = if n == 132 && &head[128..132] == b"DICM" {
        128
    } else if plausible_raw_dataset(&head[..n]) {
        0
    } else {
        bail!("{name} não é um arquivo DICOM");
    };
    let reader = Cursor::new(&head[start..n]).chain(entry);
    opts.read_preamble(ReadPreamble::Never)
        .from_reader(reader)
        .with_context(|| format!("lendo {name} em {}", zip_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_paths_roundtrip() {
        let p = entry_path(Path::new(r"C:\exames\estudo.zip"), "DICOM/0001/IM1");
        let (archive, name) = split_entry(&p).unwrap();
        assert_eq!(archive, PathBuf::from(r"C:\exames\estudo.zip"));
        assert_eq!(name, "DICOM/0001/IM1");
        assert!(is_zip(Path::new(r"C:\x\A.ZIP")));
        assert!(!is_zip(&p));
        assert!(split_entry(Path::new(r"C:\exames\IM1")).is_none());
    }
}
