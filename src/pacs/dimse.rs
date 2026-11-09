//! DIMSE message encoding/decoding on top of a `dicom-ul` association.

use anyhow::{anyhow, bail, Context as _, Result};
use dicom_core::{DataElement, PrimitiveValue, Tag, VR};
use dicom_dictionary_std::{tags, uids};
use dicom_encoding::TransferSyntaxIndex;
use dicom_object::{FileMetaTableBuilder, InMemDicomObject};
use dicom_transfer_syntax_registry::TransferSyntaxRegistry;
use dicom_ul::pdu::{PDataValue, PDataValueType, Pdu};
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const C_STORE_RQ: u16 = 0x0001;
pub const C_STORE_RSP: u16 = 0x8001;
pub const C_GET_RQ: u16 = 0x0010;
pub const C_GET_RSP: u16 = 0x8010;
pub const C_FIND_RQ: u16 = 0x0020;
pub const C_FIND_RSP: u16 = 0x8020;
pub const C_MOVE_RQ: u16 = 0x0021;
pub const C_MOVE_RSP: u16 = 0x8021;
pub const C_ECHO_RQ: u16 = 0x0030;
pub const C_ECHO_RSP: u16 = 0x8030;

pub const NO_DATASET: u16 = 0x0101;
pub const DATASET_PRESENT: u16 = 0x0001;

pub const STATUS_SUCCESS: u16 = 0x0000;

pub fn is_pending(status: u16) -> bool {
    status == 0xFF00 || status == 0xFF01
}

pub fn status_text(status: u16) -> String {
    match status {
        0x0000 => "sucesso".into(),
        0xFF00 | 0xFF01 => "pendente".into(),
        0xFE00 => "cancelado".into(),
        0xB000 => "concluído com falhas parciais".into(),
        0xA701 | 0xA702 => "sem recursos no servidor".into(),
        0xA801 => "destino de C-MOVE desconhecido no PACS (cadastre o AE local no servidor)".into(),
        0xA900 => "identificador não corresponde ao SOP Class".into(),
        0x0122 => "SOP Class não suportada".into(),
        s if s & 0xF000 == 0xC000 => format!("falha no processamento (0x{s:04X})"),
        s => format!("status 0x{s:04X}"),
    }
}

/// Transfer syntaxes proposed for storage, uncompressed first.
pub const STORAGE_TS: &[&str] = &[
    uids::EXPLICIT_VR_LITTLE_ENDIAN,
    uids::IMPLICIT_VR_LITTLE_ENDIAN,
    uids::JPEG_LOSSLESS_SV1,
    uids::JPEG_LOSSLESS,
    uids::JPEG2000_LOSSLESS,
    uids::RLE_LOSSLESS,
    uids::JPEG_BASELINE8_BIT,
    uids::JPEG_EXTENDED12_BIT,
    uids::JPEG2000,
    uids::DEFLATED_EXPLICIT_VR_LITTLE_ENDIAN,
];

/// Storage SOP classes proposed for C-GET (max. 127 presentation contexts per association).
pub const STORAGE_CLASSES: &[&str] = &[
    uids::CT_IMAGE_STORAGE,
    uids::ENHANCED_CT_IMAGE_STORAGE,
    uids::MR_IMAGE_STORAGE,
    uids::ENHANCED_MR_IMAGE_STORAGE,
    uids::COMPUTED_RADIOGRAPHY_IMAGE_STORAGE,
    uids::DIGITAL_X_RAY_IMAGE_STORAGE_FOR_PRESENTATION,
    uids::DIGITAL_X_RAY_IMAGE_STORAGE_FOR_PROCESSING,
    uids::DIGITAL_MAMMOGRAPHY_X_RAY_IMAGE_STORAGE_FOR_PRESENTATION,
    uids::DIGITAL_MAMMOGRAPHY_X_RAY_IMAGE_STORAGE_FOR_PROCESSING,
    uids::DIGITAL_INTRA_ORAL_X_RAY_IMAGE_STORAGE_FOR_PRESENTATION,
    uids::ULTRASOUND_IMAGE_STORAGE,
    uids::ULTRASOUND_MULTI_FRAME_IMAGE_STORAGE,
    uids::SECONDARY_CAPTURE_IMAGE_STORAGE,
    uids::MULTI_FRAME_GRAYSCALE_BYTE_SECONDARY_CAPTURE_IMAGE_STORAGE,
    uids::MULTI_FRAME_GRAYSCALE_WORD_SECONDARY_CAPTURE_IMAGE_STORAGE,
    uids::MULTI_FRAME_TRUE_COLOR_SECONDARY_CAPTURE_IMAGE_STORAGE,
    uids::X_RAY_ANGIOGRAPHIC_IMAGE_STORAGE,
    uids::ENHANCED_XA_IMAGE_STORAGE,
    uids::X_RAY_RADIOFLUOROSCOPIC_IMAGE_STORAGE,
    uids::X_RAY3_D_ANGIOGRAPHIC_IMAGE_STORAGE,
    uids::NUCLEAR_MEDICINE_IMAGE_STORAGE,
    uids::POSITRON_EMISSION_TOMOGRAPHY_IMAGE_STORAGE,
    uids::ENHANCED_PET_IMAGE_STORAGE,
    uids::RT_IMAGE_STORAGE,
    uids::RT_DOSE_STORAGE,
    uids::RT_STRUCTURE_SET_STORAGE,
    uids::VL_PHOTOGRAPHIC_IMAGE_STORAGE,
    uids::VL_ENDOSCOPIC_IMAGE_STORAGE,
    uids::VL_MICROSCOPIC_IMAGE_STORAGE,
    uids::OPHTHALMIC_PHOTOGRAPHY8_BIT_IMAGE_STORAGE,
    uids::BREAST_TOMOSYNTHESIS_IMAGE_STORAGE,
    uids::LEGACY_CONVERTED_ENHANCED_CT_IMAGE_STORAGE,
    uids::LEGACY_CONVERTED_ENHANCED_MR_IMAGE_STORAGE,
    uids::GRAYSCALE_SOFTCOPY_PRESENTATION_STATE_STORAGE,
    uids::BASIC_TEXT_SR_STORAGE,
    uids::ENHANCED_SR_STORAGE,
    uids::COMPREHENSIVE_SR_STORAGE,
    uids::KEY_OBJECT_SELECTION_DOCUMENT_STORAGE,
    uids::ENCAPSULATED_PDF_STORAGE,
    uids::X_RAY_RADIATION_DOSE_SR_STORAGE,
];

