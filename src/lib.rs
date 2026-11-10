//! Slyced: portable DICOM viewer (library crate; the executable lives in `main.rs`).

pub mod app;
pub mod bookmarks;
pub mod config;
pub mod dicom;
pub mod gpu_check;
pub mod imaging;
pub mod jobs;
pub mod logo_raster;
#[cfg(not(target_arch = "wasm32"))]
mod native;
pub mod pacs;
pub mod phases;
pub mod platform;
pub mod protocols;
pub mod render3d;
pub mod store;
pub mod ui;
#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(not(target_arch = "wasm32"))]
pub use native::run;

/// How the window is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Graphics {
    /// DirectX 12 / Vulkan (or WARP software rendering): everything, including 3D.
    Gpu,
    /// OpenGL compatibility mode: 2D and MPR only.
    OpenGl,
}
