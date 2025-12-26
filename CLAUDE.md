# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ScreenSearch is a Windows-native screen capture and OCR system with a REST API. The application continuously captures screens at configurable intervals, performs OCR using Windows APIs, stores results in SQLite with FTS5 full-text search + vector embeddings for semantic search, and exposes REST API endpoints for querying, automation, and AI-powered intelligence.

**Platform**: Windows 10/11 only (uses Windows-specific APIs)
**Language**: Rust 2021 edition
**Architecture**: Cargo workspace with 5 member crates + main binary + React UI

## Build & Development Commands

### Building

```bash
# Build all workspace crates
cargo build --workspace

# Build release binary (production-ready)
cargo build --release

# Build specific workspace crate
cargo build -p screensearch-capture
cargo build -p screensearch-db
cargo build -p screensearch-api
cargo build -p screensearch-automation
cargo build -p screensearch-embeddings

# Check compilation without building (fast)
cargo check --workspace

# Skip UI build during development (faster iteration)
SKIP_UI_BUILD=1 cargo build
```

**IMPORTANT**: Release builds automatically trigger UI build via `build.rs`. The build script:
- Detects if `npm` is available
- Runs `npm install` in `screensearch-ui/`
- Runs `npm run build` to create production bundle
- Embeds UI assets into binary via `rust-embed`
- Set `SKIP_UI_BUILD=1` to skip UI build (serves "UI not built" page)

### Running

```bash
# Run the main application (starts API on localhost:3131, serves embedded web UI)
cargo run --release

# Run with debug logging
RUST_LOG=debug cargo run

# Run API server standalone
cargo run -p screensearch-api

# Build web UI for production (will be embedded in binary)
cd screensearch-ui && npm install && npm run build

# Run web dashboard in development mode (hot-reload on port 5173)
cd screensearch-ui && npm run dev
```

**Note**: The production web UI is embedded in the binary at compile time using `rust-embed` and automatically served at `http://localhost:3131/`. During development, the UI can be run with `npm run dev` for hot-reload on port 5173. The binary is fully portable with no runtime UI dependencies.

### Testing

```bash
# Run all tests (unit + integration)
cargo test --workspace

# Run tests for specific crate
cargo test -p screensearch-db
cargo test -p screensearch-capture
cargo test -p screensearch-embeddings

# Run specific test by name
cargo test test_fts5_search

# Run tests with output visible
cargo test --workspace -- --nocapture

# Run examples
cargo run --example ocr_demo -p screensearch-capture
cargo run --example element_search -p screensearch-automation
```

### Linting & Formatting

```bash
# Format all code
cargo fmt --all

# Check formatting without changes
cargo fmt --all -- --check

# Run clippy lints
cargo clippy --workspace -- -D warnings

# Fix clippy suggestions automatically
cargo clippy --workspace --fix
```

### Release Build Process

**Windows only** - requires PowerShell, Inno Setup, and npm:

```powershell
# Full release build (creates installers + portable ZIP)
.\scripts\build-release.ps1 -Version 0.2.0

# Skip model download (creates lite installer only)
.\scripts\build-release.ps1 -Version 0.2.0 -SkipModel

# Clean build from scratch
.\scripts\build-release.ps1 -Version 0.2.0 -Clean
```

**Build artifacts**:
- `ScreenSearch-v{version}-Setup-Full.exe` - Includes 449MB ONNX model
- `ScreenSearch-v{version}-Setup-Lite.exe` - Downloads model on first run if embeddings enabled
- `ScreenSearch-v{version}-Portable.zip` - Standalone executable + config

### Cross-Compilation (Linux to Windows)

**Linux only** - requires cargo-xwin, npm, clang, lld:

ScreenSearch can be cross-compiled from Linux to Windows using `cargo-xwin`. This enables building Windows executables without needing a Windows machine, useful for CI/CD and Linux-based development workflows.

**Prerequisites**:
```bash
# Install system dependencies (Ubuntu/Debian)
sudo apt-get install -y clang lld llvm

# Install cross-compilation tools
cargo install cargo-xwin
rustup target add x86_64-pc-windows-msvc

# Verify installation
cargo xwin --version
```

