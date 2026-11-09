//! Service class user operations: C-ECHO, C-FIND, C-GET and C-MOVE.

use super::dimse::*;
use crate::config::PacsNode;
use crate::dicom::tagutil::{int, text};
use anyhow::{anyhow, bail, Result};
use dicom_core::VR;
use dicom_dictionary_std::{tags, uids};
use dicom_object::InMemDicomObject;
use dicom_ul::association::client::{ClientAssociation, ClientAssociationOptions};
use dicom_ul::pdu::PresentationContextResultReason;
use std::collections::HashMap;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const MAX_PDU: u32 = 116_794;

fn connect(node: &PacsNode, local_ae: &str, opts: ClientAssociationOptions<'_>) -> Result<ClientAssociation<TcpStream>> {
    opts.calling_ae_title(local_ae.to_string())
        .called_ae_title(node.ae_title.trim().to_string())
        .max_pdu_length(MAX_PDU)
        .strict(false)
        .connection_timeout(Duration::from_secs(8))
        .read_timeout(Duration::from_secs(180))
        .write_timeout(Duration::from_secs(60))
        .establish((node.host.trim(), node.port))
        .map_err(|e| {
            let msg = e.to_string();
            if msg.to_lowercase().contains("reject") {
                anyhow!("O PACS rejeitou a associação. Confira o AE Title remoto ({}) e se o AE local ({local_ae}) está cadastrado no servidor.", node.ae_title)
            } else {
                anyhow!("Não foi possível conectar em {}:{} — {msg}", node.host, node.port)
            }
        })
}

fn accepted_pc(assoc: &ClientAssociation<TcpStream>, abstract_syntax: &str) -> Result<(u8, String)> {
    assoc
        .presentation_contexts()
        .iter()
        .find(|pc| pc.reason == PresentationContextResultReason::Acceptance && pc.abstract_syntax.trim_end_matches('\0') == abstract_syntax)
        .map(|pc| (pc.id, pc.transfer_syntax.clone()))
        .ok_or_else(|| anyhow!("O PACS não aceitou o serviço solicitado ({abstract_syntax})."))
}

fn request(field: u16, sop_class: &str, message_id: u16) -> InMemDicomObject {
    let mut cmd = InMemDicomObject::new_empty();
    cmd.put(element_str(tags::AFFECTED_SOP_CLASS_UID, VR::UI, sop_class));
    cmd.put(element_u16(tags::COMMAND_FIELD, field));
    cmd.put(element_u16(tags::MESSAGE_ID, message_id));
    cmd.put(element_u16(tags::PRIORITY, 0));
    cmd.put(element_u16(tags::COMMAND_DATA_SET_TYPE, if field == C_ECHO_RQ { NO_DATASET } else { DATASET_PRESENT }));
    cmd
}

pub fn echo(node: &PacsNode, local_ae: &str) -> Result<Duration> {
    let start = Instant::now();
    let mut assoc = connect(node, local_ae, ClientAssociationOptions::new().with_abstract_syntax(uids::VERIFICATION))?;
    let (pc, _) = accepted_pc(&assoc, uids::VERIFICATION)?;
    let mut cmd = request(C_ECHO_RQ, uids::VERIFICATION, 1);
    let _ = cmd.take_element(tags::PRIORITY);
    send_message(&mut assoc, pc, cmd, None)?;
    let mut reader = MessageReader::default();
    let result = match reader.read(&mut assoc)? {
        Incoming::Message(m) if m.field() == C_ECHO_RSP && m.status() == STATUS_SUCCESS => Ok(start.elapsed()),
        Incoming::Message(m) => Err(anyhow!("C-ECHO falhou: {}", status_text(m.status()))),
        Incoming::Released => Err(anyhow!("o PACS encerrou a associação")),
    };
    let _ = assoc.release();
    result
}

#[derive(Clone, Debug, Default)]
pub struct StudyQuery {
    pub patient_name: String,
    pub patient_id: String,
    pub accession: String,
    /// `AAAAMMDD` or empty.
    pub date_from: String,
    pub date_to: String,
    pub modality: String,
    pub description: String,
}

impl StudyQuery {
    /// DICOM range matching: `A-B`, `A-`, `-B` or a single date.
    pub(crate) fn date_range(&self) -> String {
        match (self.date_from.trim(), self.date_to.trim()) {
            ("", "") => String::new(),
            (a, "") => format!("{a}-"),
            ("", b) => format!("-{b}"),
            (a, b) if a == b => a.to_string(),
            (a, b) => format!("{a}-{b}"),
        }
    }
}