pub fn element_str(tag: Tag, vr: VR, value: &str) -> DataElement<InMemDicomObject> {
    DataElement::new(tag, vr, PrimitiveValue::from(value))
}

pub fn element_u16(tag: Tag, value: u16) -> DataElement<InMemDicomObject> {
    DataElement::new(tag, VR::US, PrimitiveValue::from(value))
}

/// Encodes a command set (always implicit VR little endian), including the group length.
pub fn encode_command(mut cmd: InMemDicomObject) -> Result<Vec<u8>> {
    let ts = implicit_le();
    let _ = cmd.take_element(tags::COMMAND_GROUP_LENGTH);
    let mut body = Vec::new();
    cmd.write_dataset_with_ts(&mut body, ts).context("codificando comando")?;
    cmd.put(DataElement::new(tags::COMMAND_GROUP_LENGTH, VR::UL, PrimitiveValue::from(body.len() as u32)));
    let mut out = Vec::with_capacity(body.len() + 12);
    cmd.write_dataset_with_ts(&mut out, ts).context("codificando comando")?;
    Ok(out)
}

pub fn encode_dataset(obj: &InMemDicomObject, ts_uid: &str) -> Result<Vec<u8>> {
    let ts = transfer_syntax(ts_uid)?;
    let mut out = Vec::new();
    obj.write_dataset_with_ts(&mut out, ts).context("codificando dataset")?;
    Ok(out)
}

pub fn decode_dataset(bytes: &[u8], ts_uid: &str) -> Result<InMemDicomObject> {
    let ts = transfer_syntax(ts_uid)?;
    InMemDicomObject::read_dataset_with_ts(bytes, ts).context("lendo dataset")
}

fn implicit_le() -> &'static dicom_encoding::TransferSyntax {
    transfer_syntax(uids::IMPLICIT_VR_LITTLE_ENDIAN).expect("implicit VR LE is always registered")
}

pub fn transfer_syntax(uid: &str) -> Result<&'static dicom_encoding::TransferSyntax> {
    // A static receiver makes the returned entry `'static`.
    static REGISTRY: TransferSyntaxRegistry = TransferSyntaxRegistry;
    let uid = uid.trim_end_matches(['\0', ' ']);
    REGISTRY.get(uid).ok_or_else(|| anyhow!("transfer syntax não suportada: {uid}"))
}

/// A complete DIMSE message.
#[derive(Debug)]
pub struct Message {
    pub pc_id: u8,
    pub command: InMemDicomObject,
    pub data: Option<Vec<u8>>,
}

impl Message {
    pub fn field(&self) -> u16 {
        cmd_u16(&self.command, tags::COMMAND_FIELD).unwrap_or(0)
    }
    pub fn status(&self) -> u16 {
        cmd_u16(&self.command, tags::STATUS).unwrap_or(0xFFFF)
    }
    pub fn message_id(&self) -> u16 {
        cmd_u16(&self.command, tags::MESSAGE_ID).unwrap_or(1)
    }
    pub fn str(&self, tag: Tag) -> String {
        crate::dicom::tagutil::text(&self.command, tag)
    }
}

