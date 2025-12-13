//! AI Integration Handlers
//!
//! Handles communication with LLM providers (OpenAI, Ollama) and report generation.

use crate::error::{AppError, Result};
use crate::state::AppState;
use axum::extract::{Json, State};
use chrono::{DateTime, Duration, Utc};
use reqwest::RequestBuilder;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

// ============================================================
// Helper Functions
// ============================================================

/// Validates that a provider URL is safe to use
/// Returns Ok(()) if valid, Err with descriptive message if invalid
fn validate_provider_url(url: &str) -> std::result::Result<(), String> {
    // Parse the URL
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL format: {}", e))?;

    // Check protocol is HTTP or HTTPS
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "Invalid protocol '{}'. Only http:// and https:// are allowed",
            scheme
        ));
    }

    // Security: Warn if not localhost
    if let Some(host) = parsed.host_str() {
        if host != "localhost" && host != "127.0.0.1" && host != "[::1]" {
            warn!("Provider URL '{}' is not localhost. Ensure this is intended and the endpoint is trusted.", url);
        }
    }

    Ok(())
}

/// Adds Authorization header with Bearer token to request if API key is provided
fn add_auth_header(builder: RequestBuilder, api_key: &Option<String>) -> RequestBuilder {
    if let Some(key) = api_key {
        if !key.is_empty() {
            return builder.header("Authorization", format!("Bearer {}", key));
        }
    }
    builder
}

// ============================================================
// Models
// ============================================================

#[derive(Debug, Deserialize)]
pub struct AiConnectionRequest {
    pub provider_url: String, // e.g. "http://localhost:11434/v1" or "https://api.openai.com/v1"
    pub api_key: Option<String>,
    pub model: String,
}

#[derive(Debug, Serialize)]
pub struct AiConnectionResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct AiReportRequest {
    pub provider_url: String,
    pub api_key: Option<String>,
    pub model: String,

    // Report Context
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub prompt: Option<String>, // Custom system prompt or overridden instruction
}

#[derive(Debug, Serialize)]
pub struct AiReportResponse {
    pub report: String,
    pub model_used: String,
    pub tokens_used: Option<u32>,
    pub context_source: String,
}

// OpenAI Chat Completion Request Schema (Simplified)
#[derive(Debug, Serialize)]
struct OpenAIChatRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIChatResponse {
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    total_tokens: u32,
}

// ============================================================
// Handlers
// ============================================================

/// POST /ai/validate
/// Tests connection to the configured AI provider
pub async fn validate_connection(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<AiConnectionRequest>,
) -> Result<Json<AiConnectionResponse>> {
    debug!("Validating AI connection to {}", payload.provider_url);

    // Validate URL format and security
    if let Err(err_msg) = validate_provider_url(&payload.provider_url) {
        return Ok(Json(AiConnectionResponse {
            success: false,
            message: format!("Invalid provider URL: {}", err_msg),
        }));
    }

    let client = reqwest::Client::new();

    // We'll try a simple completion or models list request to verify connectivity
    // Using /models for Ollama or OpenAI usually works
    let url = format!("{}/models", payload.provider_url.trim_end_matches('/'));

    // First try listing models endpoint (works for Ollama and OpenAI)
    let request_builder = client.get(&url);
    let request_builder = add_auth_header(request_builder, &payload.api_key);

    match request_builder.send().await {
        Ok(res) => {
            let status = res.status();
            if status.is_success() {
                // Verify body is valid JSON to avoid "fake 200" from some servers (like LM Studio on wrong endpoint)
                let body_text = res.text().await.unwrap_or_default();
                if serde_json::from_str::<serde_json::Value>(&body_text).is_ok() {
                    Ok(Json(AiConnectionResponse {
                        success: true,
                        message: format!("Successfully connected to {}", payload.provider_url),
                    }))
                } else {
                    warn!(
                        "AI Connection returned 200 but invalid JSON. Status: {}, Body preview: {}",
                        status,
                        body_text.chars().take(200).collect::<String>()
                    );
                    Ok(Json(AiConnectionResponse {
                        success: false,
                        message: "Connected but received invalid JSON response. Ensure URL ends with /v1 if required (e.g. http://localhost:1234/v1). Check server logs for response details.".to_string(),
                    }))
                }
            } else {
                error!("AI Connection failed. Status: {}, URL: {}", status, url);
                Ok(Json(AiConnectionResponse {
                    success: false,
                    message: format!(
                        "Connection failed with HTTP {}. Check provider URL and credentials.",
                        status
                    ),
                }))
            }
        }
        Err(e) => {
            error!("AI Connection error: {} (URL: {})", e, url);
            Ok(Json(AiConnectionResponse {
                success: false,
                message: format!(
                    "Connection error: {}. Ensure provider is running and accessible.",
                    if e.is_connect() {
                        "Unable to connect to provider"
                    } else if e.is_timeout() {
                        "Request timeout"
                    } else {
                        "Network error"
                    }
                ),
            }))
        }
    }
}

