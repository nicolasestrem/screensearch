# ScreenSearch Developer Guide

**Version:** 0.2.0
**Last Updated:** 2025-12-26

## Table of Contents

1. [Getting Started](#getting-started)
2. [Project Structure](#project-structure)
3. [Development Workflow](#development-workflow)
4. [Working with Each Crate](#working-with-each-crate)
5. [Adding New Features](#adding-new-features)
6. [Testing Strategy](#testing-strategy)
7. [Debugging & Profiling](#debugging--profiling)
8. [Release Process](#release-process)
9. [Contributing Guidelines](#contributing-guidelines)

---

## Getting Started

### Prerequisites

#### 1. Rust Toolchain

ScreenSearch requires Rust 1.70 or later with the 2021 edition.

```powershell
# Install rustup (Windows)
winget install Rustlang.Rustup

# Verify installation
rustc --version
cargo --version
```

Alternatively, download from [rustup.rs](https://rustup.rs/).

#### 2. Visual Studio Build Tools

Required for Windows API bindings and native dependencies.

```powershell
# Install via winget
winget install Microsoft.VisualStudio.2022.BuildTools

# Or download from:
# https://visualstudio.microsoft.com/downloads/
```

During installation, select:
- "Desktop development with C++"
- Windows 10/11 SDK
- MSVC v143 or later

Verify installation:
```powershell
cl.exe
```

#### 3. Windows OCR Language Pack

ScreenSearch uses the Windows OCR API which requires language packs.

1. Open Windows Settings
2. Navigate to **Time & Language > Language**
3. Add language: **English (United States)**
4. Click **Options** next to English
5. Download the language pack

### Cross-Compilation from Linux

**New in v0.2.0**: ScreenSearch can be cross-compiled from Linux to Windows using `cargo-xwin`, enabling CI/CD automation and Linux-based development workflows.

#### Prerequisites (Linux)

```bash
# Install system dependencies (Ubuntu/Debian)
sudo apt-get install -y clang-19 lld llvm-19

# For Arch-based systems
sudo pacman -S clang lld llvm

# Install cargo-xwin
cargo install cargo-xwin

# Add Windows target
rustup target add x86_64-pc-windows-msvc
```

**Important**: Clang 19.0.0 or newer is required for MSVC STL compatibility. If you have Clang 18, you must upgrade:

```bash
sudo apt-get install -y clang-19
sudo update-alternatives --install /usr/bin/clang clang /usr/bin/clang-19 100
sudo update-alternatives --install /usr/bin/clang++ clang++ /usr/bin/clang++-19 100
```

#### Build Commands

```bash
# Build UI first (runs on Linux, platform-agnostic)
cd screensearch-ui && npm install && npm run build && cd ..

# Cross-compile to Windows
cargo xwin build --release --target x86_64-pc-windows-msvc

# Output: target/x86_64-pc-windows-msvc/release/screensearch.exe
```

#### Testing

- **On Linux**: Compilation can be verified, but runtime testing is not possible
- **On Windows**: Transfer the `.exe` to a Windows machine for full validation
- See [docs/cross-compilation.md](cross-compilation.md) for comprehensive troubleshooting

### IDE Configuration

#### Visual Studio Code

Recommended extensions:
- **rust-analyzer** - Rust language server
- **CodeLLDB** - Debugging support
- **Better TOML** - TOML syntax highlighting
- **Error Lens** - Inline error display

Workspace settings (`.vscode/settings.json`):
```json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.cargo.buildScripts.enable": true,
  "editor.formatOnSave": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

Launch configuration (`.vscode/launch.json`):
```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug ScreenSearch",
      "cargo": {
        "args": ["build", "--bin=screen-memories", "--package=screen-memories"]
      },
      "args": [],
      "cwd": "${workspaceFolder}",
      "env": {
        "RUST_LOG": "debug"
      }
    }
  ]
}
```

#### IntelliJ IDEA / CLion

1. Install **Rust** plugin
2. Configure Rust toolchain: **Settings > Languages & Frameworks > Rust**
3. Enable **external linter (clippy)**: **Settings > Languages & Frameworks > Rust > Linters**
4. Set Cargo check on save

### Project Setup

```bash
# Clone repository
git clone https://github.com/nicolasestrem/screen-memories.git
cd screen-memories

# Build all workspace crates
cargo build

# Run tests to verify setup
cargo test

# Run the application
cargo run
```

For first-time setup, the database will be created automatically at `screen_memories.db`.

---

## Project Structure

ScreenSearch uses a Cargo workspace architecture with five crates plus a frontend application.

```
screen-memories/
├── Cargo.toml                 # Workspace manifest
├── config.toml                # Application configuration
                                  # [storage]: format="jpeg", quality=80, max_width=1920
├── src/
│   └── main.rs               # Main binary - integrates all crates
│
├── screen-capture/           # Screen capture and OCR (Crate 1)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs           # Public API exports
│   │   ├── capture.rs       # GraphicsCaptureApiHandler implementation
│   │   ├── ocr.rs           # Windows OCR API wrapper
│   │   ├── frame_diff.rs    # Frame differencing (Pixel/Histogram/SSIM)
│   │   ├── monitor.rs       # Multi-monitor enumeration
│   │   └── window_context.rs # Active window tracking
│   └── examples/
│       └── basic_capture.rs
│
├── screen-db/                # Database layer (Crate 2)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs           # Public API exports
│   │   ├── db.rs            # DatabaseManager with pooling
│   │   ├── migrations/      # SQL migration files
│   │   │   ├── 001_initial_schema.sql
│   │   │   ├── 002_fts5_search.sql
│   │   │   └── 003_tags.sql
│   │   ├── models.rs        # Data models (Frame, OcrText, Tag)
│   │   └── queries.rs       # Query implementations
│   └── tests/
│       ├── db_tests.rs
│       └── query_tests.rs
│
├── screen-api/               # REST API server (Crate 3)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs           # Library exports
│   │   ├── main.rs          # Binary entry point
│   │   ├── server.rs        # Axum server setup
│   │   ├── routes.rs        # Route definitions
│   │   ├── state.rs         # Application state (Arc<AppState>)
│   │   ├── error.rs         # Error types and HTTP conversion
│   │   ├── models.rs        # Request/response models
│   │   └── handlers/
│   │       ├── mod.rs       # Handler exports
│   │       ├── search.rs    # Search endpoints
│   │       ├── automation.rs # Automation endpoints
│   │       └── system.rs    # System management
│   └── tests/
│       └── integration_tests.rs
│
├── screen-automation/        # UI automation (Crate 4)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs           # Public API exports
│   │   ├── engine.rs        # AutomationEngine
│   │   ├── selector.rs      # Element selectors (Playwright-inspired)
│   │   ├── element.rs       # UIElement wrapper
│   │   ├── input.rs         # InputSimulator (mouse/keyboard)
│   │   └── window.rs        # WindowManager
│   ├── examples/
│   │   ├── notepad_automation.rs
│   │   └── element_search.rs
│   └── tests/
│       └── automation_tests.rs
│
├── screen-ui/                # React frontend
│   ├── package.json
│   ├── vite.config.ts
│   ├── src/
│   │   ├── main.tsx
│   │   ├── App.tsx
│   │   ├── api/
│   │   │   └── client.ts    # Axios API client
│   │   ├── components/
│   │   │   ├── Header.tsx
│   │   │   ├── SearchBar.tsx
│   │   │   ├── Timeline.tsx
│   │   │   ├── FrameCard.tsx
│   │   │   ├── FrameModal.tsx
│   │   │   ├── TagManager.tsx
│   │   │   └── SettingsPanel.tsx
│   │   ├── hooks/
│   │   │   ├── useSearch.ts
│   │   │   ├── useFrames.ts
│   │   │   └── useTags.ts
│   │   └── store/
│   │       └── useStore.ts  # Zustand state
│   └── public/
│
├── tests/
│   └── integration/         # Cross-crate integration tests
│       ├── test_end_to_end.rs
│       └── test_capture_to_db.rs
│
├── Inspiration/             # Reference implementations
│   ├── screenpipe-core/     # Use for capture and OCR patterns
│   ├── screenpipe-db/       # Use for database architecture
│   └── screenpipe-vision/   # Reference only
│
├── docs/
│   ├── developer-guide.md   # This file
│   ├── api-reference.md
│   └── architecture.md
│
├── DEVELOPMENT.md           # Legacy development guide
├── README.md                # User documentation
└── CLAUDE.md                # Claude AI context
```

### Workspace Crate Overview

| Crate | Purpose | Key Dependencies |
|-------|---------|------------------|
| **screen-capture** | Hardware-accelerated screen capture with Windows Graphics Capture API, multi-monitor support, frame differencing | `windows-capture`, `windows`, `image`, `crossbeam` |
| **screen-db** | SQLite database with WAL mode, FTS5 search, connection pooling, automatic migrations | `sqlx`, `chrono` |
| **screen-api** | REST API on localhost:3131 with Axum, CORS, embedded UI assets (rust-embed), search and automation endpoints | `axum`, `tower-http`, `serde_json`, `rust-embed` |
| **screen-automation** | Windows UIAutomation API wrapper with Playwright-inspired selectors | `uiautomation` |
| **screen-ui** | React 18 + TypeScript + Vite frontend with TanStack Query, Zustand, Tailwind CSS | Node.js ecosystem |

---

## Development Workflow

### Building

```bash
# Build all workspace crates (debug)
cargo build

# Build specific crate
cargo build -p screen-capture
cargo build -p screen-db
cargo build -p screen-api
cargo build -p screen-automation

# Build with release optimizations
cargo build --release

# Fast syntax/type checking without building
cargo check

# Check specific crate
cargo check -p screen-db
```

**Build profiles:**
- `dev` - Fast compilation, debug symbols, no optimization
- `release` - Full optimization, LTO enabled, stripped symbols
- `bench` - Inherits from release, keeps debug symbols

### Running

```bash
# Run main binary with default config
cargo run

# Run with custom configuration
cargo run -- --config custom.toml

# Run with debug logging
$env:RUST_LOG="debug"
cargo run

# Run specific crate binary (screen-api)
cargo run -p screen-api

# Run in release mode
cargo run --release

# Run with arguments
cargo run -- --database-path /path/to/db.sqlite
```

**Environment variables:**
- `RUST_LOG` - Logging level (`trace`, `debug`, `info`, `warn`, `error`)
- `SCREEN_DB_PATH` - Database file path (default: `screen_memories.db`)
- `SKIP_UI_BUILD` - Skip UI build during compilation (useful for backend-only development)

### Testing

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p screen-db

# Run specific test by name
cargo test test_frame_differencing

# Run tests with output (show println!)
cargo test -- --nocapture

# Run integration tests
cargo test --test integration

# Run tests in release mode (faster)
cargo test --release

# Run with specific log level
RUST_LOG=debug cargo test -- --nocapture
```

**Test organization:**
- Unit tests: In `#[cfg(test)]` modules within source files
- Integration tests: In `tests/` directory at crate root
- Workspace integration: In `tests/integration/` at workspace root

### Code Quality

```bash
# Format code with rustfmt
cargo fmt

# Check formatting without modifying
cargo fmt --check

# Run clippy lints
cargo clippy

# Clippy with all targets
cargo clippy --all-targets

# Fix clippy warnings automatically
cargo clippy --fix

# Strict clippy (treat warnings as errors)
cargo clippy -- -D warnings

# Generate documentation
cargo doc --no-deps --open

# Check documentation coverage
cargo doc --no-deps
```

### Continuous Integration

Recommended CI pipeline (GitHub Actions):

```yaml
name: CI
on: [push, pull_request]
jobs:
  test:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --all-targets
      - run: cargo test --all-targets
      - run: cargo clippy -- -D warnings
      - run: cargo fmt --check
```

---

## Working with Each Crate

### screen-capture

**Purpose:** Hardware-accelerated screen capture and Windows OCR integration.

**Key modules:**
- `capture.rs` - Implements `GraphicsCaptureApiHandler` trait
- `ocr.rs` - Windows OCR API wrapper using `Windows::Media::Ocr`
- `frame_diff.rs` - Three algorithms: Pixel, Histogram, SSIM
- `monitor.rs` - Multi-monitor enumeration via Windows GDI
- `window_context.rs` - Active window tracking and browser URL extraction

**Usage example:**

```rust
use screen_capture::{CaptureConfig, ScreenCapture};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = CaptureConfig {
        interval_ms: 3000,          // Capture every 3 seconds
        enable_frame_diff: true,    // Skip unchanged frames
        diff_threshold: 0.006,      // 0.6% change threshold
        max_frames_buffer: 30,      // Buffer up to 30 frames
        monitor_indices: vec![],    // All monitors (empty = all)
        ..Default::default()
    };

    let mut capture = ScreenCapture::new(config)?;

    // Start capture with callback
    capture.start(|frame| {
        println!(
            "Frame from monitor {}: {}x{}",
            frame.monitor_index,
            frame.image.width(),
            frame.image.height()
        );

        if let Some(window) = &frame.active_window {
            println!("Active: {} ({})", window.title, window.process_name);
        }

        Ok(())
    }).await?;

    Ok(())
}
```

**Performance characteristics:**
- CPU usage: < 5% idle (with frame differencing enabled)
- Memory: < 500MB for 30-frame buffer
- Frame diff: ~1ms per frame (histogram method)
- Capture latency: ~50ms per screen

**Adding a new frame diff algorithm:**

1. Add variant to `DiffMethod` enum in `frame_diff.rs`:
   ```rust
   pub enum DiffMethod {
       Pixel,
       Histogram,
       Ssim,
       YourNewMethod,
   }
   ```

2. Implement algorithm in `FrameDiffer::calculate_diff()`:
   ```rust
   DiffMethod::YourNewMethod => {
       // Your implementation
       let diff_score = calculate_your_method(img1, img2);
       diff_score
   }
   ```

3. Add tests in `#[cfg(test)]` module

### screen-db

**Purpose:** SQLite database with FTS5 search, WAL mode, connection pooling.

**Key modules:**
- `db.rs` - `DatabaseManager` with `SqlitePool`
- `migrations/` - SQL migration files (auto-applied on startup)
- `models.rs` - Data models (`Frame`, `OcrText`, `Tag`)
- `queries.rs` - Query implementations with sqlx macros

**Database schema highlights:**

Tables:
- `frames` - Screenshot metadata (timestamp, file_path, monitor, window)
- `ocr_text` - OCR results with coordinates and confidence
- `ocr_text_fts` - FTS5 virtual table for full-text search
- `tags` - User-defined tags
- `frame_tags` - Many-to-many relationship

Indexes:
- `idx_frames_timestamp` - Time-range queries
- `idx_ocr_frame_id` - Frame lookups
- `idx_ocr_timestamp` - Time-based OCR search

**Usage example:**

```rust
use screen_db::{DatabaseManager, NewFrame, FrameFilter, Pagination};
use chrono::Utc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize database (creates file if not exists)
    let db = DatabaseManager::new("screen_memories.db").await?;

    // Insert frame
    let frame = NewFrame {
        timestamp: Utc::now(),
        file_path: "/path/to/frame.png".to_string(),
        monitor_index: 0,
        active_window: Some("Chrome - Google".to_string()),
        active_app: Some("chrome.exe".to_string()),
        url: Some("https://google.com".to_string()),
    };
    let frame_id = db.insert_frame(frame).await?;

    // Full-text search
    let results = db.search_ocr_text(
        "important document",
        FrameFilter::default(),
        Pagination { limit: 100, offset: 0 }
    ).await?;

    for result in results {
        println!("Found in frame {}: {}", result.frame_id, result.text);
    }

    Ok(())
}
```

**Adding a new table:**

1. Create migration file `screen-db/src/migrations/004_your_table.sql`:
   ```sql
   CREATE TABLE your_table (
       id INTEGER PRIMARY KEY AUTOINCREMENT,
       name TEXT NOT NULL,
       created_at TEXT NOT NULL
   );

   CREATE INDEX idx_your_table_name ON your_table(name);
   ```

2. Add model in `models.rs`:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct YourRecord {
       pub id: i64,
       pub name: String,
       pub created_at: DateTime<Utc>,
   }
   ```

3. Add query methods in `queries.rs`:
   ```rust
   impl DatabaseManager {
       pub async fn insert_your_record(&self, name: String) -> Result<i64> {
           let now = Utc::now();
           let result = sqlx::query!(
               "INSERT INTO your_table (name, created_at) VALUES (?, ?)",
               name,
               now
           )
           .execute(&self.pool)
           .await?;

           Ok(result.last_insert_rowid())
       }
   }
   ```

### screen-api

**Purpose:** REST API server on `localhost:3131` with Axum framework.

**Key modules:**
- `server.rs` - Axum server initialization with middleware
- `routes.rs` - Route definitions using `Router::new()`
- `handlers/` - Request handlers organized by domain
- `state.rs` - `AppState` with `Arc<DatabaseManager>` and `Arc<AutomationEngine>`
- `error.rs` - `AppError` type with `IntoResponse` implementation

**API endpoint categories:**

1. **Context Retrieval**
   - `GET /search` - FTS5 full-text search
   - `GET /search/keywords` - Exact keyword matching
   - `GET /frames` - List frames with filters
   - `GET /health` - Health check with statistics

2. **Computer Automation**
   - `POST /automation/find-elements` - Locate UI elements
   - `POST /automation/click` - Mouse click
   - `POST /automation/type` - Keyboard input
   - `POST /automation/scroll` - Scroll action
   - `POST /automation/press-key` - Key press with modifiers

3. **System Management**
   - `GET /tags` - List tags
   - `POST /tags` - Create tag
   - `DELETE /tags/:id` - Delete tag
   - `POST /frames/:id/tags` - Tag a frame

**Usage example (adding an endpoint):**

1. Define request/response models in `models.rs`:
   ```rust
   #[derive(Deserialize)]
   pub struct MyRequest {
       pub query: String,
   }

   #[derive(Serialize)]
   pub struct MyResponse {
       pub result: String,
   }
   ```

2. Implement handler in `handlers/your_domain.rs`:
   ```rust
   use axum::{extract::State, Json};
   use std::sync::Arc;

   pub async fn my_handler(
       State(state): State<Arc<AppState>>,
       Json(payload): Json<MyRequest>,
   ) -> Result<Json<MyResponse>, AppError> {
       // Access database
       let result = state.db.some_query(&payload.query).await?;

       Ok(Json(MyResponse {
           result: result.to_string(),
       }))
   }
   ```

3. Register route in `routes.rs`:
   ```rust
   use crate::handlers::your_domain::my_handler;

   pub fn build_router(state: Arc<AppState>) -> Router {
       Router::new()
           .route("/my-endpoint", post(my_handler))
           // ... other routes
           .with_state(state)
   }
   ```

**Testing API endpoints:**

```bash
# Start server
cargo run -p screen-api

# Test with curl
curl http://localhost:3131/health

curl -X POST http://localhost:3131/search \
  -H "Content-Type: application/json" \
  -d '{"q": "document", "limit": 10}'
```

### screen-automation

**Purpose:** Windows UIAutomation API wrapper with Playwright-inspired selectors.

**Key modules:**
- `engine.rs` - `AutomationEngine` wraps `UIAutomation` instance
- `selector.rs` - `Selector` builder for finding elements
- `element.rs` - `UIElement` wrapper with actions (click, type, focus)
- `input.rs` - `InputSimulator` for low-level mouse/keyboard
- `window.rs` - `WindowManager` for window enumeration and focus

**Usage example:**

```rust
use screen_automation::{AutomationEngine, Selector};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = AutomationEngine::new()?;

    // Find element by role and name
    let button = engine
        .find_element(&Selector::role("button").with_name("Save"))
        .await?;
    button.click()?;

    // Type text into input field
    let input = engine
        .find_element(&Selector::role("edit").with_name("Search"))
        .await?;
    input.focus()?;
    input.type_text("Hello, World!")?;

    // List windows
    let windows = engine.windows().enumerate()?;
    for win in windows {
        println!("{} - {}", win.title, win.process_name);
    }

    // Direct input simulation
    let input_sim = engine.input();
    input_sim.click_at(500, 300, MouseButton::Left)?;
    input_sim.ctrl_key("c")?; // Ctrl+C

    Ok(())
}
```

**Selector types:**

```rust
// By role
Selector::role("button").with_name("OK")

// By automation ID
Selector::id("btnSubmit")

// By text content
Selector::text("Click Here")

// String parsing
Selector::from("#myButton")      // ID selector
Selector::from("text:Login")     // Text selector
Selector::from("button:Submit")  // Role with name
```

**Click strategies (automatic fallback):**

1. Direct UIAutomation invoke
2. Clickable point from element
3. Center of bounding rectangle

**Thread safety:**

All UIAutomation objects are wrapped in `Arc` and marked `Send + Sync`:

```rust
pub struct ThreadSafeAutomation(pub Arc<UIAutomation>);
unsafe impl Send for ThreadSafeAutomation {}
unsafe impl Sync for ThreadSafeAutomation {}
```

### screen-ui (Frontend)

**Purpose:** Modern React web interface for searching and browsing captures.

**Tech stack:**
- React 18 with TypeScript
- Vite (build tool)
- TanStack Query (data fetching)
- Zustand (state management)
- Tailwind CSS (styling)
- Axios (HTTP client)

**Project structure:**

```
src/
├── api/client.ts          # API client with axios
├── components/            # React components
├── hooks/                 # React Query hooks
├── store/useStore.ts      # Zustand store
├── types/index.ts         # TypeScript types
└── App.tsx                # Main app
```

**Development workflow:**

```bash
cd screen-ui

# Install dependencies
npm install

# Start dev server (http://localhost:5173)
npm run dev

# Build for production
npm run build

# Preview production build
npm run preview

# Type checking
npm run type-check

# Linting
npm run lint
```

**Production deployment:**

The UI is embedded in the binary at compile time using `rust-embed`:

```rust
// screen-api/src/embedded.rs
#[derive(RustEmbed)]
#[folder = "../screen-ui/dist/"]
pub struct Assets;
```

When you build the release binary, the UI files are automatically:
1. Built by `build.rs` (runs `npm install` && `npm run build`)
2. Embedded into the binary by `rust-embed` (reads from `screen-ui/dist/`)
3. Served from memory at runtime (no filesystem access needed)

This makes the binary **fully portable** - it can run from any directory without needing the `screen-ui/dist/` folder at runtime.

**Vite proxy configuration:**

The dev server proxies API requests to avoid CORS:

```typescript
// vite.config.ts
export default defineConfig({
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:3131',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
});
```

**Adding a new component:**

1. Create component file `src/components/MyComponent.tsx`:
   ```typescript
   import React from 'react';

   interface MyComponentProps {
     title: string;
   }

   export const MyComponent: React.FC<MyComponentProps> = ({ title }) => {
     return (
       <div className="p-4 bg-white rounded shadow">
         <h2 className="text-xl font-bold">{title}</h2>
       </div>
     );
   };
   ```

2. Create hook for data fetching `src/hooks/useMyData.ts`:
   ```typescript
   import { useQuery } from '@tanstack/react-query';
   import { apiClient } from '../api/client';

   export const useMyData = () => {
     return useQuery({
       queryKey: ['myData'],
       queryFn: async () => {
         const { data } = await apiClient.get('/my-endpoint');
         return data;
       },
       staleTime: 30000, // 30 seconds
     });
   };
   ```

3. Use in app:
   ```typescript
   import { MyComponent } from './components/MyComponent';
   import { useMyData } from './hooks/useMyData';

   function App() {
     const { data, isLoading } = useMyData();

     if (isLoading) return <div>Loading...</div>;

     return <MyComponent title={data.title} />;
   }
   ```

---

## Adding New Features

### Example 1: Add OCR Confidence Filtering

**Requirement:** Allow filtering search results by OCR confidence score.

**Step 1: Update database schema**

Create `screen-db/src/migrations/005_ocr_confidence_index.sql`:
```sql
-- Add index for confidence filtering
CREATE INDEX IF NOT EXISTS idx_ocr_confidence ON ocr_text(confidence);
```

**Step 2: Update models**

In `screen-db/src/models.rs`:
```rust
#[derive(Debug, Clone)]
pub struct FrameFilter {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub app_name: Option<String>,
    pub min_confidence: Option<f32>, // NEW FIELD
}
```

**Step 3: Update query**

In `screen-db/src/queries.rs`:
```rust
impl DatabaseManager {
    pub async fn search_ocr_text(
        &self,
        query: &str,
        filter: FrameFilter,
        pagination: Pagination,
    ) -> Result<Vec<SearchResult>> {
        let mut sql = String::from(
            "SELECT ... FROM ocr_text_fts WHERE ocr_text_fts MATCH ?"
        );

        // Add confidence filter
        if filter.min_confidence.is_some() {
            sql.push_str(" AND confidence >= ?");
        }

        // Execute with parameters
        // ...
    }
}
```

**Step 4: Update API models**

In `screen-api/src/models.rs`:
```rust
#[derive(Deserialize)]
pub struct SearchRequest {
    pub q: String,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub app: Option<String>,
    pub min_confidence: Option<f32>, // NEW FIELD
    pub limit: Option<i64>,
}
```

**Step 5: Update API handler**

In `screen-api/src/handlers/search.rs`:
```rust
pub async fn search_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchRequest>,
) -> Result<Json<Vec<SearchResult>>, AppError> {
    let filter = FrameFilter {
        start_time: params.start_time,
        end_time: params.end_time,
        app_name: params.app,
        min_confidence: params.min_confidence, // NEW
    };

    let results = state.db.search_ocr_text(
        &params.q,
        filter,
        Pagination { limit: params.limit.unwrap_or(100), offset: 0 }
    ).await?;

    Ok(Json(results))
}
```

**Step 6: Add tests**

In `screen-db/tests/query_tests.rs`:
```rust
#[tokio::test]
async fn test_confidence_filtering() {
    let db = DatabaseManager::new(":memory:").await.unwrap();

    // Insert test data with varying confidence
    // ...

    let results = db.search_ocr_text(
        "test",
        FrameFilter {
            min_confidence: Some(0.8),
            ..Default::default()
        },
        Pagination::default()
    ).await.unwrap();

    assert!(results.iter().all(|r| r.confidence >= 0.8));
}
```

### Example 2: Add Automation Command for Double-Click

**Step 1: Add method to InputSimulator**

In `screen-automation/src/input.rs`:
```rust
impl InputSimulator {
    pub fn double_click_at(&self, x: i32, y: i32) -> Result<()> {
        self.click_at(x, y, MouseButton::Left)?;
        std::thread::sleep(Duration::from_millis(50));
        self.click_at(x, y, MouseButton::Left)?;
        Ok(())
    }
}
```

**Step 2: Add method to UIElement**

In `screen-automation/src/element.rs`:
```rust
impl UIElement {
    pub fn double_click(&self) -> Result<()> {
        let (x, y, _, _) = self.bounds()?;
        let input = InputSimulator::new();
        input.double_click_at(x, y)?;
        Ok(())
    }
}
```

**Step 3: Add API endpoint**

In `screen-api/src/handlers/automation.rs`:
```rust
pub async fn double_click_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ClickRequest>,
) -> Result<Json<SuccessResponse>, AppError> {
    let input = state.automation.input();
    input.double_click_at(payload.x, payload.y)?;

    Ok(Json(SuccessResponse {
        message: "Double-clicked successfully".to_string(),
    }))
}
```

**Step 4: Register route**

In `screen-api/src/routes.rs`:
```rust
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // ... existing routes
        .route("/automation/double-click", post(double_click_handler))
        .with_state(state)
}
```

### Example 3: Add Frame Thumbnail Generation

**Step 1: Add thumbnail module**

Create `screen-capture/src/thumbnail.rs`:
```rust
use image::{DynamicImage, imageops::FilterType};

pub struct ThumbnailGenerator {
    max_width: u32,
    max_height: u32,
}

impl ThumbnailGenerator {
    pub fn new(max_width: u32, max_height: u32) -> Self {
        Self { max_width, max_height }
    }

    pub fn generate(&self, image: &DynamicImage) -> DynamicImage {
        image.resize(self.max_width, self.max_height, FilterType::Lanczos3)
    }
}
```

**Step 2: Update capture config**

In `screen-capture/src/lib.rs`:
```rust
pub struct CaptureConfig {
    // ... existing fields
    pub generate_thumbnails: bool,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,
}
```

**Step 3: Integrate in capture pipeline**

In `screen-capture/src/capture.rs`:
```rust
impl ScreenCapture {
    fn process_frame(&mut self, image: DynamicImage) -> Result<Frame> {
        let thumbnail = if self.config.generate_thumbnails {
            let gen = ThumbnailGenerator::new(
                self.config.thumbnail_width,
                self.config.thumbnail_height
            );
            Some(gen.generate(&image))
        } else {
            None
        };

        Ok(Frame {
            image,
            thumbnail,
            // ... other fields
        })
    }
}
```

---

## Testing Strategy

### Unit Tests

Unit tests reside in `#[cfg(test)]` modules within source files.

**Example: Testing frame differencing**

```rust
// screen-capture/src/frame_diff.rs
#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn test_identical_frames_have_zero_diff() {
        let differ = FrameDiffer::new(0.01, DiffMethod::Histogram);
        let img = RgbaImage::new(100, 100);

        let diff = differ.calculate_diff(&img, &img).unwrap();
        assert_eq!(diff, 0.0);
    }

    #[test]
    fn test_completely_different_frames() {
        let differ = FrameDiffer::new(0.01, DiffMethod::Histogram);
        let img1 = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
        let img2 = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));

        let diff = differ.calculate_diff(&img1, &img2).unwrap();
        assert!(diff > 0.5); // Significant difference
    }
}
```

**Running unit tests:**
```bash
cargo test -p screen-capture test_identical_frames
```

### Integration Tests

Integration tests go in `tests/` directory at crate root or workspace root.

**Example: Database integration test**

```rust
// screen-db/tests/integration_tests.rs
use screen_db::{DatabaseManager, NewFrame};
use chrono::Utc;

#[tokio::test]
async fn test_full_database_workflow() {
    // Use in-memory database for tests
    let db = DatabaseManager::new(":memory:").await.unwrap();

    // Insert frame
    let frame = NewFrame {
        timestamp: Utc::now(),
        file_path: "/test/frame.png".to_string(),
        monitor_index: 0,
        active_window: Some("Test Window".to_string()),
        active_app: Some("test.exe".to_string()),
        url: None,
    };

    let frame_id = db.insert_frame(frame).await.unwrap();
    assert!(frame_id > 0);

    // Retrieve frame
    let retrieved = db.get_frame(frame_id).await.unwrap();
    assert!(retrieved.is_some());

    // Search (should return no results for empty OCR)
    let results = db.search_ocr_text(
        "test",
        Default::default(),
        Default::default()
    ).await.unwrap();
    assert_eq!(results.len(), 0);
}
```

### API Testing

**Example: Testing search endpoint**

```rust
// screen-api/tests/integration_tests.rs
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn test_search_endpoint() {
    let app = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/search?q=test&limit=10")
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let results: Vec<SearchResult> = serde_json::from_slice(&body).unwrap();

    assert!(results.len() <= 10);
}
```

### End-to-End Tests

**Example: Full capture-to-search pipeline**

```rust
// tests/integration/test_end_to_end.rs
use screen_capture::ScreenCapture;
use screen_db::DatabaseManager;
use tempfile::tempdir;

#[tokio::test]
async fn test_capture_to_database_to_search() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Initialize database
    let db = DatabaseManager::new(db_path.to_str().unwrap()).await.unwrap();

    // Setup capture
    let config = CaptureConfig::default();
    let mut capture = ScreenCapture::new(config).unwrap();

    // Capture one frame
    let frame = capture.capture_single_frame().await.unwrap();

    // Store in database
    let new_frame = NewFrame {
        timestamp: frame.timestamp,
        file_path: frame.file_path,
        // ... other fields
    };
    let frame_id = db.insert_frame(new_frame).await.unwrap();

    // Verify retrieval
    let retrieved = db.get_frame(frame_id).await.unwrap();
    assert!(retrieved.is_some());
}
```

### Benchmarking

Create benchmarks in `benches/` directory:

```rust
// screen-capture/benches/frame_diff_bench.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use screen_capture::{FrameDiffer, DiffMethod};
use image::RgbaImage;

fn benchmark_histogram_diff(c: &mut Criterion) {
    let differ = FrameDiffer::new(0.006, DiffMethod::Histogram);
    let img1 = RgbaImage::new(1920, 1080);
    let img2 = RgbaImage::new(1920, 1080);

    c.bench_function("histogram_diff_1080p", |b| {
        b.iter(|| differ.calculate_diff(black_box(&img1), black_box(&img2)))
    });
}

criterion_group!(benches, benchmark_histogram_diff);
criterion_main!(benches);
```

Run benchmarks:
```bash
cargo bench -p screen-capture
```

---

## Debugging & Profiling

### Debug Logging

ScreenSearch uses the `tracing` crate for structured logging.

**Log levels:**
- `trace` - Very verbose, low-level details
- `debug` - Developer information
- `info` - General application flow
- `warn` - Unexpected but recoverable situations
- `error` - Errors requiring attention

**Enable logging:**

```powershell
# All logs at debug level
$env:RUST_LOG="debug"
cargo run

# Per-module logging
$env:RUST_LOG="screen_capture=trace,screen_db=debug,screen_api=info"
cargo run

# Specific module
$env:RUST_LOG="screen_capture::ocr=trace"
cargo run
```

**Logging in code:**

```rust
use tracing::{info, warn, error, debug, trace};

// Structured logging
info!(frame_id = 123, monitor = 0, "Processing frame");
warn!(error = ?err, "Retrying operation");
error!("Fatal error occurred");

// Debug and trace for verbose output
debug!("Frame difference: {:.2}%", diff);
trace!("OCR region: {:?}", region);
```

### Common Issues

#### Issue: Compilation errors with Windows APIs

**Symptom:** Linker errors or missing Windows SDK

**Solution:**
```powershell
# Verify Visual Studio Build Tools
cl.exe

# If missing, reinstall
winget install Microsoft.VisualStudio.2022.BuildTools
```

#### Issue: OCR not working

**Symptom:** `OcrEngine::new()` fails or returns empty results

**Solution:**
1. Verify Windows OCR language pack installed
2. Check Settings > Language > English > Options
3. Download language pack if missing

#### Issue: Database locked

**Symptom:** `database is locked` error

**Solution:**
```bash
# Ensure no other instances running
tasklist | findstr screen-memories

# Remove WAL files
rm screen_memories.db-wal
rm screen_memories.db-shm
```

#### Issue: High CPU usage

**Symptom:** > 10% CPU during idle

**Solution:**
1. Increase capture interval in config
2. Raise frame diff threshold
3. Check for infinite loops in capture thread

### Visual Studio Code Debugging

**Launch configuration (`.vscode/launch.json`):**

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug screen-capture",
      "cargo": {
        "args": [
          "build",
          "--bin=screen-memories",
          "--package=screen-memories"
        ],
        "filter": {
          "name": "screen-memories",
          "kind": "bin"
        }
      },
      "args": [],
      "cwd": "${workspaceFolder}",
      "env": {
        "RUST_LOG": "debug",
        "RUST_BACKTRACE": "1"
      }
    }
  ]
}
```

Set breakpoints in VS Code by clicking left of line numbers, then press F5 to start debugging.

### Profiling

#### CPU Profiling with Flamegraph

```bash
# Install flamegraph
cargo install flamegraph

