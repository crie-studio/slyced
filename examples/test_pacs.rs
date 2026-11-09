//! Minimal Query/Retrieve SCP for testing Slyced's PACS client end to end.
//!
//!     cargo run --release --example test_pacs -- <dicom-folder> [port=4242] [ae=TESTPACS] [move-dest-port=11112]
//!
//! Supports C-ECHO, C-FIND (study/series), C-GET and C-MOVE (every move
//! destination is mapped to 127.0.0.1:<move-dest-port>).
//!
//! With `TESTPACS_ESTRITO=1` it behaves like a picky hospital PACS: a C-FIND
//! with an optional key, a leading `*` or an open date range gets 0xC000
//! ("unable to process") with an Error Comment.

use dicom_core::VR;
use dicom_dictionary_std::{tags, uids};
use dicom_object::InMemDicomObject;
use dicom_ul::association::client::ClientAssociationOptions;
use dicom_ul::association::server::ServerAssociationOptions;
use dicom_ul::association::Association;
use dicom_ul::pdu::PresentationContextResultReason;
use slyced::dicom::scan;
use slyced::dicom::tagutil::text;
use slyced::pacs::dimse::*;
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct Entry {
    path: PathBuf,
    sop_class: String,
    sop: String,
    attrs: HashMap<dicom_core::Tag, String>,
}

const STUDY_KEYS: &[(dicom_core::Tag, VR)] = &[
    (tags::PATIENT_NAME, VR::PN),
    (tags::PATIENT_ID, VR::LO),
    (tags::PATIENT_BIRTH_DATE, VR::DA),
    (tags::PATIENT_SEX, VR::CS),
    (tags::STUDY_DATE, VR::DA),
    (tags::STUDY_TIME, VR::TM),
    (tags::ACCESSION_NUMBER, VR::SH),
    (tags::STUDY_DESCRIPTION, VR::LO),
    (tags::REFERRING_PHYSICIAN_NAME, VR::PN),
    (tags::STUDY_INSTANCE_UID, VR::UI),
];
const SERIES_KEYS: &[(dicom_core::Tag, VR)] = &[
    (tags::STUDY_INSTANCE_UID, VR::UI),
    (tags::SERIES_INSTANCE_UID, VR::UI),
    (tags::SERIES_NUMBER, VR::IS),
    (tags::MODALITY, VR::CS),
    (tags::SERIES_DESCRIPTION, VR::LO),
];

fn main() {
    let mut args = std::env::args().skip(1);
    let folder = PathBuf::from(args.next().expect("uso: test_pacs <pasta> [porta] [ae] [porta-destino-move]"));
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(4242);
    let ae = args.next().unwrap_or_else(|| "TESTPACS".into());
    let dest_port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(11112);

    let progress = scan::ScanProgress::default();
    let files = scan::collect_files(&[folder], &progress);
    let mut entries = Vec::new();
    for p in files {
        let Some(obj) = scan::open_header(&p) else { continue };
        let mut attrs = HashMap::new();
        for (t, _) in STUDY_KEYS.iter().chain(SERIES_KEYS) {
            attrs.insert(*t, text(&obj, *t));
        }
        attrs.insert(tags::MODALITY, text(&obj, tags::MODALITY));
        entries.push(Entry { path: p, sop_class: text(&obj, tags::SOP_CLASS_UID), sop: text(&obj, tags::SOP_INSTANCE_UID), attrs });
    }
    println!("TESTPACS: {} instâncias indexadas; AE {ae} na porta {port}", entries.len());
    let entries = Arc::new(entries);
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    for stream in listener.incoming().flatten() {
        let (entries, ae) = (entries.clone(), ae.clone());
        std::thread::spawn(move || {
            if let Err(e) = serve(stream, &ae, &entries, dest_port) {
                eprintln!("TESTPACS: erro: {e:#}");
            }
        });
    }
}

