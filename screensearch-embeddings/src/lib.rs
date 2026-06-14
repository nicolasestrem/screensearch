//! Embedding Generation for ScreenSearch RAG
//!
//! This crate uses the managed ScreenSearch quality sidecar for multilingual
//! embedding and reranking inference.
//!
//! # Architecture
//!
//! - `EmbeddingEngine`: Main interface for generating embeddings
//! - Uses a loopback sidecar for CPU/GPU inference
//! - Supports batch embedding and quality reranking
//!
//! # Example
//!
//! ```no_run
//! use screensearch_embeddings::EmbeddingEngine;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let engine = EmbeddingEngine::new().await?;
//!     
//!     let embedding = engine.embed("Hello, world!").await?;
//!     println!("Embedding dimension: {}", embedding.len()); // 1024
//!     
//!     Ok(())
//! }
//! ```

use thiserror::Error;

mod chunker;
mod engine;

pub use chunker::TextChunker;
pub use engine::{EmbeddingEngine, RerankScore};

/// Full embedding dimension for Qwen3-Embedding-0.6B.
pub const EMBEDDING_DIM: usize = 1024;

/// Model name for metadata tracking
pub const MODEL_NAME: &str = "Qwen/Qwen3-Embedding-0.6B";
pub const RERANKER_MODEL_NAME: &str = "Qwen/Qwen3-Reranker-0.6B";

/// Embedding-related errors
#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("Model initialization failed: {0}")]
    ModelInitError(String),

    #[error("Tokenization failed: {0}")]
    TokenizationError(String),

    #[error("Inference failed: {0}")]
    InferenceError(String),

    #[error("Quality sidecar is unavailable: {0}")]
    SidecarUnavailable(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type alias for embedding operations
pub type Result<T> = std::result::Result<T, EmbeddingError>;

/// Configuration for the embedding engine
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Loopback URL for the managed AI sidecar.
    pub sidecar_url: String,
    /// Optional bearer token shared with the sidecar process.
    pub sidecar_token: Option<String>,
    /// Embedding model identifier.
    pub model: String,
    /// Model revision used to invalidate stale vectors.
    pub model_version: String,
    /// Reranker model identifier.
    pub reranker_model: String,
    pub batch_size: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            sidecar_url: std::env::var("SCREENSEARCH_AI_SIDECAR_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:3132".to_string()),
            sidecar_token: std::env::var("SCREENSEARCH_AI_SIDECAR_TOKEN").ok(),
            model: MODEL_NAME.to_string(),
            model_version: "main".to_string(),
            reranker_model: RERANKER_MODEL_NAME.to_string(),
            batch_size: 16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.batch_size, 16);
        assert_eq!(config.model, MODEL_NAME);
    }

    #[test]
    fn test_constants() {
        assert_eq!(EMBEDDING_DIM, 1024);
        assert_eq!(MODEL_NAME, "Qwen/Qwen3-Embedding-0.6B");
    }
}
