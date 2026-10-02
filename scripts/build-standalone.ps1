param()

$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$cargo = Get-Command cargo.exe -ErrorAction SilentlyContinue

if (-not $cargo) {
    $cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
    $cargoExe = Join-Path $cargoBin "cargo.exe"
    if (Test-Path $cargoExe) {
        $env:Path = "$cargoBin;$env:Path"
    }
}

if (-not (Get-Command cargo.exe -ErrorAction SilentlyContinue)) {
    throw "cargo.exe was not found. Install the Rust MSVC toolchain on the build machine."
}

if (-not (Get-Command bun.exe -ErrorAction SilentlyContinue)) {
    throw "bun.exe was not found. Bun is required only on the build machine."
}

Push-Location $repoRoot
try {
    & bun run --cwd apps/desktop tauri build --no-bundle
    if ($LASTEXITCODE -ne 0) {
        throw "Tauri standalone build failed with exit code $LASTEXITCODE"
    }

    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "verify-standalone.ps1")
    if ($LASTEXITCODE -ne 0) {
        throw "Standalone verification failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}
