# ScreenSearch Documentation Hub

**Your screen history, searchable and automated**

Welcome to the comprehensive documentation for ScreenSearch — a Windows-native screen capture and OCR system with REST API, AI intelligence, and hybrid search capabilities.

---

## Quick Start

New to ScreenSearch? Start here:

- **Installation** → [User Guide - Getting Started](user-guide.md#installation)
- **First Run** → [User Guide - Quick Start](user-guide.md#quick-start)
- **Configuration** → [User Guide - Configuration](user-guide.md#configuration)
- **First Search** → [API Reference - Search Endpoints](api-reference.md#search-endpoints)

**Quick Links:**
- [Download Latest Release (v0.2.0)](https://github.com/nicolasestrem/screensearch/releases/latest)
- [Build from Source](developer-guide.md#building-from-source)
- [API Playground](http://localhost:3131/) (requires running instance)

---

## Documentation by User Type

### 👤 End Users

Perfect for day-to-day usage, configuration, and troubleshooting.

| Document | Description |
|----------|-------------|
| [User Guide](user-guide.md) | Installation, configuration, web dashboard usage, and privacy controls |
| [API Reference](api-reference.md) | Complete REST API documentation with curl examples for all 27 endpoints |
| [Commands Summary](commands-summary.md) | Quick reference for CLI commands and common workflows |

### 🛠️ Developers

For contributing code, extending functionality, and understanding internals.

| Document | Description |
|----------|-------------|
| [Developer Guide](developer-guide.md) | Development setup, build instructions, and contribution guidelines |
| [Architecture](architecture.md) | System design, data flow, component interaction, and technical decisions |
| [Code Navigation](CODE_NAVIGATION.md) | Find code by feature with file references and line numbers |
| [Testing Guide](testing.md) | Test protocols, coverage reports, and CI/CD pipelines |
| [Performance Optimizations](performance-optimizations.md) | Deep dive into zero-copy OCR, frame differencing, and storage optimizations |

### 🏗️ System Architects

For understanding design patterns, security model, and system integration.

| Document | Description |
|----------|-------------|
| [Architecture Overview](architecture.md) | High-level system design and component relationships |
| [Security & Privacy](security.md) | Security architecture, threat model, and privacy controls |
| [Project Index](PROJECT_INDEX.md) | Comprehensive project overview with implementation status |
| [Performance Metrics](performance-optimizations.md) | Benchmarks, optimization strategies, and performance targets |

---

## Documentation by Feature

### 🔍 Search & Retrieval

- **Full-Text Search (FTS5)** → [Architecture - Search System](architecture.md#search-system)
- **Vector Embeddings** → [Architecture - Embedding Engine](architecture.md#embedding-engine)
- **Hybrid Search** → [API Reference - Search Endpoints](api-reference.md#post-apisearchhybrid)
- **Query Sanitization** → [Security - FTS5 Security](security.md#fts5-query-sanitization)

### 📸 Screen Capture

- **Multi-Monitor Support** → [User Guide - Capture Settings](user-guide.md#capture-settings)
- **Frame Differencing** → [Performance - Zero-Copy Differencing](performance-optimizations.md#zero-copy-frame-differencing)
- **OCR Pipeline** → [Architecture - OCR Processing](architecture.md#ocr-processing)
- **Storage Optimization** → [Performance - JPEG Compression](performance-optimizations.md#storage-optimization)

### 🤖 Automation & Intelligence

- **UI Automation** → [API Reference - Automation Endpoints](api-reference.md#automation-endpoints)
- **AI Intelligence** → [User Guide - Intelligence Dashboard](user-guide.md#intelligence-dashboard)
- **AI Provider Setup** → [API Reference - AI Endpoints](api-reference.md#ai-endpoints)
- **Report Generation** → [Code Navigation - AI Intelligence](CODE_NAVIGATION.md#ai-intelligence)

### 🔒 Privacy & Security

- **Application Exclusion** → [User Guide - Privacy Controls](user-guide.md#privacy-controls)
- **Data Retention** → [User Guide - Storage Management](user-guide.md#storage-management)
- **Security Model** → [Security - Threat Model](security.md#threat-model)
- **Local-Only Storage** → [Architecture - Database Layer](architecture.md#database-layer)

---

## Workspace Crates

ScreenSearch is built as a Cargo workspace with specialized crates:

| Crate | Purpose | Key Files | Documentation |
|-------|---------|-----------|---------------|
| **screensearch-capture** | Screen capture engine with multi-monitor support and OCR pipeline | `capture.rs`, `ocr.rs`, `frame_diff.rs` | [Code Navigation](CODE_NAVIGATION.md#screen-capture) |
| **screensearch-db** | SQLite database manager with FTS5 full-text search | `queries.rs`, `migrations.rs` | [Architecture - Database](architecture.md#database-layer) |
| **screensearch-api** | Axum HTTP server with 27 REST endpoints | `routes.rs`, `handlers/` | [API Reference](api-reference.md) |
| **screensearch-automation** | Windows UIAutomation API wrapper for desktop control | `engine.rs`, `element.rs` | [Code Navigation - Automation](CODE_NAVIGATION.md#ui-automation) |
| **screensearch-embeddings** | Text embedding engine with ONNX Runtime and vector search | `embedder.rs`, `vector_search.rs` | [Architecture - Embeddings](architecture.md#embedding-engine) |

**Main Binary:** `src/main.rs` — Orchestrates all services, manages configuration, and coordinates the frame processing pipeline.

---

## Configuration Reference

ScreenSearch uses `config.toml` for configuration (falls back to defaults if not present).

**Configuration Sections:**

| Section | Purpose | Documentation |
|---------|---------|---------------|
| `[capture]` | Capture intervals, frame differencing, monitor selection | [User Guide - Capture Settings](user-guide.md#capture-settings) |
| `[ocr]` | OCR confidence thresholds, worker threads | [Developer Guide - OCR Configuration](developer-guide.md#ocr-configuration) |
| `[database]` | Database path, WAL mode, cache size | [Architecture - Database](architecture.md#database-layer) |
| `[storage]` | JPEG quality, image resizing, retention policies | [User Guide - Storage Management](user-guide.md#storage-management) |
| `[api]` | API port, host binding, auto-open browser | [API Reference - Configuration](api-reference.md#configuration) |
| `[privacy]` | Excluded applications, pause on lock | [Security - Privacy Controls](security.md#privacy-controls) |

**Example Configuration:**
```toml
[capture]
interval_ms = 3000              # Capture every 3 seconds
enable_frame_diff = true        # Skip unchanged frames
diff_threshold = 0.006          # 0.6% pixel change threshold

[storage]
format = "jpeg"                 # Use JPEG compression (50x storage reduction)
jpeg_quality = 80               # Balance quality and size
max_width = 1920                # Resize to max width

[privacy]
excluded_apps = ["1Password", "KeePass", "Bitwarden"]
pause_on_lock = true            # Auto-pause when screen locks
```

---

## Troubleshooting

### Common Issues

**Issue: Windows SmartScreen Warning on First Launch**
- **Cause:** Executable is not code-signed (requires expensive annual certificate)
- **Solution:** Click "More info" → "Run anyway" → [README - Security & Trust](../README.md#security--trust)
- **Verification:** [VirusTotal Scan Results](https://www.virustotal.com/gui/file/807707d80a0886dd635e8cfbcb96d8670c2531176d248206decd248c00961eb0/detection) (0/72 detections)

**Issue: OCR Not Working**
- **Cause:** Windows OCR language pack not installed
- **Solution:** Settings → Language → Add Language → English (United States) → Install OCR
- **Documentation:** [User Guide - Prerequisites](user-guide.md#prerequisites)

**Issue: High CPU Usage**
- **Cause:** Capture interval too aggressive or frame differencing disabled
- **Solution:** Increase `capture.interval_ms` to 3000-5000ms and enable `enable_frame_diff = true`
- **Documentation:** [Performance - Optimization Tips](performance-optimizations.md#optimization-tips)

**Issue: API Connection Refused**
- **Cause:** Server not running or port conflict
- **Solution:** Check `cargo run --release` output for startup logs, verify port 3131 is available
- **Documentation:** [Developer Guide - Running the Server](developer-guide.md#running-the-server)

**Issue: Vector Search Not Working**
- **Cause:** Embedding model not downloaded or ONNX Runtime missing
- **Solution:** Model downloads automatically on first use. Check logs for download progress.
- **Documentation:** [Architecture - Embedding Engine](architecture.md#embedding-engine)

### Getting Help

- **GitHub Issues:** [Report bugs and request features](https://github.com/nicolasestrem/screensearch/issues)
- **Discussions:** [Ask questions and share ideas](https://github.com/nicolasestrem/screensearch/discussions)
- **Documentation:** Search this documentation hub for answers
- **Logs:** Run with `RUST_LOG=debug cargo run` for detailed diagnostics

---

## Project Information

**Version:** 0.2.0
**Platform:** Windows 10/11 only (uses Windows-specific APIs)
**License:** MIT
**Repository:** [github.com/nicolasestrem/screensearch](https://github.com/nicolasestrem/screensearch)
**Author:** Nicolas Estrem

**Technology Stack:**
- **Language:** Rust 2021 Edition
- **Runtime:** Tokio async runtime
- **Database:** SQLite with FTS5 full-text search
- **API Framework:** Axum
- **UI:** React + TypeScript + Vite
- **OCR:** Windows OCR API (WinRT COM)
- **Automation:** Windows UIAutomation API
- **Embeddings:** ONNX Runtime + all-MiniLM-L6-v2 model

**Performance Targets:**

| Metric | Target | Current Status |
|--------|--------|----------------|
| OCR Processing | < 100ms | 70-80ms ✓ |
| API Response | < 100ms | ~50ms ✓ |
| Vector Search | < 200ms | 150ms ✓ |
| CPU (Idle) | < 5% | ~2% ✓ |
| Memory | < 500MB | ~240MB ✓ |

**Test Coverage:** 59/59 tests passing (100% coverage)

---

## Contributing

We welcome contributions! Here's how to get started:

1. **Read the Developer Guide** → [Developer Guide](developer-guide.md)
2. **Understand the Architecture** → [Architecture Overview](architecture.md)
3. **Set Up Your Environment** → [Developer Guide - Setup](developer-guide.md#development-setup)
4. **Run Tests** → `cargo test --workspace`
5. **Create Feature Branch** → `git checkout -b feature/amazing-feature`
6. **Submit Pull Request** → Against `main` branch

**Contribution Areas:**
- Feature development (see [2DO.md](2DO.md) for planned features)
- Bug fixes and performance improvements
- Documentation improvements
- Test coverage expansion
- UI/UX enhancements

**Code Standards:**
- Run `cargo fmt --all` before committing
- Run `cargo clippy --workspace -- -D warnings` to catch issues
- Add tests for new functionality
- Update documentation for API changes

---

## Documentation Updates

This documentation hub is actively maintained. If you find errors or outdated information:

1. **Create an Issue:** [Documentation Issue Template](https://github.com/nicolasestrem/screensearch/issues/new?labels=documentation)
2. **Submit a PR:** Fix directly and submit a pull request
3. **Discuss:** Start a discussion for major documentation changes

**Recent Updates:**
- 2025-12-13: Added comprehensive documentation index
- 2025-12-13: Updated for v0.2.0 with hybrid search and embeddings
- 2025-12-10: Added AI intelligence documentation
- 2025-12-08: Added storage optimization documentation

---

**Last Updated:** 2025-12-13
**Documentation Version:** 2.0
**Covers:** ScreenSearch v0.2.0
