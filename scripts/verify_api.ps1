
# ScreenSearch API Verification Script

$baseUrl = "http://localhost:3131/api"

function Test-Endpoint {
    param (
        [string]$Method,
        [string]$Url,
        [string]$Name,
        [hashtable]$Body = $null
    )

    Write-Host "Testing $Name..." -NoNewline
    
    try {
        $params = @{
            Uri = $Url
            Method = $Method
            ErrorAction = "Stop"
        }
        
        if ($Body) {
            $params.Add("Body", ($Body | ConvertTo-Json))
            $params.Add("ContentType", "application/json")
        }

        $response = Invoke-RestMethod @params
        Write-Host " [OK]" -ForegroundColor Green
        return $response
    }
    catch {
        Write-Host " [FAILED]" -ForegroundColor Red
        Write-Host "Error: $_"
        return $null
    }
}

# 1. Health Check
$health = Test-Endpoint -Method GET -Url "$baseUrl/health" -Name "Health Check"
if ($health) {
    Write-Host "Status: $($health.status)"
    Write-Host "Frames: $($health.frame_count)"
}

# 2. Check Embedding Status
$embStatus = Test-Endpoint -Method GET -Url "$baseUrl/embeddings/status" -Name "Embedding Status"
if ($embStatus) {
    Write-Host "Embeddings Enabled: $($embStatus.enabled)"
    Write-Host "Model: $($embStatus.model)"
}

# 3. Enable Vision (if disabled)
# Getting settings first
$settings = Test-Endpoint -Method GET -Url "$baseUrl/settings" -Name "Get Settings"
if ($settings) {
    Write-Host "Vision Enabled: $($settings.vision_enabled)"
    
    # Update settings to enable vision
    $newSettings = @{
        capture_interval = $settings.capture_interval
        monitors = $settings.monitors
        excluded_apps = $settings.excluded_apps
        is_paused = $settings.is_paused
        retention_days = $settings.retention_days
        vision_enabled = 1
        vision_provider = "ollama"
        vision_model = "moondream" 
        vision_endpoint = "http://localhost:11434"
    }
    
    Test-Endpoint -Method POST -Url "$baseUrl/settings" -Name "Update Settings (Enable Vision)" -Body $newSettings | Out-Null
}

# 4. Trigger Embedding Generation (if needed) -> POST /api/embeddings/generate
Test-Endpoint -Method POST -Url "$baseUrl/embeddings/generate" -Name "Trigger Embeddings" | Out-Null

# 5. Perform Semantic Search
# Note: Might return empty if no embeddings generated yet, but verification is that call succeeds (200 OK)
$search = Test-Endpoint -Method GET -Url "$baseUrl/search?q=test&mode=semantic" -Name "Semantic Search"

if ($search) {
    Write-Host "Found $($search.Count) results."
}

Write-Host "`nVerification Complete."
