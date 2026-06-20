//! Vision analysis endpoint handlers
//!
//! On-demand frame analysis enqueue plus an aggregate status view. The actual
//! analysis is performed asynchronously by the vision worker
//! (`crate::workers::vision_worker`), which drives the unified local
//! llama-server (with `--mmproj`) or an external provider.

use crate::error::{AppError, Result};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use std::sync::Arc;
use tracing::{debug, info};

/// Priority given to user-requested (on-demand) analysis so it jumps ahead of
/// the background trickle (priority 0).
const ON_DEMAND_PRIORITY: i32 = 10;

/// POST /api/vision/analyze/:frame_id
///
/// Enqueue a single frame for vision analysis on demand. Returns the queue id
/// (0 if the frame was already queued).
pub async fn analyze_frame(
    State(state): State<Arc<AppState>>,
    Path(frame_id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    debug!("On-demand vision analysis requested for frame {}", frame_id);

    // Make sure the frame exists before queueing.
    let frame = state
        .db
        .get_frame(frame_id)
        .await
        .map_err(AppError::Database)?;
    if frame.is_none() {
        return Err(AppError::NotFound(format!("Frame {} not found", frame_id)));
    }

    let queue_id = state
        .db
        .enqueue_frame_for_analysis(frame_id, ON_DEMAND_PRIORITY)
        .await
        .map_err(AppError::Database)?;

    info!(
        "Frame {} enqueued for vision analysis (queue_id={})",
        frame_id, queue_id
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "frame_id": frame_id,
        "queue_id": queue_id,
        "already_queued": queue_id == 0,
    })))
}

/// GET /api/vision/status
///
/// Aggregate vision-analysis status: per-status frame counts, queue depth, and
/// the configured vision provider/model.
pub async fn get_vision_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<screensearch_db::models::VisionStatus>> {
    let status = state
        .db
        .get_vision_status()
        .await
        .map_err(AppError::Database)?;
    Ok(Json(status))
}
