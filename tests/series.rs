//! Writes a tiny CT series to disk and checks that scanning, decoding and
//! volume assembly reproduce its geometry and Hounsfield values.

mod common;

use common::write_slice;
use slyced::dicom::decode::decode_file;
use slyced::dicom::model::Library;
use slyced::dicom::scan::{collect_files, scan_files, ScanProgress};
use slyced::dicom::series::{LoadedSeries, Slice};

#[test]
fn scan_decode_and_assemble_volume() {
    let dir = std::env::temp_dir().join(format!("slyced-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Instance 1 is the most superior slice; 2.5 mm spacing.
    for k in 0..5 {
        write_slice(&dir, k, 10.0 - 2.5 * k as f64);
    }
    std::fs::write(dir.join("notes.txt"), "not dicom").unwrap();
    std::fs::write(dir.join("README"), "plain text without extension").unwrap();

    let progress = ScanProgress::default();
    let files = collect_files(&[dir.clone()], &progress);
    let scanned = scan_files(&files, &progress);
    assert_eq!(scanned.len(), 5);
    let mut lib = Library::default();
    lib.merge(scanned);
    let (study, series) = lib.find_series("2.25.2").unwrap();
    assert_eq!(series.image_count(), 5);

    let frames = series.instances.iter().flat_map(|i| decode_file(&i.path).unwrap()).collect();
    let loaded = LoadedSeries::build(series.clone(), study.header.clone(), frames);
    let vol = loaded.volume.as_ref().expect("volume");
    assert_eq!(vol.dims, [4, 3, 5]);
    // PixelSpacing is (row spacing, column spacing) = (dy, dx).
    assert!((vol.spacing.x - 0.7).abs() < 1e-9 && (vol.spacing.y - 0.5).abs() < 1e-9);
    assert!((vol.spacing.z - 2.5).abs() < 1e-9);
    // Acquisition order is kept: the first slice is instance 1 and the stack runs towards the feet.
    assert_eq!(loaded.images[0].instance_number, 1);
    assert!(vol.axes[2].z < -0.99);
    match loaded.slice(2) {
        Some(Slice::Gray(d, q)) => assert_eq!(q.real(d[0]), 200.0),
        _ => panic!("expected a gray slice"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