pub fn cmd_u16(cmd: &InMemDicomObject, tag: Tag) -> Option<u16> {
    cmd.element_opt(tag).ok().flatten()?.to_int::<u16>().ok()
}

/// Minimal interface shared by client and server associations.
pub trait Link {
    fn send_pdu(&mut self, pdu: &Pdu) -> Result<()>;
    fn receive_pdu(&mut self) -> Result<Pdu>;
    fn max_pdu(&self) -> u32;
}

impl Link for dicom_ul::association::client::ClientAssociation<std::net::TcpStream> {
    fn send_pdu(&mut self, pdu: &Pdu) -> Result<()> {
        self.send(pdu).map_err(|e| anyhow!("falha ao enviar: {e}"))
    }
    fn receive_pdu(&mut self) -> Result<Pdu> {
        self.receive().map_err(|e| anyhow!("falha ao receber: {e}"))
    }
    fn max_pdu(&self) -> u32 {
        self.acceptor_max_pdu_length()
    }
}

impl Link for dicom_ul::association::server::ServerAssociation<std::net::TcpStream> {
    fn send_pdu(&mut self, pdu: &Pdu) -> Result<()> {
        self.send(pdu).map_err(|e| anyhow!("falha ao enviar: {e}"))
    }
    fn receive_pdu(&mut self) -> Result<Pdu> {
        self.receive().map_err(|e| anyhow!("falha ao receber: {e}"))
    }
    fn max_pdu(&self) -> u32 {
        self.requestor_max_pdu_length()
    }
}

#[derive(Debug)]
pub enum Incoming {
    Message(Message),
    Released,
}

/// Reassembles DIMSE messages from P-DATA fragments.
#[derive(Default)]
pub struct MessageReader {
    queue: VecDeque<PDataValue>,
}

impl MessageReader {
    pub fn read<L: Link>(&mut self, link: &mut L) -> Result<Incoming> {
        let mut cmd_bytes = Vec::new();
        let mut command: Option<(u8, InMemDicomObject)> = None;
        let mut data = Vec::new();
        loop {
            let pdv = match self.queue.pop_front() {
                Some(p) => p,
                None => {
                    match link.receive_pdu()? {
                        Pdu::PData { data } => self.queue.extend(data),
                        Pdu::ReleaseRQ => {
                            let _ = link.send_pdu(&Pdu::ReleaseRP);
                            return Ok(Incoming::Released);
                        }
                        Pdu::AbortRQ { source } => bail!("associação abortada pelo par: {source:?}"),
                        other => bail!("PDU inesperada: {other:?}"),
                    }
                    continue;
                }
            };
            match pdv.value_type {
                PDataValueType::Command => {
                    cmd_bytes.extend_from_slice(&pdv.data);
                    if pdv.is_last {
                        let ts = implicit_le();
                        let cmd = InMemDicomObject::read_dataset_with_ts(&cmd_bytes[..], ts).context("lendo comando DIMSE")?;
                        let has_data = cmd_u16(&cmd, tags::COMMAND_DATA_SET_TYPE).unwrap_or(NO_DATASET) != NO_DATASET;
                        if !has_data {
                            return Ok(Incoming::Message(Message { pc_id: pdv.presentation_context_id, command: cmd, data: None }));
                        }
                        command = Some((pdv.presentation_context_id, cmd));
                        cmd_bytes.clear();
                    }
                }
                PDataValueType::Data => {
                    data.extend_from_slice(&pdv.data);
                    if pdv.is_last {
                        let (pc_id, cmd) = command.take().ok_or_else(|| anyhow!("dados recebidos antes do comando"))?;
                        return Ok(Incoming::Message(Message { pc_id, command: cmd, data: Some(data) }));
                    }
                }
            }
        }
    }
}

/// Sends a command and an optional encoded data set, fragmenting as needed.
pub fn send_message<L: Link>(link: &mut L, pc_id: u8, command: InMemDicomObject, data: Option<&[u8]>) -> Result<()> {
    let cmd = encode_command(command)?;
    link.send_pdu(&Pdu::PData {
        data: vec![PDataValue { presentation_context_id: pc_id, value_type: PDataValueType::Command, is_last: true, data: cmd }],
    })?;
    if let Some(data) = data {
        // PDV header (6 bytes) + PDU header must fit in the peer's maximum PDU length.
        let peer = match link.max_pdu() {
            0 => 65_536,
            n => n.max(4096),
        };
        let max = peer as usize - 16;
        let chunks: Vec<&[u8]> = if data.is_empty() { vec![&[][..]] } else { data.chunks(max).collect() };
        let n = chunks.len();
        for (i, chunk) in chunks.into_iter().enumerate() {
            link.send_pdu(&Pdu::PData {
                data: vec![PDataValue {
                    presentation_context_id: pc_id,
                    value_type: PDataValueType::Data,
                    is_last: i + 1 == n,
                    data: chunk.to_vec(),
                }],
            })?;
        }
    }
    Ok(())
}

