//! Model download utilities for Ministral-3B

use crate::error::{LlmError, Result};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

/// URL to download the Ministral-3B GGUF model from
pub const MODEL_URL: &str =
    "https://leophir.com/models/Ministral-3-3B-Instruct-2512-Q4_K_M.gguf";

/// Model filename
pub const MODEL_FILENAME: &str = "Ministral-3-3B-Instruct-2512-Q4_K_M.gguf";

/// Approximate model size in bytes (~2.15 GB)
pub const MODEL_SIZE_BYTES: u64 = 2_150_000_000;

/// Progress information for model download
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// Bytes downloaded so far
    pub bytes_downloaded: u64,
    /// Total bytes to download
    pub total_bytes: u64,
    /// Download speed in bytes per second
    pub speed_bps: u64,
    /// Estimated time remaining in seconds
    pub eta_seconds: u64,
}

impl DownloadProgress {
    /// Get progress as a percentage (0.0 - 100.0)
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.bytes_downloaded as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

/// Get the models directory path
///
/// Priority order:
/// 1. "models" in current working directory (portable/bundled)
/// 2. "models" next to the executable (portable install)
/// 3. User's app data directory (standard Windows install)
/// 4. Default to "models" in current directory (will be created)
pub fn get_models_dir() -> PathBuf {
    // 1. Check "models" in current working directory
    let cwd_models = PathBuf::from("models");
    if cwd_models.exists() {
        debug!("Using models directory from CWD: {:?}", cwd_models);
        return cwd_models;
    }

    // 2. Check "models" next to the executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_models = exe_dir.join("models");
            if exe_models.exists() {
                debug!("Using models directory next to exe: {:?}", exe_models);
                return exe_models;
            }
        }
    }

    // 3. Use user's app data directory
    if let Some(data_dir) = dirs::data_local_dir() {
        let app_models = data_dir.join("ScreenSearch").join("models");
        debug!("Using models directory in AppData: {:?}", app_models);
        return app_models;
    }

    // 4. Default fallback
    debug!("Using default models directory: models/");
    PathBuf::from("models")
}

/// Get the full path to the model file
pub fn get_model_path(models_dir: &Path) -> PathBuf {
    models_dir.join(MODEL_FILENAME)
}

/// Check if the model exists and is valid
pub fn model_exists(models_dir: &Path) -> bool {
    let model_path = get_model_path(models_dir);
    if !model_path.exists() {
        return false;
    }

    // Verify file size is reasonable (at least 1GB for a valid GGUF model)
    match std::fs::metadata(&model_path) {
        Ok(meta) => {
            let size = meta.len();
            if size < 1_000_000_000 {
                warn!(
                    "Model file exists but seems too small ({} bytes). May be corrupted.",
                    size
                );
                return false;
            }
            true
        }
        Err(e) => {
            warn!("Failed to check model file: {}", e);
            false
        }
    }
}

/// Check if model download is needed
pub fn needs_download(models_dir: &Path) -> bool {
    !model_exists(models_dir)
}

/// Download the model from the configured URL
///
/// This is a simple download without progress reporting.
/// For progress updates, use `download_model_with_progress`.
pub async fn download_model(models_dir: &Path) -> Result<PathBuf> {
    download_model_with_progress(models_dir, None).await
}

/// Download the model with optional progress reporting
///
/// # Arguments
/// * `models_dir` - Directory to store the model
/// * `progress_tx` - Optional channel to send progress updates
///
/// # Returns
/// Path to the downloaded model file
pub async fn download_model_with_progress(
    models_dir: &Path,
    progress_tx: Option<tokio::sync::mpsc::Sender<DownloadProgress>>,
) -> Result<PathBuf> {
    // Create models directory if it doesn't exist
    tokio::fs::create_dir_all(models_dir).await?;

    let model_path = get_model_path(models_dir);

    // Check if already downloaded
    if model_exists(models_dir) {
        info!("Model already downloaded at {:?}", model_path);
        return Ok(model_path);
    }

    info!("Downloading Ministral-3B model from: {}", MODEL_URL);
    info!("Destination: {:?}", model_path);

    // Create HTTP client with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600)) // 1 hour timeout for large file
        .build()
        .map_err(|e| LlmError::DownloadFailed(format!("Failed to create HTTP client: {}", e)))?;

    // Start download
    let response = client
        .get(MODEL_URL)
        .send()
        .await
        .map_err(|e| LlmError::DownloadFailed(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(LlmError::DownloadFailed(format!(
            "HTTP error: {} - {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        )));
    }

    let total_size = response.content_length().unwrap_or(MODEL_SIZE_BYTES);

    // Create progress bar for terminal
    let progress_bar = ProgressBar::new(total_size);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .expect("Invalid progress template")
            .progress_chars("#>-"),
    );

    // Download to temp file first (atomic write)
    let temp_path = model_path.with_extension("tmp");
    let mut file = tokio::fs::File::create(&temp_path).await?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let start_time = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| LlmError::DownloadFailed(format!("Failed to read chunk: {}", e)))?;

        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        progress_bar.set_position(downloaded);

        // Send progress update if channel provided
        if let Some(ref tx) = progress_tx {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                (downloaded as f64 / elapsed) as u64
            } else {
                0
            };
            let remaining = total_size.saturating_sub(downloaded);
            let eta = if speed > 0 {
                remaining / speed
            } else {
                0
            };

            let progress = DownloadProgress {
                bytes_downloaded: downloaded,
                total_bytes: total_size,
                speed_bps: speed,
                eta_seconds: eta,
            };

            // Non-blocking send (drop if receiver is slow)
            let _ = tx.try_send(progress);
        }
    }

    file.flush().await?;
    drop(file);

    // Rename temp file to final name (atomic on most filesystems)
    tokio::fs::rename(&temp_path, &model_path).await?;

    progress_bar.finish_with_message("Download complete!");
    info!("Model downloaded successfully to {:?}", model_path);

    // Verify the downloaded file
    if !model_exists(models_dir) {
        return Err(LlmError::DownloadFailed(
            "Downloaded file verification failed".to_string(),
        ));
    }

    Ok(model_path)
}

/// Delete the downloaded model
pub async fn delete_model(models_dir: &Path) -> Result<()> {
    let model_path = get_model_path(models_dir);
    if model_path.exists() {
        tokio::fs::remove_file(&model_path).await?;
        info!("Deleted model at {:?}", model_path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_models_dir() {
        let dir = get_models_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_model_path() {
        let models_dir = PathBuf::from("test_models");
        let path = get_model_path(&models_dir);
        assert!(path.ends_with(MODEL_FILENAME));
    }

    #[test]
    fn test_download_progress_percentage() {
        let progress = DownloadProgress {
            bytes_downloaded: 500,
            total_bytes: 1000,
            speed_bps: 100,
            eta_seconds: 5,
        };
        assert_eq!(progress.percentage(), 50.0);
    }

    #[test]
    fn test_needs_download_missing() {
        let temp_dir = PathBuf::from("/nonexistent/path");
        assert!(needs_download(&temp_dir));
    }
}
