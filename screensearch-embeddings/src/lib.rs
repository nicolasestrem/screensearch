//! Embedding Generation for ScreenSearch RAG
//!
//! This crate provides local ML-based text embedding generation using ONNX Runtime.
//! It uses the `paraphrase-multilingual-MiniLM-L12-v2` model for multilingual support,
//! producing 384-dimensional embeddings suitable for semantic search.
//!
//! # Architecture
//!
//! - `EmbeddingEngine`: Main interface for generating embeddings
//! - Uses ONNX Runtime for efficient CPU/GPU inference
//! - HuggingFace tokenizers for text preprocessing
//! - Supports batch processing for efficiency
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
//!     let embedding = engine.embed("Hello, world!")?;
//!     println!("Embedding dimension: {}", embedding.len()); // 384
//!     
//!     Ok(())
//! }
//! ```

use thiserror::Error;

mod engine;
mod chunker;
mod download;

pub use engine::EmbeddingEngine;
pub use chunker::TextChunker;
pub use download::{download_model, get_models_dir, model_exists, needs_download};

/// Embedding dimension for the multilingual MiniLM model
pub const EMBEDDING_DIM: usize = 384;

/// Model name for metadata tracking
pub const MODEL_NAME: &str = "paraphrase-multilingual-MiniLM-L12-v2";

/// Embedding-related errors
#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("Model initialization failed: {0}")]
    ModelInitError(String),

    #[error("Tokenization failed: {0}")]
    TokenizationError(String),

    #[error("Inference failed: {0}")]
    InferenceError(String),

    #[error("Model not found at path: {0}")]
    ModelNotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type alias for embedding operations
pub type Result<T> = std::result::Result<T, EmbeddingError>;

/// Configuration for the embedding engine
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Path to the ONNX model file
    pub model_path: Option<String>,
    
    /// Path to the tokenizer.json file
    pub tokenizer_path: Option<String>,
    
    /// Maximum sequence length (tokens)
    pub max_seq_length: usize,
    
    /// Batch size for batch processing
    pub batch_size: usize,
    
    /// Whether to normalize embeddings to unit vectors
    pub normalize: bool,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_path: None,
            tokenizer_path: None,
            max_seq_length: 256,
            batch_size: 32,
            normalize: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.max_seq_length, 256);
        assert_eq!(config.batch_size, 32);
        assert!(config.normalize);
    }

    #[test]
    fn test_constants() {
        assert_eq!(EMBEDDING_DIM, 384);
        assert_eq!(MODEL_NAME, "paraphrase-multilingual-MiniLM-L12-v2");
    }
}
