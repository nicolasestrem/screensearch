# Code Navigation Guide

## 🗺️ Quick Reference for Navigating the ScreenSearch Codebase

This guide helps you quickly find the code you need, whether you're debugging, adding features, or understanding how the system works.

---

## 📁 Top-Level Project Structure

```
screensearch/
├── src/                           # Main binary entry point
│   └── main.rs                   # Application orchestration, service initialization
│
├── screen-capture/               # Capture & OCR workspace crate
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
├── screen-db/                    # Database workspace crate
│   ├── src/
│   │   ├── db.rs                # DatabaseManager, connection pool
│   │   ├── queries.rs           # SQL queries, FTS5 search
│   │   ├── models.rs            # Data models (Frame, OcrText, Tags)
│   │   ├── migrations.rs        # Schema versioning
│   │   └── lib.rs               # Public API exports
│   ├── tests/
│   │   └── integration_tests.rs # Database integration tests
│   └── Cargo.toml
│
├── screen-api/                   # REST API workspace crate
│   ├── src/
│   │   ├── server.rs            # Axum server initialization
│   │   ├── routes.rs            # Route definitions (27 endpoints)
│   │   ├── handlers/            # Request handlers by domain
│   │   │   ├── mod.rs           # Handler module exports
│   │   │   ├── search.rs        # Search & query handlers
│   │   │   ├── automation.rs    # UI automation handlers
│   │   │   └── system.rs        # Health, stats, metrics
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
├── screen-automation/            # Windows UI automation workspace crate
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
├── screen-ui/                    # React web dashboard (optional)
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

## 🔍 Find Code by Feature

### Screen Capture

| What | Where | File:Line |
|------|-------|-----------|
| Start/stop capture | `screen-capture/src/capture.rs` | `CaptureEngine::start()`, `::stop()` |
| Frame differencing logic | `screen-capture/src/frame_diff.rs` | `FrameDiff::is_different()` |
| Monitor detection | `screen-capture/src/monitor.rs` | `Monitor::list_monitors()` |
| Active window tracking | `screen-capture/src/window_context.rs` | `WindowContext::get_active_window()` |
| Capture configuration | `src/main.rs` | Lines 101-109, 296-299 |

### OCR Processing

| What | Where | File:Line |
|------|-------|-----------|
| Windows OCR API wrapper | `screen-capture/src/ocr.rs` | `WindowsOcr::extract_text()` |
| Multi-threaded OCR pipeline | `screen-capture/src/ocr_processor.rs` | `OcrProcessor::start_processing()` |
| Zero-copy bitmap creation | `screen-capture/src/ocr.rs` | `create_software_bitmap()` |
| OCR metrics & monitoring | `screen-capture/src/ocr_processor.rs` | `OcrMetrics` struct |
| OCR configuration | `src/main.rs` | Lines 110-120, 285-292 |

### Database

| What | Where | File:Line |
|------|-------|-----------|
| Database connection | `screen-db/src/db.rs` | `DatabaseManager::new()`, `::with_config()` |
| FTS5 full-text search | `screen-db/src/queries.rs` | `search_text()`, `search_advanced()` |
| Frame insertion | `screen-db/src/db.rs` | `insert_frame()` |
| OCR text insertion | `screen-db/src/db.rs` | `insert_ocr_text()` |
| Schema migrations | `screen-db/src/migrations.rs` | `run_migrations()` |
| Query sanitization | `screen-db/src/queries.rs` | `sanitize_fts5_query()` |
| Database models | `screen-db/src/models.rs` | `Frame`, `OcrText`, `Tag` structs |

### REST API

| What | Where | File:Line |
|------|-------|-----------|
| Server initialization | `screen-api/src/server.rs` | `ApiServer::new()`, `::run()` |
| Route definitions | `screen-api/src/routes.rs` | `create_router()` |
| Search endpoints | `screen-api/src/handlers/search.rs` | `search_handler()`, `advanced_search_handler()` |
| Automation endpoints | `screen-api/src/handlers/automation.rs` | `click_handler()`, `type_handler()` |
| Health & stats | `screen-api/src/handlers/system.rs` | `health_handler()`, `stats_handler()` |
| API error handling | `screen-api/src/error.rs` | `ApiError` enum |
| Request/response models | `screen-api/src/models.rs` | API types |

### UI Automation

| What | Where | File:Line |
|------|-------|-----------|
| Automation engine | `screen-automation/src/engine.rs` | `AutomationEngine::new()` |
| Element finding | `screen-automation/src/element.rs` | `find_elements()`, `find_element()` |
| Mouse control | `screen-automation/src/input.rs` | `click()`, `move_mouse()` |
| Keyboard control | `screen-automation/src/input.rs` | `type_text()`, `send_keys()` |
| Window management | `screen-automation/src/window.rs` | `Window::find()`, `::activate()` |
| Element selectors | `screen-automation/src/selector.rs` | `Selector` struct |

### Main Application

| What | Where | File:Line |
|------|-------|-----------|
| Application entry point | `src/main.rs` | `main()` at line 521 |
| Configuration loading | `src/main.rs` | `AppConfig::load()` at line 162 |
| Service orchestration | `src/main.rs` | `App::run()` at line 270 |
| Frame processing pipeline | `src/main.rs` | Lines 326-392 |
| Graceful shutdown | `src/main.rs` | Lines 425-452 |
| Database frame storage | `src/main.rs` | `store_processed_frame()` at line 457 |

---

## 🧩 Common Tasks - Where to Start

### Adding a New Capture Source

1. **Add source to capture engine**: `screen-capture/src/capture.rs`
2. **Update configuration**: `src/main.rs` → `CaptureSettings` struct
3. **Update documentation**: `docs/user-guide.md`

### Improving OCR Accuracy

1. **Tune OCR parameters**: `screen-capture/src/ocr.rs`
2. **Adjust confidence threshold**: `src/main.rs:112` → `min_confidence`
3. **Add preprocessing**: `screen-capture/src/ocr.rs` → before OCR call
4. **Update tests**: `screen-capture/tests/`

### Adding a New API Endpoint

1. **Define route**: `screen-api/src/routes.rs` → `create_router()`
2. **Create handler**: `screen-api/src/handlers/` → new function
3. **Add request/response models**: `screen-api/src/models.rs`
4. **Update API reference**: `docs/api-reference.md`
5. **Add integration test**: `screen-api/tests/integration_tests.rs`

### Optimizing Database Queries

1. **Review query**: `screen-db/src/queries.rs`
2. **Check indexes**: `screen-db/src/migrations.rs`
3. **Analyze with EXPLAIN**: Add logging to query execution
4. **Update connection pool**: `src/main.rs:126-133`
5. **Benchmark**: `screen-db/tests/integration_tests.rs`

### Adding UI Automation Features

1. **Extend automation engine**: `screen-automation/src/engine.rs`
2. **Add element selectors**: `screen-automation/src/selector.rs`
3. **Update input controls**: `screen-automation/src/input.rs`
4. **Create example**: `screen-automation/examples/`
5. **Add API endpoint**: `screen-api/src/handlers/automation.rs`

---

## 🔧 Configuration - Where to Find It

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

## 📊 Data Flow - Follow the Data

### Capture → Database Flow

```
1. CaptureEngine captures frame
   📂 screen-capture/src/capture.rs:322-350

