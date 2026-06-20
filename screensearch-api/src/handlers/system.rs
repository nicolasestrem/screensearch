//! System management endpoint handlers

use crate::error::{AppError, Result};
use crate::handlers::ai::{
    load_ai_provider_config, AI_API_KEY_KEY, AI_MODEL_KEY, AI_PROVIDER_URL_KEY,
};
use crate::models::{AddTagToFrameRequest, CreateTagRequest, HealthResponse};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use regex::Regex;
use screensearch_db::models::TestVisionRequest;
use screensearch_db::{NewTag, Pagination, SettingsRecord, UpdateSettings};
use screensearch_vision::client::OllamaClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::LazyLock;
use tracing::{debug, error};

// Validation constants
const MAX_TAG_NAME_LEN: usize = 200;
const MAX_TAG_DESC_LEN: usize = 1000;

// Compile regex once at startup
static HEX_COLOR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#([0-9A-Fa-f]{6}|[0-9A-Fa-f]{8})$").unwrap());

/// Validate hex color format
fn validate_hex_color(color: &str) -> Result<()> {
    if !HEX_COLOR_REGEX.is_match(color) {
        return Err(AppError::InvalidRequest(
            "Color must be a valid hex code (#RRGGBB or #RRGGBBAA)".to_string(),
        ));
    }
    Ok(())
}

/// GET /health - Health check endpoint
///
/// Returns server health status and statistics.
pub async fn health(State(state): State<Arc<AppState>>) -> Result<Json<HealthResponse>> {
    debug!("Health check request");

    // Get database statistics
    let stats = match state.db.get_statistics().await {
        Ok(stats) => stats,
        Err(e) => {
            error!("Failed to get database statistics: {}", e);
            return Err(AppError::Database(e));
        }
    };

    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: None, // TODO: Track server uptime
        frame_count: stats.frame_count,
        ocr_count: stats.ocr_count,
        tag_count: stats.tag_count,
        oldest_frame: stats.oldest_frame,
        newest_frame: stats.newest_frame,
    }))
}

/// One subsystem's startup/readiness state, in plain language for the UI.
#[derive(Debug, serde::Serialize)]
pub struct ReadinessStage {
    /// Stable id: "core" | "search_index" | "answer_generation".
    pub id: String,
    /// Short human label.
    pub label: String,
    /// State machine value the UI styles on:
    /// "ready" | "initializing" | "loading" | "downloading" | "needs_setup" | "disabled".
    /// `initializing`/`loading`/`downloading` are *transitional* — the UI keeps the
    /// startup banner up while any stage is in one of them.
    pub state: String,
    /// One sentence explaining what's happening / what to do.
    pub detail: String,
    /// Download progress 0–100 (only for `downloading`).
    pub progress: Option<f64>,
    /// Estimated seconds remaining (only for `downloading`).
    pub eta_seconds: Option<u64>,
}

/// Aggregated startup readiness across subsystems.
#[derive(Debug, serde::Serialize)]
pub struct ReadinessResponse {
    /// Core (DB + capture/OCR/search) is up — the app is usable for search.
    pub core_ready: bool,
    /// No subsystem is still in a transitional (timed) warm-up state.
    pub all_ready: bool,
    pub stages: Vec<ReadinessStage>,
}

fn is_transitional(state: &str) -> bool {
    matches!(state, "initializing" | "loading" | "downloading")
}