**Build Commands**:
```bash
# Build UI first (runs on Linux, platform-agnostic)
cd screensearch-ui && npm install && npm run build && cd ..

# Cross-compile to Windows (x86_64-pc-windows-msvc)
cargo xwin build --release --target x86_64-pc-windows-msvc

# Output binary location
# target/x86_64-pc-windows-msvc/release/screensearch.exe

# For faster iteration, skip UI build
SKIP_UI_BUILD=1 cargo xwin build --release --target x86_64-pc-windows-msvc

# Check compilation without full build (fast validation)
cargo xwin check --target x86_64-pc-windows-msvc --workspace
```

**Build artifacts**:
- `screensearch.exe` - Windows executable (portable, requires Windows to run)

**Testing**:
- Compilation can be verified on Linux
- Runtime testing requires actual Windows machine
- Transfer `.exe` to Windows for full validation
- CI/CD workflow includes optional Windows smoke test

**Important Notes**:
- The resulting binary is identical to native Windows builds (same MSVC ABI)
- All Windows APIs remain functional (OCR, screen capture, UI automation)
- Cross-compilation does not enable running on Linux - the binary still requires Windows
- See `docs/cross-compilation.md` for troubleshooting and detailed guide

## Workspace Architecture

This is a **Cargo workspace** with a specific dependency flow. Understanding this structure is critical:

```
Main Binary (src/main.rs)
    ├─> screensearch-capture (capture + OCR)
    ├─> screensearch-db (SQLite database + vector search)
    ├─> screensearch-api (REST API server)
    ├─> screensearch-automation (Windows UI automation)
    └─> screensearch-embeddings (ONNX embeddings engine)
```

### Workspace Members

1. **screensearch-capture** (`screensearch-capture/`)
   - Screen capture engine with multi-monitor support
   - Windows OCR API integration (WinRT COM via windows-rs)
   - Zero-copy frame differencing using Arc<RgbaImage>
   - Multi-threaded OCR processing pipeline
   - JPEG compression and image resizing for storage optimization

2. **screensearch-db** (`screensearch-db/`)
   - SQLite database manager with connection pooling
   - FTS5 full-text search with BM25 ranking (sparse search)
   - In-memory vector search with cosine similarity (dense/semantic search)
   - Query sanitization to prevent FTS5 operator injection
   - Schema migrations with automatic cleanup policies
   - Frame retention and storage management

3. **screensearch-api** (`screensearch-api/`)
   - Axum HTTP server on localhost:3131
   - REST endpoints for search, automation, system, AI intelligence, embeddings
   - CORS middleware for web dashboard
   - Shared Arc state for database access
   - **Embedded UI assets** using rust-embed for portable binary
   - AI provider integration (Ollama, LM Studio, OpenAI-compatible APIs)
   - Hybrid search combining FTS5 + vector embeddings

4. **screensearch-automation** (`screensearch-automation/`)
   - Windows UIAutomation API wrapper
   - Mouse/keyboard input control
   - UI element finding and interaction
   - Window management

5. **screensearch-embeddings** (`screensearch-embeddings/`)
   - Local ML embedding generation using ONNX Runtime
   - HuggingFace tokenizers for text processing
   - Text chunking with configurable overlap
   - 384-dimensional embeddings (paraphrase-multilingual-MiniLM-L12-v2)
   - Batch processing for efficiency
   - Auto-downloads model from HuggingFace on first use

### Main Binary (`src/main.rs`)

The main binary orchestrates all services:

- **Configuration loading**: Reads `config.toml` or uses defaults
- **Service initialization**: Database → OCR → Capture → API → Embeddings worker
- **Frame processing pipeline**: Uses tokio channels for capture → OCR → database → embeddings flow
- **Graceful shutdown**: Broadcast channel for coordinated shutdown

