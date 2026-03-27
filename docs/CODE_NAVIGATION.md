# Code Navigation Guide

## [>] Quick Reference for Navigating the ScreenSearch Codebase

This guide helps you quickly find the code you need, whether you're debugging, adding features, or understanding how the system works.

---

## 📁 Top-Level Project Structure

```
screensearch/
├── src/                           # Main binary entry point
│   └── main.rs                   # Application orchestration, service initialization
│
├── screensearch-capture/         # Capture & OCR workspace crate
│   ├── src/
│   │   ├── capture.rs           # Core capture engine, frame differencing
│   │   ├── ocr.rs               # Windows OCR API wrapper
│   │   ├── ocr_processor.rs     # Multi-threaded OCR pipeline
│   │   ├── frame_diff.rs        # Arc-based frame comparison
│   │   ├── monitor.rs           # Monitor detection & selection
│   │   ├── window_context.rs    # Active window tracking
│   │   └── lib.rs               # Public API exports
│   ├── examples/                # Standalone demos
│   │   └── ocr_demo.rs          # OCR testing utility
│   └── Cargo.toml
│
├── screensearch-db/              # Database workspace crate
│   ├── src/
│   │   ├── db.rs                # DatabaseManager, connection pool
│   │   ├── queries.rs           # SQL queries, FTS5 search
│   │   ├── models.rs            # Data models (Frame, OcrText, Tags)
│   │   ├── migrations.rs        # Schema versioning
│   │   ├── vector_search.rs     # Vector similarity search for RAG
│   │   └── lib.rs               # Public API exports
│   ├── tests/
│   │   └── integration_tests.rs # Database integration tests
│   └── Cargo.toml
│
├── screensearch-api/             # REST API workspace crate
│   ├── src/
│   │   ├── server.rs            # Axum server initialization
│   │   ├── routes.rs            # Route definitions (27 endpoints)
│   │   ├── handlers/            # Request handlers by domain
│   │   │   ├── mod.rs           # Handler module exports
│   │   │   ├── search.rs        # Search & query handlers
│   │   │   ├── automation.rs    # UI automation handlers
│   │   │   ├── system.rs        # Health, stats, metrics
│   │   │   ├── ai.rs            # AI intelligence endpoints
│   │   │   ├── embeddings.rs    # Embedding management handlers
│   │   │   ├── rag_helpers.rs   # Hybrid search orchestration (207 lines)
│   │   │   └── reranker.rs      # Reranking algorithm (202 lines)
│   │   ├── workers/             # Background processing
│   │   │   ├── mod.rs           # Worker module exports
│   │   │   └── embedding_worker.rs # Background embedding processing (184 lines)
│   │   ├── state.rs             # Shared application state
│   │   ├── models.rs            # API request/response types
│   │   ├── error.rs             # API error handling
│   │   ├── lib.rs               # Public API exports
│   │   └── main.rs              # Standalone API server (optional)
│   ├── tests/
│   │   └── integration_tests.rs # API integration tests
│   ├── examples/
│   │   └── client_usage.rs      # Example API client
│   └── Cargo.toml
│
├── screensearch-automation/      # Windows UI automation workspace crate
│   ├── src/
│   │   ├── engine.rs            # Automation orchestration
│   │   ├── element.rs           # UI element detection & interaction
│   │   ├── input.rs             # Mouse & keyboard control
│   │   ├── window.rs            # Window management
│   │   ├── selector.rs          # Element selector patterns
│   │   ├── errors.rs            # Automation error types
│   │   └── lib.rs               # Public API exports
│   ├── tests/
│   │   └── integration_tests.rs # Automation integration tests
│   ├── examples/
│   │   ├── basic_usage.rs       # Simple automation demo
│   │   ├── element_search.rs    # Element finding examples
│   │   ├── mouse_keyboard.rs    # Input control examples
│   │   └── notepad_automation.rs # Notepad interaction demo
│   └── Cargo.toml
│
├── screensearch-embeddings/      # RAG embeddings workspace crate
│   ├── src/
│   │   ├── engine.rs            # ONNX inference engine (284 lines)
│   │   ├── chunker.rs           # Text preprocessing (143 lines)
│   │   ├── download.rs          # Model auto-download (136 lines)
│   │   └── lib.rs               # Public API exports (117 lines)
│   └── Cargo.toml
│
├── screensearch-llm/             # Embedded LLM workspace crate (v0.3.0)
│   ├── src/
│   │   ├── lib.rs               # Public API exports
│   │   ├── config.rs            # LlmConfig, server settings
│   │   ├── download.rs          # Model + binary auto-download
│   │   ├── server.rs            # LlamaServer process management
│   │   ├── engine.rs            # LlmEngine inference client
│   │   ├── profiles.rs          # Inference profiles (VisionAnalysis, etc.)
│   │   └── error.rs             # LlmError types
│   └── Cargo.toml
│
├── screensearch-ui/              # React web dashboard (optional)
│   ├── src/
│   │   ├── components/          # React components
│   │   └── api/                 # Frontend API client
│   └── package.json
│
├── docs/                         # Documentation
│   ├── PROJECT_INDEX.md         # Comprehensive project index (START HERE)
│   ├── CODE_NAVIGATION.md       # This file
│   ├── api-reference.md         # REST API documentation
│   ├── architecture.md          # System architecture
│   ├── developer-guide.md       # Development setup
│   ├── user-guide.md            # User installation & usage
│   ├── testing.md               # Test protocols
│   └── archived/                # Historical documentation
│
├── config.toml                   # User configuration (created by user)
├── Cargo.toml                    # Workspace manifest
└── Cargo.lock                    # Dependency lockfile
```

