# Web version (experimental) and project page: compiles Slyced to WebAssembly and assembles web/dist:
#   web/dist/            project page in Portuguese (web/landing/index.html), screenshots in img/,
#                        sharing images, sitemap.xml, llms.txt, 404.html and the IndexNow key
#   web/dist/en/         project page in English
#   web/dist/app/        the viewer (web/index.html + pkg/)
#   web/dist/sobre/      redirects: the project page lived there for a day
# Runs on Windows (PowerShell 5.1 or 7) and on Linux (pwsh, as in .github/workflows/web.yml).
#
# Requires once:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock> --locked
#   (on Windows ARM64 add --target x86_64-pc-windows-msvc: its TLS library needs clang on ARM64)
# Then serve web/dist with any static server, e.g.:
#   python -m http.server 8765 --directory web/dist
# The sample exam (web/dist/app/exemplo/fantoma.zip, opened by ?exemplo) is made by the workflow.
#
# The public address goes into canonical links, the sitemap and the structured data
# (__SITE__ in the pages). Set SLYCED_SITE_URL when the site moves to its own domain.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
$site = if ($env:SLYCED_SITE_URL) { $env:SLYCED_SITE_URL } else { 'https://crie-studio.github.io/slyced/' }
if (-not $site.EndsWith('/')) { $site += '/' }

cargo rustc --lib --crate-type cdylib --target wasm32-unknown-unknown --profile web
if ($LASTEXITCODE) { throw 'cargo failed' }

$lock = Select-String -Path Cargo.lock -Pattern '^name = "wasm-bindgen"$' -Context 0, 1
$want = $lock.Context.PostContext[0] -replace '.*"(.*)"', '$1'
$have = (wasm-bindgen --version) -replace '^wasm-bindgen\s+', ''
if ($want -ne $have) {
    throw "wasm-bindgen $have is installed but Cargo.lock needs $want (cargo install wasm-bindgen-cli --version $want --locked)"
}

$dist = 'web/dist'
foreach ($d in "$dist/app", "$dist/en", "$dist/img", "$dist/sobre/en") { New-Item -ItemType Directory -Force $d | Out-Null }
$root = (Resolve-Path $dist).Path
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
$date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')
function Fill([string]$from, [string]$to) {
    $text = (Get-Content -Raw -Encoding utf8 $from).Replace('__SITE__', $site).Replace('__VERSION__', $version).Replace('__DATE__', $date)
    [IO.File]::WriteAllText([IO.Path]::Combine($root, $to), $text)   # UTF-8 without BOM
}

# The viewer.
wasm-bindgen --target web --no-typescript --remove-name-section --remove-producers-section `
    --out-dir "$dist/app/pkg" --out-name slyced target/wasm32-unknown-unknown/web/slyced.wasm
if ($LASTEXITCODE) { throw 'wasm-bindgen failed' }
# Undefined C functions become imports from "env" (see .cargo/config.toml), which the page cannot load.
if (Select-String -Path "$dist/app/pkg/slyced.js" -SimpleMatch "from 'env'" -Quiet) {
    throw 'The module imports C functions from "env": add them to mod libc in src/web.rs'
}
Fill web/index.html 'app/index.html'
Copy-Item docs/img/logo.png "$dist/app/favicon.png" -Force

# Project page (Portuguese at the root, English in en/), with shared site.css/site.js.
Fill web/landing/index.html 'index.html'
Fill web/landing/en/index.html 'en/index.html'
Copy-Item web/landing/site.css, web/landing/site.js $dist -Force
Copy-Item web/landing/img/*.webp "$dist/img" -Force
Copy-Item web/landing/og.png, web/landing/og-en.png $dist -Force
Copy-Item docs/img/logo.png "$dist/favicon.png" -Force

# For search engines and AI crawlers: sitemap, llms.txt, 404 page and the public IndexNow key.
Fill web/landing/sitemap.xml 'sitemap.xml'
Fill web/landing/llms.txt 'llms.txt'
Fill web/landing/404.html '404.html'
Copy-Item web/landing/*.txt $dist -Exclude llms.txt -Force
# Verification files from Bing and others (docs/DIVULGACAO.md) go to the root as they are.
if (Test-Path web/landing/public) { Copy-Item web/landing/public/* $dist -Recurse -Force }

# Old addresses of the project page.
Fill web/landing/redirect-sobre.html 'sobre/index.html'
Fill web/landing/redirect-sobre-en.html 'sobre/en/index.html'
"web/dist ready: {0:N1} MB of WebAssembly" -f ((Get-Item "$dist/app/pkg/slyced_bg.wasm").Length / 1MB)
