//! Model and binary download utilities for Ministral-3B and llama-server

use crate::error::{LlmError, Result};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

/// URL to download the Ministral-3B GGUF model from
pub const MODEL_URL: &str =
    "https://huggingface.co/leophir/Ministral-3-3B-Instruct-2512-Q4_K_M/resolve/main/Ministral-3-3B-Instruct-2512-Q4_K_M.gguf";

/// Model filename
pub const MODEL_FILENAME: &str = "Ministral-3-3B-Instruct-2512-Q4_K_M.gguf";

/// Approximate model size in bytes (~2.15 GB)
pub const MODEL_SIZE_BYTES: u64 = 2_150_000_000;

// ============================================================
// llama-server binary download
// ============================================================

/// llama-server binary filename
#[cfg(windows)]
pub const LLAMA_SERVER_FILENAME: &str = "llama-server.exe";

#[cfg(not(windows))]
pub const LLAMA_SERVER_FILENAME: &str = "llama-server";

/// Approximate llama-server binary size in bytes (~22MB for CPU build as of b7562)
pub const LLAMA_SERVER_SIZE_BYTES: u64 = 22_500_000;

/// llama.cpp releases base URL (moved from ggerganov to ggml-org)
#[allow(dead_code)]
const LLAMA_CPP_RELEASES_URL: &str = "https://github.com/ggml-org/llama.cpp/releases";

/// Current llama.cpp version for downloads
#[allow(dead_code)]
const LLAMA_VERSION: &str = "b7562";

/// Get the latest llama-server download URL for the current platform
#[cfg(all(windows, target_arch = "x86_64"))]
pub fn get_llama_server_url() -> &'static str {
    // Vulkan GPU-accelerated Windows build (works on NVIDIA, AMD, Intel GPUs)
    // Falls back to CPU automatically if no compatible GPU is available
    // Vulkan is pre-installed with modern GPU drivers, no additional setup needed
    "https://github.com/ggml-org/llama.cpp/releases/download/b7562/llama-b7562-bin-win-vulkan-x64.zip"
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub fn get_llama_server_url() -> &'static str {
    // Linux x64 build (CPU)
    "https://github.com/ggml-org/llama.cpp/releases/download/b7562/llama-b7562-bin-ubuntu-x64.zip"
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn get_llama_server_url() -> &'static str {
    // macOS ARM64 build (Apple Silicon)
    "https://github.com/ggml-org/llama.cpp/releases/download/b7562/llama-b7562-bin-macos-arm64.zip"
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub fn get_llama_server_url() -> &'static str {
    // macOS x64 build (Intel)
    "https://github.com/ggml-org/llama.cpp/releases/download/b7562/llama-b7562-bin-macos-x64.zip"
}

// Fallback for unsupported platforms
#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
)))]
pub fn get_llama_server_url() -> &'static str {
    ""
}

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

/// Get the bin directory path (for llama-server and other binaries)
///
/// Priority order:
/// 1. Same directory as screensearch.exe (installed/portable)
/// 2. "bin" directory next to the executable
/// 3. User's app data directory (standard install)
pub fn get_bin_dir() -> PathBuf {
    // 1. Check same directory as executable (bundled with installer)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let llama_server = exe_dir.join(LLAMA_SERVER_FILENAME);
            if llama_server.exists() {
                debug!("Using bin directory same as exe: {:?}", exe_dir);
                return exe_dir.to_path_buf();
            }

            // 2. Check "bin" subdirectory next to executable
            let exe_bin = exe_dir.join("bin");
            if exe_bin.exists() {
                debug!("Using bin directory next to exe: {:?}", exe_bin);
                return exe_bin;
            }
        }
    }

    // 3. Use user's app data directory
    if let Some(data_dir) = dirs::data_local_dir() {
        let app_bin = data_dir.join("ScreenSearch").join("bin");
        debug!("Using bin directory in AppData: {:?}", app_bin);
        return app_bin;
    }

    // 4. Default fallback
    debug!("Using default bin directory: bin/");
    PathBuf::from("bin")
}

/// Get the full path to the llama-server binary
pub fn get_llama_server_path() -> PathBuf {
    get_bin_dir().join(LLAMA_SERVER_FILENAME)
}

/// Check if llama-server binary exists
pub fn llama_server_exists(bin_dir: &Path) -> bool {
    let server_path = bin_dir.join(LLAMA_SERVER_FILENAME);
    server_path.exists()
}

