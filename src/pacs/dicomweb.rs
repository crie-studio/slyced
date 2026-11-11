//! DICOMweb client: QIDO-RS (study/series search) and WADO-RS (download of a
//! study or series as `multipart/related; type="application/dicom"`).
//!
//! HTTPS goes through SChannel (native-tls), so certificates trusted by Windows,
//! including a hospital's internal CA, are trusted here as well.

use super::dimse::instance_path;
use super::scu::{ascii_text, contains_pattern, RetrieveProgress, SeriesResult, StudyQuery, StudyResult};
use crate::config::{PacsNode, WebAuth, APP_VERSION};
use crate::dicom::tagutil::text;
use anyhow::{anyhow, bail, Context as _, Result};
use dicom_core::Tag;
use dicom_dictionary_std::tags;
use dicom_object::OpenFileOptions;
use memchr::memmem;
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use ureq::http::Response;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use ureq::Body;

const ACCEPT_JSON: &str = "application/dicom+json, application/json;q=0.9";
const ACCEPT_DICOM_ANY_TS: &str = "multipart/related; type=\"application/dicom\"; transfer-syntax=*";
const ACCEPT_DICOM: &str = "multipart/related; type=\"application/dicom\"";
/// QIDO responses are small; this only guards against a misbehaving server.
const MAX_JSON: u64 = 256 << 20;

const STUDY_FIELDS: &[Tag] = &[
    tags::PATIENT_BIRTH_DATE,
    tags::PATIENT_SEX,
    tags::STUDY_TIME,
    tags::STUDY_DESCRIPTION,
    tags::MODALITIES_IN_STUDY,
    tags::REFERRING_PHYSICIAN_NAME,
    tags::NUMBER_OF_STUDY_RELATED_SERIES,
    tags::NUMBER_OF_STUDY_RELATED_INSTANCES,
];
const SERIES_FIELDS: &[Tag] =
    &[tags::SERIES_NUMBER, tags::MODALITY, tags::SERIES_DESCRIPTION, tags::NUMBER_OF_SERIES_RELATED_INSTANCES];

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let tls = TlsConfig::builder().provider(TlsProvider::NativeTls).root_certs(RootCerts::PlatformVerifier).build();
        ureq::Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .user_agent(format!("Slyced/{APP_VERSION}"))
            .timeout_connect(Some(Duration::from_secs(8)))
            .timeout_recv_response(Some(Duration::from_secs(120)))
            // Idle limit while streaming: a stalled server fails instead of hanging forever.
            .timeout_per_read(Some(Duration::from_secs(60)))
            .build()
            .into()
    })
}

fn base_url(node: &PacsNode) -> Result<&str> {
    let b = node.base_url.trim().trim_end_matches('/');
    let lower = b.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) || b.len() < 9 {
        bail!(trf!(
            "URL DICOMweb inválida: \"{}\". Use, por exemplo, http://servidor:8042/dicom-web",
            "Invalid DICOMweb URL: \"{}\". Use, for example, http://server:8042/dicom-web",
            node.base_url.trim()
        ));
    }
    Ok(b)
}

/// Percent-encodes a path segment (UIDs normally pass through unchanged).
fn segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.trim().trim_end_matches('\0').bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub(crate) fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for c in input.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() { T[(n >> (18 - 6 * i)) as usize & 63] as char } else { '=' });
        }
    }
    out
}

fn get(node: &PacsNode, url: &str, accept: &str, query: &[(&str, &str)]) -> Result<Response<Body>> {
    let mut rq = agent().get(url).header("Accept", accept);
    for (k, v) in query.iter().filter(|(_, v)| !v.is_empty()) {
        rq = rq.query(k, v);
    }
    rq = match node.auth {
        WebAuth::None => rq,
        WebAuth::Basic => rq.header("Authorization", format!("Basic {}", base64(format!("{}:{}", node.username, node.secret).as_bytes()))),
        WebAuth::Bearer => rq.header("Authorization", format!("Bearer {}", node.secret.trim())),
    };
    rq.call().map_err(|e| net_error(e, url))
}

