use crate::{VisionAnalysis, VisionModel};
use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::DynamicImage;
use reqwest::Client;
use serde_json::json;
use std::io::Cursor;

/// Cap on tokens generated per frame analysis. The response is a small JSON
/// object, so a tight bound keeps worst-case decode time low (the model can
/// otherwise spend its whole budget enumerating on-screen text — which native
/// OCR already captures — and never close the JSON). ~320 leaves ample headroom
/// for the terse schema below while keeping warm latency near ~1s.
const MAX_OUTPUT_TOKENS: u32 = 320;

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

            let mut request = self
                .client
                .post(format!("{}/api/generate", self.base_url))
                .json(&body);

            if let Some(key) = &self.api_key {
                request = request.header("Authorization", format!("Bearer {}", key));
            }

            let response = request
                .send()
                .await
                .context("Failed to send request to Ollama")?;
            let text = response.text().await?;
            let json_resp: serde_json::Value = serde_json::from_str(&text)?;

            json_resp["response"]
                .as_str()
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

            let response = request
                .send()
                .await
                .context("Failed to send request to OpenAI-compatible provider")?;
            let text = response.text().await?;
            let json_resp: serde_json::Value = serde_json::from_str(&text)?;

            json_resp["choices"][0]["message"]["content"]
                .as_str()
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
        image.write_to(
            &mut Cursor::new(&mut buf),
            image::ImageOutputFormat::Jpeg(80),
        )?;
        let base64_image = BASE64.encode(&buf);

        let system_prompt = r#"You are a visual intelligence engine for a screen-history tool that ALREADY has full OCR of the screen text. Do NOT transcribe or list large amounts of on-screen text.
Analyze the screenshot and return ONLY compact, valid JSON in exactly this shape:
{
  "description": "1-2 sentence summary of what the user is doing and the main on-screen content",
  "visible_text": ["up to 6 of the most prominent titles or labels only"],
  "activity_type": "one of: coding, design, browsing, communication, entertainment, productivity, other",
  "app_hint": "best guess at the active application",
  "confidence": 0.0 to 1.0
}
Be brief. Ignore the taskbar and window chrome unless relevant."#;

        let user_prompt = format!("Context: {}. Analyze this frame.", context);

        if self.provider == "ollama" {
            let mut request = self
                .client
                .post(format!("{}/api/generate", self.base_url))
                .json(&json!({
                    "model": self.model,
                    "system": system_prompt,
                    "prompt": user_prompt,
                    "images": [base64_image],
                    "format": "json",
                    "stream": false,
                    "options": { "num_predict": MAX_OUTPUT_TOKENS }
                }));

            if let Some(key) = &self.api_key {
                request = request.header("Authorization", format!("Bearer {}", key));
            }

            let response = request
                .send()
                .await
                .context("Failed to send request to Ollama")?;
            let text = response.text().await?;
            let json_resp: serde_json::Value = serde_json::from_str(&text)?;

            let response_content = json_resp["response"]
                .as_str()
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
                "max_tokens": MAX_OUTPUT_TOKENS,
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

            let response = request
                .send()
                .await
                .context("Failed to send request to OpenAI Vision provider")?;
            let text = response.text().await?;
            let json_resp: serde_json::Value = serde_json::from_str(&text)?;

            let response_content = json_resp["choices"][0]["message"]["content"]
                .as_str()
                .context(format!("Invalid OpenAI response format: {}", text))?;

            let analysis: VisionAnalysis = serde_json::from_str(response_content)
                .context("Failed to parse VisionAnalysis JSON")?;

            Ok(analysis)
        }
    }
}
