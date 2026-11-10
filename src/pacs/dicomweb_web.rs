//! DICOMweb in the web version: not available yet (the Windows client uses
//! ureq, which does not run in the browser; a fetch-based client would also
//! need the server to allow CORS).

use super::scu::{RetrieveProgress, SeriesResult, StudyQuery, StudyResult};
use crate::config::PacsNode;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

const UNAVAILABLE: &str = "PACS indisponível na versão web: use o Slyced para Windows.";

pub fn echo(_node: &PacsNode) -> Result<Duration> {
    bail!(UNAVAILABLE)
}

pub fn find_studies(_node: &PacsNode, _q: &StudyQuery) -> Result<Vec<StudyResult>> {
    bail!(UNAVAILABLE)
}

pub fn find_series(_node: &PacsNode, _study_uid: &str) -> Result<Vec<SeriesResult>> {
    bail!(UNAVAILABLE)
}

pub fn retrieve(
    _node: &PacsNode,
    _study_uid: &str,
    _series_uid: Option<&str>,
    _store_root: &Path,
    _cancel: &AtomicBool,
    _on_progress: impl FnMut(RetrieveProgress),
) -> Result<(Vec<PathBuf>, RetrieveProgress)> {
    bail!(UNAVAILABLE)
}
