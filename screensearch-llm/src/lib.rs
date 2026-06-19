//! Local LLM Integration for ScreenSearch
//!
//! This crate provides local LLM inference using Ministral-3B with auto-managed
//! llama-server process. It supports text generation and vision analysis.
//!
//! # Architecture
//!
//! The engine communicates with a locally running llama.cpp server via HTTP API.
//! The server process is managed automatically with:
//!
//! - Lazy startup on first AI request
//! - Automatic shutdown after idle timeout (TTL)
//! - Health monitoring and auto-restart on crash
//! - Port fallback if default port is busy
//!
//! # Example
//!
//! ```rust,ignore
//! use screensearch_llm::{LlmEngine, LlmConfig, LlamaServer, InferenceProfile};
//!
//! // Auto-managed server
//! let server = LlamaServer::new(config);
//! server.ensure_started().await?;
//!
//! // Generate with profile
//! let engine = LlmEngine::new().await?;
//! let response = engine.generate_with_profile("Summarize this", InferenceProfile::RagAnswer).await?;
//! ```

mod config;
mod download;
mod engine;
mod error;
mod profiles;
mod server;

// Public API exports
pub use config::{
    LlmConfig, DEFAULT_IDLE_TTL_SECS, DEFAULT_LLAMA_PORT, DEFAULT_MAX_TOKENS, DEFAULT_TEMPERATURE,
};
pub use download::{
    // Model discovery + download
    discover_local_models,
    // llama-server download
    download_llama_server,
    download_llama_server_with_progress,
    download_model,
    download_model_with_progress,
    get_bin_dir,
    get_llama_server_path,
    get_model_path,
    get_models_dir,
    llama_server_exists,
    model_exists,
    model_search_dirs,
    needs_download,
    needs_llama_server_download,
    resolve_model_path,
    DownloadProgress,
    LLAMA_SERVER_FILENAME,
    LLAMA_SERVER_SIZE_BYTES,
    MODEL_FILENAME,
    MODEL_SIZE_BYTES,
    MODEL_URL,
};
pub use engine::LlmEngine;
pub use error::{LlmError, Result};
pub use profiles::{validate_parameters, InferenceParameters, InferenceProfile};
pub use server::{
    spawn_server_monitor, LlamaServer, LlamaServerConfig, ServerStatus,
    DEFAULT_LLAMA_PORT as SERVER_PORT,
};

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
    #[allow(clippy::assertions_on_constants)]
    fn test_model_size_reasonable() {
        // Guards against accidental edits to the default-model size constant.
        assert!(MODEL_SIZE_BYTES > 1_000_000_000);
        assert!(MODEL_SIZE_BYTES < 5_000_000_000);
    }
}
