//! Embeddings API Handlers
//!
//! Provides endpoints for managing and querying vector embeddings
//! used for RAG-enhanced intelligence reports.

use crate::error::Result;
use crate::state::AppState;
use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

// ============================================================
// Models
// ============================================================

/// Response for embedding status endpoint
#[derive(Debug, Serialize)]
pub struct EmbeddingStatusResponse {
    pub enabled: bool,
    pub model: String,
    pub total_frames: i64,
    pub frames_with_embeddings: i64,
    pub coverage_percent: f32,
    pub last_processed_frame_id: i64,
    pub generating: bool,
}

/// Request to trigger embedding generation
#[derive(Debug, Deserialize)]
pub struct GenerateEmbeddingsRequest {
    /// Number of frames to process in this batch
    pub batch_size: Option<i64>,
}

/// Response from embedding generation
#[derive(Debug, Serialize)]
pub struct GenerateEmbeddingsResponse {
    pub success: bool,
    pub message: String,
    pub frames_processed: i64,
}

// ============================================================
// Handlers
// ============================================================

/// GET /embeddings/status
/// Get the current status of embedding generation
pub async fn get_embedding_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<EmbeddingStatusResponse>> {
    debug!("Getting embedding status");

    let status = state.db.get_embedding_status().await?;

    Ok(Json(EmbeddingStatusResponse {
        enabled: status.enabled,
        model: status.model,
        total_frames: status.total_frames,
        frames_with_embeddings: status.frames_with_embeddings,
        coverage_percent: status.coverage_percent,
        last_processed_frame_id: status.last_processed_frame_id,
        generating: state.is_generating_embeddings.load(std::sync::atomic::Ordering::Relaxed),
    }))
}

/// POST /embeddings/generate
/// Trigger background embedding generation for frames without embeddings
pub async fn generate_embeddings(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GenerateEmbeddingsRequest>,
) -> Result<Json<GenerateEmbeddingsResponse>> {
    // Check if already running
    if state
        .is_generating_embeddings
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        return Ok(Json(GenerateEmbeddingsResponse {
            success: false,
            message: "Embedding generation is already running".to_string(),
            frames_processed: 0,
        }));
    }

    // Set running flag
    state
        .is_generating_embeddings
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let batch_size = payload.batch_size.unwrap_or(50);
    let state_clone = state.clone();

    // Spawn background task
    tokio::spawn(async move {
        debug!("Starting background embedding generation");
        let result = async {
            // Get or initialize the embedding engine first
            let engine = state_clone
                .get_embedding_engine()
                .await
                .map_err(|e| e.to_string())?;

            let chunker = screensearch_embeddings::TextChunker::default();
            let mut total_processed = 0;

            loop {
                // Get frames without embeddings
                let frames = state_clone
                    .db
                    .get_frames_without_embeddings(batch_size)
                    .await
                    .map_err(|e| e.to_string())?;

                if frames.is_empty() {
                    break;
                }

                for frame in frames {
                    // Get OCR text for the frame
                    let ocr_texts = match state_clone.db.get_ocr_text_for_frame(frame.id).await {
                        Ok(texts) => texts,
                        Err(_) => continue,
                    };

                    if ocr_texts.is_empty() {
                        continue;
                    }

                    // Combine OCR text and chunk it
                    let combined_text: String = ocr_texts
                        .iter()
                        .map(|o| o.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");

                    let chunks = chunker.chunk_text(&combined_text);

                    // Generate embeddings for each chunk
                    for (chunk_index, chunk_text) in chunks.iter().enumerate() {
                        let embedding = match engine.embed(chunk_text) {
                            Ok(e) => e,
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to embed chunk for frame {}: {}",
                                    frame.id,
                                    e
                                );
                                continue;
                            }
                        };

                        // Insert embedding record
                        let new_embedding = screensearch_db::NewEmbedding {
                            frame_id: frame.id,
                            chunk_text: chunk_text.clone(),
                            chunk_index: chunk_index as i32,
                            embedding,
                        };

                        if let Err(e) = state_clone.db.insert_embedding(new_embedding).await {
                            tracing::warn!("Failed to insert embedding: {}", e);
                            continue;
                        }
                    }

                    total_processed += 1;

                    // Update last processed frame ID
                    let _ = state_clone
                        .db
                        .set_metadata("embeddings_last_processed_frame_id", &frame.id.to_string())
                        .await;
                }
            }
            Ok::<_, String>(total_processed)
        }
        .await;

        // Reset flag
        state_clone
            .is_generating_embeddings
            .store(false, std::sync::atomic::Ordering::SeqCst);

        match result {
            Ok(count) => debug!(
                "Background embedding generation completed. Total processed: {}",
                count
            ),
            Err(e) => tracing::error!("Background embedding generation failed: {}", e),
        }
    });

    Ok(Json(GenerateEmbeddingsResponse {
        success: true,
        message: "Background generation started".to_string(),
        frames_processed: 0,
    }))
}

/// POST /embeddings/enable
/// Enable or disable embedding generation
pub async fn toggle_embeddings(
    State(state): State<Arc<AppState>>,
    Json(enabled): Json<bool>,
) -> Result<Json<EmbeddingStatusResponse>> {
    debug!("Setting embeddings enabled: {}", enabled);

    state
        .db
        .set_metadata("embeddings_enabled", if enabled { "true" } else { "false" })
        .await?;

    // Return updated status
    get_embedding_status(State(state)).await
}
