#Requires -Version 5.1
param(
    [string]$Version = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot

# A long-lived shell can predate a user environment variable refresh.
foreach ($name in @("RUSTUP_HOME", "CARGO_HOME")) {
    if (-not [Environment]::GetEnvironmentVariable($name, "Process")) {
        $value = [Environment]::GetEnvironmentVariable($name, "User")
        if ($value) {
            Set-Item -Path "Env:$name" -Value $value
        }
    }
}

if (-not $Version) {
    $tauriConfig = Get-Content -Raw (Join-Path $Root "apps/codex-plus-manager/src-tauri/tauri.conf.json") |
        ConvertFrom-Json
    $Version = $tauriConfig.version
}

if (-not $SkipBuild) {
    Push-Location (Join-Path $Root "apps/codex-plus-manager")
    try {
        npm run vite:build
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } finally {
        Pop-Location
    }

    Push-Location $Root
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } finally {
        Pop-Location
    }
}

$Stage = Join-Path $Root "dist/windows/app"
New-Item -ItemType Directory -Force $Stage | Out-Null
Copy-Item -Force (Join-Path $Root "target/release/codex-plus-plus.exe") $Stage
Copy-Item -Force (Join-Path $Root "target/release/codex-plus-plus-manager.exe") $Stage

$Makensis = @(Get-Command makensis -ErrorAction SilentlyContinue).Source
if (-not $Makensis) {
    $candidates = @(
        "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
        "${env:LOCALAPPDATA}\tauri\NSIS\Bin\makensis.exe"
    )
    $Makensis = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Makensis) {
    throw "makensis.exe not found. Run tauri build once or install NSIS."
}

Push-Location (Join-Path $Root "scripts/installer/windows")
try {
    & $Makensis "/INPUTCHARSET" "UTF8" "/DVERSION=$Version" "CodexPlusPlus.nsi"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

$Package = Join-Path $Root "dist/windows/CodexPlusPlus-$Version-windows-x64-setup.exe"
if (-not (Test-Path $Package)) {
    throw "Setup generation failed: $Package"
}

Write-Host "Created: $Package"
