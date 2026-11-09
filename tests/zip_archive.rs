//! Opens a CT series straight from a ZIP archive (no extraction) and checks
//! that it matches what is read from plain files.

mod common;

use common::write_slice;
use slyced::dicom::decode::decode_file;
use slyced::dicom::model::Library;
use slyced::dicom::scan::{collect_files, scan_files, ScanProgress};
use slyced::dicom::series::{LoadedSeries, Slice};
use slyced::dicom::{source, tags};
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

#[test]
fn series_opens_directly_from_zip() {
    let root = std::env::temp_dir().join(format!("slyced-zip-test-{}", std::process::id()));
    let files = root.join("files");
    std::fs::create_dir_all(&files).unwrap();
    for k in 0..5 {
        write_slice(&files, k, 10.0 - 2.5 * k as f64);
    }

    // Mixed archive: deflated and stored entries in sub-folders, plus junk.
    let zip_path = root.join("Exame Tomografia.zip");
    let mut zw = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
    for k in 0..5 {
        let method = if k % 2 == 0 { CompressionMethod::Deflated } else { CompressionMethod::Stored };
        zw.start_file(format!("EXAME/DICOM/S0001/IM{k}"), SimpleFileOptions::default().compression_method(method)).unwrap();
        zw.write_all(&std::fs::read(files.join(format!("IM{k}"))).unwrap()).unwrap();
    }
    zw.start_file("EXAME/LEIAME.txt", SimpleFileOptions::default()).unwrap();
    zw.write_all(b"Visualize com qualquer visualizador DICOM").unwrap();
    zw.start_file("EXAME/DICOMDIR", SimpleFileOptions::default()).unwrap();
    zw.write_all(b"ignorado").unwrap();
    zw.start_file("__MACOSX/EXAME/._IM0", SimpleFileOptions::default()).unwrap();
    zw.write_all(b"resource fork").unwrap();
    zw.start_file("EXAME/sem-extensao-nao-dicom", SimpleFileOptions::default()).unwrap();
    zw.write_all(b"texto qualquer sem cabecalho DICOM").unwrap();
    zw.finish().unwrap();

    // Opening the .zip itself and a folder that contains it give the same entries.
    let progress = ScanProgress::default();
    let entries = collect_files(&[zip_path.clone()], &progress);
    assert_eq!(entries.len(), 6, "{entries:?}");
    assert!(entries.iter().all(|p| source::split_entry(p).is_some()));
    let zip_dir = root.join("pasta");
    std::fs::create_dir_all(&zip_dir).unwrap();
    std::fs::copy(&zip_path, zip_dir.join("copia.zip")).unwrap();
    assert_eq!(collect_files(&[zip_dir.clone()], &progress).len(), 6);

    let scanned = scan_files(&entries, &progress);
    assert_eq!(scanned.len(), 5, "only the five DICOM entries are images");
    let mut lib = Library::default();
    lib.merge(scanned);
    let (study, series) = lib.find_series("2.25.2").unwrap();
    let frames = series.instances.iter().flat_map(|i| decode_file(&i.path).unwrap()).collect();
    let loaded = LoadedSeries::build(series.clone(), study.header.clone(), frames);
    let vol = loaded.volume.as_ref().expect("volume");
    assert_eq!(vol.dims, [4, 3, 5]);
    assert!((vol.spacing.z - 2.5).abs() < 1e-9);
    assert_eq!(loaded.images[0].instance_number, 1);
    match loaded.slice(4) {
        Some(Slice::Gray(d, q)) => assert_eq!(q.real(d[0]), 400.0),
        _ => panic!("expected a gray slice"),
    }
    let rows = tags::dump(&loaded.images[0].path).unwrap();
    assert!(rows.iter().any(|r| r.name == "PatientName" && r.value.contains("TESTE")));

    // No file handle is kept open: the archive can be deleted right away.
    std::fs::remove_file(&zip_path).expect("ZIP should not stay locked");
    let _ = std::fs::remove_dir_all(&root);
}