fn net_error(e: ureq::Error, url: &str) -> anyhow::Error {
    use ureq::Error as E;
    // Windows reports failed name resolution as WSAHOST_NOT_FOUND / WSANO_DATA.
    let unresolved = matches!(&e, E::HostNotFound) || matches!(&e, E::Io(io) if matches!(io.raw_os_error(), Some(11001 | 11004)));
    match e {
        _ if unresolved => anyhow!(trf!(
            "Servidor DICOMweb não encontrado ({url}). Confira o endereço.",
            "DICOMweb server not found ({url}). Check the address."
        )),
        E::ConnectionFailed => anyhow!(trf!(
            "Não foi possível conectar em {url}. Confira o endereço, a porta e o firewall.",
            "Could not connect to {url}. Check the address, the port and the firewall."
        )),
        E::Timeout(_) => anyhow!(trf!("Tempo esgotado aguardando o servidor DICOMweb ({url}).", "Timed out waiting for the DICOMweb server ({url}).")),
        E::Io(io) if io.kind() == std::io::ErrorKind::ConnectionRefused => {
            anyhow!(trf!("Conexão recusada em {url}. O serviço DICOMweb está ativo?", "Connection refused at {url}. Is the DICOMweb service running?"))
        }
        E::Tls(_) | E::NativeTls(_) | E::Der(_) => {
            anyhow!(trf!("Falha na conexão segura (HTTPS) com {url}: {e}", "Secure connection (HTTPS) to {url} failed: {e}"))
        }
        other => anyhow!(trf!("Erro de comunicação com {url}: {other}", "Communication error with {url}: {other}")),
    }
}

fn check_status(resp: &Response<Body>, what: &str) -> Result<()> {
    let st = resp.status().as_u16();
    match st {
        200..=299 => Ok(()),
        401 | 403 => bail!(trf!(
            "Acesso negado pelo servidor DICOMweb (HTTP {st}). Confira usuário e senha ou o token.",
            "Access denied by the DICOMweb server (HTTP {st}). Check the username and password or the token."
        )),
        404 => bail!(trf!(
            "{what}: não encontrado (HTTP 404). Confira a URL base (ex.: http://servidor:8042/dicom-web).",
            "{what}: not found (HTTP 404). Check the base URL (e.g. http://server:8042/dicom-web)."
        )),
        _ => bail!(trf!("{what}: o servidor DICOMweb respondeu HTTP {st}.", "{what}: the DICOMweb server answered HTTP {st}.")),
    }
}

/// QIDO-RS search at `path` (relative to the base URL).
fn qido(node: &PacsNode, path: &str, query: &[(&str, &str)], fields: &[Tag]) -> Result<Vec<Value>> {
    let url = format!("{}/{path}", base_url(node)?);
    let fields: Vec<String> = fields.iter().map(|t| key(*t)).collect();
    let mut q = query.to_vec();
    q.extend(fields.iter().map(|f| ("includefield", f.as_str())));
    let mut resp = get(node, &url, ACCEPT_JSON, &q)?;
    if resp.status().as_u16() == 204 {
        return Ok(Vec::new());
    }
    check_status(&resp, tr!("Pesquisa QIDO-RS", "QIDO-RS query"))?;
    let bytes = resp.body_mut().with_config().limit(MAX_JSON).read_to_vec().map_err(|e| net_error(e, &url))?;
    parse_qido(&bytes)
}

pub(crate) fn parse_qido(bytes: &[u8]) -> Result<Vec<Value>> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }
    match serde_json::from_slice(bytes).context(tr!("resposta QIDO-RS não é um JSON válido", "QIDO-RS response is not valid JSON"))? {
        Value::Array(list) => Ok(list),
        _ => bail!(tr!("resposta QIDO-RS inesperada (esperada uma lista JSON)", "unexpected QIDO-RS response (expected a JSON list)")),
    }
}

