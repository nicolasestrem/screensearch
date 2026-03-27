//! LLM Engine implementation using local llama.cpp server
//!
//! This module provides the core inference engine for local LLM execution.
//! It communicates with a locally running llama.cpp server via HTTP API.
//!
//! # Architecture
//!
//! Instead of embedding the LLM directly (which has dependency conflicts with
//! Windows crates), we use a separate llama.cpp server process that exposes
//! an OpenAI-compatible API. This approach:
//!
//! 1. Avoids Rust dependency conflicts
//! 2. Allows easy model swapping
//! 3. Uses the well-tested llama.cpp implementation
//! 4. Can be bundled with the installer

use crate::config::{LlmConfig, DEFAULT_LLAMA_PORT};
use crate::download::{get_model_path, get_models_dir, model_exists};
use crate::error::{LlmError, Result};
use crate::profiles::{InferenceParameters, InferenceProfile};
use crate::TextGenerator;
use async_trait::async_trait;
use image::DynamicImage;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Default local server endpoint (llama.cpp server on port 31130)
const DEFAULT_LOCAL_ENDPOINT: &str = "http://127.0.0.1:31130";

/// The main LLM engine for local inference
///
/// Uses llama.cpp server via HTTP API for Ministral-3B inference.
/// Supports both text-only and vision (image + text) generation.
pub struct LlmEngine {
    config: LlmConfig,
    model_path: PathBuf,
    client: Client,
    endpoint: String,
    initialized: Arc<AtomicBool>,
}

/// OpenAI-compatible chat completion request
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
}

/// Chat message format
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: MessageContent,
}

/// Message content - either simple text or multimodal
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Multimodal(Vec<ContentPart>),
}

/// Content part for multimodal messages
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

/// Image URL (data URI for base64)
#[derive(Debug, Serialize)]
struct ImageUrl {
    url: String,
}

/// OpenAI-compatible chat completion response
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: String,
}

impl LlmEngine {
    /// Create a new LLM engine with default configuration
    pub async fn new() -> Result<Self> {
        Self::with_config(LlmConfig::default()).await
    }

