//! Windows start-up: log file, diagnostics, GPU choice and the window.

use crate::{config, gpu_check, logo_raster, platform, Graphics};
use crate::app::{Automation, Layout, SlycedApp, Workspace};
use crate::config::Paths;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Minimal file logger: `slyced.log` in the data folder, rotated at 2 MB.
struct FileLogger(Mutex<std::fs::File>);

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        if m.target().starts_with("slyced") || m.target().starts_with("Slyced") {
            m.level() <= log::Level::Info
        } else {
            m.level() <= log::Level::Warn
        }
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if let Ok(mut f) = self.0.lock() {
            let _ = writeln!(f, "{secs} {:5} [{}] {}", r.level(), r.target(), r.args());
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.0.lock() {
            let _ = f.flush();
        }
    }
}


static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

fn log_path_text() -> String {
    LOG_PATH.get().map(|p| p.display().to_string()).unwrap_or_else(|| "%LOCALAPPDATA%\\Slyced\\slyced.log".into())
}

/// eframe's adapter choice, using the same ranking as the startup probe.
/// `WGPU_BACKEND` still overrides it.
fn pick_adapter(
    adapters: &[eframe::wgpu::Adapter],
    surface: Option<&eframe::wgpu::Surface<'_>>,
) -> Result<eframe::wgpu::Adapter, String> {
    adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .max_by_key(|a| gpu_check::score(&a.get_info()))
        .cloned()
        .ok_or_else(|| tr!("nenhuma GPU compatível encontrada", "no compatible GPU found").to_string())
}

fn init_logging(path: &std::path::Path) {
    if std::fs::metadata(path).map(|m| m.len() > 2 * 1024 * 1024).unwrap_or(false) {
        let _ = std::fs::rename(path, path.with_extension("old.log"));
    }
    if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let logger = Box::leak(Box::new(FileLogger(Mutex::new(f))));
        let _ = log::set_logger(logger);
        log::set_max_level(log::LevelFilter::Info);
    }
    let _ = LOG_PATH.set(path.to_path_buf());
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
        log::logger().flush();
        // Worker threads catch their panics (e.g. a corrupt file); only the UI thread is fatal.
        if std::thread::current().name() == Some("main") {
            platform::message_box(
                "Slyced",
                &trf!(
                    "O Slyced encontrou um erro e precisa fechar.\n\n{info}\n\nDetalhes no log:\n{}",
                    "Slyced ran into an error and needs to close.\n\n{info}\n\nDetails in the log:\n{}",
                    log_path_text()
                ),
                true,
            );
        }
    }));
}

/// `--diagnostico`: system and graphics report, without opening the UI.
fn diagnostics(paths: &Paths) {
    let reports = gpu_check::probe(true);
    let mut lines = vec![
        format!("Slyced {}", config::APP_VERSION),
        trf!("Sistema: {}", "System: {}", platform::os_description()),
        trf!(
            "Sessão remota (RDP/Citrix): {}",
            "Remote session (RDP/Citrix): {}",
            if platform::is_remote_session() { tr!("sim", "yes") } else { tr!("não", "no") }
        ),
        trf!("Pasta de dados: {}", "Data folder: {}", paths.root.display()),
        String::new(),
        tr!("Placas de vídeo (melhor primeiro):", "Graphics cards (best first):").to_string(),
    ];
    if reports.is_empty() {
        lines.push(tr!("  nenhuma encontrada", "  none found").into());
    }
    lines.extend(reports.iter().map(|r| format!("  • {}", r.line())));
    lines.push(String::new());
    lines.push(match gpu_check::usable(&reports) {
        Some(r) => trf!(
            "Resultado: o Slyced vai usar {} ({:?}) com 2D, MPR e 3D.",
            "Result: Slyced will use {} ({:?}) with 2D, MPR and 3D.",
            r.info.name,
            r.info.backend
        ),
        None => tr!(
            "Resultado: sem DirectX 12/Vulkan utilizável; o Slyced abre em modo OpenGL (2D e MPR, sem 3D).",
            "Result: no usable DirectX 12/Vulkan; Slyced opens in OpenGL mode (2D and MPR, no 3D)."
        )
        .into(),
    });
    let report = lines.join("\n");
    log::info!("diagnóstico:\n{report}");
    let file = paths.root.join("diagnostico.txt");
    let saved = match std::fs::write(&file, &report) {
        Ok(()) => trf!("Relatório salvo em:\n{}", "Report saved to:\n{}", file.display()),
        Err(e) => trf!(
            "Não foi possível salvar o relatório ({e}): a pasta de dados não aceita gravação.",
            "Could not save the report ({e}): the data folder is not writable."
        ),
    };
    platform::message_box(
        tr!("Slyced — diagnóstico", "Slyced — diagnostics"),
        &trf!("{report}\n\n{saved}\n\n(Ctrl+C copia o texto desta janela.)", "{report}\n\n{saved}\n\n(Ctrl+C copies the text of this window.)"),
        false,
    );
}

/// Restarts the program in OpenGL mode after the GPU path failed to open a window.
fn relaunch_with_opengl() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    std::process::Command::new(exe).args(std::env::args().skip(1)).arg("--opengl").spawn().is_ok()
}

