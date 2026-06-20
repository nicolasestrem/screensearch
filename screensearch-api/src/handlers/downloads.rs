//! Download Progress Handlers
//!
//! Provides endpoints for tracking download progress of models and binaries.
//!
//! # Progress Tracking Lifecycle
//!
//! 1. Download is initiated via AI endpoints (`/api/ai/model/download` or `/api/ai/server/download`)
//! 2. Download handler creates a progress channel and spawns two tasks:
//!    - Receiver task: Updates AppState with progress from the channel
//!    - Download task: Performs the actual download, sends progress updates, and clears progress on completion
//! 3. Frontend polls `/api/downloads/status` every second while downloads are active
//! 4. On success: Download task clears the progress entry from AppState
//! 5. On error: Download task updates progress with error message (persists until user dismisses)
//!
//! # Polling Behavior
//!
//! - Frontend component starts polling on mount if downloads exist
//! - Polling continues only while `downloads.length > 0`
//! - When all downloads complete/fail, polling stops automatically
//! - This prevents unnecessary API calls when idle

use crate::error::Result;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;
use std::sync::Arc;

// ============================================================
// Models
// ============================================================

/// Progress information for a single download
#[derive(Debug, Serialize, Clone)]
pub struct DownloadProgressInfo {
    /// Stable progress key (e.g. "llm_model", "embedding_model"). Lets the UI
    /// match a specific download without depending on the display name.
    pub key: String,
    /// Human-readable name of the download
    pub name: String,
    /// Bytes downloaded so far
    pub bytes_downloaded: u64,
    /// Total bytes to download
    pub total_bytes: u64,
    /// Download speed in bytes per second
    pub speed_bps: u64,
    /// Estimated time remaining in seconds
    pub eta_seconds: u64,
    /// Progress as a percentage (0.0 - 100.0)
    pub percentage: f64,
    /// Error message if download failed
    pub error: Option<String>,
}

/// Response containing all active download progresses
#[derive(Debug, Serialize)]
pub struct AllDownloadsResponse {
    /// Map of download key to progress info
    pub downloads: Vec<DownloadProgressInfo>,
}

// ============================================================
// Handlers
// ============================================================

/// GET /downloads/status
/// Get the current status of all active downloads
pub async fn get_all_downloads_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AllDownloadsResponse>> {
    let progress_map = state.get_all_download_progress().await;

    let downloads: Vec<DownloadProgressInfo> = progress_map
        .into_iter()
        .map(|(key, progress)| {
            let name = match key.as_str() {
                "llm_model" => "Ministral-3B Model",
                "llama_server" => "llama-server Binary",
                "embedding_model" => "Search Model (EmbeddingGemma-300M)",
                _ => &key,
            };

            DownloadProgressInfo {
                key: key.clone(),
                name: name.to_string(),
                bytes_downloaded: progress.bytes_downloaded,
                total_bytes: progress.total_bytes,
                speed_bps: progress.speed_bps,
                eta_seconds: progress.eta_seconds,
                percentage: progress.percentage(),
                error: progress.error,
            }
        })
        .collect();

    Ok(Json(AllDownloadsResponse { downloads }))
}
