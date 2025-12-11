# ScreenSearch API Reference

Complete API reference for the ScreenSearch REST API server. This API provides search capabilities for captured screen content, computer automation controls, and tag management.

## Overview

### Base URL
```
http://localhost:3131
```

### Content Type
All endpoints accept and return JSON unless otherwise specified:
```
Content-Type: application/json
```

### Authentication
No authentication required. The API is designed for local use only and binds to `127.0.0.1` by default.

### Response Format
All successful responses return JSON with appropriate HTTP status codes. Error responses follow a consistent format:

```json
{
  "error": "Error message describing what went wrong",
  "status": 400
}
```

### HTTP Status Codes

| Status Code | Description |
|-------------|-------------|
| `200 OK` | Request succeeded |
| `400 Bad Request` | Invalid request parameters or malformed JSON |
| `404 Not Found` | Resource not found |
| `500 Internal Server Error` | Server error or automation failure |

---

## Context Retrieval Endpoints

### GET /search

Full-text search across all captured OCR content using SQLite FTS5 with BM25 ranking.

#### Query Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `q` | string | Yes | - | Search query string (FTS5 syntax supported) |
| `start_time` | string | No | - | Filter results after this time (ISO 8601 format) |
| `end_time` | string | No | - | Filter results before this time (ISO 8601 format) |
| `app` | string | No | - | Filter by application name |
| `limit` | integer | No | 100 | Maximum number of results to return |

#### Response

Returns an array of search results, each containing the matching frame, OCR text matches, and relevance score.

```json
[
  {
    "frame": {
      "id": 1,
      "timestamp": "2025-12-10T10:30:00Z",
      "file_path": "C:\\captures\\frame_001.png",
      "active_window": "Chrome - Google Search",
      "monitor_index": 0,
      "width": 1920,
      "height": 1080
    },
    "ocr_matches": [
      {
        "id": 1,
        "frame_id": 1,
        "text": "hello world example",
        "x": 100,
        "y": 200,
        "width": 150,
        "height": 20,
        "confidence": 0.95
      }
    ],
    "relevance_score": 0.85,
    "tags": [
      {
        "id": 1,
        "tag_name": "important",
        "description": "Important screens",
        "color": "#FF0000"
      }
    ]
  }
]
```

#### Example

```bash
# Basic search
curl "http://localhost:3131/search?q=hello&limit=10"

# Search with time filter
curl "http://localhost:3131/search?q=password&start_time=2025-12-10T00:00:00Z&end_time=2025-12-10T23:59:59Z"

# Search by application
curl "http://localhost:3131/search?q=error&app=Chrome"
```

---

### GET /search/keywords

Keyword-based search with exact matching. Useful for finding specific terms across captured content.

#### Query Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `keywords` | string | Yes | - | Comma-separated keywords to search for |
| `limit` | integer | No | 100 | Maximum number of results to return |

#### Response

Returns an array of frames containing the specified keywords.

```json
[
  {
    "frame": {
      "id": 2,
      "timestamp": "2025-12-10T11:00:00Z",
      "file_path": "C:\\captures\\frame_002.png",
      "active_window": "Notepad",
      "monitor_index": 0
    },
    "matching_keywords": ["password", "login"],
    "match_count": 2
  }
]
```

#### Example

```bash
# Search for multiple keywords
curl "http://localhost:3131/search/keywords?keywords=password,login,authentication"

# Single keyword search
curl "http://localhost:3131/search/keywords?keywords=error&limit=50"
```

---

### GET /frames

Retrieve captured frames with optional filtering by time and monitor.

#### Query Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `start_time` | string | No | - | Filter frames after this time (ISO 8601) |
| `end_time` | string | No | - | Filter frames before this time (ISO 8601) |
| `monitor_index` | integer | No | - | Filter by monitor index (0-based) |
| `limit` | integer | No | 100 | Maximum number of results to return |

#### Response

```json
[
  {
    "id": 1,
    "timestamp": "2025-12-10T10:30:00Z",
    "file_path": "C:\\captures\\frame_001.png",
    "active_window": "Visual Studio Code",
    "monitor_index": 0,
    "width": 1920,
    "height": 1080,
    "frame_hash": "a1b2c3d4e5f6",
    "tags": []
  }
]
```

#### Example

