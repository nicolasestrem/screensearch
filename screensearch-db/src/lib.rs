//! Database Module for ScreenSearch
//!
//! This crate provides SQLite database access for storing and querying screen captures
//! and OCR results. It uses sqlx for type-safe database operations with compile-time
//! query verification.
//!
//! # Architecture
//!
//! - `DatabaseManager`: Main interface for database operations
//! - Connection pooling with configurable limits
//! - WAL mode for concurrent read/write access
//! - FTS5 for full-text search on OCR content
//! - Automatic schema migrations
//!
//! # Example
//!
//! ```no_run
//! use screensearch_db::{DatabaseManager, NewFrame, Pagination, FrameFilter};
//! use chrono::Utc;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let db = DatabaseManager::new("screensearch.db").await?;
//!
//!     // Insert a frame
//!     let frame = NewFrame {
//!         timestamp: Utc::now(),
//!         device_name: "monitor-1".to_string(),
//!         file_path: "/tmp/frame.png".to_string(),
//!         monitor_index: 0,
//!         width: 1920,
//!         height: 1080,
//!         offset_index: 0,
//!         chunk_id: None,
//!         active_window: None,
//!         active_process: None,
//!         browser_url: None,
//!         focused: None,
//!     };
//!     let frame_id = db.insert_frame(frame).await?;
//!
//!     // Search OCR text
//!     let results = db.search_ocr_text(
//!         "hello",
//!         FrameFilter::default(),
//!         Pagination::default()
//!     ).await?;
//!
//!     Ok(())
//! }
//! ```

use thiserror::Error;

pub mod db;
pub mod migrations;
pub mod models;
pub mod queries;
pub mod vector_search;

pub use db::DatabaseManager;
pub use models::{
    EmbeddingRecord, EmbeddingStatus, FrameFilter, FrameRecord, FrameTagRecord, FrameWithTags,
    FtsOcrResult, HybridResult, NewEmbedding, NewFrame, NewOcrText, NewTag, NewVideoChunk,
    OcrTextRecord, Pagination, SearchResult, SemanticResult, SettingsRecord, TagRecord,
    UpdateSettings, VideoChunkRecord,
};
pub use queries::DatabaseStatistics;

/// Database-related errors
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Database initialization failed: {0}")]
    InitializationError(String),

    #[error("Migration failed: {0}")]
    MigrationError(String),

    #[error("Query failed: {0}")]
    QueryError(String),

    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("SQLx error: {0}")]
    SqlxError(#[from] sqlx::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type alias for database operations
pub type Result<T> = std::result::Result<T, DatabaseError>;

/// Database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Path to SQLite database file
    pub path: String,

    /// Maximum number of connections in pool
    pub max_connections: u32,

    /// Minimum number of connections in pool
    pub min_connections: u32,

    /// Connection acquire timeout in seconds
    pub acquire_timeout_secs: u64,

    /// Enable WAL mode
    pub enable_wal: bool,

    /// Cache size in KB (negative = KB of memory)
    pub cache_size_kb: i32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "screensearch.db".to_string(),
            max_connections: 50,
            min_connections: 3,
            acquire_timeout_secs: 10,
            enable_wal: true,
            cache_size_kb: -2000, // 2MB
        }
    }
}

impl DatabaseConfig {
    /// Create a new config with custom path
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DatabaseConfig::default();
        assert_eq!(config.path, "screensearch.db");
        assert_eq!(config.max_connections, 50);
        assert!(config.enable_wal);
    }

    #[test]
    fn test_custom_config() {
        let config = DatabaseConfig::new("custom.db");
        assert_eq!(config.path, "custom.db");
        assert_eq!(config.max_connections, 50);
    }
}
