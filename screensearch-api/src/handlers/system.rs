//! System management endpoint handlers

use crate::error::{AppError, Result};
use crate::models::{AddTagToFrameRequest, CreateTagRequest, HealthResponse};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use regex::Regex;
use screensearch_db::{NewTag, Pagination, SettingsRecord, UpdateSettings};
use screensearch_db::models::TestVisionRequest;
use screensearch_vision::client::OllamaClient;
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

/// GET /settings - Get application settings
///
/// Returns current application settings.
pub async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Json<SettingsRecord>> {
    debug!("Get settings request");

    match state.db.get_settings().await {
        Ok(settings) => {
            debug!("Retrieved settings");
            Ok(Json(settings))
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
    Json(settings): Json<UpdateSettings>,
) -> Result<Json<SettingsRecord>> {
    debug!("Update settings request");

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
                std::sync::atomic::Ordering::Relaxed
            );
            Ok(Json(updated_settings))
        }
        Err(e) => {
            error!("Failed to update settings: {}", e);
            Err(AppError::Database(e))
        }
    }
}



/// POST /api/test-vision - Test vision configuration
pub async fn test_vision_config(
    Json(req): Json<TestVisionRequest>,
) -> Result<Json<serde_json::Value>> {
    debug!("Test vision config: provider={}, model={}", req.provider, req.model);

    let client = OllamaClient::new(
        req.endpoint,
        req.model,
        req.api_key,
        req.provider,
    );

    match client.generate_text("Test connection. Reply with 'OK'.", None).await {
        Ok(response) => {
            Ok(Json(serde_json::json!({
                "success": true,
                "message": "Connection successful",
                "response": response
            })))
        },
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