```bash
# Get recent frames
curl "http://localhost:3131/frames?limit=20"

# Get frames from specific time range
curl "http://localhost:3131/frames?start_time=2025-12-10T00:00:00Z&end_time=2025-12-10T12:00:00Z"

# Get frames from specific monitor
curl "http://localhost:3131/frames?monitor_index=1&limit=10"
```

---

### GET /health

Health check endpoint providing system status and database statistics.

#### Response

```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_seconds": 3600,
  "frame_count": 1523,
  "ocr_count": 15234,
  "tag_count": 5,
  "oldest_frame": "2025-12-01T00:00:00Z",
  "newest_frame": "2025-12-10T23:59:59Z"
}
```

#### Status Values

- `ok` - System is healthy and operational
- `degraded` - System is operational but experiencing issues
- `error` - System has critical errors

#### Example

```bash
curl "http://localhost:3131/health"
```

---

## Computer Automation Endpoints

All automation endpoints use POST requests and accept JSON request bodies. These endpoints interact with the Windows UIAutomation API to control the desktop.

### POST /automation/find-elements

Locate UI elements on screen using selector syntax.

#### Request Body

```json
{
  "selector": "Button[@Name='Save']",
  "timeout_ms": 5000
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `selector` | string | Yes | - | Element selector (UIAutomation syntax) |
| `timeout_ms` | integer | No | 5000 | Maximum time to wait for elements (milliseconds) |

#### Selector Syntax

- `Button[@Name='Save']` - Button with Name property
- `Edit[@AutomationId='searchBox']` - Edit field by AutomationId
- `Window[@Name='Chrome']` - Window by name
- `Text[@Name*='contains']` - Partial name match

#### Response

```json
{
  "elements": [
    {
      "name": "Save",
      "control_type": "Button",
      "x": 100,
      "y": 200,
      "width": 80,
      "height": 30,
      "is_enabled": true,
      "is_visible": true
    }
  ]
}
```

#### Example

```bash
curl -X POST "http://localhost:3131/automation/find-elements" \
  -H "Content-Type: application/json" \
  -d '{"selector": "Button[@Name=\"Submit\"]", "timeout_ms": 3000}'