/// POST /ai/generate
/// Generates an intelligence report based on screen activity
pub async fn generate_report(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AiReportRequest>,
) -> Result<Json<AiReportResponse>> {
    debug!("Generating AI report with model {}", payload.model);

    // 1. Fetch Data Context using RAG
    let end_time = payload.end_time.unwrap_or_else(Utc::now);
    let start_time = payload
        .start_time
        .unwrap_or_else(|| end_time - Duration::hours(24));

    // Get or create the user's query for semantic search
    let user_query = payload.prompt.clone().unwrap_or_else(|| {
        format!(
            "Summarize computer activity and productivity from {} to {}",
            start_time.format("%Y-%m-%d %H:%M"),
            end_time.format("%Y-%m-%d %H:%M")
        )
    });

    // Build context using RAG (hybrid search) or traditional approach
    let (context_text, context_source) = crate::handlers::rag_helpers::build_rag_context(
        &state,
        &user_query,
        start_time,
        end_time,
    )
    .await?;

    // 2. Construct Prompt (Senior Productivity Analyst Persona)
    let system_prompt = r#"You are ScreenSearch Intelligence, a Senior Productivity Analyst.
Your goal is to reconstruct a cohesive narrative of the user's work session based on fragmented screen capture logs and OCR text.

INPUT DATA EXPLANATION:
- You will receive a list of "Frames" or "Context Chunks".
- Each item contains Timestamp, App Name, Window Title, and OCR Text (text visible on screen).
- OCR text may be fragmented or partial.
- RAG (retrieval) has prioritized relevant chunks based on the user's query.

ANALYSIS INSTRUCTIONS:
1. SYNTHESIZE, DON'T LIST: Do not just list what the user opened. Explain *what they were doing*. (e.g., instead of "User opened VS Code, then Chrome", say "User was implementing the login feature in VS Code, referencing documentation in Chrome").
2. USE OCR CONTEXT: Use the OCR text to identify specific topics, document names, or code functions being worked on.
3. IDENTIFY FLOWS: Group related activities into workflows (e.g., "Research Phase", "Coding Phase", "Communication").
4. HIGHLIGHT INTERRUPTIONS: Note if the user was frequently context-switching between unrelated apps.

OUTPUT FORMAT (Markdown):
# Executive Summary
(2-3 sentences summarizing the main focus of the period)

## Key Activities
- **[Activity Name]**: Description of work done, citing specific apps and context found in OCR.

## Productivity Analysis
- **Focus**: [High/Medium/Low] - Explanation.
- **Tools Used**: List primary tools.

## Timeline
(Bulleted list of major state changes or milestones)
"#;

    let user_prompt = format!("{}\n\nContext:\n{}", user_query, context_text);

    // 3. Call AI Provider

    // Validate URL format and security
    if let Err(err_msg) = validate_provider_url(&payload.provider_url) {
        return Err(AppError::InvalidRequest(format!(
            "Invalid provider URL: {}",
            err_msg
        )));
    }

    let client = reqwest::Client::new();
    // Ensure we handle URL construction carefully. Most providers need /chat/completions
    let url = format!(
        "{}/chat/completions",
        payload.provider_url.trim_end_matches('/')
    );

    let request_body = OpenAIChatRequest {
        model: payload.model.clone(),
        messages: vec![
            OpenAIMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            OpenAIMessage {
                role: "user".to_string(),
                content: user_prompt,
            },
        ],
        temperature: Some(0.7),
    };

    let request_builder = client.post(&url).json(&request_body);
    let request_builder = add_auth_header(request_builder, &payload.api_key);

    info!("Sending request to AI provider at {}...", url);
    let res = request_builder.send().await.map_err(|e| {
        error!("Failed to contact AI provider: {}", e);
        AppError::Internal(format!(
            "Failed to contact AI provider: {}. Ensure provider is running and accessible.",
            if e.is_connect() {
                "Connection refused"
            } else if e.is_timeout() {
                "Request timeout"
            } else {
                "Network error"
            }
        ))
    })?;

    let status = res.status();
    if !status.is_success() {
        let error_text = res.text().await.unwrap_or_default();
        error!(
            "AI Provider Error. Status: {}, Response: {}",
            status, error_text
        );
        return Err(AppError::Internal(format!(
            "AI Provider returned HTTP {}. {}",
            status,
            if status.as_u16() == 401 {
                "Check API key credentials."
            } else if status.as_u16() == 404 {
                "Endpoint not found. Verify URL ends with correct path (e.g., /v1)."
            } else if status.as_u16() >= 500 {
                "Provider server error. Check provider logs."
            } else {
                "Check server logs for details."
            }
        )));
    }

    let response_text = res
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read response body: {}", e)))?;

    let response_body: OpenAIChatResponse = serde_json::from_str(&response_text).map_err(|e| {
        error!(
            "Failed to parse AI response. Parse error: {}, Body preview: {}",
            e,
            response_text.chars().take(200).collect::<String>()
        );
        AppError::Internal(
            "Failed to parse AI response (invalid JSON format). Check server logs for response details.".to_string()
        )
    })?;

    let report_content = response_body
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_else(|| "No report generated.".to_string());

    let final_report = format!("{}\n\n---\n*Context: {}*", report_content, context_source);

    Ok(Json(AiReportResponse {
        report: final_report,
        model_used: payload.model,
        tokens_used: response_body.usage.map(|u| u.total_tokens),
        context_source,
    }))
}
