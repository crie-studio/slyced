// Embeds the icon, version information and application manifest into the
// executable. Proper metadata makes the binary look like what it is to
// Windows SmartScreen and antivirus heuristics.

include!("src/logo_raster.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/logo_raster.rs");
    println!("cargo:rerun-if-changed=assets/slyced.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = out.join("slyced.ico");
    std::fs::write(&ico, encode_ico(&[16, 20, 24, 32, 40, 48, 64, 128, 256])).expect("writing icon");

    let version = env!("CARGO_PKG_VERSION");
    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico.to_str().unwrap())
        .set("ProductName", "Slyced DICOM Viewer")
        .set("FileDescription", "Slyced DICOM Viewer")
        .set("CompanyName", "Luiz Felipe Pitzer")
        .set("LegalCopyright", "Copyright (c) 2026 Luiz Felipe Pitzer - https://lfp.app.br")
        .set("Comments", "Desenvolvido por Luiz Felipe Pitzer - https://lfp.app.br")
        .set("OriginalFilename", "Slyced.exe")
        .set("InternalName", "Slyced")
        .set("ProductVersion", version)
        .set("FileVersion", version)
        .set_manifest_file("assets/slyced.manifest");
    res.compile().expect("compiling Windows resources");
}