---

## [>] Find Code by Feature

### Screen Capture

| What | Where | File:Line |
|------|-------|-----------|
| Start/stop capture | `screensearch-capture/src/capture.rs` | `CaptureEngine::start()`, `::stop()` |
| Frame differencing logic | `screensearch-capture/src/frame_diff.rs` | `FrameDiff::is_different()` |
| Monitor detection | `screensearch-capture/src/monitor.rs` | `Monitor::list_monitors()` |
| Active window tracking | `screensearch-capture/src/window_context.rs` | `WindowContext::get_active_window()` |
| Capture configuration | `src/main.rs` | Lines 101-109, 296-299 |

### OCR Processing

| What | Where | File:Line |
|------|-------|-----------|
| Windows OCR API wrapper | `screensearch-capture/src/ocr.rs` | `WindowsOcr::extract_text()` |
| Multi-threaded OCR pipeline | `screensearch-capture/src/ocr_processor.rs` | `OcrProcessor::start_processing()` |
| Zero-copy bitmap creation | `screensearch-capture/src/ocr.rs` | `create_software_bitmap()` |
| OCR metrics & monitoring | `screensearch-capture/src/ocr_processor.rs` | `OcrMetrics` struct |
| OCR configuration | `src/main.rs` | Lines 110-120, 285-292 |

### Database

| What | Where | File:Line |
|------|-------|-----------|
| Database connection | `screensearch-db/src/db.rs` | `DatabaseManager::new()`, `::with_config()` |
| FTS5 full-text search | `screensearch-db/src/queries.rs` | `search_text()`, `search_advanced()` |
| Vector similarity search | `screensearch-db/src/vector_search.rs` | `search_by_embedding()`, `get_nearest_neighbors()` |
| Frame insertion | `screensearch-db/src/db.rs` | `insert_frame()` |
| OCR text insertion | `screensearch-db/src/db.rs` | `insert_ocr_text()` |
| Schema migrations | `screensearch-db/src/migrations.rs` | `run_migrations()` |
| Query sanitization | `screensearch-db/src/queries.rs` | `sanitize_fts5_query()` |
| Database models | `screensearch-db/src/models.rs` | `Frame`, `OcrText`, `Tag` structs |

### REST API

