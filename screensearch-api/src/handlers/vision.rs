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
use serde::Serialize;
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

/// A locally discovered vision-capable model (a GGUF with a matching `mmproj`
/// projector beside it) that the user can select for the local provider.
#[derive(Debug, Serialize)]
pub struct VisionModelEntry {
    /// Filename stem, used as the `vision_model` setting value (drives
    /// `resolve_vision_model`'s preferred match).
    pub id: String,
    /// Model GGUF filename (for display).
    pub model_file: String,
    /// Projector GGUF filename (for display).
    pub mmproj_file: String,
    /// Whether this is the model the server currently resolves to.
    pub selected: bool,
}

#[derive(Debug, Serialize)]
pub struct VisionModelsResponse {
    pub models: Vec<VisionModelEntry>,
    /// The id of the currently resolved model, if any.
    pub selected: Option<String>,
}

/// GET /api/vision/models
///
/// List the locally discovered vision-capable models (each a GGUF paired with an
/// `mmproj` projector in `.models/`) so the UI can offer a picker for the local
/// provider. The entry matching the current `vision_model` setting is flagged
/// `selected`.
pub async fn list_vision_models(
    State(state): State<Arc<AppState>>,
) -> Result<Json<VisionModelsResponse>> {
    let file_name = |p: &std::path::Path| {
        p.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    };
    let stem = |p: &std::path::Path| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    };

    // What the server would actually load given the current setting.
    let preferred = state
        .db
        .get_settings()
        .await
        .map(|s| s.vision_model)
        .unwrap_or_default();
    let selected_model = screensearch_llm::resolve_vision_model(&preferred).map(|(m, _)| stem(&m));

    let models = screensearch_llm::discover_vision_models()
        .into_iter()
        .map(|(model, mmproj)| {
            let id = stem(&model);
            VisionModelEntry {
                selected: Some(&id) == selected_model.as_ref(),
                model_file: file_name(&model),
                mmproj_file: file_name(&mmproj),
                id,
            }
        })
        .collect();

    Ok(Json(VisionModelsResponse {
        models,
        selected: selected_model,
    }))
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
