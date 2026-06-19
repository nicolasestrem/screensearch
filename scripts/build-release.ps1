# Build ScreenSearch release artifacts. All ML runs in-process in Rust — there
# is no Python sidecar to build or bundle.
# Usage: .\build-release.ps1 -Version 0.4.35 [-SignBinary] [-Clean]

param(
    [Parameter(Mandatory=$true)]
    [string]$Version,

    [switch]$SignBinary,
    [switch]$Clean
)

$ErrorActionPreference = "Stop"

Write-Host "================================================" -ForegroundColor Cyan
Write-Host "ScreenSearch Release Builder v$Version" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# Validate version format
if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    Write-Host "Error: Invalid version format. Use semantic versioning (e.g., 0.2.0)" -ForegroundColor Red
    exit 1
}

# Clean previous builds
if ($Clean) {
    Write-Host "Cleaning previous builds..." -ForegroundColor Yellow
    if (Test-Path "target\release") {
        Remove-Item "target\release\installers" -Recurse -Force -ErrorAction SilentlyContinue
    }
    cargo clean --release
    Write-Host "Clean complete" -ForegroundColor Green
    Write-Host ""
}

# Step 1: Build React UI
Write-Host "[1/7] Building React UI..." -ForegroundColor Cyan
Push-Location screensearch-ui
try {
    npm ci
    npm run build
    Write-Host "UI build complete" -ForegroundColor Green
}
catch {
    Write-Host "UI build failed: $_" -ForegroundColor Red
    Pop-Location
    exit 1
}
finally {
    Pop-Location
}
Write-Host ""

# Step 2: Build Rust binary
Write-Host "[2/7] Building Rust release binary..." -ForegroundColor Cyan
# .cargo/config.toml hard-codes the `lld` linker for Linux->Windows cross-compile,
# which is absent on a native Windows host (cargo would fail with "linker `lld`
# not found"). Import the MSVC build environment and override the linker per
# target via env vars (CARGO_TARGET_*_LINKER wins over the config file's linker
# key). Do NOT edit .cargo/config.toml.
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    $vsPath = & $vswhere -latest -property installationPath
    $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
    if (Test-Path $vcvars) {
        cmd /c "`"$vcvars`" >nul 2>&1 && set" | ForEach-Object {
            if ($_ -match '^(.*?)=(.*)$') { Set-Item -Path "env:$($matches[1])" -Value $matches[2] }
        }
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = "link.exe"
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "-C target-feature=+crt-static"
    }
}
# The UI bundle was built in step 1; skip build.rs re-running npm.
$env:SKIP_UI_BUILD = "1"
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "Rust build failed" -ForegroundColor Red
    exit 1
}
Write-Host "Rust build complete" -ForegroundColor Green
Write-Host ""

# Step 3: Code signing (optional)
if ($SignBinary) {
    Write-Host "[3/7] Signing binary..." -ForegroundColor Cyan
    & ".\scripts\sign-binary.ps1" -BinaryPath "target\release\screensearch.exe"
    Write-Host ""
}
else {
    Write-Host "[3/7] Skipping code signing (use -SignBinary to enable)" -ForegroundColor Yellow
    Write-Host ""
}

# Step 4: Check for Inno Setup
Write-Host "[4/7] Checking for Inno Setup..." -ForegroundColor Cyan
$isccCandidates = @(
    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 7\ISCC.exe",
    "C:\Program Files (x86)\Inno Setup 7\ISCC.exe"
)
$isccPath = $isccCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $isccPath) {
    $isccCmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($isccCmd) { $isccPath = $isccCmd.Source }
}
if (-not $isccPath) {
    Write-Host "Error: Inno Setup compiler (ISCC.exe) not found." -ForegroundColor Red
    Write-Host "Install from: https://jrsoftware.org/isdl.php" -ForegroundColor Yellow
    exit 1
}
Write-Host "Inno Setup found: $isccPath" -ForegroundColor Green
Write-Host ""

# Step 5: Build installer
Write-Host "[5/7] Building Installer..." -ForegroundColor Cyan
& $isccPath "installer\screensearch.iss"

if ($LASTEXITCODE -eq 0) {
    $installerPath = "target\release\installers\ScreenSearch-v$Version-Setup.exe"
    $qualityPath = "target\release\installers\ScreenSearch-v$Version-Setup-Quality.exe"

    if (Test-Path $installerPath) {
        Move-Item $installerPath $qualityPath -Force
        Write-Host "Quality installer created: $qualityPath" -ForegroundColor Green
    }
}
else {
    Write-Host "Quality installer build failed" -ForegroundColor Red
    exit 1
}
Write-Host ""

# Create Portable ZIP
Write-Host "[6/7] Creating Portable ZIP..." -ForegroundColor Cyan
$zipPath = "target\release\installers\ScreenSearch-v$Version-Portable.zip"
Compress-Archive -Path "target\release\screensearch.exe", "config.toml", "LICENSE", "README.md" `
                 -DestinationPath $zipPath -Force
Write-Host "Portable ZIP created: $zipPath" -ForegroundColor Green
Write-Host ""

# Step 7: Generate checksums
Write-Host "[7/7] Generating checksums..." -ForegroundColor Cyan
& ".\scripts\generate-checksums.ps1" -Path "target\release\installers"
Write-Host ""

# Build summary
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "Build Complete!" -ForegroundColor Green
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

$artifacts = Get-ChildItem "target\release\installers\ScreenSearch-v$Version-*"
Write-Host "Release Artifacts:" -ForegroundColor Cyan
foreach ($artifact in $artifacts) {
    $sizeMB = [math]::Round($artifact.Length / 1MB, 2)
    Write-Host "  $($artifact.Name) - $sizeMB MB" -ForegroundColor Gray
}

Write-Host ""
Write-Host "Next Steps:" -ForegroundColor Yellow
Write-Host "  1. Test installers on clean Windows 10/11 VMs" -ForegroundColor Gray
Write-Host "  2. Upload to VirusTotal for scanning" -ForegroundColor Gray
Write-Host "  3. Create git tag: git tag -a v$Version -m 'Release v$Version'" -ForegroundColor Gray
Write-Host "  4. Push tag: git push origin v$Version" -ForegroundColor Gray
Write-Host ""
