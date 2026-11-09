//! Exercises the PACS client (C-ECHO, C-FIND, C-GET, C-MOVE + Storage SCP)
//! against a running server, e.g. the `test_pacs` example or a real PACS:
//!
//!     cargo run --release --example pacs_selftest -- <host> <port> <called-ae> <out-folder> [scp-port=11112]

use slyced::config::{PacsNode, RetrieveMethod};
use slyced::pacs::scp::{ScpEvent, StorageScp};
use slyced::pacs::scu::{self, StudyQuery};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let node = PacsNode {
        name: "teste".into(),
        host: a.first().cloned().unwrap_or_else(|| "127.0.0.1".into()),
        port: a.get(1).and_then(|p| p.parse().ok()).unwrap_or(4242),
        ae_title: a.get(2).cloned().unwrap_or_else(|| "TESTPACS".into()),
        retrieve: RetrieveMethod::CGet,
        ..Default::default()
    };
    let out = PathBuf::from(a.get(3).cloned().unwrap_or_else(|| "pacs-selftest".into()));
    let scp_port: u16 = a.get(4).and_then(|p| p.parse().ok()).unwrap_or(11112);
    let local = "SLYCED";

    let t = Instant::now();
    println!("C-ECHO: {:?}", scu::echo(&node, local)?);

    let studies = scu::find_studies(&node, local, &StudyQuery::default())?;
    println!("C-FIND estudos: {}", studies.len());
    for s in &studies {
        println!("  {} | {} | {} | {} | {:?} imagens", s.patient_name, s.patient_id, s.date, s.modalities, s.instance_count);
    }
    let study = studies.first().ok_or_else(|| anyhow::anyhow!("nenhum estudo"))?;
    let named = scu::find_studies(&node, local, &StudyQuery { patient_name: "fantoma".into(), ..Default::default() })?;
    println!("C-FIND por nome 'fantoma': {}", named.len());

    let series = scu::find_series(&node, local, &study.study_uid)?;
    println!("C-FIND séries: {}", series.len());
    for s in &series {
        println!("  #{:?} {} {} ({:?})", s.number, s.modality, s.description, s.instance_count);
    }

    let cancel = AtomicBool::new(false);
    let (files, p) = scu::retrieve_get(&node, local, &study.study_uid, None, &out.join("get"), &cancel, |_| {})?;
    println!("C-GET: {} arquivo(s) gravado(s), {} concluída(s), {} falha(s)", files.len(), p.completed, p.failed);
    let readable = files.iter().filter(|f| dicom_object::open_file(f).is_ok()).count();
    println!("C-GET: {readable}/{} arquivos relidos com sucesso", files.len());

    let received = Arc::new(AtomicUsize::new(0));
    let r2 = received.clone();
    let _scp = StorageScp::start(local, scp_port, out.join("move"), move |e| match e {
        ScpEvent::Stored(_) => {
            r2.fetch_add(1, Ordering::Relaxed);
        }
        ScpEvent::Error(e) => eprintln!("SCP: {e}"),
    })?;
    let s0 = &series[0];
    let p = scu::retrieve_move(&node, local, local, &study.study_uid, Some(&s0.series_uid), &cancel, |_| {})?;
    std::thread::sleep(Duration::from_millis(300));
    println!("C-MOVE série {}: {} concluída(s), {} recebida(s) pelo SCP", s0.series_uid, p.completed, received.load(Ordering::Relaxed));
    println!("tempo total: {:.2}s", t.elapsed().as_secs_f32());
    Ok(())
}