**Critical**: All services run concurrently using tokio::spawn. The pipeline is:
```
CaptureEngine → frame_tx → OcrProcessor → processed_tx → DatabaseManager
                                                               ↓
                                                          Embeddings Worker (background)
                                                               ↓
                                                          ApiServer (reads)
```

**Embeddings Pipeline** (when enabled in config):
```
Background Worker (24h loop)
    ↓
Fetch frames without embeddings (batch: 50)
    ↓
Text Chunking (max 256 tokens, overlap: 32)
    ↓
ONNX Model Inference (384-dim vectors)
    ↓
Store embeddings in SQLite (BLOB)
```

## Performance-Critical Code

### Frame Differencing (screensearch-capture/src/frame_diff.rs)

Uses **Arc-based zero-copy** to avoid 39GB/8hr memory overhead:

```rust
// IMPORTANT: Always use Arc::clone, never clone the image directly
let previous = Arc::clone(&self.previous_frame);
```

**Why**: Cloning RgbaImage copies all pixel data. Arc-based sharing eliminates this.

### OCR Pipeline (screensearch-capture/src/ocr.rs)

Direct `SoftwareBitmap` creation saves **60-93ms per frame**:

```rust
// CRITICAL: Do NOT encode to PNG then decode
// Directly create SoftwareBitmap from RgbaImage bytes
SoftwareBitmap::Create(
    BitmapPixelFormat::Rgba8,
    width, height
)?;
```

**Why**: Old approach: RgbaImage → PNG → IBuffer → SoftwareBitmap (93ms)
New approach: RgbaImage → SoftwareBitmap (30ms)

### FTS5 Query Sanitization (screensearch-db/src/queries.rs)

**ALWAYS** sanitize user queries before FTS5 search to prevent operator injection:

```rust
// REQUIRED: Sanitize before FTS5 MATCH
let sanitized = sanitize_fts5_query(&user_input);
// Then: WHERE ocr_text_fts MATCH ?
```

**Why**: Unescaped queries with `AND`, `OR`, `*`, `"` can break FTS5 or enable injection.

### Vector Search (screensearch-db/src/vector_search.rs)

**In-memory cosine similarity** for semantic search (bypasses SQLite extension limitations on Windows):

```rust
// IMPORTANT: All embeddings loaded into memory for fast similarity search
// Trade-off: Fast (<200ms) but O(n) complexity
// Works well for <100K frames (~38MB memory for 100K embeddings)
let index = VectorIndex::build(&db).await?;
let results = index.search(&query_vector, limit).await?;
```

**Why**: SQLite vector extensions (vec0, vss) have compilation issues on Windows. In-memory approach is simpler and performant for expected scale (<1M frames).

### Storage Optimization (screensearch-capture/src/lib.rs)

**JPEG compression and resizing** reduces storage by 50x:

```rust
// IMPORTANT: Images are stored as JPEG with configurable quality
// Frames are resized to max_width (default 1920px) to reduce storage
let jpeg_quality = config.jpeg_quality; // Default: 80
let max_width = config.max_width; // Default: 1920
```

**Why**: Original PNG storage at full resolution consumed massive disk space. JPEG compression (80% quality) + resizing provides excellent quality at 2% of original size.

**Automatic cleanup**: Database runs cleanup every 24 hours to enforce retention policies (configurable in settings).

## Configuration System

Configuration is loaded from `config.toml` (optional, falls back to defaults in `src/main.rs`).