/// DICOM JSON attribute key: `GGGGEEEE` in upper-case hex.
fn key(tag: Tag) -> String {
    format!("{:04X}{:04X}", tag.group(), tag.element())
}

fn values(obj: &Value, tag: Tag) -> Option<&Vec<Value>> {
    obj.get(key(tag))?.get("Value")?.as_array()
}

/// Text of an attribute; multiple values are joined with `\` like in a data set.
pub(crate) fn json_text(obj: &Value, tag: Tag) -> String {
    let Some(vals) = values(obj, tag) else { return String::new() };
    let text = |v: &Value| match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        // Person names: {"Alphabetic": "DOE^JOHN", "Ideographic": ...}
        Value::Object(pn) => ["Alphabetic", "Ideographic", "Phonetic"]
            .iter()
            .find_map(|k| pn.get(*k).and_then(Value::as_str))
            .unwrap_or_default()
            .trim()
            .to_string(),
        _ => String::new(),
    };
    vals.iter().map(text).collect::<Vec<_>>().join("\\")
}

/// IS/US/UL values come as JSON numbers, but some servers send strings.
pub(crate) fn json_int(obj: &Value, tag: Tag) -> Option<i32> {
    let v = values(obj, tag)?.first()?;
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .or_else(|| v.as_f64().map(|f| f.round() as i32))
        .or_else(|| v.as_str()?.trim().parse().ok())
}

pub(crate) fn study_from_json(o: &Value) -> StudyResult {
    StudyResult {
        study_uid: json_text(o, tags::STUDY_INSTANCE_UID),
        patient_name: json_text(o, tags::PATIENT_NAME),
        patient_id: json_text(o, tags::PATIENT_ID),
        birth_date: json_text(o, tags::PATIENT_BIRTH_DATE),
        sex: json_text(o, tags::PATIENT_SEX),
        date: json_text(o, tags::STUDY_DATE),
        time: json_text(o, tags::STUDY_TIME),
        description: json_text(o, tags::STUDY_DESCRIPTION),
        accession: json_text(o, tags::ACCESSION_NUMBER),
        modalities: json_text(o, tags::MODALITIES_IN_STUDY).replace('\\', " "),
        referring: json_text(o, tags::REFERRING_PHYSICIAN_NAME),
        series_count: json_int(o, tags::NUMBER_OF_STUDY_RELATED_SERIES),
        instance_count: json_int(o, tags::NUMBER_OF_STUDY_RELATED_INSTANCES),
    }
}

pub(crate) fn series_from_json(o: &Value) -> SeriesResult {
    SeriesResult {
        series_uid: json_text(o, tags::SERIES_INSTANCE_UID),
        number: json_int(o, tags::SERIES_NUMBER),
        modality: json_text(o, tags::MODALITY),
        description: json_text(o, tags::SERIES_DESCRIPTION),
        instance_count: json_int(o, tags::NUMBER_OF_SERIES_RELATED_INSTANCES),
    }
}

/// Connection test: the cheapest valid QIDO query.
pub fn echo(node: &PacsNode) -> Result<Duration> {
    let start = Instant::now();
    qido(node, "studies", &[("limit", "1")], &[])?;
    Ok(start.elapsed())
}

pub fn find_studies(node: &PacsNode, q: &StudyQuery) -> Result<Vec<StudyResult>> {
    let (name, id, acc) = (contains_pattern(&q.patient_name), ascii_text(&q.patient_id), ascii_text(&q.accession));
    let (date, modality, desc) = (q.date_range(), q.modality.trim().to_ascii_uppercase(), contains_pattern(&q.description));
    let query = [
        ("PatientName", name.as_str()),
        ("PatientID", id.as_str()),
        ("AccessionNumber", acc.as_str()),
        ("StudyDate", date.as_str()),
        ("ModalitiesInStudy", modality.as_str()),
        ("StudyDescription", desc.as_str()),
    ];
    let mut results: Vec<StudyResult> =
        qido(node, "studies", &query, STUDY_FIELDS)?.iter().map(study_from_json).filter(|r| !r.study_uid.is_empty()).collect();
    results.sort_by(|a, b| (&b.date, &b.time).cmp(&(&a.date, &a.time)));
    Ok(results)
}

