//! Embeddings API Handlers
//!
//! Provides endpoints for managing and querying vector embeddings
//! used for RAG-enhanced intelligence reports.

use crate::error::Result;
use crate::state::AppState;
use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Guards against overlapping on-demand embedding jobs: clicking "Process
/// frames" repeatedly should not spawn several concurrent passes over the same
/// backlog. The background `embedding_worker` and this on-demand handler both
/// insert idempotently (unique chunk constraint), so this is purely to avoid
/// wasted, redundant work.
static EMBEDDING_JOB_RUNNING: AtomicBool = AtomicBool::new(false);

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
    /// Whether the in-process embedding model is loaded and ready.
    pub engine_ready: bool,
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

    // Report readiness without forcing a model load here; loading (and the
    // first-run download) is triggered explicitly via the prepare endpoint or
    // lazily by the first search/index request.
    let engine_ready = state.embedding_engine_initialized().await;
    let status = state.db.get_embedding_status().await?;

    Ok(Json(EmbeddingStatusResponse {
        enabled: status.enabled,
        model: status.model,
        provider: status.provider,
        model_version: status.model_version,
        dimension: status.dimension,
        reindex_required: status.reindex_required,
        engine_ready,
        error: None,
        total_frames: status.total_frames,
        frames_with_embeddings: status.frames_with_embeddings,
        coverage_percent: status.coverage_percent,
        last_processed_frame_id: status.last_processed_frame_id,
    }))
}

/// POST /embeddings/models/prepare
/// Load (and, on first run, download + cache) the in-process embedding model so
/// later search/index requests don't pay the cold-start cost.
pub async fn prepare_quality_models(
    State(state): State<Arc<AppState>>,
) -> Result<Json<EmbeddingStatusResponse>> {
    state
        .get_embedding_engine()
        .await
        .map_err(crate::error::AppError::Internal)?;
    get_embedding_status(State(state)).await
}

/// POST /embeddings/generate
/// Kick off embedding generation for frames without embeddings.
///
/// This returns immediately and runs the (CPU-bound, potentially multi-minute)
/// work on a background task so the request — and the UI — never block. Progress
/// is observed via `GET /embeddings/status` (coverage climbs as frames finish).
pub async fn generate_embeddings(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GenerateEmbeddingsRequest>,
) -> Result<Json<GenerateEmbeddingsResponse>> {
    let batch_size = payload.batch_size.unwrap_or(50);
    debug!("Triggering embedding generation for {} frames", batch_size);

    // Cheap pre-check so the common "nothing to do" case answers instantly.
    let pending = state.db.get_frames_without_embeddings(1).await?;
    if pending.is_empty() {
        return Ok(Json(GenerateEmbeddingsResponse {
            success: true,
            message: "All frames already have embeddings".to_string(),
            frames_processed: 0,
        }));
    }

    // Refuse to stack overlapping jobs; one is already draining the backlog.
    if EMBEDDING_JOB_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(Json(GenerateEmbeddingsResponse {
            success: true,
            message: "Embedding generation is already running in the background".to_string(),
            frames_processed: 0,
        }));
    }

    let job_state = Arc::clone(&state);
    tokio::spawn(async move {
        // Reset the guard via Drop so the flag is cleared even if the job panics
        // or the task is cancelled — otherwise it would block all future jobs.
        struct JobGuard;
        impl Drop for JobGuard {
            fn drop(&mut self) {
                EMBEDDING_JOB_RUNNING.store(false, Ordering::SeqCst);
            }
        }
        let _guard = JobGuard;

        let processed = run_embedding_job(&job_state, batch_size).await;
        info!(
            "Background embedding job finished: {} frame(s) processed",
            processed
        );
    });

    Ok(Json(GenerateEmbeddingsResponse {
        success: true,
        message: "Embedding generation started in the background".to_string(),
        frames_processed: 0,
    }))
}

