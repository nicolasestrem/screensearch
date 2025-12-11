# ScreenSearch Database Design

## Overview

The ScreenSearch database layer provides a high-performance, type-safe SQLite backend for storing and querying screen capture data, OCR results, and user annotations. It uses SQLx for compile-time query verification and implements FTS5 (Full-Text Search) for efficient text searching.

## Architecture

### Connection Management

- **Connection Pooling**: 50 max connections, 3 minimum idle connections
- **WAL Mode**: Enables concurrent read/write access
- **Performance Tuning**:
  - Cache size: 2MB (configurable)
  - Temporary tables in memory
  - Normal synchronous mode for balance between safety and performance

### Query Safety

All queries use parameterized statements to prevent SQL injection:

```rust
sqlx::query("SELECT * FROM frames WHERE id = ?")
    .bind(frame_id)
    .execute(pool)
    .await?
```

## Database Schema

### Tables

#### 1. video_chunks
Stores metadata about video file segments.

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
```

**Indexes**:
- `idx_video_chunks_device`: Fast lookup by device
- `idx_video_chunks_start_time`: Time-based queries
- `idx_video_chunks_time_range`: Range queries

#### 2. frames
Captures metadata for each screenshot including application context.

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
```

**Indexes**:
- `idx_frames_timestamp`: Timestamp ordering and range queries
- `idx_frames_device_time`: Device + timestamp composite for efficient filtering
- `idx_frames_process`: Application process filtering
- `idx_frames_url`: Browser URL filtering
- `idx_frames_window`: Window title filtering

#### 3. ocr_text
Stores OCR-extracted text with precise bounding box coordinates.

```sql
CREATE TABLE ocr_text (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id INTEGER NOT NULL,
    text TEXT NOT NULL,
    text_json TEXT,
    x INTEGER NOT NULL,
    y INTEGER NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (frame_id) REFERENCES frames(id) ON DELETE CASCADE
);
```

**Indexes**:
- `idx_ocr_frame_id`: Fast frame-to-ocr lookup
- `idx_ocr_confidence`: Confidence-based filtering

#### 4. ocr_text_fts
FTS5 virtual table for full-text search with BM25 ranking.

```sql
CREATE VIRTUAL TABLE ocr_text_fts USING fts5(
    text,
    content='ocr_text',
    content_rowid='id',
    tokenize = 'porter'
);
```

**Features**:
- Porter stemming for fuzzy matching ("running" matches "run")
- BM25 algorithm for relevance ranking
- Automatic synchronization with triggers

#### 5. tags
User-defined categories for annotating frames.

```sql
CREATE TABLE tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tag_name TEXT NOT NULL UNIQUE,
    description TEXT,
    color TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

#### 6. frame_tags
Junction table for many-to-many relationship between frames and tags.

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
```

#### 7. metadata
Key-value store for application configuration and statistics.

```sql
CREATE TABLE metadata (
    key TEXT PRIMARY KEY,
    value TEXT,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

## Data Models

### Input Models

- **NewFrame**: Insert frame capture metadata
- **NewOcrText**: Insert OCR extraction with bounding box
- **NewTag**: Create user annotation category
- **NewVideoChunk**: Record video file segment

### Query Result Models

- **FrameRecord**: Complete frame with all metadata
- **OcrTextRecord**: OCR text with location and confidence
- **SearchResult**: Frame with matched OCR results and relevance score
- **TagRecord**: Tag with description and styling
- **FrameWithTags**: Frame with associated tags
- **DatabaseStatistics**: Overall database metrics

### Filter & Pagination Models

```rust
pub struct FrameFilter {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub app_name: Option<String>,
    pub device_name: Option<String>,
    pub tag_ids: Option<Vec<i64>>,
    pub monitor_index: Option<i32>,
}

pub struct Pagination {
    pub limit: i64,
    pub offset: i64,
}
```

## Query Performance

### FTS5 Search Performance

The `search_ocr_text()` method provides efficient full-text search:

```rust
db.search_ocr_text(
    "database query",  // Search term
    FrameFilter {
        start_time: Some(Utc::now() - Duration::days(7)),
        end_time: Some(Utc::now()),
        app_name: Some("chrome".to_string()),
        ..Default::default()
    },
    Pagination { limit: 50, offset: 0 }
).await?
```

**Execution Plan**:
1. FTS5 indexes text using Porter stemming
2. BM25 ranking algorithm evaluates relevance
3. Results joined with frames for context
4. Filters applied for time/app/device constraints
5. Results paginated and sorted by relevance

**Target Response Time**: < 100ms for 100k+ frame databases

### Index Usage

| Query Type | Primary Index | Secondary Indexes |
|-----------|---------------|-------------------|
| Time range | idx_frames_timestamp | idx_frames_device_time |
| By app | idx_frames_process | idx_frames_timestamp |
| By device + time | idx_frames_device_time | idx_frames_timestamp |
| Full-text search | ocr_text_fts | idx_ocr_frame_id |
| By tag | idx_frame_tags_tag_id | idx_frame_tags_frame_id |
| By confidence | idx_ocr_confidence | idx_ocr_frame_id |

## API Overview

### Frame Operations

```rust
// Insert a frame
let frame_id = db.insert_frame(new_frame).await?;

// Retrieve frame
let frame = db.get_frame(frame_id).await?;

// Query frames in time range
let frames = db.get_frames_in_range(start, end, filter, pagination).await?;