| What | Where | File:Line |
|------|-------|-----------|
| Server initialization | `screensearch-api/src/server.rs` | `ApiServer::new()`, `::run()` |
| Route definitions | `screensearch-api/src/routes.rs` | `create_router()` |
| Search endpoints | `screensearch-api/src/handlers/search.rs` | `search_handler()`, `advanced_search_handler()` |
| Automation endpoints | `screensearch-api/src/handlers/automation.rs` | `click_handler()`, `type_handler()` |
| Health & stats | `screensearch-api/src/handlers/system.rs` | `health_handler()`, `stats_handler()` |
| AI intelligence | `screensearch-api/src/handlers/ai.rs` | `generate_handler()`, `validate_handler()` |
| Embedding management | `screensearch-api/src/handlers/embeddings.rs` | `status_handler()`, `generate_handler()` |
| RAG hybrid search | `screensearch-api/src/handlers/rag_helpers.rs` | `hybrid_search()`, `prepare_context()` |
| Result reranking | `screensearch-api/src/handlers/reranker.rs` | `rerank_results()` |
| API error handling | `screensearch-api/src/error.rs` | `ApiError` enum |
| Request/response models | `screensearch-api/src/models.rs` | API types |

### UI Automation

| What | Where | File:Line |
|------|-------|-----------|
| Automation engine | `screensearch-automation/src/engine.rs` | `AutomationEngine::new()` |
| Element finding | `screensearch-automation/src/element.rs` | `find_elements()`, `find_element()` |
| Mouse control | `screensearch-automation/src/input.rs` | `click()`, `move_mouse()` |
| Keyboard control | `screensearch-automation/src/input.rs` | `type_text()`, `send_keys()` |
| Window management | `screensearch-automation/src/window.rs` | `Window::find()`, `::activate()` |
| Element selectors | `screensearch-automation/src/selector.rs` | `Selector` struct |

### RAG Embeddings

| What | Where | File:Line |
|------|-------|-----------|
| Embedding generation | `screensearch-embeddings/src/engine.rs` | `EmbeddingEngine::embed()`, `::embed_batch()` |
| Text chunking | `screensearch-embeddings/src/chunker.rs` | `TextChunker::chunk()` |
| Model auto-download | `screensearch-embeddings/src/download.rs` | `download_model()`, `needs_download()` |
| Background processing | `screensearch-api/src/workers/embedding_worker.rs` | `start_embedding_worker()` |

### Embedded LLM (v0.3.0)

| What | Where | File:Line |
|------|-------|-----------|
| LLM configuration | `screensearch-llm/src/config.rs` | `LlmConfig` struct |
| Model download | `screensearch-llm/src/download.rs` | `download_model()`, `download_llama_server()` |
| Server management | `screensearch-llm/src/server.rs` | `LlamaServer::start()`, `::stop()`, `::ensure_running()` |
| Inference engine | `screensearch-llm/src/engine.rs` | `LlmEngine::generate()`, `::generate_with_image()` |
| Inference profiles | `screensearch-llm/src/profiles.rs` | `InferenceProfile::VisionAnalysis`, `::RagAnswer` |
| AI API handlers | `screensearch-api/src/handlers/ai.rs` | `generate_handler()`, `model_status()`, `model_download()` |
| Local provider detection | `screensearch-api/src/handlers/ai.rs:310-322` | `provider_url == "local"` branch |

**Key Constants:**
- Default port: `31130` (fallbacks: `31131`, `31132`)
- Model: `Ministral-3B-Instruct-2512-Q4_K_M.gguf` (~2.15 GB)
- Binary: `llama-b7562-bin-win-vulkan-x64.zip`
- Idle TTL: 300 seconds (configurable)

### Main Application

| What | Where | File:Line |
|------|-------|-----------|
| Application entry point | `src/main.rs` | `main()` at line 521 |
| Configuration loading | `src/main.rs` | `AppConfig::load()` at line 162 |
| Service orchestration | `src/main.rs` | `App::run()` at line 270 |
| Frame processing pipeline | `src/main.rs` | Lines 326-392 |
| Graceful shutdown | `src/main.rs` | Lines 425-452 |
| Database frame storage | `src/main.rs` | `store_processed_frame()` at line 457 |

### Frontend Navigation (screensearch-ui)

