//! Where Slyced keeps its data. By default it lives in the Windows user's
//! profile (`%LOCALAPPDATA%\Slyced`), so each user of a shared PC has their own
//! settings, PACS servers and downloads. A `Slyced.portable` file next to the
//! executable switches to portable mode: data in `SlycedData\` beside the
//! executable, to carry everything on a USB stick.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "Slyced";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHOR: &str = "Luiz Felipe Pitzer";
pub const WEBSITE: &str = env!("CARGO_PKG_HOMEPAGE");
/// Project page (features, screens, downloads); the web version is at app/ below it.
pub const PROJECT_PAGE: &str = "https://crie-studio.github.io/slyced/";
/// Window title: always the same, never the patient's name.
pub const WINDOW_TITLE: &str = "Slyced - por Luiz Felipe Pitzer";
/// Marker file that enables portable mode.
pub const PORTABLE_MARKER: &str = "Slyced.portable";
/// Data folder next to the executable in portable mode.
const PORTABLE_DIR: &str = "SlycedData";
/// Former name of the program (up to version 0.6).
const OLD_NAME: &str = "Lumen";

#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub store: PathBuf,
    pub log: PathBuf,
    pub portable: bool,
    pub exe_dir: PathBuf,
    /// Message for the user about how the data folder was chosen (shown once at startup).
    pub notice: Option<String>,
}

impl Paths {
    /// The web version has no disk: nothing is written, settings last for the session.
    #[cfg(target_arch = "wasm32")]
    pub fn resolve() -> Self {
        Self::resolve_in(Path::new("/"), PathBuf::from("/slyced"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn resolve() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let user_dir = user_data_dir();
        let adopted = adopt_old_data(&exe_dir, &user_dir.with_file_name(OLD_NAME), &user_dir);
        let mut paths = Self::resolve_in(&exe_dir, user_dir);
        paths.notice = adopted.or(paths.notice);
        paths
    }

    pub fn resolve_in(exe_dir: &Path, user_dir: PathBuf) -> Self {
        let portable_dir = exe_dir.join(PORTABLE_DIR);
        let wants_portable = exe_dir.join(PORTABLE_MARKER).exists();
        let mut notice = None;
        let (root, portable) = if wants_portable && is_writable(&portable_dir) {
            (portable_dir.clone(), true)
        } else {
            if wants_portable {
                notice = Some("Modo portátil indisponível: a pasta do executável é somente leitura. Usando a pasta do usuário.".to_string());
            }
            (user_dir, false)
        };
        let _ = std::fs::create_dir_all(&root);
        let mut paths = Self {
            config: root.join("config.json"),
            store: root.join("PACS"),
            log: root.join("slyced.log"),
            root,
            portable,
            exe_dir: exe_dir.to_path_buf(),
            notice,
        };
        if !portable {
            let legacy = exe_dir.join(format!("{OLD_NAME}Data"));
            paths.notice = paths.migrate_from(&portable_dir).or_else(|| paths.migrate_from(&legacy)).or(paths.notice);
        }
        paths
    }

    /// One-time import of the settings that earlier versions kept next to the executable.
    fn migrate_from(&self, old: &Path) -> Option<String> {
        let old_config = old.join("config.json");
        if self.config.exists() || !old_config.exists() {
            return None;
        }
        std::fs::copy(&old_config, &self.config).ok()?;
        let mut msg = format!("Configurações importadas de {}.", old.display());
        if old.join("PACS").is_dir() {
            msg.push_str(&format!(" Exames baixados anteriormente continuam em {}.", old.join("PACS").display()));
        }
        Some(msg)
    }

    /// Whether portable mode is requested (it applies from the next start).
    pub fn portable_requested(&self) -> bool {
        self.exe_dir.join(PORTABLE_MARKER).exists()
    }

    pub fn set_portable(&self, on: bool) -> std::io::Result<()> {
        let marker = self.exe_dir.join(PORTABLE_MARKER);
        if on {
            std::fs::write(marker, "Apague este arquivo para o Slyced voltar a salvar os dados na pasta do usuário.\r\n")
        } else {
            match std::fs::remove_file(marker) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        }
    }
}

/// Brings the data of the program's former name (Lumen) to Slyced: the user
/// folder and, in portable mode, the marker and data folder beside the executable.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn adopt_old_data(exe_dir: &Path, old_user: &Path, new_user: &Path) -> Option<String> {
    let mut adopted = false;
    let (old_marker, new_marker) = (exe_dir.join(format!("{OLD_NAME}.portable")), exe_dir.join(PORTABLE_MARKER));
    if old_marker.exists() && !new_marker.exists() {
        adopted |= std::fs::rename(&old_marker, &new_marker).is_ok();
    }
    let (old_portable, new_portable) = (exe_dir.join(format!("{OLD_NAME}Data")), exe_dir.join(PORTABLE_DIR));
    if new_marker.exists() && old_portable.is_dir() && !new_portable.exists() {
        adopted |= std::fs::rename(&old_portable, &new_portable).is_ok();
    }
    if old_user.is_dir() && !new_user.exists() {
        if std::fs::rename(old_user, new_user).is_ok() {
            adopted = true;
        } else {
            // Folder in use or not movable: bring at least settings and findings.
            let _ = std::fs::create_dir_all(new_user);
            for f in ["config.json", "achados.json"] {
                adopted |= std::fs::copy(old_user.join(f), new_user.join(f)).is_ok();
            }
        }
    }
    adopted.then(|| "O Lumen agora se chama Slyced: configurações, servidores PACS e achados foram mantidos.".to_string())
}

/// `%LOCALAPPDATA%\Slyced`: per user and not roaming, so large PACS downloads
/// stay on this computer.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn user_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData").join("Local")))
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_NAME)
}