pub fn find_series(node: &PacsNode, study_uid: &str) -> Result<Vec<SeriesResult>> {
    let path = format!("studies/{}/series", segment(study_uid));
    let mut results: Vec<SeriesResult> =
        qido(node, &path, &[], SERIES_FIELDS)?.iter().map(series_from_json).filter(|r| !r.series_uid.is_empty()).collect();
    results.sort_by_key(|r| r.number.unwrap_or(i32::MAX));
    Ok(results)
}

/// Instance count announced by QIDO, used only for the progress bar.
fn expected_instances(node: &PacsNode, study_uid: &str, series_uid: Option<&str>) -> Option<u32> {
    let found = match series_uid {
        Some(s) => qido(
            node,
            &format!("studies/{}/series", segment(study_uid)),
            &[("SeriesInstanceUID", s)],
            &[tags::NUMBER_OF_SERIES_RELATED_INSTANCES],
        )
        .ok()?
        .first()
        .and_then(|o| json_int(o, tags::NUMBER_OF_SERIES_RELATED_INSTANCES)),
        None => qido(node, "studies", &[("StudyInstanceUID", study_uid)], &[tags::NUMBER_OF_STUDY_RELATED_INSTANCES])
            .ok()?
            .first()
            .and_then(|o| json_int(o, tags::NUMBER_OF_STUDY_RELATED_INSTANCES)),
    };
    found.and_then(|n| u32::try_from(n).ok())
}

/// WADO-RS: downloads a study (or one series) into `store_root`, one file per
/// instance, laid out like C-GET results. Parts are streamed straight to disk.
pub fn retrieve(
    node: &PacsNode,
    study_uid: &str,
    series_uid: Option<&str>,
    store_root: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(RetrieveProgress),
) -> Result<(Vec<PathBuf>, RetrieveProgress)> {
    let expected = expected_instances(node, study_uid, series_uid).unwrap_or(0);
    let mut progress = RetrieveProgress { remaining: expected, ..Default::default() };
    on_progress(progress);

    let mut url = format!("{}/studies/{}", base_url(node)?, segment(study_uid));
    if let Some(s) = series_uid {
        url.push_str(&format!("/series/{}", segment(s)));
    }
    // Prefer the stored transfer syntax (no transcoding, keeps JPEG etc. compressed);
    // servers that do not support `*` get the default request (Explicit VR Little Endian).
    let mut resp = get(node, &url, ACCEPT_DICOM_ANY_TS, &[])?;
    if matches!(resp.status().as_u16(), 400 | 406 | 415) {
        log::info!("WADO-RS: servidor recusou transfer-syntax=* (HTTP {}), repetindo sem", resp.status().as_u16());
        resp = get(node, &url, ACCEPT_DICOM, &[])?;
    }
    check_status(&resp, if series_uid.is_some() { tr!("Download da série", "Series download") } else { tr!("Download do estudo", "Study download") })?;
    let content_type = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    let boundary = multipart_boundary(&content_type).ok_or_else(|| {
        anyhow!(trf!(
            "Resposta WADO-RS inesperada (Content-Type: \"{content_type}\"); esperado multipart/related.",
            "Unexpected WADO-RS response (Content-Type: \"{content_type}\"); expected multipart/related."
        ))
    })?;

    std::fs::create_dir_all(store_root)?;
    let mut parts = Multipart::new(resp.into_body().into_reader(), &boundary);
    let mut files = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!(tr!("Transferência cancelada.", "Transfer canceled."));
        }
        let Some(headers) = parts.next_part().context(tr!("download interrompido", "download interrupted"))? else { break };
        let part_type = header(&headers, "content-type").unwrap_or_default().to_ascii_lowercase();
        if !(part_type.is_empty() || part_type.starts_with("application/dicom")) {
            log::warn!("WADO-RS: parte ignorada ({part_type})");
            parts.copy_body(&mut std::io::sink(), cancel)?;
            progress.failed += 1;
        } else {
            match save_part(&mut parts, store_root, cancel) {
                Ok(path) => {
                    files.push(path);
                    progress.received += 1;
                    progress.completed += 1;
                }
                Err(e) if cancel.load(Ordering::Relaxed) => return Err(e),
                Err(e) if e.downcast_ref::<std::io::Error>().is_some() => return Err(e.context(tr!("download interrompido", "download interrupted"))),
                Err(e) => {
                    log::warn!("WADO-RS: instância descartada: {e:#}");
                    progress.failed += 1;
                }
            }
        }
        progress.remaining = expected.saturating_sub(progress.completed + progress.failed);
        on_progress(progress);
    }
    if files.is_empty() && progress.failed > 0 {
        bail!(trf!(
            "Nenhuma instância válida recebida ({} parte(s) rejeitada(s)).",
            "No valid instance received ({} part(s) rejected).",
            progress.failed
        ));
    }
    Ok((files, progress))
}

