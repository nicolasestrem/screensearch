//! Configuration for the LLM engine

use std::path::PathBuf;

/// Default temperature for generation (0.0 = deterministic, 1.0 = creative)
pub const DEFAULT_TEMPERATURE: f32 = 0.7;

/// Default maximum tokens to generate
pub const DEFAULT_MAX_TOKENS: usize = 2048;

/// Default context length (Ministral-3B supports up to 256k, but we limit for performance)
pub const DEFAULT_CONTEXT_LENGTH: usize = 8192;

/// Default number of threads (0 = auto-detect)
pub const DEFAULT_THREADS: usize = 0;

/// Configuration for the LLM engine
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// Override path to the GGUF model file (None = use auto-detected path)
    pub model_path: Option<PathBuf>,

    /// Temperature for generation (0.0-1.0)
    /// Lower values = more deterministic, higher = more creative
    pub temperature: f32,

    /// Maximum tokens to generate per response
    pub max_tokens: usize,

    /// Context length (max tokens in prompt + response)
    pub context_length: usize,

    /// Number of CPU threads for inference (0 = auto-detect)
    pub threads: usize,

    /// Enable GPU acceleration if available
    pub use_gpu: bool,

    /// Top-p (nucleus) sampling parameter
    pub top_p: f32,

    /// Top-k sampling parameter (0 = disabled)
    pub top_k: usize,

    /// Repetition penalty (1.0 = no penalty)
    pub repetition_penalty: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model_path: None,
            temperature: DEFAULT_TEMPERATURE,
            max_tokens: DEFAULT_MAX_TOKENS,
            context_length: DEFAULT_CONTEXT_LENGTH,
            threads: DEFAULT_THREADS,
            use_gpu: true, // Try GPU by default, fallback to CPU if unavailable
            top_p: 0.95,
            top_k: 40,
            repetition_penalty: 1.1,
        }
    }
}

impl LlmConfig {
    /// Create a new config with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the model path
    pub fn with_model_path(mut self, path: PathBuf) -> Self {
        self.model_path = Some(path);
        self
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set number of threads
    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    /// Enable/disable GPU
    pub fn with_gpu(mut self, use_gpu: bool) -> Self {
        self.use_gpu = use_gpu;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LlmConfig::default();
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.max_tokens, 2048);
        assert!(config.use_gpu);
    }

    #[test]
    fn test_builder_pattern() {
        let config = LlmConfig::new()
            .with_temperature(0.5)
            .with_max_tokens(1024)
            .with_gpu(false);

        assert_eq!(config.temperature, 0.5);
        assert_eq!(config.max_tokens, 1024);
        assert!(!config.use_gpu);
    }

    #[test]
    fn test_temperature_clamping() {
        let config = LlmConfig::new().with_temperature(5.0);
        assert_eq!(config.temperature, 2.0);

        let config = LlmConfig::new().with_temperature(-1.0);
        assert_eq!(config.temperature, 0.0);
    }
}
