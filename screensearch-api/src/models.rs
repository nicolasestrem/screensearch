//! API request/response models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================
// Search Models
// ============================================================

/// Search query parameters for full-text search
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Search query string for FTS5
    pub q: String,

    /// Optional start time filter (ISO 8601 format)
    #[serde(default)]
    pub start_time: Option<DateTime<Utc>>,

    /// Optional end time filter (ISO 8601 format)
    #[serde(default)]
    pub end_time: Option<DateTime<Utc>>,

    /// Optional application name filter
    #[serde(default)]
    pub app: Option<String>,

    /// Maximum results to return (default: 100)
    #[serde(default)]
    pub limit: Option<i64>,

    /// Search mode: "fts" (default), "semantic", or "hybrid"
    #[serde(default)]
    pub mode: Option<String>,
}

/// Keyword search parameters
#[derive(Debug, Deserialize)]
pub struct KeywordSearchQuery {
    /// Comma-separated keywords to search for
    pub keywords: String,

    /// Maximum results to return (default: 100)
    #[serde(default)]
    pub limit: Option<i64>,
}

/// Frame query parameters
#[derive(Debug, Deserialize)]
pub struct FrameQuery {
    /// Optional start time filter (ISO 8601 format)
    #[serde(default)]
    pub start_time: Option<DateTime<Utc>>,

    /// Optional end time filter (ISO 8601 format)
    #[serde(default)]
    pub end_time: Option<DateTime<Utc>>,

    /// Optional monitor index filter
    #[serde(default)]
    pub monitor_index: Option<i32>,

    /// Maximum results to return (default: 100)
    #[serde(default)]
    pub limit: Option<i64>,

    /// Offset for pagination (default: 0)
    #[serde(default)]
    pub offset: Option<i64>,

    /// Optional full-text search query
    #[serde(default)]
    pub q: Option<String>,
}

/// Pagination information
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginationInfo {
    /// Maximum results per page
    pub limit: i64,

    /// Number of results to skip
    pub offset: i64,

    /// Total number of results available
    pub total: i64,
}

/// Frame response with enriched data (matches frontend expectations)
#[derive(Debug, Serialize, Deserialize)]
pub struct FrameResponse {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub file_path: String,
    pub app_name: String,
    pub window_name: String,
    pub ocr_text: String,
    pub tags: Vec<TagResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_status: Option<String>,
}

