//! Model download utility
//!
//! Downloads the ONNX model from HuggingFace on first use.

use crate::{EmbeddingError, Result};
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

/// Model file names
const MODEL_FILE: &str = "model.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";

/// Model file URLs - using Xenova's ONNX export which is verified to work
/// This is the same paraphrase-multilingual-MiniLM-L12-v2 model exported to ONNX
const MODEL_URL: &str = "https://huggingface.co/Xenova/paraphrase-multilingual-MiniLM-L12-v2/resolve/main/onnx/model.onnx";
const TOKENIZER_URL: &str = "https://huggingface.co/Xenova/paraphrase-multilingual-MiniLM-L12-v2/resolve/main/tokenizer.json";

/// Approximate model file sizes in bytes
const MODEL_SIZE_BYTES: u64 = 470_000_000; // ~470MB
const TOKENIZER_SIZE_BYTES: u64 = 17_000_000; // ~17MB

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

/// Get the default models directory
pub fn get_models_dir() -> PathBuf {
    // 1. Check "models" in current working directory (Portable/Bundled)
    let cwd_models = PathBuf::from("models");
    if cwd_models.exists() {
        return cwd_models;
    }

    // 2. Check "models" next to the executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_models = exe_dir.join("models");
            if exe_models.exists() {
                return exe_models;
            }
        }
    }

    // 3. Use user's app data directory (Installed)
    if let Some(data_dir) = dirs::data_local_dir() {
        return data_dir.join("screensearch").join("models");
    }
    
    // 4. Default to current directory (will trigger download)
    PathBuf::from("models")
}

/// Check if model files exist
pub fn model_exists(models_dir: &Path) -> bool {
    let model_path = models_dir.join(MODEL_FILE);
    let tokenizer_path = models_dir.join(TOKENIZER_FILE);
    model_path.exists() && tokenizer_path.exists()
}

/// Get paths to model files
pub fn get_model_paths(models_dir: &Path) -> (PathBuf, PathBuf) {
    (
        models_dir.join(MODEL_FILE),
        models_dir.join(TOKENIZER_FILE),
    )
}

/// Download model files from HuggingFace
///
/// This function downloads the ONNX model and tokenizer if they don't exist.
pub async fn download_model(models_dir: &Path) -> Result<()> {
    download_model_with_progress(models_dir, None, None).await
}

/// Download model files with optional progress reporting
///
/// # Arguments
/// * `models_dir` - Directory to store the model files
/// * `model_progress_tx` - Optional channel to send model download progress
/// * `tokenizer_progress_tx` - Optional channel to send tokenizer download progress
pub async fn download_model_with_progress(
    models_dir: &Path,
    model_progress_tx: Option<tokio::sync::mpsc::Sender<DownloadProgress>>,
    tokenizer_progress_tx: Option<tokio::sync::mpsc::Sender<DownloadProgress>>,
) -> Result<()> {
    tokio::fs::create_dir_all(models_dir).await.map_err(EmbeddingError::IoError)?;

    let model_path = models_dir.join(MODEL_FILE);
    let tokenizer_path = models_dir.join(TOKENIZER_FILE);

    // Download model if not exists
    if !model_path.exists() {
        info!("Downloading ONNX model from HuggingFace...");
        download_file_with_progress(MODEL_URL, &model_path, MODEL_SIZE_BYTES, model_progress_tx).await?;
        info!("Model downloaded successfully");
    } else {
        info!("Model already exists at {:?}", model_path);
    }

    // Download tokenizer if not exists
    if !tokenizer_path.exists() {
        info!("Downloading tokenizer from HuggingFace...");
        download_file_with_progress(TOKENIZER_URL, &tokenizer_path, TOKENIZER_SIZE_BYTES, tokenizer_progress_tx).await?;
        info!("Tokenizer downloaded successfully");
    } else {
        info!("Tokenizer already exists at {:?}", tokenizer_path);
    }

    Ok(())
}

/// Download a file from URL to path with optional progress reporting
async fn download_file_with_progress(
    url: &str,
    path: &Path,
    expected_size: u64,
    progress_tx: Option<tokio::sync::mpsc::Sender<DownloadProgress>>,
) -> Result<()> {
    // Use reqwest for HTTP download
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600)) // 1 hour timeout
        .build()
        .map_err(|e| EmbeddingError::ModelInitError(format!("Failed to create HTTP client: {}", e)))?;

    let response = client.get(url).send().await.map_err(|e| {
        EmbeddingError::ModelInitError(format!("Failed to download from {}: {}", url, e))
    })?;

    if !response.status().is_success() {
        return Err(EmbeddingError::ModelInitError(format!(
            "Download failed with status: {}",
            response.status()
        )));
    }

    let total_size = response.content_length().unwrap_or(expected_size);

    // Download to temp file first (atomic write)
    let temp_path = path.with_extension("tmp");
    let mut file = tokio::fs::File::create(&temp_path).await.map_err(EmbeddingError::IoError)?;

    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let start_time = std::time::Instant::now();

    // Download with automatic cleanup on failure
    let download_result = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                EmbeddingError::ModelInitError(format!("Failed to read chunk: {}", e))
            })?;

            file.write_all(&chunk).await.map_err(EmbeddingError::IoError)?;
            downloaded += chunk.len() as u64;

            // Send progress update if channel provided
            if let Some(ref tx) = progress_tx {
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    (downloaded as f64 / elapsed) as u64
                } else {
                    0
                };
                let remaining = total_size.saturating_sub(downloaded);
                // Cap ETA to 24 hours to prevent overflow with very slow speeds
                let eta = if speed > 0 {
                    std::cmp::min(remaining / speed, 86400)
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

        file.flush().await.map_err(EmbeddingError::IoError)?;
        drop(file);

        // Rename temp file to final name (atomic on most filesystems)
        tokio::fs::rename(&temp_path, path).await.map_err(EmbeddingError::IoError)?;

        Ok(())
    }.await;

    // Clean up temp file on failure
    if download_result.is_err() {
        if let Err(e) = tokio::fs::remove_file(&temp_path).await {
            warn!("Failed to clean up temp file {:?}: {}", temp_path, e);
        } else {
            debug!("Cleaned up temp file {:?} after download failure", temp_path);
        }
    }

    download_result
}

/// Check if model download is needed
pub fn needs_download(models_dir: &Path) -> bool {
    !model_exists(models_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_models_dir() {
        let dir = get_models_dir();
        // Should return some path
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_model_paths() {
        let dir = PathBuf::from("/tmp/models");
        let (model, tokenizer) = get_model_paths(&dir);
        assert!(model.to_string_lossy().contains("model.onnx"));
        assert!(tokenizer.to_string_lossy().contains("tokenizer.json"));
    }
}
