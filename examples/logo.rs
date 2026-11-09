//! Writes the Slyced logo as a PNG with transparency, from the same drawing used
//! for the window and executable icons (README header, social preview image):
//!
//!   cargo run --example logo -- docs/img/logo.png 256

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "logo.png".into());
    let size: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(256);
    let rgba = slyced::logo_raster::render_logo(size);
    image::save_buffer(&out, &rgba, size, size, image::ExtendedColorType::Rgba8).expect("falha ao gravar o PNG");
    println!("{out} ({size}×{size})");
}
