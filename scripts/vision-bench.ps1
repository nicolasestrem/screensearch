param(
    [Parameter(Mandatory=$true)][string]$Model,
    [Parameter(Mandatory=$true)][string]$Mmproj,
    [int]$Port = 31199,
    [int]$ImageMaxTokens = 1024,
    [int]$MaxTokens = 256
)

# Quick A/B harness for evaluating a local vision (model, mmproj) pair through the
# real llama-server path: measures true per-frame latency (distinct images),
# checks the response is valid JSON, and exercises sequential requests. Used to
# pick the default vision model (see docs/vision.md). Run:
#   ./scripts/vision-bench.ps1 -Model <gguf> -Mmproj <mmproj-gguf>
$ErrorActionPreference = "Stop"
$bin    = "$env:LOCALAPPDATA\ScreenSearch\bin\llama-server.exe"
$models = Join-Path (Split-Path $PSScriptRoot -Parent) ".models"
$modelPath  = Join-Path $models $Model
$mmprojPath = Join-Path $models $Mmproj

# Build 1280-longest-edge JPEGs from the N newest DISTINCT captures (matches the
# new pipeline). Using distinct images per request measures true per-frame cost:
# each request pays the vision-encoder cost (no prompt-cache image reuse).
Add-Type -AssemblyName System.Drawing
$cap = "$env:LOCALAPPDATA\screensearch\captures"
$srcs = Get-ChildItem $cap -Filter *.jpg | Sort-Object LastWriteTime -Descending | Select-Object -First 4
$b64list = @()
foreach ($src in $srcs) {
    $img = [System.Drawing.Image]::FromFile($src.FullName)
    $scale = 1280.0 / [Math]::Max($img.Width, $img.Height)
    if ($scale -ge 1) { $scale = 1.0 }
    $nw = [int]($img.Width * $scale); $nh = [int]($img.Height * $scale)
    $bmp = New-Object System.Drawing.Bitmap $nw, $nh
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = 'HighQualityBicubic'
    $g.DrawImage($img, 0, 0, $nw, $nh)
    $tmp = Join-Path $env:TEMP "vbench_$($src.BaseName).jpg"
    $bmp.Save($tmp, [System.Drawing.Imaging.ImageFormat]::Jpeg)
    $g.Dispose(); $bmp.Dispose(); $img.Dispose()
    $b64list += [Convert]::ToBase64String([IO.File]::ReadAllBytes($tmp))
    Write-Host "Test image: $($src.Name) -> ${nw}x${nh} ($([math]::Round((Get-Item $tmp).Length/1KB,0)) KB)"
}

# Launch llama-server with the production-equivalent vision flags.
$args = @("-m",$modelPath,"--mmproj",$mmprojPath,"--port",$Port,"--host","127.0.0.1",
          "-c","8192","--jinja","-ngl","99","--image-max-tokens",$ImageMaxTokens,"--flash-attn","on")
$log = Join-Path $env:TEMP "vbench_server.log"
Write-Host "Launching: llama-server $($args -join ' ')"
$proc = Start-Process -FilePath $bin -ArgumentList $args -PassThru -NoNewWindow -RedirectStandardError $log -RedirectStandardOutput "$log.out"

try {
    # Wait for readiness (model + mmproj load over Vulkan can take a while).
    $ready = $false
    for ($i = 0; $i -lt 120; $i++) {
        Start-Sleep -Seconds 1
        if ($proc.HasExited) { throw "Server exited early (code $($proc.ExitCode)). See $log" }
        try {
            $h = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/health" -TimeoutSec 2
            if ($h.status -eq "ok") { $ready = $true; break }
        } catch {}
    }
    if (-not $ready) { throw "Server not ready after 120s. See $log" }
    Write-Host "Server ready after ~${i}s"

    $sys = 'You are a visual intelligence engine for a screen-history tool that ALREADY has full OCR of the screen text. Do NOT transcribe or list large amounts of on-screen text. Analyze the screenshot and return ONLY compact JSON in exactly this shape: {"description":"1-2 sentence summary of what the user is doing and the main on-screen content","visible_text":["up to 6 of the most prominent titles or labels only"],"activity_type":"one of: coding, design, browsing, communication, entertainment, productivity, other","app_hint":"best guess at the active application","confidence":0.0-1.0}. Be brief. Ignore taskbar and window chrome unless relevant.'
    # One sequential request per DISTINCT image -> true per-frame latency +
    # #17200 repeated-request check. First request also pays one-time Vulkan
    # vision-graph warmup, so it is reported but excluded from the warm average.
    $warm = @()
    for ($r = 0; $r -lt $b64list.Count; $r++) {
        $body = @{
            model = "test"
            messages = @(
                @{ role = "system"; content = $sys },
                @{ role = "user"; content = @(
                    @{ type = "text"; text = "Context: App: Unknown. Analyze this frame." },
                    @{ type = "image_url"; image_url = @{ url = "data:image/jpeg;base64,$($b64list[$r])" } }
                )}
            )
            response_format = @{ type = "json_object" }
            max_tokens = $MaxTokens
            stream = $false
        } | ConvertTo-Json -Depth 8

        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $resp = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/v1/chat/completions" -Method Post -Body $body -ContentType "application/json" -TimeoutSec 120
        $sw.Stop()
        $content = $resp.choices[0].message.content
        $valid = $false; try { $null = $content | ConvertFrom-Json; $valid = $true } catch {}
        Write-Host ("--- request {0} (distinct image): {1} ms | validJSON={2} ---" -f ($r+1), $sw.ElapsedMilliseconds, $valid)
        Write-Host $content
        if ($r -gt 0) { $warm += $sw.ElapsedMilliseconds }
    }
    if ($warm.Count -gt 0) {
        Write-Host ("=== warm per-frame avg (excl. first): {0} ms over {1} distinct images ===" -f [math]::Round(($warm | Measure-Object -Average).Average,0), $warm.Count)
    }
}
finally {
    if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
    Write-Host "=== server stderr tail ==="
    if (Test-Path $log) { Get-Content $log -Tail 25 }
}