**Structure**:
```toml
[capture]
interval_ms = 3000              # Capture every 3 seconds
enable_frame_diff = true        # Skip unchanged frames
diff_threshold = 0.006          # 0.6% pixel change threshold
monitor_indices = []            # Empty = all monitors, or [0, 1] for specific monitors

[ocr]
min_confidence = 0.7            # Filter OCR results below 70% confidence
worker_threads = 2              # OCR processing threads

[database]
path = "screensearch.db"
enable_wal = true               # Write-Ahead Logging for concurrency
cache_size_kb = -2000           # 2MB cache (negative = KB)

[storage]
format = "jpeg"                 # Storage format (jpeg recommended)
jpeg_quality = 80               # JPEG quality 1-100 (80 = excellent quality, small size)
max_width = 1920                # Resize frames to max width (maintains aspect ratio)

[embeddings]
enabled = false                 # Enable semantic search with embeddings
batch_size = 50                 # Frames to process per batch
model = "local"                 # Model type (currently only "local" supported)
model_name = "paraphrase-multilingual-MiniLM-L12-v2"
embedding_dim = 384             # Embedding vector dimensions
max_chunk_tokens = 256          # Max tokens per text chunk
chunk_overlap = 32              # Token overlap between chunks
hybrid_search_alpha = 0.3       # Weight for FTS5 vs vector (0.0 = pure vector, 1.0 = pure FTS5)

[api]
port = 3131
host = "127.0.0.1"
auto_open_browser = true        # Automatically open browser on startup

[logging]
level = "info"                  # Log level: error, warn, info, debug, trace
log_to_file = true              # Enable file logging with rotation
log_file = "screensearch.log"   # Log file path (relative or absolute)
log_rotation_count = 5          # Number of daily log files to keep

[privacy]
excluded_apps = ["1Password", "KeePass", "Bitwarden"]
pause_on_lock = true
```

**File Logging**: Uses tracing-appender with daily rotation. Logs are rotated daily and kept for N days (configurable via `log_rotation_count`). Logs appear in both console and file when `log_to_file = true`.

