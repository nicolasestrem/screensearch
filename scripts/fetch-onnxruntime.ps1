# Download and stage the ONNX Runtime DLL that the in-process fastembed embedding
# engine loads dynamically (the `ort` crate's `load-dynamic` feature). At runtime
# the engine looks for `onnxruntime.dll` next to the executable (see
# configure_ort_dylib_path in screensearch-embeddings/src/engine.rs), so the
# release/local/CI tooling must place it there.
#
# The version MUST match what `ort-sys` 2.0.0-rc.12 expects: ONNX Runtime 1.24.2.
# A mismatched DLL fails at session creation with an API-version error.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\fetch-onnxruntime.ps1 [-DestDir <dir>]
param(
    [string]$DestDir = "target/release"
)

$ErrorActionPreference = "Stop"

$Version   = "1.24.2"
$Url       = "https://github.com/microsoft/onnxruntime/releases/download/v$Version/onnxruntime-win-x64-$Version.zip"
# SHA256 of the official onnxruntime-win-x64-1.24.2.zip release asset.
$ZipSha256 = "8e3e9c826375352e29cb2614fe44f3d7a4b0ff7b8028ad7a456af9d949a7e8b0"
# SHA256 of lib\onnxruntime.dll inside that archive (x64 CPU build).
$DllSha256 = "114947d633e6844ce3c4b51ef6678f776628571d08a5763859c61642c8dcca9c"

$DestDir = (New-Item -ItemType Directory -Force -Path $DestDir).FullName
$DestDll = Join-Path $DestDir "onnxruntime.dll"

# Skip if the correct DLL is already staged (idempotent across repeated builds).
if (Test-Path $DestDll) {
    $have = (Get-FileHash -Path $DestDll -Algorithm SHA256).Hash.ToLower()
    if ($have -eq $DllSha256) {
        Write-Host "onnxruntime.dll $Version already staged in $DestDir"
        return
    }
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) "ort-$Version"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp "onnxruntime.zip"

Write-Host "Downloading ONNX Runtime $Version from $Url"
# Manual retry loop (Invoke-WebRequest -MaximumRetryCount is pwsh 7+ only; this
# script also runs under Windows PowerShell 5.1 via build-local.ps1).
$attempts = 0
while ($true) {
    $attempts++
    try {
        Invoke-WebRequest -Uri $Url -OutFile $zip -UseBasicParsing
        break
    }
    catch {
        if ($attempts -ge 3) { throw }
        Write-Host "Download attempt $attempts failed ($_); retrying..."
        Start-Sleep -Seconds 3
    }
}

$gotZip = (Get-FileHash -Path $zip -Algorithm SHA256).Hash.ToLower()
if ($gotZip -ne $ZipSha256) {
    throw "ONNX Runtime archive checksum mismatch: expected $ZipSha256, got $gotZip"
}

$extract = Join-Path $tmp "extracted"
if (Test-Path $extract) { Remove-Item $extract -Recurse -Force }
Expand-Archive -Path $zip -DestinationPath $extract -Force

$src = Get-ChildItem -Path $extract -Recurse -Filter "onnxruntime.dll" | Select-Object -First 1
if (-not $src) { throw "onnxruntime.dll not found inside the downloaded archive" }
Copy-Item $src.FullName $DestDll -Force

$gotDll = (Get-FileHash -Path $DestDll -Algorithm SHA256).Hash.ToLower()
if ($gotDll -ne $DllSha256) {
    throw "Staged onnxruntime.dll checksum mismatch: expected $DllSha256, got $gotDll"
}

Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "Staged onnxruntime.dll $Version -> $DestDll"
