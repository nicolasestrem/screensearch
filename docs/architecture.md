# ScreenSearch - System Architecture Documentation

**Version**: 0.2.0
**Last Updated**: 2025-12-13
**Status**: Production Ready

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Component Architecture](#2-component-architecture)
3. [RAG & Embeddings Architecture](#3-rag--embeddings-architecture)
4. [Database Schema](#4-database-schema)
5. [Data Flow](#5-data-flow)
6. [Concurrency Model](#6-concurrency-model)
7. [Configuration Architecture](#7-configuration-architecture)
8. [Error Handling Strategy](#8-error-handling-strategy)
9. [Performance Characteristics](#9-performance-characteristics)
10. [Security & Privacy Architecture](#10-security--privacy-architecture)
11. [Extension Points](#11-extension-points)

---

## 1. System Overview

ScreenSearch is a high-performance, locally-run screen capture and OCR system designed for Windows. The architecture emphasizes modularity, performance, and privacy through a workspace-based design with clear separation of concerns.

### 1.1 High-Level Architecture

```
┌──────────────────────────────────────────────────────────────────────────┐
│                          ScreenSearch System                           │
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                       Main Binary (src/main.rs)                   │   │
│  │  - Configuration loading (config.toml)                            │   │
│  │  - Service orchestration & lifecycle management                   │   │
│  │  - System Tray integration (tray-icon + winit event loop)         │   │
│  │  - Graceful shutdown handling (Ctrl+C signal)                     │   │
│  │  - Channel-based task coordination                                │   │
│  │  - Metrics aggregation & reporting                                │   │
│  └────┬──────────────┬───────────────┬────────────────┬─────────────┘   │
│       │              │               │                │                   │
│  ┌────▼─────┐  ┌────▼──────┐  ┌────▼──────┐  ┌─────▼──────┐            │
│  │ Capture  │  │    OCR    │  │ Database  │  │    API     │            │
│  │  Engine  │  │ Processor │  │  Manager  │  │   Server   │            │
│  │ (screen- │  │ (screen-  │  │ (screen-  │  │ (screen-   │            │
│  │ capture) │  │ capture)  │  │   db)     │  │   api)     │            │
│  └────┬─────┘  └────┬──────┘  └────┬──────┘  └─────┬──────┘            │
│       │             │               │               │                    │
└───────┼─────────────┼───────────────┼───────────────┼────────────────────┘
        │             │               │               │
        ▼             ▼               ▼               ▼
┌──────────────┐ ┌──────────────┐ ┌──────────┐ ┌──────────┐
│   Windows    │ │   Windows    │ │  SQLite  │ │   Axum   │
│  Graphics    │ │   OCR API    │ │ Database │ │   HTTP   │
│ Capture API  │ │ (WinRT COM)  │ │  (WAL)   │ │  Server  │
└──────────────┘ └──────────────┘ └──────────┘ └──────────┘
```

### 1.2 Component Summary

| Component | Crate | Language | Purpose |
|-----------|-------|----------|---------|
| Capture Engine | screen-capture | Rust | Screen capture with frame differencing |
| OCR Processor | screen-capture | Rust | Text extraction using Windows OCR API |
| Database Manager | screen-db | Rust | SQLite storage with FTS5 search |
| API Server | screen-api | Rust | REST API on localhost:3131 |
| UI Automation | screen-automation | Rust | Windows UIAutomation for control |
| Main Binary | src/main.rs | Rust | Service orchestration & lifecycle |
| Embedding Engine | screensearch-embeddings | Rust | Vector embedding generation (ONNX) |

### 1.3 Technology Stack

- **Runtime**: Tokio async runtime (multi-threaded)
- **Database**: SQLite 3.x with WAL mode, FTS5, sqlx ORM
- **HTTP Server**: Axum (tokio-based web framework)
- **Windows Integration**: windows-rs crate for WinRT/COM APIs
- **Image Processing**: image crate (RgbaImage format)
- **Logging**: tracing + tracing-subscriber

---

## 2. Component Architecture

### 2.1 Capture Engine (screen-capture crate)

**Purpose**: Continuously capture screen content with intelligent frame differencing to minimize redundant storage.

**Key Components**:

```
CaptureEngine
├─> ScreenCapture (platform-specific capture)
├─> FrameDiffer (change detection algorithm)
├─> MonitorInfo (multi-monitor enumeration)
└─> WindowContext (active window/process tracking)
```

**Configuration**:
```rust
pub struct CaptureConfig {
    interval_ms: u64,              // Capture every N milliseconds (default: 3000)
    monitor_indices: Vec<usize>,   // Which monitors to capture (empty = all)
    enable_frame_diff: bool,       // Enable change detection (default: true)
    diff_threshold: f32,           // Change threshold 0.0-1.0 (default: 0.006)
    max_frames_buffer: usize,      // Frame queue size (default: 30)
    include_cursor: bool,          // Include mouse cursor (default: true)
    draw_border: bool,             // Draw capture border (default: false)
}
```

**Frame Differencing Algorithm**:

```
Input: previous_frame, current_frame
Output: has_changed (bool)

1. For each pixel position (x, y):
   a. Compare RGB values
   b. If abs(diff) > threshold: increment changed_pixels

2. change_ratio = changed_pixels / total_pixels

3. Return change_ratio > diff_threshold (0.006 = 0.6% change)

Optimization: Use 4-pixel stride for faster comparison
```

**Data Flow**:

```
Monitor Detection
    ↓
Capture Loop (interval: 3s)
    ↓
Screen Capture → Get Active Window Context
    ↓
Frame Differencing
    ├─> Unchanged → Skip (log stats)
    └─> Changed → Queue for OCR
```

**Performance Characteristics**:
- Capture latency: 10-50ms per frame (depends on resolution)
- Memory: ~50MB for frame buffer (30 frames @ 1920x1080)
- CPU: 1-2% idle usage
- Frame difference check: ~5ms per comparison

**API**:
```rust
// Create and configure
let mut engine = CaptureEngine::new(config)?;

// Start background capture
engine.start()?;

// Poll for new frames (non-blocking)
while let Some(frame) = engine.try_get_frame() {
    // Process frame
}

// Stop capture
engine.stop()?;
```

### 2.2 OCR Processor (screen-capture crate)

**Purpose**: Extract text from captured frames using Windows OCR API with high throughput and accuracy.

**Architecture**:

```
OcrProcessor (orchestrator)
├─> Worker Pool (configurable, default: 2 workers)
│   ├─> Worker 1: OcrEngine (Windows.Media.Ocr)
│   └─> Worker 2: OcrEngine (Windows.Media.Ocr)
├─> Input Channel: CapturedFrame queue
├─> Output Channel: ProcessedFrame queue
└─> OcrMetrics: Performance tracking
```

**Processing Pipeline**:

```
CapturedFrame
    ↓
Worker Pool Distribution (async spawn)
    ↓
Windows OCR API Processing:
    1. RgbaImage → PNG encoding (in-memory)
    2. PNG bytes → IRandomAccessStream
    3. Stream → SoftwareBitmap decoding
    4. SoftwareBitmap → OCR recognition
    5. Extract lines + words + bounding boxes
    ↓
Confidence Filtering (min: 0.7)
    ├─> Low confidence → Discard
    └─> High confidence → Keep
    ↓
ProcessedFrame (with OCR result)
    ↓
Database Storage
```

**Key Types**:

```rust
pub struct TextRegion {
    text: String,        // Extracted text content
    x: u32,             // Top-left X coordinate
    y: u32,             // Top-left Y coordinate
    width: u32,         // Region width in pixels
    height: u32,        // Region height in pixels
    confidence: f32,    // Confidence score (0.0-1.0)
}

pub struct OcrResult {
    regions: Vec<TextRegion>,           // All text regions
    full_text: String,                  // Combined text (space-separated)
    processing_time_ms: u64,            // Time taken
    image_dimensions: (u32, u32),       // Source image size
}

pub struct ProcessedFrame {
    frame: CapturedFrame,      // Original capture
    ocr_result: OcrResult,     // OCR extraction
    frame_id: Option<i64>,     // DB ID (set after storage)
}
```

**Configuration**:

```rust
pub struct OcrProcessorConfig {
    min_confidence: f32,         // Filter threshold (default: 0.7)
    worker_threads: usize,       // Concurrent workers (default: 2)
    max_retries: u32,           // Retry attempts on error (default: 3)
    retry_backoff_ms: u64,      // Retry delay (default: 1000ms)
    store_empty_frames: bool,   // Store frames with no text (default: false)
    channel_buffer_size: usize, // Queue capacity (default: 100)
    enable_metrics: bool,       // Track performance (default: true)
    metrics_interval_secs: u64, // Metrics logging (default: 60s)
}
```

**Thread Safety Considerations**:

Windows COM objects (OcrEngine) are **not Send/Sync safe**. The implementation handles this by:
- Using `tokio::task::spawn_blocking` for OCR operations
- Creating fresh OcrEngine instance per worker thread
- Never sharing COM objects across thread boundaries
- Cloning image data before moving to blocking tasks

**Performance Metrics**:

```rust
pub struct OcrMetrics {
    frames_processed: AtomicU64,         // Total frames
    errors: AtomicU64,                   // Failed operations
    regions_extracted: AtomicU64,        // Total text regions
    total_processing_time_ms: AtomicU64, // Cumulative time
    empty_frames: AtomicU64,             // No text found
    filtered_frames: AtomicU64,          // Below confidence threshold
}

// Methods:
metrics.avg_processing_time_ms()  // Average per frame
metrics.success_rate()             // % successful
metrics.log_metrics()              // Log to tracing
```

**Performance Characteristics**:
- Processing time: 50-150ms per frame (1920x1080)
- Throughput: 5-10 frames/second (2 workers)
- CPU: 2-3% per worker during processing
- Memory: ~100MB for worker pool

**Retry Logic**:
```
Attempt 1: Process immediately
    ↓ (if error)
Attempt 2: Wait retry_backoff_ms (1000ms)
    ↓ (if error)
Attempt 3: Wait retry_backoff_ms * 2 (2000ms)
    ↓ (if still error)
Log error & discard frame
```

### 2.3 Database Manager (screen-db crate)

**Purpose**: Persistent storage with full-text search, connection pooling, and optimized queries.

**Architecture**:

```
DatabaseManager
├─> Connection Pool (sqlx::SqlitePool)
│   ├─> Max connections: 50
│   ├─> Min connections: 3
│   └─> Acquire timeout: 10s
├─> Migration System (automatic schema versioning)
├─> Query Interface (type-safe sqlx queries)
└─> FTS5 Search Engine (full-text indexing)
```

**Configuration**:

```rust
pub struct DatabaseConfig {
    path: String,                    // DB file path
    max_connections: u32,            // Pool max (default: 50)
    min_connections: u32,            // Pool min (default: 3)
    acquire_timeout_secs: u64,       // Timeout (default: 10)
    enable_wal: bool,                // WAL mode (default: true)
    cache_size_kb: i32,              // Page cache (default: -2000 = 2MB)
}
```

**Connection Pool Optimization**:

```sql
-- Applied at pool initialization:
PRAGMA journal_mode = WAL;           -- Write-Ahead Logging (concurrent R/W)
PRAGMA synchronous = NORMAL;         -- Balance safety/performance
PRAGMA cache_size = -2000;           -- 2MB cache (negative = KB)
PRAGMA temp_store = MEMORY;          -- Temp tables in RAM
PRAGMA mmap_size = 268435456;        -- Memory-mapped I/O (256MB)
PRAGMA page_size = 4096;             -- Optimal page size
```

**API Overview**:

```rust
// Initialization
let db = DatabaseManager::new("screensearch.db").await?;
let db = DatabaseManager::with_config(config).await?;

// Frame operations
let frame_id = db.insert_frame(new_frame).await?;
let frame = db.get_frame(frame_id).await?;
let frames = db.get_frames_in_range(start, end, filter, pagination).await?;
let count = db.count_frames_in_range(start, end).await?;

// OCR operations
let ocr_id = db.insert_ocr_text(new_ocr).await?;
let texts = db.get_ocr_text_for_frame(frame_id).await?;
let results = db.search_ocr_text(query, filter, pagination).await?;

// Tag operations
let tag_id = db.create_tag(new_tag).await?;
db.add_tag_to_frame(frame_id, tag_id).await?;
let tags = db.get_tags_for_frame(frame_id).await?;

// Statistics & maintenance
let stats = db.get_statistics().await?;
db.cleanup_old_data(days_to_keep).await?;
db.close().await;
```

**Query Performance Targets**:
- Frame insertion: 2-5ms (with transaction)
- OCR insertion: 1-3ms (triggers FTS5 update)
- Time range query: 10-50ms (with indexes)
- Full-text search: 50-200ms (100k+ frames)
- Pagination: 10-30ms (limit 100)

### 2.4 API Server (screen-api crate)

**Purpose**: REST API for querying data and controlling computer automation on localhost:3131.

**Framework**: Axum (tokio-based HTTP server)

**Architecture**:

```
ApiServer
├─> Axum Router (endpoint definitions)
├─> Middleware Stack
│   ├─> CORS (explicit allow-list for localhost with credentials)
│   ├─> Request tracing (logging)
│   ├─> Error handling (AppError → HTTP)
│   └─> JSON serialization/deserialization
├─> Embedded UI Assets (rust-embed)
│   ├─> All files from screen-ui/dist/ embedded at compile time
│   ├─> Served from memory with proper MIME types
│   └─> SPA fallback for client-side routing
├─> State (shared database pool)
└─> Handlers (endpoint implementations)
```

**Endpoint Categories**:

**1. Context Retrieval Endpoints**:
```
GET  /search              - Full-text search with filters
GET  /frames              - Retrieve frames (paginated)
GET  /frames/:id          - Get specific frame
GET  /ocr/:frame_id       - Get OCR for frame
GET  /tags                - List all tags
GET  /health              - Health check & statistics
```

**2. Computer Automation Endpoints**:
```
POST /automation/find-elements  - Locate UI elements by selector
POST /automation/click          - Click at coordinates
POST /automation/type           - Type text
POST /automation/scroll         - Scroll window/element
POST /automation/press-key      - Keyboard input
POST /automation/get-text       - Extract element text
POST /automation/list-elements  - Enumerate UI tree
POST /automation/open-app       - Launch application
POST /automation/open-url       - Open URL in browser
```

**3. System Management Endpoints**:
```
POST   /tags               - Create new tag
DELETE /tags/:id           - Delete tag
POST   /frames/:id/tags    - Add tag to frame
DELETE /frames/:id/tags    - Remove tag from frame
```

**Request/Response Flow**:

```
HTTP Request
    ↓
Axum Router (match endpoint)
    ↓
Extract & Validate Parameters
    ↓
Acquire Database Connection (from pool)
    ↓
Execute Query / Automation Action
    ↓
Map Results to Response Models
    ↓
Serialize to JSON
    ↓
HTTP Response (with status code)
```

**Error Handling**:

```rust
pub enum AppError {
    DatabaseError(DatabaseError),    // DB operations
    AutomationError(AutomationError), // UI automation
    ValidationError(String),          // Invalid input
    NotFound(String),                 // Resource missing
}

impl IntoResponse for AppError {
    // Maps to appropriate HTTP status:
    // DatabaseError → 500 Internal Server Error
    // ValidationError → 400 Bad Request
    // NotFound → 404 Not Found
    // AutomationError → 500 or 400 (depends on cause)
}
```

**Static Asset Serving**:

As of version 0.1.0, the web UI is embedded directly in the binary using `rust-embed`:

```rust
#[derive(RustEmbed)]
#[folder = "../screen-ui/dist/"]
pub struct Assets;

// Assets are served from memory, making the binary portable
async fn serve_embedded(uri: Uri) -> impl IntoResponse {
    // 1. Try to serve file from embedded assets
    // 2. If not found and not API route → serve index.html (SPA fallback)
    // 3. Proper MIME type detection based on file extension
}
```

**Benefits of Embedded Assets**:
- **Portable**: Binary runs from any directory without requiring `screen-ui/dist/` at runtime
- **Fast**: Assets served from memory instead of filesystem I/O
- **Simple**: Single binary deployment with no external dependencies
- **Secure**: No directory traversal vulnerabilities

**Configuration**:

```rust
pub struct ApiConfig {
    host: String,              // Bind address (default: "127.0.0.1")
    port: u16,                 // Port (default: 3131)
    database_path: String,     // Path to SQLite DB
    auto_open_browser: bool,   // Auto-launch browser on startup (default: true)
}
```

**Example Search Request**:

```bash
# Full-text search with filters
curl -X GET "http://localhost:3131/search?q=database&app=chrome&limit=50&offset=0"

# Response:
{
  "results": [
    {
      "frame_id": 12345,
      "timestamp": "2025-12-10T10:30:00Z",
      "text": "Database Management System",
      "confidence": 0.95,
      "x": 100, "y": 200, "width": 300, "height": 50,
      "active_process": "chrome.exe",
      "relevance_score": 0.87
    }
  ],
  "total": 42,
  "limit": 50,
  "offset": 0
}
```

### 2.5 UI Automation (screen-automation crate)

**Purpose**: Windows UI automation using UIAutomation API for programmatic control.

**Key Components**:

```
AutomationEngine
├─> UIAutomation API (Windows.UI.Automation)
├─> Selector System (Playwright-inspired)
├─> UIElement Wrapper (safe operations)
├─> InputSimulator (mouse/keyboard)
└─> WindowManager (window enumeration)
```

**Selector System**:

```rust
// Find by role and name
let button = Selector::role("button").with_name("Submit");

// Find by role and value
let input = Selector::role("edit").with_value("username");

// Complex selector with containment
let item = Selector::new()
    .role("listitem")
    .within("window", "Chrome")
    .containing_text("Settings");

// Execute selector
let element = engine.find_element(&button).await?;
```

**Element Operations**:

```rust
// Click element
element.click().await?;

// Type text
element.type_text("Hello World").await?;

// Extract text
let text = element.get_text().await?;

// Get bounding box
let bounds = element.get_bounds().await?;  // (x, y, width, height)

// Check visibility
if element.is_visible().await? {
    // Element is on screen
}
```

**Input Simulation**:

```rust
// Mouse click at coordinates
engine.click_at(x, y).await?;

// Type text (simulates keyboard)
engine.type_text("password123").await?;

// Press keyboard key
engine.press_key(VirtualKey::Enter).await?;

// Scroll window
engine.scroll(direction, amount).await?;
```

**Window Management**:

```rust
// List all windows
let windows = engine.list_windows().await?;

// Find window by title
let window = engine.find_window("Chrome").await?;

// Activate window
window.activate().await?;

// Get window info
let info = window.get_info().await?;  // title, process, rect
```

---

### 2.6 Front-End Architecture (screen-ui)

**Purpose**: Modern, responsive web interface for interacting with the system.

**Tech Stack**:
- **Framework**: React 18
- **Build Tool**: Vite
- **State Management**: Zustand
- **Data Fetching**: TanStack Query (React Query)
- **Styling**: Tailwind CSS + CSS Modules
- **Icons**: Lucide React

**Key Components**:
- **Timeline**: Virtualized grid/list view of captured frames.
- **Activity Graph**: D3/SVG-based visualization of daily activity density.
- **Search**: Real-time search with debounce and highlighting.
- **Footer**: Sticky footer with branding and links.

**Integration**:
- Communicates with `screen-api` via REST calls.
- Assets are embedded into the Rust binary (`rust-embed`) for single-file deployment.

---

### 2.7 Embedding Engine (screensearch-embeddings crate)

**Purpose**: Generates high-dimensional vector embeddings for screen content to enable semantic search (RAG).

**Architecture**:
- **Engine**: ONNX Runtime (ort) pinned to version 1.17.1 equivalent (2.0.0-rc.0).
- **Model**: BGE-M3 or MiniLM-L12 (configurable).
- **Operation**: Background worker task processing frames from `ocr_text`.
- **Output**: 384/1024-dimension float vectors stored as BLOBs.

## 3. RAG & Embeddings Architecture

### 3.1 Overview

ScreenSearch V2 implements a **Retrieval-Augmented Generation (RAG)** system that combines traditional keyword search (FTS5) with semantic vector search for intelligent screen content retrieval. This hybrid approach enables both exact keyword matching and semantic understanding of user queries.

**Key Components**:
- **Embedding Engine**: ONNX Runtime-based text embedding generation
- **Vector Index**: In-memory cosine similarity search
- **Hybrid Search**: Weighted fusion of FTS5 and vector results
- **Background Worker**: Asynchronous embedding generation pipeline
- **Reranking**: Heuristic post-processing for relevance optimization

**Why RAG for Screen Search?**:
- Users often don't remember exact keywords ("that database thing" vs "PostgreSQL query")
- Semantic search finds conceptually similar content across different wording
- Combines precision of keyword search with recall of semantic search
- Enables AI-powered intelligence features (daily summaries, activity insights)

### 3.2 System Components

```
┌─────────────────────────────────────────────────────────────────────┐
│                    RAG & Embeddings Pipeline                         │
└─────────────────────────────────────────────────────────────────────┘
    │
    ├─> Frame Capture → OCR Processing → Database Storage
    │                                           │
    │                                           ▼
    │                                  ┌────────────────────┐
    │                                  │ Embedding Worker    │
    │                                  │ (Background Task)   │
    │                                  └──────────┬─────────┘
    │                                             │
    │   ┌─────────────────────────────────────────▼─────────┐
    │   │         Embedding Generation Pipeline              │
    │   │                                                     │
    │   │  1. Fetch frames without embeddings (batch: 50)   │
    │   │       ↓                                            │
    │   │  2. Load OCR text & concatenate                   │
    │   │       ↓                                            │
    │   │  3. Text Chunking (max 256 tokens, overlap 32)    │
    │   │       ↓                                            │
    │   │  4. ONNX Model Inference (MiniLM-L12-v2)          │
    │   │       - Tokenization (HuggingFace tokenizers)     │
    │   │       - Forward pass (ONNX Runtime)               │
    │   │       - Mean pooling over sequence                │
    │   │       - L2 normalization                          │
    │   │       ↓                                            │
    │   │  5. Store embeddings in SQLite (BLOB)             │
    │   │       - frame_id, chunk_text, chunk_index         │
    │   │       - embedding (Vec<f32> as bytes)             │
    │   │       - model_name, embedding_dim                 │
    │   └─────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         Hybrid Search Query                          │
│                                                                       │
│  User Query: "What database work did I do yesterday?"               │
│       │                                                               │
│       ├──────────────────────┬──────────────────────────────┐       │
│       ▼                      ▼                              ▼       │
│  ┌─────────────┐      ┌────────────┐              ┌──────────────┐ │
│  │ Embed Query │      │FTS5 Search │              │ Filters      │ │
│  │ (ONNX)      │      │(keyword)   │              │(time range)  │ │
│  └──────┬──────┘      └─────┬──────┘              └──────┬───────┘ │
│         │                   │                            │         │
│         ▼                   ▼                            │         │
│  ┌──────────────────────────────────────┐               │         │
│  │   In-Memory Vector Search             │               │         │
│  │                                        │               │         │
│  │  1. Load all embeddings from SQLite   │               │         │
│  │  2. Compute cosine similarity          │               │         │
│  │  3. Sort by similarity (descending)    │◄──────────────┘         │
│  │  4. Take top K (50 default)           │                         │
│  └──────────┬─────────────────────────────┘                         │
│             │                   │                                    │
│             │                   │                                    │
│  ┌──────────▼───────────────────▼─────────────────────────┐         │
│  │         Hybrid Score Fusion (Weighted Sum)              │         │
│  │                                                          │         │
│  │  score = (alpha × vector_sim) + ((1-alpha) × fts_rank) │         │
│  │  default alpha = 0.3 (60% vector, 40% keyword)         │         │
│  └──────────┬───────────────────────────────────────────────┘         │
│             │                                                         │
│             ▼                                                         │
│  ┌─────────────────────────────────────┐                            │
│  │  Reranking & Post-Processing        │                            │
│  │                                      │                            │
│  │  - Keyword boosting (+20% if match) │                            │
│  │  - Recency weighting                │                            │
│  │  - Deduplication (same frame)       │                            │
│  │  - Length normalization             │                            │
│  └──────────┬──────────────────────────┘                            │
│             │                                                         │
│             ▼                                                         │
│  ┌─────────────────────────────────────┐                            │
│  │   Final Results (Top 20 default)    │                            │
│  │                                      │                            │
│  │  - frame metadata                   │                            │
│  │  - chunk_text (OCR excerpt)         │                            │
│  │  - similarity_score (0.0-1.0)       │                            │
│  └─────────────────────────────────────┘                            │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.3 Embedding Generation Pipeline

**Model Details**:
```rust
Model: paraphrase-multilingual-MiniLM-L12-v2
Source: HuggingFace (sentence-transformers)
Embedding Dimension: 384 (float32)
Max Sequence Length: 256 tokens
Model Size: ~120MB (ONNX format)
Languages: 50+ languages supported
```

**4-Step Generation Process**:

```rust
// 1. Text Chunking
pub struct TextChunker {
    max_tokens: usize,      // 256 (MiniLM limit)
    overlap: usize,         // 32 tokens (preserve context)
}

// Example: Long OCR text split into chunks
let text = "PostgreSQL query optimization... [2000 tokens] ...performance tuning";
let chunks = chunker.chunk_text(text);
// Result: [
//   "PostgreSQL query optimization...[256 tokens]",
//   "...[32 overlap]...database indexing...[256 tokens]",
//   "...[32 overlap]...performance tuning"
// ]

// 2. Tokenization (HuggingFace tokenizers)
let encoding = tokenizer.encode(chunk_text, true)?;
let input_ids = encoding.get_ids();           // Token IDs
let attention_mask = encoding.get_attention_mask();  // 1 for real tokens, 0 for padding
let token_type_ids = encoding.get_type_ids(); // Segment IDs

// 3. ONNX Model Inference
let inputs = ort::inputs![
    "input_ids" => Array2::from_vec(input_ids),
    "attention_mask" => Array2::from_vec(attention_mask),
    "token_type_ids" => Array2::from_vec(token_type_ids)
]?;

let outputs = session.run(inputs)?;
let last_hidden_state = outputs["last_hidden_state"]; // Shape: [batch, seq_len, 384]

// 4. Mean Pooling & Normalization
let mut sum_vec = vec![0.0f32; 384];
let mut count = 0.0;

for token_idx in 0..seq_len {
    if attention_mask[token_idx] == 1 {
        for dim in 0..384 {
            sum_vec[dim] += last_hidden_state[[0, token_idx, dim]];
        }
        count += 1.0;
    }
}

// Average over real tokens
for x in sum_vec.iter_mut() {
    *x /= count;
}

// L2 normalization (unit vector)
let norm = sum_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
for x in sum_vec.iter_mut() {
    *x /= norm;
}

// Result: normalized 384-dimensional embedding
```

**Storage Format**:
```sql
-- Embeddings stored as raw bytes (Vec<f32> serialized)
CREATE TABLE embeddings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id INTEGER NOT NULL,
    chunk_text TEXT NOT NULL,           -- The text chunk that was embedded
    chunk_index INTEGER NOT NULL,       -- 0, 1, 2... for multi-chunk frames
    embedding BLOB NOT NULL,            -- 384 × 4 bytes = 1,536 bytes
    model_name TEXT NOT NULL,           -- "paraphrase-multilingual-MiniLM-L12-v2"
    embedding_dim INTEGER NOT NULL,     -- 384
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (frame_id) REFERENCES frames(id) ON DELETE CASCADE
);
```

### 3.4 Hybrid Search Algorithm

**Pseudocode**:
```python
def hybrid_search(query: str, alpha: float = 0.3, limit: int = 50):
    # 1. Generate query embedding
    query_embedding = embed_text(query)  # 384-dim vector

    # 2. Vector Search (Semantic)
    vector_results = semantic_search(query_embedding, limit)
    # Returns: [(frame_id, chunk_text, similarity_score), ...]
    # Similarity: cosine distance 0.0-1.0

    # 3. FTS5 Search (Keyword)
    fts_results = fts5_search(query, limit)
    # Returns: [(frame_id, text, bm25_score), ...]
    # BM25 score: normalized 0.0-1.0

    # 4. Score Fusion (Weighted Reciprocal Rank Fusion)
    merged_scores = {}

    for (frame_id, text, sim) in vector_results:
        key = (frame_id, text)
        merged_scores[key] = alpha * sim

    for (frame_id, text, bm25) in fts_results:
        key = (frame_id, text)
        if key in merged_scores:
            merged_scores[key] += (1 - alpha) * bm25
        else:
            merged_scores[key] = (1 - alpha) * bm25

    # 5. Sort by combined score
    results = sorted(merged_scores.items(),
                     key=lambda x: x[1],
                     reverse=True)

    return results[:limit]
```

**Weight Configuration**:
```toml
[embeddings]
hybrid_search_alpha = 0.3  # 30% vector, 70% keyword (default)

# Use cases:
# alpha = 0.0  → Pure keyword search (FTS5 only)
# alpha = 0.3  → Balanced (default: slight keyword preference)
# alpha = 0.5  → Equal weighting
# alpha = 1.0  → Pure semantic search (vector only)
```

**Example Search Flow**:
```
Query: "database optimization work"
Alpha: 0.3

Vector Search Results (cosine similarity):
1. Frame 1234: "PostgreSQL query tuning..." → 0.89
2. Frame 1235: "Database indexing strategy..." → 0.85
3. Frame 1236: "SQL performance analysis..." → 0.82

FTS5 Results (BM25 ranking):
1. Frame 1234: "PostgreSQL query tuning..." → 0.95 (exact match "database")
2. Frame 1237: "Optimizing database schemas..." → 0.88
3. Frame 1235: "Database indexing strategy..." → 0.80

Hybrid Fusion:
Frame 1234: (0.3 × 0.89) + (0.7 × 0.95) = 0.932 ← Best match
Frame 1235: (0.3 × 0.85) + (0.7 × 0.80) = 0.815
Frame 1237: (0.3 × 0.00) + (0.7 × 0.88) = 0.616 (no vector match)
Frame 1236: (0.3 × 0.82) + (0.7 × 0.00) = 0.246 (no keyword match)

Final Ranking:
1. Frame 1234 (0.932) - Both strong vector + keyword match
2. Frame 1235 (0.815) - Strong in both
3. Frame 1237 (0.616) - Keyword match only
4. Frame 1236 (0.246) - Semantic match only
```

### 3.5 Reranking & Post-Processing

**Reranking Pipeline**:
```rust
pub struct RerankConfig {
    pub top_k: usize,           // 20 (return top 20 after reranking)
    pub recency_weight: f32,    // 0.1 (boost recent frames)
    pub length_weight: f32,     // 0.05 (prefer longer chunks)
    pub min_score: f32,         // 0.0 (minimum similarity threshold)
}

pub fn rerank_results(
    results: Vec<SemanticResult>,
    config: &RerankConfig
) -> Vec<SemanticResult> {
    let mut scored_results = Vec::new();
    let now = Utc::now();

    for result in results {
        let mut score = result.similarity_score;

        // 1. Recency Boost: Exponential decay
        let age_hours = (now - result.frame.timestamp).num_hours() as f32;
        let recency_boost = (-age_hours / 168.0).exp() * config.recency_weight;
        score += recency_boost;

        // 2. Length Boost: Prefer substantive chunks
        let length_boost = (result.chunk_text.len() as f32 / 1000.0).min(1.0)
                          * config.length_weight;
        score += length_boost;

        // 3. Apply minimum threshold
        if score >= config.min_score {
            scored_results.push((result, score));
        }
    }

    // Sort by adjusted score
    scored_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    scored_results
        .into_iter()
        .take(config.top_k)
        .map(|(r, _)| r)
        .collect()
}
```

**Keyword Boosting**:
```rust
// Boost results that contain exact query terms
pub fn boost_keyword_matches(
    results: &mut Vec<SemanticResult>,
    query: &str,
    boost_amount: f32  // 0.2 = +20%
) {
    let query_terms: Vec<&str> = query.split_whitespace().collect();

    for result in results.iter_mut() {
        let text_lower = result.chunk_text.to_lowercase();
        let mut match_count = 0;

        for term in &query_terms {
            if text_lower.contains(&term.to_lowercase()) {
                match_count += 1;
            }
        }

        if match_count > 0 {
            let boost = (match_count as f32 / query_terms.len() as f32)
                       * boost_amount;
            result.similarity_score += boost;
        }
    }
}
```

### 3.6 In-Memory Vector Index (Why Not SQLite?)

**Design Decision: In-Memory vs sqlite-vec**

ScreenSearch uses **in-memory vector search** instead of the sqlite-vec extension for the following reasons:

**Technical Constraints**:
1. **Windows DLL Compatibility**: sqlite-vec requires custom SQLite compilation with extension support
2. **Deployment Complexity**: Would require shipping additional DLL files, breaking portability
3. **Build Dependencies**: sqlx doesn't easily support runtime-loaded extensions
4. **User Experience**: Binary should be self-contained with no manual setup

**Performance Trade-offs**:

| Approach | Pros | Cons |
|----------|------|------|
| **In-Memory (Current)** | ✓ No dependencies<br>✓ Simple deployment<br>✓ Fast for <100K embeddings<br>✓ Full Rust control | ✗ Loads all vectors into RAM<br>✗ Slower at scale (>100K)<br>✗ No index persistence |
| **sqlite-vec** | ✓ Disk-based indexing<br>✓ Scales to millions<br>✓ HNSW/IVF algorithms | ✗ Requires extension DLL<br>✗ Build complexity<br>✗ Platform-specific |

**Implementation**:
```rust
pub async fn semantic_search(
    &self,
    query_embedding: Vec<f32>,
    limit: i64,
) -> Result<Vec<SemanticResult>> {
    // 1. Fetch all embeddings from SQLite (BLOB deserialization)
    let rows = sqlx::query(
        "SELECT frame_id, chunk_text, chunk_index, embedding FROM embeddings"
    )
    .fetch_all(self.pool())
    .await?;

    // 2. Load vectors into memory
    let mut candidates = Vec::with_capacity(rows.len());

    for row in rows {
        let embedding_blob: Vec<u8> = row.get("embedding");

        // Deserialize Vec<f32> from little-endian bytes
        let vector: Vec<f32> = embedding_blob
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();

        // 3. Compute cosine similarity
        let similarity = cosine_similarity(&query_embedding, &vector);

        candidates.push((
            row.get("frame_id"),
            row.get("chunk_text"),
            row.get("chunk_index"),
            similarity
        ));
    }

    // 4. Sort by similarity (brute-force)
    candidates.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());

    // 5. Take top K
    let top_k = candidates.into_iter().take(limit as usize);

    // 6. Fetch frame metadata (bulk query)
    // ... (see vector_search.rs for full implementation)
}
```

**Memory Footprint**:
```
10,000 embeddings:
  = 10,000 × (384 floats × 4 bytes)
  = 10,000 × 1,536 bytes
  = 15.36 MB

100,000 embeddings:
  = 100,000 × 1,536 bytes
  = 153.6 MB

1,000,000 embeddings:
  = 1,000,000 × 1,536 bytes
  = 1.536 GB (starts to impact performance)
```

**Performance Characteristics**:
- **Load Time**: 50ms for 10K embeddings, 500ms for 100K
- **Search Time**: ~10ms for 1K, ~150ms for 10K, ~1.5s for 100K
- **Memory Overhead**: Embeddings loaded on-demand, released after search
- **Bottleneck**: Brute-force cosine similarity (O(n) per query)

### 3.7 Embedding Worker Lifecycle

**Background Processing Loop**:
```rust
pub async fn run(&self) {
    let mut tick = interval(Duration::from_secs(60)); // Every 60 seconds

    loop {
        tick.tick().await;

        // 1. Check if embeddings are enabled (dynamic config)
        let enabled = self.db.get_metadata("embeddings_enabled")
            .await
            .unwrap_or(Some("false".to_string()))
            == Some("true".to_string());

        if !enabled {
            debug!("Embeddings disabled, skipping batch");
            continue;
        }

        // 2. Process batch of frames without embeddings
        match self.process_batch().await {
            Ok(count) => {
                if count > 0 {
                    info!("Processed {} frames", count);
                }
            }
            Err(e) => {
                error!("Batch processing failed: {}", e);
            }
        }
    }
}
```

**Batch Processing Logic**:
```sql
-- 1. Find frames without embeddings (batch size: 50)
SELECT f.id, f.timestamp, f.active_process
FROM frames f
LEFT JOIN embeddings e ON f.id = e.frame_id
WHERE e.id IS NULL
ORDER BY f.timestamp DESC
LIMIT 50;

-- 2. For each frame, fetch OCR text
SELECT text FROM ocr_text WHERE frame_id = ?;

-- 3. Chunk, embed, and store
INSERT INTO embeddings (frame_id, chunk_text, chunk_index, embedding, model_name, embedding_dim)
VALUES (?, ?, ?, ?, 'paraphrase-multilingual-MiniLM-L12-v2', 384);

-- 4. Update progress metadata
UPDATE metadata SET value = ? WHERE key = 'embeddings_last_processed_frame_id';
```

**Graceful Degradation**:
- If model download fails → Worker continues without embeddings (fallback mode)
- If ONNX runtime unavailable → Falls back to simple hashing (deterministic but low quality)
- If worker crashes → Next restart resumes from last processed frame ID

### 3.8 Configuration

**Embedding Configuration**:
```toml
[embeddings]
enabled = true                                    # Enable/disable embedding generation
batch_size = 50                                   # Frames per batch
model = "local"                                   # "local" or "remote" (future)
model_name = "paraphrase-multilingual-MiniLM-L12-v2"
embedding_dim = 384                               # Vector dimension
max_chunk_tokens = 256                            # Max tokens per chunk
chunk_overlap = 32                                # Overlap between chunks
hybrid_search_alpha = 0.3                         # Vector weight (0.0-1.0)
max_context_chunks = 20                           # Max chunks for RAG context
```

**Runtime Configuration** (via API):
```bash
# Enable embeddings
curl -X POST http://localhost:3131/api/embeddings/enable

# Disable embeddings
curl -X POST http://localhost:3131/api/embeddings/disable

# Check status
curl http://localhost:3131/api/embeddings/status
# Response:
{
  "enabled": true,
  "total_embeddings": 15234,
  "frames_with_embeddings": 1523,
  "last_processed_frame_id": 1600,
  "model_name": "paraphrase-multilingual-MiniLM-L12-v2",
  "embedding_dim": 384
}
```

### 3.9 Performance Characteristics

| Metric | Target | Actual (10K frames) | Actual (100K frames) |
|--------|--------|---------------------|----------------------|
| **Embedding Generation** | | | |
| Single frame embedding | <500ms | ~300ms | ~300ms |
| Batch (50 frames) | <30s | ~15s | ~15s |
| Worker throughput | >100 frames/min | ~200 frames/min | ~200 frames/min |
| **Search Performance** | | | |
| Vector search (in-memory) | <200ms | ~50ms | ~150ms |
| FTS5 keyword search | <100ms | ~30ms | ~80ms |
| Hybrid search (combined) | <300ms | ~80ms | ~230ms |
| Reranking overhead | <50ms | ~20ms | ~30ms |
| **Memory Usage** | | | |
| Embedding engine (ONNX) | <1GB | ~600MB | ~600MB |
| Vector index (loaded) | <200MB | ~15MB | ~153MB |
| Query processing | <100MB | ~50MB | ~80MB |
| **Storage** | | | |
| Embeddings (database) | ~1.5KB/embedding | ~15MB (10K) | ~150MB (100K) |
| Model files (disk) | ~120MB | 118MB | 118MB |

**Bottlenecks**:
1. **Brute-force vector search**: O(n) complexity, becomes slow >100K embeddings
2. **Model loading**: 1-2 second startup time for ONNX session initialization
3. **Memory**: Entire vector index loaded during search (no lazy loading)

### 3.10 Future Optimizations

**1. Approximate Nearest Neighbor (ANN) Algorithms**:
```rust
// Replace brute-force with HNSW (Hierarchical Navigable Small World)
use hnsw_rs::{Hnsw, DistCosine};

let mut hnsw = Hnsw::new(16, 384, 200, DistCosine);
for (id, vector) in embeddings {
    hnsw.insert(&vector, id);
}

// Search: O(log n) instead of O(n)
let results = hnsw.search(&query, 50, 100);
```

**Benefits**: 10-100x faster search for large datasets (>100K)

**2. Quantization (Reduce Memory)**:
```rust
// Convert f32 → i8 (4x smaller)
fn quantize_vector(vec: &[f32]) -> Vec<i8> {
    vec.iter()
        .map(|&x| (x * 127.0).clamp(-127.0, 127.0) as i8)
        .collect()
}

// Memory: 384 bytes instead of 1,536 bytes per embedding
// Trade-off: Slight accuracy loss (~2-3% worse recall)
```

**3. GPU Acceleration (CUDA/DirectML)**:
```toml
[dependencies]
ort = { version = "2.0", features = ["cuda", "tensorrt"] }

# Configuration
[embeddings]
use_gpu = true
gpu_device_id = 0
```

**Benefits**: 5-10x faster embedding generation, especially for batch processing

**4. Persistent Vector Index (Future sqlite-vec)**:
```sql
-- When sqlite-vec becomes Windows-compatible
CREATE VIRTUAL TABLE vec_embeddings USING vec0(
    embedding FLOAT[384]
);

-- HNSW index
SELECT * FROM vec_embeddings
WHERE embedding MATCH ?
ORDER BY distance
LIMIT 50;
```

**5. Streaming Search (Incremental Results)**:
```rust
// Return results as they're computed (async stream)
pub async fn streaming_search(
    &self,
    query: Vec<f32>
) -> impl Stream<Item = SemanticResult> {
    stream! {
        for chunk in self.load_embeddings_chunked(1000).await {
            for (id, vector) in chunk {
                let score = cosine_similarity(&query, &vector);
                if score > 0.5 {
                    yield fetch_result(id, score).await;
                }
            }
        }
    }
}
```

**Benefits**: Lower latency for first results, better UX for large datasets

---

## 4. Database Schema

### 4.1 Core Tables

**video_chunks** - Video file segment metadata:
```sql
CREATE TABLE video_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    start_time DATETIME NOT NULL,
    end_time DATETIME NOT NULL,
    duration_ms INTEGER NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    fps INTEGER NOT NULL DEFAULT 2,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(device_name, start_time, end_time)
);

-- Indexes:
CREATE INDEX idx_video_chunks_device ON video_chunks(device_name);
CREATE INDEX idx_video_chunks_start_time ON video_chunks(start_time);
CREATE INDEX idx_video_chunks_time_range ON video_chunks(start_time, end_time);
```

**frames** - Screenshot metadata with application context:
```sql
CREATE TABLE frames (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chunk_id INTEGER,
    timestamp DATETIME NOT NULL,
    monitor_index INTEGER NOT NULL DEFAULT 0,
    device_name TEXT NOT NULL DEFAULT 'default',
    file_path TEXT NOT NULL,
    active_window TEXT,
    active_process TEXT,
    browser_url TEXT,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    offset_index INTEGER NOT NULL DEFAULT 0,
    focused BOOLEAN DEFAULT FALSE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (chunk_id) REFERENCES video_chunks(id) ON DELETE SET NULL
);

-- Indexes:
CREATE INDEX idx_frames_timestamp ON frames(timestamp);
CREATE INDEX idx_frames_device_time ON frames(device_name, timestamp);
CREATE INDEX idx_frames_process ON frames(active_process);
CREATE INDEX idx_frames_url ON frames(browser_url);
CREATE INDEX idx_frames_window ON frames(active_window);
```

**ocr_text** - OCR extraction results with bounding boxes:
```sql
CREATE TABLE ocr_text (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id INTEGER NOT NULL,
    text TEXT NOT NULL,
    text_json TEXT,              -- JSON with confidence + coords
    x INTEGER NOT NULL,
    y INTEGER NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (frame_id) REFERENCES frames(id) ON DELETE CASCADE
);

-- Indexes:
CREATE INDEX idx_ocr_frame_id ON ocr_text(frame_id);
CREATE INDEX idx_ocr_confidence ON ocr_text(confidence);
```

**tags** - User-defined annotation categories:
```sql
CREATE TABLE tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tag_name TEXT NOT NULL UNIQUE,
    description TEXT,
    color TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

**frame_tags** - Many-to-many frame-tag relationship:
```sql
CREATE TABLE frame_tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id INTEGER NOT NULL,
    tag_id INTEGER NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(frame_id, tag_id),
    FOREIGN KEY (frame_id) REFERENCES frames(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

-- Indexes:
CREATE INDEX idx_frame_tags_frame_id ON frame_tags(frame_id);
CREATE INDEX idx_frame_tags_tag_id ON frame_tags(tag_id);
```

**metadata** - Key-value configuration store:
```sql
CREATE TABLE metadata (
    key TEXT PRIMARY KEY,
    value TEXT,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

**embeddings** - Vector embeddings for semantic search:
```sql
CREATE TABLE embeddings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id INTEGER NOT NULL,
    model_name TEXT NOT NULL,
    embedding BLOB NOT NULL,     -- Serialized Vec<f32>
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (frame_id) REFERENCES frames(id) ON DELETE CASCADE
);

-- Indexes:
CREATE INDEX idx_embeddings_frame_id ON embeddings(frame_id);
```

### 4.2 Full-Text Search (FTS5)

**ocr_text_fts** - Virtual table for full-text search:
```sql
CREATE VIRTUAL TABLE ocr_text_fts USING fts5(
    text,
    content='ocr_text',         -- Content table
    content_rowid='id',         -- Primary key mapping
    tokenize = 'porter'         -- Porter stemming algorithm
);

-- Automatic synchronization triggers:
CREATE TRIGGER ocr_text_ai AFTER INSERT ON ocr_text BEGIN
    INSERT INTO ocr_text_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TRIGGER ocr_text_ad AFTER DELETE ON ocr_text BEGIN
    DELETE FROM ocr_text_fts WHERE rowid = old.id;
END;

CREATE TRIGGER ocr_text_au AFTER UPDATE ON ocr_text BEGIN
    DELETE FROM ocr_text_fts WHERE rowid = old.id;
    INSERT INTO ocr_text_fts(rowid, text) VALUES (new.id, new.text);
END;
```

**FTS5 Features**:
- **Porter Stemming**: "running" matches "run", "runs", "runner"
- **BM25 Ranking**: Relevance scoring based on term frequency/document length
- **Phrase Search**: `"exact phrase"` matches exact sequence
- **Boolean Operators**: `term1 AND term2`, `term1 OR term2`, `NOT term`
- **Prefix Matching**: `data*` matches "data", "database", "datastore"

**Search Query Example**:
```sql
SELECT
    f.id, f.timestamp, f.active_process,
    o.text, o.confidence, o.x, o.y, o.width, o.height,
    bm25(ocr_text_fts) AS relevance_score
FROM ocr_text_fts fts
JOIN ocr_text o ON fts.rowid = o.id
JOIN frames f ON o.frame_id = f.id
WHERE fts MATCH 'database AND query'
    AND f.timestamp >= ?
    AND f.active_process LIKE ?
ORDER BY relevance_score DESC
LIMIT 50 OFFSET 0;
```

### 4.3 Entity Relationships

```
video_chunks (1) ──< (0..n) frames
                           │
                           ├──< (0..n) ocr_text ──> (1) ocr_text_fts
                           │
                           └──< (0..n) frame_tags >── (1) tags
```

### 4.4 Indexing Strategy

| Query Pattern | Primary Index | Secondary Indexes |
|---------------|---------------|-------------------|
| Time range queries | `idx_frames_timestamp` | `idx_frames_device_time` |
| By application | `idx_frames_process` | `idx_frames_timestamp` |
| By device + time | `idx_frames_device_time` | - |
| Full-text search | `ocr_text_fts` | `idx_ocr_frame_id` |
| By tag | `idx_frame_tags_tag_id` | `idx_frame_tags_frame_id` |
| High confidence OCR | `idx_ocr_confidence` | `idx_ocr_frame_id` |
| Browser history | `idx_frames_url` | `idx_frames_timestamp` |

### 4.5 Storage Estimates

| Data Type | Size per Record | 100k Frames | 1M Frames |
|-----------|-----------------|-------------|-----------|
| Frame record | ~200 bytes | ~20 MB | ~200 MB |
| OCR text (avg 10 regions) | ~300 bytes each | ~300 MB | ~3 GB |
| FTS5 index | ~50% of text size | ~150 MB | ~1.5 GB |
| Indexes | ~20% of table size | ~100 MB | ~1 GB |
| **Total (DB file)** | - | **~570 MB** | **~5.7 GB** |
| Images (JPEG, Q80, 1920px) | ~100 KB each | ~10 GB | ~100 GB |
| Images (Legacy PNG) | ~1.5 MB each | ~150 GB | ~1.5 TB |

---

### 4.6 Vector Search (RAG)

ScreenSearch implements a **Hybrid Search** system combining FTS5 (Sparse) and Vector Search (Dense):

**In-Memory Vector Search**:
Due to Windows DLL limitations with `sqlite-vec`, vector search is performed in-memory:
1.  **Load**: On demand/startup, embeddings are loaded into memory (optimized `Vec<f32>`).
2.  **Query**: Incoming query is embedded using the ONNX engine.
3.  **Similarity**: Cosine similarity is calculated against all frame embeddings.
4.  **Top-K**: Top results are identified and joined with SQLite metadata.

**Reranker**:
A heuristic reranker refines results by:
-   Boosting recent frames (time decay).
-   Boosting exact keyword matches in OCR text.
-   Deduplicating multiple chunks from the same frame.

## 5. Data Flow

### 5.1 Capture to Database Pipeline

```
┌──────────────────────────────────────────────────────────────────┐
│ CAPTURE ENGINE (Task 1)                                          │
│                                                                   │
│  Timer Tick (3s interval)                                        │
│       ↓                                                           │
│  Enumerate Monitors → Select Target Monitor                      │
│       ↓                                                           │
│  Capture Screen Image (Windows Graphics Capture API)             │
│       ↓                                                           │
│  Get Active Window Context (Win32 API)                           │
│   - Window title                                                 │
│   - Process name                                                 │
│   - Window bounds                                                │
│       ↓                                                           │
│  Frame Differencing Algorithm                                    │
│   - Compare with previous frame                                  │
│   - Calculate change ratio                                       │
│       ↓                                                           │
│  Decision Point:                                                 │
│   ├─> Change < 0.6% → Skip frame (log stats)                    │
│   └─> Change ≥ 0.6% → Continue                                   │
│       ↓                                                           │
│  Create CapturedFrame:                                           │
│   - image: RgbaImage                                             │
│   - timestamp: DateTime<Utc>                                     │
│   - monitor_index: usize                                         │
│   - active_window: Option<String>                                │
│   - active_process: Option<String>                               │
│       ↓                                                           │
│  Send to channel → frame_tx.send(captured_frame)                 │
└───────────────────────┬──────────────────────────────────────────┘
                        │
                        │ mpsc::channel (buffer: 100)
                        │
┌───────────────────────▼──────────────────────────────────────────┐
│ OCR PROCESSOR (Task 2 - Worker Pool)                             │
│                                                                   │
│  Receive from channel ← frame_rx.recv()                          │
│       ↓                                                           │
│  Spawn async task (worker from pool)                             │
│       ↓                                                           │
│  Clone image data (for thread safety)                            │
│       ↓                                                           │
│  spawn_blocking (move to blocking thread pool)                   │
│       ↓                                                           │
│  Windows OCR API Processing:                                     │
│   1. RgbaImage → PNG encoding (in-memory buffer)                 │
│   2. PNG bytes → IRandomAccessStream                             │
│   3. Stream → SoftwareBitmap decoding                            │
│   4. SoftwareBitmap → OcrEngine.RecognizeAsync()                 │
│   5. Extract OcrLines:                                           │
│      - For each line:                                            │
│        - For each word:                                          │
│          - Extract text                                          │
│          - Get bounding box (x, y, width, height)                │
│      - Aggregate words into line regions                         │
│       ↓                                                           │
│  Confidence Filtering:                                           │
│   - Filter regions where confidence < 0.7                        │
│   - Count filtered regions in metrics                            │
│       ↓                                                           │
│  Create OcrResult:                                               │
│   - regions: Vec<TextRegion>                                     │
│   - full_text: String (space-separated)                          │
│   - processing_time_ms: u64                                      │
│       ↓                                                           │
│  Update Metrics (atomic operations):                             │
│   - frames_processed++                                           │
│   - regions_extracted += region_count                            │
│   - total_processing_time_ms += duration                         │
│       ↓                                                           │
│  Create ProcessedFrame:                                          │
│   - frame: CapturedFrame                                         │
│   - ocr_result: OcrResult                                        │
│   - frame_id: None (set by DB)                                   │
│       ↓                                                           │
│  Send to channel → processed_tx.send(processed_frame)            │
└───────────────────────┬──────────────────────────────────────────┘
                        │
                        │ mpsc::channel (buffer: 100)
                        │
┌───────────────────────▼──────────────────────────────────────────┐
│ DATABASE WRITER (Task 3)                                         │
│                                                                   │
│  Receive from channel ← processed_rx.recv()                      │
│       ↓                                                           │
│  Generate filename: frame_{monitor}_{timestamp}.png              │
│       ↓                                                           │
│  Save image to disk:                                             │
│   - Create captures/ directory if needed                         │
│   - processed.frame.image.save(path)                             │
│       ↓                                                           │
│  Begin database transaction (implicit with sqlx)                 │
│       ↓                                                           │
│  INSERT INTO frames:                                             │
│   - timestamp, device_name, file_path                            │
│   - monitor_index, width, height                                 │
│   - active_window, active_process                                │
│   - focused = true                                               │
│       ↓                                                           │
│  Get frame_id (RETURNING id)                                     │
│       ↓                                                           │
│  For each TextRegion in ocr_result.regions:                      │
│   ↓                                                               │
│   INSERT INTO ocr_text:                                          │
│    - frame_id, text                                              │
│    - x, y, width, height                                         │
│    - confidence                                                  │
│    - text_json (JSON with all metadata)                          │
│   ↓                                                               │
│   Trigger: INSERT INTO ocr_text_fts (automatic)                  │
│    - Updates full-text search index                              │
│    - BM25 ranking data structure updated                         │
│       ↓                                                           │
│  Commit transaction (automatic with sqlx)                        │
│       ↓                                                           │
│  Log success (frame_id, OCR region count)                        │
└──────────────────────────────────────────────────────────────────┘
```

### 5.2 API Query Flow

```
┌──────────────────────────────────────────────────────────────────┐
│ CLIENT                                                            │
│                                                                   │
│  HTTP Request:                                                   │
│  GET /search?q=database&app=chrome&start=...&limit=50            │
└───────────────────────┬──────────────────────────────────────────┘
                        │ HTTP over localhost
┌───────────────────────▼──────────────────────────────────────────┐
│ API SERVER (Axum)                                                │
│                                                                   │
│  Axum Router → Match route "/search"                             │
│       ↓                                                           │
│  Extract Query Parameters:                                       │
│   - q: "database"                                                │
│   - app: "chrome"                                                │
│   - start: DateTime                                              │
│   - limit: 50                                                    │
│   - offset: 0                                                    │
│       ↓                                                           │
│  Validate Parameters:                                            │
│   - Check required fields present                                │
│   - Validate data types                                          │
│   - Check bounds (limit ≤ 1000)                                  │
│       ↓                                                           │
│  Build FrameFilter:                                              │
│   - start_time: Some(start)                                      │
│   - app_name: Some("chrome")                                     │
│   - device_name: None                                            │
│   - tag_ids: None                                                │
│       ↓                                                           │
│  Build Pagination:                                               │
│   - limit: 50                                                    │
│   - offset: 0                                                    │
└───────────────────────┬──────────────────────────────────────────┘
                        │
┌───────────────────────▼──────────────────────────────────────────┐
│ DATABASE MANAGER                                                 │
│                                                                   │
│  db.search_ocr_text(query, filter, pagination)                   │
│       ↓                                                           │
│  Acquire connection from pool (timeout: 10s)                     │
│       ↓                                                           │
│  Build SQL query:                                                │
│                                                                   │
│  SELECT                                                          │
│      f.id AS frame_id,                                           │
│      f.timestamp,                                                │
│      f.active_window,                                            │
│      f.active_process,                                           │
│      f.file_path,                                                │
│      o.text,                                                     │
│      o.confidence,                                               │
│      o.x, o.y, o.width, o.height,                                │
│      bm25(ocr_text_fts) AS relevance_score                       │
│  FROM ocr_text_fts fts                                           │
│  JOIN ocr_text o ON fts.rowid = o.id                             │
│  JOIN frames f ON o.frame_id = f.id                              │
│  WHERE fts MATCH ?                     -- "database"             │
│      AND f.timestamp >= ?              -- start time             │
│      AND f.active_process LIKE ?       -- "%chrome%"             │
│  ORDER BY relevance_score DESC                                   │
│  LIMIT ? OFFSET ?                      -- 50, 0                  │
│       ↓                                                           │
│  Execute query (parameterized)                                   │
│       ↓                                                           │
│  FTS5 Index Scan:                                                │
│   - Parse search term: "database"                                │
│   - Apply Porter stemming: "databas" (stem)                      │
│   - Lookup in FTS index (B-tree)                                 │
│   - Get matching document IDs with term frequency                │
│   - Calculate BM25 scores for ranking                            │
│       ↓                                                           │
│  Apply Filters:                                                  │
│   - Use idx_frames_timestamp for time range                      │
│   - Use idx_frames_process for app filter                        │
│   - Combine with FTS results (join)                              │
│       ↓                                                           │
│  Sort by relevance_score DESC                                    │
│       ↓                                                           │
│  Apply pagination (LIMIT 50 OFFSET 0)                            │
│       ↓                                                           │
│  Map rows to SearchResult structs:                               │
│   - frame_id: i64                                                │
│   - timestamp: DateTime<Utc>                                     │
│   - text: String                                                 │
│   - confidence: f32                                              │
│   - bounding_box: (i32, i32, i32, i32)                           │
│   - active_process: Option<String>                               │
│   - relevance_score: f64                                         │
│       ↓                                                           │
│  Return Vec<SearchResult>                                        │
└───────────────────────┬──────────────────────────────────────────┘
                        │
┌───────────────────────▼──────────────────────────────────────────┐
│ API SERVER (Axum)                                                │
│                                                                   │
│  Serialize results to JSON                                       │
│       ↓                                                           │
│  Build response:                                                 │
│  {                                                               │
│    "results": [                                                  │
│      {                                                           │
│        "frame_id": 12345,                                        │
│        "timestamp": "2025-12-10T10:30:00Z",                      │
│        "text": "Database Management System",                     │
│        "confidence": 0.95,                                       │
│        "x": 100, "y": 200, "width": 300, "height": 50,          │
│        "active_process": "chrome.exe",                           │
│        "relevance_score": 0.87                                   │
│      },                                                          │
│      ...                                                         │
│    ],                                                            │
│    "total": 42,                                                  │
│    "limit": 50,                                                  │
│    "offset": 0                                                   │
│  }                                                               │
│       ↓                                                           │
│  HTTP Response: 200 OK                                           │
└───────────────────────┬──────────────────────────────────────────┘
                        │
┌───────────────────────▼──────────────────────────────────────────┐
│ CLIENT                                                            │
│                                                                   │
│  Receive JSON response                                           │
│  Display results to user                                         │
└──────────────────────────────────────────────────────────────────┘
```

### 5.3 Automation Flow

```
┌──────────────────────────────────────────────────────────────────┐
│ CLIENT                                                            │
│                                                                   │
│  POST /automation/click                                          │
│  { "selector": "role:button name:Submit" }                       │
└───────────────────────┬──────────────────────────────────────────┘
                        │
┌───────────────────────▼──────────────────────────────────────────┐
│ API SERVER                                                       │
│                                                                   │
│  Parse selector string                                           │
│       ↓                                                           │
│  Call automation engine                                          │
└───────────────────────┬──────────────────────────────────────────┘
                        │
┌───────────────────────▼──────────────────────────────────────────┐
│ AUTOMATION ENGINE (UIAutomation API)                             │
│                                                                   │
│  Initialize UIAutomation COM object                              │
│       ↓                                                           │
│  Parse Selector:                                                 │
│   - role: "button"                                               │
│   - name: "Submit"                                               │
│       ↓                                                           │
│  Get root element (desktop)                                      │
│       ↓                                                           │
│  FindAll descendants:                                            │
│   - Filter by ControlType == Button                              │
│   - Filter by Name == "Submit"                                   │
│       ↓                                                           │
│  Found element? Yes                                              │
│       ↓                                                           │
│  Get element bounding rectangle                                  │
│       ↓                                                           │
│  Calculate center point:                                         │
│   - center_x = x + width / 2                                     │
│   - center_y = y + height / 2                                    │
│       ↓                                                           │
│  Invoke UI Automation Action:                                    │
│   - IUIAutomationElement.Invoke()                                │
│   OR simulate mouse click at (center_x, center_y)                │
│       ↓                                                           │
│  Return success                                                  │
└───────────────────────┬──────────────────────────────────────────┘
                        │
┌───────────────────────▼──────────────────────────────────────────┐
│ API SERVER                                                       │
│                                                                   │
│  HTTP Response: 200 OK                                           │
│  { "status": "success" }                                         │
└──────────────────────────────────────────────────────────────────┘
```

---

## 6. Concurrency Model

### 6.1 Task Distribution

ScreenSearch uses tokio's multi-threaded async runtime with independent concurrent tasks:

```
Main Thread (tokio runtime)
│
├─> Task 1: Capture Loop (spawned with tokio::spawn)
│   ├─ Timer: tokio::time::interval(3000ms)
│   ├─ Polls: capture_engine.try_get_frame()
│   ├─ Sends: frame_tx.send(frame)
│   └─ Shutdown: via broadcast::Receiver
│
├─> Task 2: OCR Processing (spawned by OcrProcessor)
│   ├─ Worker Pool: 2 async workers (configurable)
│   ├─ Receives: frame_rx.recv()
│   ├─ Spawns: tokio::task::spawn_blocking (per frame)
│   ├─ Sends: processed_tx.send(processed)
│   └─ Metrics: atomic operations (lock-free)
│
├─> Task 3: Database Insertion (spawned with tokio::spawn)
│   ├─ Receives: processed_rx.recv()
│   ├─ Writes: db.insert_frame() + db.insert_ocr_text()
│   ├─ Disk I/O: image.save() (blocking, uses tokio::fs)
│   └─ Shutdown: via broadcast::Receiver
│
├─> Task 4: API Server (spawned with tokio::spawn)
│   ├─ Axum HTTP server (handles requests concurrently)
│   ├─ Connection pool: shared Arc<DatabaseManager>
│   ├─ Request handlers: async functions
│   └─ Shutdown: via broadcast::Receiver
│
└─> Task 5: Metrics Reporter (optional, spawned with tokio::spawn)
    ├─ Timer: tokio::time::interval(60s)
    ├─ Reads: ocr_metrics (atomic reads)
    ├─ Logs: via tracing::info!()
    └─ Shutdown: via broadcast::Receiver

└─> Task 6: Background Cleanup (spawned with tokio::spawn)
    ├─ Timer: tokio::time::interval(24h)
    ├─ Action: db.cleanup_old_data(retention_days)
    └─ Shutdown: via broadcast::Receiver

└─> Task 7: Embedding Worker (spawned via ApiServer)
    ├─ Channel: Receives frame IDs from OCR/db
    ├─ Process: Generates embeddings via ONNX
    └─ Store: Inserts into `embeddings` table
```

### 6.2 Communication Channels

**Frame Pipeline Channels**:
```rust
// Capture → OCR
let (frame_tx, frame_rx) = tokio::sync::mpsc::channel::<CapturedFrame>(100);
// Type: unbounded producer (capture), multiple consumer workers (OCR)
// Buffer: 100 frames (~150 MB at 1920x1080)

// OCR → Database
let (processed_tx, processed_rx) = tokio::sync::mpsc::channel::<ProcessedFrame>(100);
// Type: multiple producers (OCR workers), single consumer (DB writer)
// Buffer: 100 frames

// Shutdown signal
let (shutdown_tx, _) = broadcast::channel::<()>(10);
// Type: broadcast (one signal, multiple receivers)
// Cloned for each task
```

**Channel Characteristics**:
- **mpsc**: Multi-producer, single-consumer (async)
- **Bounded**: Fixed buffer size (backpressure handling)
- **broadcast**: One-to-many shutdown signaling
- **Non-blocking sends**: await on send if buffer full

### 6.3 Synchronization Primitives

**Database Connection Pool** (thread-safe):
```rust
Arc<DatabaseManager>  // Shared across tasks via Arc
    └─> SqlitePool    // Internal connection pool (thread-safe)
        ├─ Max: 50 connections
        ├─ Min: 3 connections
        └─ Mutex per connection (managed by sqlx)
```

**OCR Metrics** (lock-free):
```rust
pub struct OcrMetrics {
    frames_processed: AtomicU64,         // Atomic increment
    errors: AtomicU64,                   // Atomic increment
    regions_extracted: AtomicU64,        // Atomic add
    total_processing_time_ms: AtomicU64, // Atomic add
    // ... other atomics
}
// No locks needed - uses atomic CPU instructions
```

**Capture Engine** (internal mutex):
```rust
pub struct CaptureEngine {
    frame_queue: Arc<Mutex<VecDeque<CapturedFrame>>>, // Mutex-protected queue
    // try_get_frame() locks briefly to pop frame
}
```

### 6.4 Async vs Blocking

**Async Operations** (tokio runtime):
- HTTP request handling (Axum)
- Database queries (sqlx with async)
- Channel send/recv operations
- Timer waits (tokio::time::interval)
- Shutdown coordination

**Blocking Operations** (tokio::task::spawn_blocking):
- Windows OCR API calls (COM not async-safe)
- Image encoding (PNG)
- Screen capture (Windows API)
- File I/O (image.save() - synchronous)

**Thread Pool Usage**:
- Tokio runtime: Default = # of CPU cores (for async tasks)
- Blocking pool: Separate pool (default: 512 threads max)

### 6.5 Shutdown Sequence

```
Ctrl+C Signal Received
    ↓
Main: shutdown_tx.send(())  // Broadcast to all tasks
    ↓
┌───────────────┬───────────────┬───────────────┬───────────────┐
│ Task 1        │ Task 2        │ Task 3        │ Task 4        │
│ Capture       │ OCR           │ Database      │ API           │
├───────────────┼───────────────┼───────────────┼───────────────┤
│ shutdown_rx1  │ shutdown_rx2  │ shutdown_rx3  │ shutdown_rx4  │
│ .recv()       │ .recv()       │ .recv()       │ .recv()       │
│     ↓         │     ↓         │     ↓         │     ↓         │
│ Stop capture  │ Stop workers  │ Flush queue   │ Stop server   │
│ engine        │               │               │               │
│     ↓         │     ↓         │     ↓         │     ↓         │
│ Return from   │ Return from   │ Return from   │ Return from   │
│ task          │ task          │ task          │ task          │
└───────────────┴───────────────┴───────────────┴───────────────┘
    ↓               ↓               ↓               ↓
Main: tokio::join!(task1, task2, task3, task4)
    ↓
Wait for all tasks to complete
    ↓
Final logging: "All services stopped. Goodbye!"
    ↓
Process exit
```

**Graceful Shutdown Guarantees**:
- All queued frames are processed before shutdown
- Database transactions are committed
- Open file handles are closed
- HTTP connections are drained

### 6.6 Error Isolation

Tasks are isolated - errors in one task don't crash others:

```
Task 1 Error (capture fails)
    ↓
Log error: error!("Capture failed: {}", e)
    ↓
Continue capture loop (skip this frame)
    ↓
Other tasks unaffected

Task 2 Error (OCR fails)
    ↓
Retry logic (max 3 attempts)
    ↓
If still fails: Log + discard frame
    ↓
Other tasks unaffected

Task 3 Error (database fails)
    ↓
FATAL: Log + propagate error
    ↓
Shutdown signal sent
    ↓
All tasks stop gracefully
```

---

## 7. Configuration Architecture

### 7.1 Configuration File Structure

**config.toml** - TOML format for human-friendly configuration:

```toml
[capture]
interval_ms = 3000                # Capture every 3 seconds
enable_frame_diff = true          # Enable change detection
diff_threshold = 0.006            # 0.6% change threshold
max_frames_buffer = 30            # Max frames in queue
monitor_indices = []              # Empty = all monitors
include_cursor = true             # Include mouse cursor
draw_border = false               # Draw capture border

[ocr]
engine = "windows"                # OCR engine (currently only "windows")
min_confidence = 0.7              # Confidence threshold (0.0-1.0)
worker_threads = 2                # OCR worker count
max_retries = 3                   # Retry attempts on error
retry_backoff_ms = 1000           # Retry delay
store_empty_frames = false        # Store frames with no text
channel_buffer_size = 100         # Queue capacity
enable_metrics = true             # Enable metrics tracking
metrics_interval_secs = 60        # Metrics logging interval

[api]
host = "127.0.0.1"                # Bind address (localhost only)
port = 3131                       # HTTP port
cors_origin = ""                  # CORS origin (empty = permissive localhost)

[database]
path = "screensearch.db"       # SQLite file path
max_connections = 50              # Connection pool max
min_connections = 3               # Connection pool min
acquire_timeout_secs = 10         # Connection acquire timeout
enable_wal = true                 # WAL mode (recommended)
cache_size_kb = -2000             # Page cache (-2000 = 2MB)

[embeddings]
enabled = true                    # Enable semantic search
model_path = "models/bge-m3"      # Path to ONNX model
tokenizer_path = "models/tokenizer.json"

[privacy]
excluded_apps = [                 # Apps to skip capturing
    "1Password",
    "KeePass",
    "Bitwarden",
    "LastPass",
    "Password",
    "Bank"
]
pause_on_lock = true              # Pause when screen locked

[performance]
max_cpu_percent = 5               # CPU usage limit (advisory)
max_memory_mb = 500               # Memory usage limit (advisory)

[logging]
level = "info"                    # Log level: trace, debug, info, warn, error
log_to_file = true                # Enable file logging
log_file = "screensearch.log"  # Log file path
max_log_size_mb = 100             # Max log file size
log_rotation_count = 5            # Number of rotated logs to keep
```

### 7.2 Configuration Loading Process

```
Application Start
    ↓
Check for config.toml in current directory
    ↓
    ├─> File exists:
    │   ├─ Read file content
    │   ├─ Parse TOML → AppConfig struct
    │   ├─ Validate configuration
    │   └─ Log: "Loaded configuration from config.toml"
    │
    └─> File not exists:
        ├─ Use AppConfig::default()
        └─ Log: "config.toml not found, using default configuration"
    ↓
Convert AppConfig to component-specific configs:
    ├─> CaptureConfig
    ├─> OcrProcessorConfig
    ├─> DatabaseConfig
    └─> ApiConfig
    ↓
Initialize components with configs
```

### 7.3 Configuration Validation

**Validation Rules**:
```rust
// Port availability
if port < 1024 || port > 65535 {
    return Err("Invalid port number");
}

// Database path writability
if !Path::new(&db_path).parent().unwrap().exists() {
    return Err("Database directory does not exist");
}

// Privacy app list (case-insensitive)
for app in excluded_apps {
    if app.is_empty() {
        warn!("Empty app name in excluded_apps");
    }
}

// Performance limits (advisory only)
if max_cpu_percent > 100 || max_cpu_percent == 0 {
    warn!("Invalid max_cpu_percent: {}", max_cpu_percent);
}

// Capture interval bounds
if interval_ms < 1000 {
    warn!("Capture interval < 1s may cause high CPU usage");
}
```

### 7.4 Runtime Configuration Changes

**Currently**: Configuration is loaded once at startup. Changes require restart.

**Future Enhancement** (not implemented):
```rust
// Watch config.toml for changes
let watcher = notify::watcher(|event| {
    if event.path == "config.toml" {
        reload_config();
        apply_to_running_services();
    }
});
```

### 7.5 Environment Variable Overrides

**Planned Feature** (not yet implemented):
```bash
# Override configuration via environment variables
export SCREEN_MEMORIES_DATABASE_PATH="/custom/path/db.sqlite"
export SCREEN_MEMORIES_API_PORT=8080
export SCREEN_MEMORIES_LOG_LEVEL=debug

./screensearch.exe
```

---

## 8. Error Handling Strategy

### 8.1 Error Type Hierarchy

```
anyhow::Error (top-level in main.rs)
    ├─> Used for fatal errors that should stop application
    └─> Context added with .context("message")

screen_capture::CaptureError
    ├─> InitializationError(String)      // Failed to init capture/OCR
    ├─> ScreenCaptureError(String)       // Screen capture failed
    ├─> OcrError(String)                 // OCR processing failed
    ├─> WindowsApiError(String)          // Windows API call failed
    ├─> ImageProcessingError(String)     // Image encoding/decoding
    └─> ChannelError(String)             // Channel send/recv failed

screen_db::DatabaseError
    ├─> InitializationError(String)      // DB init/migration failed
    ├─> MigrationError(String)           // Migration execution failed
    ├─> QueryError(String)               // SQL query failed
    ├─> NotFound(String)                 // Resource not found
    ├─> InvalidParameter(String)         // Invalid input
    ├─> SqlxError(sqlx::Error)           // Underlying sqlx error
    └─> IoError(std::io::Error)          // File I/O error

screen_api::AppError (implements IntoResponse)
    ├─> DatabaseError(DatabaseError)     // DB operation failed → 500
    ├─> AutomationError(AutomationError) // UI automation failed → 500/400
    ├─> ValidationError(String)          // Invalid request → 400
    └─> NotFound(String)                 // Resource missing → 404

screen_automation::AutomationError
    ├─> ElementNotFound(String)          // UI element not found
    ├─> ActionFailed(String)             // Action execution failed
    ├─> WindowsApiError(String)          // UIAutomation API error
    └─> InvalidSelector(String)          // Malformed selector
```

### 8.2 Error Recovery Strategies

**Capture Errors** (non-fatal):
```
Error in capture_engine.try_get_frame()
    ↓
Log error: error!("Capture failed: {}", e)
    ↓
Skip this frame
    ↓
Continue capture loop (next interval tick)
    ↓
Increment error counter in metrics
```

**OCR Errors** (retry with backoff):
```
Error in OcrEngine.process_image()
    ↓
Attempt 1 failed
    ↓
Wait retry_backoff_ms (1000ms)
    ↓
Attempt 2 failed
    ↓
Wait retry_backoff_ms * 2 (2000ms)
    ↓
Attempt 3 failed
    ↓
Log error: error!("OCR failed after 3 retries: {}", e)
    ↓
Discard frame
    ↓
Increment metrics.errors
    ↓
Continue processing next frame
```

**Database Errors** (fatal):
```
Error in db.insert_frame()
    ↓
Log error: error!("Database error: {}", e)
    ↓
Propagate error to main task
    ↓
Main task receives error
    ↓
Broadcast shutdown signal
    ↓
All tasks stop gracefully
    ↓
Application exits with error code
```

**API Errors** (HTTP response):
```
Error in API handler
    ↓
Convert to AppError
    ↓
Implement IntoResponse:
    ├─> DatabaseError → 500 Internal Server Error
    ├─> ValidationError → 400 Bad Request
    ├─> NotFound → 404 Not Found
    └─> AutomationError → 500 or 400 (depends on cause)
    ↓
Serialize error to JSON:
{
  "error": "Database query failed",
  "message": "Connection timeout",
  "timestamp": "2025-12-10T10:30:00Z"
}
    ↓
Return HTTP response
    ↓
Log error: warn!("API error: {}", e)
    ↓
Continue serving other requests
```

### 8.3 Error Logging

**Logging Levels**:
```rust
// ERROR: Fatal errors, unrecoverable state
error!("Database initialization failed: {}", e);

// WARN: Recoverable errors, unexpected but handled
warn!("OCR processing failed, retrying: {}", e);

// INFO: Important events, normal operation
info!("Captured frame {} (changed)", frame_id);

// DEBUG: Detailed debugging information
debug!("Frame diff ratio: {:.4}", diff_ratio);

// TRACE: Very detailed tracing
trace!("Acquiring database connection from pool");
```

**Error Context**:
```rust
// Use anyhow::Context to add context to errors
db.insert_frame(frame)
    .await
    .context("Failed to insert frame into database")?;

// Results in error message:
// "Failed to insert frame into database: UNIQUE constraint failed: frames.id"
```

### 8.4 Error Metrics

Track error rates for monitoring:
```rust
// OCR metrics
pub struct OcrMetrics {
    frames_processed: AtomicU64,  // Total attempts
    errors: AtomicU64,             // Failed attempts
}

impl OcrMetrics {
    pub fn success_rate(&self) -> f64 {
        let processed = self.frames_processed.load(Ordering::Relaxed);
        let errors = self.errors.load(Ordering::Relaxed);
        if processed == 0 { return 100.0; }
        100.0 * (1.0 - (errors as f64 / processed as f64))
    }
}

// Log periodically:
// "OCR success rate: 99.80% (998/1000)"
```

### 8.5 Panic Handling

**Panic Strategy**:
```rust
// Panics should be rare and indicate programming errors
// Tokio runtime catches panics in spawned tasks

tokio::spawn(async move {
    // If this task panics:
    // 1. Task stops
    // 2. Other tasks continue
    // 3. JoinHandle returns Err(JoinError)
});

// In main:
let handle = tokio::spawn(capture_task);
match handle.await {
    Ok(result) => { /* Normal completion */ },
    Err(join_error) => {
        if join_error.is_panic() {
            error!("Task panicked: {:?}", join_error);
            // Application can continue or shutdown
        }
    }
}
```

**Panic Hooks**:
```rust
// Set custom panic hook for better error reporting
std::panic::set_hook(Box::new(|panic_info| {
    error!("PANIC: {:?}", panic_info);
    // Could send crash report, etc.
}));
```

---

## 9. Performance Characteristics

### 9.1 System Resource Targets

| Component | CPU (Idle) | CPU (Active) | Memory | Disk I/O |
|-----------|------------|--------------|--------|----------|
| Capture Engine | 1-3% | 5-10% | ~50 MB | Minimal |
| OCR Processor | 0% | Varies | ~200 MB | None |
| Embedding Engine | 0% | Varies | ~600 MB | None |
| Database Manager | <1% | 2-5% | ~150 MB | Medium |
| API Server | <1% | 1-2% | ~50 MB | None |
| **Total System** | **<5%** | **Varies** | **Varies** | **Low** |

**Notes**:
- Idle: Waiting for next capture interval (most of the time)
- Active: During capture + OCR + database write burst
- Measurements on: Intel i5-8250U, 16GB RAM, SSD

### 9.2 Latency Characteristics

| Operation | p50 | p95 | p99 | Target |
|-----------|-----|-----|-----|--------|
| Frame capture (1920x1080) | 15ms | 40ms | 80ms | <50ms |
| Frame differencing | 3ms | 8ms | 15ms | <10ms |
| OCR processing (1920x1080) | 80ms | 150ms | 250ms | <100ms |
| Database frame insert | 3ms | 8ms | 15ms | <10ms |
| Database OCR insert | 2ms | 5ms | 10ms | <5ms |
| Full-text search (10k frames) | 30ms | 60ms | 100ms | <50ms |
| Full-text search (100k frames) | 50ms | 120ms | 200ms | <100ms |
| API response (simple query) | 20ms | 40ms | 80ms | <50ms |
| UI element find | 50ms | 150ms | 300ms | <100ms |

### 9.3 Throughput Metrics

**Capture Pipeline**:
```
Capture interval: 3s
Frame change rate: ~40% (typical desktop use)
Effective capture rate: 0.33 fps * 0.40 = 0.13 effective fps

Per hour:
- Total capture attempts: 1200
- Frames with changes: ~480 (40%)
- Frames stored: ~480
- Storage: ~720 MB images + ~50 MB database
```

**OCR Pipeline**:
```
Worker count: 2
Processing time: 80ms avg per frame
Throughput: 2 workers / 0.08s = 25 frames/second max

Actual throughput (limited by capture):
- 0.13 fps effective capture rate
- 480 frames/hour
- ~8 frames/minute
- Well within OCR capacity (25 fps)
```

**Database Pipeline**:
```
Write operations per frame:
- 1x INSERT INTO frames (3ms)
- ~10x INSERT INTO ocr_text (2ms each) = 20ms
- Total: ~23ms per frame

Throughput: 1 / 0.023s = ~43 frames/second max
Actual: 0.13 fps (well within capacity)
```

### 9.4 Scalability Limits

**Database Scalability**:
| Metric | 10k Frames | 100k Frames | 1M Frames | 10M Frames |
|--------|------------|-------------|-----------|------------|
| DB file size | ~60 MB | ~570 MB | ~5.7 GB | ~57 GB |
| Image storage | ~15 GB | ~150 GB | ~1.5 TB | ~15 TB |
| Full-text search | <30ms | <100ms | <300ms | <1s |
| Frame insert | <5ms | <10ms | <20ms | <50ms |
| Index rebuild | <1s | <10s | <2min | <30min |

**Notes**:
- Tested up to 1M frames in development
- 10M frames extrapolated (not tested)
- Performance degrades gradually with size
- Recommend periodic cleanup of old data

**Concurrent Request Handling**:
```
API Server (Axum):
- Connection pool: 50 max connections
- Concurrent requests: 50+ (limited by pool)
- Request latency: Increases linearly with concurrency
  - 1 request: 20ms
  - 10 concurrent: 30ms avg
  - 50 concurrent: 100ms avg
  - 100 concurrent: 500ms avg (some timeout)
```

**Memory Scalability**:
```
Base memory: ~350 MB
+ Embedding Model (ONNX): ~600 MB (loaded in memory)
+ Frame buffer: ~5 MB per frame in queue (max 30 frames) = 150 MB
+ DB cache: Configurable (default 2 MB)
+ Connection pool overhead: ~200 KB per connection (50 max) = 10 MB
+ Image processing: ~6 MB per active OCR task (2 workers) = 12 MB

**Memory Scalability**:
```
Base memory: ~350 MB
+ Embedding Model (ONNX): ~600 MB (loaded in memory)
+ Frame buffer: ~5 MB per frame in queue (max 30 frames) = 150 MB
+ DB cache: Configurable (default 2 MB)
+ Connection pool overhead: ~200 KB per connection (50 max) = 10 MB
+ Image processing: ~6 MB per active OCR task (2 workers) = 12 MB

Worst case: Varies by model
```

### 9.5 Performance Optimization Techniques

**1. Frame Differencing**:
```rust
// Skip 60% of frames on average (no changes detected)
// Saves:
// - OCR processing: 80ms per skipped frame
// - Database insert: 23ms per skipped frame
// - Disk I/O: 1.5 MB per skipped frame
```

**2. Connection Pooling**:
```rust
// Reuse database connections (avoid handshake overhead)
// Handshake cost: ~10ms
// With pool: <1ms to acquire existing connection
// 10x speedup for frequent operations
```

**3. WAL Mode**:
```sql
PRAGMA journal_mode = WAL;
-- Allows concurrent readers during writes
-- Read performance: No blocking
-- Write performance: Faster commits (sequential writes)
```

**4. FTS5 Indexing**:
```sql
-- Full-text search without index: O(n) scan
-- With FTS5 index: O(log n + m) where m = result count
-- 100k frames: ~100ms with index vs ~10s without
```

**5. Async I/O**:
```rust
// Non-blocking I/O allows high concurrency
// Without async: 50 concurrent requests → 50 threads → high overhead
// With async: 50 concurrent requests → ~8 threads (tokio) → low overhead
```

### 9.6 Performance Monitoring

**Built-in Metrics**:
```rust
// OCR metrics logged every 60s
OCR Metrics:
  - frames_processed: 1000
  - regions_extracted: 15234
  - avg_processing_time_ms: 87.3
  - success_rate: 99.80%
  - empty_frames: 45
  - filtered_frames: 123

// Database statistics
db.get_statistics():
  - frame_count: 12345
  - ocr_text_count: 234567
  - database_size_mb: 567
  - index_size_mb: 123
  - oldest_frame: "2025-11-01T00:00:00Z"
  - newest_frame: "2025-12-10T10:30:00Z"
```

**External Monitoring**:
- CPU: Windows Task Manager / Resource Monitor
- Memory: Task Manager (Private Working Set)
- Disk I/O: Resource Monitor (Disk Activity)
- Network: Should be 0 (localhost only)

---

## 10. Security & Privacy Architecture

### 10.1 Privacy-First Design Principles

1. **Local-Only**: All data stored locally, no network transmission
2. **User Control**: Explicit configuration for what to capture
3. **Transparency**: Open source, auditable code
4. **Opt-Out**: Easy exclusion of sensitive applications
5. **Deletion**: Simple data cleanup mechanisms

### 10.2 Application Exclusion System

**Configuration**:
```toml
[privacy]
excluded_apps = [
    "1Password",
    "KeePass",
    "Bitwarden",
    "LastPass",
    "Password",
    "Bank",
    "Crypto",
    "Wallet"
]
```

**Implementation**:
```rust
fn should_capture_window(window_title: &str, process_name: &str) -> bool {
    let excluded_apps = config.privacy.excluded_apps;

    for excluded in excluded_apps {
        // Case-insensitive substring match
        if window_title.to_lowercase().contains(&excluded.to_lowercase()) {
            return false;  // Skip this window
        }
        if process_name.to_lowercase().contains(&excluded.to_lowercase()) {
            return false;  // Skip this process
        }
    }

    true  // Capture this window
}
```

**Match Examples**:
- `"1Password"` excludes:
  - Window title: "1Password - Vault"
  - Process: "1Password.exe"
- `"Bank"` excludes:
  - Window title: "Chase Bank - Account Summary"
  - Process: "BankApp.exe"

### 10.3 Screen Lock Detection

**Windows Session Monitoring**:
```rust
// Listen for Windows session events
fn on_session_event(event: SessionEvent) {
    match event {
        SessionEvent::Lock => {
            info!("Screen locked, pausing capture");
            capture_engine.pause();
        }
        SessionEvent::Unlock => {
            info!("Screen unlocked, resuming capture");
            capture_engine.resume();
        }
        _ => {}
    }
}
```

**Configuration**:
```toml
[privacy]
pause_on_lock = true  # Auto-pause when screen locked
```

### 10.4 Data Isolation

**File System Security**:
```
C:\Users\<User>\AppData\Local\ScreenMemories\
├── screensearch.db       (User read/write only)
├── screensearch.db-wal   (User read/write only)
├── screensearch.db-shm   (User read/write only)
└── captures\                (User read/write only)
    └── *.png                (User read/write only)

Windows ACLs:
- Owner: Current user
- Permissions: Full control (current user only)
- No network share access
```

**No Network Communication**:
- API binds only to 127.0.0.1 (localhost)
- No outbound network requests
- No telemetry or analytics
- No cloud sync (future feature would be opt-in)

### 10.5 Data Retention & Cleanup

**Automatic Cleanup**:
```rust
// Example: Delete data older than 90 days
let retention_days = 90;
db.cleanup_old_data(retention_days).await?;

// Deletes:
// - Frame records (CASCADE deletes OCR text and tags)
// - FTS index entries (via trigger)
// - Image files on disk
```

**Manual Deletion**:
```bash
# Delete specific frame
curl -X DELETE http://localhost:3131/frames/12345

# Delete all data (nuclear option)
rm screensearch.db
rm -r captures/
```

**Secure Deletion** (future enhancement):
```rust
// Overwrite file contents before deletion (DoD 5220.22-M)
fn secure_delete(path: &Path) -> Result<()> {
    let file_size = fs::metadata(path)?.len();
    let mut file = OpenOptions::new().write(true).open(path)?;

    // Pass 1: Write zeros
    file.write_all(&vec![0u8; file_size as usize])?;
    // Pass 2: Write ones
    file.write_all(&vec![0xFFu8; file_size as usize])?;
    // Pass 3: Write random
    file.write_all(&random_bytes(file_size as usize))?;

    fs::remove_file(path)?;  // Final deletion
    Ok(())
}
```

### 10.6 API Security

**Localhost-Only Binding**:
```rust
// API server configuration
let addr = SocketAddr::from(([127, 0, 0, 1], 3131));
// Only accessible from same machine
// Not accessible from network (even local network)
```

**No Authentication** (current design):
- Assumes single-user, trusted local environment
- No password or API key required
- Appropriate for personal productivity tool

**Future: Optional Authentication** (not implemented):
```rust
// Middleware for API key validation
async fn require_auth(
    headers: HeaderMap,
    req: Request<Body>,
    next: Next<Body>,
) -> Response {
    if let Some(api_key) = headers.get("X-API-Key") {
        if api_key == config.api_key {
            return next.run(req).await;
        }
    }
    StatusCode::UNAUTHORIZED.into_response()
}
```

### 10.7 Sensitive Data Handling

**OCR Text Storage**:
```rust
// Store exact text as captured
// No filtering of potentially sensitive content (by design)
// Responsibility: User must configure excluded_apps appropriately

// Future: Optional PII detection (not implemented)
fn detect_sensitive_data(text: &str) -> bool {
    // Patterns:
    // - Credit card numbers: \d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}
    // - Social Security: \d{3}-\d{2}-\d{4}
    // - Email: \S+@\S+\.\S+
    // - Phone: \d{3}[- ]?\d{3}[- ]?\d{4}

    // If detected: Flag frame for review or auto-redact
}
```

**Bounding Box Privacy**:
- OCR text includes screen coordinates
- Can reveal UI layout and application state
- Future: Option to store only text, not coordinates

### 10.8 Windows Security Context

**Privilege Requirements**:
- **Standard User**: Can capture own desktop and windows
- **Administrator**: Not required (recommended to run as standard user)
- **UIAutomation API**: Requires same integrity level as target app

**Windows Defender SmartScreen**:
- Unsigned binary may trigger warnings
- Recommend: Code signing certificate for distribution
- Open source allows users to build from source

**Anti-Virus Considerations**:
- Screen capture may be flagged as "keylogger" behavior
- OCR and window monitoring can trigger heuristics
- Mitigation: Submit to AV vendors for whitelisting

---

## 11. Extension Points

### 11.1 Plugin Architecture (Future)

**Proposed Interface**:
```rust
pub trait ScreenMemoriesPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;

    // Lifecycle hooks
    fn on_init(&mut self) -> Result<()>;
    fn on_shutdown(&mut self) -> Result<()>;

    // Event hooks
    fn on_frame_captured(&self, frame: &CapturedFrame) -> Result<()>;
    fn on_ocr_completed(&self, result: &OcrResult) -> Result<()>;
    fn on_frame_stored(&self, frame_id: i64) -> Result<()>;
}

// Plugin loading
let plugin: Box<dyn ScreenMemoriesPlugin> =
    load_plugin("plugins/my_plugin.dll")?;
plugin.on_init()?;
```

**Plugin Types**:

1. **Capture Filters**:
```rust
trait CaptureFilter: ScreenMemoriesPlugin {
    // Modify frame before OCR
    fn filter_frame(&self, frame: &mut CapturedFrame) -> Result<()>;

    // Example: Blur sensitive regions, crop to window, etc.
}
```

2. **OCR Engines**:
```rust
trait OcrEnginePlugin: ScreenMemoriesPlugin {
    // Alternative OCR implementation
    fn process_image(&self, image: &RgbaImage) -> Result<OcrResult>;

    // Example: Tesseract, Google Vision, AWS Textract
}
```

3. **Storage Backends**:
```rust
trait StorageBackend: ScreenMemoriesPlugin {
    // Alternative storage (cloud, encrypted, etc.)
    fn store_frame(&self, frame: &ProcessedFrame) -> Result<i64>;
    fn query_frames(&self, filter: &FrameFilter) -> Result<Vec<FrameRecord>>;

    // Example: S3, Azure Blob, encrypted SQLite
}
```

4. **API Extensions**:
```rust
trait ApiExtension: ScreenMemoriesPlugin {
    // Add custom endpoints
    fn routes(&self) -> Vec<(String, Handler)>;

    // Example: Custom search algorithms, integrations, webhooks
}
```

### 11.2 Custom OCR Confidence Filtering

**Current**: Fixed threshold (0.7)

**Extension Point**:
```rust
pub trait ConfidenceFilter: Send + Sync {
    fn should_keep_region(&self, region: &TextRegion, context: &FrameContext) -> bool;
}

// Example: Adaptive threshold based on content
struct AdaptiveConfidenceFilter {
    min_confidence: f32,
}

impl ConfidenceFilter for AdaptiveConfidenceFilter {
    fn should_keep_region(&self, region: &TextRegion, context: &FrameContext) -> bool {
        // Higher threshold for password-looking text
        if region.text.chars().all(|c| c.is_alphanumeric())
            && region.text.len() > 8 {
            return region.confidence > 0.9;
        }

        // Lower threshold for UI labels
        if region.height < 20 {
            return region.confidence > 0.6;
        }

        region.confidence > self.min_confidence
    }
}
```

### 11.3 Custom Frame Differencing Algorithms

**Current**: Simple pixel-wise comparison

**Extension Point**:
```rust
pub trait FrameDiffer: Send + Sync {
    fn has_changed(&self, prev: &RgbaImage, curr: &RgbaImage) -> bool;
}

// Example: Perceptual hash comparison
struct PerceptualHashDiffer {
    threshold: u32,
}

impl FrameDiffer for PerceptualHashDiffer {
    fn has_changed(&self, prev: &RgbaImage, curr: &RgbaImage) -> bool {
        let hash1 = perceptual_hash(prev);
        let hash2 = perceptual_hash(curr);
        hamming_distance(hash1, hash2) > self.threshold
    }
}

// Example: Region-based comparison (focus on specific areas)
struct RegionDiffer {
    regions: Vec<Rect>,  // Areas to monitor
}
```

### 11.4 Custom Database Queries

**Extension Point**:
```rust
// Allow custom SQL queries via API
POST /query/custom
{
    "query": "SELECT * FROM frames WHERE active_process LIKE ? LIMIT ?",
    "params": ["chrome%", 100]
}

// Security: Whitelist of allowed operations
// - Only SELECT queries
// - No DDL (CREATE, DROP, ALTER)
// - Rate limiting
```

### 11.5 Webhook Integration

**Extension Point**:
```rust
pub struct WebhookConfig {
    pub url: String,
    pub events: Vec<WebhookEvent>,
    pub headers: HashMap<String, String>,
}

pub enum WebhookEvent {
    FrameCaptured,
    OcrCompleted,
    HighConfidenceText,  // OCR confidence > 0.95
    SensitiveAppDetected,
}

// Send HTTP POST with event data
async fn trigger_webhook(config: &WebhookConfig, event: &Event) {
    let payload = serde_json::json!({
        "event": event.event_type(),
        "timestamp": event.timestamp(),
        "data": event.data(),
    });

    reqwest::Client::new()
        .post(&config.url)
        .headers(config.headers.clone())
        .json(&payload)
        .send()
        .await?;
}
```

### 11.6 Vector Embeddings for Semantic Search

**Future Enhancement** (using sqlite-vec):
```rust
// Generate embeddings for OCR text
pub trait EmbeddingGenerator: Send + Sync {
    fn generate(&self, text: &str) -> Result<Vec<f32>>;
}

// Store in database
CREATE VIRTUAL TABLE ocr_embeddings USING vec0(
    ocr_text_id INTEGER PRIMARY KEY,
    embedding FLOAT[384]  -- Example: MiniLM embedding dimension
);

// Semantic search
SELECT
    o.text,
    vec_distance_cosine(e.embedding, ?) AS similarity
FROM ocr_embeddings e
JOIN ocr_text o ON e.ocr_text_id = o.id
ORDER BY similarity DESC
LIMIT 50;
```

### 11.7 Browser Extension Integration

**Proposed Architecture**:
```
Browser Extension (Chrome/Firefox)
    ↓ (WebSocket)
ScreenSearch API Server
    ↓
Enhanced capture with browser context:
- Current tab URL
- Page title
- Selected text
- Form data (opt-in)
```

**WebSocket Endpoint**:
```rust
// ws://localhost:3131/ws
async fn websocket_handler(ws: WebSocket, state: Arc<AppState>) {
    while let Some(msg) = ws.next().await {
        match msg {
            Message::Text(json) => {
                let event: BrowserEvent = serde_json::from_str(&json)?;
                handle_browser_event(event, &state).await?;
            }
            _ => {}
        }
    }
}
```

### 11.8 Real-time Notifications

**Extension Point**:
```rust
// Notify when specific text is detected
pub struct TextWatcher {
    patterns: Vec<Regex>,
    callback: Box<dyn Fn(&TextRegion) + Send + Sync>,
}

impl TextWatcher {
    pub fn watch(&self, ocr_result: &OcrResult) {
        for region in &ocr_result.regions {
            for pattern in &self.patterns {
                if pattern.is_match(&region.text) {
                    (self.callback)(region);
                }
            }
        }
    }
}

// Example: Alert when "Error" appears on screen
let watcher = TextWatcher {
    patterns: vec![Regex::new(r"(?i)error|exception|failed").unwrap()],
    callback: Box::new(|region| {
        // Send desktop notification
        notify_user(&format!("Error detected: {}", region.text));
    }),
};
```

### 11.9 Configuration UI

**Future: Web-based UI** (not implemented):
```
http://localhost:3131/settings

Settings Page:
- Capture interval slider
- Monitor selection checkboxes
- Privacy: Excluded apps (text area)
- OCR: Confidence threshold slider
- Database: Retention period
- View statistics
- Manage tags
```

### 11.10 API Client Libraries

**Proposed SDKs**:
```rust
// Rust
let client = ScreenMemoriesClient::new("http://localhost:3131")?;
let results = client.search("database query").await?;

// Python
from screensearch import Client
client = Client("http://localhost:3131")
results = client.search("database query")

// JavaScript
import { ScreenMemoriesClient } from 'screensearch-js';
const client = new ScreenMemoriesClient('http://localhost:3131');
const results = await client.search('database query');
```

---

## Conclusion

This architecture document provides a comprehensive overview of the ScreenSearch system design. The modular workspace-based architecture enables independent development and testing of components while maintaining clear interfaces and data flow.

**Key Architectural Strengths**:
- **Performance**: Async runtime, connection pooling, frame differencing, hybrid search
- **Privacy**: Local-only, application exclusion, user control
- **Scalability**: Connection pooling, FTS5 indexing, WAL mode, in-memory vector search
- **Maintainability**: Clear separation of concerns, type-safe queries
- **Extensibility**: Plugin hooks, custom filters, API extensions
- **Intelligence**: RAG-powered semantic search with ONNX embeddings

**For Implementation Details**:
- Database schema: `screen-db/DATABASE_DESIGN.md`
- OCR pipeline: `screen-capture/OCR_IMPLEMENTATION.md`
- API endpoints: `screen-api/README.md` (when created)
- Configuration: `config.toml` in project root

**Development Resources**:
- Source code: Workspace crates in project root
- Examples: `examples/` directory
- Tests: `cargo test` in each crate
- Benchmarks: `cargo bench` in screen-db

---

**Document Version**: 2.0
**Architecture Version**: 0.2.0
**Last Updated**: 2025-12-13
**Contributors**: ScreenSearch Team
