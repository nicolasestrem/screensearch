# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Fixed
- The quality sidecar no longer freezes while OCR runs. The `/v1/ocr` route was
  declared `async` but called the blocking, CPU-bound PaddleOCR predict directly
  on the asyncio event loop, stalling every other route (`/health`,
  `/v1/models/status`) for the entire OCR duration. With OCR taking tens of
  seconds, this left the Settings → Data & AI panel stuck on its loading
  skeleton because `/api/embeddings/status` (which proxies to
  `/v1/models/status`) was queued behind the in-flight OCR. Model loading and
  inference now run via `run_in_threadpool`, so the sidecar stays responsive
  (verified: status returned HTTP 200 at ~234 ms while a multi-second OCR was in
  flight).
- `EmbeddingsStatus` now shows a retryable error state instead of an indefinite
  blank when `/api/embeddings/status` fails or times out.

### Changed
- OCR frames are downscaled to a 2000 px longest side before being sent to the
  sidecar (`screensearch-capture/src/sidecar_ocr.rs`), with returned boxes mapped
  back to original-frame coordinates. On a 3440×1440 ultrawide frame this cut
  PP-OCRv5 inference time ~3.2× (88.7 s → 27.6 s in a warm contended benchmark)
  with no change to stored frame resolution.
- MKLDNN/oneDNN acceleration is explicitly kept disabled for PP-OCRv5: enabling
  it crashes detection under PaddleOCR 3.x's PIR executor
  (`ConvertPirAttribute2RuntimeAttribute not support`).

## [0.4.36] - 2026-06-15

### Changed
- Replaced legacy OCR and retrieval choices with the fixed local PP-OCRv5,
  Qwen3 embedding, Qwen3 reranking, sqlite-vec, and RRF quality stack.
- Added Linux-first local and release build scripts while retaining PowerShell
  helpers for Windows maintainers.
- Added packaged Windows sidecar validation that performs real PP-OCRv5
  inference before installer and portable artifacts are published.
- Split the release workflow's sidecar build into staged steps (pip upgrade,
  dependency install, PyInstaller bundle) so `--windows-bundle` watchers see
  progress instead of one ~7.5 min step that appears to hang, and added a
  heads-up message before `gh run watch`.

### Fixed
- The packaged Windows sidecar no longer crashes on launch with
  `OSError: [WinError 1114]` while importing torch. PyInstaller could bundle an
  older MSVC C runtime (`msvcp140`/`vcruntime140_1`) than PyTorch's `c10.dll`
  requires; because PyTorch's restricted DLL search prefers the bundle's
  `_internal` directory over System32, the stale runtime broke startup on every
  machine. `sidecar/build.py` now refreshes the bundled MSVC runtime from the
  host's System32 (the same version vc_redist installs) after PyInstaller runs.
- Frame embedding chunks are now replaced atomically, preventing partially
  indexed frames after an insertion failure.
- Migration 010 removes legacy duplicate chunks and enforces one embedding per
  `(frame_id, chunk_index)`.
- Time-filtered vector search expands its global candidate window and falls
  back to the full sqlite-vec index when required.
- Embedding responses validate model identity, revision, dimension, vector
  count, and per-vector dimensions.
- Sidecar model loaders are serialized during first initialization.
- PP-OCR uploads use JPEG transport and reject encoded images over 20 MiB or
  decoded images over 50 million pixels.
- PaddleOCR result cardinality mismatches are logged instead of silently
  truncated.

### Migration Notice
- Migration 009 intentionally deletes all legacy 384-dimensional embeddings
  because they are incompatible with the fixed 1024-dimensional Qwen3
  contract. Existing OCR data is retained and must be reindexed.

---

## [0.5.0] - 2026-06-10

