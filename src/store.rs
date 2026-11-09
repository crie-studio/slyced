//! Exams downloaded from the PACS (the "PACS" folder in the data folder): how
//! much space they take, and their removal after some days, on request or when
//! the program closes. Folders are named by study UID (see
//! `pacs::dimse::instance_path`); studies open in the program are never removed.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Number of exams and their size on disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoreUsage {
    pub studies: usize,
    pub bytes: u64,
}

impl StoreUsage {
    /// "3 exames (1,2 GB)".
    pub fn describe(&self) -> String {
        let size = if self.bytes >= 1 << 30 {
            format!("{:.1} GB", self.bytes as f64 / (1u64 << 30) as f64)
        } else {
            format!("{:.0} MB", (self.bytes as f64 / (1u64 << 20) as f64).ceil())
        };
        let n = if self.studies == 1 { "1 exame".to_string() } else { format!("{} exames", self.studies) };
        format!("{n} ({})", size.replace('.', ","))
    }
}

fn study_dirs(root: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(root).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default()
}

fn files(dir: &Path) -> impl Iterator<Item = std::fs::Metadata> {
    walkdir::WalkDir::new(dir).into_iter().flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file())
}

/// Space taken by the downloaded exams.
pub fn usage(root: &Path) -> StoreUsage {
    let dirs = study_dirs(root);
    StoreUsage { studies: dirs.len(), bytes: dirs.iter().map(|d| files(d).map(|m| m.len()).sum::<u64>()).sum() }
}

/// Removes the downloaded exams last written more than `max_age` ago (all of
/// them when `None`), except the studies in `keep`. Returns what was removed.
pub fn clean(root: &Path, max_age: Option<Duration>, keep: &HashSet<String>) -> StoreUsage {
    let now = SystemTime::now();
    let mut removed = StoreUsage::default();
    for dir in study_dirs(root) {
        let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if keep.contains(&name) {
            continue;
        }
        let (bytes, newest) = files(&dir).fold((0u64, None::<SystemTime>), |(b, n), m| (b + m.len(), m.modified().ok().max(n)));
        if let Some(age) = max_age {
            // Kept until its newest file is old enough (a series added later counts as new).
            let young = newest.is_some_and(|t| now.duration_since(t).unwrap_or_default() < age);
            if young {
                continue;
            }
        }
        if std::fs::remove_dir_all(&dir).is_ok() {
            removed.studies += 1;
            removed.bytes += bytes;
        } else {
            log::warn!("não foi possível apagar {}", dir.display());
        }
    }
    if removed.studies > 0 {
        log::info!("exames baixados apagados: {}", removed.describe());
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_open_and_recent_exams() {
        let root = std::env::temp_dir().join(format!("slyced-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for study in ["1.2.3", "1.2.4", "1.2.5"] {
            let series = root.join(study).join("9.8");
            std::fs::create_dir_all(&series).unwrap();
            std::fs::write(series.join("1.dcm"), vec![0u8; 1000]).unwrap();
        }
        assert_eq!(usage(&root), StoreUsage { studies: 3, bytes: 3000 });
        // Everything was just written: nothing is old enough.
        assert_eq!(clean(&root, Some(Duration::from_secs(3600)), &HashSet::new()).studies, 0);
        // Remove all but the open study.
        let keep: HashSet<String> = ["1.2.4".to_string()].into();
        assert_eq!(clean(&root, None, &keep), StoreUsage { studies: 2, bytes: 2000 });
        assert!(root.join("1.2.4").exists() && !root.join("1.2.3").exists());
        assert_eq!(StoreUsage { studies: 2, bytes: 1_610_612_736 }.describe(), "2 exames (1,5 GB)");
        let _ = std::fs::remove_dir_all(&root);
    }
}
