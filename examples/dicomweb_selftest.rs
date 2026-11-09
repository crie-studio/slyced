//! Exercises the DICOMweb client (QIDO-RS + WADO-RS) against a running server,
//! e.g. Orthanc with the DICOMweb plugin:
//!
//!     cargo run --release --example dicomweb_selftest -- <base-url> <out-folder> [study|series] [patient|-] [user:password | token]
//!
//! Downloads the smallest series of the first study found (or the whole study
//! with `study`; `patient` picks the study by patient name) and re-opens every
//! file the way the viewer does.

use slyced::config::{PacsNode, PacsProtocol, WebAuth};
use slyced::dicom::scan::{self, ScanProgress};
use slyced::pacs::dicomweb;
use slyced::pacs::scu::StudyQuery;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut node = PacsNode {
        name: "teste".into(),
        protocol: PacsProtocol::DicomWeb,
        base_url: a.first().cloned().unwrap_or_else(|| "http://127.0.0.1:8042/dicom-web".into()),
        ..Default::default()
    };
    let out = PathBuf::from(a.get(1).cloned().unwrap_or_else(|| "dicomweb-selftest".into()));
    let whole_study = a.get(2).is_some_and(|m| m == "study");
    let patient = a.get(3).filter(|p| *p != "-").cloned();
    match a.get(4) {
        Some(c) if c.contains(':') => {
            let (u, p) = c.split_once(':').unwrap();
            (node.auth, node.username, node.secret) = (WebAuth::Basic, u.into(), p.into());
        }
        Some(t) => (node.auth, node.secret) = (WebAuth::Bearer, t.clone()),
        None => {}
    }

    let t = Instant::now();
    println!("teste de conexão: {:?}", dicomweb::echo(&node)?);

    let studies = dicomweb::find_studies(&node, &StudyQuery::default())?;
    println!("QIDO estudos: {}", studies.len());
    for s in studies.iter().take(10) {
        println!(
            "  {} | {} | {} {} | {} | {} | {:?} séries, {:?} imagens",
            s.patient_name, s.patient_id, s.date, s.time, s.modalities, s.description, s.series_count, s.instance_count
        );
    }
    let study = studies
        .iter()
        .find(|s| patient.as_ref().is_none_or(|p| s.patient_name.eq_ignore_ascii_case(p)))
        .ok_or_else(|| anyhow::anyhow!("nenhum estudo"))?;
    let word = study.patient_name.split(['^', ' ']).find(|w| w.len() >= 3).unwrap_or_default().to_string();
    let named = dicomweb::find_studies(&node, &StudyQuery { patient_name: word.to_lowercase(), ..Default::default() })?;
    println!("QIDO por nome '{}': {} (contém o primeiro: {})", word.to_lowercase(), named.len(), named.iter().any(|s| s.study_uid == study.study_uid));
    let modality = study.modalities.split_whitespace().next().unwrap_or_default().to_string();
    let by_mod = dicomweb::find_studies(&node, &StudyQuery { modality: modality.clone(), ..Default::default() })?;
    println!("QIDO por modalidade {modality}: {}", by_mod.len());
    let by_date = dicomweb::find_studies(&node, &StudyQuery { date_from: study.date.clone(), date_to: study.date.clone(), ..Default::default() })?;
    println!("QIDO por data {}: {}", study.date, by_date.len());

    let series = dicomweb::find_series(&node, &study.study_uid)?;
    println!("QIDO séries: {}", series.len());
    for s in &series {
        println!("  #{:?} {} {} ({:?})", s.number, s.modality, s.description, s.instance_count);
    }
    let smallest = series.iter().min_by_key(|s| s.instance_count.unwrap_or(i32::MAX)).ok_or_else(|| anyhow::anyhow!("nenhuma série"))?;

    let cancel = AtomicBool::new(false);
    let target = if whole_study { None } else { Some(smallest.series_uid.as_str()) };
    let mut updates = 0;
    let t_get = Instant::now();
    let (files, p) = dicomweb::retrieve(&node, &study.study_uid, target, &out, &cancel, |p| {
        updates += 1;
        if updates <= 3 {
            println!("  progresso: {} concluída(s), {} restante(s), {} falha(s)", p.completed, p.remaining, p.failed);
        }
    })?;
    let bytes: u64 = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
    println!(
        "WADO-RS {}: {} arquivo(s), {:.1} MB em {:.2}s, {} falha(s), {} atualizações de progresso",
        if whole_study { "estudo" } else { "série" },
        files.len(),
        bytes as f64 / 1e6,
        t_get.elapsed().as_secs_f32(),
        p.failed,
        updates
    );
    let leftovers = std::fs::read_dir(&out)?.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".part")).count();
    println!("arquivos temporários restantes: {leftovers}");

    // Same path as a finished C-GET: scan the files, then group them into series.
    let progress = ScanProgress::default();
    let scanned = scan::scan_files(&files, &progress);
    println!("varredura: {}/{} arquivos reconhecidos", scanned.len(), files.len());
    let mut decoded = 0;
    for f in &files {
        if slyced::dicom::decode::decode_file(f).is_ok() {
            decoded += 1;
        }
    }
    println!("decodificação de pixels: {decoded}/{}", files.len());
    println!("tempo total: {:.2}s", t.elapsed().as_secs_f32());
    Ok(())
}