### Changed
- **Brutalist / Minimalist Paper UI Redesign**: Overhauled the entire React frontend to match the print-media brutalist design of `screensearch-website`.
  - Replaced glassmorphism visual layout, translucent backdrops, radial gradients, glowing accents, and rounded corners with a flat, sharp, 90-degree corner design.
  - Implemented dynamic light and dark theme modes using custom HSL theme tokens (`bg-paper`, `text-ink`, `border-rule`).
  - Adopted newspaper-style typography: **Newsreader** (Serif) for headers and inputs, **Geist** (Sans) for UI copy, and **Geist Mono** (Monospace) for data metrics.
  - Redesigned `Sidebar.tsx`, `Timeline.tsx`, `FrameCard.tsx`, `SearchBar.tsx`, `Dashboard.tsx`, `Intelligence.tsx`, `AiSettings.tsx`, `SettingsPanel.tsx`, and `GlassCard.tsx` base components to utilize flat paper backgrounds, solid rule borders, and physical hover transforms instead of glows.

### Added
- **Dynamic Dark Mode**: Built support directly into the HSL variables in `index.css` via the `.dark` class, toggled by the existing state mechanism in `useStore.ts` and `App.tsx`.

---

## [0.4.32] - 2026-01-10

### Fixed
- **Installer Launch Failure (Error 740)** - Resolved "CreateProcess failed; code 740. The requested operation requires elevation" error that prevented app from launching after installation
  - Changed installation directory from `C:\Program Files\` to `%LOCALAPPDATA%\ScreenSearch\`
  - Removed admin privilege requirement - installer now runs without UAC prompt
  - Added `shellexec` flag to post-install launch for proper process handling
  - VC++ Redistributable installation now uses `/passive` mode with UAC prompt only when needed

### Technical
- Modified `installer/screensearch.iss`:
  - `DefaultDirName` changed from `{autopf}` to `{localappdata}`
  - `PrivilegesRequired` changed from `admin` to `lowest`
  - Post-install launch uses `shellexec` flag
  - VC++ install uses `shellexec` for UAC elevation

---

## [0.4.31] - 2026-01-10

### Fixed
- **Installer now bundles Visual C++ 2015-2022 Redistributable** - Fixes "VCRUNTIME140.dll not found" error on fresh Windows 11 installations when using the embedded LLM (llama-server.exe)
  - Installer automatically installs VC++ Runtime if not present (adds ~25MB to installer size)
  - Detection logic skips installation if VC++ Runtime already installed
- **Runtime DLL error detection** - Added user-friendly error messages for portable users when VC++ Runtime is missing
  - Detects Windows error codes 126 and 0xc0000135 (DLL_NOT_FOUND)
  - Provides direct download link: https://aka.ms/vs/17/release/vc_redist.x64.exe
  - Only affects llama-server.exe component; main app has no dependencies

### Technical
- Modified `installer/screensearch.iss` with `VCRedistNeedsInstall` registry check
- Updated `.github/workflows/release.yml` to download vc_redist.x64.exe during CI build
- Added `LlmError::MissingDependency` variant in `screensearch-llm/src/error.rs`
- Enhanced `screensearch-llm/src/server.rs` spawn logic with Windows-specific DLL detection

---

## [0.4.3] - 2026-01-09

### Fixed
- **Log File Permissions on Windows**: Fixed startup failure when running without admin privileges
  - Logs now written to `%LOCALAPPDATA%\ScreenSearch\logs\` in production builds
  - Previously failed with "Access is denied (os error 5)" when installed to `C:\Program Files\`
  - Development builds continue using relative path from `config.toml`
  - Follows same pattern as database and captures which already use LocalAppData

---

## [0.4.2] - 2026-01-08

### Fixed
- **Local LLM Auto-Start**: Fixed issue where llama-server would not auto-start when generating AI reports with the local provider
  - `generate_report()` now calls `ensure_started()` on the llama-server before making HTTP requests
  - Added prerequisite checks for model and server binary availability with clear error messages
  - Server now properly auto-starts on first AI request as designed (Lazy Loading)
  - Uses dynamic port from running server (handles fallback ports 31131, 31132 if default 31130 is busy)
  - Resolves "connection refused" errors on fresh install when user hadn't manually started server from Settings

- **GPU/Vulkan Fallback (VirtualBox Compatibility)**: Automatic fallback from GPU to CPU mode
  - First attempts GPU mode (Vulkan) with 45s timeout
  - If GPU fails (common in VirtualBox or systems without Vulkan), automatically retries in CPU-only mode
  - CPU mode uses 120s timeout to allow for slower model loading
  - Clear logging of which mode is being used

- **Health Check Robustness**: Improved llama-server startup reliability
  - Added INFO-level logging to track model loading progress during startup
  - Proper handling of HTTP 503 status (model still loading) vs connection errors
  - Clear error logging when server process exits unexpectedly or health check times out

- **TypeScript Build Error**: Fixed TS2532 "Object is possibly 'undefined'" in ReportGenerator.tsx
  - Added nullish coalescing operator for ISO date string parsing

- **CI Cross-Compilation**: Fixed `cargo-xwin` installation failure due to yanked `xwin` dependencies
  - Patched `cargo-xwin` v0.20.2 to use non-yanked `xwin = "0.6.8"` instead of yanked `^0.6.6`/`^0.6.7`
  - Clone from git, patch Cargo.toml, and install from local path to avoid dependency resolution failures
  - Maintains reproducible builds using pinned commit 074ac4d (2026-01-08)

### Technical Details
- Modified `screensearch-api/src/handlers/ai.rs`: Added auto-start logic to `generate_report()` function
- Modified `screensearch-llm/src/server.rs`: Refactored `start()` with GPU fallback and `wait_for_health_with_timeout()`
- Modified `screensearch-ui/src/components/ReportGenerator.tsx`: Fixed TypeScript strict null check
- Error messages now direct users to Settings → AI for missing dependencies
- Aligns with documented behavior in `docs/embedded-llm.md`: "Lazy Loading: Server starts only when first AI request is made"

---

## [0.4.1] - 2026-01-02

### Added
- **Virtual Scrolling**: Implemented react-window `FixedSizeGrid` for efficient rendering of large frame collections
  - New `VirtualFrameGrid` component with memoized cell renderer
  - `AutoSizeVirtualFrameGrid` wrapper for responsive layouts
  - Only renders visible items + 2 row overscan for smooth scrolling
  - 80% memory reduction for large frame collections (1000+ frames)

- **Performance Benchmarks**: Added Criterion.rs benchmark suite for database operations
  - `bench_get_frame`: Frame retrieval by ID
  - `bench_frame_range_query`: Range queries with varying dataset sizes
  - `bench_fts5_search`: Full-text search (single/multi-word)
  - `bench_frame_insertion`: Write performance testing
  - `bench_cosine_similarity`: Vector search simulation (384-dim)
  - `bench_statistics`: Statistics collection benchmark
  - Run with: `cargo bench -p screensearch-db`

- **Database Performance Indexes**: Migration 008 adds optimized indexes
  - `idx_frames_device_monitor_time`: Multi-column index for device/monitor queries
  - `idx_metadata_key`: Fast metadata lookups
  - `idx_analysis_queue_locked_priority`: Queue optimization
  - `idx_embeddings_frame_created`: Embedding lookup acceleration

- **Time-Filtered Embeddings Search**: New `search_embeddings_with_time_range()` method
  - Prevents loading all embeddings into memory
  - Bounded memory usage for large embedding sets
  - Optional start/end time parameters

### Changed
- **Database Connection Pool**: Reduced from 50 to 10 max connections
  - SQLite single-writer limitation makes 50 connections excessive
  - Lower memory usage and reduced contention
  - Optimal: 1 writer + 9 readers for concurrent access

- **API Logging**: Console logging now dev-only (`import.meta.env.DEV` check)
  - Eliminates 5-10ms overhead per request in production
  - Request/response logging still available in development

- **Bundle Optimization**: Vite manual chunk splitting for better caching
  - `vendor-react`: react, react-dom (141KB)
  - `vendor-query`: @tanstack/react-query, axios (76KB)
  - `vendor-ui`: framer-motion, lucide-react (114KB)
  - `vendor-markdown`: react-markdown (118KB)
  - Intelligence page lazy-loaded (10KB)

- **Code Splitting**: React.lazy() + Suspense for route-based splitting
  - IntelligencePage loaded on-demand
  - PageLoader fallback during chunk loading
  - 30-40% initial bundle reduction

### Fixed
- **Blob URL Memory Leak**: Implemented proper cleanup with `URL.revokeObjectURL()`
  - `blobUrlCache` Map tracks frame ID → blob URL mappings
  - `revokeBlobUrl()` function for manual cleanup
  - `revokeAllBlobUrls()` for bulk cleanup
  - useFrames hook auto-revokes on unmount

- **FrameCard Re-renders**: Wrapped in `React.memo()` with custom comparator
  - Only re-renders when frame data actually changes
  - Compares frame.id, timestamp, tags.length, and searchQuery
  - 20-30% faster grid rendering

- **Timeline Computation**: Wrapped `framesByDate` in `useMemo()`
  - O(1) for cached data instead of O(n) on every render
  - Significant improvement for large frame lists

- **RAG Context Building**: Early limit in first loop iteration
  - Prevents collecting all chunks then discarding most
  - Minor allocation reduction

- **React Error #310**: Fixed "Rendered fewer hooks than expected" in Timeline component
  - Root cause: `useMemo` called AFTER early returns for loading/error states
  - Solution: Moved `useMemo` before conditional rendering, use JSX ternary instead of early returns
  - All hooks now called unconditionally on every render per React Rules of Hooks
  - Prevents reconciliation errors during state transitions (loading → data)

### Technical Details
- Frontend bundle: 670KB total (properly chunked)
- All 14 database tests passing
- Rust workspace compiles cleanly
- react-window v1.8.10 with @types/react-window v1.8.8
- Criterion v0.5 with html_reports and async_tokio features

### Dependencies Added
- `react-window: ^1.8.10` - Virtual scrolling
- `@types/react-window: ^1.8.8` - TypeScript types
- `criterion: 0.5` (dev) - Rust benchmarks

---

## [0.4.0] - 2025-12-28

### Added
- **Embedded Ministral-3B LLM**: Local AI inference with no external API required
  - New `screensearch-llm` crate for managing llama.cpp server lifecycle
  - Auto-downloads llama-server binary and Ministral-3B-Instruct GGUF model on first use
  - GPU acceleration via Vulkan (works on NVIDIA, AMD, Intel) with CPU fallback
  - Model files stored in `%APPDATA%\ScreenSearch\models\`
  - Server runs on `http://127.0.0.1:31130` with OpenAI-compatible API