# Profile application (requires admin privileges on Windows)
cargo flamegraph --release

# Output: flamegraph.svg (open in browser)
```

#### Memory Profiling

Use Windows Performance Analyzer or Task Manager:

```powershell
# Monitor memory usage
tasklist | findstr screen-memories
```

#### Performance Checklist

- [ ] CPU usage < 5% idle
- [ ] Memory usage < 500MB
- [ ] API response time < 100ms (p95)
- [ ] Frame capture < 50ms
- [ ] OCR processing < 200ms per frame
- [ ] Database queries < 50ms

---

## Release Process

### Version Bump

ScreenSearch uses semantic versioning: `MAJOR.MINOR.PATCH`

**Step 1: Update version numbers**

Update `version` in `Cargo.toml`:
```toml
[workspace.package]
version = "0.2.0"
```

**Step 2: Update CHANGELOG**

Create/update `CHANGELOG.md`:
```markdown
# Changelog

## [0.2.0] - 2025-12-15

### Added
- Thumbnail generation for captured frames
- OCR confidence filtering in search API
- Double-click automation command

### Fixed
- Database locking issue on concurrent writes
- Memory leak in frame buffer

### Changed
- Improved frame differencing performance by 30%
```

**Step 3: Commit and tag**

```bash
git add .
git commit -m "Bump version to 0.2.0"
git tag -a v0.2.0 -m "Version 0.2.0 - Thumbnail support and performance improvements"
git push origin main --tags
```

### Build Release Binary

```bash
# Build with release optimizations
cargo build --release

