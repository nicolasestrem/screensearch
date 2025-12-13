# Changelog

All notable changes to ScreenSearch will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

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
