# Debug Plan: ScreenSearch v0.3.0 + Ministral-3B Integration


## Step 2: Verify UI Loaded (0.3.0 Check)
1. Open browser to `http://localhost:3131`
2. **Expected**: "ScreenSearch Intel" dashboard with glassmorphism design
3. **If you see old UI or blank page**: UI embedding failed - report exact behavior

### Visual Indicators of 0.3.0 UI:
- Header says "ScreenSearch Intel"
- Glassmorphism cards with blur/transparency
- "Daily Digest" card on left
- Circular "Memory Status" gauge on right
- Blue primary color (#2563eb)

---

## Step 3: Test Settings Panel
1. Click Settings gear icon (top right)
2. Go to "Data & AI" tab
3. Enable "Vision Engine" toggle
4. **Check dropdown for "Provider Protocol"**

### Expected Options:
| Option | Description |
|--------|-------------|
| `Local (Ministral-3B) - No API needed` | **NEW** - Uses embedded LLM |
| `Ollama (Local Server)` | External Ollama server |
| `OpenAI Compatible` | ChatGPT, vLLM, LM Studio |

5. Select "Local (Ministral-3B)"
6. **Expected**: Model status panel appears showing download button or "Model ready"

---

## Step 4: Test Local LLM API Endpoints

### 4.1 Model Status
```powershell
curl http://localhost:3131/api/ai/model/status
```

**Expected Response:**
```json
{
  "downloaded": false,
  "downloading": false,
  "model_name": "Ministral-3B-Instruct-2512-Q4_K_M",
  "model_size_bytes": 2150000000,
  "model_path": null
}
```

### 4.2 Trigger Download
```powershell
curl -X POST http://localhost:3131/api/ai/model/download
```

**Expected Response:**
```json
{
  "success": true,
  "message": "Download started. Model size: 2.15 GB"
}
```

### 4.3 Validate Connection (after download + llama-server running)
```powershell
curl -X POST http://localhost:3131/api/ai/validate -H "Content-Type: application/json" -d "{\"provider_url\":\"local\",\"model\":\"ministral-3b\"}"
```

---

## Step 5: Report Results

For each step, note:
- ✅ Works as expected
- ❌ Failed - describe what you see
- ⚠️ Partial - works but with issues

### Checklist:
| Test | Result | Notes |
|------|--------|-------|
| Binary launches | | |
| UI loads at localhost:3131 | | |
| UI is 0.3.0 (Intel dashboard) | | |
| Settings panel opens | | |
| "Local (Ministral-3B)" in dropdown | | |
| Model status card appears | | |
| /api/ai/model/status returns JSON | | |
| /api/ai/model/download triggers | | |

---

## Known Warnings (Ignorable)
- "Model not downloaded" is expected on first run
- "llama-server not running" is expected until model is downloaded and server started
- Console warnings about unused fields are harmless

---

## Troubleshooting

### UI Shows Blank Page
- Check browser console (F12) for JavaScript errors
- Try hard refresh: Ctrl+Shift+R

### UI Shows Old 0.2.0 Design
- Build cache issue - need to rebuild with `cargo xwin build --release`
- Verify `screensearch-api/build.rs` exists (watches dist folder)

### API Endpoints Return 404
- Ensure app is running on port 3131
- Check for firewall blocking localhost

### Model Download Fails
- Check internet connectivity
- Verify disk space (need ~2.5GB free)
