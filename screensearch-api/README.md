# ScreenSearch REST API

REST API server for ScreenSearch - provides search capabilities for captured screen content and computer automation controls.

## Features

- **Full-text search** with FTS5 and BM25 ranking
- **Keyword search** with exact matching
- **Frame retrieval** with time-based filtering
- **Computer automation** via Windows UIAutomation API
- **Tag management** for organizing captured content
- **Health monitoring** with database statistics

## Quick Start

### Running the Server

```bash
# Development mode
cargo run

# Release mode
cargo run --release

# Custom database path
SCREEN_DB_PATH=/path/to/db.sqlite cargo run
```

The server starts on `http://localhost:3131` by default.

### Configuration

Set environment variables:
- `RUST_LOG` - Logging level (default: `screensearch_api=debug`)
- `SCREEN_DB_PATH` - Database file path (default: `screensearch.db`)

## API Endpoints

### Context Retrieval

#### GET /search
Full-text search across OCR content with FTS5.

**Query Parameters:**
- `q` (required): Search query string
- `start_time` (optional): Start time filter (ISO 8601)
- `end_time` (optional): End time filter (ISO 8601)
- `app` (optional): Filter by application name
- `limit` (optional): Max results (default: 100)

**Example:**
```bash
curl "http://localhost:3131/search?q=hello&limit=10"
```

**Response:**
```json
[
  {
    "frame": {
      "id": 1,
      "timestamp": "2024-01-01T12:00:00Z",
      "file_path": "/path/to/frame.png",
      "active_window": "Chrome",
      ...
    },
    "ocr_matches": [
      {
        "id": 1,
        "text": "hello world",
        "x": 100,
        "y": 200,
        ...
      }
    ],
    "relevance_score": 0.95,
    "tags": []
  }
]
```

#### GET /search/keywords
Keyword-based search with exact matching.

**Query Parameters:**
- `keywords` (required): Comma-separated keywords
- `limit` (optional): Max results (default: 100)

**Example:**
```bash
curl "http://localhost:3131/search/keywords?keywords=password,login"
```

#### GET /frames
Retrieve captured frames with metadata.

**Query Parameters:**
- `start_time` (optional): Start time filter
- `end_time` (optional): End time filter
- `monitor_index` (optional): Monitor index filter
- `limit` (optional): Max results (default: 100)

**Example:**
```bash
curl "http://localhost:3131/frames?limit=20"
```

#### GET /health
System health check and statistics.

**Example:**
```bash
curl "http://localhost:3131/health"
```

**Response:**
```json
{
  "status": "ok",
  "version": "0.1.0",
  "frame_count": 1523,
  "ocr_count": 15234,
  "tag_count": 5,
  "oldest_frame": "2024-01-01T00:00:00Z",
  "newest_frame": "2024-01-10T23:59:59Z"
}
```

### Computer Automation

#### POST /automation/find-elements
Locate UI elements by selector.

**Request Body:**
```json
{
  "selector": "Button[@Name='Save']",
  "timeout_ms": 5000
}
```

#### POST /automation/click
Click at screen coordinates.

**Request Body:**
```json
{
  "x": 100,
  "y": 200,
  "button": "left"
}
```

**Button options:** `"left"`, `"right"`, `"middle"`

#### POST /automation/type
Type text into the active element.

**Request Body:**
```json
{
  "text": "Hello, World!",
  "delay_ms": 50
}
```

#### POST /automation/scroll
Scroll in a direction.

**Request Body:**
```json
{
  "direction": "down",
  "amount": 3
}
```

**Direction options:** `"up"`, `"down"`, `"left"`, `"right"`

#### POST /automation/press-key
Press a keyboard key with modifiers.

**Request Body:**
```json
{
  "key": "enter",
  "modifiers": ["ctrl", "shift"]
}
```

**Key examples:** `"enter"`, `"escape"`, `"tab"`, `"a"`, `"f1"`

**Modifiers:** `"ctrl"`, `"alt"`, `"shift"`, `"win"`

#### POST /automation/get-text
Extract text from a UI element.

**Request Body:**
```json
{
  "selector": "Edit[@Name='Search']"
}
```

#### POST /automation/list-elements
List interactive elements in active window.

**Request Body:**
```json
{
  "root_selector": "Window[@Name='Chrome']"
}
```

#### POST /automation/open-app
Launch an application.

