//! Web version: start-up in the page and the bridge to `web/index.html`, which
//! reads the files the user chooses or drops (folders included) and hands them
//! over here. Nothing leaves the browser: files are kept in memory only.

use crate::app::{Automation, SlycedApp};
use crate::config::Paths;
use crate::dicom::memfs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use wasm_bindgen::prelude::*;

static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static CTX: OnceLock<egui::Context> = OnceLock::new();

#[wasm_bindgen]
extern "C" {
    /// Opens the browser's picker for a folder (`true`) or for files.
    #[wasm_bindgen(js_namespace = slyced, js_name = pick)]
    fn js_pick(folder: bool);
    /// Offers `data` to the user as a download named `name`.
    #[wasm_bindgen(js_namespace = slyced, js_name = download)]
    fn js_download(name: &str, data: &[u8]);
    /// Replaces the loading message with an error.
    #[wasm_bindgen(js_namespace = slyced, js_name = failed)]
    fn js_failed(message: &str);
    /// Removes the loading message once the viewer is running.
    #[wasm_bindgen(js_namespace = slyced, js_name = started)]
    fn js_started();
    /// Language chosen for this visit: `?lang=` in the address, else the one the
    /// page remembers, else "auto".
    #[wasm_bindgen(js_namespace = slyced, js_name = language)]
    fn js_language() -> String;
    /// The browser's language (`navigator.language`).
    #[wasm_bindgen(js_namespace = slyced, js_name = browserLanguage)]
    fn js_browser_language() -> String;
    /// Remembers the language chosen in Configurações for the next visits.
    #[wasm_bindgen(js_namespace = slyced, js_name = rememberLanguage)]
    fn js_remember_language(code: &str);
}

/// Language chosen on the page (or automatic).
pub fn chosen_language() -> crate::config::Language {
    crate::config::Language::from_code(&js_language())
}

pub fn browser_language() -> String {
    js_browser_language()
}

pub fn remember_language(language: crate::config::Language) {
    js_remember_language(language.code());
}

pub fn pick(folder: bool) {
    js_pick(folder);
}

pub fn download(name: &str, data: &[u8]) {
    js_download(name, data);
}

/// A new virtual folder for a batch of files (called by the page).
#[wasm_bindgen]
pub fn slyced_begin() -> String {
    memfs::new_folder()
}

/// One file of the batch, with its path relative to what the user chose.
#[wasm_bindgen]
pub fn slyced_add_file(folder: &str, relative: &str, data: Vec<u8>) {
    memfs::insert(folder, relative, data.into());
}

/// The batch is complete: the viewer scans and opens it on its next frame.
#[wasm_bindgen]
pub fn slyced_open(folder: &str) {
    if let Ok(mut opened) = OPENED.lock() {
        opened.push(PathBuf::from(folder));
    }
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

/// Batches handed over since the last frame.
pub fn take_opened() -> Vec<PathBuf> {
    OPENED.lock().map(|mut o| std::mem::take(&mut *o)).unwrap_or_default()
}

/// Runs `f` after the browser has drawn the current frame (there are no
/// worker threads here, so long jobs block the page while they run).
pub fn defer(f: impl FnOnce() + 'static) {
    let callback = Closure::once_into_js(f);
    if let Some(window) = web_sys::window() {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 0);
    }
}

/// The few C library functions that openjp2 (JPEG 2000, ported from C) calls.
/// The browser target has no C library: memory blocks come from Rust's
/// allocator with their size stored just before them. `build-web.ps1` fails if
/// the module ever needs another one.
mod libc {
    use std::alloc::{alloc, alloc_zeroed, dealloc, realloc as grow, Layout};
    use std::ptr::null_mut;

    /// Room for the size, keeping the 16-byte alignment C expects.
    const HEADER: usize = 16;

    fn layout(size: usize) -> Option<Layout> {
        Layout::from_size_align(size.checked_add(HEADER)?, HEADER).ok()
    }

    unsafe fn finish(base: *mut u8, size: usize) -> *mut u8 {
        if base.is_null() {
            return null_mut();
        }
        unsafe {
            base.cast::<usize>().write(size);
            base.add(HEADER)
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
        let Some(l) = layout(size) else { return null_mut() };
        unsafe { finish(alloc(l), size) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn calloc(count: usize, size: usize) -> *mut u8 {
        let Some(total) = count.checked_mul(size) else { return null_mut() };
        let Some(l) = layout(total) else { return null_mut() };
        unsafe { finish(alloc_zeroed(l), total) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn free(ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        unsafe {
            let base = ptr.sub(HEADER);
            let size = base.cast::<usize>().read();
            dealloc(base, Layout::from_size_align_unchecked(size + HEADER, HEADER));
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn realloc(ptr: *mut u8, size: usize) -> *mut u8 {
        if ptr.is_null() {
            return unsafe { malloc(size) };
        }
        let Some(new) = size.checked_add(HEADER) else { return null_mut() };
        unsafe {
            let base = ptr.sub(HEADER);
            let old = base.cast::<usize>().read();
            finish(grow(base, Layout::from_size_align_unchecked(old + HEADER, HEADER), new), size)
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn strcpy(dst: *mut u8, src: *const u8) -> *mut u8 {
        let mut i = 0;
        loop {
            unsafe {
                let c = src.add(i).read();
                dst.add(i).write(c);
                if c == 0 {
                    return dst;
                }
            }
            i += 1;
        }
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    let _ = eframe::WebLogger::init(log::LevelFilter::Info);
    log::info!("Slyced {} (web)", crate::config::APP_VERSION);
    crate::i18n::set(chosen_language().resolve());
    wasm_bindgen_futures::spawn_local(async {
        let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("slyced"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            js_failed(tr!("Página sem o elemento <canvas id=\"slyced\">.", "The page has no <canvas id=\"slyced\"> element."));
            return;
        };
        let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
            setup.device_descriptor = std::sync::Arc::new(crate::gpu_check::device_descriptor);
        }
        let options = eframe::WebOptions { renderer: eframe::Renderer::Wgpu, wgpu_options, ..Default::default() };
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(|cc| {
                    let _ = CTX.set(cc.egui_ctx.clone());
                    Ok(Box::new(SlycedApp::new(cc, Paths::resolve(), Vec::new(), Automation::default(), crate::Graphics::Gpu)))
                }),
            )
            .await;
        match result {
            Ok(()) => js_started(),
            Err(e) => js_failed(&trf!("Não foi possível iniciar o Slyced neste navegador: {e:?}", "Slyced could not start in this browser: {e:?}")),
        }
    });
}