/// Embed up to `batch_size` frames that lack embeddings. Chunks are batched
/// across frames into a single `embed_batch` call (one model lock / blocking-pool
/// hop instead of one per frame), then sliced back per frame for atomic storage.
/// Returns the number of frames stored.
async fn run_embedding_job(state: &Arc<AppState>, batch_size: i64) -> i64 {
    let frames = match state.db.get_frames_without_embeddings(batch_size).await {
        Ok(f) => f,
        Err(error) => {
            warn!("Failed to fetch frames for embedding: {}", error);
            return 0;
        }
    };
    if frames.is_empty() {
        return 0;
    }

    let engine = match state.get_embedding_engine().await {
        Ok(e) => e,
        Err(e) => {
            warn!("Failed to initialize embedding engine: {}", e);
            return 0;
        }
    };

    // 1. Build per-frame chunk lists, dropping empty chunks so the flattened
    //    order stays aligned with `embed_batch`'s output (which skips blanks).
    let mut per_frame: Vec<(i64, Vec<String>)> = Vec::new();
    for frame in &frames {
        let ocr_texts = match state.db.get_ocr_text_for_frame(frame.id).await {
            Ok(texts) => texts,
            Err(error) => {
                warn!("Failed to load OCR text for frame {}: {}", frame.id, error);
                continue;
            }
        };
        if ocr_texts.is_empty() {
            continue;
        }

        let ocr_text: String = ocr_texts
            .iter()
            .map(|o| o.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let combined_text =
            crate::workers::embedding_worker::build_frame_embedding_text(frame, &ocr_text);
        let chunks = match engine.chunk_text(&combined_text, 512, 64).await {
            Ok(chunks) => chunks,
            Err(error) => {
                warn!("Failed to chunk frame {}: {}", frame.id, error);
                continue;
            }
        };
        let chunks: Vec<String> = chunks
            .into_iter()
            .filter(|c| !c.trim().is_empty())
            .collect();
        if !chunks.is_empty() {
            per_frame.push((frame.id, chunks));
        }
    }

    if per_frame.is_empty() {
        return 0;
    }

    // 2. Embed in groups that bound each `embed_batch` to ~MAX_CHUNKS_PER_CALL
    //    chunks. This batches several small frames per call (fewer model-lock /
    //    blocking-pool hops than one-frame-at-a-time) without holding the shared
    //    model lock for the whole backlog, which would starve search-time query
    //    embedding.
    const MAX_CHUNKS_PER_CALL: usize = 64;
    let mut processed = 0;
    let mut i = 0;
    while i < per_frame.len() {
        // Accumulate frames until the chunk budget would be exceeded (always
        // take at least one frame, even if it alone exceeds the budget).
        let mut group_end = i;
        let mut chunk_count = 0;
        while group_end < per_frame.len()
            && (group_end == i || chunk_count + per_frame[group_end].1.len() <= MAX_CHUNKS_PER_CALL)
        {
            chunk_count += per_frame[group_end].1.len();
            group_end += 1;
        }
        let group = &per_frame[i..group_end];
        i = group_end;

        let flat: Vec<&str> = group
            .iter()
            .flat_map(|(_, chunks)| chunks.iter().map(String::as_str))
            .collect();
        let embeddings = match engine.embed_batch(&flat).await {
            Ok(e) => e,
            Err(error) => {
                warn!("Failed to embed batch of {} chunks: {}", flat.len(), error);
                continue;
            }
        };
        if embeddings.len() != flat.len() {
            // Defensive: a length mismatch would corrupt the per-frame slicing.
            warn!(
                "Embedding count {} != chunk count {}; skipping group",
                embeddings.len(),
                flat.len()
            );
            continue;
        }

        // Slice the flat embeddings back per frame and store atomically.
        let mut cursor = 0usize;
        for (frame_id, chunks) in group {
            let slice = &embeddings[cursor..cursor + chunks.len()];
            cursor += chunks.len();

            let new_embeddings = chunks
                .iter()
                .zip(slice.iter())
                .enumerate()
                .map(
                    |(chunk_index, (chunk_text, embedding))| screensearch_db::NewEmbedding {
                        frame_id: *frame_id,
                        chunk_text: chunk_text.clone(),
                        chunk_index: chunk_index as i32,
                        embedding: embedding.clone(),
                        provider: engine.provider().to_string(),
                        model: engine.model().to_string(),
                        model_version: engine.model_version().to_string(),
                        content_hash: screensearch_embeddings::EmbeddingEngine::content_hash(
                            chunk_text,
                        ),
                    },
                )
                .collect();

            if let Err(error) = state.db.insert_embeddings(new_embeddings).await {
                warn!(
                    "Failed to atomically store embeddings for frame {}: {}",
                    frame_id, error
                );
                continue;
            }

            processed += 1;
            let _ = state
                .db
                .set_metadata("embeddings_last_processed_frame_id", &frame_id.to_string())
                .await;
        }
    }

    if processed > 0
        && state
            .db
            .get_frames_without_embeddings(1)
            .await
            .map(|f| f.is_empty())
            .unwrap_or(false)
    {
        let _ = state
            .db
            .set_metadata("embeddings_reindex_required", "false")
            .await;
    }

    processed
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