# Binary location
# target\release\screen-memories.exe

# Test release build
.\target\release\screen-memories.exe --help
```

**Release build optimizations (configured in `Cargo.toml`):**
- `opt-level = 3` - Maximum optimization
- `lto = true` - Link-time optimization
- `codegen-units = 1` - Single codegen unit for better optimization
- `strip = true` - Strip debug symbols (smaller binary)

### Create Distribution Package

```powershell
# Create distribution directory
mkdir screen-memories-v0.2.0

# Copy files
copy target\release\screen-memories.exe screen-memories-v0.2.0\
copy config.toml screen-memories-v0.2.0\
copy README.md screen-memories-v0.2.0\
copy LICENSE screen-memories-v0.2.0\

# Create zip archive
Compress-Archive -Path screen-memories-v0.2.0 -DestinationPath screen-memories-v0.2.0-windows-x64.zip
```

### Frontend Release

```bash
cd screen-ui

# Build production frontend
npm run build

# Output in dist/ directory
```

Serve with static file server or bundle with Rust binary using `tower-http` static file serving.

### GitHub Release

1. Go to GitHub repository > Releases
2. Click "Create new release"
3. Select tag `v0.2.0`
4. Title: "ScreenSearch v0.2.0"
5. Description: Copy from CHANGELOG
6. Upload `screen-memories-v0.2.0-windows-x64.zip`
7. Publish release

---

## Contributing Guidelines

### Code Style

**Rust:**
- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `rustfmt` for formatting (configured in `rustfmt.toml`)
- Use `clippy` for linting (CI enforces clippy warnings)
- Document all public APIs with doc comments (`///`)
- Keep functions under 50 lines when possible
- Prefer composition over inheritance

