use crate::{VisionAnalysis, VisionModel};
use anyhow::{Context, Result};
use async_trait::async_trait;
use image::DynamicImage;
use reqwest::Client;
use serde_json::json;
use std::io::Cursor;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

pub struct OllamaClient {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    provider: String,
}

impl OllamaClient {
    pub fn new(base_url: String, model: String, api_key: Option<String>, provider: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            model,
            api_key,
            provider,
        }
    }

    /// Generate text response from a prompt (no image)
    pub async fn generate_text(&self, prompt: &str, system: Option<&str>) -> Result<String> {
        if self.provider == "ollama" {
            let mut body = json!({
                "model": self.model,
                "prompt": prompt,
                "stream": false
            });

            if let Some(sys) = system {
                body["system"] = json!(sys);
            }

            let mut request = self.client.post(format!("{}/api/generate", self.base_url))
                .json(&body);

            if let Some(key) = &self.api_key {
                request = request.header("Authorization", format!("Bearer {}", key));
            }

            let response = request.send().await.context("Failed to send request to Ollama")?;
            let text = response.text().await?;
            let json_resp: serde_json::Value = serde_json::from_str(&text)?;
            
            json_resp["response"].as_str()
                .map(|s| s.to_string())
                .context("Invalid Ollama response format")
        } else {
            // OpenAI Compatible
            let mut messages = Vec::new();
            if let Some(sys) = system {
                messages.push(json!({"role": "system", "content": sys}));
            }
            messages.push(json!({"role": "user", "content": prompt}));

            let body = json!({
                "model": self.model,
                "messages": messages,
                "stream": false
            });

            // Normalize base_url to remove trailing slash
            let base = self.base_url.trim_end_matches('/');
            let url = if base.ends_with("/current") || base.contains("/v1") {
                format!("{}/chat/completions", base)
            } else {
                format!("{}/v1/chat/completions", base)
            };
            
            let mut request = self.client.post(&url).json(&body);

            if let Some(key) = &self.api_key {
                request = request.header("Authorization", format!("Bearer {}", key));
            }

            let response = request.send().await.context("Failed to send request to OpenAI-compatible provider")?;
            let text = response.text().await?;
            let json_resp: serde_json::Value = serde_json::from_str(&text)?;
            
            json_resp["choices"][0]["message"]["content"].as_str()
                .map(|s| s.to_string())
                .context(format!("Invalid OpenAI response format: {}", text))
        }
    }
}

#[async_trait]
impl VisionModel for OllamaClient {
    async fn analyze(&self, image: &DynamicImage, context: &str) -> Result<VisionAnalysis> {
        // Encode image to base64
        let mut buf = Vec::new();
        image.write_to(&mut Cursor::new(&mut buf), image::ImageOutputFormat::Jpeg(80))?;
        let base64_image = BASE64.encode(&buf);

        let system_prompt = r#"You are a visual intelligence engine.
Analyze the screenshot and extract structured data.
Output strictly valid JSON matching this schema:
{
  "description": "Concise summary of visual content and user intent",
  "visible_text": ["List", "of", "prominent", "text"],
  "activity_type": "One of: coding, design, verified, browsing, communication, entertainment, other",
  "application": "Name of the active application inferred from content",
  "confidence": 0.0 to 1.0
}
Ignore UI chrome (taskbar, window borders) unless relevant to context.
Prefer intent, names, colors."#;

        let user_prompt = format!("Context: {}. Analyze this frame.", context);

        if self.provider == "ollama" {
             let mut request = self.client.post(format!("{}/api/generate", self.base_url))
                .json(&json!({
                    "model": self.model,
                    "system": system_prompt,
                    "prompt": user_prompt,
                    "images": [base64_image],
                    "format": "json",
                    "stream": false
                }));

            if let Some(key) = &self.api_key {
                request = request.header("Authorization", format!("Bearer {}", key));
            }

            let response = request.send().await.context("Failed to send request to Ollama")?;
            let text = response.text().await?;
            let json_resp: serde_json::Value = serde_json::from_str(&text)?;
            
            let response_content = json_resp["response"].as_str()
                .context("Invalid Ollama response format")?;

            let analysis: VisionAnalysis = serde_json::from_str(response_content)
                .context("Failed to parse VisionAnalysis JSON")?;

            Ok(analysis)
        } else {
            // OpenAI Vision
            let body = json!({
                "model": self.model,
                "messages": [
                    {
                        "role": "system",
                        "content": system_prompt
                    },
                    {
                        "role": "user",
                        "content": [
                            { 
                                "type": "text", 
                                "text": user_prompt 
                            },
                            {
                                "type": "image_url",
                                "image_url": {
                                    "url": format!("data:image/jpeg;base64,{}", base64_image)
                                }
                            }
                        ]
                    }
                ],
                "response_format": { "type": "json_object" },
                "stream": false
            });

            // Normalize base_url to remove trailing slash
            let base = self.base_url.trim_end_matches('/');
            let url = if base.ends_with("/current") || base.contains("/v1") {
                format!("{}/chat/completions", base)
            } else {
                format!("{}/v1/chat/completions", base)
            };

            let mut request = self.client.post(&url).json(&body);

            if let Some(key) = &self.api_key {
                request = request.header("Authorization", format!("Bearer {}", key));
            }

            let response = request.send().await.context("Failed to send request to OpenAI Vision provider")?;
            let text = response.text().await?;
            let json_resp: serde_json::Value = serde_json::from_str(&text)?;
            
            let response_content = json_resp["choices"][0]["message"]["content"].as_str()
                .context(format!("Invalid OpenAI response format: {}", text))?;

            let analysis: VisionAnalysis = serde_json::from_str(response_content)
                .context("Failed to parse VisionAnalysis JSON")?;
            
            Ok(analysis)
        }
    }
}