pub fn response(
    field: u16,
    sop_class: &str,
    responding_to: u16,
    status: u16,
    sop_instance: Option<&str>,
) -> InMemDicomObject {
    let mut cmd = InMemDicomObject::new_empty();
    cmd.put(element_str(tags::AFFECTED_SOP_CLASS_UID, VR::UI, sop_class));
    cmd.put(element_u16(tags::COMMAND_FIELD, field));
    cmd.put(element_u16(tags::MESSAGE_ID_BEING_RESPONDED_TO, responding_to));
    cmd.put(element_u16(tags::COMMAND_DATA_SET_TYPE, NO_DATASET));
    cmd.put(element_u16(tags::STATUS, status));
    if let Some(uid) = sop_instance {
        cmd.put(element_str(tags::AFFECTED_SOP_INSTANCE_UID, VR::UI, uid));
    }
    cmd
}

/// Location of a stored instance: `<root>/<StudyUID>/<SeriesUID>/<SOPInstanceUID>.dcm`.
/// Only digits and dots survive, so a hostile UID cannot escape `root`.
pub fn instance_path(root: &Path, study: &str, series: &str, sop_instance: &str) -> PathBuf {
    let safe = |s: &str, fallback: &str| {
        let s: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
        if s.is_empty() || s.chars().all(|c| c == '.') { fallback.to_string() } else { s }
    };
    root.join(safe(study, "sem-estudo")).join(safe(series, "sem-serie")).join(format!("{}.dcm", safe(sop_instance, "instancia")))
}

/// Writes a received C-STORE data set as a Part 10 file without re-encoding it,
/// at [`instance_path`].
pub fn store_instance(root: &Path, sop_class: &str, sop_instance: &str, ts_uid: &str, data: &[u8]) -> Result<PathBuf> {
    let (study, series) = match decode_dataset(data, ts_uid) {
        Ok(obj) => (
            crate::dicom::tagutil::text(&obj, tags::STUDY_INSTANCE_UID),
            crate::dicom::tagutil::text(&obj, tags::SERIES_INSTANCE_UID),
        ),
        Err(_) => (String::new(), String::new()),
    };
    let path = instance_path(root, &study, &series, sop_instance);
    std::fs::create_dir_all(path.parent().unwrap_or(root))?;

    let meta = FileMetaTableBuilder::new()
        .media_storage_sop_class_uid(sop_class.trim_end_matches('\0'))
        .media_storage_sop_instance_uid(sop_instance.trim_end_matches('\0'))
        .transfer_syntax(ts_uid.trim_end_matches('\0'))
        .implementation_class_uid(IMPLEMENTATION_CLASS_UID)
        .implementation_version_name(IMPLEMENTATION_VERSION)
        .build()
        .map_err(|e| anyhow!("meta inválida: {e}"))?;
    let tmp = path.with_extension("part");
    {
        let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        f.write_all(&[0u8; 128])?;
        f.write_all(b"DICM")?;
        meta.write(&mut f).map_err(|e| anyhow!("gravando meta: {e}"))?;
        f.write_all(data)?;
        f.flush()?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Root UID registered for this implementation (UUID-derived, 2.25 arc).
pub const IMPLEMENTATION_CLASS_UID: &str = "2.25.278316741187120468239591738264210391742";
pub const IMPLEMENTATION_VERSION: &str = "SLYCED_100";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_has_correct_group_length_and_roundtrips() {
        let cmd = response(C_ECHO_RSP, uids::VERIFICATION, 7, STATUS_SUCCESS, None);
        let bytes = encode_command(cmd).unwrap();
        // (0000,0000) UL in implicit VR: tag (4) + length (4) + value (4).
        let group_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        assert_eq!(group_len, bytes.len() - 12);
        let ts = transfer_syntax(uids::IMPLICIT_VR_LITTLE_ENDIAN).unwrap();
        let back = InMemDicomObject::read_dataset_with_ts(&bytes[..], ts).unwrap();
        assert_eq!(cmd_u16(&back, tags::COMMAND_FIELD), Some(C_ECHO_RSP));
        assert_eq!(cmd_u16(&back, tags::MESSAGE_ID_BEING_RESPONDED_TO), Some(7));
        assert_eq!(cmd_u16(&back, tags::COMMAND_DATA_SET_TYPE), Some(NO_DATASET));
    }

    #[test]
    fn status_helpers() {
        assert!(is_pending(0xFF00) && is_pending(0xFF01) && !is_pending(0));
        assert!(status_text(0xA801).contains("AE"));
    }
}
