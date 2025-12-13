# Download AI RAG Search Model from HuggingFace
# Usage: .\download-model.ps1

param(
    [string]$OutputDir = "installer\models"
)

$ErrorActionPreference = "Stop"

# Model URLs
$modelUrl = "https://huggingface.co/Xenova/paraphrase-multilingual-MiniLM-L12-v2/resolve/main/onnx/model.onnx"
$tokenizerUrl = "https://huggingface.co/Xenova/paraphrase-multilingual-MiniLM-L12-v2/resolve/main/tokenizer.json"

Write-Host "================================================" -ForegroundColor Cyan
Write-Host "AI RAG Search Model Downloader" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# Create output directory
if (-not (Test-Path $OutputDir)) {
    Write-Host "Creating directory: $OutputDir" -ForegroundColor Yellow
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

# Download model.onnx
$modelPath = Join-Path $OutputDir "model.onnx"
Write-Host "Downloading model.onnx (449 MB)..." -ForegroundColor Green
Write-Host "Source: $modelUrl" -ForegroundColor Gray

try {
    $ProgressPreference = 'SilentlyContinue'  # Speeds up downloads
    Invoke-WebRequest -Uri $modelUrl -OutFile $modelPath -MaximumRetryCount 3
    $ProgressPreference = 'Continue'

    # Verify size
    $modelSize = (Get-Item $modelPath).Length / 1MB
    Write-Host "Downloaded: $([math]::Round($modelSize, 2)) MB" -ForegroundColor Green

    if ($modelSize -lt 400) {
        throw "Model file too small: $modelSize MB (expected >400 MB)"
    }
}
catch {
    Write-Host "Failed to download model: $_" -ForegroundColor Red
    exit 1
}

# Download tokenizer.json
$tokenizerPath = Join-Path $OutputDir "tokenizer.json"
Write-Host ""
Write-Host "Downloading tokenizer.json..." -ForegroundColor Green
Write-Host "Source: $tokenizerUrl" -ForegroundColor Gray

try {
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -Uri $tokenizerUrl -OutFile $tokenizerPath -MaximumRetryCount 3
    $ProgressPreference = 'Continue'

    $tokenizerSize = (Get-Item $tokenizerPath).Length / 1MB
    Write-Host "Downloaded: $([math]::Round($tokenizerSize, 2)) MB" -ForegroundColor Green
}
catch {
    Write-Host "Failed to download tokenizer: $_" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "Download complete!" -ForegroundColor Green
Write-Host "Model location: $OutputDir" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