2. Frame sent to OCR processor
   📂 src/main.rs:336 (frame_tx.send())

3. OCR processes frame
   📂 screen-capture/src/ocr_processor.rs:353

4. ProcessedFrame sent to database
   📂 src/main.rs:367 (processed_rx.recv())

5. Frame stored in database
   📂 src/main.rs:457 (store_processed_frame())
   📂 screen-db/src/db.rs:insert_frame()
```

### Search Query Flow

```
1. API receives search request
   📂 screen-api/src/handlers/search.rs:search_handler()

2. Query sanitized for FTS5
   📂 screen-db/src/queries.rs:sanitize_fts5_query()

3. FTS5 search executed
   📂 screen-db/src/queries.rs:search_text()

4. Results formatted as JSON
   📂 screen-api/src/handlers/search.rs

5. Response sent to client
   📂 screen-api/src/server.rs
```

### Automation Flow

```
1. API receives automation request
   📂 screen-api/src/handlers/automation.rs:click_handler()

2. AutomationEngine invoked
   📂 screen-automation/src/engine.rs:execute()

3. Input control executed
   📂 screen-automation/src/input.rs:click()

4. Result returned to API
   📂 screen-api/src/handlers/automation.rs

5. Response sent to client
   📂 screen-api/src/server.rs
