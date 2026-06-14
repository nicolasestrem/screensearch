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
    pub provider: String,
    pub model_version: String,
    pub dimension: i64,
    pub reindex_required: bool,
    pub sidecar_ready: bool,
    pub error: Option<String>,
    pub total_frames: i64,
    pub frames_with_embeddings: i64,
    pub coverage_percent: f32,
    pub last_processed_frame_id: i64,
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

    let sidecar_error = state.get_embedding_engine().await.err();
    let status = state.db.get_embedding_status().await?;

    Ok(Json(EmbeddingStatusResponse {
        enabled: status.enabled,
        model: status.model,
        provider: status.provider,
        model_version: status.model_version,
        dimension: status.dimension,
        reindex_required: status.reindex_required,
        sidecar_ready: sidecar_error.is_none(),
        error: sidecar_error,
        total_frames: status.total_frames,
        frames_with_embeddings: status.frames_with_embeddings,
        coverage_percent: status.coverage_percent,
        last_processed_frame_id: status.last_processed_frame_id,
    }))
}

/// POST /embeddings/generate
/// Trigger background embedding generation for frames without embeddings
pub async fn generate_embeddings(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GenerateEmbeddingsRequest>,
) -> Result<Json<GenerateEmbeddingsResponse>> {
    let batch_size = payload.batch_size.unwrap_or(50);
    debug!("Triggering embedding generation for {} frames", batch_size);

    // Get frames without embeddings
    let frames = state.db.get_frames_without_embeddings(batch_size).await?;

    let frames_count = frames.len() as i64;

    if frames_count == 0 {
        return Ok(Json(GenerateEmbeddingsResponse {
            success: true,
            message: "All frames already have embeddings".to_string(),
            frames_processed: 0,
        }));
    }

    // Get or initialize the embedding engine
    let engine = match state.get_embedding_engine().await {
        Ok(e) => e,
        Err(e) => {
            return Ok(Json(GenerateEmbeddingsResponse {
                success: false,
                message: format!("Failed to initialize embedding engine: {}", e),
                frames_processed: 0,
            }));
        }
    };

    let mut processed = 0;

    for frame in frames {
        // Get OCR text for the frame
        let ocr_texts = match state.db.get_ocr_text_for_frame(frame.id).await {
            Ok(texts) => texts,
            Err(_) => continue,
        };

        if ocr_texts.is_empty() {
            continue;
        }

        // Combine OCR text and chunk it
        let ocr_text: String = ocr_texts
            .iter()
            .map(|o| o.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let combined_text = format!(
            "Timestamp: {}\nApplication: {}\nWindow: {}\nScreen text:\n{}",
            frame.timestamp,
            frame.active_process.as_deref().unwrap_or("Unknown"),
            frame.active_window.as_deref().unwrap_or(""),
            ocr_text
        );
        let chunks = match engine.chunk_text(&combined_text, 512, 64).await {
            Ok(chunks) => chunks,
            Err(error) => {
                tracing::warn!("Failed to chunk frame {}: {}", frame.id, error);
                continue;
            }
        };

        let chunk_refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
        let embeddings = match engine.embed_batch(&chunk_refs).await {
            Ok(embeddings) => embeddings,
            Err(error) => {
                tracing::warn!("Failed to embed frame {}: {}", frame.id, error);
                continue;
            }
        };

        for (chunk_index, (chunk_text, embedding)) in chunks.iter().zip(embeddings).enumerate() {
            // Insert embedding record
            let new_embedding = screensearch_db::NewEmbedding {
                frame_id: frame.id,
                chunk_text: chunk_text.clone(),
                chunk_index: chunk_index as i32,
                embedding,
                provider: engine.provider().to_string(),
                model: engine.model().to_string(),
                model_version: engine.model_version().to_string(),
                content_hash: screensearch_embeddings::EmbeddingEngine::content_hash(chunk_text),
            };

            if let Err(e) = state.db.insert_embedding(new_embedding).await {
                tracing::warn!("Failed to insert embedding: {}", e);
                continue;
            }
        }

        processed += 1;

        // Update last processed frame ID
        let _ = state
            .db
            .set_metadata("embeddings_last_processed_frame_id", &frame.id.to_string())
            .await;
    }

    if processed > 0 && state.db.get_frames_without_embeddings(1).await?.is_empty() {
        let _ = state
            .db
            .set_metadata("embeddings_reindex_required", "false")
            .await;
    }

    Ok(Json(GenerateEmbeddingsResponse {
        success: true,
        message: format!("Processed {} frames with embeddings", processed),
        frames_processed: processed,
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