fn is_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-test");
    let ok = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RetrieveMethod {
    /// C-GET: images come back on the same association (no inbound port needed).
    #[default]
    CGet,
    /// C-MOVE: the PACS opens a new association to our Storage SCP.
    CMove,
}

impl RetrieveMethod {
    pub fn label(self) -> &'static str {
        match self {
            RetrieveMethod::CGet => "C-GET",
            RetrieveMethod::CMove => "C-MOVE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PacsProtocol {
    /// Classic DICOM network (C-FIND / C-GET / C-MOVE) to AE title, host and port.
    #[default]
    Dimse,
    /// HTTP(S): QIDO-RS for queries and WADO-RS for downloads, from `base_url`.
    DicomWeb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WebAuth {
    #[default]
    None,
    /// HTTP Basic with `username` / `secret`.
    Basic,
    /// `Authorization: Bearer <secret>`.
    Bearer,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PacsNode {
    pub name: String,
    pub ae_title: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub retrieve: RetrieveMethod,
    #[serde(default)]
    pub protocol: PacsProtocol,
    /// DICOMweb root, e.g. `http://servidor:8042/dicom-web`.
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub auth: WebAuth,
    #[serde(default)]
    pub username: String,
    /// Password (Basic) or token (Bearer). Stored as plain text in `config.json`.
    #[serde(default)]
    pub secret: String,
}

impl PacsNode {
    pub fn badge(&self) -> &'static str {
        match self.protocol {
            PacsProtocol::Dimse => self.retrieve.label(),
            PacsProtocol::DicomWeb => "DICOMweb",
        }
    }

    /// One-line address shown in the server list.
    pub fn address(&self) -> String {
        match self.protocol {
            PacsProtocol::Dimse => format!("{} @ {}:{}", self.ae_title, self.host, self.port),
            PacsProtocol::DicomWeb => self.base_url.clone(),
        }
    }
}

impl Default for PacsNode {
    fn default() -> Self {
        Self {
            name: "Novo servidor".into(),
            ae_title: "ORTHANC".into(),
            host: "127.0.0.1".into(),
            port: 4242,
            retrieve: RetrieveMethod::CGet,
            protocol: PacsProtocol::Dimse,
            base_url: "http://127.0.0.1:8042/dicom-web".into(),
            auth: WebAuth::None,
            username: String::new(),
            secret: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub local_ae: String,
    pub local_port: u16,
    /// Start the Storage SCP at launch (needed to receive C-MOVE / pushes).
    pub scp_autostart: bool,
    pub nodes: Vec<PacsNode>,
    pub recent: Vec<PathBuf>,
    pub smooth: bool,
    pub overlays: bool,
    /// Samples per voxel along each ray in the 3D view.
    pub vr_quality: f32,
    pub memory_budget_mb: usize,
    pub theme: Theme,
    /// Size of the whole interface (1.0 = 100%).
    pub ui_scale: f32,
    /// The welcome tour was shown (or skipped).
    pub tour_done: bool,
    /// Version whose news the user already saw.
    pub seen_version: String,
    /// Exams downloaded from the PACS are removed after this many days (0 = never).
    pub store_keep_days: u32,
    /// Remove every downloaded exam when the program closes.
    pub store_clear_on_exit: bool,
    /// Small front view of the volume on axial images.
    pub localizer: bool,
    /// Strip of the study's series under the images.
    pub filmstrip: bool,
    /// Light of the 3D view (VR and surface).
    pub vr_lighting: Lighting,
    /// Text over the images in the reading full screen (F11).
    pub reading_info: InfoLevel,
    /// The patient timeline also asks the PACS for the patient's other exams.
    pub timeline_pacs: bool,
}

/// How much text goes over the images in the reading full screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InfoLevel {
    /// Patient, series, slice, window and scale, as in the normal screen.
    Full,
    /// Plane, slice and window only.
    #[default]
    Minimal,
    /// Orientation letters only.
    None,
}

impl InfoLevel {
    pub const ALL: [InfoLevel; 3] = [InfoLevel::Full, InfoLevel::Minimal, InfoLevel::None];

    pub fn label(self) -> &'static str {
        match self {
            InfoLevel::Full => "Completa",
            InfoLevel::Minimal => "Mínima",
            InfoLevel::None => "Nenhuma",
        }
    }

    pub fn next(self) -> Self {
        match self {
            InfoLevel::Full => InfoLevel::Minimal,
            InfoLevel::Minimal => InfoLevel::None,
            InfoLevel::None => InfoLevel::Full,
        }
    }
}

/// How the 3D view is lit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lighting {
    /// Light from the camera, as before 1.3: the fastest.
    Simple,
    /// Key light with shadows, ambient occlusion and tone mapping, in real time.
    #[default]
    Realistic,
    /// The realistic light refined over many frames once the volume stops moving.
    Cinematic,
}

impl Lighting {
    pub const ALL: [Lighting; 3] = [Lighting::Simple, Lighting::Realistic, Lighting::Cinematic];