/// `*TEXT*` substring match, or empty (match everything).
pub(crate) fn contains_pattern(s: &str) -> String {
    let p = ascii_pattern(s);
    if p.is_empty() { p } else { format!("*{}*", p.trim_matches('*')) }
}

#[derive(Clone, Debug, Default)]
pub struct StudyResult {
    pub study_uid: String,
    pub patient_name: String,
    pub patient_id: String,
    pub birth_date: String,
    pub sex: String,
    pub date: String,
    pub time: String,
    pub description: String,
    pub accession: String,
    pub modalities: String,
    pub referring: String,
    pub series_count: Option<i32>,
    pub instance_count: Option<i32>,
}

#[derive(Clone, Debug, Default)]
pub struct SeriesResult {
    pub series_uid: String,
    pub number: Option<i32>,
    pub modality: String,
    pub description: String,
    pub instance_count: Option<i32>,
}

/// DICOM query strings are ASCII here; accented letters become the `?` wildcard.
pub(crate) fn ascii_pattern(s: &str) -> String {
    s.trim().chars().map(|c| if c.is_ascii() { c.to_ascii_uppercase() } else { '?' }).collect()
}

/// Like [`ascii_pattern`] but keeping the case: IDs and accession numbers may
/// have lowercase letters, and their matching is case sensitive.
pub(crate) fn ascii_text(s: &str) -> String {
    s.trim().chars().map(|c| if c.is_ascii() { c } else { '?' }).collect()
}

/// `TEXT*` prefix match (for servers that do not accept a leading `*`).
fn prefix_pattern(s: &str) -> String {
    let p = ascii_pattern(s);
    let p = p.trim_matches('*');
    if p.is_empty() { String::new() } else { format!("{p}*") }
}

/// How much a C-FIND asks of the PACS. Some servers answer "unable to process"
/// (0xC000) to optional keys, open date ranges or leading wildcards; the query
/// is then repeated with less, and the form that worked is remembered per server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum QueryStyle {
    /// Every key the list shows.
    Full,
    /// Without the optional counts and referring physician; closed date ranges.
    Plain,
    /// Only the keys every Query/Retrieve SCP must support, prefix wildcards only.
    Minimal,
}

impl QueryStyle {
    const ALL: [QueryStyle; 3] = [QueryStyle::Full, QueryStyle::Plain, QueryStyle::Minimal];
}

/// The form of query that last worked for each server (`AE@host:port`).
static QUERY_STYLE: std::sync::Mutex<Option<HashMap<String, QueryStyle>>> = std::sync::Mutex::new(None);

fn node_key(node: &PacsNode) -> String {
    format!("{}@{}:{}", node.ae_title.trim(), node.host.trim(), node.port)
}

/// The PACS answered the query with a failure status (as opposed to a network error).
#[derive(Debug)]
struct FindRefused {
    status: u16,
    /// The server's own explanation (Error Comment), when it sends one.
    comment: String,
}

impl std::fmt::Display for FindRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "C-FIND falhou: {}", status_text(self.status))?;
        if !self.comment.is_empty() {
            write!(f, " — o PACS informou: \"{}\"", self.comment)?;
        }
        Ok(())
    }
}

impl std::error::Error for FindRefused {}

fn find(node: &PacsNode, local_ae: &str, ident: InMemDicomObject) -> Result<Vec<InMemDicomObject>> {
    let sop = uids::STUDY_ROOT_QUERY_RETRIEVE_INFORMATION_MODEL_FIND;
    let mut assoc = connect(
        node,
        local_ae,
        ClientAssociationOptions::new()
            .with_presentation_context(sop, vec![uids::EXPLICIT_VR_LITTLE_ENDIAN, uids::IMPLICIT_VR_LITTLE_ENDIAN]),
    )?;
    let (pc, ts) = accepted_pc(&assoc, sop)?;
    let data = encode_dataset(&ident, &ts)?;
    send_message(&mut assoc, pc, request(C_FIND_RQ, sop, 1), Some(&data))?;
    let mut reader = MessageReader::default();
    let mut out = Vec::new();
    loop {
        match reader.read(&mut assoc)? {
            Incoming::Message(m) => {
                let st = m.status();
                if is_pending(st) {
                    if let Some(d) = &m.data {
                        out.push(decode_dataset(d, &ts)?);
                    }
                } else if st == STATUS_SUCCESS {
                    break;
                } else {
                    let _ = assoc.abort();
                    let mut comment = m.str(tags::ERROR_COMMENT).trim().to_string();
                    if let Some(e) = m.command.element_opt(tags::OFFENDING_ELEMENT).ok().flatten()
                        && let dicom_core::value::Value::Primitive(dicom_core::value::PrimitiveValue::Tags(t)) = e.value()
                        && let Some(t) = t.first() {
                            comment = format!("{comment} (campo {t})").trim().to_string();
                        }
                    return Err(FindRefused { status: st, comment }.into());
                }
            }
            Incoming::Released => return Ok(out),
        }
    }
    let _ = assoc.release();
    Ok(out)
}

