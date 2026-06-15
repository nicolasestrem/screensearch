//! Database migrations
//!
//! Manages application of SQL migrations to ensure schema consistency.
//! Migrations are applied in order and tracked in the _migrations table.

use crate::Result;
use sqlx::{Pool, Sqlite};

/// Run all database migrations
pub async fn run_migrations(pool: &Pool<Sqlite>) -> Result<()> {
    tracing::debug!("Initializing migrations table");

    // Create migrations table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create migrations table: {}", e);
        crate::DatabaseError::MigrationError(format!("Failed to create migrations table: {}", e))
    })?;

    // Apply migrations in order - ensure order is maintained for dependencies
    apply_migration(pool, "001_initial_schema", MIGRATION_001_INITIAL).await?;
    apply_migration(pool, "002_settings_table", MIGRATION_002_SETTINGS).await?;
    apply_migration(pool, "003_embeddings_table", MIGRATION_003_EMBEDDINGS).await?;
    apply_migration(
        pool,
        "004_add_embedding_column",
        MIGRATION_004_ADD_EMBEDDING_COLUMN,
    )
    .await?;
    apply_migration(pool, "005_vision_analysis", MIGRATION_005_VISION).await?;
    apply_migration(pool, "006_api_key", MIGRATION_006_API_KEY).await?;
    apply_migration(
        pool,
        "007_reset_vision_defaults",
        MIGRATION_007_RESET_VISION_DEFAULTS,
    )
    .await?;
    apply_migration(
        pool,
        "008_performance_indexes",
        MIGRATION_008_PERFORMANCE_INDEXES,
    )
    .await?;
    apply_migration(pool, "009_quality_rag", MIGRATION_009_QUALITY_RAG).await?;
    apply_migration(
        pool,
        "010_unique_embedding_chunks",
        MIGRATION_010_UNIQUE_EMBEDDING_CHUNKS,
    )
    .await?;

    tracing::info!("All migrations completed successfully");
    Ok(())
}

/// Apply a single migration if not already applied
async fn apply_migration(pool: &Pool<Sqlite>, name: &str, sql: &str) -> Result<()> {
    // Check if migration already applied
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _migrations WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check migration status for {}: {}", name, e);
            crate::DatabaseError::MigrationError(format!("Failed to check migration status: {}", e))
        })?;

    if exists == 0 {
        tracing::info!("Applying migration: {}", name);

        // For SQLite, execute the entire migration as one block
        // SQLite supports multiple statements when sent via query
        let mut conn = pool.acquire().await.map_err(|e| {
            tracing::error!("Failed to acquire connection for migration: {}", e);
            crate::DatabaseError::MigrationError(format!("Failed to acquire connection: {}", e))
        })?;

        // Execute the full SQL migration
        sqlx::raw_sql(sql).execute(&mut *conn).await.map_err(|e| {
            tracing::error!("Failed to execute migration {}: {}", name, e);
            crate::DatabaseError::MigrationError(format!(
                "Failed to execute migration {}: {}",
                name, e
            ))
        })?;

        // Record migration
        sqlx::query("INSERT INTO _migrations (name) VALUES (?)")
            .bind(name)
            .execute(pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to record migration {}: {}", name, e);
                crate::DatabaseError::MigrationError(format!("Failed to record migration: {}", e))
            })?;

        tracing::info!("Migration {} applied successfully", name);
    } else {
        tracing::debug!("Migration {} already applied, skipping", name);
    }

    Ok(())
}

/// Initial schema migration - creates all base tables, indexes, and FTS infrastructure
const MIGRATION_001_INITIAL: &str = r#"
-- Enable performance pragmas first
PRAGMA journal_mode = WAL;
PRAGMA cache_size = -2000;
PRAGMA temp_store = MEMORY;
PRAGMA synchronous = NORMAL;
PRAGMA query_only = FALSE;

