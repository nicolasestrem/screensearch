# Code signing script for ScreenSearch
# Creates self-signed certificate and signs the binary
# Usage: .\sign-binary.ps1 [-BinaryPath target\release\screensearch.exe]

param(
    [string]$BinaryPath = "target\release\screensearch.exe",
    [string]$CertPassword = "ScreenSearch2025"
)

$ErrorActionPreference = "Stop"

Write-Host "================================================" -ForegroundColor Cyan
Write-Host "Code Signing Tool" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

if (-not (Test-Path $BinaryPath)) {
    Write-Host "Error: Binary not found: $BinaryPath" -ForegroundColor Red
    exit 1
}

# Certificate details
$certName = "ScreenSearch Self-Signed Certificate"
$certSubject = "CN=Nicolas Estrem, O=ScreenSearch, L=San Francisco, S=California, C=US"

# Check if certificate already exists
$existingCert = Get-ChildItem -Path Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $certSubject }

if ($existingCert) {
    Write-Host "Using existing certificate: $certSubject" -ForegroundColor Green
    $cert = $existingCert
}
else {
    Write-Host "Creating self-signed certificate..." -ForegroundColor Yellow
    Write-Host "Subject: $certSubject" -ForegroundColor Gray

    try {
        $cert = New-SelfSignedCertificate `
            -Type CodeSigningCert `
            -Subject $certSubject `
            -KeyUsage DigitalSignature `
            -FriendlyName $certName `
            -CertStoreLocation "Cert:\CurrentUser\My" `
            -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3") `
            -NotAfter (Get-Date).AddYears(3)

        Write-Host "Certificate created successfully" -ForegroundColor Green
        Write-Host "Thumbprint: $($cert.Thumbprint)" -ForegroundColor Gray
    }
    catch {
        Write-Host "Failed to create certificate: $_" -ForegroundColor Red
        exit 1
    }
}

# Sign the binary
Write-Host ""
Write-Host "Signing binary: $BinaryPath" -ForegroundColor Yellow

try {
    Set-AuthenticodeSignature `
        -FilePath $BinaryPath `
        -Certificate $cert `
        -TimestampServer "http://timestamp.digicert.com" `
        -HashAlgorithm SHA256

    Write-Host "Binary signed successfully!" -ForegroundColor Green
}
catch {
    Write-Host "Failed to sign binary: $_" -ForegroundColor Red
    exit 1
}

# Verify signature
Write-Host ""
Write-Host "Verifying signature..." -ForegroundColor Yellow
$signature = Get-AuthenticodeSignature -FilePath $BinaryPath

if ($signature.Status -eq "Valid" -or $signature.Status -eq "UnknownError") {
    Write-Host "Signature status: $($signature.Status)" -ForegroundColor Green
    Write-Host "Signer: $($signature.SignerCertificate.Subject)" -ForegroundColor Gray
}
else {
    Write-Host "Warning: Signature status: $($signature.Status)" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "Note: This is a self-signed certificate." -ForegroundColor Yellow
Write-Host "Users will still see SmartScreen warnings." -ForegroundColor Yellow
Write-Host ""
Write-Host "For production, purchase an EV code signing" -ForegroundColor Yellow
Write-Host "certificate from a trusted CA." -ForegroundColor Yellow
Write-Host "================================================" -ForegroundColor Cyan
