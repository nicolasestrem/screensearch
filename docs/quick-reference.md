# ScreenSearch Quick Reference

**Single-page cheat sheet for common tasks, commands, and troubleshooting**

---

## Installation

### Download Binary (Windows 10/11)
```bash
# Download from GitHub Releases
https://github.com/nicolasestrem/screensearch/releases/latest

# Extract ZIP and run
screensearch.exe
```

### Build from Source
```bash
# Prerequisites: Rust 1.70+, Visual Studio Build Tools, Node.js 18+

# Clone repository
git clone https://github.com/nicolasestrem/screensearch.git
cd screensearch

# Build backend (release mode)
cargo build --release

# Run application
cargo run --release

# Build web UI (optional - embedded in binary)
cd screensearch-ui
npm install && npm run build
```

---

## Configuration

### Key Settings (config.toml)

```toml
[capture]
interval_ms = 3000              # Capture every 3 seconds
enable_frame_diff = true        # Skip unchanged frames
diff_threshold = 0.006          # 0.6% change threshold
monitor_indices = []            # Empty = all monitors, [0] = primary

[storage]
format = "jpeg"                 # "jpeg" or "png"
jpeg_quality = 80               # 1-100 (80 = good quality, small size)
max_width = 1920                # Resize to max width (maintains aspect)

[ocr]
min_confidence = 0.7            # Filter OCR below 70% confidence
worker_threads = 2              # OCR processing threads

[database]
path = "screensearch.db"
enable_wal = true               # Write-Ahead Logging
cache_size_kb = -2000           # 2MB cache

[cleanup]
enabled = true
retention_days = 30             # Delete data older than 30 days
cleanup_interval_hours = 24

[privacy]
excluded_apps = ["1Password", "KeePass", "Bitwarden"]
pause_on_lock = true

[embeddings]
enabled = false                 # Enable semantic search
model = "local"                 # "local" or "api"
embedding_dim = 384
hybrid_search_alpha = 0.3       # 70% FTS5, 30% vector

[api]
host = "127.0.0.1"
port = 3131
auto_open_browser = true
```

**Config Location**: `config.toml` in app directory (create if missing)

---

## Common API Calls

### Search Operations

```bash
# Basic keyword search
curl "http://localhost:3131/search?q=meeting&limit=10"

# Search with time filter
curl "http://localhost:3131/search?q=error&start_time=2025-12-10T00:00:00Z&end_time=2025-12-10T23:59:59Z"

# Search by application
curl "http://localhost:3131/search?q=password&app=Chrome&limit=20"

# Multi-keyword search
curl "http://localhost:3131/search?q=meeting+AND+calendar"

# Get all recent frames
curl "http://localhost:3131/frames?limit=50"

# Get specific frame
curl "http://localhost:3131/frames/123"

# Get frame image
curl "http://localhost:3131/frames/123/image" --output frame.jpg
```

### Embeddings & Semantic Search

```bash
# Check embedding status
curl "http://localhost:3131/api/embeddings/status"

# Enable embeddings
curl -X POST "http://localhost:3131/api/embeddings/enable"

# Generate embeddings for existing frames
curl -X POST "http://localhost:3131/api/embeddings/generate"

# Semantic search query
curl -X POST "http://localhost:3131/api/embeddings/search" \
  -H "Content-Type: application/json" \
  -d '{"query": "database migration code", "limit": 10}'

# Hybrid search (FTS5 + Vector)
curl "http://localhost:3131/search?q=meeting&use_embeddings=true"
```

### AI Intelligence

```bash
# Test AI provider connection
curl -X POST "http://localhost:3131/api/ai/validate" \
  -H "Content-Type: application/json" \
  -d '{"provider_url": "http://localhost:11434/v1", "model": "llama3"}'

# Generate daily summary
curl -X POST "http://localhost:3131/api/ai/generate" \
  -H "Content-Type: application/json" \
  -d '{
    "provider_url": "http://localhost:11434/v1",
    "model": "llama3",
    "start_time": "2025-12-10T00:00:00Z",
    "end_time": "2025-12-11T00:00:00Z",
    "prompt": "Summarize my work activity"
  }'

# Custom query with OpenAI
curl -X POST "http://localhost:3131/api/ai/generate" \
  -H "Content-Type: application/json" \
  -d '{
    "provider_url": "https://api.openai.com/v1",
    "model": "gpt-4",
    "api_key": "sk-...",
    "start_time": "2025-12-10T09:00:00Z",
    "end_time": "2025-12-10T17:00:00Z",
    "prompt": "What projects did I work on today?"
  }'
```

### Automation