/// GET /system/readiness
///
/// One call the UI polls at startup to explain, in plain language, what the
/// backend is doing and roughly how long until each capability is usable: core
/// services (DB/capture/OCR/search), the semantic-search model, and local AI
/// answer generation (server/model download + load). Designed to be cheap and
/// non-blocking so it can be polled once a second during warm-up.
pub async fn get_readiness(State(state): State<Arc<AppState>>) -> Result<Json<ReadinessResponse>> {
    let mut stages: Vec<ReadinessStage> = Vec::new();

    // --- Core (database + API + capture/OCR pipeline) ---
    let core_ready = state.db.get_statistics().await.is_ok();
    stages.push(ReadinessStage {
        id: "core".into(),
        label: "Core services".into(),
        state: if core_ready { "ready" } else { "initializing" }.into(),
        detail: if core_ready {
            "Capture, OCR, and keyword search are running.".into()
        } else {
            "Starting the database and capture pipeline…".into()
        },
        progress: None,
        eta_seconds: None,
    });

    // Tracked downloads (shared by the AI stage's messaging).
    let downloads = state.get_all_download_progress().await;

    // --- Semantic search (in-process embedding model) ---
    {
        let status = state.db.get_embedding_status().await.ok();
        let indexing_on = status.as_ref().map(|s| s.enabled).unwrap_or(false);
        let coverage = status.as_ref().map(|s| s.coverage_percent).unwrap_or(0.0);
        let engine_ready = state.embedding_engine_initialized().await;

        // Only treat the search model as "warming up" when indexing is actually
        // on. With semantic search OFF the engine is never initialized, so
        // keying the state on `engine_ready` alone would pin the banner to
        // "Loading the search model…" forever even though nothing is
        // downloading. Report "disabled" (non-transitional) in that case so the
        // banner hides it.
        let (st, detail, progress, eta): (&str, String, Option<f64>, Option<u64>) = if !indexing_on
        {
            (
                "disabled",
                "Semantic search is off — enable it in Settings → Data & AI.".to_string(),
                None,
                None,
            )
        } else if engine_ready {
            (
                "ready",
                format!("Semantic search ready ({coverage:.0}% of frames indexed)."),
                None,
                None,
            )
        } else if let Some(p) = downloads.get("embedding_model") {
            (
                "downloading",
                "Downloading the search model… (~300 MB, one-time).".to_string(),
                Some(p.percentage()),
                Some(p.eta_seconds),
            )
        } else {
            (
                "loading",
                "Loading the search model into memory…".to_string(),
                None,
                None,
            )
        };
        stages.push(ReadinessStage {
            id: "search_index".into(),
            label: "Semantic search".into(),
            state: st.into(),
            detail,
            progress,
            eta_seconds: eta,
        });
    }

    // --- Local AI answer generation (vision + RAG via llama-server) ---
    {
        let settings = state.db.get_settings().await.ok();
        let vision_enabled = settings
            .as_ref()
            .map(|s| s.vision_enabled != 0)
            .unwrap_or(false);
        let provider = settings
            .as_ref()
            .map(|s| s.vision_provider.clone())
            .unwrap_or_default();
        let vision_model = settings
            .as_ref()
            .map(|s| s.vision_model.clone())
            .unwrap_or_default();

        // The local path needs filesystem checks (binary version marker, scan of
        // .models/). Run them off the async executor so this frequently-polled
        // handler never blocks the runtime on blocking syscalls.
        let (server_ready, vision_present) = if vision_enabled && provider == "local" {
            let vm = vision_model.clone();
            tokio::task::spawn_blocking(move || {
                (
                    screensearch_llm::llama_server_up_to_date(&screensearch_llm::get_bin_dir()),
                    screensearch_llm::resolve_vision_model(&vm).is_some(),
                )
            })
            .await
            .unwrap_or((false, false))
        } else {
            (false, false)
        };

        let (st, detail, progress, eta): (String, String, Option<f64>, Option<u64>) =
            if !vision_enabled {
                (
                    "disabled".into(),
                    "Answer Generation is off — turn it on in Settings → Data & AI.".into(),
                    None,
                    None,
                )
            } else if provider != "local" {
                (
                    "ready".into(),
                    format!("Using your configured provider ({provider})."),
                    None,
                    None,
                )
            } else if let Some(p) = downloads.get("llama_server") {
                (
                    "downloading".into(),
                    "Downloading the local AI server…".into(),
                    Some(p.percentage()),
                    Some(p.eta_seconds),
                )
            } else if let Some(p) = downloads.get("llm_model") {
                (
                    "downloading".into(),
                    "Downloading the local AI model…".into(),
                    Some(p.percentage()),
                    Some(p.eta_seconds),
                )
            } else if !server_ready {
                (
                    "needs_setup".into(),
                    "Download the local AI server in Settings → Data & AI.".into(),
                    None,
                    None,
                )
            } else if !vision_present {
                (
                    "needs_setup".into(),
                    "Add a Gemma 4 model and its *mmproj*.gguf to the .models/ folder.".into(),
                    None,
                    None,
                )
            } else {
                // Binary + model are present: reflect the server's runtime state.
                // `try_read` so a held write lock (server (re)building/shutting
                // down) reports a transitional state instead of blocking this
                // frequently-polled handler.
                match state.llama_server.try_read() {
                    Ok(guard) => match guard.as_ref() {
                        Some(server) => {
                            let status = server.status().await;
                            match status.as_str() {
                                "running" => {
                                    let accel = match server.gpu_active().await {
                                        Some(true) => "GPU (Vulkan)",
                                        Some(false) => "CPU",
                                        None => "your device",
                                    };
                                    (
                                        "ready".into(),
                                        format!("Local model loaded on {accel}."),
                                        None,
                                        None,
                                    )
                                }
                                "starting" => (
                                    "loading".into(),
                                    "Loading the model into memory (first request can take a few seconds)…"
                                        .into(),
                                    None,
                                    None,
                                ),
                                _ => (
                                    "ready".into(),
                                    "Ready — the model loads on the first request.".into(),
                                    None,
                                    None,
                                ),
                            }
                        }
                        None => (
                            "ready".into(),
                            "Ready — the model loads on the first request.".into(),
                            None,
                            None,
                        ),
                    },
                    Err(_) => (
                        "loading".into(),
                        "Initializing the local AI server…".into(),
                        None,
                        None,
                    ),
                }
            };

        stages.push(ReadinessStage {
            id: "answer_generation".into(),
            label: "AI answer generation".into(),
            state: st,
            detail,
            progress,
            eta_seconds: eta,
        });
    }

    let all_ready = core_ready && !stages.iter().any(|s| is_transitional(&s.state));

    Ok(Json(ReadinessResponse {
        core_ready,
        all_ready,
        stages,
    }))
}