**TypeScript/React:**
- Use ESLint configuration (`.eslintrc.json`)
- Functional components with hooks
- Avoid CSS-in-JS, use Tailwind classes
- Document complex logic with comments

### Error Handling

**Libraries (screen-capture, screen-db, screen-automation):**

Use `thiserror` for custom error types:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("Monitor not found: {0}")]
    MonitorNotFound(usize),

    #[error("Frame differencing failed: {0}")]
    FrameDiffFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CaptureError>;
```

**Applications (main.rs, API handlers):**

Use `anyhow` for application errors:
```rust
use anyhow::{Context, Result};

fn main() -> Result<()> {
    let config = load_config()
        .context("Failed to load configuration")?;

    start_server(config)
        .context("Failed to start server")?;

    Ok(())
}
```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

- `feat: add OCR confidence filtering`
- `fix: resolve database locking issue`
- `docs: update developer guide`
- `refactor: simplify frame differencing logic`
- `test: add integration tests for search API`
- `chore: bump dependencies`

### Pull Request Process

1. **Fork and branch**
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make changes**
   - Write code following style guide
   - Add tests for new functionality
   - Update documentation

3. **Verify quality**
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test --all-targets
   ```

4. **Commit and push**
   ```bash
   git commit -m "feat: add your feature"
   git push origin feature/your-feature-name
   ```

