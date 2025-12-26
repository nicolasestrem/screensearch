# ScreenSearch - Comprehensive Project Index

## 📋 Table of Contents

1. [Project Overview](#project-overview)
2. [Architecture Overview](#architecture-overview)
3. [Workspace Structure](#workspace-structure)
4. [Core Components](#core-components)
5. [API Documentation](#api-documentation)
6. [Development Guides](#development-guides)
7. [Configuration](#configuration)
8. [Quick Navigation](#quick-navigation)

---

## Project Overview

**ScreenSearch** is a Windows-based screen capture and OCR system that continuously records your screen, extracts text using Windows OCR API, and provides a powerful REST API for searching and automating your screen history.

### Key Capabilities

- **Continuous Screen Capture**: Configurable intervals (2-5 seconds) with multi-monitor support
- **OCR Text Extraction**: Windows OCR API with bounding box coordinates and confidence scores
- **Full-Text Search**: FTS5-powered search with BM25 ranking across all captured text
- **REST API**: 27 endpoints for search, automation, and tag management on localhost:3131
- **Timeline Visualization**: Activity density graph showing daily screen usage patterns
- **System Tray Integration**: Background operation with quick access menu (Open/Quit)
- **UI Automation**: Programmatic control of Windows applications via accessibility APIs
- **Privacy Controls**: Exclude sensitive applications, pause on screen lock

### Technology Stack

- **Language**: Rust 1.70+ (Edition 2021)
- **Platform**: Windows 10/11
- **Runtime**: Tokio async runtime
- **Database**: SQLite with FTS5 full-text search
- **Web Server**: Axum framework
- **UI Framework**: React (web dashboard)
- **Build System**: Cargo workspace

---

## Architecture Overview

### System Architecture

ScreenSearch follows a modular architecture with four main workspace crates:

```
┌─────────────────────────────────────────────────────┐
│              Main Binary (screensearch)          │
│  Orchestrates all services and handles lifecycle    │
└──────────┬──────────────┬──────────────┬───────────┘
           │              │              │
    ┌──────▼─────┐ ┌─────▼──────┐ ┌─────▼──────┐
    │screen-     │ │screen-     │ │screen-     │
    │capture     │ │db          │ │api         │
    └──────┬─────┘ └─────┬──────┘ └─────┬──────┘
           │              │              │
    ┌──────▼─────────────────────────────▼───────┐
    │         screen-automation                   │
    │    Windows UI Automation & Input Control    │
    └────────────────────────────────────────────┘
```

### Data Flow Pipeline

```
Screen Capture → Frame Differencing → OCR Processing → Database Storage → API Queries
     (2-5s)          (Arc-based)      (Windows OCR)      (SQLite+FTS5)   (Axum REST)
```

### Performance Characteristics

| Metric | Target | Actual | Status |
|--------|--------|--------|--------|
| **OCR Processing** | < 100 ms | 70-80 ms | ✅ 20-30% faster |
| **CPU Usage (idle)** | < 5% | ~2% | ✅ Excellent |
| **Memory** | < 500 MB | ~240 MB | ✅ 52% under target |
| **API Response** | < 100 ms | ~50 ms | ✅ 2x faster |

---

## Workspace Structure

### Workspace Members

The project uses a Cargo workspace with 5 member crates:

#### 1. **screen-capture** - Screen Capture & OCR Engine
- **Path**: `screen-capture/`
- **Purpose**: Continuous screen capture with frame differencing and OCR processing
- **Key Files**:
  - `src/capture.rs` - Frame capture with ffmpeg-sidecar
  - `src/ocr.rs` - Windows OCR API integration
  - `src/ocr_processor.rs` - Multi-threaded OCR pipeline
  - `src/frame_diff.rs` - Zero-copy frame differencing (Arc-based)
  - `src/monitor.rs` - Multi-monitor detection and selection
  - `src/window_context.rs` - Active window tracking

#### 2. **screen-db** - Database Layer
- **Path**: `screen-db/`
- **Purpose**: SQLite database with FTS5 full-text search
- **Key Files**:
  - `src/db.rs` - Database connection pool and manager
  - `src/queries.rs` - SQL queries and FTS5 search
  - `src/models.rs` - Database models (Frame, OcrText, Tags)
  - `src/migrations.rs` - Schema versioning and migrations

#### 3. **screen-api** - REST API Server
- **Path**: `screen-api/`
- **Purpose**: Axum-based REST API on localhost:3131
- **Key Files**:
  - `src/server.rs` - Axum server initialization
  - `src/routes.rs` - API endpoint routing (27 endpoints)
  - `src/handlers/` - Request handlers
    - `search.rs` - Search and query endpoints
    - `automation.rs` - UI automation endpoints
    - `system.rs` - System health and stats
  - `src/state.rs` - Shared application state
  - `src/models.rs` - API request/response models

#### 4. **screen-automation** - Windows UI Automation
- **Path**: `screen-automation/`
- **Purpose**: Programmatic control of Windows applications
- **Key Files**:
  - `src/engine.rs` - Automation orchestration
  - `src/element.rs` - UI element detection and interaction
  - `src/input.rs` - Mouse and keyboard control
  - `src/window.rs` - Window management
  - `src/selector.rs` - Element selector patterns

#### 5. **screensearch-embeddings** - Vector Embeddings Engine
- **Path**: `screensearch-embeddings/`
- **Purpose**: Local ML embedding generation using ONNX Runtime for semantic search
- **Key Files**:
  - `src/engine.rs` - ONNX embedding engine with batch processing
  - `src/chunker.rs` - Text chunking with configurable overlap
  - `src/download.rs` - Automatic model download from HuggingFace

#### 6. **Main Binary** - Application Entry Point
- **Path**: `src/main.rs`
- **Purpose**: Integrates all workspace crates into a single executable
- **Responsibilities**:
  - Configuration loading (config.toml)
  - Service initialization and orchestration
  - Graceful shutdown handling
  - Logging setup

---

## Core Components

### Capture Engine

**Location**: `screen-capture/src/capture.rs`

```rust
pub struct CaptureEngine {
    config: CaptureConfig,
    frame_buffer: Arc<Mutex<VecDeque<CapturedFrame>>>,
    previous_frame: Option<Arc<RgbaImage>>,
    // ...
}
```

**Key Features**:
- Multi-monitor support with configurable indices
- Frame differencing to skip duplicate captures (0.6% threshold)
- Arc-based memory sharing (eliminates 39GB/8hr overhead)
- Configurable capture intervals (2-5 seconds)

**Configuration**: `src/main.rs:101-109`

### OCR Processor

**Location**: `screen-capture/src/ocr_processor.rs`

```rust
pub struct OcrProcessor {
    config: OcrProcessorConfig,
    worker_handles: Vec<JoinHandle<()>>,
    metrics: Arc<Mutex<OcrMetrics>>,
    // ...
}
```

**Key Features**:
- Multi-threaded worker pool (default: 2 threads)
- Zero-copy `SoftwareBitmap` creation (60-93ms savings per frame)
- Confidence-based filtering (default: 0.7 threshold)
- Automatic retry with exponential backoff
- Real-time metrics and performance tracking

**Configuration**: `src/main.rs:110-120`

### Database Manager

**Location**: `screen-db/src/db.rs`

```rust
pub struct DatabaseManager {
    pool: SqlitePool,
    config: DatabaseConfig,
}
```

**Key Features**:
- SQLite with WAL mode for concurrent access
- FTS5 full-text search with BM25 ranking
- Connection pooling (3-50 connections)
- Automatic schema migrations
- Query sanitization to prevent injection attacks

**Schema**:
```sql
-- frames table
CREATE TABLE frames (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    device_name TEXT,
    file_path TEXT NOT NULL,
    monitor_index INTEGER,
    width INTEGER,
    height INTEGER,
    active_window TEXT,
    active_process TEXT
);

-- ocr_text table with FTS5
CREATE VIRTUAL TABLE ocr_text_fts USING fts5(
    text,
    content='ocr_text',
    content_rowid='id'
);
```

**Configuration**: `src/main.rs:126-133`

### API Server

**Location**: `screen-api/src/server.rs`

```rust
pub struct ApiServer {
    config: ApiConfig,
    db: Arc<DatabaseManager>,
}
```

**Endpoints** (27 total):
- **Search**: `/search`, `/search/advanced`
- **Frames**: `/frames`, `/frames/:id`
- **Automation**: `/automation/click`, `/automation/type`, `/automation/find-elements`
- **System**: `/health`, `/stats`, `/metrics`
- **Tags**: `/tags`, `/tags/:id`

**Configuration**: `src/main.rs:121-125`

---

## API Documentation

### Complete API Reference

See [docs/api-reference.md](./api-reference.md) for detailed endpoint documentation.

### Quick Examples

#### Search Your Screen History
```bash
# Basic search
curl "http://localhost:3131/search?q=meeting&limit=10"

# Advanced search with filters
curl "http://localhost:3131/search?q=meeting&app=Chrome&start=2025-12-10"
```

#### Automate Desktop Interactions
```bash
# Click at coordinates
curl -X POST http://localhost:3131/automation/click \
  -H "Content-Type: application/json" \
  -d '{"x":100,"y":200,"button":"left"}'

# Type text
curl -X POST http://localhost:3131/automation/type \
  -H "Content-Type: application/json" \
  -d '{"text":"Hello, World!"}'
```

---

## Development Guides

### Quick Start

See [docs/user-guide.md](./user-guide.md) for installation and setup.

### Development Setup

See [docs/developer-guide.md](./developer-guide.md) for:
- Development environment setup
- Building and running tests
- Code organization and patterns
- Contribution guidelines

### Architecture Deep Dive

See [docs/architecture.md](./architecture.md) for:
- System design decisions
- Data flow diagrams
- Performance optimizations
- Future roadmap

### Testing

See [docs/testing.md](./testing.md) for:
- Test coverage (59/59 passing)
- Integration test setup
- Performance benchmarks

---

## Configuration

### Configuration File

**Location**: `config.toml` (created by user, falls back to defaults)

**Structure**:
```toml
[capture]
interval_ms = 3000
enable_frame_diff = true
diff_threshold = 0.006
max_frames_buffer = 30
monitor_indices = []
include_cursor = true
draw_border = false

[ocr]
engine = "windows"
min_confidence = 0.7
worker_threads = 2
max_retries = 3
retry_backoff_ms = 1000
store_empty_frames = false
channel_buffer_size = 100
enable_metrics = true
metrics_interval_secs = 60

[api]
host = "127.0.0.1"
port = 3131
cors_origin = ""

[database]
path = "screensearch.db"
max_connections = 50
min_connections = 3
acquire_timeout_secs = 10
enable_wal = true
cache_size_kb = -2000

[privacy]
excluded_apps = ["1Password", "KeePass", "Bitwarden", "LastPass", "Password", "Bank"]
pause_on_lock = true

[performance]
max_cpu_percent = 5
max_memory_mb = 500

[logging]
level = "info"
log_to_file = true
log_file = "screensearch.log"
max_log_size_mb = 100
log_rotation_count = 5
```

**Defaults**: See `src/main.rs:98-158`

---

## Quick Navigation

### By Use Case

**I want to...**

- **Install and run the app** → [User Guide](./user-guide.md)
- **Understand the architecture** → [Architecture](./architecture.md)
- **Contribute code** → [Developer Guide](./developer-guide.md)
- **Use the API** → [API Reference](./api-reference.md)
- **Run tests** → [Testing Guide](./testing.md)
- **View commands** → [Commands Summary](./commands-summary.md)

### By Component

| Component | Source Code | Documentation |
|-----------|-------------|---------------|
| **Capture Engine** | `screen-capture/src/capture.rs` | [Architecture](./architecture.md#capture-engine) |
| **OCR Processor** | `screen-capture/src/ocr_processor.rs` | [Architecture](./architecture.md#ocr-pipeline) |
| **Database** | `screen-db/src/db.rs` | [Architecture](./architecture.md#database-layer) |
| **API Server** | `screen-api/src/server.rs` | [API Reference](./api-reference.md) |
| **Automation** | `screen-automation/src/engine.rs` | [API Reference](./api-reference.md#automation) |
| **Main Binary** | `src/main.rs` | [Developer Guide](./developer-guide.md) |

### By File Type

**Source Files**:
```
src/main.rs                          # Application entry point
screen-capture/src/*.rs              # Capture and OCR
screen-db/src/*.rs                   # Database layer
screen-api/src/*.rs                  # REST API
screen-automation/src/*.rs           # Windows automation
```

**Documentation**:
```
docs/
├── README.md                        # Documentation index
├── user-guide.md                    # Installation and usage
├── developer-guide.md               # Development setup
├── architecture.md                  # System design
├── api-reference.md                 # API endpoints
├── testing.md                       # Test protocols
├── commands-summary.md              # CLI commands
├── performance-optimizations.md     # Performance details
└── archived/                        # Historical docs
```

**Configuration**:
```
config.toml                          # User configuration
Cargo.toml                           # Workspace manifest
screen-*/Cargo.toml                  # Crate manifests
```

---

## Dependencies

### Workspace Dependencies

```toml
[workspace.dependencies]
# Async runtime
tokio = { version = "1.35", features = ["full"] }
futures = "0.3"

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Time handling
chrono = { version = "0.4", features = ["serde"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Database
sqlx = { version = "0.7", features = ["runtime-tokio", "sqlite"] }

# HTTP server
axum = "0.7"
tower-http = { version = "0.5", features = ["cors", "trace"] }

# Windows-specific
windows = "0.52"
uiautomation = "0.16.1"
windows-capture = "2.0.0-alpha.7"
tray-icon = "0.14.3"
winit = "0.29.15"

# Image processing
image = "0.24"

# Concurrency
crossbeam = "0.8"
```

---

## Performance Optimizations

### Recent Optimizations

**[+] Zero-Copy OCR Pipeline**
- **Location**: `screen-capture/src/ocr.rs`
- **Improvement**: Direct `SoftwareBitmap` creation eliminates PNG encoding/decoding
- **Impact**: 60-93ms savings per frame (53% faster)
- **Enables**: 1-second capture intervals

**[+] Memory Efficiency**
- **Location**: `screen-capture/src/frame_diff.rs`
- **Improvement**: Arc-based frame differencing eliminates redundant allocations
- **Impact**: Memory pressure reduced from 39GB/8hr → <1GB/8hr

**[+] Search Security**
- **Location**: `screen-db/src/queries.rs`
- **Improvement**: FTS5 query sanitization
- **Impact**: Prevents injection attacks while handling special characters

See [docs/performance-optimizations.md](./performance-optimizations.md) for detailed analysis.

---

## Project Statistics

### Codebase Metrics

- **Total Source Files**: 37 Rust files
- **Workspace Crates**: 5 members + main binary
- **API Endpoints**: 29 REST endpoints
- **Test Coverage**: 59/59 tests passing (100%)
- **Lines of Code**: ~5,000+ lines (excluding dependencies)

### Build Configuration

**Release Profile**:
```toml
[profile.release]
opt-level = 3          # Maximum optimizations
lto = true             # Link-time optimization
codegen-units = 1      # Single codegen unit
strip = true           # Strip debug symbols
```

**Development Profile**:
```toml
[profile.dev]
opt-level = 0          # Fast compilation
debug = true           # Full debug info
```

---

## License

This project is licensed under the **MIT License** — see the [LICENSE](../LICENSE) file for details.

---

## Contributing

We welcome contributions! See the [Developer Guide](./developer-guide.md) for:
1. Development environment setup
2. Code organization and patterns
3. Testing requirements
4. Pull request process

---

## Support

- **Issues**: [GitHub Issues](https://github.com/nicolasestrem/screensearch/issues)
- **Documentation**: This directory (`docs/`)
- **API Reference**: [docs/api-reference.md](./api-reference.md)

---

**Last Updated**: 2025-12-13
**Version**: 0.2.0
**Maintainer**: ScreenSearch Project