fn glob(pattern: &str, value: &str) -> bool {
    fn m(p: &[char], v: &[char]) -> bool {
        match (p.first(), v.first()) {
            (None, None) => true,
            (Some('*'), _) => m(&p[1..], v) || (!v.is_empty() && m(p, &v[1..])),
            (Some('?'), Some(_)) => m(&p[1..], &v[1..]),
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b) && m(&p[1..], &v[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let v: Vec<char> = value.chars().collect();
    m(&p, &v)
}

fn matches(e: &Entry, ident: &InMemDicomObject) -> bool {
    for (tag, _) in STUDY_KEYS.iter().chain(SERIES_KEYS) {
        let q = text(ident, *tag);
        if q.is_empty() || q == "*" {
            continue;
        }
        let v = e.attrs.get(tag).cloned().unwrap_or_default();
        let ok = if *tag == tags::STUDY_DATE && q.contains('-') {
            let (a, b) = q.split_once('-').unwrap();
            (a.is_empty() || v.as_str() >= a) && (b.is_empty() || v.as_str() <= b)
        } else {
            glob(&q, &v)
        };
        if !ok {
            return false;
        }
    }
    let mods = text(ident, tags::MODALITIES_IN_STUDY);
    mods.is_empty() || e.attrs.get(&tags::MODALITY).is_some_and(|m| *m == mods)
}

/// Strict mode: why this identifier is refused, if it is.
fn strict_refusal(ident: &InMemDicomObject) -> Option<String> {
    const SUPPORTED: &[dicom_core::Tag] = &[
        tags::SPECIFIC_CHARACTER_SET,
        tags::QUERY_RETRIEVE_LEVEL,
        tags::PATIENT_NAME,
        tags::PATIENT_ID,
        tags::STUDY_DATE,
        tags::STUDY_TIME,
        tags::ACCESSION_NUMBER,
        tags::STUDY_ID,
        tags::STUDY_INSTANCE_UID,
        tags::SERIES_INSTANCE_UID,
        tags::SERIES_NUMBER,
        tags::MODALITY,
    ];
    for e in ident {
        if !SUPPORTED.contains(&e.header().tag) {
            return Some(format!("Unsupported query key {}", e.header().tag));
        }
    }
    let name = text(ident, tags::PATIENT_NAME);
    if name.starts_with('*') && name.len() > 1 {
        return Some("Leading wildcard not supported".into());
    }
    let date = text(ident, tags::STUDY_DATE);
    if date.starts_with('-') || date.ends_with('-') {
        return Some("Open date range not supported".into());
    }
    None
}

fn serve(stream: TcpStream, ae: &str, entries: &[Entry], dest_port: u16) -> anyhow::Result<()> {
    let mut opts = ServerAssociationOptions::new()
        .accept_any()
        .ae_title(ae.to_string())
        .promiscuous(true)
        .with_abstract_syntax(uids::VERIFICATION)
        .with_abstract_syntax(uids::STUDY_ROOT_QUERY_RETRIEVE_INFORMATION_MODEL_FIND)
        .with_abstract_syntax(uids::STUDY_ROOT_QUERY_RETRIEVE_INFORMATION_MODEL_GET)
        .with_abstract_syntax(uids::STUDY_ROOT_QUERY_RETRIEVE_INFORMATION_MODEL_MOVE);
    for sc in STORAGE_CLASSES {
        opts = opts.with_abstract_syntax(*sc);
    }
    let mut assoc = opts.establish(stream)?;
    let pcs: Vec<(u8, String, String)> = assoc
        .presentation_contexts()
        .iter()
        .filter(|p| p.reason == PresentationContextResultReason::Acceptance)
        .map(|p| (p.id, p.abstract_syntax.trim_end_matches('\0').to_string(), p.transfer_syntax.clone()))
        .collect();
    println!("TESTPACS: associação de {} ({} contextos aceitos)", assoc.peer_ae_title(), pcs.len());
    let ts_of = |id: u8| pcs.iter().find(|p| p.0 == id).map(|p| p.2.clone()).unwrap_or_default();
    let mut reader = MessageReader::default();
    loop {
        let m = match reader.read(&mut assoc)? {
            Incoming::Released => return Ok(()),
            Incoming::Message(m) => m,
        };
        let sop_class = m.str(tags::AFFECTED_SOP_CLASS_UID);
        match m.field() {
            C_ECHO_RQ => send_message(&mut assoc, m.pc_id, response(C_ECHO_RSP, &sop_class, m.message_id(), 0, None), None)?,
            C_FIND_RQ => {
                let ident = decode_dataset(m.data.as_deref().unwrap_or(&[]), &ts_of(m.pc_id))?;
                let level = text(&ident, tags::QUERY_RETRIEVE_LEVEL);
                if std::env::var_os("TESTPACS_ESTRITO").is_some()
                    && let Some(why) = strict_refusal(&ident) {
                        let mut cmd = response(C_FIND_RSP, &sop_class, m.message_id(), 0xC000, None);
                        cmd.put(element_str(tags::ERROR_COMMENT, VR::LO, &why));
                        send_message(&mut assoc, m.pc_id, cmd, None)?;
                        println!("TESTPACS: C-FIND {level} recusado (0xC000): {why}");
                        continue;
                    }
                let keys = if level == "SERIES" { SERIES_KEYS } else { STUDY_KEYS };
                let key_tag = if level == "SERIES" { tags::SERIES_INSTANCE_UID } else { tags::STUDY_INSTANCE_UID };
                let mut seen = Vec::new();
                for e in entries.iter().filter(|e| matches(e, &ident)) {
                    let k = e.attrs[&key_tag].clone();
                    if seen.contains(&k) {
                        continue;
                    }
                    seen.push(k.clone());
                    let mut r = InMemDicomObject::new_empty();
                    r.put(element_str(tags::QUERY_RETRIEVE_LEVEL, VR::CS, &level));
                    for (t, vr) in keys {
                        r.put(element_str(*t, *vr, e.attrs.get(t).map(|s| s.as_str()).unwrap_or("")));
                    }
                    let count = entries.iter().filter(|x| x.attrs[&key_tag] == k).count();
                    if level == "SERIES" {
                        r.put(element_str(tags::NUMBER_OF_SERIES_RELATED_INSTANCES, VR::IS, &count.to_string()));
                    } else {
                        r.put(element_str(tags::NUMBER_OF_STUDY_RELATED_INSTANCES, VR::IS, &count.to_string()));
                        r.put(element_str(tags::MODALITIES_IN_STUDY, VR::CS, &e.attrs[&tags::MODALITY]));
                    }
                    let mut cmd = response(C_FIND_RSP, &sop_class, m.message_id(), 0xFF00, None);
                    cmd.put(element_u16(tags::COMMAND_DATA_SET_TYPE, DATASET_PRESENT));
                    send_message(&mut assoc, m.pc_id, cmd, Some(&encode_dataset(&r, &ts_of(m.pc_id))?))?;
                }
                send_message(&mut assoc, m.pc_id, response(C_FIND_RSP, &sop_class, m.message_id(), 0, None), None)?;
                println!("TESTPACS: C-FIND {level}: {} resultado(s)", seen.len());
            }
            C_GET_RQ | C_MOVE_RQ => {
                let ident = decode_dataset(m.data.as_deref().unwrap_or(&[]), &ts_of(m.pc_id))?;
                let todo: Vec<&Entry> = entries.iter().filter(|e| matches(e, &ident)).collect();
                let is_get = m.field() == C_GET_RQ;
                let rsp_field = if is_get { C_GET_RSP } else { C_MOVE_RSP };
                let total = todo.len() as u16;
                let (mut done, mut failed) = (0u16, 0u16);

                let mut dest = if is_get {
                    None
                } else {
                    let mut o = ClientAssociationOptions::new().calling_ae_title(ae.to_string()).called_ae_title(m.str(tags::MOVE_DESTINATION));
                    for sc in todo.iter().map(|e| e.sop_class.clone()).collect::<std::collections::BTreeSet<_>>() {
                        o = o.with_presentation_context(sc, vec![uids::EXPLICIT_VR_LITTLE_ENDIAN.to_string(), uids::IMPLICIT_VR_LITTLE_ENDIAN.to_string()]);
                    }
                    Some(o.establish(("127.0.0.1", dest_port))?)
                };
                for (i, e) in todo.iter().enumerate() {
                    let obj = dicom_object::open_file(&e.path)?;
                    let ok = {
                        let cmd = {
                            let mut c = InMemDicomObject::new_empty();
                            c.put(element_str(tags::AFFECTED_SOP_CLASS_UID, VR::UI, &e.sop_class));
                            c.put(element_u16(tags::COMMAND_FIELD, C_STORE_RQ));
                            c.put(element_u16(tags::MESSAGE_ID, i as u16 + 100));
                            c.put(element_u16(tags::PRIORITY, 0));
                            c.put(element_u16(tags::COMMAND_DATA_SET_TYPE, DATASET_PRESENT));
                            c.put(element_str(tags::AFFECTED_SOP_INSTANCE_UID, VR::UI, &e.sop));
                            c
                        };
                        let mut r2 = MessageReader::default();
                        match &mut dest {
                            None => {
                                let (pc, ts) = pcs.iter().find(|p| p.1 == e.sop_class).map(|p| (p.0, p.2.clone())).unwrap_or((0, String::new()));
                                if pc == 0 {
                                    false
                                } else {
                                    let data = encode_dataset(&obj, &ts)?;
                                    send_message(&mut assoc, pc, cmd, Some(&data))?;
                                    matches!(r2.read(&mut assoc)?, Incoming::Message(r) if r.field() == C_STORE_RSP && r.status() == 0)
                                }
                            }
                            Some(d) => {
                                let (pc, ts) = d
                                    .presentation_contexts()
                                    .iter()
                                    .find(|p| p.reason == PresentationContextResultReason::Acceptance && p.abstract_syntax.trim_end_matches('\0') == e.sop_class)
                                    .map(|p| (p.id, p.transfer_syntax.clone()))
                                    .unwrap_or((0, String::new()));
                                let data = encode_dataset(&obj, &ts)?;
                                send_message(d, pc, cmd, Some(&data))?;
                                matches!(r2.read(d)?, Incoming::Message(r) if r.field() == C_STORE_RSP && r.status() == 0)
                            }
                        }
                    };
                    if ok { done += 1 } else { failed += 1 }
                    let mut p = response(rsp_field, &sop_class, m.message_id(), 0xFF00, None);
                    p.put(element_u16(tags::NUMBER_OF_REMAINING_SUBOPERATIONS, total - done - failed));
                    p.put(element_u16(tags::NUMBER_OF_COMPLETED_SUBOPERATIONS, done));
                    p.put(element_u16(tags::NUMBER_OF_FAILED_SUBOPERATIONS, failed));
                    p.put(element_u16(tags::NUMBER_OF_WARNING_SUBOPERATIONS, 0));
                    send_message(&mut assoc, m.pc_id, p, None)?;
                }
                if let Some(d) = dest {
                    let _ = d.release();
                }
                let mut fin = response(rsp_field, &sop_class, m.message_id(), if failed > 0 { 0xB000 } else { 0 }, None);
                fin.put(element_u16(tags::NUMBER_OF_COMPLETED_SUBOPERATIONS, done));
                fin.put(element_u16(tags::NUMBER_OF_FAILED_SUBOPERATIONS, failed));
                send_message(&mut assoc, m.pc_id, fin, None)?;
                println!("TESTPACS: {} {done}/{total} enviadas", if is_get { "C-GET" } else { "C-MOVE" });
            }
            other => eprintln!("TESTPACS: comando 0x{other:04X} ignorado"),
        }
    }
}
