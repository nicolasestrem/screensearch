//! Model download utility
//!
//! Downloads the ONNX model from HuggingFace on first use.

use crate::{EmbeddingError, Result};
use std::path::{Path, PathBuf};
use tracing::info;

/// Model file names
const MODEL_FILE: &str = "model.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";

/// Model file URLs - using Xenova's ONNX export which is verified to work
/// This is the same paraphrase-multilingual-MiniLM-L12-v2 model exported to ONNX
const MODEL_URL: &str = "https://huggingface.co/Xenova/paraphrase-multilingual-MiniLM-L12-v2/resolve/main/onnx/model.onnx";
const TOKENIZER_URL: &str = "https://huggingface.co/Xenova/paraphrase-multilingual-MiniLM-L12-v2/resolve/main/tokenizer.json";

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
    std::fs::create_dir_all(models_dir).map_err(EmbeddingError::IoError)?;
    
    let model_path = models_dir.join(MODEL_FILE);
    let tokenizer_path = models_dir.join(TOKENIZER_FILE);
    
    // Download model if not exists
    if !model_path.exists() {
        info!("Downloading ONNX model from HuggingFace...");
        download_file(MODEL_URL, &model_path).await?;
        info!("Model downloaded successfully");
    } else {
        info!("Model already exists at {:?}", model_path);
    }
    
    // Download tokenizer if not exists
    if !tokenizer_path.exists() {
        info!("Downloading tokenizer from HuggingFace...");
        download_file(TOKENIZER_URL, &tokenizer_path).await?;
        info!("Tokenizer downloaded successfully");
    } else {
        info!("Tokenizer already exists at {:?}", tokenizer_path);
    }
    
    Ok(())
}

/// Download a file from URL to path
async fn download_file(url: &str, path: &Path) -> Result<()> {
    // Use reqwest for HTTP download
    let response = reqwest::get(url).await.map_err(|e| {
        EmbeddingError::ModelInitError(format!("Failed to download from {}: {}", url, e))
    })?;
    
    if !response.status().is_success() {
        return Err(EmbeddingError::ModelInitError(format!(
            "Download failed with status: {}",
            response.status()
        )));
    }
    
    let bytes = response.bytes().await.map_err(|e| {
        EmbeddingError::ModelInitError(format!("Failed to read response: {}", e))
    })?;
    
    std::fs::write(path, bytes).map_err(EmbeddingError::IoError)?;
    
    Ok(())
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
        assert!(model.to_string_lossy().contains("model_quantized.onnx"));
        assert!(tokenizer.to_string_lossy().contains("tokenizer.json"));
    }
}