| What | Where | File |
|------|-------|------|
| **Main Application Layout** | `screensearch-ui/src/App.tsx` | Root layout, Grid background, Footer integration |
| **Timeline View** | `screensearch-ui/src/components/Timeline.tsx` | Main timeline container and logic |
| **Activity Graph** | `screensearch-ui/src/components/timeline/ActivityGraph.tsx` | Density visualization (Daily activity bars) |
| **Sidebar Navigation** | `screensearch-ui/src/components/Sidebar.tsx` | Main navigation menu |
| **Search Functionality** | `screensearch-ui/src/components/SearchBar.tsx` | Global search input |
| **Frame Display** | `screensearch-ui/src/components/FrameCard.tsx` | Individual capture card |
| **Embeddings Status** | `screensearch-ui/src/components/EmbeddingsStatus.tsx` | RAG embedding generation status |
| **Settings Panel** | `screensearch-ui/src/components/SettingsPanel.tsx` | Application configuration |
| **Footer** | `screensearch-ui/src/components/Footer.tsx` | App footer with credits |
| **Data Hooks** | `screensearch-ui/src/hooks/` | `useDailyActivity`, `useFrames` |

---

## 🧩 Common Tasks - Where to Start

### Adding a New Capture Source

1. **Add source to capture engine**: `screensearch-capture/src/capture.rs`
2. **Update configuration**: `src/main.rs` → `CaptureSettings` struct
3. **Update documentation**: `docs/user-guide.md`

### Improving OCR Accuracy

1. **Tune OCR parameters**: `screensearch-capture/src/ocr.rs`
2. **Adjust confidence threshold**: `src/main.rs:112` → `min_confidence`
3. **Add preprocessing**: `screensearch-capture/src/ocr.rs` → before OCR call
4. **Update tests**: `screensearch-capture/tests/`

### Adding a New API Endpoint

1. **Define route**: `screensearch-api/src/routes.rs` → `create_router()`
2. **Create handler**: `screensearch-api/src/handlers/` → new function
3. **Add request/response models**: `screensearch-api/src/models.rs`
4. **Update API reference**: `docs/api-reference.md`
5. **Add integration test**: `screensearch-api/tests/integration_tests.rs`

### Optimizing Database Queries

1. **Review query**: `screensearch-db/src/queries.rs`
2. **Check indexes**: `screensearch-db/src/migrations.rs`
3. **Analyze with EXPLAIN**: Add logging to query execution
4. **Update connection pool**: `src/main.rs:126-133`
5. **Benchmark**: `screensearch-db/tests/integration_tests.rs`

### Adding UI Automation Features

1. **Extend automation engine**: `screensearch-automation/src/engine.rs`
2. **Add element selectors**: `screensearch-automation/src/selector.rs`
3. **Update input controls**: `screensearch-automation/src/input.rs`
4. **Create example**: `screensearch-automation/examples/`
5. **Add API endpoint**: `screensearch-api/src/handlers/automation.rs`

### Improving RAG Search Quality

1. **Tune embedding model**: `screensearch-embeddings/src/engine.rs`
2. **Adjust text chunking**: `screensearch-embeddings/src/chunker.rs`
3. **Modify hybrid search weights**: `screensearch-api/src/handlers/rag_helpers.rs`
4. **Improve reranking**: `screensearch-api/src/handlers/reranker.rs`
5. **Monitor embedding worker**: `screensearch-api/src/workers/embedding_worker.rs`

---

## [>] Configuration - Where to Find It

### Runtime Configuration

| Setting | File | Location |
|---------|------|----------|
| **Capture interval** | `config.toml` | `[capture] interval_ms` |
| **OCR confidence threshold** | `config.toml` | `[ocr] min_confidence` |
| **API port** | `config.toml` | `[api] port` |
| **Database path** | `config.toml` | `[database] path` |
| **Excluded apps** | `config.toml` | `[privacy] excluded_apps` |
| **Logging level** | `config.toml` | `[logging] level` |

**Defaults**: See `src/main.rs:98-158` → `AppConfig::default()`

### Build Configuration

| Setting | File | Location |
|---------|------|----------|
| **Workspace members** | `Cargo.toml` | Lines 2-8 |
| **Shared dependencies** | `Cargo.toml` | Lines 55-92 |
| **Release optimizations** | `Cargo.toml` | Lines 93-97 |
| **Development profile** | `Cargo.toml` | Lines 99-101 |

---

## [>] Data Flow - Follow the Data

### Capture → Database Flow

```
1. CaptureEngine captures frame
   📂 screensearch-capture/src/capture.rs:322-350

2. Frame sent to OCR processor
   📂 src/main.rs:336 (frame_tx.send())

3. OCR processes frame
   📂 screensearch-capture/src/ocr_processor.rs:353

4. ProcessedFrame sent to database
   📂 src/main.rs:367 (processed_rx.recv())

5. Frame stored in database
   📂 src/main.rs:457 (store_processed_frame())
   📂 screensearch-db/src/db.rs:insert_frame()
```

