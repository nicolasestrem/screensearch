//! Download Progress Handlers
//!
//! Provides endpoints for tracking download progress of models and binaries.

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
                "embeddings_model" => "Embeddings Model",
                "embeddings_tokenizer" => "Embeddings Tokenizer",
                _ => &key,
            };

            DownloadProgressInfo {
                name: name.to_string(),
                bytes_downloaded: progress.bytes_downloaded,
                total_bytes: progress.total_bytes,
                speed_bps: progress.speed_bps,
                eta_seconds: progress.eta_seconds,
                percentage: progress.percentage(),
            }
        })
        .collect();

    Ok(Json(AllDownloadsResponse { downloads }))
}
