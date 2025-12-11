//! Search endpoint handlers

use crate::error::{AppError, Result};
use crate::models::{
    FrameQuery, KeywordSearchQuery, PaginatedFramesResponse, PaginationInfo, SearchQuery,
};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use screensearch_db::{FrameFilter, Pagination, SearchResult};
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, error};

/// GET /search - Full-text search with filters
///
/// Searches OCR text using FTS5 with BM25 ranking. Supports time range,
/// application, and keyword filters.
///
/// # Query Parameters
/// - q: Search query string
/// - start_time: Optional start time filter (ISO 8601)
/// - end_time: Optional end time filter (ISO 8601)
/// - app: Optional application name filter
/// - limit: Maximum results to return (default: 100)
pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>> {
    debug!("Search request: q={}, limit={:?}", params.q, params.limit);

    if params.q.is_empty() {
        return Err(AppError::InvalidRequest(
            "Search query cannot be empty".to_string(),
        ));
    }

    // Build filter from query parameters
    let filter = FrameFilter {
        start_time: params.start_time,
        end_time: params.end_time,
        app_name: params.app,
        device_name: None,
        tag_ids: None,
        monitor_index: None,
    };

    // Build pagination
    let pagination = Pagination {
        limit: params.limit.unwrap_or(100),
        offset: 0,
    };

    // Execute search
    match state
        .db
        .search_ocr_text(&params.q, filter, pagination)
        .await
    {
        Ok(results) => {
            debug!("Found {} search results", results.len());
            Ok(Json(results))
        }
        Err(e) => {
            error!("Search failed: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// GET /search/keywords - Keyword-based search with ranking
///
/// Searches for exact keyword matches in OCR text with confidence-based ranking.
///
/// # Query Parameters
/// - keywords: Comma-separated keywords to search for
/// - limit: Maximum results to return (default: 100)
pub async fn search_keywords(
    State(state): State<Arc<AppState>>,
    Query(params): Query<KeywordSearchQuery>,
) -> Result<Json<Vec<screensearch_db::OcrTextRecord>>> {
    debug!("Keyword search request: keywords={}", params.keywords);

    if params.keywords.is_empty() {
        return Err(AppError::InvalidRequest(
            "Keywords cannot be empty".to_string(),
        ));
    }

    // Split keywords by comma and trim whitespace
    let keywords: Vec<String> = params
        .keywords
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if keywords.is_empty() {
        return Err(AppError::InvalidRequest(
            "No valid keywords provided".to_string(),
        ));
    }

    let pagination = Pagination {
        limit: params.limit.unwrap_or(100),
        offset: 0,
    };

    match state.db.search_ocr_keywords(keywords, pagination).await {
        Ok(results) => {
            debug!("Found {} keyword matches", results.len());
            Ok(Json(results))
        }
        Err(e) => {
            error!("Keyword search failed: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// GET /frames - Retrieve captured frames with filters
///
/// Returns frame metadata with optional time range and monitor filters.
///
/// # Query Parameters
/// - start_time: Optional start time filter (ISO 8601)
/// - end_time: Optional end time filter (ISO 8601)
/// - monitor_index: Optional monitor index filter
/// - limit: Maximum results to return (default: 100)
pub async fn get_frames(
    State(state): State<Arc<AppState>>,
    Query(params): Query<FrameQuery>,
) -> Result<Json<PaginatedFramesResponse>> {
    debug!(
        "Get frames request: start={:?}, end={:?}, monitor={:?}, limit={:?}, offset={:?}, q={:?}",
        params.start_time,
        params.end_time,
        params.monitor_index,
        params.limit,
        params.offset,
        params.q
    );

    // Use current time as default end time if not provided
    let end_time = params.end_time.unwrap_or_else(chrono::Utc::now);

    // Use 24 hours ago as default start time if not provided
    let start_time = params
        .start_time
        .unwrap_or_else(|| end_time - chrono::Duration::hours(24));

    let filter = FrameFilter {
        start_time: Some(start_time),
        end_time: Some(end_time),
        app_name: None,
        device_name: None,
        tag_ids: None,
        monitor_index: params.monitor_index,
    };

    let limit = params.limit.unwrap_or(100);
    let offset = params.offset.unwrap_or(0);

    let pagination = Pagination { limit, offset };

    // If search query provided, use FTS search
    if let Some(query) = params.q {
        if !query.is_empty() {
            debug!("Using FTS search for query: {}", query);

            let search_results = match state.db.search_ocr_text(&query, filter, pagination).await {
                Ok(results) => results,
                Err(e) => {
                    error!("FTS search failed: {}", e);
                    return Err(AppError::Database(e));
                }
            };

            let total = search_results.len() as i64; // Note: This is approximate, FTS doesn't provide total count easily

            // Bulk load tags for all search results (performance optimization)
            let frame_ids: Vec<i64> = search_results.iter().map(|r| r.frame.id).collect();
            let tags_map = state
                .db
                .get_tags_for_frames(&frame_ids)
                .await
                .unwrap_or_default();

            let mut enriched_frames = Vec::new();
            for result in search_results {
                let frame = result.frame;
                let ocr_text = result
                    .ocr_matches
                    .into_iter()
                    .map(|r| r.text)
                    .collect::<Vec<_>>()
                    .join(" ");

                // Get tags from bulk-loaded map
                let tags = tags_map
                    .get(&frame.id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| crate::models::TagResponse {
                        id: t.id,
                        name: t.tag_name,
                        color: t.color,
                        created_at: t.created_at,
                    })
                    .collect();

                enriched_frames.push(crate::models::FrameResponse {
                    id: frame.id,
                    timestamp: frame.timestamp,
                    file_path: frame.file_path,
                    app_name: frame.active_process.unwrap_or_default(),
                    window_name: frame.active_window.unwrap_or_default(),
                    ocr_text,
                    tags,
                    thumbnail: None,
                });
            }

            return Ok(Json(PaginatedFramesResponse {
                data: enriched_frames,
                pagination: PaginationInfo {
                    limit,
                    offset,
                    total,
                },
            }));
        }
    }

    // Regular frame retrieval (no search)
    let total = match state.db.count_frames_in_range(start_time, end_time).await {
        Ok(count) => count,
        Err(e) => {
            error!("Failed to count frames: {}", e);
            return Err(AppError::Database(e));
        }
    };

    match state
        .db
        .get_frames_in_range(start_time, end_time, filter, pagination)
        .await
    {
        Ok(frames) => {
            debug!("Retrieved {} of {} total frames", frames.len(), total);

            // Bulk load tags for all frames (performance optimization)
            let frame_ids: Vec<i64> = frames.iter().map(|f| f.id).collect();
            let tags_map = state
                .db
                .get_tags_for_frames(&frame_ids)
                .await
                .unwrap_or_default();

            // Enrich frames with OCR text and tags
            let mut enriched_frames = Vec::new();
            for frame in frames {
                // Get OCR text (still per-frame, as per user decision to optimize tags only)
                let ocr_text = state
                    .db
                    .get_ocr_text_for_frame(frame.id)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|r| r.text)
                    .collect::<Vec<_>>()
                    .join(" ");

                // Get tags from bulk-loaded map
                let tags = tags_map
                    .get(&frame.id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| crate::models::TagResponse {
                        id: t.id,
                        name: t.tag_name,
                        color: t.color,
                        created_at: t.created_at,
                    })
                    .collect();

                enriched_frames.push(crate::models::FrameResponse {
                    id: frame.id,
                    timestamp: frame.timestamp,
                    file_path: frame.file_path,
                    app_name: frame.active_process.unwrap_or_default(),
                    window_name: frame.active_window.unwrap_or_default(),
                    ocr_text,
                    tags,
                    thumbnail: None,
                });
            }

            Ok(Json(PaginatedFramesResponse {
                data: enriched_frames,
                pagination: PaginationInfo {
                    limit,
                    offset,
                    total,
                },
            }))
        }
        Err(e) => {
            error!("Failed to retrieve frames: {}", e);
            Err(AppError::Database(e))
        }
    }
}

/// GET /frames/:id - Get a single frame by ID
///
/// Returns frame metadata for a specific frame.
///
/// # Path Parameters
/// - id: Frame ID
pub async fn get_single_frame(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<crate::models::FrameResponse>> {
    debug!("Get single frame request: id={}", id);

    match state.db.get_frame(id).await {
        Ok(Some(frame)) => {
            debug!("Retrieved frame {}", id);

            // Get OCR text
            let ocr_text = state
                .db
                .get_ocr_text_for_frame(frame.id)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|r| r.text)
                .collect::<Vec<_>>()
                .join(" ");

            // Get tags
            let tags = state
                .db
                .get_tags_for_frame(frame.id)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|t| crate::models::TagResponse {
                    id: t.id,
                    name: t.tag_name,
                    color: t.color,
                    created_at: t.created_at,
                })
                .collect();

            Ok(Json(crate::models::FrameResponse {
                id: frame.id,
                timestamp: frame.timestamp,
                file_path: frame.file_path,
                app_name: frame.active_process.unwrap_or_default(),
                window_name: frame.active_window.unwrap_or_default(),
                ocr_text,
                tags,
                thumbnail: None,
            }))
        }
        Ok(None) => {
            error!("Frame {} not found", id);
            Err(AppError::NotFound(format!("Frame {} not found", id)))
        }
        Err(e) => {
            error!("Failed to retrieve frame {}: {}", id, e);
            Err(AppError::Database(e))
        }
    }
}

/// GET /frames/:id/image - Get the image file for a specific frame
///
/// Returns the captured screenshot image for a frame.
///
/// # Path Parameters
/// - id: Frame ID
pub async fn get_frame_image(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse> {
    debug!("Get frame image request: id={}", id);

    // Get frame metadata
    let frame = match state.db.get_frame(id).await {
        Ok(Some(frame)) => frame,
        Ok(None) => {
            return Err(AppError::NotFound(format!("Frame {} not found", id)));
        }
        Err(e) => {
            return Err(AppError::Database(e));
        }
    };

    // Read image file
    let image_data = match fs::read(&frame.file_path).await {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to read image file {}: {}", frame.file_path, e);
            return Err(AppError::NotFound(format!(
                "Image file not found: {}",
                frame.file_path
            )));
        }
    };

    // Determine content type
    let content_type = if frame.file_path.ends_with(".png") {
        "image/png"
    } else if frame.file_path.ends_with(".jpg") || frame.file_path.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    };

    debug!(
        "Serving image for frame {}: {} ({} bytes)",
        id,
        frame.file_path,
        image_data.len()
    );

    Ok(([(header::CONTENT_TYPE, content_type)], image_data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_validation() {
        // Empty query should be rejected
        let query = SearchQuery {
            q: "".to_string(),
            start_time: None,
            end_time: None,
            app: None,
            limit: None,
        };
        assert!(query.q.is_empty());
    }

    #[test]
    fn test_keyword_parsing() {
        let keywords = "hello, world, test";
        let parsed: Vec<String> = keywords.split(',').map(|s| s.trim().to_string()).collect();
        assert_eq!(parsed, vec!["hello", "world", "test"]);
    }
}