/// Check if llama-server download is needed
pub fn needs_llama_server_download() -> bool {
    let bin_dir = get_bin_dir();
    !llama_server_exists(&bin_dir)
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
#[allow(dead_code)]
pub async fn delete_model(models_dir: &Path) -> Result<()> {
    let model_path = get_model_path(models_dir);
    if model_path.exists() {
        tokio::fs::remove_file(&model_path).await?;
        info!("Deleted model at {:?}", model_path);
    }
    Ok(())
}

// ============================================================
// llama-server Binary Download
// ============================================================

/// Download the llama-server binary
///
/// Downloads from llama.cpp GitHub releases and extracts the server binary.
pub async fn download_llama_server() -> Result<PathBuf> {
    download_llama_server_with_progress(None).await
}

/// Download the llama-server binary with optional progress reporting
pub async fn download_llama_server_with_progress(
    progress_tx: Option<tokio::sync::mpsc::Sender<DownloadProgress>>,
) -> Result<PathBuf> {
    let bin_dir = get_bin_dir();

    // Check if already downloaded
    if llama_server_exists(&bin_dir) {
        let server_path = bin_dir.join(LLAMA_SERVER_FILENAME);
        info!("llama-server already downloaded at {:?}", server_path);
        return Ok(server_path);
    }

    // Get download URL for current platform
    let download_url = get_llama_server_url();
    if download_url.is_empty() {
        return Err(LlmError::DownloadFailed(
            "llama-server download not available for this platform".to_string(),
        ));
    }

    info!("Downloading llama-server from: {}", download_url);
    info!("Destination: {:?}", bin_dir);

    // Create bin directory if it doesn't exist
    tokio::fs::create_dir_all(&bin_dir).await?;

    // Create HTTP client with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600)) // 10 minute timeout
        .build()
        .map_err(|e| LlmError::DownloadFailed(format!("Failed to create HTTP client: {}", e)))?;

    // Start download
    let response = client
        .get(download_url)
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

    let total_size = response.content_length().unwrap_or(LLAMA_SERVER_SIZE_BYTES * 5); // ZIP is larger

    // Create progress bar for terminal
    let progress_bar = ProgressBar::new(total_size);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .expect("Invalid progress template")
            .progress_chars("#>-"),
    );

    // Download to temp file
    let temp_zip_path = bin_dir.join("llama-server.zip.tmp");
    let mut file = tokio::fs::File::create(&temp_zip_path).await?;

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
            let eta = if speed > 0 { remaining / speed } else { 0 };

            let progress = DownloadProgress {
                bytes_downloaded: downloaded,
                total_bytes: total_size,
                speed_bps: speed,
                eta_seconds: eta,
            };

            let _ = tx.try_send(progress);
        }
    }

    file.flush().await?;
    drop(file);

    progress_bar.finish_with_message("Download complete! Extracting...");

    // Extract the ZIP file
    let zip_path = bin_dir.join("llama-server.zip");
    tokio::fs::rename(&temp_zip_path, &zip_path).await?;

    // Extract synchronously (zip crate isn't async)
    let bin_dir_clone = bin_dir.clone();
    let server_path = tokio::task::spawn_blocking(move || {
        extract_llama_server(&zip_path, &bin_dir_clone)
    })
    .await
    .map_err(|e| LlmError::DownloadFailed(format!("Extract task failed: {}", e)))??;

    // Clean up ZIP file
    let _ = tokio::fs::remove_file(bin_dir.join("llama-server.zip")).await;

    info!("llama-server extracted to {:?}", server_path);

    // Verify extraction
    if !llama_server_exists(&bin_dir) {
        return Err(LlmError::DownloadFailed(
            "Extraction failed - llama-server not found".to_string(),
        ));
    }

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&server_path, permissions)
            .map_err(|e| LlmError::DownloadFailed(format!("Failed to set permissions: {}", e)))?;
    }

    Ok(server_path)
}

/// Extract llama-server and all dependencies from a ZIP archive
///
/// Extracts the llama-server executable plus all shared libraries (.dll, .so, .dylib)
/// that are required for the server to run.
fn extract_llama_server(zip_path: &Path, bin_dir: &Path) -> Result<PathBuf> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| LlmError::DownloadFailed(format!("Failed to open ZIP: {}", e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| LlmError::DownloadFailed(format!("Failed to read ZIP: {}", e)))?;

    let target_filename = LLAMA_SERVER_FILENAME;
    let mut server_path: Option<PathBuf> = None;
    let mut extracted_count = 0;

    info!("Extracting {} files from ZIP archive...", archive.len());

    // Extract ALL relevant files from the archive (executables and shared libraries)
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| {
            LlmError::DownloadFailed(format!("Failed to read ZIP entry: {}", e))
        })?;

        // Skip directories
        if entry.is_dir() {
            continue;
        }

        let entry_name = entry.name().to_string();

        // Get just the filename (strip any subdirectory path like "build/bin/")
        let filename = match Path::new(&entry_name).file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        if filename.is_empty() {
            continue;
        }

        // Check if this is a file we need to extract:
        // - Executables (.exe on Windows, no extension on Unix)
        // - Shared libraries (.dll on Windows, .so on Linux, .dylib on macOS)
        let is_executable = filename == target_filename
            || filename == "llama-server"
            || filename == "llama-server.exe"
            || filename.ends_with(".exe");

        let is_shared_library = filename.ends_with(".dll")
            || filename.ends_with(".so")
            || filename.contains(".so.")  // e.g., libfoo.so.1
            || filename.ends_with(".dylib");

        if is_executable || is_shared_library {
            let output_path = bin_dir.join(&filename);
            debug!("Extracting: {} -> {:?}", entry_name, output_path);

            let mut output_file = std::fs::File::create(&output_path)
                .map_err(|e| LlmError::DownloadFailed(format!("Failed to create {}: {}", filename, e)))?;

            std::io::copy(&mut entry, &mut output_file)
                .map_err(|e| LlmError::DownloadFailed(format!("Failed to extract {}: {}", filename, e)))?;

            extracted_count += 1;

            // Track the server executable path
            if filename == target_filename || filename == "llama-server" || filename == "llama-server.exe" {
                server_path = Some(output_path.clone());
                info!("Found llama-server executable: {:?}", output_path);
            }
        }
    }

    info!("Extracted {} files to {:?}", extracted_count, bin_dir);

    server_path.ok_or_else(|| {
        LlmError::DownloadFailed(format!(
            "{} not found in ZIP archive (extracted {} other files)",
            target_filename, extracted_count
        ))
    })
}

/// Delete the downloaded llama-server
#[allow(dead_code)]
pub async fn delete_llama_server() -> Result<()> {
    let server_path = get_llama_server_path();
    if server_path.exists() {
        tokio::fs::remove_file(&server_path).await?;
        info!("Deleted llama-server at {:?}", server_path);
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