**Tray Icon**: The application includes a system tray icon with the following interactions:
- **Left-click or double-click**: Opens web interface (http://localhost:3131)
- **Right-click**: Shows menu with "Open Interface" and "Quit ScreenSearch" options
- **Menu actions**: "Open Interface" opens browser, "Quit ScreenSearch" cleanly shuts down the application

**When modifying config**:
1. Update `AppConfig` structs in `src/main.rs` (struct definitions near top)
2. Update default in `impl Default for AppConfig`
3. Update conversion functions that map config to workspace crate configs
4. Test with both config.toml and default configuration

## Database Schema

**FTS5 Full-Text Search** (Sparse/Keyword):
```sql
-- Virtual table for fast text search
CREATE VIRTUAL TABLE ocr_text_fts USING fts5(
    text,
    content='ocr_text',
    content_rowid='id'
);

-- Triggers keep FTS5 in sync
CREATE TRIGGER ocr_text_after_insert ...
```

**Embeddings Table** (Dense/Semantic):
```sql
-- Stores 384-dim vectors for semantic search
CREATE TABLE embeddings (
    id INTEGER PRIMARY KEY,
    frame_id INTEGER NOT NULL,
    chunk_text TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    embedding BLOB NOT NULL,              -- Vec<f32> serialized to bytes
    model_name TEXT NOT NULL,
    embedding_dim INTEGER NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (frame_id) REFERENCES frames(id) ON DELETE CASCADE
);

CREATE INDEX idx_embeddings_frame_id ON embeddings(frame_id);
```

**IMPORTANT**: FTS5 requires special query syntax. NEVER pass raw user input to MATCH.

**Migrations**: Located in `screensearch-db/src/migrations.rs`. Runs automatically on startup.

## Hybrid Search System

ScreenSearch combines **sparse (FTS5)** and **dense (vector embeddings)** search for best results:

1. **FTS5 Search** (keyword matching):
   - Fast BM25 ranking
   - Exact phrase matching
   - Boolean operators (AND, OR, NOT)
   - Use for: Known keywords, specific terms

2. **Vector Search** (semantic similarity):
   - Cosine similarity on 384-dim embeddings
   - Understands context and meaning
   - Use for: Concepts, questions, paraphrases

3. **Hybrid Search** (combines both):
   - Weighted combination: `alpha * fts5_score + (1-alpha) * vector_score`
   - Default `alpha = 0.3` (70% semantic, 30% keyword)
   - Configure via `embeddings.hybrid_search_alpha` in config.toml

**API Usage**:
```bash
# Pure FTS5 (keyword search)
curl "http://localhost:3131/search?q=meeting"

# Hybrid search (if embeddings enabled)
curl "http://localhost:3131/search?q=meeting&use_embeddings=true"
```

## Windows API Integration

### OCR (WinRT COM)

```rust
use windows::Media::Ocr::OcrEngine;
use windows::Graphics::Imaging::SoftwareBitmap;

// REQUIRED: Initialize COM apartment (done in OcrProcessor::new)
// REQUIRED: Create SoftwareBitmap with BitmapPixelFormat::Rgba8
// REQUIRED: Handle RecognizeAsync().get() for async COM calls
```

**Critical**: All Windows OCR calls must happen on the same thread that initialized COM.

### UI Automation

```rust
use uiautomation::UIAutomation;

// IMPORTANT: UIAutomation requires STA (Single-Threaded Apartment)
// Never use UIAutomation from multiple threads simultaneously
```

## AI Intelligence System

ScreenSearch includes AI-powered intelligence endpoints that generate insights from captured screen history.

### Architecture

- **Provider-agnostic**: Works with any OpenAI-compatible API (Ollama, LM Studio, OpenAI)
- **Location**: `screensearch-api/src/handlers/ai.rs`
- **UI Integration**: `screensearch-ui/src/pages/IntelligencePage.tsx`

### Key Endpoints

```
POST /api/ai/validate              - Test AI provider connection
POST /api/ai/generate              - Generate intelligence report from time range
GET  /api/embeddings/status        - Check embedding generation progress
POST /api/embeddings/generate      - Trigger batch embedding generation
POST /api/embeddings/enable        - Enable/disable embeddings
```

### Usage Pattern

1. User configures AI provider in UI (provider URL, model name, API key if needed)
2. Frontend calls `/api/ai/validate` to test connection
3. User optionally enables embeddings for semantic search
4. User requests report (daily summary, custom query, etc.)
5. Frontend calls `/api/ai/generate` with time range and prompt
6. Backend fetches frames from database (using hybrid search if embeddings enabled)
7. AI generates summary/insights based on captured screen activity

**Important**: All AI processing happens via user-selected provider. No data is sent to external services without explicit configuration.

## Common Development Tasks

### Adding a New API Endpoint

1. Define route in `screensearch-api/src/routes.rs` → `build_router()`
2. Create handler in `screensearch-api/src/handlers/` (search.rs, automation.rs, system.rs, ai.rs, or embeddings.rs)
3. Add request/response models in `screensearch-api/src/models.rs`
4. Update `docs/api-reference.md`
5. Add integration test in `screensearch-api/tests/integration_tests.rs`

### Adding OCR Preprocessing

1. Modify `screensearch-capture/src/ocr.rs` → before `RecognizeAsync()`
2. Add preprocessing to `RgbaImage` (contrast, denoise, etc.)
3. Update metrics in `OcrMetrics` if tracking new stats
4. Test with `cargo run --example ocr_demo -p screensearch-capture`

### Optimizing Database Queries

1. Review query in `screensearch-db/src/queries.rs`
2. Check indexes in `screensearch-db/src/migrations.rs`
3. Use `EXPLAIN QUERY PLAN` in SQLite to analyze
4. Consider adding connection pool tuning in `src/main.rs` (database settings)

### Working with Embeddings

**Enable embeddings**:
1. Set `embeddings.enabled = true` in `config.toml`
2. First run downloads model from HuggingFace (449MB) to `%APPDATA%\ScreenSearch\models\`
3. Background worker processes frames in batches (default: 50 frames per batch)

**Generate embeddings for existing frames**:
```bash
curl -X POST "http://localhost:3131/api/embeddings/generate"
```

**Check embedding coverage**:
```bash
curl "http://localhost:3131/api/embeddings/status"
```

**Key files**:
- Embedding engine: `screensearch-embeddings/src/engine.rs`
- Text chunking: `screensearch-embeddings/src/chunker.rs`
- Model download: `screensearch-embeddings/src/download.rs`
- Vector search: `screensearch-db/src/vector_search.rs`

## Testing Notes

- **Integration tests** require Windows OCR language pack installed (Settings → Language → English)
- **screensearch-automation tests** may require interactive desktop session
- **screensearch-embeddings tests** download model on first run (~449MB)
- Use `#[ignore]` for tests that require specific Windows features
- Database tests use in-memory SQLite (`:memory:`)
- Run `cargo test --workspace` before committing to ensure all tests pass

## Documentation

**Primary docs**:
- `docs/PROJECT_INDEX.md` - Comprehensive project overview (START HERE)
- `docs/CODE_NAVIGATION.md` - Find code by feature
- `docs/architecture.md` - System architecture deep dive (includes embeddings architecture)
- `docs/security.md` - Security architecture and privacy controls
- `docs/api-reference.md` - REST API documentation (includes embeddings endpoints)
- `docs/developer-guide.md` - Development setup and workflow

**When updating code**:
- Update `docs/CODE_NAVIGATION.md` if file structure changes
- Update `docs/api-reference.md` if API endpoints change
- Update `docs/performance-optimizations.md` if performance characteristics change
- Update `docs/architecture.md` if embeddings pipeline changes

## Performance Targets

Maintain these performance characteristics:

| Metric | Target | Current |
|--------|--------|---------|
| OCR processing | < 100ms | 70-80ms |
| API response | < 100ms | ~50ms |
| Vector search | < 200ms | ~150ms |
| CPU (idle) | < 5% | ~2% |
| Memory | < 500MB | ~240MB |

**When optimizing**: Profile first using `cargo flamegraph` or `cargo bench`.

## Critical Constraints

1. **Windows-only**: No cross-platform abstractions. Use Windows APIs directly.
2. **Local-only**: No network calls except localhost API server and model downloads.
3. **Privacy-first**: All data stored locally in SQLite. No cloud uploads.
4. **Zero-copy**: Prefer Arc and reference counting over cloning large buffers.
5. **Async-first**: Use tokio for all I/O operations.
6. **Portable binary**: UI assets embedded via rust-embed, ONNX model auto-downloads on first use.

## Git Workflow

- Main branch: `main`
- Create feature branches: `feature/description`
- Current branch: Check with `git branch --show-current`
- Never commit to main directly - always use feature branches and PRs
- Run tests before committing: `cargo test --workspace`
- Format code before committing: `cargo fmt --all`
- Use clippy before committing: `cargo clippy --workspace -- -D warnings`

## Recent Optimizations (v0.1.0 → v0.2.0)

### Storage Optimization
- **50x storage reduction**: JPEG compression + resizing (1920px max width)
- **Automatic cleanup**: 24-hour loop enforces retention policies
- **Configurable quality**: `storage.jpeg_quality` in config.toml (default: 80)
- **Backward compatible**: Existing PNG frames work alongside new JPEG storage

**Implementation files**:
- Storage settings: `src/main.rs` → `StorageSettings` struct
- Compression logic: `screensearch-capture/src/lib.rs` → image resizing/JPEG encoding
- Cleanup loop: `src/main.rs` → automatic cleanup task with 24h interval

### Hybrid Search (v0.2.0)
- **Semantic search**: ONNX-based embeddings (384-dim vectors)
- **In-memory vector index**: Cosine similarity in Rust (bypasses SQLite extension issues on Windows)
- **Automatic model download**: First-run downloads from HuggingFace (449MB)
- **Background processing**: Worker processes frames in batches without blocking main pipeline
- **Hybrid ranking**: Combines FTS5 (keyword) + vector (semantic) scores

**Implementation files**:
- Embeddings engine: `screensearch-embeddings/src/engine.rs`
- Text chunking: `screensearch-embeddings/src/chunker.rs`
- Vector search: `screensearch-db/src/vector_search.rs`
- Embeddings API: `screensearch-api/src/handlers/embeddings.rs`

### Performance Improvements
- Zero-copy frame differencing (Arc-based)
- Direct SoftwareBitmap creation (60-93ms savings per frame)
- FTS5 query sanitization for security
- Connection pooling for database access
- In-memory vector search (150ms for 100K embeddings)