/// POST /tags - Create a new tag
///
/// Creates a new tag that can be applied to frames.
///
/// # Request Body
/// - tag_name: Name of the tag
/// - description: Optional tag description
/// - color: Optional color code (e.g., "#FF0000")
pub async fn create_tag(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTagRequest>,
) -> Result<Json<crate::models::TagResponse>> {
    debug!("Create tag request: {}", req.tag_name);

    // Validation: empty check
    if req.tag_name.trim().is_empty() {
        return Err(AppError::InvalidRequest(
            "Tag name cannot be empty".to_string(),
        ));
    }

    // Validation: name length
    if req.tag_name.trim().len() > MAX_TAG_NAME_LEN {
        return Err(AppError::InvalidRequest(format!(
            "Tag name must be <= {} characters",
            MAX_TAG_NAME_LEN
        )));
    }

    // Validation: description length
    if let Some(ref desc) = req.description {
        if desc.len() > MAX_TAG_DESC_LEN {
            return Err(AppError::InvalidRequest(format!(
                "Description must be <= {} characters",
                MAX_TAG_DESC_LEN
            )));
        }
    }

    // Validation: hex color format
    if let Some(ref color) = req.color {
        validate_hex_color(color)?;
    }

    // Check if tag already exists
    if let Ok(Some(_)) = state.db.get_tag_by_name(&req.tag_name).await {
        return Err(AppError::InvalidRequest(format!(
            "Tag '{}' already exists",
            req.tag_name
        )));
    }

    let new_tag = NewTag {
        tag_name: req.tag_name.trim().to_string(),
        description: req.description,
        color: req.color,
    };

    let tag_id = match state.db.create_tag(new_tag).await {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to create tag: {}", e);
            return Err(AppError::Database(e));
        }
    };

    // Retrieve the created tag
    match state.db.get_tag(tag_id).await {
        Ok(Some(tag)) => {
            debug!("Created tag: {} (id={})", tag.tag_name, tag.id);
            // Convert to TagResponse
            Ok(Json(crate::models::TagResponse {
                id: tag.id,
                name: tag.tag_name,
                color: tag.color,
                created_at: tag.created_at,
            }))
        }
        Ok(None) => Err(AppError::Internal("Tag created but not found".to_string())),
        Err(e) => {
            error!("Failed to retrieve created tag: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// GET /tags - List all tags
///
/// Returns all available tags with pagination.
///
/// # Query Parameters
/// - limit: Maximum tags to return (default: 100)
/// - offset: Number of tags to skip (default: 0)
pub async fn list_tags(
    State(state): State<Arc<AppState>>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<Vec<crate::models::TagResponse>>> {
    debug!("List tags request");

    match state.db.list_tags(pagination).await {
        Ok(tags) => {
            debug!("Retrieved {} tags", tags.len());
            // Convert TagRecord to TagResponse (tag_name -> name)
            let response_tags = tags
                .into_iter()
                .map(|t| crate::models::TagResponse {
                    id: t.id,
                    name: t.tag_name,
                    color: t.color,
                    created_at: t.created_at,
                })
                .collect();
            Ok(Json(response_tags))
        }
        Err(e) => {
            error!("Failed to list tags: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// PUT /tags/:id - Update a tag
///
/// Updates an existing tag's name and/or color.
///
/// # Path Parameters
/// - id: Tag ID to update
///
/// # Request Body
/// - tag_name: New name for the tag
/// - color: New color code (optional)
pub async fn update_tag(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(req): Json<CreateTagRequest>,
) -> Result<Json<crate::models::TagResponse>> {
    debug!("Update tag request: id={}", id);

    // Validation: empty check
    if req.tag_name.trim().is_empty() {
        return Err(AppError::InvalidRequest(
            "Tag name cannot be empty".to_string(),
        ));
    }

    // Validation: name length
    if req.tag_name.trim().len() > MAX_TAG_NAME_LEN {
        return Err(AppError::InvalidRequest(format!(
            "Tag name must be <= {} characters",
            MAX_TAG_NAME_LEN
        )));
    }

    // Validation: description length
    if let Some(ref desc) = req.description {
        if desc.len() > MAX_TAG_DESC_LEN {
            return Err(AppError::InvalidRequest(format!(
                "Description must be <= {} characters",
                MAX_TAG_DESC_LEN
            )));
        }
    }

    // Validation: hex color format
    if let Some(ref color) = req.color {
        validate_hex_color(color)?;
    }

    // Check if tag exists
    match state.db.get_tag(id).await {
        Ok(Some(_)) => {}
        Ok(None) => return Err(AppError::NotFound(format!("Tag with id {} not found", id))),
        Err(e) => {
            error!("Failed to check tag existence: {}", e);
            return Err(AppError::Database(e));
        }
    }

    // Check if new name conflicts with existing tag (excluding current tag)
    if let Ok(Some(existing)) = state.db.get_tag_by_name(&req.tag_name).await {
        if existing.id != id {
            return Err(AppError::InvalidRequest(format!(
                "Tag '{}' already exists",
                req.tag_name
            )));
        }
    }

    // Update the tag
    let new_tag = NewTag {
        tag_name: req.tag_name.trim().to_string(),
        description: req.description,
        color: req.color,
    };

    match state.db.update_tag(id, new_tag).await {
        Ok(_) => {}
        Err(e) => {
            error!("Failed to update tag: {}", e);
            return Err(AppError::Database(e));
        }
    }

    // Retrieve the updated tag
    match state.db.get_tag(id).await {
        Ok(Some(tag)) => {
            debug!("Updated tag: {} (id={})", tag.tag_name, tag.id);
            // Convert to TagResponse
            Ok(Json(crate::models::TagResponse {
                id: tag.id,
                name: tag.tag_name,
                color: tag.color,
                created_at: tag.created_at,
            }))
        }
        Ok(None) => Err(AppError::Internal("Tag updated but not found".to_string())),
        Err(e) => {
            error!("Failed to retrieve updated tag: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// DELETE /tags/:id - Delete a tag
///
/// Deletes a tag by ID. This also removes all associations with frames.
///
/// # Path Parameters
/// - id: Tag ID to delete
pub async fn delete_tag(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    debug!("Delete tag request: id={}", id);

    // Check if tag exists
    match state.db.get_tag(id).await {
        Ok(Some(_)) => {}
        Ok(None) => return Err(AppError::NotFound(format!("Tag with id {} not found", id))),
        Err(e) => {
            error!("Failed to check tag existence: {}", e);
            return Err(AppError::Database(e));
        }
    }

    // Delete tag
    match state.db.delete_tag(id).await {
        Ok(affected) => {
            debug!("Deleted tag: id={}, rows_affected={}", id, affected);
            Ok(Json(serde_json::json!({
                "success": true,
                "message": format!("Tag {} deleted", id)
            })))
        }
        Err(e) => {
            error!("Failed to delete tag: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// POST /frames/:id/tags - Add tag to frame
///
/// Associates a tag with a frame.
///
/// # Path Parameters
/// - id: Frame ID
///
/// # Request Body
/// - tag_id: ID of the tag to add
pub async fn add_tag_to_frame(
    State(state): State<Arc<AppState>>,
    Path(frame_id): Path<i64>,
    Json(req): Json<AddTagToFrameRequest>,
) -> Result<Json<serde_json::Value>> {
    debug!(
        "Add tag to frame: frame_id={}, tag_id={}",
        frame_id, req.tag_id
    );

    // Rely on database foreign key constraints for validation (performance optimization)
    // This reduces 3 queries to 1 query
    match state.db.add_tag_to_frame(frame_id, req.tag_id).await {
        Ok(_) => {
            debug!("Added tag {} to frame {}", req.tag_id, frame_id);
            Ok(Json(serde_json::json!({
                "success": true,
                "message": format!("Tag {} added to frame {}", req.tag_id, frame_id)
            })))
        }
        Err(e) => {
            let error_msg = e.to_string();
            // Check if error is due to foreign key constraint violation
            if error_msg.contains("FOREIGN KEY") || error_msg.contains("foreign key") {
                error!(
                    "Foreign key constraint violated: frame_id={}, tag_id={}",
                    frame_id, req.tag_id
                );
                Err(AppError::NotFound("Frame or tag not found".to_string()))
            } else if error_msg.contains("UNIQUE") {
                // Tag already assigned to frame (duplicate)
                debug!("Tag {} already assigned to frame {}", req.tag_id, frame_id);
                Ok(Json(serde_json::json!({
                    "success": true,
                    "message": format!("Tag {} already assigned to frame {}", req.tag_id, frame_id)
                })))
            } else {
                error!("Failed to add tag to frame: {}", e);
                Err(AppError::Database(e))
            }
        }
    }
}

/// DELETE /frames/:id/tags/:tag_id - Remove tag from frame
///
/// Removes a tag association from a frame.
///
/// # Path Parameters
/// - id: Frame ID
/// - tag_id: Tag ID to remove
pub async fn remove_tag_from_frame(
    State(state): State<Arc<AppState>>,
    Path((frame_id, tag_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>> {
    debug!(
        "Remove tag from frame: frame_id={}, tag_id={}",
        frame_id, tag_id
    );

    match state.db.remove_tag_from_frame(frame_id, tag_id).await {
        Ok(affected) => {
            if affected == 0 {
                return Err(AppError::NotFound(format!(
                    "Tag {} not associated with frame {}",
                    tag_id, frame_id
                )));
            }
            debug!(
                "Removed tag {} from frame {}, rows_affected={}",
                tag_id, frame_id, affected
            );
            Ok(Json(serde_json::json!({
                "success": true,
                "message": format!("Tag {} removed from frame {}", tag_id, frame_id)
            })))
        }
        Err(e) => {
            error!("Failed to remove tag from frame: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// GET /frames/:id/tags - Get tags for a frame
///
/// Returns all tags associated with a frame.
///
/// # Path Parameters
/// - id: Frame ID
pub async fn get_frame_tags(
    State(state): State<Arc<AppState>>,
    Path(frame_id): Path<i64>,
) -> Result<Json<Vec<crate::models::TagResponse>>> {
    debug!("Get frame tags: frame_id={}", frame_id);

    // Verify frame exists
    match state.db.get_frame(frame_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err(AppError::NotFound(format!(
                "Frame with id {} not found",
                frame_id
            )))
        }
        Err(e) => return Err(AppError::Database(e)),
    }

    match state.db.get_tags_for_frame(frame_id).await {
        Ok(tags) => {
            debug!("Retrieved {} tags for frame {}", tags.len(), frame_id);
            // Convert TagRecord to TagResponse
            let response_tags = tags
                .into_iter()
                .map(|t| crate::models::TagResponse {
                    id: t.id,
                    name: t.tag_name,
                    color: t.color,
                    created_at: t.created_at,
                })
                .collect();
            Ok(Json(response_tags))
        }
        Err(e) => {
            error!("Failed to get frame tags: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// Settings as exposed to the UI: the persisted [`SettingsRecord`] plus the AI
/// report-provider config. The provider config is stored as database metadata
/// (not `SettingsRecord` columns), so it is merged in here rather than living
/// only in the browser's local storage.
#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    #[serde(flatten)]
    pub base: SettingsRecord,
    pub ai_provider_url: String,
    pub ai_model: String,
    pub ai_api_key: Option<String>,
}

/// Settings update payload: the base settings plus the optional AI provider
/// config. The AI fields are optional so older clients that omit them leave the
/// persisted provider untouched.
#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    #[serde(flatten)]
    pub base: UpdateSettings,
    #[serde(default)]
    pub ai_provider_url: Option<String>,
    #[serde(default)]
    pub ai_model: Option<String>,
    #[serde(default)]
    pub ai_api_key: Option<String>,
}

/// Merge a `SettingsRecord` with the persisted AI provider config.
async fn build_settings_response(state: &AppState, base: SettingsRecord) -> SettingsResponse {
    let (ai_provider_url, ai_model, ai_api_key) = load_ai_provider_config(&state.db).await;
    SettingsResponse {
        base,
        ai_provider_url,
        ai_model,
        ai_api_key,
    }
}

/// GET /settings - Get application settings
///
/// Returns current application settings (including the persisted AI provider).
pub async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Json<SettingsResponse>> {
    debug!("Get settings request");

    match state.db.get_settings().await {
        Ok(settings) => {
            debug!("Retrieved settings");
            Ok(Json(build_settings_response(&state, settings).await))
        }
        Err(e) => {
            error!("Failed to get settings: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// POST /settings - Update application settings
///
/// Updates application settings.
///
/// # Request Body
/// - capture_interval: Capture interval in seconds
/// - monitors: JSON array of monitor indices
/// - excluded_apps: JSON array of excluded application names
/// - is_paused: Whether capture is paused (0/1)
/// - retention_days: Number of days to retain data
pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<SettingsResponse>> {
    debug!("Update settings request");

    let UpdateSettingsRequest {
        base: settings,
        ai_provider_url,
        ai_model,
        ai_api_key,
    } = req;

    // Validate settings
    if settings.capture_interval < 1 {
        return Err(AppError::InvalidRequest(
            "Capture interval must be at least 1 second".to_string(),
        ));
    }

    if settings.retention_days < 1 {
        return Err(AppError::InvalidRequest(
            "Retention days must be at least 1 day".to_string(),
        ));
    }

    match state.db.update_settings(settings).await {
        Ok(updated_settings) => {
            debug!("Settings updated successfully");
            // Update shared state for capture loop
            state.capture_interval_ms.store(
                updated_settings.capture_interval as u64 * 1000,
                std::sync::atomic::Ordering::Relaxed,
            );
            // Push the new monitor selection to the capture engine so it can
            // reconfigure live. `[]` (or unparseable) means "all monitors".
            let monitors: Vec<usize> =
                serde_json::from_str(&updated_settings.monitors).unwrap_or_default();
            let _ = state.monitor_config_tx.send(monitors);

            // Persist the AI report-provider config (metadata, not columns) when
            // the client supplied it. Empty string clears the value.
            if let Some(v) = ai_provider_url {
                state
                    .db
                    .set_metadata(AI_PROVIDER_URL_KEY, &v)
                    .await
                    .map_err(AppError::Database)?;
            }
            if let Some(v) = ai_model {
                state
                    .db
                    .set_metadata(AI_MODEL_KEY, &v)
                    .await
                    .map_err(AppError::Database)?;
            }
            if let Some(v) = ai_api_key {
                state
                    .db
                    .set_metadata(AI_API_KEY_KEY, &v)
                    .await
                    .map_err(AppError::Database)?;
            }

            Ok(Json(
                build_settings_response(&state, updated_settings).await,
            ))
        }
        Err(e) => {
            error!("Failed to update settings: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// A monitor as exposed to the frontend monitor picker.
#[derive(Debug, serde::Serialize)]
pub struct MonitorInfoDto {
    /// 0-based index — matches the order the capture engine uses.
    pub index: usize,
    /// Human-readable label, e.g. "Monitor 1 (3440x1440) — Primary".
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

/// GET /monitors - List connected monitors
///
/// Enumerates monitors via the same `screenshots::Screen::all()` source the
/// capture engine indexes, so the returned indices line up with what is
/// actually captured. Returns an empty list if enumeration fails (e.g. a
/// transient display hotplug) rather than erroring the UI.
pub async fn list_monitors() -> Result<Json<Vec<MonitorInfoDto>>> {
    debug!("List monitors request");

    // Screen enumeration is a blocking platform call; keep it off the runtime.
    let monitors = tokio::task::spawn_blocking(|| {
        let screens = screenshots::Screen::all().unwrap_or_default();
        screens
            .into_iter()
            .enumerate()
            .map(|(index, screen)| {
                let info = screen.display_info;
                let label = format!(
                    "Monitor {} ({}x{}){}",
                    index + 1,
                    info.width,
                    info.height,
                    if info.is_primary { " — Primary" } else { "" }
                );
                MonitorInfoDto {
                    index,
                    label,
                    width: info.width,
                    height: info.height,
                    is_primary: info.is_primary,
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();

    Ok(Json(monitors))
}

/// POST /api/test-vision - Test vision configuration
pub async fn test_vision_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TestVisionRequest>,
) -> Result<Json<serde_json::Value>> {
    debug!(
        "Test vision config: provider={}, model={}",
        req.provider, req.model
    );

    // Handle local provider specially - check llama-server health
    if req.provider == "local" {
        // Accept any user-provided GGUF discovered in `.models/` (etc.), not just
        // the downloadable default model.
        if !screensearch_llm::local_model_available() {
            return Ok(Json(serde_json::json!({
                "success": false,
                "message": "No local model found. Drop a GGUF into .models/ or download the default model first."
            })));
        }

        // Check if llama-server is running by hitting its health endpoint on the
        // port it actually bound (it falls back to 31131/31132 if 31130 is busy).
        let port = match state.llama_server.try_read() {
            Ok(guard) => match guard.as_ref() {
                Some(server) => server.active_port().await,
                None => screensearch_llm::DEFAULT_LLAMA_PORT,
            },
            Err(_) => screensearch_llm::DEFAULT_LLAMA_PORT,
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        match client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await
        {
            Ok(res) if res.status().is_success() => {
                return Ok(Json(serde_json::json!({
                    "success": true,
                    "message": "Local model ready. llama-server is running."
                })));
            }
            _ => {
                return Ok(Json(serde_json::json!({
                    "success": false,
                    "message": "Model downloaded but llama-server not running. Click 'Start Server' first."
                })));
            }
        }
    }

    // For ollama/openai providers, use OllamaClient
    let client = OllamaClient::new(req.endpoint, req.model, req.api_key, req.provider);

    match client
        .generate_text("Test connection. Reply with 'OK'.", None)
        .await
    {
        Ok(response) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Connection successful",
            "response": response
        }))),
        Err(e) => {
            error!("Vision test failed: {}", e);
            Ok(Json(serde_json::json!({
                "success": false,
                "message": format!("Connection failed: {}", e)
            })))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_tag_name_validation() {
        assert!("".trim().is_empty());
        assert!(!"valid_tag".trim().is_empty());
        assert!("  ".trim().is_empty());
    }

    #[test]
    fn test_url_validation() {
        assert!("http://example.com".starts_with("http://"));
        assert!("https://example.com".starts_with("https://"));
        assert!(!"example.com".starts_with("http://"));
    }
}