### Search Query Flow

```
1. API receives search request
   📂 screensearch-api/src/handlers/search.rs:search_handler()

2. Query sanitized for FTS5
   📂 screensearch-db/src/queries.rs:sanitize_fts5_query()

3. FTS5 search executed
   📂 screensearch-db/src/queries.rs:search_text()

4. Results formatted as JSON
   📂 screensearch-api/src/handlers/search.rs

5. Response sent to client
   📂 screensearch-api/src/server.rs
```

### RAG Hybrid Search Flow

```
1. API receives RAG search request
   📂 screensearch-api/src/handlers/rag_helpers.rs:hybrid_search()

2. Text embedded using ONNX model
   📂 screensearch-embeddings/src/engine.rs:embed()

3. Vector similarity search
   📂 screensearch-db/src/vector_search.rs:search_by_embedding()

4. FTS5 keyword search (parallel)
   📂 screensearch-db/src/queries.rs:search_text()

5. Results reranked by relevance
   📂 screensearch-api/src/handlers/reranker.rs:rerank_results()

6. Top results sent to LLM
   📂 screensearch-api/src/handlers/ai.rs:generate_handler()
```

### Automation Flow

```
1. API receives automation request
   📂 screensearch-api/src/handlers/automation.rs:click_handler()

2. AutomationEngine invoked
   📂 screensearch-automation/src/engine.rs:execute()

3. Input control executed
   📂 screensearch-automation/src/input.rs:click()

4. Result returned to API
   📂 screensearch-api/src/handlers/automation.rs

5. Response sent to client
   📂 screensearch-api/src/server.rs
```

### Embedding Generation Flow

```
1. Background worker checks for frames without embeddings
   📂 screensearch-api/src/workers/embedding_worker.rs:process_pending()

2. Text chunked for optimal embedding
   📂 screensearch-embeddings/src/chunker.rs:chunk()

3. ONNX model generates 384-dim embeddings
   📂 screensearch-embeddings/src/engine.rs:embed_batch()

4. Embeddings stored in database
   📂 screensearch-db/src/db.rs:update_frame_embedding()
```

---

## 🧪 Testing - Where to Add Tests

### Unit Tests

| Component | Test Location |
|-----------|---------------|
| **Capture Engine** | `screensearch-capture/src/capture.rs` → inline `#[cfg(test)]` modules |
| **OCR Processor** | `screensearch-capture/src/ocr_processor.rs` → inline tests |
| **Frame Differencing** | `screensearch-capture/src/frame_diff.rs` → inline tests |
| **Database Queries** | `screensearch-db/src/queries.rs` → inline tests |
| **Vector Search** | `screensearch-db/src/vector_search.rs` → inline tests |
| **Query Sanitization** | `screensearch-db/src/queries.rs` → inline tests |
| **Embedding Engine** | `screensearch-embeddings/src/engine.rs` → inline tests |
| **Text Chunker** | `screensearch-embeddings/src/chunker.rs` → inline tests |

### Integration Tests

| Component | Test Location |
|-----------|---------------|
| **Database** | `screensearch-db/tests/integration_tests.rs` |
| **API Server** | `screensearch-api/tests/integration_tests.rs` |
| **Automation** | `screensearch-automation/tests/integration_tests.rs` |

### Test Commands

```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p screensearch-db
cargo test -p screensearch-api
cargo test -p screensearch-capture
cargo test -p screensearch-automation
cargo test -p screensearch-embeddings

# Run with output
cargo test --workspace -- --nocapture

# Run specific test
cargo test test_fts5_search
```

---

## 🐛 Debugging - Where to Add Logging

### Tracing Initialization

- **Location**: `src/main.rs:228-252` → `init_tracing()`
- **Configuration**: `config.toml` → `[logging]`
- **Environment**: Set `RUST_LOG=debug` for verbose logging

### Adding Tracing