    /// Create a new LLM engine with custom configuration
    ///
    /// # Arguments
    /// * `config` - Configuration for the engine
    ///
    /// # Returns
    /// Initialized engine ready for inference
    pub async fn with_config(config: LlmConfig) -> Result<Self> {
        info!("Initializing LLM engine...");

        // Determine models directory
        let models_dir = get_models_dir();

        // Check if model exists (download happens separately)
        let model_path = config
            .model_path
            .clone()
            .unwrap_or_else(|| get_model_path(&models_dir));

        if !model_exists(&models_dir) {
            warn!(
                "Model not found at {:?}. Download required before inference.",
                model_path
            );
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for long generations
            .build()
            .map_err(|e| LlmError::ModelInitError(format!("Failed to create HTTP client: {}", e)))?;

        // Use configured endpoint or default to local llama.cpp server
        // Priority: env var > config port > default
        let endpoint = std::env::var("LLM_ENDPOINT").unwrap_or_else(|_| {
            let port = config.server_port;
            if port == DEFAULT_LLAMA_PORT {
                DEFAULT_LOCAL_ENDPOINT.to_string()
            } else {
                format!("http://127.0.0.1:{}", port)
            }
        });

        info!("LLM engine configured:");
        info!("  Model path: {:?}", model_path);
        info!("  Endpoint: {}", endpoint);
        info!("  Temperature: {}", config.temperature);
        info!("  Max tokens: {}", config.max_tokens);

        Ok(Self {
            config,
            model_path,
            client,
            endpoint,
            initialized: Arc::new(AtomicBool::new(true)),
        })
    }

    /// Send a chat completion request to the local server
    async fn chat_completion(&self, messages: Vec<ChatMessage>) -> Result<String> {
        self.chat_completion_with_params(messages, None).await
    }

    /// Send a chat completion request with custom parameters
    async fn chat_completion_with_params(
        &self,
        messages: Vec<ChatMessage>,
        params: Option<&InferenceParameters>,
    ) -> Result<String> {
        let (temp, max_tok, top_p, top_k, repeat_pen) = if let Some(p) = params {
            (p.temperature, p.max_tokens, p.top_p, p.top_k, p.repeat_penalty)
        } else {
            (
                self.config.temperature,
                self.config.max_tokens,
                self.config.top_p,
                self.config.top_k,
                self.config.repetition_penalty,
            )
        };

        let request = ChatCompletionRequest {
            model: "ministral-3b".to_string(), // Model is loaded by server
            messages,
            temperature: temp,
            max_tokens: max_tok,
            top_p: Some(top_p),
            top_k: Some(top_k),
            repeat_penalty: Some(repeat_pen),
        };

        debug!("Sending chat completion request to {}", self.endpoint);

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.endpoint))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    LlmError::ModelInitError(format!(
                        "Cannot connect to local LLM server at {}. Is llama-server running?",
                        self.endpoint
                    ))
                } else {
                    LlmError::GenerationError(format!("HTTP request failed: {}", e))
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(LlmError::GenerationError(format!(
                "Server returned error {}: {}",
                status, text
            )));
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| LlmError::GenerationError(format!("Failed to parse response: {}", e)))?;

        completion
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| LlmError::GenerationError("No response generated".to_string()))
    }

    /// Check if the model is loaded and ready
    pub fn is_loaded(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    /// Get the model path
    pub fn model_path(&self) -> &PathBuf {
        &self.model_path
    }

    /// Get configuration
    pub fn config(&self) -> &LlmConfig {
        &self.config
    }

    /// Get the server endpoint
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Encode an image to base64 for vision prompts
    fn encode_image(image: &DynamicImage) -> Result<String> {
        use base64::Engine;

        let mut buffer = Cursor::new(Vec::new());
        image
            .write_to(&mut buffer, image::ImageOutputFormat::Jpeg(80))
            .map_err(|e| LlmError::ImageProcessingError(format!("Failed to encode image: {}", e)))?;

        let encoded = base64::engine::general_purpose::STANDARD.encode(buffer.into_inner());
        Ok(format!("data:image/jpeg;base64,{}", encoded))
    }

    /// Generate text using a specific inference profile
    ///
    /// # Arguments
    /// * `prompt` - The user prompt
    /// * `profile` - The inference profile to use
    ///
    /// # Returns
    /// Generated text response
    pub async fn generate_with_profile(
        &self,
        prompt: &str,
        profile: InferenceProfile,
    ) -> Result<String> {
        let params = profile.parameters();
        let system = profile.system_prompt();

        let mut messages = Vec::new();

        if !system.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(system.to_string()),
            });
        }

        messages.push(ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(prompt.to_string()),
        });

        self.chat_completion_with_params(messages, Some(&params)).await
    }

    /// Generate text with an image using a specific inference profile
    pub async fn generate_with_image_and_profile(
        &self,
        prompt: &str,
        image: &DynamicImage,
        profile: InferenceProfile,
    ) -> Result<String> {
        let params = profile.parameters();
        let system = profile.system_prompt();
        let image_url = Self::encode_image(image)?;

        let mut messages = Vec::new();

        if !system.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(system.to_string()),
            });
        }

        messages.push(ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Multimodal(vec![
                ContentPart::Text {
                    text: prompt.to_string(),
                },
                ContentPart::ImageUrl {
                    image_url: ImageUrl { url: image_url },
                },
            ]),
        });

        self.chat_completion_with_params(messages, Some(&params)).await
    }

    /// Update the endpoint URL (useful when server port changes)
    pub fn set_endpoint(&mut self, endpoint: String) {
        self.endpoint = endpoint;
    }
}

#[async_trait]
impl TextGenerator for LlmEngine {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> Result<String> {
        let mut messages = Vec::new();

        if let Some(sys) = system {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(sys.to_string()),
            });
        }

        messages.push(ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(prompt.to_string()),
        });

        self.chat_completion(messages).await
    }

    async fn generate_with_image(
        &self,
        prompt: &str,
        image: &DynamicImage,
        system: Option<&str>,
    ) -> Result<String> {
        let image_url = Self::encode_image(image)?;

        let mut messages = Vec::new();

        if let Some(sys) = system {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(sys.to_string()),
            });
        }

        // Multimodal message with text and image
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Multimodal(vec![
                ContentPart::Text {
                    text: prompt.to_string(),
                },
                ContentPart::ImageUrl {
                    image_url: ImageUrl { url: image_url },
                },
            ]),
        });

        self.chat_completion(messages).await
    }

    fn is_ready(&self) -> bool {
        self.is_loaded()
    }

    fn provider_name(&self) -> &str {
        "local-ministral-3b"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_endpoint() {
        assert_eq!(DEFAULT_LOCAL_ENDPOINT, "http://127.0.0.1:31130");
    }

    #[test]
    fn test_chat_message_serialization() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Hello".to_string()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("user"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_multimodal_message_serialization() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Multimodal(vec![
                ContentPart::Text {
                    text: "Describe this".to_string(),
                },
                ContentPart::ImageUrl {
                    image_url: ImageUrl {
                        url: "data:image/jpeg;base64,/9j/4AAQ...".to_string(),
                    },
                },
            ]),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Describe this"));
        assert!(json.contains("image_url"));
    }

    #[tokio::test]
    #[ignore] // Requires running llama-server
    async fn test_engine_initialization() {
        let result = LlmEngine::new().await;
        assert!(result.is_ok());
    }
}