- **Local Provider UI Integration**:
  - "Local (Ministral-3B)" as default AI provider option in Settings
  - Model download progress indicator with real-time status
  - llama-server start/stop controls with health monitoring
  - Provider dropdown replacing free-form URL input
  - Automatic hiding of API key/model fields when local provider selected

- **Vision System Enhancements**:
  - Local provider handling in vision config test endpoint
  - Health check for llama-server at `http://127.0.0.1:31130/health`
  - Proper error messages for model download and server states

### Changed
- **Default AI Provider**: Changed from Ollama to local embedded LLM
- **Default Vision Settings**: Fresh installs now default to:
  - `vision_enabled = 0` (opt-in, not auto-start)
  - `vision_provider = 'local'`
  - `vision_endpoint = 'http://127.0.0.1:31130'`
- **llama-server Binary**: Switched from CPU-only to Vulkan GPU build for cross-platform acceleration
- **Database Migration**: Added migration to reset existing databases to local provider defaults

### Fixed
- **GitHub Actions Workflow**: Fixed cross-compile workflow failure caused by incorrect llvm-mingw download URL
  - Updated version format from `2024-11-01` to `20251216`
  - Fixed archive name from Windows binary (`x86_64-windows-gnu`) to Linux binary (`ucrt-ubuntu-22.04-x86_64`)
  - Removed non-existent packages (`llvm-tools-19`, `llvm-mingw`) from apt-get