-- Video chunks table: stores video file segments with metadata
CREATE TABLE IF NOT EXISTS video_chunks (
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

CREATE INDEX IF NOT EXISTS idx_video_chunks_device ON video_chunks(device_name);
CREATE INDEX IF NOT EXISTS idx_video_chunks_start_time ON video_chunks(start_time);
CREATE INDEX IF NOT EXISTS idx_video_chunks_time_range ON video_chunks(start_time, end_time);

-- Frames table: stores metadata for captured screenshots
CREATE TABLE IF NOT EXISTS frames (
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

CREATE INDEX IF NOT EXISTS idx_frames_timestamp ON frames(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_frames_device_time ON frames(device_name, timestamp);
CREATE INDEX IF NOT EXISTS idx_frames_process ON frames(active_process);
CREATE INDEX IF NOT EXISTS idx_frames_url ON frames(browser_url);
CREATE INDEX IF NOT EXISTS idx_frames_window ON frames(active_window);

-- OCR text table: stores extracted text with precise bounding box coordinates
CREATE TABLE IF NOT EXISTS ocr_text (
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

CREATE INDEX IF NOT EXISTS idx_ocr_frame_id ON ocr_text(frame_id);
CREATE INDEX IF NOT EXISTS idx_ocr_confidence ON ocr_text(confidence DESC);

-- FTS5 virtual table for full-text search with BM25 ranking
CREATE VIRTUAL TABLE IF NOT EXISTS ocr_text_fts USING fts5(
    text,
    content='ocr_text',
    content_rowid='id',
    tokenize = 'porter'
);

-- Triggers to keep FTS5 in sync with ocr_text table
CREATE TRIGGER IF NOT EXISTS ocr_text_ai AFTER INSERT ON ocr_text BEGIN
    INSERT INTO ocr_text_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TRIGGER IF NOT EXISTS ocr_text_ad AFTER DELETE ON ocr_text BEGIN
    DELETE FROM ocr_text_fts WHERE rowid = old.id;
END;

CREATE TRIGGER IF NOT EXISTS ocr_text_au AFTER UPDATE ON ocr_text BEGIN
    DELETE FROM ocr_text_fts WHERE rowid = old.id;
    INSERT INTO ocr_text_fts(rowid, text) VALUES (new.id, new.text);
END;

-- Tags table: user annotations and categorization of frames
CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tag_name TEXT NOT NULL UNIQUE,
    description TEXT,
    color TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(tag_name);

-- Frame tags junction table: many-to-many relationship
CREATE TABLE IF NOT EXISTS frame_tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id INTEGER NOT NULL,
    tag_id INTEGER NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(frame_id, tag_id),
    FOREIGN KEY (frame_id) REFERENCES frames(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_frame_tags_frame_id ON frame_tags(frame_id);
CREATE INDEX IF NOT EXISTS idx_frame_tags_tag_id ON frame_tags(tag_id);

-- Metadata table for storing application configuration and statistics
CREATE TABLE IF NOT EXISTS metadata (
    key TEXT PRIMARY KEY,
    value TEXT,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
"#;

/// Settings table migration - stores application configuration
const MIGRATION_002_SETTINGS: &str = r#"
-- Settings table: stores application settings (singleton with id=1)
CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    capture_interval INTEGER NOT NULL DEFAULT 5,
    monitors TEXT NOT NULL DEFAULT '[]',
    excluded_apps TEXT NOT NULL DEFAULT '[]',
    is_paused INTEGER NOT NULL DEFAULT 0,
    retention_days INTEGER NOT NULL DEFAULT 30,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Insert default settings row
INSERT OR IGNORE INTO settings (id, capture_interval, monitors, excluded_apps, is_paused, retention_days)
VALUES (1, 5, '[]', '["1Password", "KeePass", "Bitwarden"]', 0, 30);
"#;

/// Embeddings table migration - stores text chunks with embeddings for semantic search
/// Note: The vec0 virtual table is created separately via rusqlite with sqlite-vec extension
const MIGRATION_003_EMBEDDINGS: &str = r#"
-- Embeddings table: stores text chunks from OCR with vector embeddings
-- The actual vector data is stored in a vec0 virtual table created via rusqlite
CREATE TABLE IF NOT EXISTS embeddings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id INTEGER NOT NULL,
    chunk_text TEXT NOT NULL,           -- The text that was embedded
    chunk_index INTEGER NOT NULL,       -- Position in frame's OCR text (0-indexed)
    embedding_dim INTEGER NOT NULL DEFAULT 384, -- Dimension of the embedding vector
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (frame_id) REFERENCES frames(id) ON DELETE CASCADE
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_embeddings_frame_id ON embeddings(frame_id);
CREATE INDEX IF NOT EXISTS idx_embeddings_chunk ON embeddings(frame_id, chunk_index);
CREATE INDEX IF NOT EXISTS idx_embeddings_created_at ON embeddings(created_at DESC);

-- Metadata to track embedding generation status
INSERT OR IGNORE INTO metadata (key, value) VALUES ('embeddings_enabled', 'false');
INSERT OR IGNORE INTO metadata (key, value) VALUES ('embeddings_model', 'all-MiniLM-L6-v2');
INSERT OR IGNORE INTO metadata (key, value) VALUES ('embeddings_last_processed_frame_id', '0');
"#;

/// Migration 004 - Add embedding blob column for in-memory search
const MIGRATION_004_ADD_EMBEDDING_COLUMN: &str = r#"
-- Add embedding column for BLOB storage (replacing previous plan of using virtual table)
ALTER TABLE embeddings ADD COLUMN embedding BLOB;
-- Clear existing data to force re-processing with actual vectors
DELETE FROM embeddings;
"#;

/// Versioned embeddings and a persistent sqlite-vec index.
const MIGRATION_009_QUALITY_RAG: &str = r#"
ALTER TABLE embeddings ADD COLUMN provider TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE embeddings ADD COLUMN model TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE embeddings ADD COLUMN model_version TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE embeddings ADD COLUMN content_hash TEXT NOT NULL DEFAULT '';

-- Existing 384-dimensional vectors are incompatible with the fixed
-- Qwen3 1024-dimensional contract. Clear them and require a full reindex.
DELETE FROM embeddings;

CREATE VIRTUAL TABLE IF NOT EXISTS embedding_vectors USING vec0(
    embedding_id INTEGER PRIMARY KEY,
    embedding float[1024] distance_metric=cosine
);

CREATE TRIGGER IF NOT EXISTS embeddings_vector_delete
AFTER DELETE ON embeddings BEGIN
    DELETE FROM embedding_vectors WHERE embedding_id = old.id;
END;

INSERT OR REPLACE INTO metadata (key, value) VALUES
    ('embeddings_model', 'Qwen/Qwen3-Embedding-0.6B'),
    ('embeddings_provider', 'quality-sidecar'),
    ('embeddings_model_version', 'main'),
    ('embeddings_dimension', '1024'),
    ('embeddings_last_processed_frame_id', '0'),
    ('embeddings_reindex_required', 'true');
"#;

/// Prevent concurrent or repeated indexing from duplicating frame chunks.
const MIGRATION_010_UNIQUE_EMBEDDING_CHUNKS: &str = r#"
-- Keep the oldest row if an earlier build created duplicate frame chunks.
-- The delete trigger removes the corresponding sqlite-vec rows.
DELETE FROM embeddings
WHERE id NOT IN (
    SELECT MIN(id)
    FROM embeddings
    GROUP BY frame_id, chunk_index
);

DROP INDEX IF EXISTS idx_embeddings_chunk;
CREATE UNIQUE INDEX IF NOT EXISTS idx_embeddings_frame_chunk
ON embeddings(frame_id, chunk_index);
"#;

/// Migration 005 - Vision Analysis Support
const MIGRATION_005_VISION: &str = r#"
-- Add columns to frames table
ALTER TABLE frames ADD COLUMN analysis_status TEXT DEFAULT 'pending';
ALTER TABLE frames ADD COLUMN description TEXT;
ALTER TABLE frames ADD COLUMN visible_text_json TEXT;
ALTER TABLE frames ADD COLUMN activity_type TEXT;
ALTER TABLE frames ADD COLUMN app_hint TEXT;
ALTER TABLE frames ADD COLUMN confidence REAL;
ALTER TABLE frames ADD COLUMN analysis_time_ms INTEGER;
ALTER TABLE frames ADD COLUMN analysis_error TEXT;

CREATE INDEX IF NOT EXISTS idx_frames_analysis_status ON frames(analysis_status) WHERE analysis_status != 'completed';

-- Analysis Queue for prioritized processing
CREATE TABLE IF NOT EXISTS analysis_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
    priority INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    locked_until DATETIME,
    attempts INTEGER DEFAULT 0,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_analysis_queue_priority ON analysis_queue(priority DESC, created_at ASC);

-- FTS5 for descriptions
CREATE VIRTUAL TABLE IF NOT EXISTS frames_fts USING fts5(
    description,
    active_window,
    content='frames',
    content_rowid='id'
);

-- Triggers to keep frames_fts updated
CREATE TRIGGER IF NOT EXISTS frames_ai_insert AFTER INSERT ON frames BEGIN
  INSERT INTO frames_fts(rowid, description, active_window) 
  VALUES(new.id, new.description, new.active_window);
END;

CREATE TRIGGER IF NOT EXISTS frames_ai_delete AFTER DELETE ON frames BEGIN
  INSERT INTO frames_fts(frames_fts, rowid, description, active_window) 
  VALUES('delete', old.id, old.description, old.active_window);
END;

CREATE TRIGGER IF NOT EXISTS frames_ai_update AFTER UPDATE ON frames BEGIN
  INSERT INTO frames_fts(frames_fts, rowid, description, active_window) 
  VALUES('delete', old.id, old.description, old.active_window);
  
  INSERT INTO frames_fts(rowid, description, active_window) 
  VALUES(new.id, new.description, new.active_window);
END;

-- Add vision settings columns
-- Default to local provider (Ministral-3B) with vision disabled
-- Users must opt-in to enable AI features
ALTER TABLE settings ADD COLUMN vision_enabled INTEGER DEFAULT 0;
ALTER TABLE settings ADD COLUMN vision_provider TEXT DEFAULT 'local';
ALTER TABLE settings ADD COLUMN vision_model TEXT DEFAULT 'ministral-3:3b';
ALTER TABLE settings ADD COLUMN vision_endpoint TEXT DEFAULT 'http://127.0.0.1:31130';
"#;

/// Migration 006 - Add API Key for external providers
const MIGRATION_006_API_KEY: &str = r#"
ALTER TABLE settings ADD COLUMN vision_api_key TEXT;
"#;

/// Migration 007 - Reset vision defaults to local provider
/// This ensures existing databases don't auto-connect to external APIs
/// Users who previously configured external providers will need to re-enable
const MIGRATION_007_RESET_VISION_DEFAULTS: &str = r#"
-- Reset vision settings to use local provider by default
-- This migration ensures existing databases don't auto-connect to external APIs
-- User can still manually enable and configure external providers in settings
UPDATE settings SET
    vision_enabled = 0,
    vision_provider = 'local',
    vision_model = 'ministral-3:3b',
    vision_endpoint = 'http://127.0.0.1:31130'
WHERE id = 1;
"#;

/// Migration 008 - Performance Indexes
/// Adds missing indexes identified through query analysis for improved performance
const MIGRATION_008_PERFORMANCE_INDEXES: &str = r#"
-- Composite index for multi-monitor queries with time range
-- Optimizes queries filtering by device, monitor, and timestamp together
CREATE INDEX IF NOT EXISTS idx_frames_device_monitor_time
    ON frames(device_name, monitor_index, timestamp DESC);

-- Index on metadata key for fast settings lookups
-- The metadata table is used for embeddings status and other configs
CREATE INDEX IF NOT EXISTS idx_metadata_key ON metadata(key);

-- Composite index for analysis queue worker queries
-- Workers query: WHERE locked_until IS NULL OR locked_until < NOW ORDER BY priority DESC
CREATE INDEX IF NOT EXISTS idx_analysis_queue_locked_priority
    ON analysis_queue(locked_until, priority DESC);

-- Index for embeddings with timestamp from joined frames table
-- Used by semantic search when filtering by time range
CREATE INDEX IF NOT EXISTS idx_embeddings_frame_created
    ON embeddings(frame_id, created_at DESC);
"#;
