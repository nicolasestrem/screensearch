//! Background embedding worker
//!
//! Processes frames without embeddings in the background.

use screensearch_db::{DatabaseManager, FrameRecord};
use screensearch_embeddings::EmbeddingEngine;
use std::fmt::Write as _;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

/// Build the text embedded for a frame: OCR text plus the vision worker's
/// `description` and `visible_text` labels when present.
///
/// Shared by the background worker and the manual generate-embeddings handler so
/// the two never drift. Including the vision fields gives non-OCR visual content
/// (charts, canvases, icon-heavy UIs) a recall path it otherwise lacks; the
/// vision fields are skipped when absent/empty so OCR-only frames are unchanged.
pub fn build_frame_embedding_text(frame: &FrameRecord, ocr_text: &str) -> String {
    let mut text = format!(
        "Timestamp: {}\nApplication: {}\nWindow: {}\nScreen text:\n{}",
        frame.timestamp,
        frame.active_process.as_deref().unwrap_or("Unknown"),
        frame.active_window.as_deref().unwrap_or(""),
        ocr_text
    );

    if let Some(desc) = frame.description.as_deref() {
        let desc = desc.trim();
        if !desc.is_empty() {
            let _ = write!(text, "\nVisual summary:\n{desc}");
        }
    }

    if let Some(raw) = frame.visible_text_json.as_deref() {
        let raw = raw.trim();
        if !raw.is_empty() {
            // `visible_text_json` is a JSON array of strings; flatten to a
            // readable line, falling back to the raw value if it isn't that shape.
            let labels = serde_json::from_str::<Vec<String>>(raw)
                .map(|v| v.join(", "))
                .unwrap_or_else(|_| raw.to_string());
            let labels = labels.trim();
            if !labels.is_empty() {
                let _ = write!(text, "\nVisible labels: {labels}");
            }
        }
    }

    text
}

/// Whether a frame carries vision-derived text worth embedding: a non-empty
/// `description` or non-empty `visible_text` labels. Mirrors the fields
/// [`build_frame_embedding_text`] appends, and is used to decide whether a frame
/// with no OCR text is still embeddable.
pub fn frame_has_vision_text(description: Option<&str>, visible_text_json: Option<&str>) -> bool {
    let has_description = description.map(|d| !d.trim().is_empty()).unwrap_or(false);
    let has_labels = visible_text_json
        .map(|v| {
            let trimmed = v.trim();
            !trimmed.is_empty() && trimmed != "[]"
        })
        .unwrap_or(false);
    has_description || has_labels
}

/// Configuration for the background embedding worker
#[derive(Debug, Clone)]
pub struct EmbeddingWorkerConfig {
    /// Batch size for processing frames
    pub batch_size: i64,
    /// Interval between processing runs (seconds)
    pub interval_secs: u64,
    /// Whether the worker is enabled
    pub enabled: bool,
}

impl Default for EmbeddingWorkerConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            interval_secs: 60,
            enabled: false,
        }
    }
}

/// Background worker for generating embeddings
pub struct EmbeddingWorker {
    db: Arc<DatabaseManager>,
    engine: Arc<EmbeddingEngine>,
    config: EmbeddingWorkerConfig,
}

impl EmbeddingWorker {
    /// Create a new embedding worker
    pub fn new(
        db: Arc<DatabaseManager>,
        engine: Arc<EmbeddingEngine>,
        config: EmbeddingWorkerConfig,
    ) -> Self {
        Self { db, engine, config }
    }

    /// Process a batch of frames without embeddings
    pub async fn process_batch(&self) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        // Get frames without embeddings
        let frames = self
            .db
            .get_frames_without_embeddings(self.config.batch_size)
            .await?;

        if frames.is_empty() {
            debug!("No frames to process");
            return Ok(0);
        }

        info!("Processing {} frames for embeddings", frames.len());
        let mut processed = 0;

        for frame in frames {
            // Get OCR text for the frame
            let ocr_texts = self.db.get_ocr_text_for_frame(frame.id).await?;
            let ocr_text: String = ocr_texts
                .iter()
                .map(|o| o.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            // Embed when there's OCR text OR vision-derived text; a frame with
            // neither has nothing to embed (would be metadata only), so skip it.
            if ocr_text.trim().is_empty()
                && !frame_has_vision_text(
                    frame.description.as_deref(),
                    frame.visible_text_json.as_deref(),
                )
            {
                debug!("Frame {} has no embeddable text", frame.id);
                continue;
            }

            // Combine OCR text + vision fields and chunk it
            let combined_text = build_frame_embedding_text(&frame, &ocr_text);
            let chunks = self.engine.chunk_text(&combined_text, 512, 64).await?;

            let chunk_refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
            let embeddings = self.engine.embed_batch(&chunk_refs).await?;
            let new_embeddings = chunks
                .iter()
                .zip(embeddings)
                .enumerate()
                .map(
                    |(chunk_index, (chunk_text, embedding))| screensearch_db::NewEmbedding {
                        frame_id: frame.id,
                        chunk_text: chunk_text.clone(),
                        chunk_index: chunk_index as i32,
                        embedding,
                        provider: self.engine.provider().to_string(),
                        model: self.engine.model().to_string(),
                        model_version: self.engine.model_version().to_string(),
                        content_hash: EmbeddingEngine::content_hash(chunk_text),
                    },
                )
                .collect();
            self.db.insert_embeddings(new_embeddings).await?;

            processed += 1;

            // Update last processed frame ID
            self.db
                .set_metadata("embeddings_last_processed_frame_id", &frame.id.to_string())
                .await?;
        }

        if processed > 0 && self.db.get_frames_without_embeddings(1).await?.is_empty() {
            self.db
                .set_metadata("embeddings_reindex_required", "false")
                .await?;
        }
        info!("Processed {} frames", processed);
        Ok(processed)
    }

