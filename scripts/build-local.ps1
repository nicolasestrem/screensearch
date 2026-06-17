# Build a complete local Windows directory, including the managed AI runtime.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\build-local.ps1 [-Debug]

param(
    [switch]$Debug,
    [switch]$SkipDependencyInstall
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Profile = if ($Debug) { "debug" } else { "release" }
$CargoArgs = @("build", "--locked")
if (-not $Debug) {
    $CargoArgs += "--release"
}

Set-Location $Root

Write-Host "[1/4] Building embedded dashboard..." -ForegroundColor Cyan
Push-Location screensearch-ui
try {
    npm ci
    npm run build
}
finally {
    Pop-Location
}

Write-Host "[2/4] Building ScreenSearch ($Profile)..." -ForegroundColor Cyan
& cargo @CargoArgs
if ($LASTEXITCODE -ne 0) {
    throw "Rust build failed"
}

Write-Host "[3/4] Building PP-OCRv5/Qwen quality runtime..." -ForegroundColor Cyan
$Python = Get-Command py -ErrorAction SilentlyContinue
if ($Python) {
    $PythonExe = "py"
    $PythonPrefix = @("-3.12")
}
elseif (Get-Command python -ErrorAction SilentlyContinue) {
    $PythonExe = "python"
    $PythonPrefix = @()
}
else {
    throw "Python 3.12 was not found. Install it from https://www.python.org/downloads/"
}

if (-not $SkipDependencyInstall) {
    & $PythonExe @PythonPrefix -m pip install -r sidecar\requirements.txt
    if ($LASTEXITCODE -ne 0) {
        throw "Sidecar dependency installation failed"
    }
}

& $PythonExe @PythonPrefix sidecar\build.py
if ($LASTEXITCODE -ne 0) {
    throw "Quality runtime build failed"
}

Write-Host "[4/4] Assembling runnable directory..." -ForegroundColor Cyan
$Bundle = Join-Path $Root "target\$Profile\screensearch-local"
$SidecarDestination = Join-Path $Bundle "bin\screensearch-ai-sidecar"
New-Item -ItemType Directory -Force -Path $SidecarDestination | Out-Null
Copy-Item "target\$Profile\screensearch.exe" $Bundle -Force
Copy-Item "sidecar\dist\screensearch-ai-sidecar\*" $SidecarDestination -Recurse -Force
Copy-Item "config.toml" $Bundle -Force
Copy-Item "README.md" $Bundle -Force
Copy-Item "LICENSE" $Bundle -Force

Write-Host ""
Write-Host "Complete local build:" -ForegroundColor Green
Write-Host "  $Bundle\screensearch.exe" -ForegroundColor White
Write-Host ""
Write-Host "Run that executable from the assembled directory. Do not copy the EXE alone." -ForegroundColor Yellow
