//! Background embedding worker
//!
//! Processes frames without embeddings in the background.

use screensearch_db::DatabaseManager;
use screensearch_embeddings::{EmbeddingEngine, TextChunker};
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

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
    chunker: TextChunker,
    config: EmbeddingWorkerConfig,
}

impl EmbeddingWorker {
    /// Create a new embedding worker
    pub fn new(
        db: Arc<DatabaseManager>,
        engine: Arc<EmbeddingEngine>,
        config: EmbeddingWorkerConfig,
    ) -> Self {
        Self {
            db,
            engine,
            chunker: TextChunker::default(),
            config,
        }
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

            if ocr_texts.is_empty() {
                // No OCR text, still mark as processed by inserting empty embedding
                debug!("Frame {} has no OCR text", frame.id);
                continue;
            }

            // Combine OCR text and chunk it
            let combined_text: String = ocr_texts
                .iter()
                .map(|o| o.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            let chunks = self.chunker.chunk_text(&combined_text);

            // Generate embeddings for each chunk
            // Start a transaction for this frame's embeddings
            // This ensures we don't have partial embeddings if something fails
            let mut tx = self.db.pool().begin().await.map_err(|e| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to start transaction: {}", e),
                )) as Box<dyn std::error::Error + Send + Sync>
            })?;

            // Generate embeddings for each chunk
            for (chunk_index, chunk_text) in chunks.iter().enumerate() {
                // Generate embedding
                let embedding = self.engine.embed(chunk_text)?;

                // Convert Vec<f32> to Vec<u8> (little-endian bytes) for BLOB storage
                let embedding_bytes: Vec<u8> = embedding
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect();

                // Insert into DB using the transaction
                sqlx::query(
                    r#"
                    INSERT INTO embeddings (frame_id, chunk_text, chunk_index, embedding, embedding_dim)
                    VALUES (?, ?, ?, ?, ?)
                    "#
                )
                .bind(frame.id)
                .bind(chunk_text)
                .bind(chunk_index as i32)
                .bind(embedding_bytes)
                .bind(384) // Dimension
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                   Box::new(std::io::Error::new(
                       std::io::ErrorKind::Other,
                       format!("Failed to insert embedding: {}", e),
                   )) as Box<dyn std::error::Error + Send + Sync>
                })?;
            }
            
            // Commit transaction
            tx.commit().await.map_err(|e| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to commit transaction: {}", e),
                )) as Box<dyn std::error::Error + Send + Sync>
            })?;

            processed += 1;

            // Update last processed frame ID
            self.db
                .set_metadata(
                    "embeddings_last_processed_frame_id",
                    &frame.id.to_string(),
                )
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