/// Streams the current part to a temporary file, then moves it to its final
/// place once the UIDs are read from its header.
fn save_part<R: Read>(parts: &mut Multipart<R>, root: &Path, cancel: &AtomicBool) -> Result<PathBuf> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let tmp = root.join(format!(".wado-{}-{}.part", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
    let result = (|| {
        {
            let mut f = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&tmp)?);
            parts.copy_body(&mut f, cancel)?;
            f.flush()?;
        }
        let (study, series, sop) = {
            // The UIDs come early; stopping there skips e.g. megabytes of RT contours.
            let obj = OpenFileOptions::new()
                .read_until(tags::SERIES_NUMBER)
                .open_file(&tmp)
                .map_err(|e| anyhow!(trf!("parte recebida não é um arquivo DICOM: {e}", "received part is not a DICOM file: {e}")))?;
            let mut sop = text(&obj, tags::SOP_INSTANCE_UID);
            if sop.is_empty() {
                sop = obj.meta().media_storage_sop_instance_uid().trim_end_matches('\0').to_string();
            }
            (text(&obj, tags::STUDY_INSTANCE_UID), text(&obj, tags::SERIES_INSTANCE_UID), sop)
        };
        let path = instance_path(root, &study, &series, &sop);
        std::fs::create_dir_all(path.parent().unwrap_or(root))?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Part headers as (name, value) pairs, in order.
type Headers = Vec<(String, String)>;

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// Splits `a=1; b="x;y"` into parameters, honoring quoted strings.
fn mime_params(s: &str) -> Vec<(String, String)> {
    let mut parts = Vec::new();
    let (mut cur, mut quoted) = (String::new(), false);
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    parts.push(cur);
    parts
        .iter()
        .skip(1)
        .filter_map(|p| p.split_once('=').map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string())))
        .collect()
}

/// Boundary of a `multipart/related` Content-Type.
pub(crate) fn multipart_boundary(content_type: &str) -> Option<String> {
    if !content_type.trim_start().to_ascii_lowercase().starts_with("multipart/") {
        return None;
    }
    mime_params(content_type).into_iter().find(|(k, _)| k == "boundary").map(|(_, v)| v).filter(|b| !b.is_empty())
}

/// Streaming reader for a MIME multipart body (RFC 2046). Memory use is bounded
/// by the buffer size no matter how large each part is.
pub(crate) struct Multipart<R> {
    inner: R,
    buf: Vec<u8>,
    start: usize,
    end: usize,
    eof: bool,
    /// `\r\n--boundary`
    delim: Vec<u8>,
    finder: memmem::Finder<'static>,
    /// True while the current part's body has not been consumed yet.
    in_body: bool,
    done: bool,
}

const MAX_PART_HEADERS: usize = 64 << 10;

