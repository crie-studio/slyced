//! Storage SCP: receives instances pushed by a PACS (C-MOVE destination or
//! manual "send to" from the PACS console). Also answers C-ECHO.

use super::dimse::*;
use anyhow::{Context as _, Result};
use dicom_dictionary_std::{tags, uids};
use dicom_ul::association::server::ServerAssociationOptions;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub enum ScpEvent {
    Stored(PathBuf),
    Error(String),
}

pub struct StorageScp {
    stop: Arc<AtomicBool>,
    pub port: u16,
    pub ae_title: String,
}

impl Drop for StorageScp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl StorageScp {
    pub fn start(
        ae_title: &str,
        port: u16,
        root: PathBuf,
        on_event: impl Fn(ScpEvent) + Send + Sync + 'static,
    ) -> Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port)).with_context(|| trf!("porta {port} indisponível", "port {port} unavailable"))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let on_event = Arc::new(on_event);
        let (stop2, ae) = (stop.clone(), ae_title.to_string());
        std::thread::Builder::new().name("storage-scp".into()).spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        log::info!("SCP: conexão de {peer}");
                        let (ae, root, ev) = (ae.clone(), root.clone(), on_event.clone());
                        std::thread::spawn(move || {
                            if let Err(e) = serve(stream, &ae, &root, &*ev) {
                                ev(ScpEvent::Error(format!("{e:#}")));
                            }
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(80)),
                    Err(e) => {
                        on_event(ScpEvent::Error(e.to_string()));
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
            }
        })?;
        Ok(Self { stop, port, ae_title: ae_title.to_string() })
    }
}

fn serve(stream: TcpStream, ae: &str, root: &Path, on_event: &dyn Fn(ScpEvent)) -> Result<()> {
    stream.set_nonblocking(false)?;
    let mut opts = ServerAssociationOptions::new()
        .accept_any()
        .ae_title(ae.to_string())
        .promiscuous(true)
        .strict(false)
        .max_pdu_length(116_794)
        .read_timeout(Duration::from_secs(120))
        .with_abstract_syntax(uids::VERIFICATION);
    for sc in STORAGE_CLASSES {
        opts = opts.with_abstract_syntax(*sc);
    }
    let mut assoc = opts.establish(stream).context(tr!("negociando associação", "negotiating association"))?;
    let pc_ts: std::collections::HashMap<u8, String> =
        assoc.presentation_contexts().iter().map(|p| (p.id, p.transfer_syntax.clone())).collect();
    let mut reader = MessageReader::default();
    loop {
        match reader.read(&mut assoc)? {
            Incoming::Released => return Ok(()),
            Incoming::Message(m) => match m.field() {
                C_ECHO_RQ => {
                    let rsp = response(C_ECHO_RSP, uids::VERIFICATION, m.message_id(), STATUS_SUCCESS, None);
                    send_message(&mut assoc, m.pc_id, rsp, None)?;
                }
                C_STORE_RQ => {
                    let sop_class = m.str(tags::AFFECTED_SOP_CLASS_UID);
                    let sop_inst = m.str(tags::AFFECTED_SOP_INSTANCE_UID);
                    let status = match (&m.data, pc_ts.get(&m.pc_id)) {
                        (Some(d), Some(ts)) => match store_instance(root, &sop_class, &sop_inst, ts, d) {
                            Ok(p) => {
                                on_event(ScpEvent::Stored(p));
                                STATUS_SUCCESS
                            }
                            Err(e) => {
                                on_event(ScpEvent::Error(format!("{e:#}")));
                                0xA700
                            }
                        },
                        _ => 0xC000,
                    };
                    let rsp = response(C_STORE_RSP, &sop_class, m.message_id(), status, Some(&sop_inst));
                    send_message(&mut assoc, m.pc_id, rsp, None)?;
                }
                other => log::warn!("SCP: comando 0x{other:04X} não suportado"),
            },
        }
    }
}