5. **Create pull request**
   - Title: Clear description of change
   - Description: Why the change is needed, how it works
   - Link related issues

6. **Code review**
   - Address reviewer feedback
   - Update PR with changes
   - Ensure CI passes

### Documentation Requirements

- Public APIs must have doc comments
- Complex algorithms need explanation
- Update README.md for user-facing changes
- Update this developer guide for architectural changes

**Doc comment example:**

```rust
/// Captures a single frame from the specified monitor.
///
/// # Arguments
///
/// * `monitor_index` - Zero-based monitor index
///
/// # Returns
///
/// Returns a `Frame` containing the captured image and metadata.
///
/// # Errors
///
/// Returns `CaptureError::MonitorNotFound` if monitor doesn't exist.
///
/// # Example
///
/// ```
/// use screen_capture::ScreenCapture;
///
/// let mut capture = ScreenCapture::new(Default::default())?;
/// let frame = capture.capture_frame(0)?;
/// println!("Captured {}x{}", frame.width(), frame.height());
/// ```
pub fn capture_frame(&mut self, monitor_index: usize) -> Result<Frame> {
    // Implementation
}
```

### Testing Requirements

All pull requests must include:

- Unit tests for new functions
- Integration tests for new features
- Tests must pass on Windows
- No decrease in code coverage (if coverage is tracked)

### Performance Requirements

- Maintain < 5% CPU usage idle
- Memory usage < 500MB
- API response times < 100ms (p95)
- No regressions in benchmarks

### Security Considerations

- Never commit secrets or API keys
- Validate all user input
- Sanitize file paths
- Use prepared statements for SQL
- Document security implications

---

## Resources

### Rust Documentation

- [Rust Book](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Cargo Book](https://doc.rust-lang.org/cargo/)

### Async Rust

- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Async Book](https://rust-lang.github.io/async-book/)

### Web Framework

- [Axum Documentation](https://docs.rs/axum/)
- [Tower Documentation](https://docs.rs/tower/)

### Database

- [SQLx Guide](https://github.com/launchbadge/sqlx)
- [SQLite Documentation](https://www.sqlite.org/docs.html)
- [FTS5 Full-Text Search](https://www.sqlite.org/fts5.html)

### Windows APIs

- [Windows Crate](https://docs.rs/windows/)
- [UIAutomation Documentation](https://docs.microsoft.com/en-us/windows/win32/winauto/uiauto-uiautomation)
- [Graphics Capture API](https://docs.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture)

### Frontend

- [React Documentation](https://react.dev/)
- [TanStack Query](https://tanstack.com/query/latest)
- [Vite Guide](https://vitejs.dev/guide/)
- [Tailwind CSS](https://tailwindcss.com/docs)

---

## Appendix

### Project History

ScreenSearch began as a Windows-focused rewrite of the screenpipe project, simplifying the architecture while maintaining core screen capture and OCR functionality.

**Design decisions:**
- Rust-only backend (no Node.js)
- Windows-exclusive (Graphics Capture API, UIAutomation)
- Local-first (all data stays on device)
- REST API for extensibility
- React frontend for modern UX

### Performance Targets (Reference)

Based on CLAUDE.md specifications:

| Metric | Target | Current |
|--------|--------|---------|
| CPU usage (idle) | < 5% | TBD |
| Memory usage | < 500MB | TBD |
| API response time (p95) | < 100ms | TBD |
| Database scalability | 100k+ frames | TBD |
| Frame capture latency | < 50ms | TBD |

### Inspiration Directory

The `Inspiration/` directory contains reference code from screenpipe:

**Use these modules:**
- `screenpipe-core/` - Screen capture and OCR patterns
- `screenpipe-db/` - Database architecture and migrations
- `screenpipe-vision/` - OCR processing pipeline

**Ignore these modules:**
- Audio capture (out of scope)
- UI components (using React instead)
- Plugin system (not implemented)

### Workspace Dependency Strategy

Shared dependencies are defined in `[workspace.dependencies]` to ensure version consistency:

```toml
[workspace.dependencies]
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"
# ... etc
```

Individual crates reference with `{ workspace = true }`:

```toml
[dependencies]
tokio = { workspace = true }
```

This prevents version conflicts across crates.

---

**Happy coding! For questions or issues, open a ticket on GitHub or consult the technical documentation in `/docs`.**

---

**File:** `\path\to\app\ScreenSearch\docs\developer-guide.md`
**Lines:** ~500
**Last Updated:** 2025-12-10