```bash
# Click at coordinates
curl -X POST "http://localhost:3131/automation/click" \
  -H "Content-Type: application/json" \
  -d '{"x": 500, "y": 300, "button": "left"}'

# Type text
curl -X POST "http://localhost:3131/automation/type" \
  -H "Content-Type: application/json" \
  -d '{"text": "Hello, World!", "delay_ms": 50}'

# Press key combination (Ctrl+S)
curl -X POST "http://localhost:3131/automation/press-key" \
  -H "Content-Type: application/json" \
  -d '{"key": "s", "modifiers": ["ctrl"]}'

# Find UI elements
curl -X POST "http://localhost:3131/automation/find-elements" \
  -H "Content-Type: application/json" \
  -d '{"selector": "Button[@Name=\"Submit\"]", "timeout_ms": 5000}'

# Open application
curl -X POST "http://localhost:3131/automation/open-app" \
  -H "Content-Type: application/json" \
  -d '{"app_name": "notepad.exe"}'
```

### System Health

```bash
# Health check
curl "http://localhost:3131/health"

# System stats
curl "http://localhost:3131/stats"

# Performance metrics
curl "http://localhost:3131/metrics"
```

---

## Troubleshooting

### Windows OCR Not Working

**Symptoms**: No text extracted, OCR errors in logs

**Solutions**:
→ Install Windows OCR Language Pack:
  - Settings → Time & Language → Language
  - Add English (United States)
  - Click Options → Download OCR
  - Restart application

→ Verify OCR availability:
```bash
# Check Windows version
winver
# Requires Windows 10 build 17763+ or Windows 11
```

→ Check logs for specific errors:
```bash
type screensearch.log | findstr "OCR"
```

---

### Embeddings Download Failed

**Symptoms**: "Failed to download model", embedding generation errors

**Solutions**:
→ Check internet connection (required for first-time model download)

→ Verify model download path has write permissions:
```bash
# Windows default: C:\Users\<username>\.screensearch\models\
# Check folder exists and is writable
```

→ Manual download (if automatic fails):
1. Download from: https://huggingface.co/sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2
2. Extract to: `C:\Users\<username>\.screensearch\models\paraphrase-multilingual-MiniLM-L12-v2\`
3. Restart application

→ Disable embeddings temporarily:
```toml
[embeddings]
enabled = false
```

---

### High CPU Usage

**Symptoms**: CPU constantly above 50%, system slowdown

**Solutions**:
→ Increase capture interval:
```toml
[capture]
interval_ms = 5000  # Increase from 3000 to 5000ms
```

→ Reduce OCR worker threads:
```toml
[ocr]
worker_threads = 1  # Reduce from 2
```

→ Enable frame differencing (skip unchanged frames):
```toml
[capture]
enable_frame_diff = true
diff_threshold = 0.010  # Increase threshold (skip more)
```

→ Disable embeddings if not needed:
```toml
[embeddings]
enabled = false
```

→ Check for runaway processes:
```bash
# View detailed metrics
curl "http://localhost:3131/metrics"
```

---

### Database Locked

**Symptoms**: "Database is locked", write failures

**Solutions**:
→ Enable WAL mode (Write-Ahead Logging):
```toml
[database]
enable_wal = true
```

→ Increase connection pool:
```toml
[database]
max_connections = 50
acquire_timeout_secs = 30  # Increase from 10
```

→ Check for multiple instances:
```bash
# Windows Task Manager → Details → screensearch.exe
# Only one instance should be running
```

→ Restart application:
```bash
# Close all instances and restart
cargo run --release
```

→ Database corruption (last resort):
```bash
# Backup current database
copy screensearch.db screensearch.db.backup

# Run integrity check
sqlite3 screensearch.db "PRAGMA integrity_check;"

# If corrupted, delete and restart (data loss)
del screensearch.db
cargo run --release
```

---

### API Server Not Responding

**Symptoms**: Connection refused, timeout errors

**Solutions**:
→ Check if server is running:
```bash
netstat -an | findstr :3131
# Should show LISTENING on 127.0.0.1:3131
```

→ Verify firewall settings:
- Windows Defender Firewall → Allow an app
- Ensure screensearch.exe has local network access

→ Check port availability:
```bash
# If port 3131 is in use, change in config:
[api]
port = 3132  # Use different port
```

→ Check logs for startup errors:
```bash
type screensearch.log | findstr "API"
```

---

### Empty Search Results

**Symptoms**: Search returns nothing despite captured frames

**Solutions**:
→ Verify OCR is working:
```bash
curl "http://localhost:3131/frames?limit=5"
# Check if frames have ocr_text field populated
```

→ Check FTS5 index:
```bash
sqlite3 screensearch.db
SELECT count(*) FROM ocr_text_fts;
# Should return > 0 if text was extracted
```

→ Use broader search terms:
```bash
# Instead of exact phrase:
curl "http://localhost:3131/search?q=meeting"
# Try partial match:
curl "http://localhost:3131/search?q=meet*"
```

→ Check time filters aren't excluding results:
```bash
# Remove time filters and retry
curl "http://localhost:3131/search?q=meeting"
```

---

### Storage Running Out

**Symptoms**: Disk space full, application crashes

**Solutions**:
→ Enable automatic cleanup:
```toml
[cleanup]
enabled = true
retention_days = 7  # Reduce from 30
cleanup_on_startup = true
```

→ Enable JPEG compression:
```toml
[storage]
format = "jpeg"
jpeg_quality = 70  # Reduce from 80 for smaller files
max_width = 1280   # Reduce from 1920
```

→ Manual cleanup:
```bash
# Delete frames older than 7 days
sqlite3 screensearch.db "DELETE FROM frames WHERE timestamp < datetime('now', '-7 days');"