    pub fn label(self) -> &'static str {
        match self {
            Lighting::Simple => "Simples",
            Lighting::Realistic => "Realista",
            Lighting::Cinematic => "Cinemático",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Lighting::Simple => "Luz vinda da câmera, sem sombras: a mais leve para a placa de vídeo.",
            Lighting::Realistic => "Luz lateral com sombras suaves, oclusão nas reentrâncias e contorno, em tempo real.",
            Lighting::Cinematic => "Parado, a imagem é refinada por alguns segundos (sombras e luz ambiente mais naturais); girando, usa a luz realista.",
        }
    }

    pub fn code(self) -> f32 {
        match self {
            Lighting::Simple => 0.0,
            Lighting::Realistic => 1.0,
            Lighting::Cinematic => 2.0,
        }
    }
}

/// Colors of the interface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    Dark,
    Light,
    HighContrast,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Dark, Theme::Light, Theme::HighContrast];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Escuro",
            Theme::Light => "Claro",
            Theme::HighContrast => "Alto contraste",
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            local_ae: "SLYCED".into(),
            local_port: 11112,
            scp_autostart: false,
            nodes: Vec::new(),
            recent: Vec::new(),
            smooth: true,
            overlays: true,
            vr_quality: 1.0,
            memory_budget_mb: 6144,
            theme: Theme::Dark,
            ui_scale: 1.0,
            tour_done: false,
            seen_version: String::new(),
            store_keep_days: 30,
            store_clear_on_exit: false,
            localizer: true,
            filmstrip: true,
            vr_lighting: Lighting::Realistic,
            reading_info: InfoLevel::Minimal,
            timeline_pacs: true,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    /// Removes what still carries the former name: the AE Title "LUMEN" becomes
    /// "SLYCED" and old program packages (`Lumen-….zip`) leave the recent list.
    /// Returns a notice when the AE Title changed.
    pub fn drop_old_name(&mut self) -> Option<String> {
        let old_package = |p: &PathBuf| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            name.starts_with(&format!("{}-", OLD_NAME.to_lowercase())) && name.ends_with(".zip")
        };
        self.recent.retain(|p| !old_package(p));
        if !self.local_ae.trim().eq_ignore_ascii_case(OLD_NAME) {
            return None;
        }
        self.local_ae = APP_NAME.to_uppercase();
        Some(format!(
            "O AE Title deste computador passou de {} para {}. Se o PACS só aceita AEs cadastrados, cadastre {} nele (ou volte o nome em Configurações → Rede DICOM).",
            OLD_NAME.to_uppercase(),
            self.local_ae,
            self.local_ae
        ))
    }

    pub fn save(&self, path: &Path) {
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    pub fn push_recent(&mut self, p: PathBuf) {
        self.recent.retain(|r| r != &p);
        self.recent.insert(0, p);
        self.recent.truncate(10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_saved_before_dicomweb_still_load() {
        let old = r#"{"local_ae":"SLYCED","nodes":[{"name":"PACS","ae_title":"ORTHANC","host":"10.0.0.5","port":4242,"retrieve":"CMove"}]}"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        let n = &s.nodes[0];
        assert_eq!((n.protocol, n.retrieve, n.auth), (PacsProtocol::Dimse, RetrieveMethod::CMove, WebAuth::None));
        assert_eq!(n.address(), "ORTHANC @ 10.0.0.5:4242");

        let web = PacsNode { protocol: PacsProtocol::DicomWeb, auth: WebAuth::Bearer, secret: "t".into(), ..Default::default() };
        let back: PacsNode = serde_json::from_str(&serde_json::to_string(&web).unwrap()).unwrap();
        assert_eq!(back, web);
        assert_eq!(back.badge(), "DICOMweb");
    }

    #[test]
    fn data_goes_to_user_folder_unless_portable() {
        let base = std::env::temp_dir().join(format!("slyced-paths-{}", std::process::id()));
        let exe = base.join("app");
        std::fs::create_dir_all(&exe).unwrap();

        let p = Paths::resolve_in(&exe, base.join("user"));
        assert!(!p.portable);
        assert_eq!(p.root, base.join("user"));
        assert!(!exe.join("SlycedData").exists(), "nothing is written next to the executable");
        assert!(p.notice.is_none());

        // Settings left next to the executable by an earlier version are imported once.
        std::fs::create_dir_all(exe.join("SlycedData")).unwrap();
        std::fs::write(exe.join("SlycedData").join("config.json"), r#"{"local_ae":"ANTIGO"}"#).unwrap();
        let p = Paths::resolve_in(&exe, base.join("user2"));
        assert_eq!(Settings::load(&p.config).local_ae, "ANTIGO");
        assert!(p.notice.is_some());
        assert!(Paths::resolve_in(&exe, base.join("user2")).notice.is_none());

        p.set_portable(true).unwrap();
        let portable = Paths::resolve_in(&exe, base.join("user"));
        assert!(portable.portable && portable.portable_requested());
        assert_eq!(portable.root, exe.join("SlycedData"));
        portable.set_portable(false).unwrap();
        assert!(!Paths::resolve_in(&exe, base.join("user")).portable);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn data_of_the_old_name_is_adopted() {
        let base = std::env::temp_dir().join(format!("slyced-rename-{}", std::process::id()));
        let exe = base.join("app");
        let (old, new) = (base.join("Lumen"), base.join("Slyced"));
        std::fs::create_dir_all(&exe).unwrap();
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("config.json"), r#"{"local_ae":"HOSPITAL"}"#).unwrap();
        // Portable install of the old version.
        std::fs::write(exe.join("Lumen.portable"), "").unwrap();
        std::fs::create_dir_all(exe.join("LumenData")).unwrap();
        std::fs::write(exe.join("LumenData").join("achados.json"), "[]").unwrap();

        assert!(adopt_old_data(&exe, &old, &new).is_some());
        assert_eq!(Settings::load(&new.join("config.json")).local_ae, "HOSPITAL");
        assert!(!old.exists());
        assert!(exe.join(PORTABLE_MARKER).exists() && exe.join("SlycedData").join("achados.json").exists());
        // Second start: nothing left to adopt.
        assert!(adopt_old_data(&exe, &old, &new).is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn old_name_leaves_settings() {
        let mut s = Settings { local_ae: "lumen".into(), ..Default::default() };
        s.recent = vec![PathBuf::from(r"C:\Downloads\Lumen-0.2.0-x64.zip"), PathBuf::from(r"C:\Exames\TC.zip"), PathBuf::from(r"C:\Exames\Lumen")];
        assert!(s.drop_old_name().is_some());
        assert_eq!(s.local_ae, "SLYCED");
        assert_eq!(s.recent, vec![PathBuf::from(r"C:\Exames\TC.zip"), PathBuf::from(r"C:\Exames\Lumen")]);
        // An AE Title chosen by the user stays.
        let mut s = Settings { local_ae: "HOSPITAL".into(), ..Default::default() };
        assert!(s.drop_old_name().is_none());
        assert_eq!(s.local_ae, "HOSPITAL");
    }
}
