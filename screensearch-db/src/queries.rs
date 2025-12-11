//! Database query implementations
//!
//! Implements all database operations including insert, search, filter, and delete
//! operations. Uses parameterized queries to prevent SQL injection.

use crate::models::*;
use crate::{DatabaseManager, Result};
use chrono::{DateTime, Utc};
use sqlx::Row;

impl DatabaseManager {
    // ===== Video Chunk Operations =====

    /// Insert a new video chunk record
    pub async fn insert_video_chunk(&self, chunk: NewVideoChunk) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO video_chunks (device_name, file_path, start_time, end_time, duration_ms, width, height, fps)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(chunk.device_name)
        .bind(chunk.file_path)
        .bind(chunk.start_time)
        .bind(chunk.end_time)
        .bind(chunk.duration_ms)
        .bind(chunk.width)
        .bind(chunk.height)
        .bind(chunk.fps)
        .execute(self.pool())
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get a video chunk by ID
    pub async fn get_video_chunk(&self, id: i64) -> Result<Option<VideoChunkRecord>> {
        let chunk = sqlx::query_as::<_, VideoChunkRecord>(
            r#"
            SELECT id, device_name, file_path, start_time, end_time, duration_ms, width, height, fps, created_at
            FROM video_chunks
            WHERE id = ?
            "#
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;

        Ok(chunk)
    }

    // ===== Frame Operations =====

    /// Insert a new frame record
    pub async fn insert_frame(&self, frame: NewFrame) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO frames (
                chunk_id, timestamp, monitor_index, device_name, file_path,
                active_window, active_process, browser_url, width, height,
                offset_index, focused
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(frame.chunk_id)
        .bind(frame.timestamp)
        .bind(frame.monitor_index)
        .bind(frame.device_name)
        .bind(frame.file_path)
        .bind(frame.active_window)
        .bind(frame.active_process)
        .bind(frame.browser_url)
        .bind(frame.width)
        .bind(frame.height)
        .bind(frame.offset_index)
        .bind(frame.focused)
        .execute(self.pool())
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get a frame by ID with all metadata
    pub async fn get_frame(&self, id: i64) -> Result<Option<FrameRecord>> {
        let frame = sqlx::query_as::<_, FrameRecord>(
            r#"
            SELECT id, chunk_id, timestamp, monitor_index, device_name, file_path,
                   active_window, active_process, browser_url, width, height,
                   offset_index, focused, created_at
            FROM frames
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;

        Ok(frame)
    }

    /// Get frames within a time range with optional filters
    pub async fn get_frames_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        filter: FrameFilter,
        pagination: Pagination,
    ) -> Result<Vec<FrameRecord>> {
        let mut query = String::from(
            r#"
            SELECT id, chunk_id, timestamp, monitor_index, device_name, file_path,
                   active_window, active_process, browser_url, width, height,
                   offset_index, focused, created_at
            FROM frames
            WHERE timestamp >= ? AND timestamp <= ?
            "#,
        );

        if let Some(_app) = &filter.app_name {
            query.push_str(" AND active_process = ?");
        }
        if let Some(_device) = &filter.device_name {
            query.push_str(" AND device_name = ?");
        }
        if let Some(_monitor) = filter.monitor_index {
            query.push_str(" AND monitor_index = ?");
        }

        query.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");

        let mut query_builder = sqlx::query_as::<_, FrameRecord>(&query)
            .bind(start)
            .bind(end);

        if let Some(app) = &filter.app_name {
            query_builder = query_builder.bind(app);
        }
        if let Some(device) = &filter.device_name {
            query_builder = query_builder.bind(device);
        }
        if let Some(monitor) = filter.monitor_index {
            query_builder = query_builder.bind(monitor);
        }

        let frames = query_builder
            .bind(pagination.limit)
            .bind(pagination.offset)
            .fetch_all(self.pool())
            .await?;

        Ok(frames)
    }

    /// Get frames with tags
    pub async fn get_frames_with_tags(&self, frame_ids: Vec<i64>) -> Result<Vec<FrameWithTags>> {
        if frame_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();

        for frame_id in frame_ids {
            if let Some(frame) = self.get_frame(frame_id).await? {
                let tags = self.get_tags_for_frame(frame_id).await?;
                results.push(FrameWithTags { frame, tags });
            }
        }

        Ok(results)
    }

    /// Delete frames older than specified timestamp
    pub async fn delete_old_frames(&self, before: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query("DELETE FROM frames WHERE timestamp < ?")
            .bind(before)
            .execute(self.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Get frame count within a time range
    pub async fn count_frames_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM frames WHERE timestamp >= ? AND timestamp <= ?",
        )
        .bind(start)
        .bind(end)
        .fetch_one(self.pool())
        .await?;

        Ok(count)
    }

    // ===== OCR Text Operations =====

    /// Insert OCR text result for a frame
    pub async fn insert_ocr_text(&self, ocr: NewOcrText) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO ocr_text (frame_id, text, text_json, x, y, width, height, confidence)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(ocr.frame_id)
        .bind(ocr.text)
        .bind(ocr.text_json)
        .bind(ocr.x)
        .bind(ocr.y)
        .bind(ocr.width)
        .bind(ocr.height)
        .bind(ocr.confidence)
        .execute(self.pool())
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get all OCR text for a frame
    pub async fn get_ocr_text_for_frame(&self, frame_id: i64) -> Result<Vec<OcrTextRecord>> {
        let ocr_texts = sqlx::query_as::<_, OcrTextRecord>(
            r#"
            SELECT id, frame_id, text, text_json, x, y, width, height, confidence, created_at
            FROM ocr_text
            WHERE frame_id = ?
            ORDER BY y ASC, x ASC
            "#,
        )
        .bind(frame_id)
        .fetch_all(self.pool())
        .await?;

        Ok(ocr_texts)
    }

    /// Get OCR text by ID
    pub async fn get_ocr_text(&self, id: i64) -> Result<Option<OcrTextRecord>> {
        let ocr = sqlx::query_as::<_, OcrTextRecord>(
            r#"
            SELECT id, frame_id, text, text_json, x, y, width, height, confidence, created_at
            FROM ocr_text
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;

        Ok(ocr)
    }

    // ===== Full-Text Search Operations =====

    /// Search OCR text using FTS5 with BM25 ranking
    ///
    /// Uses FTS5 virtual table for efficient full-text search with Porter stemming
    /// and BM25 relevance ranking.
    pub async fn search_ocr_text(
        &self,
        query: &str,
        filter: FrameFilter,
        pagination: Pagination,
    ) -> Result<Vec<SearchResult>> {
        let mut sql = String::from(
            r#"
            SELECT
                f.id, f.chunk_id, f.timestamp, f.monitor_index, f.device_name,
                f.file_path, f.active_window, f.active_process, f.browser_url,
                f.width, f.height, f.offset_index, f.focused, f.created_at,
                o.id, o.frame_id, o.text, o.text_json, o.x, o.y, o.width, o.height,
                o.confidence, o.created_at,
                ocr_text_fts.rank
            FROM ocr_text_fts
            JOIN ocr_text o ON ocr_text_fts.rowid = o.id
            JOIN frames f ON o.frame_id = f.id
            WHERE ocr_text_fts MATCH ?
            "#,
        );

        // Add optional filters
        if let Some(start) = filter.start_time {
            let _ = (start,); // Use filter to avoid unused variable warnings
            sql.push_str(" AND f.timestamp >= ?");
        }
        if let Some(end) = filter.end_time {
            let _ = (end,);
            sql.push_str(" AND f.timestamp <= ?");
        }
        if let Some(app) = &filter.app_name {
            let _ = app;
            sql.push_str(" AND f.active_process = ?");
        }
        if let Some(device) = &filter.device_name {
            let _ = device;
            sql.push_str(" AND f.device_name = ?");
        }

        sql.push_str(" ORDER BY ocr_text_fts.rank ASC LIMIT ? OFFSET ?");

        let mut query_builder = sqlx::query(&sql).bind(query);

        if let Some(start) = filter.start_time {
            query_builder = query_builder.bind(start);
        }
        if let Some(end) = filter.end_time {
            query_builder = query_builder.bind(end);
        }
        if let Some(app) = &filter.app_name {
            query_builder = query_builder.bind(app);
        }
        if let Some(device) = &filter.device_name {
            query_builder = query_builder.bind(device);
        }

        let rows = query_builder
            .bind(pagination.limit)
            .bind(pagination.offset)
            .fetch_all(self.pool())
            .await?;

        let mut results: std::collections::HashMap<i64, SearchResult> =
            std::collections::HashMap::new();

        for row in rows {
            let frame = FrameRecord {
                id: row.get("id"),
                chunk_id: row.get("chunk_id"),
                timestamp: row.get("timestamp"),
                monitor_index: row.get("monitor_index"),
                device_name: row.get("device_name"),
                file_path: row.get("file_path"),
                active_window: row.get("active_window"),
                active_process: row.get("active_process"),
                browser_url: row.get("browser_url"),
                width: row.get("width"),
                height: row.get("height"),
                offset_index: row.get("offset_index"),
                focused: row.get("focused"),
                created_at: row.get::<DateTime<Utc>, _>("created_at"),
            };

            let ocr = OcrTextRecord {
                id: row.get::<i64, _>("id"),
                frame_id: row.get("frame_id"),
                text: row.get("text"),
                text_json: row.get("text_json"),
                x: row.get::<i32, _>("x"),
                y: row.get::<i32, _>("y"),
                width: row.get::<i32, _>("width"),
                height: row.get::<i32, _>("height"),
                confidence: row.get("confidence"),
                created_at: row.get::<DateTime<Utc>, _>("created_at"),
            };

            let rank: f32 = row.get("rank");
            let relevance_score = -rank; // BM25 rank is negative, invert for score

            results
                .entry(frame.id)
                .or_insert_with(|| SearchResult {
                    frame: frame.clone(),
                    ocr_matches: Vec::new(),
                    relevance_score,
                    tags: Vec::new(),
                })
                .ocr_matches
                .push(ocr);
        }

        let mut search_results: Vec<SearchResult> = results.into_values().collect();
        search_results.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());

        Ok(search_results)
    }

    /// Search OCR text by exact keywords
    pub async fn search_ocr_keywords(
        &self,
        keywords: Vec<String>,
        pagination: Pagination,
    ) -> Result<Vec<OcrTextRecord>> {
        if keywords.is_empty() {
            return Ok(Vec::new());
        }

        let mut query = String::from(
            r#"
            SELECT id, frame_id, text, text_json, x, y, width, height, confidence, created_at
            FROM ocr_text
            WHERE 1=1
            "#,
        );

        for _ in &keywords {
            query.push_str(" AND (text LIKE ? OR text_json LIKE ?)");
        }

        query.push_str(" ORDER BY confidence DESC LIMIT ? OFFSET ?");

        let mut query_builder = sqlx::query_as::<_, OcrTextRecord>(&query);

        // Build patterns with lifetime that extends through binding
        let patterns: Vec<String> = keywords.iter().map(|k| format!("%{}%", k)).collect();

        for pattern in &patterns {
            query_builder = query_builder.bind(pattern).bind(pattern);
        }

        let results = query_builder
            .bind(pagination.limit)
            .bind(pagination.offset)
            .fetch_all(self.pool())
            .await?;

        Ok(results)
    }

    // ===== Tag Operations =====

    /// Create a new tag
    pub async fn create_tag(&self, tag: NewTag) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO tags (tag_name, description, color)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(tag.tag_name)
        .bind(tag.description)
        .bind(tag.color)
        .execute(self.pool())
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get a tag by ID
    pub async fn get_tag(&self, id: i64) -> Result<Option<TagRecord>> {
        let tag = sqlx::query_as::<_, TagRecord>(
            "SELECT id, tag_name, description, color, created_at FROM tags WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;

        Ok(tag)
    }

    /// Get a tag by name
    pub async fn get_tag_by_name(&self, name: &str) -> Result<Option<TagRecord>> {
        let tag = sqlx::query_as::<_, TagRecord>(
            "SELECT id, tag_name, description, color, created_at FROM tags WHERE tag_name = ?",
        )
        .bind(name)
        .fetch_optional(self.pool())
        .await?;

        Ok(tag)
    }

    /// List all tags
    pub async fn list_tags(&self, pagination: Pagination) -> Result<Vec<TagRecord>> {
        let tags = sqlx::query_as::<_, TagRecord>(
            r#"
            SELECT id, tag_name, description, color, created_at
            FROM tags
            ORDER BY tag_name
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(self.pool())
        .await?;

        Ok(tags)
    }

    /// Add a tag to a frame
    pub async fn add_tag_to_frame(&self, frame_id: i64, tag_id: i64) -> Result<i64> {
        let result = sqlx::query("INSERT INTO frame_tags (frame_id, tag_id) VALUES (?, ?)")
            .bind(frame_id)
            .bind(tag_id)
            .execute(self.pool())
            .await?;

        Ok(result.last_insert_rowid())
    }

    /// Remove a tag from a frame
    pub async fn remove_tag_from_frame(&self, frame_id: i64, tag_id: i64) -> Result<u64> {
        let result = sqlx::query("DELETE FROM frame_tags WHERE frame_id = ? AND tag_id = ?")
            .bind(frame_id)
            .bind(tag_id)
            .execute(self.pool())
            .await?;

        Ok(result.rows_affected())
    }

    /// Get tags for a frame
    pub async fn get_tags_for_frame(&self, frame_id: i64) -> Result<Vec<TagRecord>> {
        let tags = sqlx::query_as::<_, TagRecord>(
            r#"
            SELECT t.id, t.tag_name, t.description, t.color, t.created_at
            FROM tags t
            JOIN frame_tags ft ON t.id = ft.tag_id
            WHERE ft.frame_id = ?
            ORDER BY t.tag_name
            "#,
        )
        .bind(frame_id)
        .fetch_all(self.pool())
        .await?;

        Ok(tags)
    }

    /// Get tags for multiple frames in a single query (bulk optimization)
    ///
    /// This method efficiently loads tags for multiple frames using a single JOIN query,
    /// avoiding the N+1 query problem. Returns a HashMap mapping frame_id to Vec<TagRecord>.
    pub async fn get_tags_for_frames(
        &self,
        frame_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, Vec<TagRecord>>> {
        use std::collections::HashMap;

        if frame_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Build parameterized IN clause
        let placeholders = frame_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

        let query_str = format!(
            r#"
            SELECT ft.frame_id, t.id, t.tag_name, t.description, t.color, t.created_at
            FROM frame_tags ft
            JOIN tags t ON ft.tag_id = t.id
            WHERE ft.frame_id IN ({})
            ORDER BY ft.frame_id, t.tag_name
            "#,
            placeholders
        );

        // Execute query with all frame IDs bound
        let mut query = sqlx::query(&query_str);

        for &id in frame_ids {
            query = query.bind(id);
        }

        let rows = query.fetch_all(self.pool()).await?;

        // Group tags by frame_id
        let mut result: HashMap<i64, Vec<TagRecord>> = HashMap::new();
        for row in rows {
            let frame_id: i64 = row.try_get("frame_id")?;
            let tag_id: i64 = row.try_get("id")?;
            let tag_name: String = row.try_get("tag_name")?;
            let description: Option<String> = row.try_get("description")?;
            let color: Option<String> = row.try_get("color")?;
            let created_at_str: String = row.try_get("created_at")?;
            let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now());

            result.entry(frame_id).or_default().push(TagRecord {
                id: tag_id,
                tag_name,
                description,
                color,
                created_at,
            });
        }

        Ok(result)
    }

    /// Get frames by tag
    pub async fn get_frames_by_tag(
        &self,
        tag_id: i64,
        pagination: Pagination,
    ) -> Result<Vec<FrameRecord>> {
        let frames = sqlx::query_as::<_, FrameRecord>(
            r#"
            SELECT f.id, f.chunk_id, f.timestamp, f.monitor_index, f.device_name,
                   f.file_path, f.active_window, f.active_process, f.browser_url,
                   f.width, f.height, f.offset_index, f.focused, f.created_at
            FROM frames f
            JOIN frame_tags ft ON f.id = ft.frame_id
            WHERE ft.tag_id = ?
            ORDER BY f.timestamp DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(tag_id)
        .bind(pagination.limit)
        .bind(pagination.offset)
        .fetch_all(self.pool())
        .await?;

        Ok(frames)
    }

    /// Update a tag
    pub async fn update_tag(&self, id: i64, tag: NewTag) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE tags
            SET tag_name = ?, description = ?, color = ?
            WHERE id = ?
            "#,
        )
        .bind(tag.tag_name)
        .bind(tag.description)
        .bind(tag.color)
        .bind(id)
        .execute(self.pool())
        .await?;

        Ok(result.rows_affected())
    }

    /// Delete a tag
    pub async fn delete_tag(&self, id: i64) -> Result<u64> {
        let result = sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;

        Ok(result.rows_affected())
    }

    // ===== Settings Operations =====

    /// Get application settings (singleton record with id=1)
    pub async fn get_settings(&self) -> Result<SettingsRecord> {
        let settings = sqlx::query_as::<_, SettingsRecord>(
            r#"
            SELECT id, capture_interval, monitors, excluded_apps, is_paused,
                   retention_days, updated_at
            FROM settings
            WHERE id = 1
            "#,
        )
        .fetch_one(self.pool())
        .await?;

        Ok(settings)
    }

    /// Update application settings
    pub async fn update_settings(&self, settings: UpdateSettings) -> Result<SettingsRecord> {
        sqlx::query(
            r#"
            UPDATE settings
            SET capture_interval = ?,
                monitors = ?,
                excluded_apps = ?,
                is_paused = ?,
                retention_days = ?,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = 1
            "#,
        )
        .bind(settings.capture_interval)
        .bind(settings.monitors)
        .bind(settings.excluded_apps)
        .bind(settings.is_paused)
        .bind(settings.retention_days)
        .execute(self.pool())
        .await?;

        // Return the updated settings
        self.get_settings().await
    }

    // ===== Statistics and Metadata Operations =====

    /// Get database statistics
    pub async fn get_statistics(&self) -> Result<DatabaseStatistics> {
        let frame_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM frames")
            .fetch_one(self.pool())
            .await?;

        let ocr_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ocr_text")
            .fetch_one(self.pool())
            .await?;

        let tag_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tags")
            .fetch_one(self.pool())
            .await?;

        let oldest_frame =
            sqlx::query_scalar::<_, Option<DateTime<Utc>>>("SELECT MIN(timestamp) FROM frames")
                .fetch_one(self.pool())
                .await?;

        let newest_frame =
            sqlx::query_scalar::<_, Option<DateTime<Utc>>>("SELECT MAX(timestamp) FROM frames")
                .fetch_one(self.pool())
                .await?;

        Ok(DatabaseStatistics {
            frame_count,
            ocr_count,
            tag_count,
            oldest_frame,
            newest_frame,
        })
    }

    /// Store metadata value
    pub async fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(self.pool())
            .await?;

        Ok(())
    }

    /// Get metadata value
    pub async fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        let value = sqlx::query_scalar::<_, String>("SELECT value FROM metadata WHERE key = ?")
            .bind(key)
            .fetch_optional(self.pool())
            .await?;

        Ok(value)
    }

    /// Clean up old data
    pub async fn cleanup_old_data(&self, days_to_keep: i32) -> Result<u64> {
        let cutoff_date = Utc::now() - chrono::Duration::days(days_to_keep as i64);

        let deleted = self.delete_old_frames(cutoff_date).await?;

        tracing::info!(
            "Cleaned up {} old frames (older than {} days)",
            deleted,
            days_to_keep
        );

        Ok(deleted)
    }
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStatistics {
    pub frame_count: i64,
    pub ocr_count: i64,
    pub tag_count: i64,
    pub oldest_frame: Option<DateTime<Utc>>,
    pub newest_frame: Option<DateTime<Utc>>,
}