/// Tag response
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TagResponse {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Paginated frames response
#[derive(Debug, Serialize)]
pub struct PaginatedFramesResponse {
    /// Frame data
    pub data: Vec<FrameResponse>,

    /// Pagination metadata
    pub pagination: PaginationInfo,
}

// ============================================================
// Automation Models
// ============================================================

/// Find elements request
#[derive(Debug, Deserialize)]
pub struct FindElementsRequest {
    /// Element selector string
    pub selector: String,

    /// Timeout in milliseconds (default: 5000)
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// UI element information
#[derive(Debug, Serialize, Deserialize)]
pub struct ElementInfo {
    /// Element name
    pub name: String,

    /// Element control type (e.g., "Button", "Edit", "Text")
    pub control_type: String,

    /// X coordinate on screen
    pub x: i32,

    /// Y coordinate on screen
    pub y: i32,

    /// Element width
    pub width: i32,

    /// Element height
    pub height: i32,

    /// Whether element is enabled
    #[serde(default)]
    pub is_enabled: bool,

    /// Whether element is visible
    #[serde(default)]
    pub is_visible: bool,
}

impl ElementInfo {
    /// Convert UIElement from screensearch-automation to ElementInfo DTO
    pub fn from_ui_element(
        element: &screensearch_automation::UIElement,
    ) -> Result<Self, crate::error::AppError> {
        let bounds = element
            .bounds()
            .map_err(crate::error::AppError::Automation)?;

        Ok(Self {
            name: element.name().unwrap_or_else(|| String::from("")),
            control_type: element.role(),
            x: bounds.0 as i32,
            y: bounds.1 as i32,
            width: bounds.2 as i32,
            height: bounds.3 as i32,
            is_enabled: element.is_enabled().unwrap_or(false),
            is_visible: element.is_visible().unwrap_or(false),
        })
    }
}

/// Click request
#[derive(Debug, Deserialize)]
pub struct ClickRequest {
    /// X coordinate to click
    pub x: i32,

    /// Y coordinate to click
    pub y: i32,

    /// Button to click ("left", "right", "middle")
    #[serde(default)]
    pub button: Option<String>,
}

/// Type text request
#[derive(Debug, Deserialize)]
pub struct TypeRequest {
    /// Text to type
    pub text: String,

    /// Delay between characters in milliseconds
    #[serde(default)]
    pub delay_ms: Option<u64>,
}

/// Scroll request
#[derive(Debug, Deserialize)]
pub struct ScrollRequest {
    /// Scroll direction ("up", "down", "left", "right")
    pub direction: String,

    /// Scroll amount (in lines or pixels)
    pub amount: i32,
}

/// Key press request
#[derive(Debug, Deserialize)]
pub struct KeyPressRequest {
    /// Key to press (e.g., "enter", "escape", "a")
    pub key: String,

    /// Optional modifier keys (e.g., ["ctrl", "shift"])
    #[serde(default)]
    pub modifiers: Option<Vec<String>>,
}

/// Get text request
#[derive(Debug, Deserialize)]
pub struct GetTextRequest {
    /// Element selector to get text from
    pub selector: String,
}

/// Get text response
#[derive(Debug, Serialize)]
pub struct GetTextResponse {
    /// Extracted text content
    pub text: String,
}

/// List elements request
#[derive(Debug, Deserialize)]
pub struct ListElementsRequest {
    /// Optional root element selector
    #[serde(default)]
    pub root_selector: Option<String>,
}

/// Open application request
#[derive(Debug, Deserialize)]
pub struct OpenAppRequest {
    /// Application name or path to open
    pub app_name: String,
}

/// Open URL request
#[derive(Debug, Deserialize)]
pub struct OpenUrlRequest {
    /// URL to open in default browser
    pub url: String,
}

/// Generic automation response
#[derive(Debug, Serialize)]
pub struct AutomationResponse {
    /// Whether the operation succeeded
    pub success: bool,

    /// Optional message describing the result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ============================================================
// System Management Models
// ============================================================

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Server status ("ok", "degraded", "error")
    pub status: String,

    /// API version
    pub version: String,

    /// Server uptime in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<u64>,

    /// Total number of frames in database
    pub frame_count: i64,

    /// Total number of OCR text records
    pub ocr_count: i64,

    /// Total number of tags
    pub tag_count: i64,

    /// Timestamp of oldest frame
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_frame: Option<DateTime<Utc>>,

    /// Timestamp of newest frame
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newest_frame: Option<DateTime<Utc>>,
}

/// Create tag request
#[derive(Debug, Deserialize)]
pub struct CreateTagRequest {
    /// Tag name
    pub tag_name: String,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// Optional color code (e.g., "#FF0000")
    #[serde(default)]
    pub color: Option<String>,
}

/// Add tag to frame request
#[derive(Debug, Deserialize)]
pub struct AddTagToFrameRequest {
    /// Tag ID to add to frame
    pub tag_id: i64,
}

/// Remove tag from frame request
#[derive(Debug, Deserialize)]
pub struct RemoveTagFromFrameRequest {
    /// Tag ID to remove from frame
    pub tag_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_query_deserialization() {
        let json = r#"{"q":"hello","limit":50}"#;
        let query: SearchQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.q, "hello");
        assert_eq!(query.limit, Some(50));
    }

    #[test]
    fn test_automation_response_serialization() {
        let response = AutomationResponse {
            success: true,
            message: Some("Operation completed".to_string()),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("success"));
        assert!(json.contains("message"));
    }
}