/// Runs `build(style)` from the form remembered for this server; when the PACS
/// refuses it (not a network failure), tries the simpler forms.
fn find_adaptive(node: &PacsNode, local_ae: &str, what: &str, build: impl Fn(QueryStyle) -> InMemDicomObject) -> Result<(Vec<InMemDicomObject>, QueryStyle)> {
    let key = node_key(node);
    let start = QUERY_STYLE.lock().ok().and_then(|m| m.as_ref().and_then(|m| m.get(&key).copied())).unwrap_or(QueryStyle::Full);
    let mut last: Option<anyhow::Error> = None;
    for style in QueryStyle::ALL.into_iter().filter(|s| *s >= start) {
        let ident = build(style);
        match find(node, local_ae, ident) {
            Ok(out) => {
                if style != start || last.is_some() {
                    log::info!("C-FIND {what} em {key}: aceito no formato {style:?}");
                }
                if let Ok(mut m) = QUERY_STYLE.lock() {
                    m.get_or_insert_with(HashMap::new).insert(key, style);
                }
                return Ok((out, style));
            }
            Err(e) if e.downcast_ref::<FindRefused>().is_some() => {
                log::warn!("C-FIND {what} em {key} recusado no formato {style:?}: {e}");
                last = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    let e = last.unwrap_or_else(|| anyhow!("C-FIND falhou"));
    Err(anyhow!("{e}. O Slyced também tentou buscas simplificadas, sem sucesso. Tente filtrar por nome, ID ou data; se continuar, envie o arquivo slyced.log ao suporte do PACS."))
}

/// `A-B` with both ends: `A-` becomes `A-today` and `-B` becomes `19000101-B`.
fn closed_range(range: &str) -> String {
    match range.split_once('-') {
        Some((a, "")) => format!("{a}-{}", today_yyyymmdd()),
        Some(("", b)) => format!("19000101-{b}"),
        _ => range.to_string(),
    }
}

fn today_yyyymmdd() -> String {
    let (y, m, d) = crate::platform::local_date();
    format!("{y:04}{m:02}{d:02}")
}

fn study_identifier(q: &StudyQuery, style: QueryStyle) -> InMemDicomObject {
    let mut id = InMemDicomObject::new_empty();
    let mut put = |tag, vr, v: &str| {
        id.put(element_str(tag, vr, v));
    };
    let (name, description) = match style {
        QueryStyle::Minimal => (prefix_pattern(&q.patient_name), prefix_pattern(&q.description)),
        _ => (contains_pattern(&q.patient_name), contains_pattern(&q.description)),
    };
    let date = if style == QueryStyle::Full { q.date_range() } else { closed_range(&q.date_range()) };
    let modality = q.modality.trim().to_ascii_uppercase();
    put(tags::QUERY_RETRIEVE_LEVEL, VR::CS, "STUDY");
    // Keys every Study Root C-FIND SCP must support (required or unique).
    put(tags::PATIENT_NAME, VR::PN, &name);
    put(tags::PATIENT_ID, VR::LO, &ascii_text(&q.patient_id));
    put(tags::STUDY_DATE, VR::DA, &date);
    put(tags::STUDY_TIME, VR::TM, "");
    put(tags::ACCESSION_NUMBER, VR::SH, &ascii_text(&q.accession));
    put(tags::STUDY_ID, VR::SH, "");
    put(tags::STUDY_INSTANCE_UID, VR::UI, "");
    if style == QueryStyle::Minimal {
        return id;
    }
    put(tags::PATIENT_BIRTH_DATE, VR::DA, "");
    put(tags::PATIENT_SEX, VR::CS, "");
    put(tags::STUDY_DESCRIPTION, VR::LO, &description);
    if style == QueryStyle::Full || !modality.is_empty() {
        put(tags::MODALITIES_IN_STUDY, VR::CS, &modality);
    }
    if style == QueryStyle::Full {
        put(tags::REFERRING_PHYSICIAN_NAME, VR::PN, "");
        put(tags::NUMBER_OF_STUDY_RELATED_SERIES, VR::IS, "");
        put(tags::NUMBER_OF_STUDY_RELATED_INSTANCES, VR::IS, "");
    }
    id
}

pub fn find_studies(node: &PacsNode, local_ae: &str, q: &StudyQuery) -> Result<Vec<StudyResult>> {
    let (found, style) = find_adaptive(node, local_ae, "de estudos", |style| study_identifier(q, style))?;
    // The minimal form cannot filter by description or modality: done here when the values came back.
    let description = ascii_pattern(&q.description).trim_matches('*').to_string();
    let modality = q.modality.trim().to_ascii_uppercase();
    let mut results: Vec<StudyResult> = found
        .iter()
        .map(|o| StudyResult {
            study_uid: text(o, tags::STUDY_INSTANCE_UID),
            patient_name: text(o, tags::PATIENT_NAME),
            patient_id: text(o, tags::PATIENT_ID),
            birth_date: text(o, tags::PATIENT_BIRTH_DATE),
            sex: text(o, tags::PATIENT_SEX),
            date: text(o, tags::STUDY_DATE),
            time: text(o, tags::STUDY_TIME),
            description: text(o, tags::STUDY_DESCRIPTION),
            accession: text(o, tags::ACCESSION_NUMBER),
            modalities: text(o, tags::MODALITIES_IN_STUDY).replace('\\', " "),
            referring: text(o, tags::REFERRING_PHYSICIAN_NAME),
            series_count: int(o, tags::NUMBER_OF_STUDY_RELATED_SERIES),
            instance_count: int(o, tags::NUMBER_OF_STUDY_RELATED_INSTANCES),
        })
        .filter(|r| !r.study_uid.is_empty())
        .filter(|r| style != QueryStyle::Minimal || description.is_empty() || r.description.is_empty() || ascii_pattern(&r.description).contains(&description))
        .filter(|r| style != QueryStyle::Minimal || modality.is_empty() || r.modalities.is_empty() || r.modalities.split_whitespace().any(|m| m == modality))
        .collect();
    results.sort_by(|a, b| (&b.date, &b.time).cmp(&(&a.date, &a.time)));
    Ok(results)
}

pub fn find_series(node: &PacsNode, local_ae: &str, study_uid: &str) -> Result<Vec<SeriesResult>> {
    let build = |style: QueryStyle| {
        let mut id = InMemDicomObject::new_empty();
        id.put(element_str(tags::QUERY_RETRIEVE_LEVEL, VR::CS, "SERIES"));
        id.put(element_str(tags::STUDY_INSTANCE_UID, VR::UI, study_uid));
        id.put(element_str(tags::SERIES_INSTANCE_UID, VR::UI, ""));
        id.put(element_str(tags::SERIES_NUMBER, VR::IS, ""));
        id.put(element_str(tags::MODALITY, VR::CS, ""));
        if style != QueryStyle::Minimal {
            id.put(element_str(tags::SERIES_DESCRIPTION, VR::LO, ""));
        }
        if style == QueryStyle::Full {
            id.put(element_str(tags::NUMBER_OF_SERIES_RELATED_INSTANCES, VR::IS, ""));
        }
        id
    };
    let (found, _) = find_adaptive(node, local_ae, "de séries", build)?;
    let mut results: Vec<SeriesResult> = found
        .iter()
        .map(|o| SeriesResult {
            series_uid: text(o, tags::SERIES_INSTANCE_UID),
            number: int(o, tags::SERIES_NUMBER),
            modality: text(o, tags::MODALITY),
            description: text(o, tags::SERIES_DESCRIPTION),
            instance_count: int(o, tags::NUMBER_OF_SERIES_RELATED_INSTANCES),
        })
        .filter(|r| !r.series_uid.is_empty())
        .collect();
    results.sort_by_key(|r| r.number.unwrap_or(i32::MAX));
    Ok(results)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RetrieveProgress {
    pub completed: u32,
    pub remaining: u32,
    pub failed: u32,
    pub received: u32,
}

impl RetrieveProgress {
    fn update_from(&mut self, m: &Message) {
        let get = |t| cmd_u16(&m.command, t).map(u32::from);
        if let Some(v) = get(tags::NUMBER_OF_COMPLETED_SUBOPERATIONS) {
            self.completed = v;
        }
        if let Some(v) = get(tags::NUMBER_OF_REMAINING_SUBOPERATIONS) {
            self.remaining = v;
        }
        if let Some(v) = get(tags::NUMBER_OF_FAILED_SUBOPERATIONS) {
            self.failed = v;
        }
    }
}

fn retrieve_identifier(study_uid: &str, series_uid: Option<&str>) -> InMemDicomObject {
    let mut id = InMemDicomObject::new_empty();
    id.put(element_str(tags::QUERY_RETRIEVE_LEVEL, VR::CS, if series_uid.is_some() { "SERIES" } else { "STUDY" }));
    id.put(element_str(tags::STUDY_INSTANCE_UID, VR::UI, study_uid));
    if let Some(s) = series_uid {
        id.put(element_str(tags::SERIES_INSTANCE_UID, VR::UI, s));
    }
    id
}

/// C-GET: the PACS sends the instances back over the same association.
pub fn retrieve_get(
    node: &PacsNode,
    local_ae: &str,
    study_uid: &str,
    series_uid: Option<&str>,
    store_root: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(RetrieveProgress),
) -> Result<(Vec<PathBuf>, RetrieveProgress)> {
    let sop = uids::STUDY_ROOT_QUERY_RETRIEVE_INFORMATION_MODEL_GET;
    let mut opts = ClientAssociationOptions::new()
        .with_presentation_context(sop, vec![uids::EXPLICIT_VR_LITTLE_ENDIAN, uids::IMPLICIT_VR_LITTLE_ENDIAN]);
    for sc in STORAGE_CLASSES {
        opts = opts.with_presentation_context(*sc, STORAGE_TS.to_vec()).with_role_selection(*sc, false, true);
    }
    let mut assoc = connect(node, local_ae, opts)?;
    let Ok((pc, ts)) = accepted_pc(&assoc, sop) else {
        let _ = assoc.abort();
        bail!("Este PACS não aceita C-GET. Em PACS → Servidores, edite o servidor e escolha C-MOVE (o receptor do Slyced liga sozinho; cadastre no PACS o AE {local_ae} com o IP deste computador).");
    };
    let pc_ts: HashMap<u8, String> = assoc
        .presentation_contexts()
        .iter()
        .filter(|p| p.reason == PresentationContextResultReason::Acceptance)
        .map(|p| (p.id, p.transfer_syntax.clone()))
        .collect();

    let data = encode_dataset(&retrieve_identifier(study_uid, series_uid), &ts)?;
    send_message(&mut assoc, pc, request(C_GET_RQ, sop, 1), Some(&data))?;

    let mut reader = MessageReader::default();
    let mut progress = RetrieveProgress::default();
    let mut files = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = assoc.abort();
            bail!("Transferência cancelada.");
        }
        match reader.read(&mut assoc)? {
            Incoming::Message(m) if m.field() == C_STORE_RQ => {
                let sop_class = m.str(tags::AFFECTED_SOP_CLASS_UID);
                let sop_inst = m.str(tags::AFFECTED_SOP_INSTANCE_UID);
                let status = match (&m.data, pc_ts.get(&m.pc_id)) {
                    (Some(d), Some(ts)) => match store_instance(store_root, &sop_class, &sop_inst, ts, d) {
                        Ok(p) => {
                            files.push(p);
                            progress.received += 1;
                            STATUS_SUCCESS
                        }
                        Err(e) => {
                            log::warn!("falha ao gravar {sop_inst}: {e:#}");
                            0xA700
                        }
                    },
                    _ => 0xC000,
                };
                let rsp = response(C_STORE_RSP, &sop_class, m.message_id(), status, Some(&sop_inst));
                send_message(&mut assoc, m.pc_id, rsp, None)?;
                on_progress(progress);
            }
            Incoming::Message(m) if m.field() == C_GET_RSP => {
                progress.update_from(&m);
                on_progress(progress);
                let st = m.status();
                if !is_pending(st) {
                    let _ = assoc.release();
                    if st != STATUS_SUCCESS && files.is_empty() {
                        let comment = m.str(tags::ERROR_COMMENT);
                        let comment = if comment.trim().is_empty() { String::new() } else { format!(" — o PACS informou: \"{}\"", comment.trim()) };
                        bail!("C-GET falhou: {}{comment}. Se o PACS só enviar exames por C-MOVE, troque a forma de recuperação na edição do servidor.", status_text(st));
                    }
                    return Ok((files, progress));
                }
            }
            Incoming::Message(m) => log::warn!("mensagem DIMSE ignorada: 0x{:04X}", m.field()),
            Incoming::Released => return Ok((files, progress)),
        }
    }
}

/// C-MOVE: the PACS pushes the instances to our Storage SCP (`destination_ae`).
pub fn retrieve_move(
    node: &PacsNode,
    local_ae: &str,
    destination_ae: &str,
    study_uid: &str,
    series_uid: Option<&str>,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(RetrieveProgress),
) -> Result<RetrieveProgress> {
    let sop = uids::STUDY_ROOT_QUERY_RETRIEVE_INFORMATION_MODEL_MOVE;
    let mut assoc = connect(
        node,
        local_ae,
        ClientAssociationOptions::new()
            .with_presentation_context(sop, vec![uids::EXPLICIT_VR_LITTLE_ENDIAN, uids::IMPLICIT_VR_LITTLE_ENDIAN]),
    )?;
    let (pc, ts) = accepted_pc(&assoc, sop)?;
    let mut cmd = request(C_MOVE_RQ, sop, 1);
    cmd.put(element_str(tags::MOVE_DESTINATION, VR::AE, destination_ae));
    let data = encode_dataset(&retrieve_identifier(study_uid, series_uid), &ts)?;
    send_message(&mut assoc, pc, cmd, Some(&data))?;

    let mut reader = MessageReader::default();
    let mut progress = RetrieveProgress::default();
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = assoc.abort();
            bail!("Transferência cancelada.");
        }
        match reader.read(&mut assoc)? {
            Incoming::Message(m) if m.field() == C_MOVE_RSP => {
                progress.update_from(&m);
                on_progress(progress);
                let st = m.status();
                if !is_pending(st) {
                    let _ = assoc.release();
                    if st != STATUS_SUCCESS && progress.completed == 0 {
                        bail!("C-MOVE falhou: {}", status_text(st));
                    }
                    return Ok(progress);
                }
            }
            Incoming::Message(m) => log::warn!("mensagem DIMSE ignorada: 0x{:04X}", m.field()),
            Incoming::Released => return Ok(progress),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(id: &InMemDicomObject) -> Vec<dicom_core::Tag> {
        id.iter().map(|e| e.header().tag).collect()
    }

    #[test]
    fn simpler_query_forms() {
        let q = StudyQuery { patient_name: "silva".into(), patient_id: "ab12".into(), date_from: "20261030".into(), ..Default::default() };
        let full = study_identifier(&q, QueryStyle::Full);
        let plain = study_identifier(&q, QueryStyle::Plain);
        let minimal = study_identifier(&q, QueryStyle::Minimal);
        assert!(keys(&full).contains(&tags::NUMBER_OF_STUDY_RELATED_INSTANCES));
        assert!(!keys(&plain).contains(&tags::NUMBER_OF_STUDY_RELATED_INSTANCES) && !keys(&plain).contains(&tags::MODALITIES_IN_STUDY));
        // Only the keys every Study Root SCP must support.
        let required = [
            tags::QUERY_RETRIEVE_LEVEL,
            tags::PATIENT_NAME,
            tags::PATIENT_ID,
            tags::STUDY_DATE,
            tags::STUDY_TIME,
            tags::ACCESSION_NUMBER,
            tags::STUDY_ID,
            tags::STUDY_INSTANCE_UID,
        ];
        assert!(keys(&minimal).iter().all(|t| required.contains(t)));
        assert_eq!(text(&full, tags::PATIENT_NAME), "*SILVA*");
        assert_eq!(text(&minimal, tags::PATIENT_NAME), "SILVA*");
        assert_eq!(text(&minimal, tags::PATIENT_ID), "ab12", "IDs keep their case");
        assert_eq!(text(&full, tags::STUDY_DATE), "20261030-");
        assert_eq!(text(&plain, tags::STUDY_DATE), format!("20261030-{}", today_yyyymmdd()));
        assert_eq!(closed_range("-20250101"), "19000101-20250101");
        assert_eq!(closed_range("20250101"), "20250101");
    }
}
