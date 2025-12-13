# Generate SHA256 checksums for release artifacts
# Usage: .\generate-checksums.ps1 [-Path target\release\installers]

param(
    [string]$Path = "target\release\installers"
)

$ErrorActionPreference = "Stop"

Write-Host "================================================" -ForegroundColor Cyan
Write-Host "Checksum Generator" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

if (-not (Test-Path $Path)) {
    Write-Host "Error: Directory not found: $Path" -ForegroundColor Red
    exit 1
}

# Find all release artifacts
$files = Get-ChildItem -Path $Path -Filter "ScreenSearch-v*" | Where-Object { -not $_.PSIsContainer }

if ($files.Count -eq 0) {
    Write-Host "No release artifacts found in $Path" -ForegroundColor Yellow
    exit 0
}

Write-Host "Found $($files.Count) file(s):" -ForegroundColor Green
$files | ForEach-Object { Write-Host "  - $($_.Name)" -ForegroundColor Gray }
Write-Host ""

# Generate checksums
$checksums = @()
foreach ($file in $files) {
    Write-Host "Hashing $($file.Name)..." -ForegroundColor Yellow
    $hash = (Get-FileHash -Path $file.FullName -Algorithm SHA256).Hash.ToLower()
    $checksums += "$hash  $($file.Name)"
}

# Write to file
$outputPath = Join-Path $Path "checksums.txt"
$checksums | Out-File -FilePath $outputPath -Encoding UTF8

Write-Host ""
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "Checksums written to: $outputPath" -ForegroundColor Green
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""
Get-Content $outputPath
