//! Database models and types
//!
//! Defines Rust structs that map to database tables with proper serialization
//! and deserialization support.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Video chunk record - stores video file segments
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct VideoChunkRecord {
    pub id: i64,
    pub device_name: String,
    pub file_path: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_ms: i64,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub created_at: DateTime<Utc>,
}

/// Frame record - metadata for captured screenshot
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FrameRecord {
    pub id: i64,
    pub chunk_id: Option<i64>,
    pub timestamp: DateTime<Utc>,
    pub monitor_index: i32,
    pub device_name: String,
    pub file_path: String,
    pub active_window: Option<String>,
    pub active_process: Option<String>,
    pub browser_url: Option<String>,
    pub width: i32,
    pub height: i32,
    pub offset_index: i32,
    pub focused: Option<bool>,
    pub created_at: DateTime<Utc>,
}

/// OCR text record with precise bounding box coordinates
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OcrTextRecord {
    pub id: i64,
    pub frame_id: i64,
    pub text: String,
    pub text_json: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
}

/// Tag record - user-defined category/annotation
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TagRecord {
    pub id: i64,
    pub tag_name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Frame tag record - junction table entry
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FrameTagRecord {
    pub id: i64,
    pub frame_id: i64,
    pub tag_id: i64,
    pub created_at: DateTime<Utc>,
}

/// Settings record - application configuration (singleton)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SettingsRecord {
    pub id: i64,
    pub capture_interval: i64,
    pub monitors: String,      // JSON array
    pub excluded_apps: String, // JSON array
    pub is_paused: i64,        // SQLite boolean (0/1)
    pub retention_days: i64,
    pub updated_at: DateTime<Utc>,
}

/// Search result combining frame and OCR data with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub frame: FrameRecord,
    pub ocr_matches: Vec<OcrTextRecord>,
    pub relevance_score: f32,
    pub tags: Vec<String>,
}

/// Frame with associated tags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameWithTags {
    pub frame: FrameRecord,
    pub tags: Vec<TagRecord>,
}

/// OCR content from FTS5 search
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FtsOcrResult {
    pub id: i64,
    pub frame_id: i64,
    pub text: String,
    pub rank: f32,
}

// Input types for creating new records

/// New video chunk input
#[derive(Debug, Clone)]
pub struct NewVideoChunk {
    pub device_name: String,
    pub file_path: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_ms: i64,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
}

/// New frame input
#[derive(Debug, Clone)]
pub struct NewFrame {
    pub chunk_id: Option<i64>,
    pub timestamp: DateTime<Utc>,
    pub monitor_index: i32,
    pub device_name: String,
    pub file_path: String,
    pub active_window: Option<String>,
    pub active_process: Option<String>,
    pub browser_url: Option<String>,
    pub width: i32,
    pub height: i32,
    pub offset_index: i32,
    pub focused: Option<bool>,
}

/// New OCR text input
#[derive(Debug, Clone)]
pub struct NewOcrText {
    pub frame_id: i64,
    pub text: String,
    pub text_json: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub confidence: f32,
}

/// New tag input
#[derive(Debug, Clone)]
pub struct NewTag {
    pub tag_name: String,
    pub description: Option<String>,
    pub color: Option<String>,
}

/// Update settings input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSettings {
    pub capture_interval: i64,
    pub monitors: String,      // JSON array
    pub excluded_apps: String, // JSON array
    pub is_paused: i64,        // SQLite boolean (0/1)
    pub retention_days: i64,
}

/// Frame filter parameters for queries
#[derive(Debug, Clone, Default)]
pub struct FrameFilter {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub app_name: Option<String>,
    pub device_name: Option<String>,
    pub tag_ids: Option<Vec<i64>>,
    pub monitor_index: Option<i32>,
}

/// Pagination parameters
#[derive(Debug, Clone, Deserialize)]
pub struct Pagination {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    100
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            limit: 100,
            offset: 0,
        }
    }
}
