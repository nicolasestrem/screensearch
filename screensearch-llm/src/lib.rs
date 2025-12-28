//! Local LLM Integration for ScreenSearch
//!
//! This crate provides local LLM inference using Ministral-3B.
//! It supports text generation and vision analysis for the ScreenSearch application.
//!
//! # Architecture
//!
//! The engine communicates with a locally running llama.cpp server via HTTP API.
//! This approach avoids Rust dependency conflicts while providing:
//!
//! - Local inference without external API calls
//! - Auto-download model from configured server
//! - Vision capabilities (image + text prompts)
//! - Configurable temperature, max tokens, and threading
//!
//! # Example
//!
//! ```rust,ignore
//! use screensearch_llm::{LlmEngine, LlmConfig};
//!
//! let engine = LlmEngine::new().await?;
//! let response = engine.generate("Hello, world!", None).await?;
//! ```

mod config;
mod download;
mod engine;
mod error;

// Public API exports
pub use config::{LlmConfig, DEFAULT_MAX_TOKENS, DEFAULT_TEMPERATURE};
pub use download::{
    download_model, download_model_with_progress, get_model_path, get_models_dir, model_exists,
    needs_download, DownloadProgress, MODEL_FILENAME, MODEL_SIZE_BYTES, MODEL_URL,
};
pub use engine::LlmEngine;
pub use error::{LlmError, Result};

use async_trait::async_trait;
use image::DynamicImage;

/// Trait for text generation (unified interface for local and remote providers)
#[async_trait]
pub trait TextGenerator: Send + Sync {
    /// Generate text response from a prompt
    ///
    /// # Arguments
    /// * `prompt` - The user prompt to generate from
    /// * `system` - Optional system prompt to guide generation
    ///
    /// # Returns
    /// Generated text response
    async fn generate(&self, prompt: &str, system: Option<&str>) -> Result<String>;

    /// Generate text with an image input (vision model)
    ///
    /// # Arguments
    /// * `prompt` - The user prompt describing what to analyze
    /// * `image` - The image to analyze
    /// * `system` - Optional system prompt
    ///
    /// # Returns
    /// Generated text response based on image analysis
    async fn generate_with_image(
        &self,
        prompt: &str,
        image: &DynamicImage,
        system: Option<&str>,
    ) -> Result<String>;

    /// Check if the generator is ready for inference
    fn is_ready(&self) -> bool;

    /// Get the provider name for logging/UI
    fn provider_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = LlmConfig::default();
        assert_eq!(config.temperature, DEFAULT_TEMPERATURE);
        assert_eq!(config.max_tokens, DEFAULT_MAX_TOKENS);
        assert!(config.model_path.is_none());
    }

    #[test]
    fn test_model_url_defined() {
        assert!(!MODEL_URL.is_empty());
        assert!(MODEL_URL.starts_with("https://"));
    }

    #[test]
    fn test_model_size_reasonable() {
        // Model should be around 2GB
        assert!(MODEL_SIZE_BYTES > 1_000_000_000);
        assert!(MODEL_SIZE_BYTES < 5_000_000_000);
    }
}