```

---

## 🧪 Testing - Where to Add Tests

### Unit Tests

| Component | Test Location |
|-----------|---------------|
| **Capture Engine** | `screen-capture/src/capture.rs` → inline `#[cfg(test)]` modules |
| **OCR Processor** | `screen-capture/src/ocr_processor.rs` → inline tests |
| **Frame Differencing** | `screen-capture/src/frame_diff.rs` → inline tests |
| **Database Queries** | `screen-db/src/queries.rs` → inline tests |
| **Query Sanitization** | `screen-db/src/queries.rs` → inline tests |

### Integration Tests

| Component | Test Location |
|-----------|---------------|
| **Database** | `screen-db/tests/integration_tests.rs` |
| **API Server** | `screen-api/tests/integration_tests.rs` |
| **Automation** | `screen-automation/tests/integration_tests.rs` |

### Test Commands

```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p screen-db
cargo test -p screen-api
cargo test -p screen-capture
cargo test -p screen-automation

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
// 📂 screen-capture/src/capture.rs
info!("Captured frame from monitor {}", monitor_index);

// OCR processor
// 📂 screen-capture/src/ocr_processor.rs
debug!("OCR processing frame {} with {} regions", frame_id, regions.len());

// Database
// 📂 screen-db/src/db.rs
trace!("Executing query: {}", sql);

// API
// 📂 screen-api/src/handlers/search.rs
warn!("Search query returned 0 results for: {}", query);
```

---

## 🚀 Performance - Where to Optimize

### Critical Performance Paths

| Path | File | Key Metrics |
|------|------|-------------|
| **OCR Processing** | `screen-capture/src/ocr.rs` | Target: < 100ms per frame |
| **Frame Differencing** | `screen-capture/src/frame_diff.rs` | Arc-based, zero-copy |
| **Database Insertion** | `screen-db/src/db.rs` | Batched inserts |
| **FTS5 Search** | `screen-db/src/queries.rs` | Indexed search, < 50ms |
| **API Response** | `screen-api/src/handlers/` | Total: < 100ms |

### Performance Monitoring

- **OCR Metrics**: `screen-capture/src/ocr_processor.rs` → `OcrMetrics`
- **Database Stats**: `screen-db/src/db.rs` → query timing
- **API Metrics**: `screen-api/src/handlers/system.rs` → `stats_handler()`

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
| **windows** | `screen-capture/src/ocr.rs` | Windows OCR API |
| **windows-capture** | `screen-capture/src/capture.rs` | Screen capture |
| **uiautomation** | `screen-automation/src/` | UI automation |

### Core Libraries

| Dependency | Used In | Purpose |
|------------|---------|---------|
| **tokio** | All crates | Async runtime |
| **sqlx** | `screen-db/` | Database access |
| **axum** | `screen-api/` | HTTP server |
| **image** | `screen-capture/` | Image processing |
| **serde/serde_json** | All crates | Serialization |

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

**Last Updated**: 2025-12-10
**Version**: 0.1.0
