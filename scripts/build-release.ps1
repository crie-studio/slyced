# Builds the portable executables for x64 and ARM64 (Windows 10/11) and for
# Windows 7 (64 and 32 bits), and packages them in dist\.
#
# The Windows 7 targets are tier 3 in Rust: they need the nightly toolchain with
# rust-src (the standard library is built from source with -Z build-std) and are
# built without DirectX 12 (--no-default-features), which Windows 7 does not have.
# Override the toolchain with SLYCED_NIGHTLY (e.g. "nightly-2026-11-05").
#
#   powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1
#
# Optional local code signing: set SLYCED_SIGN_CERT (path to .pfx) and SLYCED_SIGN_PASS,
# or SLYCED_SIGN_THUMBPRINT for a certificate in the Windows store / hardware token.
#
# Releases signed by SignPath Foundation are built by GitHub Actions
# (.github/workflows/release.yml) in two steps: -NoZip builds and checks the
# executables, SignPath signs them, and -ZipOnly packages the signed ones.

param(
    # Build and check the executables in dist\ without packaging them.
    [switch]$NoZip,
    # Only package what is already in dist\ (zip + SHA256SUMS.txt).
    [switch]$ZipOnly
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
# CharLS (JPEG-LS) is built with CMake; Visual Studio ships one.
if (-not $ZipOnly -and -not $env:CMAKE -and -not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $vs = Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft Visual Studio", "$env:ProgramFiles\Microsoft Visual Studio" -Recurse -Filter cmake.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match "\\CMake\\bin\\" } | Select-Object -First 1 -ExpandProperty FullName
    if (-not $vs) { throw "CMake não encontrado (instale 'C++ CMake tools for Windows' no Visual Studio Build Tools)" }
    $env:CMAKE = $vs
}
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value

$nightly = if ($env:SLYCED_NIGHTLY) { $env:SLYCED_NIGHTLY } else { "nightly" }
$targets = @(
    @{ Triple = "x86_64-pc-windows-msvc"; Name = "x64" },
    @{ Triple = "aarch64-pc-windows-msvc"; Name = "arm64" },
    @{ Triple = "x86_64-win7-windows-msvc"; Name = "win7-x64"; Win7 = $true },
    @{ Triple = "i686-win7-windows-msvc"; Name = "win7-x86"; Win7 = $true }
)

# Windows 7 guard: DLLs and functions that only exist from Windows 8 on.
$notOnWin7 = "combase\.dll|api-ms-win-core-synch-l1-2|bcryptprimitives|d3d12\.dll|dcomp\.dll|GetSystemTimePreciseAsFileTime|WaitOnAddress|WakeByAddress|ProcessPrng|CreateFile2|SetThreadDescription|GetDpiForWindow|SetProcessDpiAwarenessContext|RoGetActivationFactory|WindowsCreateString"

function Find-SignTool {
    $kits = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
    Get-ChildItem $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match "\\x64\\" } | Select-Object -Last 1 -ExpandProperty FullName
}

New-Item -ItemType Directory -Force dist | Out-Null
if (-not $NoZip) { Remove-Item dist\SHA256SUMS.txt -ErrorAction SilentlyContinue }
# x64/x86-hosted dumpbin: runs natively on x64 (GitHub runners) and emulated on ARM64.
$dumpbin = Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft Visual Studio", "$env:ProgramFiles\Microsoft Visual Studio" -Recurse -Filter dumpbin.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\Host(x64|x86)\\" } | Select-Object -First 1 -ExpandProperty FullName

function Build-Target($t, $out) {
    if ($t.Win7) {
        rustup toolchain install $nightly --profile minimal --component rust-src | Out-Null
        & cargo "+$nightly" build --release --locked --target $t.Triple -Z build-std --no-default-features
    } else {
        rustup target add $t.Triple | Out-Null
        cargo build --release --locked --target $t.Triple
    }
    if ($LASTEXITCODE -ne 0) { throw "falha ao compilar $($t.Triple)" }

    Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $out | Out-Null
    Copy-Item "target\$($t.Triple)\release\Slyced.exe" "$out\Slyced.exe"
    # Portability guard: no Visual C++ Redistributable may be needed.
    if ($dumpbin) {
        $crt = & $dumpbin /dependents "$out\Slyced.exe" | Select-String -Pattern "MSVCP|VCRUNTIME|ucrtbase"
        if ($crt) { throw "Slyced.exe ($($t.Name)) depende do Visual C++ Redistributable: $crt" }
        if ($t.Win7) {
            $bad = & $dumpbin /imports "$out\Slyced.exe" | Select-String -Pattern $notOnWin7
            if ($bad) { throw "Slyced.exe ($($t.Name)) importa algo que o Windows 7 não tem: $($bad -join '; ')" }
        }
    }
    Copy-Item docs\MANUAL.md "$out\LEIA-ME.md"
    Copy-Item LICENSE-MIT, LICENSE-APACHE "$out\"
    Copy-Item assets\fonts\LICENSE-Phosphor.txt "$out\"
    Copy-Item "assets\Slyced - diagnostico.cmd" "$out\"

    $sign = Find-SignTool
    if ($sign -and ($env:SLYCED_SIGN_CERT -or $env:SLYCED_SIGN_THUMBPRINT)) {
        $signArgs = @("sign", "/fd", "SHA256", "/tr", "http://timestamp.digicert.com", "/td", "SHA256")
        if ($env:SLYCED_SIGN_CERT) { $signArgs += @("/f", $env:SLYCED_SIGN_CERT, "/p", $env:SLYCED_SIGN_PASS) }
        else { $signArgs += @("/sha1", $env:SLYCED_SIGN_THUMBPRINT) }
        & $sign @signArgs "$out\Slyced.exe"
        if ($LASTEXITCODE -ne 0) { throw "falha ao assinar" }
    }
}

foreach ($t in $targets) {
    $out = "dist\Slyced-$version-$($t.Name)"
    if (-not $ZipOnly) {
        Build-Target $t $out
    } elseif (-not (Test-Path "$out\Slyced.exe")) {
        throw "$out\Slyced.exe não existe: compile antes (sem -ZipOnly)"
    }
    if ($NoZip) {
        Write-Host "OK $out\Slyced.exe (sem pacote)"
        continue
    }
    Compress-Archive -Path "$out\*" -DestinationPath "$out.zip" -Force
    $hash = (Get-FileHash "$out\Slyced.exe" -Algorithm SHA256).Hash
    "$hash  Slyced-$($t.Name).exe" | Add-Content "dist\SHA256SUMS.txt"
    Write-Host "OK $out.zip  SHA256 $hash"
}
