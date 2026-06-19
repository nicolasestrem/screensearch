//! Database query implementations
//!
//! Implements all database operations including insert, search, filter, and delete
//! operations. Uses parameterized queries to prevent SQL injection.

use crate::models::*;
use crate::{DatabaseManager, Result, EMBEDDING_DIM};
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
                   offset_index, focused, created_at,
                   analysis_status, description, visible_text_json, activity_type,
                   app_hint, confidence, analysis_time_ms, analysis_error
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
                   offset_index, focused, created_at,
                   analysis_status, description, visible_text_json, activity_type,
                   app_hint, confidence, analysis_time_ms, analysis_error
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
        // Escape the query for FTS5 - wrap in double quotes to treat as literal phrase
        // This prevents numbers and special chars from being misinterpreted
        let escaped_query = format!("\"{}\"", query.replace("\"", "\"\""));

        let mut sql = String::from(
            r#"
            SELECT
                f.id as frame_primary_id, f.chunk_id, f.timestamp, f.monitor_index, f.device_name,
                f.file_path, f.active_window, f.active_process, f.browser_url,
                f.width, f.height, f.offset_index, f.focused, f.created_at,
                f.analysis_status, f.description, f.visible_text_json, f.activity_type,
                f.app_hint, f.confidence, f.analysis_time_ms, f.analysis_error,
                o.id as ocr_primary_id, o.frame_id, o.text, o.text_json, o.x, o.y, o.width, o.height,
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

        let mut query_builder = sqlx::query(&sql).bind(&escaped_query);

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
                id: row.get("frame_primary_id"),
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
                analysis_status: row.try_get("analysis_status").ok(),
                description: row.try_get("description").ok(),
                visible_text_json: row.try_get("visible_text_json").ok(),
                activity_type: row.try_get("activity_type").ok(),
                app_hint: row.try_get("app_hint").ok(),
                confidence: row.try_get("confidence").ok(),
                analysis_time_ms: row.try_get("analysis_time_ms").ok(),
                analysis_error: row.try_get("analysis_error").ok(),
            };

            let ocr = OcrTextRecord {
                id: row.get::<i64, _>("ocr_primary_id"),
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
                   f.width, f.height, f.offset_index, f.focused, f.created_at,
                   f.analysis_status, f.description, f.visible_text_json, f.activity_type,
                   f.app_hint, f.confidence, f.analysis_time_ms, f.analysis_error
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
                   retention_days, updated_at,
                   vision_enabled, vision_provider, vision_model, vision_endpoint, vision_api_key
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
                vision_enabled = ?,
                vision_provider = ?,
                vision_model = ?,
                vision_endpoint = ?,
                vision_api_key = ?,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = 1
            "#,
        )
        .bind(settings.capture_interval)
        .bind(settings.monitors)
        .bind(settings.excluded_apps)
        .bind(settings.is_paused)
        .bind(settings.retention_days)
        .bind(settings.vision_enabled)
        .bind(settings.vision_provider)
        .bind(settings.vision_model)
        .bind(settings.vision_endpoint)
        .bind(settings.vision_api_key)
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

    // ===== Embedding Operations (RAG) =====

    /// Insert an embedding record (stores metadata and vector blob)
    pub async fn insert_embedding(&self, embedding: NewEmbedding) -> Result<i64> {
        let embedding_blob: Vec<u8> = embedding
            .embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM embeddings WHERE frame_id = ? AND chunk_index = ?")
            .bind(embedding.frame_id)
            .bind(embedding.chunk_index)
            .execute(&mut *tx)
            .await?;

        let result = sqlx::query(
            r#"
            INSERT INTO embeddings (
                frame_id, chunk_text, chunk_index, embedding_dim, embedding,
                provider, model, model_version, content_hash
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(embedding.frame_id)
        .bind(embedding.chunk_text)
        .bind(embedding.chunk_index)
        .bind(embedding.embedding.len() as i32)
        .bind(&embedding_blob)
        .bind(embedding.provider)
        .bind(embedding.model)
        .bind(embedding.model_version)
        .bind(embedding.content_hash)
        .execute(&mut *tx)
        .await?;
        let embedding_id = result.last_insert_rowid();

        sqlx::query("INSERT INTO embedding_vectors (embedding_id, embedding) VALUES (?, ?)")
            .bind(embedding_id)
            .bind(embedding_blob)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(embedding_id)
    }

    /// Replace every embedding chunk for one frame atomically.
    ///
    /// Existing chunks remain visible if any insertion fails because the
    /// delete and all metadata/vector inserts share one transaction.
    pub async fn insert_embeddings(&self, embeddings: Vec<NewEmbedding>) -> Result<()> {
        let Some(frame_id) = embeddings.first().map(|embedding| embedding.frame_id) else {
            return Ok(());
        };
        if embeddings
            .iter()
            .any(|embedding| embedding.frame_id != frame_id)
        {
            return Err(crate::DatabaseError::InvalidParameter(
                "batch embeddings must belong to one frame".to_string(),
            ));
        }

        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM embeddings WHERE frame_id = ?")
            .bind(frame_id)
            .execute(&mut *tx)
            .await?;

        for embedding in embeddings {
            let embedding_blob: Vec<u8> = embedding
                .embedding
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            let result = sqlx::query(
                r#"
                INSERT INTO embeddings (
                    frame_id, chunk_text, chunk_index, embedding_dim, embedding,
                    provider, model, model_version, content_hash
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(embedding.frame_id)
            .bind(embedding.chunk_text)
            .bind(embedding.chunk_index)
            .bind(embedding.embedding.len() as i32)
            .bind(&embedding_blob)
            .bind(embedding.provider)
            .bind(embedding.model)
            .bind(embedding.model_version)
            .bind(embedding.content_hash)
            .execute(&mut *tx)
            .await?;

            sqlx::query("INSERT INTO embedding_vectors (embedding_id, embedding) VALUES (?, ?)")
                .bind(result.last_insert_rowid())
                .bind(embedding_blob)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Get all embeddings for a frame
    pub async fn get_embeddings_for_frame(&self, frame_id: i64) -> Result<Vec<EmbeddingRecord>> {
        // Fetch rows with blob
        let rows = sqlx::query(
            r#"
            SELECT id, frame_id, chunk_text, chunk_index, embedding_dim, provider, model,
                   model_version, content_hash, created_at, embedding
            FROM embeddings
            WHERE frame_id = ?
            ORDER BY chunk_index ASC
            "#,
        )
        .bind(frame_id)
        .fetch_all(self.pool())
        .await?;

        // Convert rows to EmbeddingRecord manually to handle blob conversion
        let mut results = Vec::new();
        for row in rows {
            let embedding_blob: Vec<u8> = row.get("embedding");
            let dim: i32 = row.get("embedding_dim");

            // Convert blob bytes back to Vec<f32>
            // Assumes little-endian (Intel/standard)
            let embedding: Vec<f32> = embedding_blob
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
                .collect();

            if embedding.len() != dim as usize {
                tracing::warn!(
                    "Embedding dimension mismatch for id {}: expected {}, got {}",
                    row.get::<i64, _>("id"),
                    dim,
                    embedding.len()
                );
            }

            results.push(EmbeddingRecord {
                id: row.get("id"),
                frame_id: row.get("frame_id"),
                chunk_text: row.get("chunk_text"),
                chunk_index: row.get("chunk_index"),
                embedding_dim: dim,
                provider: row.get("provider"),
                model: row.get("model"),
                model_version: row.get("model_version"),
                content_hash: row.get("content_hash"),
                created_at: row.get("created_at"),
                embedding, // The decoded vector
            });
        }

        Ok(results)
    }

    /// Search for semantically similar text chunks using vector embeddings
    ///
    /// Uses the persistent sqlite-vec KNN index.
    ///
    /// # Arguments
    /// * `query_vector` - The embedding vector to search for
    /// * `limit` - Maximum number of results to return
    /// * `min_score` - Minimum cosine similarity score (0.0 to 1.0)
    /// * `start_time` - Optional start time filter (inclusive)
    /// * `end_time` - Optional end time filter (inclusive)
    pub async fn search_embeddings(
        &self,
        query_vector: Vec<f32>,
        limit: usize,
        min_score: f32,
    ) -> Result<Vec<SemanticResult>> {
        self.search_embeddings_with_time_range(query_vector, limit, min_score, None, None)
            .await
    }

    /// Search for semantically similar text chunks with optional time range filter
    ///
    /// Retrieves a wider KNN candidate set before applying optional frame
    /// metadata filters. This avoids loading all vectors into Rust memory.
    pub async fn search_embeddings_with_time_range(
        &self,
        query_vector: Vec<f32>,
        limit: usize,
        min_score: f32,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<Vec<SemanticResult>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        if query_vector.len() != EMBEDDING_DIM {
            return Err(crate::DatabaseError::InvalidParameter(format!(
                "Expected a {EMBEDDING_DIM}-dimensional query vector, got {}",
                query_vector.len()
            )));
        }

        let query_blob: Vec<u8> = query_vector
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let total_vectors = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM embedding_vectors")
            .fetch_one(self.pool())
            .await?;
        if total_vectors == 0 {
            return Ok(Vec::new());
        }

        let base_query = r#"
            WITH nearest AS (
                SELECT embedding_id, distance
                FROM embedding_vectors
                WHERE embedding MATCH ?
                ORDER BY distance
                LIMIT ?
            )
            SELECT e.frame_id, e.chunk_text, e.chunk_index,
                   f.timestamp, f.monitor_index, f.device_name, f.file_path,
                   f.active_window, f.active_process, f.browser_url, f.width, f.height,
                   f.offset_index, f.focused, f.created_at,
                   f.analysis_status, f.description, f.visible_text_json, f.activity_type,
                   f.app_hint, f.confidence, f.analysis_time_ms, f.analysis_error,
                   nearest.distance
            FROM nearest
            JOIN embeddings e ON e.id = nearest.embedding_id
            JOIN frames f ON e.frame_id = f.id
        "#;

        // Build WHERE clause dynamically
        let mut conditions = Vec::new();
        if start_time.is_some() {
            conditions.push("f.timestamp >= ?");
        }
        if end_time.is_some() {
            conditions.push("f.timestamp <= ?");
        }

        let query = if conditions.is_empty() {
            base_query.to_string()
        } else {
            format!("{} WHERE {}", base_query, conditions.join(" AND "))
        };

        let filtered = start_time.is_some() || end_time.is_some();
        let initial_limit = if filtered {
            limit.saturating_mul(12).max(100)
        } else {
            limit
        };
        let expanded_limit = limit.saturating_mul(100).max(initial_limit);
        let candidate_limits = [initial_limit, expanded_limit, total_vectors as usize];
        let mut rows = Vec::new();

        for requested_limit in candidate_limits {
            let candidate_limit = requested_limit.min(total_vectors as usize).max(limit) as i64;
            let mut query_builder = sqlx::query(&query).bind(&query_blob).bind(candidate_limit);
            if let Some(start) = &start_time {
                query_builder = query_builder.bind(start);
            }
            if let Some(end) = &end_time {
                query_builder = query_builder.bind(end);
            }

            rows = query_builder.fetch_all(self.pool()).await?;
            if !filtered || rows.len() >= limit || candidate_limit >= total_vectors {
                break;
            }
        }

        let mut candidates: Vec<SemanticResult> = Vec::new();

        for row in rows {
            let distance: f32 = row.get("distance");
            let similarity = 1.0 - distance;

            if similarity >= min_score {
                let frame = FrameRecord {
                    id: row.get("frame_id"),
                    chunk_id: None,
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
                    analysis_status: row.try_get("analysis_status").ok(),
                    description: row.try_get("description").ok(),
                    visible_text_json: row.try_get("visible_text_json").ok(),
                    activity_type: row.try_get("activity_type").ok(),
                    app_hint: row.try_get("app_hint").ok(),
                    confidence: row.try_get("confidence").ok(),
                    analysis_time_ms: row.try_get("analysis_time_ms").ok(),
                    analysis_error: row.try_get("analysis_error").ok(),
                };

                candidates.push(SemanticResult {
                    frame,
                    chunk_text: row.get("chunk_text"),
                    chunk_index: row.get("chunk_index"),
                    similarity_score: similarity,
                    retrieval_source: "vector".to_string(),
                });
            }
        }

        // Sort by similarity descending
        candidates.sort_by(|a, b| {
            b.similarity_score
                .partial_cmp(&a.similarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take top K
        Ok(candidates.into_iter().take(limit).collect())
    }

    // ===== Vision Analysis Queue Operations =====

    /// Enqueue a frame for analysis
    pub async fn enqueue_frame_for_analysis(&self, frame_id: i64, priority: i32) -> Result<i64> {
        // Check if already in queue
        let exists =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM analysis_queue WHERE frame_id = ?")
                .bind(frame_id)
                .fetch_one(self.pool())
                .await?;

        if exists > 0 {
            return Ok(0); // Already queued
        }

        let result = sqlx::query("INSERT INTO analysis_queue (frame_id, priority) VALUES (?, ?)")
            .bind(frame_id)
            .bind(priority)
            .execute(self.pool())
            .await?;

        // Also update status
        sqlx::query("UPDATE frames SET analysis_status = 'pending' WHERE id = ?")
            .bind(frame_id)
            .execute(self.pool())
            .await?;

        Ok(result.last_insert_rowid())
    }

    /// Recent frames that still need vision analysis and are not already queued.
    ///
    /// Captured frames default to `analysis_status = 'pending'` but are not
    /// auto-queued, so "needs analysis" is any frame whose status is not a
    /// terminal/in-flight state (`completed`/`processing`/`failed`) and that is
    /// not already in the queue. Returned newest-first so a throttled background
    /// enqueuer works through the most relevant (recent) history first.
    pub async fn get_unanalyzed_frame_ids(&self, limit: i64) -> Result<Vec<i64>> {
        let ids = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT f.id
            FROM frames f
            WHERE (f.analysis_status IS NULL
                   OR f.analysis_status NOT IN ('completed', 'processing', 'failed'))
              AND NOT EXISTS (SELECT 1 FROM analysis_queue q WHERE q.frame_id = f.id)
            ORDER BY f.id DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        Ok(ids)
    }

    /// Aggregate status of vision analysis (counts by frame analysis_status plus
    /// the current queue depth and the configured vision settings).
    pub async fn get_vision_status(&self) -> Result<crate::models::VisionStatus> {
        let settings = self.get_settings().await?;

        let total_frames = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM frames")
            .fetch_one(self.pool())
            .await?;

        let count_status = |status: &str| {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM frames WHERE analysis_status = ?")
                .bind(status.to_string())
                .fetch_one(self.pool())
        };

        let completed = count_status("completed").await?;
        let pending = count_status("pending").await?;
        let processing = count_status("processing").await?;
        let failed = count_status("failed").await?;

        let queue_depth = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM analysis_queue")
            .fetch_one(self.pool())
            .await?;

        Ok(crate::models::VisionStatus {
            enabled: settings.vision_enabled,
            provider: settings.vision_provider,
            model: settings.vision_model,
            total_frames,
            completed,
            pending,
            processing,
            failed,
            queue_depth,
        })
    }

    /// Get pending analysis tasks (locks them implicitly by return, caller must process)
    /// Ideally we should use a transaction to lock rows, but SQLite is single-writer.
    /// We can use 'locked_until' to implement a soft lock.
    pub async fn claim_analysis_task(&self, _worker_id: &str) -> Result<Option<AnalysisQueueItem>> {
        // Find highest priority pending task not locked
        // We use a transaction to ensure atomicity
        let mut tx = self.pool().begin().await?;

        let task = sqlx::query_as::<_, AnalysisQueueItem>(
            r#"
            SELECT id, frame_id, priority, created_at, locked_until, attempts, last_error
            FROM analysis_queue
            WHERE locked_until IS NULL OR locked_until < CURRENT_TIMESTAMP
            ORDER BY priority DESC, created_at ASC
            LIMIT 1
            "#,
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(task) = task {
            // Lock it for 5 minutes
            sqlx::query(
                "UPDATE analysis_queue SET locked_until = datetime('now', '+5 minutes'), attempts = attempts + 1 WHERE id = ?"
            )
            .bind(task.id)
            .execute(&mut *tx)
            .await?;

            // Update frame status
            sqlx::query("UPDATE frames SET analysis_status = 'processing' WHERE id = ?")
                .bind(task.frame_id)
                .execute(&mut *tx)
                .await?;

            tx.commit().await?;
            Ok(Some(task))
        } else {
            tx.commit().await?;
            Ok(None)
        }
    }

    /// Mark analysis as complete and remove from queue
    pub async fn complete_analysis_task(
        &self,
        queue_id: i64,
        frame_id: i64,
        analysis: crate::models::FrameAnalysisUpdate,
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;

        // Update frame with analysis results
        sqlx::query(
            r#"
            UPDATE frames SET
                analysis_status = 'completed',
                description = ?,
                visible_text_json = ?,
                activity_type = ?,
                app_hint = ?,
                confidence = ?,
                analysis_time_ms = ?,
                analysis_error = NULL
            WHERE id = ?
            "#,
        )
        .bind(analysis.description)
        .bind(analysis.visible_text_json)
        .bind(analysis.activity_type)
        .bind(analysis.app_hint)
        .bind(analysis.confidence)
        .bind(analysis.analysis_time_ms)
        .bind(frame_id)
        .execute(&mut *tx)
        .await?;

        // Remove from queue
        sqlx::query("DELETE FROM analysis_queue WHERE id = ?")
            .bind(queue_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Mark analysis as failed
    pub async fn fail_analysis_task(
        &self,
        queue_id: i64,
        frame_id: i64,
        error: String,
    ) -> Result<()> {
        let mut tx = self.pool().begin().await?;

        // Update frame status
        sqlx::query(
            "UPDATE frames SET analysis_status = 'failed', analysis_error = ? WHERE id = ?",
        )
        .bind(&error)
        .bind(frame_id)
        .execute(&mut *tx)
        .await?;

        // Update queue item - unlock and record error, or delete if max attempts?
        // Let's just release lock and let retry handle it, unless max attempts reached
        // Doing simpler logic: release lock immediately so it can be retried later or inspect manually
        sqlx::query("UPDATE analysis_queue SET locked_until = NULL, last_error = ? WHERE id = ?")
            .bind(&error)
            .bind(queue_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    /// Get OCR frames that don't have embeddings yet (for background processing).
    pub async fn get_frames_without_embeddings(&self, limit: i64) -> Result<Vec<FrameRecord>> {
        let frames = sqlx::query_as::<_, FrameRecord>(
            r#"
            SELECT f.id, f.chunk_id, f.timestamp, f.monitor_index, f.device_name,
                   f.file_path, f.active_window, f.active_process, f.browser_url,
                   f.width, f.height, f.offset_index, f.focused, f.created_at,
                   f.analysis_status, f.description, f.visible_text_json, f.activity_type,
                   f.app_hint, f.confidence, f.analysis_time_ms, f.analysis_error
            FROM frames f
            LEFT JOIN embeddings e ON f.id = e.frame_id
            WHERE e.id IS NULL
              AND EXISTS (SELECT 1 FROM ocr_text o WHERE o.frame_id = f.id)
            ORDER BY f.id ASC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        Ok(frames)
    }

    /// Get embedding status statistics
    pub async fn get_embedding_status(&self) -> Result<EmbeddingStatus> {
        let total_frames =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(DISTINCT frame_id) FROM ocr_text")
                .fetch_one(self.pool())
                .await?;

        let frames_with_embeddings =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(DISTINCT frame_id) FROM embeddings")
                .fetch_one(self.pool())
                .await?;

        let enabled = self
            .get_metadata("embeddings_enabled")
            .await?
            .map(|v| v == "true")
            .unwrap_or(false);

        let model = self
            .get_metadata("embeddings_model")
            .await?
            .unwrap_or_else(|| "EmbeddingGemma-300M".to_string());
        let provider = self
            .get_metadata("embeddings_provider")
            .await?
            .unwrap_or_else(|| "unknown".to_string());
        let model_version = self
            .get_metadata("embeddings_model_version")
            .await?
            .unwrap_or_else(|| "unknown".to_string());
        let dimension = self
            .get_metadata("embeddings_dimension")
            .await?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let reindex_required = self
            .get_metadata("embeddings_reindex_required")
            .await?
            .map(|value| value == "true")
            .unwrap_or(false);

        let last_processed_frame_id = self
            .get_metadata("embeddings_last_processed_frame_id")
            .await?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        let coverage_percent = if total_frames > 0 {
            (frames_with_embeddings as f32 / total_frames as f32) * 100.0
        } else {
            0.0
        };

        Ok(EmbeddingStatus {
            enabled,
            model,
            provider,
            model_version,
            dimension,
            reindex_required,
            total_frames,
            frames_with_embeddings,
            coverage_percent,
            last_processed_frame_id,
        })
    }

    /// Invalidate vectors when the configured embedding model contract changes.
    pub async fn ensure_embedding_model(
        &self,
        provider: &str,
        model: &str,
        model_version: &str,
        dimension: usize,
    ) -> Result<bool> {
        let current_provider = self.get_metadata("embeddings_provider").await?;
        let current_model = self.get_metadata("embeddings_model").await?;
        let current_version = self.get_metadata("embeddings_model_version").await?;
        let current_dimension = self.get_metadata("embeddings_dimension").await?;
        let expected_dimension = dimension.to_string();

        if current_provider.as_deref() == Some(provider)
            && current_model.as_deref() == Some(model)
            && current_version.as_deref() == Some(model_version)
            && current_dimension.as_deref() == Some(expected_dimension.as_str())
        {
            return Ok(false);
        }

        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM embedding_vectors")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM embeddings")
            .execute(&mut *tx)
            .await?;
        for (key, value) in [
            ("embeddings_provider", provider),
            ("embeddings_model", model),
            ("embeddings_model_version", model_version),
            ("embeddings_dimension", expected_dimension.as_str()),
            ("embeddings_last_processed_frame_id", "0"),
            ("embeddings_reindex_required", "true"),
        ] {
            sqlx::query(
                "INSERT INTO metadata (key, value) VALUES (?, ?) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
            )
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Delete embeddings for a frame
    pub async fn delete_embeddings_for_frame(&self, frame_id: i64) -> Result<u64> {
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "DELETE FROM embedding_vectors WHERE embedding_id IN (SELECT id FROM embeddings WHERE frame_id = ?)",
        )
            .bind(frame_id)
            .execute(&mut *tx)
            .await?;
        let result = sqlx::query("DELETE FROM embeddings WHERE frame_id = ?")
            .bind(frame_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(result.rows_affected())
    }

    /// Get total embedding count
    pub async fn count_embeddings(&self) -> Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM embeddings")
            .fetch_one(self.pool())
            .await?;

        Ok(count)
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
