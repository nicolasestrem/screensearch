# Build ScreenSearch release artifacts
# Usage: .\build-release.ps1 -Version 0.4.35 [-SignBinary] [-SkipSidecar]

param(
    [Parameter(Mandatory=$true)]
    [string]$Version,

    [switch]$SignBinary,
    [Alias("SkipModel")]
    [switch]$SkipSidecar,
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
Write-Host "[1/8] Building React UI..." -ForegroundColor Cyan
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
Write-Host "[2/8] Building Rust release binary..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "Rust build failed" -ForegroundColor Red
    exit 1
}
Write-Host "Rust build complete" -ForegroundColor Green
Write-Host ""

# Step 3: Code signing (optional)
if ($SignBinary) {
    Write-Host "[3/8] Signing binary..." -ForegroundColor Cyan
    & ".\scripts\sign-binary.ps1" -BinaryPath "target\release\screensearch.exe"
    Write-Host ""
}
else {
    Write-Host "[3/8] Skipping code signing (use -SignBinary to enable)" -ForegroundColor Yellow
    Write-Host ""
}

# Step 4: Build the managed quality sidecar
if (-not $SkipSidecar) {
    Write-Host "[4/8] Building PP-OCRv5/Qwen quality sidecar..." -ForegroundColor Cyan
    python -m pip install -r sidecar\requirements.txt
    python sidecar\build.py
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Quality sidecar build failed" -ForegroundColor Red
        exit 1
    }
    Write-Host ""
}
else {
    Write-Host "[4/8] Skipping quality sidecar build" -ForegroundColor Yellow
    Write-Host ""
}

# Step 5: Check for Inno Setup
Write-Host "[5/8] Checking for Inno Setup..." -ForegroundColor Cyan
$isccPath = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
if (-not (Test-Path $isccPath)) {
    Write-Host "Error: Inno Setup not found at $isccPath" -ForegroundColor Red
    Write-Host "Install from: https://jrsoftware.org/isdl.php" -ForegroundColor Yellow
    exit 1
}
Write-Host "Inno Setup found" -ForegroundColor Green
Write-Host ""

# Step 6: Build quality installer
Write-Host "[6/8] Building Quality Installer..." -ForegroundColor Cyan
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
Write-Host "[7/8] Creating Portable ZIP..." -ForegroundColor Cyan
$zipPath = "target\release\installers\ScreenSearch-v$Version-Portable.zip"
New-Item -ItemType Directory -Force -Path "target\release\bin" | Out-Null
Copy-Item "sidecar\dist\screensearch-ai-sidecar" "target\release\bin\" -Recurse -Force
Compress-Archive -Path "target\release\screensearch.exe", "target\release\bin", "config.toml", "LICENSE", "README.md" `
                 -DestinationPath $zipPath -Force
Write-Host "Portable ZIP created: $zipPath" -ForegroundColor Green
Write-Host ""

# Step 8: Generate checksums
Write-Host "[8/8] Generating checksums..." -ForegroundColor Cyan
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