    /// Run the worker continuously
    pub async fn run(&self) {
        if !self.config.enabled {
            warn!("Embedding worker is disabled");
            return;
        }

        info!(
            "Starting embedding worker with {}s interval",
            self.config.interval_secs
        );

        let mut tick = interval(Duration::from_secs(self.config.interval_secs));

        loop {
            tick.tick().await;

            // Dynamic check for enabled status from DB metadata
            // detailed: check if "embeddings_enabled" is explicitly "false"
            // If missing, we fall back to config (or keep running if enabled initially)
            // But since API sets it to "true"/"false", we should respect it.
            let enabled = match self.db.get_metadata("embeddings_enabled").await {
                Ok(Some(val)) => val == "true",
                Ok(None) => self.config.enabled, // Fallback to config if not set
                Err(e) => {
                    warn!("Failed to fetch embedding status: {}", e);
                    self.config.enabled // Conservative fallback
                }
            };

            if !enabled {
                // Skips processing this tick if disabled dynamically
                continue;
            }

            match self.process_batch().await {
                Ok(count) => {
                    if count > 0 {
                        info!("Embedding worker processed {} frames", count);
                    }
                }
                Err(e) => {
                    error!("Embedding worker error: {}", e);
                }
            }
        }
    }

    /// Run a single batch (for manual triggering)
    pub async fn run_once(&self) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        self.process_batch().await
    }
}

/// Start the embedding worker as a background task
pub fn spawn_embedding_worker(
    db: Arc<DatabaseManager>,
    engine: Arc<EmbeddingEngine>,
    config: EmbeddingWorkerConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let worker = EmbeddingWorker::new(db, engine, config);
        worker.run().await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_frame(description: Option<&str>, visible_text_json: Option<&str>) -> FrameRecord {
        FrameRecord {
            id: 1,
            chunk_id: None,
            timestamp: Utc::now(),
            monitor_index: 0,
            device_name: "monitor-0".to_string(),
            file_path: "captures/frame.jpg".to_string(),
            active_window: Some("Editor".to_string()),
            active_process: Some("code.exe".to_string()),
            browser_url: None,
            width: 1280,
            height: 720,
            offset_index: 0,
            focused: Some(true),
            created_at: Utc::now(),
            analysis_status: Some("completed".to_string()),
            description: description.map(str::to_string),
            visible_text_json: visible_text_json.map(str::to_string),
            activity_type: Some("coding".to_string()),
            app_hint: Some("VS Code".to_string()),
            confidence: Some(0.9),
            analysis_time_ms: Some(100),
            analysis_error: None,
        }
    }

    #[test]
    fn ocr_only_when_no_vision_fields() {
        let frame = make_frame(None, None);
        let text = build_frame_embedding_text(&frame, "hello world");
        assert!(text.contains("Screen text:\nhello world"));
        assert!(!text.contains("Visual summary"));
        assert!(!text.contains("Visible labels"));
    }

    #[test]
    fn includes_description_and_flattened_visible_labels() {
        let frame = make_frame(
            Some("a bar chart of Q3 revenue"),
            Some(r#"["Q3","Revenue","Sales"]"#),
        );
        let text = build_frame_embedding_text(&frame, "ocr text");
        assert!(text.contains("Screen text:\nocr text"));
        assert!(text.contains("Visual summary:\na bar chart of Q3 revenue"));
        assert!(text.contains("Visible labels: Q3, Revenue, Sales"));
    }

    #[test]
    fn falls_back_on_non_json_visible_text() {
        let frame = make_frame(None, Some("just a raw string"));
        let text = build_frame_embedding_text(&frame, "ocr");
        assert!(text.contains("Visible labels: just a raw string"));
    }

    #[test]
    fn skips_empty_vision_fields() {
        let frame = make_frame(Some("   "), Some("[]"));
        let text = build_frame_embedding_text(&frame, "ocr");
        assert!(!text.contains("Visual summary"));
        // an empty JSON array flattens to an empty string -> no label line
        assert!(!text.contains("Visible labels"));
    }

    #[test]
    fn vision_text_detection() {
        assert!(!frame_has_vision_text(None, None));
        assert!(!frame_has_vision_text(Some("   "), Some("[]")));
        assert!(!frame_has_vision_text(Some(""), Some("")));
        assert!(frame_has_vision_text(Some("a bar chart"), None));
        assert!(frame_has_vision_text(None, Some(r#"["Submit","Cancel"]"#)));
        assert!(frame_has_vision_text(Some("desc"), Some("[]")));
    }
}