### Technical Details
- llama-server manages Ministral-3B-Instruct-Q4_K_M.gguf (1.9GB model)
- GPU offloading with `-ngl 99` flag for maximum acceleration
- Vulkan binary auto-falls back to CPU if no GPU available
- DLL extraction from ZIP archives for Windows compatibility
- Server lifecycle managed via child process with graceful shutdown

---

## [0.3.0] - 2025-12-27

### Added
- **AI-First Dashboard**: New "Intel Dash" homepage with glassmorphism design aesthetic
  - **Daily Digest**: Auto-generated AI summaries of daily activity on page load
    - Session storage caching to avoid redundant API calls
    - Markdown rendering with custom bullet styling
    - Refresh button for on-demand regeneration
  - **Memory Status Gauge**: Circular radial gauge showing RAG indexing progress
    - Real-time embedding coverage percentage
    - Visual indicator for semantic search readiness
  - **Productivity Pulse**: Custom SVG line/area chart displaying hourly activity
    - Smooth cubic bezier curves
    - Interactive tooltip on hover
    - Gradient fill with glow effects
  - **Coming Soon Placeholders**: Knowledge Graph and Analytics previews

- **Smart Answer Card**: Enhanced AI-powered search results
  - Generates context-aware answers from screen history
  - Related Activity section showing app breakdown
  - Collapsible activity list with app icons

