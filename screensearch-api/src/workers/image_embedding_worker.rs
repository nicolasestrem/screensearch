//! Background image-embedding worker.
//!
//! Embeds stored screenshots (by `file_path`) with the in-process nomic vision
//! model into the separate image-embedding index, so text queries can retrieve
//! frames by their pixels. Mirrors the text [`super::embedding_worker`]; gated at
//! runtime by the `image_embeddings_enabled` metadata flag.

use screensearch_db::DatabaseManager;
use screensearch_embeddings::{EmbeddingEngine, ImageEmbeddingEngine};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Configuration for the background image-embedding worker.
///
/// The worker loop is always spawned but gated at runtime by the
/// `image_embeddings_enabled` metadata flag (so the API toggle takes effect
/// without a restart, and the models load lazily only when enabled).
#[derive(Debug, Clone)]
pub struct ImageEmbeddingWorkerConfig {
    pub batch_size: i64,
    pub interval_secs: u64,
}

impl Default for ImageEmbeddingWorkerConfig {
    fn default() -> Self {
        Self {
            batch_size: 16,
            interval_secs: 60,
        }
    }
}

/// Background worker for generating image embeddings.
pub struct ImageEmbeddingWorker {
    db: Arc<DatabaseManager>,
    engine: Arc<ImageEmbeddingEngine>,
    config: ImageEmbeddingWorkerConfig,
}

impl ImageEmbeddingWorker {
    pub fn new(
        db: Arc<DatabaseManager>,
        engine: Arc<ImageEmbeddingEngine>,
        config: ImageEmbeddingWorkerConfig,
    ) -> Self {
        Self { db, engine, config }
    }

    /// Process a batch of frames that lack an image embedding.
    pub async fn process_batch(&self) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let frames = self
            .db
            .get_frames_without_image_embeddings(self.config.batch_size)
            .await?;
        if frames.is_empty() {
            debug!("No frames to image-embed");
            return Ok(0);
        }

        info!("Processing {} frames for image embeddings", frames.len());
        let mut processed = 0;

        for frame in frames {
            // Embed one frame at a time so a missing/corrupt screenshot only skips
            // that frame instead of failing the whole batch.
            let vectors = match self
                .engine
                .embed_images(vec![frame.file_path.clone()])
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    warn!("Failed to image-embed frame {}: {}", frame.id, e);
                    // Advance the cursor so a persistently-bad frame doesn't wedge
                    // the worker on every tick.
                    self.db
                        .set_metadata(
                            "image_embeddings_last_processed_frame_id",
                            &frame.id.to_string(),
                        )
                        .await?;
                    continue;
                }
            };
            let Some(embedding) = vectors.into_iter().next() else {
                continue;
            };

            self.db
                .insert_image_embedding(
                    frame.id,
                    embedding,
                    self.engine.provider(),
                    self.engine.model(),
                    self.engine.model_version(),
                    &EmbeddingEngine::content_hash(&frame.file_path),
                )
                .await?;
            processed += 1;

            self.db
                .set_metadata(
                    "image_embeddings_last_processed_frame_id",
                    &frame.id.to_string(),
                )
                .await?;
        }

        if processed > 0
            && self
                .db
                .get_frames_without_image_embeddings(1)
                .await?
                .is_empty()
        {
            self.db
                .set_metadata("image_embeddings_reindex_required", "false")
                .await?;
        }
        info!("Image-embedded {} frames", processed);
        Ok(processed)
    }
}
