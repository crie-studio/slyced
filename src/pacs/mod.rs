#[cfg(not(target_arch = "wasm32"))]
pub mod dicomweb;
#[cfg(target_arch = "wasm32")]
#[path = "dicomweb_web.rs"]
pub mod dicomweb;
pub mod dimse;
pub mod scp;
pub mod scu;
