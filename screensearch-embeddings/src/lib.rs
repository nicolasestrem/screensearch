//! In-process embedding generation for ScreenSearch RAG.
//!
//! This crate runs embeddings and (optional) reranking fully in-process in Rust
//! via [`fastembed`] (ONNX Runtime), with no Python sidecar and no network calls
//! beyond the first-run model download from HuggingFace (cached locally).
//!
//! # Architecture
//!
//! - [`EmbeddingEngine`]: loads an ONNX embedding model once and exposes async
//!   `embed`/`embed_batch` helpers. Blocking ONNX inference is offloaded to a
//!   blocking thread pool so it never stalls the async runtime.
//! - [`chunker::TextChunker`]: splits OCR text into overlapping chunks before
//!   embedding (previously done by the sidecar `/v1/chunk` endpoint).
//! - Optional cross-encoder reranking via the same ONNX runtime; the default
//!   retrieval path relies on Reciprocal Rank Fusion and skips it.
//!
//! # Example
//!
//! ```no_run
//! use screensearch_embeddings::EmbeddingEngine;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let engine = EmbeddingEngine::new().await?;
//!     let embedding = engine.embed("Hello, world!").await?;
//!     assert_eq!(embedding.len(), screensearch_embeddings::EMBEDDING_DIM);
//!     Ok(())
//! }
//! ```

use std::path::PathBuf;
use thiserror::Error;

pub mod chunker;
mod engine;
mod image_engine;

pub use chunker::TextChunker;
pub use engine::{EmbeddingEngine, RerankScore};
pub use image_engine::ImageEmbeddingEngine;

/// Embedding dimension for EmbeddingGemma-300M (and the configured fallbacks).
pub const EMBEDDING_DIM: usize = 768;

/// Image-embedding model (screenshots) — aligned with [`IMAGE_QUERY_MODEL_NAME`]
/// in a shared 768-dim space. Used by the optional visual-recall index.
pub const IMAGE_MODEL_NAME: &str = "nomic-embed-vision-v1.5";

/// Text encoder for image-search queries, aligned with [`IMAGE_MODEL_NAME`].
pub const IMAGE_QUERY_MODEL_NAME: &str = "nomic-embed-text-v1.5";

/// Model revision recorded in the image-vector contract.
pub const IMAGE_MODEL_VERSION: &str = "1";

/// Stable model identity recorded in the persisted-vector contract.
///
/// Changing this string invalidates existing vectors and forces a reindex
/// (see `embeddings_reindex_required` in the database metadata).
pub const MODEL_NAME: &str = "EmbeddingGemma-300M";

/// Model revision used to invalidate stale vectors.
pub const MODEL_VERSION: &str = "1";

/// Default cross-encoder reranker (only loaded when reranking is enabled).
pub const RERANKER_MODEL_NAME: &str = "bge-reranker-v2-m3";

/// Approximate size of the first-run EmbeddingGemma-300M (quantized) download.
///
/// `fastembed`/ONNX Runtime download the model opaquely (no byte-progress
/// callback), so a determinate progress bar has to infer progress from the
/// growing cache file against this estimate. The dominant file is
/// `model_quantized.onnx_data` (~295 MB); with tokenizer + graph the total is
/// ~315 MB. The watcher tracks the single largest growing file, so this is set
/// just above that dominant file: the bar reaches ~100% as it completes, then
/// the watcher clears its entry and readiness flips to "ready". A slightly-off
/// estimate only affects the bar's final few percent, never correctness.
pub const MODEL_DOWNLOAD_APPROX_BYTES: u64 = 300 * 1024 * 1024;

/// Resolve the directory `fastembed`/`hf-hub` use to cache downloaded models.
///
/// Mirrors fastembed 5.x precedence (`pull_from_hf` + `get_cache_dir`) so a
/// progress watcher can locate the file being written on first run:
/// 1. `HF_HOME` — honored first inside `pull_from_hf`.
/// 2. `SCREENSEARCH_MODEL_CACHE` — our [`EmbeddingConfig::cache_dir`] override.
/// 3. `FASTEMBED_CACHE_DIR`, else `.fastembed_cache` (relative to the CWD).
pub fn resolve_cache_root() -> PathBuf {
    if let Some(hf_home) = std::env::var_os("HF_HOME") {
        return PathBuf::from(hf_home);
    }
    if let Some(custom) = std::env::var_os("SCREENSEARCH_MODEL_CACHE") {
        return PathBuf::from(custom);
    }
    std::env::var_os("FASTEMBED_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".fastembed_cache"))
}

/// Embedding-related errors.
#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("Model initialization failed: {0}")]
    ModelInitError(String),

    #[error("Tokenization failed: {0}")]
    TokenizationError(String),

    #[error("Inference failed: {0}")]
    InferenceError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type alias for embedding operations.
pub type Result<T> = std::result::Result<T, EmbeddingError>;

/// Configuration for the in-process embedding engine.
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Embedding model identity (recorded in the vector contract).
    pub model: String,
    /// Model revision used to invalidate stale vectors.
    pub model_version: String,
    /// Inference batch size.
    pub batch_size: usize,
    /// Directory used to cache downloaded ONNX models (None = fastembed default).
    pub cache_dir: Option<PathBuf>,
    /// Show a progress bar while downloading models on first use.
    pub show_download_progress: bool,
    /// Load the cross-encoder reranker. Off by default; RRF handles fusion.
    pub reranker_enabled: bool,
    /// Reranker model identity (only used when `reranker_enabled`).
    pub reranker_model: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: MODEL_NAME.to_string(),
            model_version: MODEL_VERSION.to_string(),
            // Larger batch = better throughput when indexing a backlog of frames
            // (the on-demand "Process frames" job batches chunks across frames).
            batch_size: 32,
            cache_dir: std::env::var_os("SCREENSEARCH_MODEL_CACHE").map(PathBuf::from),
            show_download_progress: true,
            reranker_enabled: false,
            reranker_model: RERANKER_MODEL_NAME.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.batch_size, 32);
        assert_eq!(config.model, MODEL_NAME);
        assert!(!config.reranker_enabled);
    }

    #[test]
    fn test_constants() {
        assert_eq!(EMBEDDING_DIM, 768);
        assert_eq!(MODEL_NAME, "EmbeddingGemma-300M");
    }
}
