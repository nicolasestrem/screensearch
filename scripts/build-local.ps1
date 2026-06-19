# Build a complete local Windows directory. All ML runs in-process in Rust
# (native Windows OCR, fastembed/ONNX embeddings, llama.cpp answer generation) —
# there is no Python sidecar to build or bundle.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\build-local.ps1 [-Debug]

param(
    [switch]$Debug
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Profile = if ($Debug) { "debug" } else { "release" }
$CargoArgs = @("build", "--locked")
if (-not $Debug) {
    $CargoArgs += "--release"
}

Set-Location $Root

Write-Host "[1/3] Building embedded dashboard..." -ForegroundColor Cyan
Push-Location screensearch-ui
try {
    npm ci
    npm run build
}
finally {
    Pop-Location
}

Write-Host "[2/3] Building ScreenSearch ($Profile)..." -ForegroundColor Cyan
& cargo @CargoArgs
if ($LASTEXITCODE -ne 0) {
    throw "Rust build failed"
}

Write-Host "[3/3] Assembling runnable directory..." -ForegroundColor Cyan
$Bundle = Join-Path $Root "target\$Profile\screensearch-local"
New-Item -ItemType Directory -Force -Path $Bundle | Out-Null
Copy-Item "target\$Profile\screensearch.exe" $Bundle -Force
Copy-Item "config.toml" $Bundle -Force
Copy-Item "README.md" $Bundle -Force
Copy-Item "LICENSE" $Bundle -Force

Write-Host ""
Write-Host "Complete local build:" -ForegroundColor Green
Write-Host "  $Bundle\screensearch.exe" -ForegroundColor White
Write-Host ""
Write-Host "Run that executable from the assembled directory. Do not copy the EXE alone." -ForegroundColor Yellow
