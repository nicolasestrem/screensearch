//! Error types for the LLM module

use thiserror::Error;

/// LLM-related errors
#[derive(Error, Debug)]
pub enum LlmError {
    /// Model initialization failed
    #[error("Model initialization failed: {0}")]
    ModelInitError(String),

    /// Model not found at expected path
    #[error("Model not found at path: {0}. Run model download first.")]
    ModelNotFound(String),

    /// Model download failed
    #[error("Model download failed: {0}")]
    DownloadFailed(String),

    /// Inference/generation failed
    #[error("Generation failed: {0}")]
    GenerationError(String),

    /// Image processing failed
    #[error("Image processing failed: {0}")]
    ImageProcessingError(String),

    /// Configuration error
    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    /// Engine not initialized
    #[error("LLM engine not initialized. Model may still be loading.")]
    NotInitialized,

    /// Timeout during inference
    #[error("Inference timeout after {0} seconds")]
    Timeout(u64),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type alias for LLM operations
pub type Result<T> = std::result::Result<T, LlmError>;