```

---

### POST /automation/click

Simulate mouse click at specified screen coordinates.

#### Request Body

```json
{
  "x": 100,
  "y": 200,
  "button": "left"
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `x` | integer | Yes | - | X coordinate on screen |
| `y` | integer | Yes | - | Y coordinate on screen |
| `button` | string | No | "left" | Mouse button: "left", "right", or "middle" |

#### Response

```json
{
  "success": true,
  "message": "Click performed at (100, 200)"
}
```

#### Example

```bash
# Left click
curl -X POST "http://localhost:3131/automation/click" \
  -H "Content-Type: application/json" \
  -d '{"x": 500, "y": 300}'

# Right click
curl -X POST "http://localhost:3131/automation/click" \
  -H "Content-Type: application/json" \
  -d '{"x": 500, "y": 300, "button": "right"}'
```

---

### POST /automation/type

Type text into the currently focused UI element.

#### Request Body

```json
{
  "text": "Hello, World!",
  "delay_ms": 50
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `text` | string | Yes | - | Text to type |
| `delay_ms` | integer | No | 0 | Delay between characters (milliseconds) |

#### Response

```json
{
  "success": true,
  "message": "Text typed successfully"
}
```

#### Example

```bash
curl -X POST "http://localhost:3131/automation/type" \
  -H "Content-Type: application/json" \
  -d '{"text": "Hello, World!", "delay_ms": 100}'
```

---

### POST /automation/scroll

Scroll the active window or element in a specified direction.

#### Request Body

```json
{
  "direction": "down",
  "amount": 3
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `direction` | string | Yes | - | Scroll direction: "up", "down", "left", "right" |
| `amount` | integer | Yes | - | Scroll amount (lines or units) |

#### Response

```json
{
  "success": true,
  "message": "Scrolled down by 3 units"
}
```

#### Example

```bash
# Scroll down
curl -X POST "http://localhost:3131/automation/scroll" \
  -H "Content-Type: application/json" \
  -d '{"direction": "down", "amount": 5}'

# Scroll up
curl -X POST "http://localhost:3131/automation/scroll" \
  -H "Content-Type: application/json" \
  -d '{"direction": "up", "amount": 2}'
```

---

### POST /automation/press-key

Press a keyboard key with optional modifier keys.

#### Request Body

```json
{
  "key": "enter",
  "modifiers": ["ctrl", "shift"]
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `key` | string | Yes | - | Key to press (see key names below) |
| `modifiers` | array | No | [] | Modifier keys: "ctrl", "alt", "shift", "win" |

#### Supported Keys

- **Special**: "enter", "escape", "tab", "backspace", "delete", "space"
- **Function**: "f1", "f2", ... "f12"
- **Navigation**: "up", "down", "left", "right", "home", "end", "pageup", "pagedown"
- **Characters**: "a"-"z", "0"-"9", and punctuation

#### Response

```json
{
  "success": true,
  "message": "Key pressed: ctrl+shift+enter"
}
```

#### Example

```bash
# Press Enter
curl -X POST "http://localhost:3131/automation/press-key" \
  -H "Content-Type: application/json" \
  -d '{"key": "enter"}'

# Press Ctrl+S (Save)
curl -X POST "http://localhost:3131/automation/press-key" \
  -H "Content-Type: application/json" \
  -d '{"key": "s", "modifiers": ["ctrl"]}'

# Press Ctrl+Shift+P
curl -X POST "http://localhost:3131/automation/press-key" \
  -H "Content-Type: application/json" \
  -d '{"key": "p", "modifiers": ["ctrl", "shift"]}'
```

---

### POST /automation/get-text

Extract text content from a UI element specified by selector.

#### Request Body

```json
{
  "selector": "Edit[@Name='Search']"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `selector` | string | Yes | Element selector to extract text from |

#### Response

```json
{
  "text": "Extracted text content from the element"
}
```

#### Example

```bash
curl -X POST "http://localhost:3131/automation/get-text" \
  -H "Content-Type: application/json" \
  -d '{"selector": "Edit[@AutomationId=\"searchBox\"]"}'
```

---

### POST /automation/list-elements

List all interactive UI elements in the active window or under a specified root element.

#### Request Body

```json
{
  "root_selector": "Window[@Name='Chrome']"
}
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `root_selector` | string | No | Active window | Root element to start listing from |

#### Response

```json
{
  "elements": [
    {
      "name": "Address bar",
      "control_type": "Edit",
      "x": 200,
      "y": 100,
      "width": 600,
      "height": 30,
      "is_enabled": true,
      "is_visible": true
    },
    {
      "name": "Refresh",
      "control_type": "Button",
      "x": 850,
      "y": 100,
      "width": 40,
      "height": 30,
      "is_enabled": true,
      "is_visible": true
    }
  ]
}
```

#### Example

```bash
# List all elements in active window
curl -X POST "http://localhost:3131/automation/list-elements" \
  -H "Content-Type: application/json" \
  -d '{}'

# List elements in specific window
curl -X POST "http://localhost:3131/automation/list-elements" \
  -H "Content-Type: application/json" \
  -d '{"root_selector": "Window[@Name=\"Visual Studio Code\"]"}'
```

---

### POST /automation/open-app

Launch an application by name or executable path.

#### Request Body

```json
{
  "app_name": "notepad.exe"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `app_name` | string | Yes | Application name or full path to executable |

#### Response

```json
{
  "success": true,
  "message": "Application launched: notepad.exe"
}
```

#### Example

```bash
# Launch Notepad
curl -X POST "http://localhost:3131/automation/open-app" \
  -H "Content-Type: application/json" \
  -d '{"app_name": "notepad.exe"}'

# Launch with full path
curl -X POST "http://localhost:3131/automation/open-app" \
  -H "Content-Type: application/json" \
  -d '{"app_name": "C:\\Program Files\\MyApp\\app.exe"}'
```

---

### POST /automation/open-url

Open a URL in the default web browser.

#### Request Body

```json
{
  "url": "https://example.com"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `url` | string | Yes | URL to open (must include protocol) |

#### Response

```json
{
  "success": true,
  "message": "URL opened: https://example.com"
}
```

#### Example

```bash
curl -X POST "http://localhost:3131/automation/open-url" \
  -H "Content-Type: application/json" \
  -d '{"url": "https://github.com"}'
```

---

## Tag Management Endpoints

### GET /tags

List all available tags with optional pagination.

#### Query Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `limit` | integer | No | 100 | Maximum number of tags to return |
| `offset` | integer | No | 0 | Number of tags to skip |

#### Response

```json
[
  {
    "id": 1,
    "tag_name": "important",
    "description": "Important screens to review",
    "color": "#FF0000",
    "created_at": "2025-12-01T10:00:00Z"
  },
  {
    "id": 2,
    "tag_name": "work",
    "description": "Work-related captures",
    "color": "#0000FF",
    "created_at": "2025-12-02T09:00:00Z"
  }
]
```

#### Example

```bash
# Get all tags
curl "http://localhost:3131/tags"

# Get tags with pagination
curl "http://localhost:3131/tags?limit=10&offset=20"
```

---

### POST /tags

Create a new tag for organizing captured frames.

#### Request Body

```json
{
  "tag_name": "important",
  "description": "Important screens to review",
  "color": "#FF0000"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `tag_name` | string | Yes | Unique name for the tag |
| `description` | string | No | Optional description |
| `color` | string | No | Hex color code (e.g., "#FF0000") |

#### Response

```json
{
  "id": 1,
  "tag_name": "important",
  "description": "Important screens to review",
  "color": "#FF0000",
  "created_at": "2025-12-10T10:00:00Z"
}
```

#### Example

```bash
curl -X POST "http://localhost:3131/tags" \
  -H "Content-Type: application/json" \
  -d '{"tag_name": "urgent", "description": "Urgent items", "color": "#FF6600"}'
```

---

### DELETE /tags/:id

Delete a tag by ID. This removes the tag and all associations with frames.

#### Path Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `id` | integer | Tag ID to delete |

#### Response

```json
{
  "success": true,
  "message": "Tag deleted successfully"
}
```

#### Example

```bash
curl -X DELETE "http://localhost:3131/tags/1"
```

---

### GET /frames/:id/tags

Get all tags associated with a specific frame.

#### Path Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `id` | integer | Frame ID |

#### Response

```json
[
  {
    "id": 1,
    "tag_name": "important",
    "description": "Important screens to review",
    "color": "#FF0000"
  },
  {
    "id": 2,
    "tag_name": "work",
    "description": "Work-related captures",
    "color": "#0000FF"
  }
]
```

#### Example

```bash
curl "http://localhost:3131/frames/123/tags"
```

---

### POST /frames/:id/tags

Add a tag to a frame.

#### Path Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `id` | integer | Frame ID |

#### Request Body

```json
{
  "tag_id": 1
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `tag_id` | integer | Yes | ID of tag to add to frame |

#### Response

```json
{
  "success": true,
  "message": "Tag added to frame"
}
```

#### Example

```bash
curl -X POST "http://localhost:3131/frames/123/tags" \
  -H "Content-Type: application/json" \
  -d '{"tag_id": 1}'
```

---

### DELETE /frames/:id/tags/:tag_id

Remove a tag from a frame.

#### Path Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `id` | integer | Frame ID |
| `tag_id` | integer | Tag ID to remove |

#### Response

```json
{
  "success": true,
  "message": "Tag removed from frame"
}
```

#### Example

```bash
curl -X DELETE "http://localhost:3131/frames/123/tags/1"
```

---

## Error Handling

### Error Response Format

All errors return a consistent JSON structure with appropriate HTTP status codes:

```json
{
  "error": "Detailed error message",
  "status": 400
}
```

### Common Error Scenarios

#### 400 Bad Request

Returned when request parameters are invalid or malformed.

```json
{
  "error": "Missing required parameter: q",
  "status": 400
}
```

**Common causes:**
- Missing required parameters
- Invalid JSON in request body
- Malformed date/time formats
- Invalid selector syntax

#### 404 Not Found

Returned when a requested resource doesn't exist.

```json
{
  "error": "Frame with id 999 not found",
  "status": 404
}
```

**Common causes:**
- Non-existent frame ID
- Non-existent tag ID
- Invalid endpoint path

#### 500 Internal Server Error

Returned when the server encounters an unexpected error.

```json
{
  "error": "Database connection failed",
  "status": 500
}
```

**Common causes:**
- Database connectivity issues
- Automation API failures
- File system errors
- Internal server bugs

---

## Complete Examples

### Search and Tag Workflow

```bash
# 1. Search for content
RESULTS=$(curl -s "http://localhost:3131/search?q=important+document&limit=1")
FRAME_ID=$(echo $RESULTS | jq -r '.[0].frame.id')

# 2. Create a tag
TAG=$(curl -s -X POST "http://localhost:3131/tags" \
  -H "Content-Type: application/json" \
  -d '{"tag_name": "review", "color": "#FF9900"}')
TAG_ID=$(echo $TAG | jq -r '.id')

# 3. Add tag to frame
curl -X POST "http://localhost:3131/frames/$FRAME_ID/tags" \
  -H "Content-Type: application/json" \
  -d "{\"tag_id\": $TAG_ID}"

# 4. Verify tags on frame
curl "http://localhost:3131/frames/$FRAME_ID/tags"
```

### Automation Workflow

```bash
# 1. Open application
curl -X POST "http://localhost:3131/automation/open-app" \
  -H "Content-Type: application/json" \
  -d '{"app_name": "notepad.exe"}'

# 2. Wait for window to appear (add delay in script)
sleep 2

# 3. Find text input element
curl -X POST "http://localhost:3131/automation/find-elements" \
  -H "Content-Type: application/json" \
  -d '{"selector": "Edit[@Name=\"Text Editor\"]"}'

# 4. Click on text area
curl -X POST "http://localhost:3131/automation/click" \
  -H "Content-Type: application/json" \
  -d '{"x": 400, "y": 300}'

# 5. Type text
curl -X POST "http://localhost:3131/automation/type" \
  -H "Content-Type: application/json" \
  -d '{"text": "Hello from ScreenSearch API!"}'

# 6. Save file (Ctrl+S)
curl -X POST "http://localhost:3131/automation/press-key" \
  -H "Content-Type: application/json" \
  -d '{"key": "s", "modifiers": ["ctrl"]}'
```

### Advanced Search with Multiple Filters

```bash
# Search for "error" in Chrome during specific time window
curl -G "http://localhost:3131/search" \
  --data-urlencode "q=error OR exception" \
  --data-urlencode "app=Chrome" \
  --data-urlencode "start_time=2025-12-10T09:00:00Z" \
  --data-urlencode "end_time=2025-12-10T17:00:00Z" \
  --data-urlencode "limit=50"
```

---

## Performance Targets

The API is designed to meet these performance benchmarks:

| Operation | Target | Description |
|-----------|--------|-------------|
| Search response time | < 100ms | 95th percentile for full-text search |
| Frame retrieval | < 50ms | 95th percentile for frame queries |
| Automation actions | < 200ms | 95th percentile for UI automation |
| Health check | < 10ms | Health endpoint response time |
| Concurrent connections | 50+ | Simultaneous API connections supported |

---

## Configuration

### Environment Variables

Configure the API server using these environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `screen_api=debug` | Logging level |
| `SCREEN_DB_PATH` | `screensearch.db` | Path to SQLite database |
| `API_HOST` | `127.0.0.1` | API server bind address |
| `API_PORT` | `3131` | API server port |

### Example Configuration

```bash
# Windows PowerShell
$env:RUST_LOG="screen_api=info"
$env:SCREEN_DB_PATH="C:\Users\user\screensearch.db"
cargo run --release

# Windows CMD
set RUST_LOG=screen_api=info
set SCREEN_DB_PATH=C:\Users\user\screensearch.db
cargo run --release
```

---

## Security Considerations

### Local-Only Access

The API binds to `127.0.0.1` by default and is designed for local use only. Do not expose this API to external networks without proper authentication and encryption.

### Automation Risks

The automation endpoints have full desktop access and can control any application. Use these endpoints carefully:

- Validate all automation requests
- Implement rate limiting for automation endpoints
- Monitor automation actions for unexpected behavior
- Consider application exclusion lists for sensitive apps

### Data Privacy

All captured screen content is stored locally. The API does not:
- Transmit data to external services
- Include built-in authentication (add your own if needed)
- Log sensitive captured content

---

## Version History

### v0.1.0 (Current)

Initial release with core functionality:
- Full-text search with FTS5
- Frame retrieval and filtering
- Computer automation via Windows UIAutomation
- Tag management system
- Health monitoring

---

## Support and Resources

### Documentation

- Project README: `\path\to\app\ScreenSearch\README_FULL.md`
- Architecture Guide: `\path\to\app\ScreenSearch\CLAUDE.md`
- API Server README: `\path\to\app\ScreenSearch\screen-api\README.md`

### Development

- Source Code: `\path\to\app\ScreenSearch\`
- API Routes: `screen-api\src\routes.rs`
- Request Models: `screen-api\src\models.rs`
- Handler Implementation: `screen-api\src\handlers\`

### Testing

Run API tests:
```bash
# Unit tests
cargo test --lib -p screen-api

# Integration tests
cargo test --test integration_tests -- --ignored

# All tests
cargo test -p screen-api
```

---

**ScreenSearch API** - Locally stored, searchable screen capture with powerful automation.