**Request Body:**
```json
{
  "app_name": "notepad.exe"
}
```

#### POST /automation/open-url
Open URL in default browser.

**Request Body:**
```json
{
  "url": "https://example.com"
}
```

### System Management

#### GET /tags
List all available tags.

**Query Parameters:**
- `limit` (optional): Max results (default: 100)
- `offset` (optional): Skip N results (default: 0)

**Example:**
```bash
curl "http://localhost:3131/tags"
```

#### POST /tags
Create a new tag.

**Request Body:**
```json
{
  "tag_name": "important",
  "description": "Important screens",
  "color": "#FF0000"
}
```

#### DELETE /tags/:id
Delete a tag by ID.

**Example:**
```bash
curl -X DELETE "http://localhost:3131/tags/1"
```

#### GET /frames/:id/tags
Get all tags for a specific frame.

**Example:**
```bash
curl "http://localhost:3131/frames/123/tags"
```

#### POST /frames/:id/tags
Add a tag to a frame.

**Request Body:**
```json
{
  "tag_id": 1
}
```

**Example:**
```bash
curl -X POST "http://localhost:3131/frames/123/tags" \
  -H "Content-Type: application/json" \
  -d '{"tag_id": 1}'
```

#### DELETE /frames/:id/tags/:tag_id
Remove a tag from a frame.

**Example:**
```bash
curl -X DELETE "http://localhost:3131/frames/123/tags/1"
```

## Error Handling

All endpoints return standard HTTP status codes:

- `200 OK` - Success
- `400 Bad Request` - Invalid request parameters
- `404 Not Found` - Resource not found
- `500 Internal Server Error` - Server error

Error response format:
```json
{
  "error": "Error message here",
  "status": 400
}
```

## Performance Targets

- Search response time: < 100ms (p95)
- Frame retrieval: < 50ms (p95)
- Automation actions: < 200ms (p95)
- Concurrent connections: 50+

## Architecture

The API is built with:
- **Axum** - Modern async web framework
- **Tower HTTP** - Middleware (CORS, tracing)
- **SQLite + FTS5** - Full-text search backend
- **UIAutomation** - Windows automation API

### State Management

```rust
pub struct AppState {
    pub db: Arc<DatabaseManager>,
    pub automation: Arc<AutomationEngine>,
}
```

State is shared across all handlers using `Arc` for thread-safe access.

### Middleware Stack

1. **CORS Layer** - Permissive CORS for local development
2. **Trace Layer** - Request/response logging
3. **Error Handling** - Custom `AppError` type with HTTP response conversion

## Development

### Running Tests

```bash
# Unit tests
cargo test --lib

# Integration tests (requires running server)
cargo test --test integration_tests -- --ignored

# All tests
cargo test
```

### Building

```bash
# Debug build
cargo build

# Release build with optimizations
cargo build --release
```

### Code Structure

```
src/
├── lib.rs              # Library exports
├── main.rs             # Binary entry point
├── error.rs            # Error types and handling
├── state.rs            # Application state
├── routes.rs           # Route definitions
├── server.rs           # Server initialization
├── models.rs           # Request/response models
└── handlers/
    ├── mod.rs          # Handler module exports
    ├── search.rs       # Search endpoints
    ├── automation.rs   # Automation endpoints
    └── system.rs       # System management endpoints
```

## Production Deployment

### Recommendations

1. **Reverse Proxy** - Use nginx or Caddy for TLS and rate limiting
2. **Database Backups** - Regular SQLite backups
3. **Monitoring** - Track `/health` endpoint for uptime
4. **Resource Limits** - Set memory/CPU limits via systemd or Docker
5. **Log Rotation** - Configure log rotation for production logs

### Example systemd Service

```ini
[Unit]
Description=ScreenSearch API Server
After=network.target

[Service]
Type=simple
User=screensearch
WorkingDirectory=/opt/screensearch
ExecStart=/opt/screensearch/screensearch-api
Restart=on-failure
Environment="RUST_LOG=screensearch_api=info"
Environment="SCREEN_DB_PATH=/var/lib/screensearch/data.db"

[Install]
WantedBy=multi-user.target
```

## Security Considerations

- **Local Only** - Binds to `127.0.0.1` by default
- **No Authentication** - Designed for local use only
- **Automation Risks** - UI automation has full desktop access
- **Data Privacy** - All data stored locally, never transmitted

## License

See project root LICENSE file.