```rust
use tracing::{debug, info, warn, error, trace};

// Example locations to add tracing:

// Capture engine
// 📂 screensearch-capture/src/capture.rs
info!("Captured frame from monitor {}", monitor_index);

// OCR processor
// 📂 screensearch-capture/src/ocr_processor.rs
debug!("OCR processing frame {} with {} regions", frame_id, regions.len());

// Database
// 📂 screensearch-db/src/db.rs
trace!("Executing query: {}", sql);

// API
// 📂 screensearch-api/src/handlers/search.rs
warn!("Search query returned 0 results for: {}", query);

// Embeddings
// 📂 screensearch-embeddings/src/engine.rs
debug!("Generated embedding with dimension: {}", EMBEDDING_DIM);

// Background worker
// 📂 screensearch-api/src/workers/embedding_worker.rs
info!("Processing {} pending frames for embeddings", count);
```

---

## 🚀 Performance - Where to Optimize

### Critical Performance Paths

| Path | File | Key Metrics |
|------|------|-------------|
| **OCR Processing** | `screensearch-capture/src/ocr.rs` | Target: < 100ms per frame |
| **Frame Differencing** | `screensearch-capture/src/frame_diff.rs` | Arc-based, zero-copy |
| **Database Insertion** | `screensearch-db/src/db.rs` | Batched inserts |
| **FTS5 Search** | `screensearch-db/src/queries.rs` | Indexed search, < 50ms |
| **Vector Search** | `screensearch-db/src/vector_search.rs` | Cosine similarity, < 100ms |
| **Embedding Generation** | `screensearch-embeddings/src/engine.rs` | ONNX inference, batch processing |
| **API Response** | `screensearch-api/src/handlers/` | Total: < 100ms |

### Performance Monitoring

- **OCR Metrics**: `screensearch-capture/src/ocr_processor.rs` → `OcrMetrics`
- **Database Stats**: `screensearch-db/src/db.rs` → query timing
- **API Metrics**: `screensearch-api/src/handlers/system.rs` → `stats_handler()`
- **Embedding Stats**: `screensearch-api/src/handlers/embeddings.rs` → `status_handler()`

---

## 📚 Documentation - Where to Update

| When | Update |
|------|--------|
| **New API endpoint** | `docs/api-reference.md`, `docs/PROJECT_INDEX.md` |
| **Architecture change** | `docs/architecture.md`, `docs/PROJECT_INDEX.md` |
| **Configuration option** | `docs/user-guide.md`, `docs/PROJECT_INDEX.md` |
| **Performance improvement** | `docs/performance-optimizations.md` |
| **New feature** | `README.md`, `docs/user-guide.md` |
| **Dependency change** | `docs/developer-guide.md` |

---

## 🔗 External Dependencies - Where They're Used

### Windows-Specific

| Dependency | Used In | Purpose |
|------------|---------|---------|
| **windows** | `screensearch-capture/src/ocr.rs` | Windows OCR API |
| **windows-capture** | `screensearch-capture/src/capture.rs` | Screen capture |
| **uiautomation** | `screensearch-automation/src/` | UI automation |

### Core Libraries

| Dependency | Used In | Purpose |
|------------|---------|---------|
| **tokio** | All crates | Async runtime |
| **sqlx** | `screensearch-db/` | Database access |
| **axum** | `screensearch-api/` | HTTP server |
| **image** | `screensearch-capture/` | Image processing |
| **serde/serde_json** | All crates | Serialization |

### Machine Learning

| Dependency | Used In | Purpose |
|------------|---------|---------|
| **ort** (ONNX Runtime) | `screensearch-embeddings/src/engine.rs` | ML inference |
| **tokenizers** | `screensearch-embeddings/src/engine.rs` | Text tokenization |
| **reqwest** | `screensearch-embeddings/src/download.rs` | Model download |

---

## 💡 Pro Tips for Navigation

1. **Use grep to find usage**: `cargo tree -p screensearch`
2. **Find function definitions**: Search for `pub fn function_name` or `fn function_name`
3. **Find struct definitions**: Search for `pub struct StructName` or `struct StructName`
4. **Find imports**: Search for `use module_name::`
5. **Check documentation**: Run `cargo doc --open` for generated docs
6. **Follow types**: Use IDE "Go to Definition" on type names
7. **Trace data flow**: Start from `src/main.rs` and follow channel sends/receives

---

**Last Updated**: 2025-12-28
**Version**: 0.3.0