/// Runs the viewer; returns when the window is closed.
pub fn run() {
    // Language first: notices and message boxes below already need it. The
    // system's until the settings are found, then the one chosen in them.
    crate::i18n::set(crate::i18n::system());
    let paths = Paths::resolve();
    crate::i18n::set(config::Settings::load(&paths.config).language.resolve());
    init_logging(&paths.log);
    log::info!("Slyced {} iniciado · {} · dados em {}", config::APP_VERSION, platform::os_description(), paths.root.display());
    if platform::is_remote_session() {
        log::info!("sessão remota (RDP/Citrix) detectada");
    }

    let mut open = Vec::new();
    let mut automation =
        Automation {
            screenshot: None,
            workspace: None,
            layout: None,
            pacs_query: false,
            about: false,
            zoom: None,
            actions: Vec::new(),
            requested: false,
            settle_since: 0.0,
        };
    let mut size = [1440.0f32, 900.0];
    let mut force_opengl = std::env::var_os("SLYCED_OPENGL").is_some();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--diagnostico" => {
                diagnostics(&paths);
                return;
            }
            "--opengl" => force_opengl = true,
            "--screenshot" => automation.screenshot = args.next().map(PathBuf::from),
            "--pacs" => automation.pacs_query = true,
            "--about" => automation.about = true,
            "--zoom" => automation.zoom = args.next().and_then(|z| z.parse().ok()),
            "--do" => automation.actions.extend(args.next()),
            // Interface language for this run (screenshots): pt, en or auto.
            "--lang" => {
                if let Some(code) = args.next() {
                    crate::i18n::set(config::Language::from_code(&code).resolve());
                    automation.actions.insert(0, format!("lang:{code}"));
                }
            }
            "--workspace" => {
                automation.workspace = match args.next().as_deref() {
                    Some("mpr") => Some(Workspace::Mpr),
                    Some("3d") => Some(Workspace::Volume),
                    Some("compare") | Some("comparar") => Some(Workspace::Compare),
                    _ => Some(Workspace::Viewer),
                }
            }
            "--layout" => {
                automation.layout = match args.next().as_deref() {
                    Some("2") => Some(Layout::Cols2),
                    Some("4") => Some(Layout::Grid4),
                    _ => Some(Layout::One),
                }
            }
            "--size" => {
                if let Some((w, h)) = args.next().and_then(|s| s.split_once('x').map(|(w, h)| (w.parse().ok(), h.parse().ok())))
                    && let (Some(w), Some(h)) = (w, h) {
                        size = [w, h];
                    }
            }
            other => open.push(PathBuf::from(other)),
        }
    }

    // Decide before any window exists: a broken driver must lead to a fallback, not a crash.
    let graphics = if force_opengl {
        log::info!("modo OpenGL solicitado");
        Graphics::OpenGl
    } else {
        let reports = gpu_check::probe(false);
        for r in &reports {
            log::info!("GPU: {}", r.line());
        }
        match gpu_check::usable(&reports) {
            Some(r) => {
                log::info!("renderização: {} ({:?})", r.info.name, r.info.backend);
                Graphics::Gpu
            }
            None => {
                log::warn!("nenhuma GPU DirectX 12/Vulkan utilizável: modo OpenGL (sem 3D)");
                Graphics::OpenGl
            }
        }
    };

    let icon = egui::IconData { rgba: logo_raster::render_logo(64), width: 64, height: 64 };
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
        if std::env::var_os("WGPU_BACKEND").is_none() {
            setup.native_adapter_selector = Some(std::sync::Arc::new(pick_adapter));
        }
        setup.device_descriptor = std::sync::Arc::new(gpu_check::device_descriptor);
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(config::window_title())
            .with_app_id("slyced-dicom")
            .with_inner_size(size)
            .with_min_inner_size([960.0, 600.0])
            .with_icon(icon)
            .with_drag_and_drop(true)
            .with_maximized(automation.screenshot.is_none()),
        renderer: match graphics {
            Graphics::Gpu => eframe::Renderer::Wgpu,
            Graphics::OpenGl => eframe::Renderer::Glow,
        },
        wgpu_options,
        centered: true,
        ..Default::default()
    };
    let result = eframe::run_native(
        "Slyced",
        options,
        Box::new(move |cc| Ok(Box::new(SlycedApp::new(cc, paths, open, automation, graphics)))),
    );
    if let Err(e) = result {
        log::error!("falha ao abrir a janela ({graphics:?}): {e}");
        log::logger().flush();
        if graphics == Graphics::Gpu && relaunch_with_opengl() {
            log::info!("reiniciando em modo OpenGL");
            return;
        }
        platform::message_box(
            "Slyced",
            &trf!(
                "Não foi possível abrir a janela do Slyced.\n\nDetalhes: {e}\n\n\
                 Atualize o driver de vídeo. Para um relatório do sistema, execute \"Slyced - diagnostico.cmd\".\n\n\
                 Log: {}",
                "Could not open the Slyced window.\n\nDetails: {e}\n\n\
                 Update the graphics driver. For a system report, run \"Slyced - diagnostico.cmd\".\n\n\
                 Log: {}",
                log_path_text()
            ),
            true,
        );
    }
}