impl<R: Read> Multipart<R> {
    pub(crate) fn new(inner: R, boundary: &str) -> Self {
        let delim = [b"\r\n--", boundary.as_bytes()].concat();
        let finder = memmem::Finder::new(&delim).into_owned();
        // A leading CRLF lets the first delimiter (which may start the body) match like the others.
        let mut buf = vec![0u8; 256 << 10];
        buf[..2].copy_from_slice(b"\r\n");
        Self { inner, buf, start: 0, end: 2, eof: false, delim, finder, in_body: false, done: false }
    }

    fn avail(&self) -> &[u8] {
        &self.buf[self.start..self.end]
    }

    /// Reads more input, keeping unconsumed bytes. Returns false at end of input.
    fn fill(&mut self) -> std::io::Result<bool> {
        if self.eof {
            return Ok(false);
        }
        if self.start > 0 {
            self.buf.copy_within(self.start..self.end, 0);
            self.end -= self.start;
            self.start = 0;
        }
        if self.end == self.buf.len() {
            self.buf.resize(self.buf.len() * 2, 0);
        }
        loop {
            match self.inner.read(&mut self.buf[self.end..]) {
                Ok(0) => {
                    self.eof = true;
                    return Ok(false);
                }
                Ok(n) => {
                    self.end += n;
                    return Ok(true);
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }

    fn truncated() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::UnexpectedEof, tr!("resposta multipart incompleta", "incomplete multipart response"))
    }

    /// Advances to the next part and returns its headers, or `None` after the
    /// closing delimiter. An unread body of the previous part is skipped.
    pub(crate) fn next_part(&mut self) -> std::io::Result<Option<Headers>> {
        if self.in_body {
            self.copy_body(&mut std::io::sink(), &AtomicBool::new(false)).map_err(std::io::Error::other)?;
        }
        if self.done {
            return Ok(None);
        }
        // Find the delimiter (skips the preamble before the first one).
        loop {
            if let Some(i) = self.finder.find(self.avail()) {
                self.start += i + self.delim.len();
                break;
            }
            let keep = self.delim.len() - 1;
            if self.end - self.start > keep {
                self.start = self.end - keep;
            }
            if !self.fill()? {
                return Err(Self::truncated());
            }
        }
        // `--` closes the body; otherwise skip transport padding up to the CRLF.
        loop {
            let a = self.avail();
            if a.len() >= 2 && &a[..2] == b"--" {
                self.done = true;
                return Ok(None);
            }
            if let Some(i) = memmem::find(a, b"\r\n") {
                self.start += i + 2;
                break;
            }
            if a.len() > 1024 {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, tr!("delimitador multipart inválido", "invalid multipart delimiter")));
            }
            if !self.fill()? {
                return Err(Self::truncated());
            }
        }
        // Part headers end with an empty line (immediately, when there are none).
        let headers = loop {
            let a = self.avail();
            if a.starts_with(b"\r\n") {
                self.start += 2;
                break Vec::new();
            }
            if let Some(i) = memmem::find(a, b"\r\n\r\n") {
                let block = String::from_utf8_lossy(&a[..i]).into_owned();
                self.start += i + 4;
                break block
                    .split("\r\n")
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .collect();
            }
            if a.len() > MAX_PART_HEADERS {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, tr!("cabeçalhos multipart muito grandes", "multipart headers too large")));
            }
            if !self.fill()? {
                return Err(Self::truncated());
            }
        };
        self.in_body = true;
        Ok(Some(headers))
    }

    /// Copies the current part's body to `out`, stopping before the next delimiter.
    pub(crate) fn copy_body(&mut self, out: &mut impl Write, cancel: &AtomicBool) -> Result<u64> {
        if !self.in_body {
            return Ok(0);
        }
        let mut total = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!(tr!("Transferência cancelada.", "Transfer canceled."));
            }
            if let Some(i) = self.finder.find(self.avail()) {
                out.write_all(&self.buf[self.start..self.start + i])?;
                self.start += i;
                self.in_body = false;
                return Ok(total + i as u64);
            }
            // Everything except a possible partial delimiter at the end is body.
            let keep = self.delim.len() - 1;
            let n = (self.end - self.start).saturating_sub(keep);
            if n > 0 {
                out.write_all(&self.buf[self.start..self.start + n])?;
                self.start += n;
                total += n as u64;
            }
            if !self.fill()? {
                return Err(Self::truncated().into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hands out input in small, uneven pieces so delimiters straddle reads.
    struct Trickle<'a>(&'a [u8], usize);
    impl Read for Trickle<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            self.1 += 1;
            let n = (self.1 % 7 + 1).min(self.0.len()).min(out.len());
            out[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    fn body(parts: &[&[u8]], boundary: &str) -> Vec<u8> {
        let mut b = b"preamble to ignore\r\n".to_vec();
        for p in parts {
            b.extend_from_slice(format!("--{boundary}  \r\nContent-Type: application/dicom\r\nContent-Length: {}\r\n\r\n", p.len()).as_bytes());
            b.extend_from_slice(p);
            b.extend_from_slice(b"\r\n");
        }
        b.extend_from_slice(format!("--{boundary}--\r\nepilogue").as_bytes());
        b
    }

    fn read_all<R: Read>(mut m: Multipart<R>) -> Vec<(Headers, Vec<u8>)> {
        let mut out = Vec::new();
        while let Some(h) = m.next_part().unwrap() {
            let mut data = Vec::new();
            m.copy_body(&mut data, &AtomicBool::new(false)).unwrap();
            out.push((h, data));
        }
        out
    }

    #[test]
    fn multipart_parts_survive_any_read_pattern() {
        let boundary = "XYZ-123";
        // Part contents that look like a delimiter (missing CRLF, wrong boundary, CRLF only).
        let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let parts: Vec<&[u8]> = vec![b"DICM first", b"--XYZ-123 without CRLF", b"\r\n--XYZ-12", b"", &big, b"\r\n"];
        let raw = body(&parts, boundary);
        for got in [read_all(Multipart::new(&raw[..], boundary)), read_all(Multipart::new(Trickle(&raw, 0), boundary))] {
            assert_eq!(got.len(), parts.len());
            for ((h, data), want) in got.iter().zip(&parts) {
                assert_eq!(header(h, "content-type"), Some("application/dicom"));
                assert_eq!(&data[..], *want);
            }
        }
    }

    #[test]
    fn multipart_edge_cases() {
        // Body starts with the delimiter (no preamble), part without headers, unread body skipped.
        let raw = b"--b\r\n\r\nfirst\r\n--b\r\nX-A: 1\r\n\r\nsecond\r\n--b--";
        let mut m = Multipart::new(&raw[..], "b");
        assert!(m.next_part().unwrap().unwrap().is_empty());
        let h = m.next_part().unwrap().unwrap();
        assert_eq!(header(&h, "x-a"), Some("1"));
        let mut data = Vec::new();
        m.copy_body(&mut data, &AtomicBool::new(false)).unwrap();
        assert_eq!(data, b"second");
        assert!(m.next_part().unwrap().is_none());
        assert!(m.next_part().unwrap().is_none());

        // Truncated stream is an error, not a silently short file.
        let raw = b"--b\r\n\r\nunfinished data";
        let mut m = Multipart::new(&raw[..], "b");
        m.next_part().unwrap();
        assert!(m.copy_body(&mut Vec::new(), &AtomicBool::new(false)).is_err());

        // Cancellation stops the copy.
        let raw = body(&[b"data"], "b");
        let mut m = Multipart::new(&raw[..], "b");
        m.next_part().unwrap();
        assert!(m.copy_body(&mut Vec::new(), &AtomicBool::new(true)).is_err());
    }

    #[test]
    fn boundary_from_content_type() {
        assert_eq!(
            multipart_boundary(r#"multipart/related; type="application/dicom"; boundary=abc123"#).as_deref(),
            Some("abc123")
        );
        assert_eq!(
            multipart_boundary(r#"Multipart/Related;boundary="a;b c";type="application/dicom; transfer-syntax=*""#).as_deref(),
            Some("a;b c")
        );
        assert_eq!(multipart_boundary("application/dicom"), None);
        assert_eq!(multipart_boundary("multipart/related; type=\"application/dicom\""), None);
    }

    #[test]
    fn qido_json_mapping() {
        let json = r#"[
          {
            "00080020": {"vr": "DA", "Value": ["20240315"]},
            "00080030": {"vr": "TM", "Value": ["101500.000"]},
            "00080050": {"vr": "SH", "Value": ["ACC42"]},
            "00080061": {"vr": "CS", "Value": ["CT", "SR"]},
            "00080090": {"vr": "PN", "Value": [{"Alphabetic": "HOUSE^GREGORY"}]},
            "00081030": {"vr": "LO", "Value": ["TC DE CRÂNIO"]},
            "00100010": {"vr": "PN", "Value": [{"Alphabetic": "SILVA^JOSÉ", "Ideographic": "x"}]},
            "00100020": {"vr": "LO", "Value": ["123"]},
            "00100030": {"vr": "DA"},
            "00100040": {"vr": "CS", "Value": ["M"]},
            "0020000D": {"vr": "UI", "Value": ["1.2.3.4"]},
            "00201206": {"vr": "IS", "Value": [2]},
            "00201208": {"vr": "IS", "Value": ["154"]}
          },
          {"00100010": {"vr": "PN", "Value": [{"Ideographic": "山田^太郎"}]}}
        ]"#.as_bytes();
        let list = parse_qido(json).unwrap();
        let s = study_from_json(&list[0]);
        assert_eq!(s.study_uid, "1.2.3.4");
        assert_eq!(s.patient_name, "SILVA^JOSÉ");
        assert_eq!(s.patient_id, "123");
        assert_eq!(s.birth_date, "");
        assert_eq!(s.sex, "M");
        assert_eq!((s.date.as_str(), s.time.as_str()), ("20240315", "101500.000"));
        assert_eq!(s.accession, "ACC42");
        assert_eq!(s.modalities, "CT SR");
        assert_eq!(s.referring, "HOUSE^GREGORY");
        assert_eq!(s.description, "TC DE CRÂNIO");
        assert_eq!((s.series_count, s.instance_count), (Some(2), Some(154)));
        let other = study_from_json(&list[1]);
        assert_eq!(other.patient_name, "山田^太郎");
        assert!(other.study_uid.is_empty());

        let series = parse_qido(
            br#"[{"0020000E": {"vr": "UI", "Value": ["1.2.3.4.5"]}, "00200011": {"vr": "IS", "Value": [3]},
                  "00080060": {"vr": "CS", "Value": ["MR"]}, "0008103E": {"vr": "LO", "Value": ["T1 AX"]},
                  "00201209": {"vr": "IS", "Value": [24.0]}}]"#,
        )
        .unwrap();
        let r = series_from_json(&series[0]);
        assert_eq!((r.series_uid.as_str(), r.number, r.modality.as_str()), ("1.2.3.4.5", Some(3), "MR"));
        assert_eq!((r.description.as_str(), r.instance_count), ("T1 AX", Some(24)));

        assert!(parse_qido(b"").unwrap().is_empty());
        assert!(parse_qido(b"{}").is_err());
    }

    #[test]
    fn helpers() {
        assert_eq!(base64(b"Aladdin:open sesame"), "QWxhZGRpbjpvcGVuIHNlc2FtZQ==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b""), "");
        assert_eq!(segment("1.2.840.113619\0"), "1.2.840.113619");
        assert_eq!(segment("../x y"), "..%2Fx%20y");
        assert_eq!(key(tags::STUDY_INSTANCE_UID), "0020000D");
        let node = |url: &str| PacsNode { base_url: url.into(), ..Default::default() };
        assert_eq!(base_url(&node(" https://pacs/dicom-web/ ")).unwrap(), "https://pacs/dicom-web");
        assert!(base_url(&node("pacs:8042/dicom-web")).is_err());
    }
}