# Vacuum database to reclaim space
sqlite3 screensearch.db "VACUUM;"
```

→ Change storage location:
```toml
[database]
path = "D:\\screensearch\\screensearch.db"  # Move to different drive
```

---

## File Locations

### Windows Paths

```
Application Binary:
  C:\Users\<username>\Desktop\screensearch\target\release\screensearch.exe

Configuration:
  C:\Users\<username>\Desktop\screensearch\config.toml

Database:
  C:\Users\<username>\Desktop\screensearch\screensearch.db
  (or path specified in config.toml)

Logs:
  C:\Users\<username>\Desktop\screensearch\screensearch.log

Captured Images:
  (stored as BLOBs in database, not separate files in v0.2.0+)

Embedding Models:
  C:\Users\<username>\.screensearch\models\
  └── paraphrase-multilingual-MiniLM-L12-v2\
      ├── model.onnx
      ├── tokenizer.json
      └── config.json

Web UI (Development):
  C:\Users\<username>\Desktop\screensearch\screensearch-ui\

Web UI (Production/Embedded):
  (embedded in screensearch.exe binary)
```

### Temporary Files

```
ONNX Runtime Cache:
  C:\Users\<username>\AppData\Local\Temp\onnxruntime\

Log Rotation:
  screensearch.log
  screensearch.log.1
  screensearch.log.2
  ...
```

---

## Performance Targets

| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| **OCR Processing** | < 100ms | 70-80ms | ✓ Excellent |
| **API Response** | < 100ms | ~50ms | ✓ Excellent |
| **Vector Search** | < 200ms | ~150ms | ✓ Good |
| **CPU (idle)** | < 5% | ~2% | ✓ Excellent |
| **Memory Usage** | < 500MB | ~240MB | ✓ Excellent |
| **Storage (JPEG)** | 50x reduction | 98% less | ✓ Excellent |
| **Test Coverage** | 100% | 59/59 pass | ✓ Complete |

### Performance Tuning

```toml
# Low-end systems (4GB RAM, dual-core)
[capture]
interval_ms = 5000
[ocr]
worker_threads = 1
[embeddings]
enabled = false

# High-end systems (16GB+ RAM, 8+ cores)
[capture]
interval_ms = 2000
[ocr]
worker_threads = 4
[embeddings]
enabled = true
batch_size = 100
```

---

## Keyboard Shortcuts (Web UI)

```
Global:
  Ctrl + K       → Focus search bar
  Ctrl + /       → Open command palette
  Esc            → Close modals/dialogs

Timeline Page:
  ← →            → Navigate frames
  Space          → Play/pause timeline
  F              → Toggle fullscreen
  I              → Open frame inspector

Search Page:
  Enter          → Execute search
  Ctrl + Enter   → Advanced search
  Alt + F        → Add filter

Settings Page:
  Ctrl + S       → Save settings
  Ctrl + R       → Reset to defaults
```

---

## Support

### Documentation

- **User Guide**: `docs/user-guide.md` - Installation and usage
- **API Reference**: `docs/api-reference.md` - Complete endpoint docs
- **Architecture**: `docs/architecture.md` - System design deep dive
- **Developer Guide**: `docs/developer-guide.md` - Contributing and development

### Online Resources

- **GitHub Repository**: https://github.com/nicolasestrem/screensearch
- **Issue Tracker**: https://github.com/nicolasestrem/screensearch/issues
- **Releases**: https://github.com/nicolasestrem/screensearch/releases

### Getting Help

1. **Check Logs**: `screensearch.log` for error messages
2. **Search Issues**: GitHub issues for similar problems
3. **Create Issue**: Provide logs, config, and steps to reproduce
4. **API Debugging**: Use `curl -v` for verbose output

### Useful Commands

```bash
# View live logs
Get-Content screensearch.log -Wait -Tail 50

# Check Rust version
rustc --version

# Check Windows version
winver

# Test API connectivity
curl -v "http://localhost:3131/health"

# Database shell
sqlite3 screensearch.db

# Clean rebuild
cargo clean && cargo build --release
```

---

**ScreenSearch v0.2.0** - Your screen history, searchable and automated

**Last Updated**: 2025-12-13