- **Glassmorphism Design System**: Premium UI components matching screensearch.app
  - `GlassCard` component with backdrop blur and translucent backgrounds
  - `CircularGauge` component with animated SVG progress
  - `ComingSoonCard` placeholder component with pulsing border
  - CSS custom properties for consistent theming (`--glass-bg`, `--glass-border`, `--glass-glow`)
  - Utility classes: `.glass-panel`, `.glass-card`, `.glow-blue`, `.gradient-text`
  - Animations: `fade-in-up`, `pulse-glow`, `border-glow`

- **Activity List Component**: App-specific breakdown of screen captures
  - Icon mapping for common applications (VS Code, Chrome, Slack, etc.)
  - Relative timestamps using date-fns
  - Frame count per application

### Changed
- **Primary Color**: Switched from violet to blue (#2563eb) to match screensearch.app branding
- **Default Page**: Dashboard is now the default landing page (previously Timeline)
- **Navigation**: Redesigned sidebar with AI Features section and Coming Soon badges
- **State Management**: Updated Zustand store with new `activeTab` options: `dashboard`, `timeline`, `reports`
- **Tailwind Config**: Extended theme with glass colors, primary light/dark variants, and new keyframe animations

### Technical Details
- Production bundle: 471.08 kB JS, 62.57 kB CSS
- Custom SVG charts avoid external charting library dependencies
- HSL color system enables consistent dark/light theme support
- Session storage caching reduces API calls for Daily Digest
- All new components use TypeScript with proper type safety

## [0.2.1] - 2025-12-14

### Added
- **Generic AI Provider Support**: Full compatibility with OpenAI-compatible APIs (e.g., LM Studio, vLLM, Ollama generic) via user settings.
- **AI Connectivity Test**: "Test Connection" button in settings to verify AI provider configuration immediately.
- **RAG Generation Endpoint**: New `/api/generate` endpoint for creating structured answers from search results.
- **Vision Client**: Dedicated `screensearch-vision` crate for managing AI interactions (Models, Prompts, Client).

### Changed
- **Settings UI**:
    - Added explicit "Save Configuration" button (removed auto-save on blur).
    - Added "Provider Protocol" selector (Ollama / OpenAI-compatible).
    - Added "Vision Model" configuration.
- **Cleaned UI**: Removed unused "Activity Graph" styling elements for a cleaner dashboard look.
- **Database Schema**: Added `vision_api_key` to settings for authenticated providers.

### Fixed
- **Settings Persistence**: Resolved issue where Vision AI settings (Provider, Model, Endpoint) were reset on save.
- **Ollama Defaulting**: Fixed backend hardcoding that forced Ollama usage even when other providers were selected.

## [0.2.0] - 2025-12-13

### Added
- **Timeline Visualization**: New "Activity Graph" component showing daily screen activity density in 10-minute buckets.
- **System Tray Integration**: Full system tray support with "Open" and "Quit" menu interactions.
- **Branding**: Complete "ScreenSearch" rebranding with premium "Tech-Panel" UI aesthetic matching `screensearch.app`.
- **Footer**: Added professional footer with author credits and repository links.
- **Icons**: New application icon (Blue/Cyan Activity Pulse) replacing placeholders.

### Changed
- **UI Overhaul**: Redesigned `App.tsx` layout with background grids, sidebar navigation, and glassmorphism effects.
- **Event Loop**: Refactored Winit event loop in `main.rs` for stable background operation and clean shutdown.
- **Performance**: Improved timeline data fetching with `useDailyActivity` hook for full-day statistics.

### Fixed
- **System Tray Infinite Loop**: Fixed critical bug where the browser would open endlessly on mouse hover events.
- **Search Reliability**: Hardened OCR text processing to prevent React rendering crashes on complex objects.
- **Build System**: Fixed Rust compilation errors related to accidental code truncation in `main.rs`.

## [0.1.4] - 2025-12-12

### Added
- **Retrieval Augmented Generation (RAG)**: Full support for RAG-based AI reports.
    - **In-Memory Vector Search**: Implemented high-performance in-memory semantic search (BGE-M3/MiniLM-L12 compatible) to bypass `sqlite-vec` limitations on Windows.
    - **Hybrid Search**: Combines Dense Retrieval (Embeddings) with Sparse Retrieval (FTS5) for robust context lookup.
    - **Reranker**: Added heuristic reranker boosting newer results and keyword matches.
- **Context Source Indicator**: Reports now include a footer (e.g., `*Context: Semantic Search (20 results)*`) indicating if the Vector Database or Traditional Fallback was used.
- **Database Schema**: Added `embedding` BLOB column to `embeddings` table (Migration 004).

### Changed
- **Dependency Optimization**: Downgraded `ort` (ONNX Runtime) to `2.0.0-rc.0` to match system-provided `1.17.1` DLLs, ensuring stability without external downloads.
- **API Response**: `AiReportResponse` now includes a `context_source` field.

### Fixed
- **Embedding Storage**: Fixed critical bug where embeddings were not being persisted (inserting 0 bytes), now correctly serializing `Vec<f32>` to BLOB.

## [0.1.3] - 2025-12-12

### Added
- **Storage Optimization**: Implemented JPEG compression for captured frames (default quality 80) to significantly reduce storage usage.
- **Image Resizing**: Added automatic resizing of captured frames to a maximum width (default 1920px) to further reduce file size.
- **Automatic Cleanup**: Implemented a background task that runs every 24 hours to enforce the data retention policy (deletes old frames based on `retention_days` setting).
- **Configuration**: Added `[storage]` section to `config.toml` for customizing format, quality, and max width.

### Changed
- **Default Image Format**: Changed default capture format from PNG to JPEG.

## [0.1.2] - 2025-12-11

### Added
- **Embedded UI Assets**: UI files are now embedded directly into the release binary using `rust-embed`, making the binary fully portable and self-contained
  - Binary can run from any directory without requiring `screen-ui/dist/` to exist
  - Assets served from memory for faster performance
  - Simpler deployment - just ship the binary
  - Binary size remains ~11MB (efficient compression)

### Fixed
- **JSX Structure in AI Settings**: Fixed orphaned form fields (API Key, Model Name, Test Button) in AiSettings component by correcting premature div closure (screen-ui/src/components/AiSettings.tsx:58)
- **Build Script Silent Failures**: npm install/build failures now properly fail the cargo build with clear error messages instead of silently continuing, preventing shipment of binaries without UI
- **npm Command Detection**: Improved Windows compatibility by using `npm.cmd` and proper `Command::current_dir()` instead of shell command strings

### Security
- **CORS Configuration**: Fixed invalid CORS setup that caused runtime panics - now properly uses explicit allow-lists for HTTP methods and headers when credentials are enabled (per CORS specification)
- **Information Disclosure in AI Errors**: Improved error message handling in AI endpoints - now provides error type information (HTTP status codes, error categories) for debugging while sanitizing sensitive response data from clients
- **URL Validation**: Added provider URL validation before making HTTP requests to AI endpoints - validates format and logs warnings for non-localhost URLs

### Changed
- **Browser Auto-Launch**: Made browser auto-launch configurable via `auto_open_browser` setting in config.toml (defaults to `true` for backward compatibility)
  - Set to `false` for headless servers, Docker containers, or background services

### Improvements
- **Code Documentation**: Added detailed comments explaining ServeDir SPA routing pattern in routes.rs
- **Code Clarity**: Documented magic number (1024*1024 = 1MiB) in request body limit configuration
- **Code Quality**: Extracted duplicated API key header logic to reusable `add_auth_header()` helper function
- **Developer Experience**: Added `SKIP_UI_BUILD` environment variable to allow skipping UI build during development for faster iteration (usage: `SKIP_UI_BUILD=1 cargo build`)

### Breaking Changes
- **Health Endpoint Route Change**: The health check endpoint has been moved from `/health` to `/api/health` to maintain consistency with other API routes
  - Update any monitoring systems, health check configurations, or client code that references the old `/health` endpoint
  - The endpoint functionality remains the same, only the path has changed

### Added
- **Search Autocomplete**: Intelligent search suggestions with keyboard navigation (↑↓ arrows, Enter, Escape)
  - Debounced API calls (300ms) for optimal performance
  - Text highlighting in suggestions
  - Visual hover and selected states
  - Click-outside to close dropdown
  - ARIA accessibility attributes
- **Timeline Filters**: Advanced filtering system for frames
  - Date range filter (start/end dates)
  - Application name filter
  - Tag-based filtering with multi-select support
  - URL bookmarkability - all filters reflected in query parameters
  - "Clear all filters" functionality
- **Complete Tag Management**: Full CRUD operations for tags
  - Create tags with custom names and colors
  - Edit existing tags (name and color)
  - Delete tags with confirmation dialog
  - Assign/remove tags to/from frames via frame modal
  - Tag picker dropdown showing unassigned tags
  - Hover-to-delete on assigned tags
  - Optimistic UI updates and toast notifications
- **Tag Filtering Backend**: Server-side support for filtering frames by multiple tags
  - Comma-separated tag IDs in API query parameters
  - Proper SQL joins for tag-based frame retrieval
  - Integration with existing date and app filters

### Improvements
- **Accessibility**: Added ARIA labels to icon-only buttons (theme toggle, settings, tag menu)
- **Code Organization**: Extracted OCR text handling logic to dedicated `lib/ocrUtils.ts` utility file with proper documentation
- **Performance Optimizations**:
  - Memoized OCR text extraction in FrameCard to prevent unnecessary re-computation
  - Memoized search highlighting to avoid re-processing on every render
  - Added cancel method to debounce utility for proper cleanup

### Performance
- **OCR Pipeline Optimization**: Eliminated 60-93ms bottleneck by implementing direct `SoftwareBitmap` creation from raw RGBA data
  - **Before**: 112-196ms per frame (PNG encode 40-60ms + PNG decode 15-25ms + OCR 50-100ms)
  - **After**: 55-105ms per frame (Direct bitmap 5ms + OCR 50-100ms)
  - **Improvement**: 53-73% faster, achieving <100ms target
  - Zero-copy memory transfer using unsafe Rust with safety guarantees
  - Removed `std::io::Cursor` and PNG-related dependencies from hot path
- **Tag Loading Optimization**: Implemented bulk tag loading to eliminate N+1 query problem
  - **Before**: 201 queries for 100 frames (1 frame query + 100 tag queries + 100 OCR queries)
  - **After**: 3 queries for 100 frames (1 frame query + 1 bulk tag query + 100 OCR queries)
  - **Improvement**: 15x faster tag loading, 100 frames now load in <200ms (down from ~1500ms)
  - New `get_tags_for_frames()` method uses single JOIN query with parameterized IN clause
  - Returns HashMap for O(1) tag lookup per frame
- **Tag Assignment Optimization**: Simplified `add_tag_to_frame` to rely on database FK constraints
  - **Before**: 3 queries (verify frame exists + verify tag exists + insert)
  - **After**: 1 query (insert with FK constraint error handling)
  - Gracefully handles duplicate assignments (idempotent behavior)
- **Memory Optimization**: Refactored frame differencing to use `Arc<RgbaImage>` reference counting, eliminating 8.2MB allocations per frame change (reduces memory pressure from 39GB/8hr to <1GB/8hr)
- **Query Sanitization**: Added FTS5 query input sanitization to prevent operator injection and handle special characters (e.g., `C++`, `$100`) correctly

### Fixed
- **Critical API Route Mismatch**: Fixed `addTagToFrame()` to send `tag_id` in request body instead of URL path (tag assignment was completely broken)
- **Search Autocomplete API**: Fixed parameter name from `q` to `keywords` to match backend expectation
- **OCR Text Extraction**: Updated API client to handle `OcrTextRecord[]` response format correctly
- **Filter Integration**: Connected all UI filters to backend API with proper query parameters
- **Tag API Response Format**: Fixed field mapping from `tag_name` (database) to `name` (API response)
- **Error Messages**: Enhanced error handling in FrameModal to extract and display specific validation errors from API responses
- **TypeScript Compilation**: Removed unused imports and variables causing build warnings
- **Null Safety**: Added optional chaining for `frame.tags?.length` to prevent undefined errors
- **FrameCard Rendering Error**: Fixed crash when `ocr_text` is returned as an object instead of string by adding robust type handling
- **Type Safety**: Replaced `any` types with proper TypeScript union types for OCR content (`OCRTextContent`, `OCRTextData`)
- **Click-Outside Handler**: Added missing click-outside handler for tag menu dropdown in FrameCard
- **Debounce Cleanup**: Added proper cleanup for debounced search queries to prevent memory leaks on component unmount

### Security
- **XSS Vulnerability**: Eliminated `dangerouslySetInnerHTML` usage in FrameCard and replaced with safe React element rendering for search highlighting
  - Text is now properly escaped and rendered as React elements instead of raw HTML
  - Search highlights use `<mark>` elements rendered safely through React
- **Hex Color Validation**: Added regex validation for tag colors (`#RRGGBB` or `#RRGGBBAA` format only)
- **Input Size Limits**: Enforced maximum lengths - tag_name (200 chars), description (1000 chars)
- **Request Body Limits**: Added 1MB max request size via `DefaultBodyLimit` middleware to prevent DoS attacks
- **Frontend Validation**: Added character counters and maxLength attributes to tag creation form

### Technical Details
- OCR processing time reduced from 112-196ms to 55-105ms (53-73% improvement)
- Tag loading optimized from 201 queries to 3 queries for 100 frames (15x faster)
- Direct `SoftwareBitmap::Create()` with `BitmapPixelFormat::Rgba8` eliminates intermediate format conversions
- Bulk tag loading uses dynamic SQL with parameterized IN clause and HashMap grouping
- Unsafe memory copy with buffer size validation and exclusive write lock ensures memory safety
- Frame differencing now uses 8-byte Arc pointer clones instead of full 8.2MB image clones
- Search queries now wrap user input in quotes with proper escaping to prevent FTS5 syntax errors
- System can now handle 1-second capture intervals (down from 3 seconds) while maintaining <5% CPU usage
- Tag filtering supports efficient SQL queries with proper JOIN operations on `frame_tags` table
- All filter operations update URL query parameters for shareable/bookmarkable states
- Foreign key constraint validation reduces redundant existence checks in `add_tag_to_frame`
- Error messages now include specific validation details from backend API responses

## [0.1.1] - 2025-12-10

### Added
- `GET /frames/:id` endpoint for retrieving individual frame details with OCR text and tags
- `GET /settings` endpoint for retrieving current capture configuration
- `POST /settings` endpoint for updating capture settings (interval, monitors, excluded apps, etc.)
- Settings panel in web UI with backend integration for:
  - Capture interval adjustment (2-30 seconds)
  - Monitor selection
  - Excluded applications management
  - Pause/resume capture control
  - Data retention settings

### Fixed
- "Frame not found" error when clicking frame cards in web interface
- Settings panel now properly loads and saves configuration via backend API
- Type mismatch between Rust snake_case field names and TypeScript camelCase expectations

### Changed
- Web interface status updated from "Broken" to fully functional
- Settings interface now uses snake_case field names to match backend API response format

## [0.1.0] - 2025-12-10

### Added
- Initial release with core functionality:
  - Continuous screen capture with frame differencing
  - OCR text extraction using Windows OCR API
  - SQLite database with FTS5 full-text search
  - REST API with 27 endpoints
  - UI automation via Windows accessibility APIs
  - React-based web interface
  - Privacy controls and application exclusions

[0.5.0]: https://github.com/nicolasestrem/screensearch/compare/v0.4.32...v0.5.0
[0.4.2]: https://github.com/nicolasestrem/screensearch/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/nicolasestrem/screensearch/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/nicolasestrem/screensearch/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/nicolasestrem/screensearch/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/nicolasestrem/screensearch/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/nicolasestrem/screensearch/releases/tag/v0.1.0
