//! AI Integration Handlers
//!
//! Handles communication with LLM providers (OpenAI, Ollama, Local) and report generation.

use crate::error::{AppError, Result};
use crate::state::AppState;
use axum::extract::{Json, State};
use chrono::{DateTime, Duration, Utc};
use reqwest::RequestBuilder;
use screensearch_llm::{
    get_models_dir, DownloadProgress as LlmDownloadProgress, MODEL_FILENAME, MODEL_SIZE_BYTES,
};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Local LLM server endpoint (llama.cpp server on port 31130)
const LOCAL_LLM_ENDPOINT: &str = "http://127.0.0.1:31130/v1";

// ============================================================
// Helper Functions
// ============================================================

/// Whether a local GGUF model is available — either a user-provided model
/// discovered in `.models/` (etc.) or the downloadable default.
fn local_model_available() -> bool {
    screensearch_llm::local_model_available()
}

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
    /// Reasoning/"thinking" output emitted by reasoning models (Qwen3.5, Gemma 4)
    /// when the server runs with `--jinja`. Returned separately from `content`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reasoning_content: Option<String>,
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

    // Handle local provider specially
    if payload.provider_url == "local" {
        let models_dir = get_models_dir();
        if local_model_available() {
            // Check if llama-server is running
            let client = reqwest::Client::new();
            match client
                .get(format!("{}/models", LOCAL_LLM_ENDPOINT))
                .send()
                .await
            {
                Ok(res) if res.status().is_success() => {
                    return Ok(Json(AiConnectionResponse {
                        success: true,
                        message: "Local Ministral-3B model is ready. llama-server is running."
                            .to_string(),
                    }));
                }
                _ => {
                    return Ok(Json(AiConnectionResponse {
                        success: false,
                        message: format!(
                            "Model downloaded but llama-server not running. Start it with: llama-server -m {}",
                            models_dir.join(MODEL_FILENAME).display()
                        ),
                    }));
                }
            }
        } else {
            let size_gb = MODEL_SIZE_BYTES as f64 / 1_000_000_000.0;
            return Ok(Json(AiConnectionResponse {
                success: false,
                message: format!(
                    "Local model not downloaded. Model size: {:.2} GB. Download required.",
                    size_gb
                ),
            }));
        }
    }

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
///
/// For local provider (`provider_url == "local"`), this function:
/// 1. Checks if the Ministral-3B model is downloaded
/// 2. Checks if the llama-server binary is available  
/// 3. Auto-starts the llama-server if not already running (Lazy Loading)
/// 4. Uses the server's dynamic port (handles fallback ports 31131, 31132)
///
/// This ensures users don't need to manually start the server from Settings.
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
    let (context_text, context_source) =
        crate::handlers::rag_helpers::build_rag_context(&state, &user_query, start_time, end_time)
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

    // Determine effective URL - use local endpoint if provider is "local"
    let (effective_url, effective_model) = if payload.provider_url == "local" {
        // Check if a local model is available
        if !local_model_available() {
            return Err(AppError::InvalidRequest(
                "Local model not downloaded. Please download the model first.".to_string(),
            ));
        }

        // Check if llama-server binary is available and matches the pinned build
        // (an older build is treated as missing so it gets re-downloaded).
        let bin_dir = screensearch_llm::get_bin_dir();
        if !screensearch_llm::llama_server_up_to_date(&bin_dir) {
            return Err(AppError::InvalidRequest(
                "llama-server not downloaded (or outdated). Please download it from Settings → AI → Download Server.".to_string()
            ));
        }

        // Get or create llama-server and ensure it's running (auto-start on demand)
        let server = state
            .get_llama_server()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to initialize llama-server: {}", e)))?;

        // Auto-start the server if not running
        if let Err(e) = server.ensure_started().await {
            return Err(AppError::Internal(format!(
                "Failed to start local LLM server: {}. Check logs for details.",
                e
            )));
        }

        // Get the endpoint from the running server (handles port fallback)
        let endpoint = format!("{}/v1", server.endpoint().await);
        // Report the actual GGUF the server resolved (not a hardcoded name).
        let model_name = screensearch_llm::resolve_model_path()
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "local".to_string());
        (endpoint, model_name)
    } else {
        (payload.provider_url.clone(), payload.model.clone())
    };

    // Validate URL format and security (skip for local which we already handle)
    if payload.provider_url != "local" {
        if let Err(err_msg) = validate_provider_url(&effective_url) {
            return Err(AppError::InvalidRequest(format!(
                "Invalid provider URL: {}",
                err_msg
            )));
        }
    }

    let client = reqwest::Client::new();
    // Ensure we handle URL construction carefully. Most providers need /chat/completions
    let url = format!("{}/chat/completions", effective_url.trim_end_matches('/'));

    let request_body = OpenAIChatRequest {
        model: effective_model.clone(),
        messages: vec![
            OpenAIMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
                reasoning_content: None,
            },
            OpenAIMessage {
                role: "user".to_string(),
                content: user_prompt,
                reasoning_content: None,
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

    // Use the model's answer (`content`). If a reasoning model exhausted the
    // context window thinking and never produced a final answer, `content` is
    // empty — surface a clear note instead of a blank report.
    let report_content = response_body
        .choices
        .first()
        .map(|c| {
            let content = c.message.content.trim();
            if !content.is_empty() {
                content.to_string()
            } else if c.message.reasoning_content.is_some() {
                "_The model spent the full context window reasoning and did not produce a final answer. Try a narrower time range or a smaller/non-reasoning model._".to_string()
            } else {
                "No report generated.".to_string()
            }
        })
        .unwrap_or_else(|| "No report generated.".to_string());

    // Add metadata header with generation date and model
    let generation_timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    let metadata_header = format!(
        "---\n**Generated:** {}\n**Model:** {}\n**Time Period:** {} to {}\n---\n\n",
        generation_timestamp,
        effective_model,
        start_time.format("%Y-%m-%d %H:%M"),
        end_time.format("%Y-%m-%d %H:%M")
    );

    let final_report = format!(
        "{}{}\n\n---\n*Context: {}*",
        metadata_header, report_content, context_source
    );

    Ok(Json(AiReportResponse {
        report: final_report,
        model_used: effective_model,
        tokens_used: response_body.usage.map(|u| u.total_tokens),
        context_source,
    }))
}

// ============================================================
// Local Model Management Handlers
// ============================================================

#[derive(Debug, Serialize)]
pub struct ModelStatusResponse {
    pub downloaded: bool,
    pub downloading: bool,
    pub model_name: String,
    pub model_size_bytes: u64,
    pub model_path: Option<String>,
    /// All GGUF models discovered locally (e.g. dropped into `.models/`), as
    /// absolute paths. The first entry is the one the server will use.
    pub available_models: Vec<String>,
}

/// GET /ai/model/status
/// Returns the status of the local Ministral-3B model
pub async fn get_model_status(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<ModelStatusResponse>> {
    // Prefer a user-provided GGUF discovered in `.models/` (etc.); fall back to
    // the downloadable default model.
    let discovered = screensearch_llm::discover_local_models();
    let available_models: Vec<String> = discovered
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    let models_dir = get_models_dir();
    let (downloaded, model_name, model_path) = match discovered.first() {
        Some(path) => {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "local-gguf".to_string());
            (true, name, Some(path.to_string_lossy().to_string()))
        }
        None => {
            let downloaded = local_model_available();
            let model_path = downloaded.then(|| {
                models_dir
                    .join(MODEL_FILENAME)
                    .to_string_lossy()
                    .to_string()
            });
            (
                downloaded,
                "Ministral-3B-Instruct-2512-Q4_K_M".to_string(),
                model_path,
            )
        }
    };

    Ok(Json(ModelStatusResponse {
        downloaded,
        downloading: false, // TODO: Track download state in AppState
        model_name,
        model_size_bytes: MODEL_SIZE_BYTES,
        model_path,
        available_models,
    }))
}

#[derive(Debug, Serialize)]
pub struct ModelDownloadResponse {
    pub success: bool,
    pub message: String,
}

/// POST /ai/model/download
/// Triggers download of the local Ministral-3B model
pub async fn start_model_download(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ModelDownloadResponse>> {
    let models_dir = get_models_dir();

    // Check if already downloaded
    if local_model_available() {
        return Ok(Json(ModelDownloadResponse {
            success: true,
            message: "Model already downloaded".to_string(),
        }));
    }

    // Create progress channel
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<LlmDownloadProgress>(100);

    // Create oneshot channel to signal completion status (Ok = success, Err = error message)
    let (completion_tx, completion_rx) =
        tokio::sync::oneshot::channel::<std::result::Result<(), String>>();

    // Clone state for background task
    let state_clone = state.clone();

    // Spawn receiver task to update progress in state
    tokio::spawn(async move {
        // Process all progress updates
        while let Some(progress) = progress_rx.recv().await {
            state_clone
                .update_download_progress("llm_model".to_string(), progress.into())
                .await;
        }

        // Wait for completion signal from download task (with 2-hour timeout to prevent task leak)
        use tokio::time::{timeout, Duration};
        match timeout(Duration::from_secs(7200), completion_rx).await {
            Ok(Ok(Ok(()))) => {
                // Success - clear progress
                state_clone.clear_download_progress("llm_model").await;
            }
            Ok(Ok(Err(error_msg))) => {
                // Error - update with error state
                use crate::state::DownloadProgress;
                state_clone
                    .update_download_progress(
                        "llm_model".to_string(),
                        DownloadProgress {
                            bytes_downloaded: 0,
                            total_bytes: 0,
                            speed_bps: 0,
                            eta_seconds: 0,
                            error: Some(error_msg),
                        },
                    )
                    .await;
            }
            Ok(Err(_)) => {
                // Channel closed without signal - treat as error
                warn!("Download completion channel closed unexpectedly for llm_model");
            }
            Err(_) => {
                // Timeout - download task likely panicked or hung
                error!("Download receiver task timed out after 2 hours for llm_model");
                use crate::state::DownloadProgress;
                state_clone
                    .update_download_progress(
                        "llm_model".to_string(),
                        DownloadProgress {
                            bytes_downloaded: 0,
                            total_bytes: 0,
                            speed_bps: 0,
                            eta_seconds: 0,
                            error: Some("Download timed out after 2 hours".to_string()),
                        },
                    )
                    .await;
            }
        }
    });

    // Start download in background
    tokio::spawn(async move {
        info!("Starting model download in background...");
        let result =
            screensearch_llm::download_model_with_progress(&models_dir, Some(progress_tx)).await;

        // Close progress channel to signal end of progress updates

        match result {
            Ok(_) => {
                info!("Model download completed successfully");
                // Signal success to receiver task
                let _ = completion_tx.send(Ok(()));
            }
            Err(e) => {
                error!("Model download failed: {}", e);
                // Send error message to receiver task
                let _ = completion_tx.send(Err(format!("Download failed: {}", e)));
            }
        }
    });

    Ok(Json(ModelDownloadResponse {
        success: true,
        message: format!(
            "Download started. Model size: {:.2} GB",
            MODEL_SIZE_BYTES as f64 / 1_000_000_000.0
        ),
    }))
}

// ============================================================
// llama-server Management Handlers
// ============================================================

#[derive(Debug, Serialize)]
pub struct ServerStatusResponse {
    pub status: String,
    pub port: u16,
    pub idle_seconds: u64,
    pub model_loaded: bool,
    pub server_binary_available: bool,
}

/// GET /ai/server/status
/// Returns the status of the local llama-server process
pub async fn get_server_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ServerStatusResponse>> {
    use screensearch_llm::{get_bin_dir, llama_server_up_to_date, ServerStatus};

    let bin_dir = get_bin_dir();
    // "Available" means present AND matching the pinned build, so the UI prompts a
    // re-download after a llama.cpp version bump.
    let server_binary_available = llama_server_up_to_date(&bin_dir);

    // Check if server is initialized
    let guard = state.llama_server.read().await;
    if let Some(server) = guard.as_ref() {
        let status = server.status().await;
        let port = server.active_port().await;
        let idle_duration = server.idle_duration().await;

        Ok(Json(ServerStatusResponse {
            status: status.as_str().to_string(),
            port,
            idle_seconds: idle_duration.as_secs(),
            model_loaded: status == ServerStatus::Running,
            server_binary_available,
        }))
    } else {
        Ok(Json(ServerStatusResponse {
            status: "stopped".to_string(),
            port: screensearch_llm::DEFAULT_LLAMA_PORT,
            idle_seconds: 0,
            model_loaded: false,
            server_binary_available,
        }))
    }
}

#[derive(Debug, Serialize)]
pub struct ServerControlResponse {
    pub success: bool,
    pub message: String,
    pub status: String,
}

/// POST /ai/server/start
/// Start the llama-server process
pub async fn start_server(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ServerControlResponse>> {
    // Check prerequisites
    if !local_model_available() {
        return Ok(Json(ServerControlResponse {
            success: false,
            message: "Model not downloaded. Please download the model first.".to_string(),
            status: "error".to_string(),
        }));
    }

    let bin_dir = screensearch_llm::get_bin_dir();
    if !screensearch_llm::llama_server_up_to_date(&bin_dir) {
        return Ok(Json(ServerControlResponse {
            success: false,
            message: "llama-server not found. Downloading...".to_string(),
            status: "error".to_string(),
        }));
    }

    // Get or create server
    match state.get_llama_server().await {
        Ok(server) => {
            // Try to start it
            match server.ensure_started().await {
                Ok(()) => {
                    let status = server.status().await;
                    Ok(Json(ServerControlResponse {
                        success: true,
                        message: format!("Server running on port {}", server.active_port().await),
                        status: status.as_str().to_string(),
                    }))
                }
                Err(e) => Ok(Json(ServerControlResponse {
                    success: false,
                    message: format!("Failed to start server: {}", e),
                    status: "error".to_string(),
                })),
            }
        }
        Err(e) => Ok(Json(ServerControlResponse {
            success: false,
            message: format!("Failed to initialize server: {}", e),
            status: "error".to_string(),
        })),
    }
}

/// POST /ai/server/stop
/// Stop the llama-server process
pub async fn stop_server(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ServerControlResponse>> {
    let guard = state.llama_server.read().await;
    if let Some(server) = guard.as_ref() {
        if let Err(e) = server.stop().await {
            return Ok(Json(ServerControlResponse {
                success: false,
                message: format!("Failed to stop server: {}", e),
                status: "error".to_string(),
            }));
        }
        Ok(Json(ServerControlResponse {
            success: true,
            message: "Server stopped".to_string(),
            status: "stopped".to_string(),
        }))
    } else {
        Ok(Json(ServerControlResponse {
            success: true,
            message: "Server was not running".to_string(),
            status: "stopped".to_string(),
        }))
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateTtlRequest {
    pub ttl_seconds: u64,
}

/// POST /ai/server/ttl
/// Update the server idle timeout
pub async fn update_server_ttl(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateTtlRequest>,
) -> Result<Json<ServerControlResponse>> {
    use std::time::Duration;

    // Validate TTL (minimum 1 minute, maximum 1 hour)
    if payload.ttl_seconds < 60 {
        return Ok(Json(ServerControlResponse {
            success: false,
            message: "TTL must be at least 60 seconds".to_string(),
            status: "error".to_string(),
        }));
    }
    if payload.ttl_seconds > 3600 {
        return Ok(Json(ServerControlResponse {
            success: false,
            message: "TTL cannot exceed 3600 seconds (1 hour)".to_string(),
            status: "error".to_string(),
        }));
    }

    let guard = state.llama_server.read().await;
    if let Some(server) = guard.as_ref() {
        server
            .set_idle_ttl(Duration::from_secs(payload.ttl_seconds))
            .await;
        Ok(Json(ServerControlResponse {
            success: true,
            message: format!("TTL updated to {} seconds", payload.ttl_seconds),
            status: server.status().await.as_str().to_string(),
        }))
    } else {
        Ok(Json(ServerControlResponse {
            success: false,
            message: "Server not initialized".to_string(),
            status: "stopped".to_string(),
        }))
    }
}

/// POST /ai/server/download
/// Download the llama-server binary
pub async fn download_llama_server(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ModelDownloadResponse>> {
    use screensearch_llm::{get_bin_dir, llama_server_up_to_date, LLAMA_SERVER_SIZE_BYTES};

    let bin_dir = get_bin_dir();

    // Check if already downloaded at the pinned build (re-download if outdated)
    if llama_server_up_to_date(&bin_dir) {
        return Ok(Json(ModelDownloadResponse {
            success: true,
            message: "llama-server already downloaded".to_string(),
        }));
    }

    // Create progress channel
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<LlmDownloadProgress>(100);

    // Create oneshot channel to signal completion status (Ok = success, Err = error message)
    let (completion_tx, completion_rx) =
        tokio::sync::oneshot::channel::<std::result::Result<(), String>>();

    // Clone state for background task
    let state_clone = state.clone();

    // Spawn receiver task to update progress in state
    tokio::spawn(async move {
        // Process all progress updates
        while let Some(progress) = progress_rx.recv().await {
            state_clone
                .update_download_progress("llama_server".to_string(), progress.into())
                .await;
        }

        // Wait for completion signal from download task (with 2-hour timeout to prevent task leak)
        use tokio::time::{timeout, Duration};
        match timeout(Duration::from_secs(7200), completion_rx).await {
            Ok(Ok(Ok(()))) => {
                // Success - clear progress
                state_clone.clear_download_progress("llama_server").await;
            }
            Ok(Ok(Err(error_msg))) => {
                // Error - update with error state
                use crate::state::DownloadProgress;
                state_clone
                    .update_download_progress(
                        "llama_server".to_string(),
                        DownloadProgress {
                            bytes_downloaded: 0,
                            total_bytes: 0,
                            speed_bps: 0,
                            eta_seconds: 0,
                            error: Some(error_msg),
                        },
                    )
                    .await;
            }
            Ok(Err(_)) => {
                // Channel closed without signal - treat as error
                warn!("Download completion channel closed unexpectedly for llama_server");
            }
            Err(_) => {
                // Timeout - download task likely panicked or hung
                error!("Download receiver task timed out after 2 hours for llama_server");
                use crate::state::DownloadProgress;
                state_clone
                    .update_download_progress(
                        "llama_server".to_string(),
                        DownloadProgress {
                            bytes_downloaded: 0,
                            total_bytes: 0,
                            speed_bps: 0,
                            eta_seconds: 0,
                            error: Some("Download timed out after 2 hours".to_string()),
                        },
                    )
                    .await;
            }
        }
    });

    // Start download in background
    tokio::spawn(async move {
        info!("Starting llama-server download in background...");
        let result = screensearch_llm::download_llama_server_with_progress(Some(progress_tx)).await;

        // Close progress channel to signal end of progress updates

        match result {
            Ok(path) => {
                info!("llama-server downloaded to {:?}", path);
                // Signal success to receiver task
                let _ = completion_tx.send(Ok(()));
            }
            Err(e) => {
                error!("llama-server download failed: {}", e);
                // Send error message to receiver task
                let _ = completion_tx.send(Err(format!("Download failed: {}", e)));
            }
        }
    });

    Ok(Json(ModelDownloadResponse {
        success: true,
        message: format!(
            "Download started. Binary size: ~{:.1} MB",
            LLAMA_SERVER_SIZE_BYTES as f64 / 1_000_000.0
        ),
    }))
}