// Get frame count
let count = db.count_frames_in_range(start, end).await?;

// Delete old frames
let deleted = db.delete_old_frames(cutoff_date).await?;
```

### OCR Operations

```rust
// Insert OCR result
let ocr_id = db.insert_ocr_text(new_ocr).await?;

// Get OCR for frame
let ocr_texts = db.get_ocr_text_for_frame(frame_id).await?;

// Full-text search
let results = db.search_ocr_text(query, filter, pagination).await?;

// Keyword search
let matches = db.search_ocr_keywords(keywords, pagination).await?;
```

### Tag Operations

```rust
// Create tag
let tag_id = db.create_tag(new_tag).await?;

// Add tag to frame
db.add_tag_to_frame(frame_id, tag_id).await?;

// Remove tag from frame
db.remove_tag_from_frame(frame_id, tag_id).await?;

// Get frame tags
let tags = db.get_tags_for_frame(frame_id).await?;

// Get frames with tag
let frames = db.get_frames_by_tag(tag_id, pagination).await?;
```

### Statistics & Metadata

```rust
// Get database statistics
let stats = db.get_statistics().await?;

// Store metadata
db.set_metadata("key", "value").await?;

// Retrieve metadata
let value = db.get_metadata("key").await?;

// Cleanup old data
let deleted = db.cleanup_old_data(days_to_keep).await?;
```

## Error Handling

Database operations return `screensearch_db::Result<T>` which wraps `DatabaseError`:

```rust
pub enum DatabaseError {
    InitializationError(String),
    MigrationError(String),
    QueryError(String),
    NotFound(String),
    InvalidParameter(String),
    SqlxError(sqlx::Error),
    IoError(std::io::Error),
}
```

All errors are logged via the `tracing` crate at appropriate levels (error, warn, info, debug).

## Migrations

Migrations are managed automatically and tracked in the `_migrations` table. Current schema is defined in:

- `src/migrations.rs`: Migration code
- `001_initial_schema`: Complete schema definition

To add a new migration:

1. Update `run_migrations()` function
2. Define new migration constant
3. Call `apply_migration()` with migration SQL
4. Migrations are idempotent and applied only once

## Testing

Comprehensive integration tests cover:

- Database initialization
- Frame insertion and retrieval
- OCR text indexing and storage
- FTS5 search functionality
- Tag creation and assignment
- Frame filtering by time, app, and device
- Pagination
- Database statistics
- Metadata storage
- Cleanup operations

Run tests with:

```bash
cargo test -p screensearch-db
```

## Performance Characteristics

### Storage

- **Frame Record**: ~200 bytes
- **OCR Text Record**: ~300 bytes (average)
- **Tag Record**: ~100 bytes
- **100,000 frames**: ~100-150 MB (database file)

### Query Performance (100k frames)

| Operation | Time | Notes |
|-----------|------|-------|
| Insert frame | 2-5ms | With transaction |
| Insert OCR | 1-3ms | Triggers FTS5 update |
| Time range query | 10-50ms | With indexes |
| Full-text search | 50-200ms | Depends on result size |
| Tag assignment | 1-2ms | Simple insert |
| Pagination (limit 100) | 10-30ms | With proper indexes |

### Memory Usage

- **Connection Pool**: ~10 MB (50 connections)
- **Cache Buffer**: 2 MB (configurable)
- **Query Overhead**: ~1 MB per active query

## Best Practices

1. **Use Filters**: Always provide time ranges and filters to narrow result sets
2. **Pagination**: Use pagination for large result sets (limit: 100-1000)
3. **Batch Inserts**: Group multiple OCR inserts in quick succession
4. **Cleanup**: Regularly cleanup old frames using `cleanup_old_data()`
5. **Connection Reuse**: Don't recreate DatabaseManager for each request
6. **Error Handling**: Always log and handle database errors appropriately
7. **Monitoring**: Use `get_statistics()` to monitor database growth

## Example Usage

```rust
use screensearch_db::{DatabaseManager, NewFrame, NewOcrText, Pagination, FrameFilter};
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize database
    let db = DatabaseManager::new("screensearch.db").await?;

    // Insert frame
    let frame = NewFrame {
        timestamp: Utc::now(),
        device_name: "monitor-1".to_string(),
        file_path: "/captures/frame-001.png".to_string(),
        active_process: Some("chrome".to_string()),
        // ... other fields
    };

    let frame_id = db.insert_frame(frame).await?;

    // Insert OCR text
    let ocr = NewOcrText {
        frame_id,
        text: "Hello World".to_string(),
        x: 100,
        y: 150,
        // ... other fields
    };

    db.insert_ocr_text(ocr).await?;

    // Search
    let mut filter = FrameFilter::default();
    filter.app_name = Some("chrome".to_string());

    let results = db.search_ocr_text(
        "hello",
        filter,
        Pagination { limit: 50, offset: 0 }
    ).await?;

    println!("Found {} results", results.len());

    // Get statistics
    let stats = db.get_statistics().await?;
    println!("Database has {} frames", stats.frame_count);

    db.close().await;
    Ok(())
}
```

## Future Enhancements

1. **Vector Embeddings**: Add semantic search using sqlite-vec
2. **Clustering**: Group similar frames using embeddings
3. **Analytics**: Built-in aggregation queries
4. **Compression**: Compressed storage for older data
5. **Replication**: Multi-device synchronization
6. **Sharding**: Partitioning for very large datasets
7. **Query Caching**: Redis integration for frequent queries
