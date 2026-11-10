# Web version (experimental): compiles Slyced to WebAssembly and assembles web/dist.
# Runs on Windows (PowerShell 5.1 or 7) and on Linux (pwsh, as in .github/workflows/web.yml).
#
# Requires once:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock> --locked
#   (on Windows ARM64 add --target x86_64-pc-windows-msvc: its TLS library needs clang on ARM64)
# Then serve web/dist with any static server, e.g.:
#   python -m http.server 8765 --directory web/dist
# The sample exam (web/dist/exemplo/fantoma.zip, opened by ?exemplo) is made by the workflow.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

cargo rustc --lib --crate-type cdylib --target wasm32-unknown-unknown --profile web
if ($LASTEXITCODE) { throw 'cargo failed' }

$lock = Select-String -Path Cargo.lock -Pattern '^name = "wasm-bindgen"$' -Context 0, 1
$want = $lock.Context.PostContext[0] -replace '.*"(.*)"', '$1'
$have = (wasm-bindgen --version) -replace '^wasm-bindgen\s+', ''
if ($want -ne $have) {
    throw "wasm-bindgen $have is installed but Cargo.lock needs $want (cargo install wasm-bindgen-cli --version $want --locked)"
}

$dist = 'web/dist'
wasm-bindgen --target web --no-typescript --remove-name-section --remove-producers-section `
    --out-dir "$dist/pkg" --out-name slyced target/wasm32-unknown-unknown/web/slyced.wasm
if ($LASTEXITCODE) { throw 'wasm-bindgen failed' }
# Undefined C functions become imports from "env" (see .cargo/config.toml), which the page cannot load.
if (Select-String -Path "$dist/pkg/slyced.js" -SimpleMatch "from 'env'" -Quiet) {
    throw 'The module imports C functions from "env": add them to mod libc in src/web.rs'
}
Copy-Item web/index.html $dist -Force
Copy-Item docs/img/logo.png "$dist/favicon.png" -Force
"web/dist ready: {0:N1} MB of WebAssembly" -f ((Get-Item "$dist/pkg/slyced_bg.wasm").Length / 1MB)
