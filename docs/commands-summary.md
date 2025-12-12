# Commands Summary

Quick reference for all CLI commands and API endpoints in Screen Memory.

## Build Commands

```bash
# Check all crates (fast, no build)
cargo check

# Build all crates (debug)
cargo build

# Build release binary
cargo build --release

# Build specific crate
cargo build -p screen-capture
cargo build -p screen-db
cargo build -p screen-api
cargo build -p screen-automation
```

## Test Commands

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p screen-db          # 16 tests
cargo test -p screen-capture     # 33 tests
cargo test -p screen-api         # 11 tests
cargo test -p screen-automation  # 13 tests

# Run specific test
cargo test test_fts5_search

# Run with output
cargo test -- --nocapture

# Run integration tests only
cargo test --test integration
```

## Code Quality

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Run linter
cargo clippy

# Auto-fix warnings
cargo clippy --fix

# Generate documentation
cargo doc --no-deps --open
```

## Run Commands

```bash
# Run application (debug)
cargo run

# Run application (release)
cargo run --release

# Run with debug logging
$env:RUST_LOG="debug"; cargo run

# Run with specific log level
$env:RUST_LOG="screen_memories=debug,sqlx=warn"; cargo run
```

## Frontend Commands

```bash
cd screen-ui

# Install dependencies
npm install

# Start development server
npm run dev

# Build for production
npm run build

# Run linter
npm run lint
```

---

## API Quick Reference

**Base URL:** `http://localhost:3131`

### Health & Status

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Server health check |

### Search Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/search` | Full-text search with filters |
| GET | `/search/keywords` | Keyword-based search |
| GET | `/frames` | Retrieve captured frames |

### Search Parameters

```
GET /search?q=<query>&limit=50&start_time=<ISO>&end_time=<ISO>&app=<name>
GET /search/keywords?keywords=word1,word2&limit=50
GET /frames?limit=20&offset=0&start_time=<ISO>&end_time=<ISO>
```

### Tag Management

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/tags` | List all tags |
| POST | `/tags` | Create new tag |
| DELETE | `/tags/:id` | Delete tag |
| GET | `/frames/:id/tags` | Get frame tags |
| POST | `/frames/:id/tags` | Add tag to frame |
| DELETE | `/frames/:id/tags/:tag_id` | Remove tag from frame |

### Automation Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/automation/find-elements` | Find UI elements |
| POST | `/automation/click` | Click at coordinates |
| POST | `/automation/type` | Type text |
| POST | `/automation/scroll` | Scroll action |
| POST | `/automation/press-key` | Press keyboard key |
| POST | `/automation/get-text` | Get element text |
| POST | `/automation/list-elements` | List UI elements |
| POST | `/automation/open-app` | Open application |
| POST | `/automation/open-url` | Open URL in browser |

---

## API Examples

### Search

```bash
# Full-text search
curl "http://localhost:3131/search?q=hello&limit=10"

# Search with time range
curl "http://localhost:3131/search?q=meeting&start_time=2025-01-01T00:00:00Z"

# Keyword search
curl "http://localhost:3131/search/keywords?keywords=error,warning&limit=20"
```

### Frames

```bash
# Get recent frames
curl "http://localhost:3131/frames?limit=10"

# Get frames with pagination
curl "http://localhost:3131/frames?limit=20&offset=40"
```

### Tags

```bash
# List tags
curl http://localhost:3131/tags

# Create tag
curl -X POST http://localhost:3131/tags \
  -H "Content-Type: application/json" \
  -d '{"tag_name":"important","description":"Important frames","color":"#FF0000"}'

# Delete tag
curl -X DELETE http://localhost:3131/tags/1

# Add tag to frame
curl -X POST http://localhost:3131/frames/123/tags \
  -H "Content-Type: application/json" \
  -d '{"tag_id":1}'
```

### Automation

```bash
# Click at coordinates
curl -X POST http://localhost:3131/automation/click \
  -H "Content-Type: application/json" \
  -d '{"x":100,"y":200,"button":"left"}'

# Type text
curl -X POST http://localhost:3131/automation/type \
  -H "Content-Type: application/json" \
  -d '{"text":"Hello World"}'

# Press key with modifier
curl -X POST http://localhost:3131/automation/press-key \
  -H "Content-Type: application/json" \
  -d '{"key":"c","modifiers":["ctrl"]}'

# Open application
curl -X POST http://localhost:3131/automation/open-app \
  -H "Content-Type: application/json" \
  -d '{"name":"notepad"}'

# Open URL
curl -X POST http://localhost:3131/automation/open-url \
  -H "Content-Type: application/json" \
  -d '{"url":"https://example.com"}'
```

---

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Logging level (error, warn, info, debug, trace) | info |
| `SCREEN_DB_PATH` | Database file path | ./screen_memories.db |

## Configuration Quick Reference

**File:** `config.toml`

```toml
[capture]
interval_ms = 3000              # Capture interval (ms)
enable_frame_diff = true        # Skip unchanged frames
diff_threshold = 0.006          # Change threshold (0.6%)
max_frames_buffer = 30          # Frame buffer size

[storage]
format = "jpeg"                 # Image format (png/jpeg)
jpeg_quality = 80               # JPEG quality (1-100)
max_width = 1920                # Max image width (0 = original)

[ocr]
engine = "windows"              # OCR engine
min_confidence = 0.7            # Confidence threshold
worker_threads = 2              # OCR worker threads

[api]
host = "127.0.0.1"              # API host
port = 3131                     # API port

[database]
path = "screen_memories.db"     # Database path
max_connections = 50            # Connection pool max
enable_wal = true               # WAL mode

[privacy]
excluded_apps = ["1Password", "KeePass"]  # Apps to exclude
pause_on_lock = true            # Pause on screen lock

[logging]
level = "info"                  # Log level
log_to_file = true              # Enable file logging
```

---

*For detailed documentation, see [API Reference](api-reference.md) and [User Guide](user-guide.md).*
