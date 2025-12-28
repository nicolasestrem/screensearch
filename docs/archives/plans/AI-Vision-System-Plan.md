# AI Vision System - Detailed Implementation Plan

## Executive Summary

Replace the current Windows OCR engine (which produces noisy text from UI elements, toolbars, and truncated content) with an AI-powered vision system using DeepSeek-VL2-Tiny. This enables semantic understanding of screenshots, rich contextual descriptions, and natural language queries like "What was that yellow design I saw Tuesday?"

---

## Table of Contents

1. [Problem Statement](#1-problem-statement)
2. [Solution Architecture](#2-solution-architecture)
3. [New Workspace Crate: screensearch-vision](#3-new-workspace-crate-screensearch-vision)
4. [Database Schema Changes](#4-database-schema-changes)
5. [Backend API Implementation](#5-backend-api-implementation)
6. [Background Processing Worker](#6-background-processing-worker)
7. [Embedding Pipeline Modifications](#7-embedding-pipeline-modifications)
8. [Frontend UI Implementation](#8-frontend-ui-implementation)
9. [Configuration System](#9-configuration-system)
10. [Main Binary Integration](#10-main-binary-integration)
11. [Model Management](#11-model-management)
12. [Error Handling Strategy](#12-error-handling-strategy)
13. [Performance Considerations](#13-performance-considerations)
14. [Testing Strategy](#14-testing-strategy)
15. [Migration Path](#15-migration-path)
16. [File Change Summary](#16-file-change-summary)

---

## 1. Problem Statement

### Current Issues

**Windows OCR produces noisy, low-quality text:**
- Captures toolbar text: "File Edit View Help"
- Captures UI components: "Save", "Cancel", "OK", "×"
- Captures truncated text: "Meeting with Jo..."
- Captures system UI: "11:42 AM", "WiFi", "Battery 87%"
- No semantic understanding of what the user is actually doing

**Impact on RAG pipeline:**
- Embeddings created from garbage text
- Search returns irrelevant results
- AI reports lack meaningful context
- Users can't find what they're looking for

### Desired Outcome

Users should be able to ask:
- "What was the name of that yellow design I saw Tuesday around 11am?"
- "Summarize what I read on Reddit today"
- "How long did I spend on the ScreenSearch project this week?"
- "Find when I was looking at that budget spreadsheet"

---

## 2. Solution Architecture

### High-Level Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           CURRENT ARCHITECTURE                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Screenshot ──► Windows OCR ──► Noisy Text ──► Embeddings ──► Poor Search   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘

                                    ▼ REPLACE WITH ▼

┌─────────────────────────────────────────────────────────────────────────────┐
│                            NEW ARCHITECTURE                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Screenshot ──► Analysis Queue ──► DeepSeek-VL2-Tiny ──► Rich Description   │
│       │              │                     │                    │            │
│       │         [priority]            [llama.cpp]               │            │
│       │              │                     │                    ▼            │
│       │              │                     │         ┌─────────────────────┐ │
│       │              │                     │         │ "User is reviewing  │ │
│       │              │                     │         │  a Figma design     │ │
│       │              │                     │         │  called 'Sunrise    │ │
│       │              │                     │         │  Dashboard' with    │ │
│       │              │                     │         │  orange/yellow      │ │
│       │              │                     │         │  color scheme..."   │ │
│       │              │                     │         └─────────────────────┘ │
│       │              │                     │                    │            │
│       │              │                     │                    ▼            │
│       │              │                     │              Embeddings         │
│       │              │                     │              (MiniLM)           │
│       │              │                     │                    │            │
│       │              │                     │                    ▼            │
│       │              │                     │            Semantic Search      │
│       │              │                     │                    │            │
│       │              │                     │                    ▼            │
│       │              │                     │         ┌─────────────────────┐ │
│       │              │                     │         │   "Ask" Interface   │ │
│       │              │                     │         │   Natural Language  │ │
│       │              │                     │         │   Queries + Answers │ │
│       │              │                     │         └─────────────────────┘ │
│       │              │                     │                                 │
└───────┴──────────────┴─────────────────────┴─────────────────────────────────┘
```

### Processing Modes

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          PROCESSING TRIGGERS                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. BACKGROUND PROCESSING (automatic)                                        │
│     ├─ Runs when system is idle (CPU < threshold)                           │
│     ├─ Processes oldest pending frames first                                 │
│     ├─ Configurable batch size (default: 5 frames)                          │
│     └─ Pauses when user becomes active                                       │
│                                                                              │
│  2. ON-DEMAND PROCESSING (user-triggered)                                    │
│     ├─ User clicks "Process Now" on time range                              │
│     ├─ User searches → unanalyzed frames processed first                    │
│     ├─ Higher priority than background queue                                 │
│     └─ Shows progress indicator                                              │
│                                                                              │
│  3. SEARCH-TIME PROCESSING (automatic)                                       │
│     ├─ When user searches, check if recent frames are unanalyzed            │
│     ├─ Process them immediately before returning results                     │
│     └─ Ensures fresh captures are searchable                                 │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. New Workspace Crate: screensearch-vision

### Directory Structure

```
screensearch-vision/
├── Cargo.toml
├── src/
│   ├── lib.rs                 # Public API exports
│   ├── engine.rs              # VisionEngine - llama.cpp wrapper
│   ├── download.rs            # Model auto-download from HuggingFace
│   ├── prompt.rs              # Prompt templates and parsing
│   ├── queue.rs               # Processing queue manager
│   ├── analysis.rs            # FrameAnalysis types
│   └── error.rs               # Error types
├── models/                    # Created at runtime
│   └── .gitkeep
└── tests/
    ├── engine_tests.rs
    └── prompt_tests.rs
```

### Cargo.toml

```toml
[package]
name = "screensearch-vision"
version = "0.1.0"
edition = "2021"
description = "AI vision analysis for ScreenSearch using DeepSeek-VL2-Tiny"

[dependencies]
# LLM Runtime - llama.cpp Rust bindings
llama-cpp-2 = "0.1"              # Or latest version with vision support

# Image processing
image = { version = "0.25", default-features = false, features = ["jpeg", "png"] }
base64 = "0.22"

# Async runtime
tokio = { version = "1", features = ["sync", "time", "fs"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# HTTP client for model download
reqwest = { version = "0.12", features = ["stream"] }
futures-util = "0.3"

# Progress indication
indicatif = "0.17"

# Error handling
thiserror = "1"
anyhow = "1"

# Logging
tracing = "0.1"

# File system utilities
dirs = "5"

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
tempfile = "3"
```

### src/lib.rs

```rust
//! AI Vision Analysis for ScreenSearch
//!
//! This crate provides vision-language model integration using DeepSeek-VL2-Tiny
//! via llama.cpp for analyzing screenshots and generating rich descriptions.

mod analysis;
mod download;
mod engine;
mod error;
mod prompt;
mod queue;

pub use analysis::{ActivityType, FrameAnalysis, VisibleText};
pub use download::{download_model, get_model_path, is_model_downloaded, ModelInfo};
pub use engine::{VisionEngine, VisionEngineConfig};
pub use error::{VisionError, VisionResult};
pub use prompt::PromptTemplate;
pub use queue::{AnalysisQueue, QueueItem, QueuePriority};

/// Default model identifier
pub const DEFAULT_MODEL: &str = "deepseek-vl2-tiny";

/// Model file size (approximate, for progress indication)
pub const MODEL_SIZE_BYTES: u64 = 2_000_000_000; // ~2GB

/// Embedding dimension (not used directly, but documented for reference)
pub const VISION_OUTPUT_FOR_EMBEDDING: &str = "description + visible_text";
```

### src/error.rs

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VisionError {
    #[error("Model not found: {0}. Run model download first.")]
    ModelNotFound(String),

    #[error("Model download failed: {0}")]
    DownloadFailed(String),

    #[error("Model initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Image processing failed: {0}")]
    ImageProcessingFailed(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),

    #[error("Response parsing failed: {0}")]
    ParseFailed(String),

    #[error("Queue operation failed: {0}")]
    QueueError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Timeout during analysis")]
    Timeout,

    #[error("Engine not initialized")]
    NotInitialized,
}

pub type VisionResult<T> = Result<T, VisionError>;
```

### src/analysis.rs

```rust
use serde::{Deserialize, Serialize};

/// Result of analyzing a screenshot with the vision model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameAnalysis {
    /// Human-readable description of what's happening in the screenshot
    /// Example: "User is reviewing a Figma design called 'Sunrise Dashboard'
    ///           featuring charts and an orange/yellow color scheme"
    pub description: String,

    /// Important visible text extracted from the screenshot
    /// Excludes UI chrome (buttons, menus, toolbars)
    /// Example: ["Sunrise Dashboard", "Q4 Revenue: $1.2M", "Monthly Active Users"]
    pub visible_text: Vec<String>,

    /// Classified activity type
    pub activity_type: ActivityType,

    /// Detected or inferred application name (if identifiable)
    /// Example: Some("Figma"), Some("VS Code"), None
    pub application_hint: Option<String>,

    /// Model's confidence in the analysis (0.0 - 1.0)
    pub confidence: f32,

    /// Raw model output (for debugging)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_output: Option<String>,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

impl FrameAnalysis {
    /// Create an empty/failed analysis result
    pub fn empty() -> Self {
        Self {
            description: String::new(),
            visible_text: Vec::new(),
            activity_type: ActivityType::Unknown,
            application_hint: None,
            confidence: 0.0,
            raw_output: None,
            processing_time_ms: 0,
        }
    }

    /// Convert to text suitable for embedding
    /// Combines description and visible text for the embedding pipeline
    pub fn to_embedding_text(&self) -> String {
        let mut parts = vec![self.description.clone()];
        if !self.visible_text.is_empty() {
            parts.push(self.visible_text.join(" "));
        }
        if let Some(app) = &self.application_hint {
            parts.push(format!("Application: {}", app));
        }
        parts.push(format!("Activity: {}", self.activity_type.as_str()));
        parts.join("\n")
    }

    /// Check if analysis produced meaningful results
    pub fn is_valid(&self) -> bool {
        !self.description.is_empty() && self.confidence > 0.3
    }
}

/// Classified activity types for filtering and aggregation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityType {
    /// Writing or reading code, using IDE/editor
    Coding,
    /// Web browsing, reading articles
    Browsing,
    /// Reading documents, PDFs, ebooks
    Reading,
    /// Design work (Figma, Sketch, etc.)
    Design,
    /// Chat, email, video calls
    Communication,
    /// Watching videos, streaming
    Video,
    /// Spreadsheets, data analysis
    DataAnalysis,
    /// Writing documents, notes
    Writing,
    /// Gaming
    Gaming,
    /// File management, system settings
    SystemAdmin,
    /// Cannot be classified
    Unknown,
}

impl ActivityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Browsing => "browsing",
            Self::Reading => "reading",
            Self::Design => "design",
            Self::Communication => "communication",
            Self::Video => "video",
            Self::DataAnalysis => "data_analysis",
            Self::Writing => "writing",
            Self::Gaming => "gaming",
            Self::SystemAdmin => "system_admin",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "coding" | "programming" | "development" => Self::Coding,
            "browsing" | "web" | "internet" => Self::Browsing,
            "reading" | "document" | "pdf" => Self::Reading,
            "design" | "designing" | "creative" => Self::Design,
            "communication" | "chat" | "email" | "messaging" => Self::Communication,
            "video" | "watching" | "streaming" | "youtube" => Self::Video,
            "data" | "spreadsheet" | "analysis" | "excel" => Self::DataAnalysis,
            "writing" | "document" | "notes" => Self::Writing,
            "gaming" | "game" => Self::Gaming,
            "system" | "settings" | "files" | "admin" => Self::SystemAdmin,
            _ => Self::Unknown,
        }
    }
}

/// Wrapper for visible text with optional metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibleText {
    pub text: String,
    pub importance: TextImportance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextImportance {
    /// Main content (document title, article headline)
    High,
    /// Supporting content (body text, details)
    Medium,
    /// Minor content (captions, labels)
    Low,
}
```

### src/prompt.rs

```rust
use crate::{ActivityType, FrameAnalysis, VisionError, VisionResult};
use serde::Deserialize;

/// Prompt template for the vision model
pub struct PromptTemplate;

impl PromptTemplate {
    /// Generate the analysis prompt for a screenshot
    pub fn analysis_prompt() -> &'static str {
        r#"Analyze this screenshot and provide a JSON response with the following structure:

{
  "description": "A 1-2 sentence description of what the user is doing. Focus on the main activity and important content visible.",
  "visible_text": ["Array of important text visible in the image. Include: document titles, headings, names, key content. Exclude: UI buttons, menu items, toolbar labels, status bar text, window controls."],
  "activity_type": "One of: coding, browsing, reading, design, communication, video, data_analysis, writing, gaming, system_admin, unknown",
  "application": "Name of the application if identifiable, or null",
  "confidence": 0.85
}

Guidelines:
- Focus on MEANINGFUL content the user would want to search for later
- IGNORE all UI chrome: File/Edit/View menus, toolbar buttons, status bars, window controls, taskbar
- IGNORE truncated text that doesn't convey meaning
- For code editors: mention the language and what the code appears to do
- For browsers: mention the website and what content is being viewed
- For documents: mention the document type and subject matter
- Be specific about colors, names, and distinctive features that would help identify this later

Respond ONLY with valid JSON, no additional text."#
    }

    /// Parse the model's JSON response into a FrameAnalysis
    pub fn parse_response(response: &str, processing_time_ms: u64) -> VisionResult<FrameAnalysis> {
        // Try to extract JSON from response (model might include extra text)
        let json_str = Self::extract_json(response)?;

        #[derive(Deserialize)]
        struct ModelResponse {
            description: String,
            visible_text: Vec<String>,
            activity_type: String,
            application: Option<String>,
            confidence: Option<f32>,
        }

        let parsed: ModelResponse = serde_json::from_str(&json_str).map_err(|e| {
            VisionError::ParseFailed(format!(
                "Failed to parse model response: {}. Response was: {}",
                e,
                &response[..response.len().min(200)]
            ))
        })?;

        Ok(FrameAnalysis {
            description: parsed.description,
            visible_text: parsed.visible_text,
            activity_type: ActivityType::from_str(&parsed.activity_type),
            application_hint: parsed.application,
            confidence: parsed.confidence.unwrap_or(0.8),
            raw_output: Some(response.to_string()),
            processing_time_ms,
        })
    }

    /// Extract JSON object from potentially messy model output
    fn extract_json(response: &str) -> VisionResult<String> {
        // Find the first { and last } to extract JSON
        let start = response.find('{').ok_or_else(|| {
            VisionError::ParseFailed("No JSON object found in response".to_string())
        })?;

        let end = response.rfind('}').ok_or_else(|| {
            VisionError::ParseFailed("No closing brace found in response".to_string())
        })?;

        if end <= start {
            return Err(VisionError::ParseFailed(
                "Invalid JSON structure in response".to_string(),
            ));
        }

        Ok(response[start..=end].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_response() {
        let response = r#"{"description": "User is viewing code in VS Code", "visible_text": ["main.rs", "fn main()"], "activity_type": "coding", "application": "VS Code", "confidence": 0.9}"#;

        let analysis = PromptTemplate::parse_response(response, 100).unwrap();

        assert_eq!(analysis.description, "User is viewing code in VS Code");
        assert_eq!(analysis.visible_text, vec!["main.rs", "fn main()"]);
        assert_eq!(analysis.activity_type, ActivityType::Coding);
        assert_eq!(analysis.application_hint, Some("VS Code".to_string()));
        assert_eq!(analysis.confidence, 0.9);
    }

    #[test]
    fn test_parse_response_with_extra_text() {
        let response = r#"Here's my analysis:
        {"description": "Test", "visible_text": [], "activity_type": "unknown", "application": null, "confidence": 0.5}
        Hope this helps!"#;

        let analysis = PromptTemplate::parse_response(response, 100).unwrap();
        assert_eq!(analysis.description, "Test");
    }

    #[test]
    fn test_activity_type_parsing() {
        assert_eq!(ActivityType::from_str("coding"), ActivityType::Coding);
        assert_eq!(ActivityType::from_str("CODING"), ActivityType::Coding);
        assert_eq!(ActivityType::from_str("programming"), ActivityType::Coding);
        assert_eq!(ActivityType::from_str("invalid"), ActivityType::Unknown);
    }
}
```

### src/download.rs

```rust
use crate::{VisionError, VisionResult, MODEL_SIZE_BYTES};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{info, warn};

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    pub size_bytes: u64,
}

impl Default for ModelInfo {
    fn default() -> Self {
        Self {
            name: "deepseek-vl2-tiny",
            filename: "deepseek-vl2-tiny.gguf",
            // Note: Update this URL when the GGUF version is available
            // Currently using placeholder - actual URL depends on quantization
            url: "https://huggingface.co/deepseek-ai/deepseek-vl2-tiny/resolve/main/deepseek-vl2-tiny-Q4_K_M.gguf",
            size_bytes: MODEL_SIZE_BYTES,
        }
    }
}

/// Get the default model storage directory
pub fn get_models_dir() -> PathBuf {
    // Use the crate's models directory or a user-specific location
    let base = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screensearch")
        .join("models");
    base
}

/// Get the full path to the model file
pub fn get_model_path(model_info: &ModelInfo) -> PathBuf {
    get_models_dir().join(model_info.filename)
}

/// Check if the model is already downloaded
pub async fn is_model_downloaded(model_info: &ModelInfo) -> bool {
    let path = get_model_path(model_info);
    if !path.exists() {
        return false;
    }

    // Verify file size is reasonable (at least 1GB for a valid model)
    match fs::metadata(&path).await {
        Ok(meta) => meta.len() > 1_000_000_000,
        Err(_) => false,
    }
}

/// Download the model from HuggingFace
pub async fn download_model(
    model_info: &ModelInfo,
    progress_callback: Option<Box<dyn Fn(u64, u64) + Send>>,
) -> VisionResult<PathBuf> {
    let models_dir = get_models_dir();
    fs::create_dir_all(&models_dir).await?;

    let model_path = get_model_path(model_info);

    // Check if already downloaded
    if is_model_downloaded(model_info).await {
        info!("Model already downloaded: {:?}", model_path);
        return Ok(model_path);
    }

    info!("Downloading model from: {}", model_info.url);

    let client = reqwest::Client::new();
    let response = client
        .get(model_info.url)
        .send()
        .await
        .map_err(|e| VisionError::DownloadFailed(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(VisionError::DownloadFailed(format!(
            "HTTP error: {}",
            response.status()
        )));
    }

    let total_size = response.content_length().unwrap_or(model_info.size_bytes);

    // Create progress bar
    let progress = ProgressBar::new(total_size);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Download with progress
    let temp_path = model_path.with_extension("tmp");
    let mut file = fs::File::create(&temp_path).await?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| VisionError::DownloadFailed(e.to_string()))?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        progress.set_position(downloaded);

        if let Some(ref callback) = progress_callback {
            callback(downloaded, total_size);
        }
    }

    file.flush().await?;
    drop(file);

    // Rename temp file to final name
    fs::rename(&temp_path, &model_path).await?;

    progress.finish_with_message("Download complete");
    info!("Model downloaded to: {:?}", model_path);

    Ok(model_path)
}

/// Delete the downloaded model
pub async fn delete_model(model_info: &ModelInfo) -> VisionResult<()> {
    let path = get_model_path(model_info);
    if path.exists() {
        fs::remove_file(&path).await?;
        info!("Deleted model: {:?}", path);
    }
    Ok(())
}

/// Get download status
#[derive(Debug, Clone)]
pub struct DownloadStatus {
    pub is_downloaded: bool,
    pub file_size: Option<u64>,
    pub path: PathBuf,
}

pub async fn get_download_status(model_info: &ModelInfo) -> DownloadStatus {
    let path = get_model_path(model_info);
    let is_downloaded = is_model_downloaded(model_info).await;
    let file_size = if is_downloaded {
        fs::metadata(&path).await.ok().map(|m| m.len())
    } else {
        None
    };

    DownloadStatus {
        is_downloaded,
        file_size,
        path,
    }
}
```

### src/engine.rs

```rust
use crate::{
    download::{get_model_path, is_model_downloaded, ModelInfo},
    prompt::PromptTemplate,
    FrameAnalysis, VisionError, VisionResult,
};
use image::DynamicImage;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument};

/// Configuration for the VisionEngine
#[derive(Debug, Clone)]
pub struct VisionEngineConfig {
    /// Use GPU if available
    pub use_gpu: bool,
    /// Number of GPU layers to offload (0 = CPU only)
    pub n_gpu_layers: u32,
    /// Context size for the model
    pub context_size: u32,
    /// Number of threads for CPU inference
    pub n_threads: u32,
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// Temperature for generation (lower = more deterministic)
    pub temperature: f32,
    /// Timeout for inference in seconds
    pub timeout_secs: u64,
}

impl Default for VisionEngineConfig {
    fn default() -> Self {
        Self {
            use_gpu: true,
            n_gpu_layers: 99, // Offload all layers if GPU available
            context_size: 4096,
            n_threads: 4,
            max_tokens: 512,
            temperature: 0.1, // Low temperature for consistent JSON output
            timeout_secs: 30,
        }
    }
}

/// Vision analysis engine using DeepSeek-VL2-Tiny via llama.cpp
pub struct VisionEngine {
    // Note: Actual implementation depends on llama-cpp-2 crate API
    // This is a placeholder structure
    config: VisionEngineConfig,
    model_info: ModelInfo,
    // The actual llama.cpp context would be stored here
    // context: Arc<Mutex<LlamaContext>>,
    initialized: Arc<Mutex<bool>>,
}

impl VisionEngine {
    /// Create a new VisionEngine instance
    ///
    /// This will load the model into memory. Ensure the model is downloaded first
    /// using `download::download_model()`.
    #[instrument(skip_all)]
    pub async fn new() -> VisionResult<Self> {
        Self::with_config(VisionEngineConfig::default()).await
    }

    /// Create a new VisionEngine with custom configuration
    #[instrument(skip_all, fields(use_gpu = config.use_gpu, n_threads = config.n_threads))]
    pub async fn with_config(config: VisionEngineConfig) -> VisionResult<Self> {
        let model_info = ModelInfo::default();

        // Check if model is downloaded
        if !is_model_downloaded(&model_info).await {
            return Err(VisionError::ModelNotFound(
                "Model not downloaded. Call download_model() first.".to_string(),
            ));
        }

        let model_path = get_model_path(&model_info);
        info!("Loading vision model from: {:?}", model_path);

        // TODO: Initialize llama.cpp context
        // This is where we'd actually load the model:
        //
        // let params = LlamaModelParams::default()
        //     .with_n_gpu_layers(config.n_gpu_layers);
        // let model = LlamaModel::load_from_file(&model_path, params)?;
        // let ctx_params = LlamaContextParams::default()
        //     .with_n_ctx(config.context_size)
        //     .with_n_threads(config.n_threads);
        // let context = model.create_context(ctx_params)?;

        info!("Vision engine initialized successfully");

        Ok(Self {
            config,
            model_info,
            initialized: Arc::new(Mutex::new(true)),
        })
    }

    /// Analyze a screenshot image
    #[instrument(skip(self, image_data), fields(image_size = image_data.len()))]
    pub async fn analyze(&self, image_data: &[u8]) -> VisionResult<FrameAnalysis> {
        let start = Instant::now();

        // Verify engine is initialized
        if !*self.initialized.lock().await {
            return Err(VisionError::NotInitialized);
        }

        // Decode and preprocess image
        let image = image::load_from_memory(image_data)
            .map_err(|e| VisionError::ImageProcessingFailed(e.to_string()))?;

        // Resize if too large (max 1024px on longest side for efficiency)
        let image = Self::resize_for_inference(image);

        // Convert to base64 for model input
        let image_base64 = Self::image_to_base64(&image)?;

        debug!("Image preprocessed, size: {}x{}", image.width(), image.height());

        // Get the prompt
        let prompt = PromptTemplate::analysis_prompt();

        // TODO: Run inference with llama.cpp
        // This is where we'd actually run the model:
        //
        // let mut output = String::new();
        // let tokens = self.tokenize_with_image(&prompt, &image_base64)?;
        //
        // for token in self.context.generate(tokens, self.config.max_tokens)? {
        //     output.push_str(&token);
        // }

        // Placeholder response for now
        let model_output = self.run_inference(&prompt, &image_base64).await?;

        let processing_time_ms = start.elapsed().as_millis() as u64;
        debug!("Inference completed in {}ms", processing_time_ms);

        // Parse the response
        PromptTemplate::parse_response(&model_output, processing_time_ms)
    }

    /// Analyze a screenshot from a file path
    pub async fn analyze_file(&self, path: &std::path::Path) -> VisionResult<FrameAnalysis> {
        let image_data = tokio::fs::read(path).await?;
        self.analyze(&image_data).await
    }

    /// Resize image for efficient inference
    fn resize_for_inference(image: DynamicImage) -> DynamicImage {
        const MAX_DIMENSION: u32 = 1024;

        let (width, height) = (image.width(), image.height());
        let max_dim = width.max(height);

        if max_dim <= MAX_DIMENSION {
            return image;
        }

        let scale = MAX_DIMENSION as f32 / max_dim as f32;
        let new_width = (width as f32 * scale) as u32;
        let new_height = (height as f32 * scale) as u32;

        image.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
    }

    /// Convert image to base64 for model input
    fn image_to_base64(image: &DynamicImage) -> VisionResult<String> {
        use base64::Engine;
        use std::io::Cursor;

        let mut buffer = Cursor::new(Vec::new());
        image
            .write_to(&mut buffer, image::ImageFormat::Jpeg)
            .map_err(|e| VisionError::ImageProcessingFailed(e.to_string()))?;

        Ok(base64::engine::general_purpose::STANDARD.encode(buffer.into_inner()))
    }

    /// Run inference with the model (placeholder implementation)
    async fn run_inference(&self, _prompt: &str, _image_base64: &str) -> VisionResult<String> {
        // TODO: Implement actual llama.cpp inference
        // For now, return a placeholder that demonstrates the expected format

        // In real implementation:
        // 1. Tokenize the prompt with image embedding
        // 2. Run autoregressive generation
        // 3. Stop at max_tokens or end-of-sequence
        // 4. Return the generated text

        // Placeholder for development/testing:
        Err(VisionError::NotInitialized)
    }

    /// Check if the engine is ready for inference
    pub async fn is_ready(&self) -> bool {
        *self.initialized.lock().await
    }

    /// Get the current configuration
    pub fn config(&self) -> &VisionEngineConfig {
        &self.config
    }

    /// Get model information
    pub fn model_info(&self) -> &ModelInfo {
        &self.model_info
    }

    /// Shutdown the engine and free resources
    pub async fn shutdown(&self) {
        let mut initialized = self.initialized.lock().await;
        *initialized = false;
        info!("Vision engine shut down");
    }
}

// Ensure engine can be shared across threads
unsafe impl Send for VisionEngine {}
unsafe impl Sync for VisionEngine {}
```

### src/queue.rs

```rust
use crate::VisionResult;
use serde::{Deserialize, Serialize};
use std::collections::BinaryHeap;
use std::cmp::Ordering;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

/// Priority levels for the analysis queue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueuePriority {
    /// Background processing (lowest priority)
    Background = 0,
    /// Normal priority
    Normal = 1,
    /// User-triggered processing
    UserTriggered = 2,
    /// Search-time processing (highest priority)
    SearchTime = 3,
}

impl Default for QueuePriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// An item in the analysis queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    /// Frame ID to analyze
    pub frame_id: i64,
    /// Priority level
    pub priority: QueuePriority,
    /// When the item was queued (unix timestamp)
    pub queued_at: i64,
}

impl QueueItem {
    pub fn new(frame_id: i64, priority: QueuePriority) -> Self {
        Self {
            frame_id,
            priority,
            queued_at: chrono::Utc::now().timestamp(),
        }
    }
}

// Implement ordering for priority queue (higher priority first, then older items first)
impl PartialEq for QueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.frame_id == other.frame_id
    }
}

impl Eq for QueueItem {}

impl PartialOrd for QueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueItem {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first
        match (self.priority as u8).cmp(&(other.priority as u8)) {
            Ordering::Equal => {
                // For same priority, older items first (lower timestamp)
                other.queued_at.cmp(&self.queued_at)
            }
            other => other,
        }
    }
}

/// In-memory analysis queue
///
/// This queue is used for coordinating frame analysis between
/// background workers and on-demand requests.
pub struct AnalysisQueue {
    queue: Arc<Mutex<BinaryHeap<QueueItem>>>,
    max_size: usize,
}

impl AnalysisQueue {
    /// Create a new analysis queue
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: Arc::new(Mutex::new(BinaryHeap::new())),
            max_size,
        }
    }

    /// Add an item to the queue
    pub async fn push(&self, item: QueueItem) -> VisionResult<()> {
        let mut queue = self.queue.lock().await;

        // Check for duplicates
        if queue.iter().any(|i| i.frame_id == item.frame_id) {
            debug!("Frame {} already in queue, skipping", item.frame_id);
            return Ok(());
        }

        // Enforce max size by removing lowest priority items
        while queue.len() >= self.max_size {
            // Remove from the "back" (lowest priority)
            let items: Vec<_> = queue.drain().collect();
            let mut sorted: Vec<_> = items.into_iter().collect();
            sorted.sort();
            sorted.pop(); // Remove lowest priority
            for item in sorted {
                queue.push(item);
            }
        }

        queue.push(item);
        Ok(())
    }

    /// Add multiple items to the queue
    pub async fn push_many(&self, items: Vec<QueueItem>) -> VisionResult<()> {
        for item in items {
            self.push(item).await?;
        }
        Ok(())
    }

    /// Get the next item to process (highest priority)
    pub async fn pop(&self) -> Option<QueueItem> {
        let mut queue = self.queue.lock().await;
        queue.pop()
    }

    /// Peek at the next item without removing it
    pub async fn peek(&self) -> Option<QueueItem> {
        let queue = self.queue.lock().await;
        queue.peek().cloned()
    }

    /// Get the current queue length
    pub async fn len(&self) -> usize {
        let queue = self.queue.lock().await;
        queue.len()
    }

    /// Check if queue is empty
    pub async fn is_empty(&self) -> bool {
        let queue = self.queue.lock().await;
        queue.is_empty()
    }

    /// Remove a specific frame from the queue
    pub async fn remove(&self, frame_id: i64) -> bool {
        let mut queue = self.queue.lock().await;
        let original_len = queue.len();
        let items: Vec<_> = queue.drain().filter(|i| i.frame_id != frame_id).collect();
        for item in items {
            queue.push(item);
        }
        queue.len() < original_len
    }

    /// Clear the entire queue
    pub async fn clear(&self) {
        let mut queue = self.queue.lock().await;
        queue.clear();
    }

    /// Get queue statistics
    pub async fn stats(&self) -> QueueStats {
        let queue = self.queue.lock().await;
        let items: Vec<_> = queue.iter().collect();

        QueueStats {
            total: items.len(),
            background: items.iter().filter(|i| i.priority == QueuePriority::Background).count(),
            normal: items.iter().filter(|i| i.priority == QueuePriority::Normal).count(),
            user_triggered: items.iter().filter(|i| i.priority == QueuePriority::UserTriggered).count(),
            search_time: items.iter().filter(|i| i.priority == QueuePriority::SearchTime).count(),
        }
    }
}

/// Queue statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub total: usize,
    pub background: usize,
    pub normal: usize,
    pub user_triggered: usize,
    pub search_time: usize,
}

impl Default for AnalysisQueue {
    fn default() -> Self {
        Self::new(10000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_queue_priority_ordering() {
        let queue = AnalysisQueue::new(100);

        // Add items with different priorities
        queue.push(QueueItem::new(1, QueuePriority::Background)).await.unwrap();
        queue.push(QueueItem::new(2, QueuePriority::SearchTime)).await.unwrap();
        queue.push(QueueItem::new(3, QueuePriority::Normal)).await.unwrap();

        // Should get highest priority first
        assert_eq!(queue.pop().await.unwrap().frame_id, 2); // SearchTime
        assert_eq!(queue.pop().await.unwrap().frame_id, 3); // Normal
        assert_eq!(queue.pop().await.unwrap().frame_id, 1); // Background
    }

    #[tokio::test]
    async fn test_queue_duplicate_prevention() {
        let queue = AnalysisQueue::new(100);

        queue.push(QueueItem::new(1, QueuePriority::Normal)).await.unwrap();
        queue.push(QueueItem::new(1, QueuePriority::SearchTime)).await.unwrap(); // Duplicate

        assert_eq!(queue.len().await, 1);
    }
}
```

---

## 4. Database Schema Changes

### Migration File: `screensearch-db/migrations/005_add_vision_analysis.sql`

```sql
-- ============================================================================
-- Migration 005: Add Vision Analysis Support
-- ============================================================================
-- This migration adds columns and tables to support AI-powered screenshot
-- analysis using DeepSeek-VL2-Tiny vision model.
-- ============================================================================

-- ----------------------------------------------------------------------------
-- 1. Add analysis columns to frames table
-- ----------------------------------------------------------------------------

-- Analysis status tracks the processing state of each frame
-- Values: 'pending', 'queued', 'analyzing', 'analyzed', 'failed', 'skipped'
ALTER TABLE frames ADD COLUMN analysis_status TEXT DEFAULT 'pending';

-- AI-generated description of what's happening in the screenshot
-- Example: "User is reviewing a Figma design called 'Sunrise Dashboard'"
ALTER TABLE frames ADD COLUMN description TEXT;

-- Classified activity type for filtering and time tracking
-- Values: coding, browsing, reading, design, communication, video, etc.
ALTER TABLE frames ADD COLUMN activity_type TEXT;

-- JSON array of important visible text (not UI chrome)
-- Example: ["Sunrise Dashboard", "Q4 Revenue: $1.2M"]
ALTER TABLE frames ADD COLUMN visible_text_json TEXT;

-- Application hint from AI analysis (may differ from captured app_name)
ALTER TABLE frames ADD COLUMN ai_application_hint TEXT;

-- AI confidence score (0.0 - 1.0)
ALTER TABLE frames ADD COLUMN analysis_confidence REAL;

-- Timestamp when analysis was completed
ALTER TABLE frames ADD COLUMN analyzed_at TIMESTAMP;

-- Processing time in milliseconds (for performance tracking)
ALTER TABLE frames ADD COLUMN analysis_time_ms INTEGER;

-- Error message if analysis failed
ALTER TABLE frames ADD COLUMN analysis_error TEXT;

-- ----------------------------------------------------------------------------
-- 2. Create analysis queue table
-- ----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS analysis_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
    priority INTEGER DEFAULT 0,  -- Higher = process first
    queued_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    attempts INTEGER DEFAULT 0,  -- Number of processing attempts
    last_attempt_at TIMESTAMP,
    UNIQUE(frame_id)
);

-- ----------------------------------------------------------------------------
-- 3. Create indexes for efficient queries
-- ----------------------------------------------------------------------------

-- Index for finding frames by analysis status
CREATE INDEX IF NOT EXISTS idx_frames_analysis_status
    ON frames(analysis_status);

-- Index for finding pending frames ordered by timestamp
CREATE INDEX IF NOT EXISTS idx_frames_pending_analysis
    ON frames(analysis_status, timestamp DESC)
    WHERE analysis_status = 'pending';

-- Index for finding analyzed frames by activity type
CREATE INDEX IF NOT EXISTS idx_frames_activity_type
    ON frames(activity_type)
    WHERE activity_type IS NOT NULL;

-- Index for queue processing order
CREATE INDEX IF NOT EXISTS idx_analysis_queue_priority
    ON analysis_queue(priority DESC, queued_at ASC);

-- Index for queue cleanup
CREATE INDEX IF NOT EXISTS idx_analysis_queue_frame_id
    ON analysis_queue(frame_id);

-- ----------------------------------------------------------------------------
-- 4. Create FTS5 table for description search
-- ----------------------------------------------------------------------------

-- Full-text search on AI descriptions
CREATE VIRTUAL TABLE IF NOT EXISTS frame_description_fts USING fts5(
    description,
    content='frames',
    content_rowid='id'
);

-- Triggers to keep FTS5 in sync with frames table

CREATE TRIGGER IF NOT EXISTS frames_description_ai AFTER INSERT ON frames
WHEN NEW.description IS NOT NULL
BEGIN
    INSERT INTO frame_description_fts(rowid, description)
    VALUES (NEW.id, NEW.description);
END;

CREATE TRIGGER IF NOT EXISTS frames_description_au AFTER UPDATE OF description ON frames
WHEN NEW.description IS NOT NULL
BEGIN
    DELETE FROM frame_description_fts WHERE rowid = OLD.id;
    INSERT INTO frame_description_fts(rowid, description)
    VALUES (NEW.id, NEW.description);
END;

CREATE TRIGGER IF NOT EXISTS frames_description_ad AFTER DELETE ON frames
BEGIN
    DELETE FROM frame_description_fts WHERE rowid = OLD.id;
END;

-- ----------------------------------------------------------------------------
-- 5. Create vision settings table
-- ----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS vision_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Insert default settings
INSERT OR IGNORE INTO vision_settings (key, value) VALUES
    ('enabled', 'false'),
    ('background_processing', 'true'),
    ('batch_size', '5'),
    ('idle_cpu_threshold', '20'),
    ('use_gpu', 'true'),
    ('model_downloaded', 'false');

-- ----------------------------------------------------------------------------
-- 6. Create analysis statistics table for time tracking
-- ----------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS analysis_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,  -- YYYY-MM-DD
    activity_type TEXT NOT NULL,
    frame_count INTEGER DEFAULT 0,
    total_duration_seconds INTEGER DEFAULT 0,  -- Based on capture intervals
    UNIQUE(date, activity_type)
);

-- Index for efficient stats queries
CREATE INDEX IF NOT EXISTS idx_analysis_stats_date
    ON analysis_stats(date DESC);
```

### Update migrations.rs

Add to `screensearch-db/src/migrations.rs`:

```rust
// Add to MIGRATIONS array
const MIGRATION_005_VISION_ANALYSIS: &str = include_str!("../migrations/005_add_vision_analysis.sql");

// Update the migrations list
pub const MIGRATIONS: &[(&str, &str)] = &[
    ("001_initial_schema", MIGRATION_001_INITIAL),
    ("002_add_tags", MIGRATION_002_TAGS),
    ("003_add_indexes", MIGRATION_003_INDEXES),
    ("004_add_embedding_column", MIGRATION_004_EMBEDDINGS),
    ("005_add_vision_analysis", MIGRATION_005_VISION_ANALYSIS),
];
```

---

## 5. Backend API Implementation

### New Handler File: `screensearch-api/src/handlers/vision.rs`

```rust
//! Vision analysis API handlers

use crate::{error::AppError, state::AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use screensearch_vision::{
    download::{download_model, get_download_status, ModelInfo},
    AnalysisQueue, FrameAnalysis, QueueItem, QueuePriority, VisionEngine,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, instrument};

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    /// Specific frame IDs to analyze
    pub frame_ids: Option<Vec<i64>>,
    /// Time range to analyze (alternative to frame_ids)
    pub time_range: Option<TimeRange>,
    /// Priority for processing
    #[serde(default)]
    pub priority: AnalyzePriority,
}

#[derive(Debug, Deserialize)]
pub struct TimeRange {
    pub start: i64,  // Unix timestamp
    pub end: i64,    // Unix timestamp
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalyzePriority {
    Background,
    #[default]
    Normal,
    High,
}

impl From<AnalyzePriority> for QueuePriority {
    fn from(p: AnalyzePriority) -> Self {
        match p {
            AnalyzePriority::Background => QueuePriority::Background,
            AnalyzePriority::Normal => QueuePriority::Normal,
            AnalyzePriority::High => QueuePriority::UserTriggered,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AnalyzeResponse {
    pub queued_count: usize,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct VisionStatusResponse {
    pub model_downloaded: bool,
    pub model_path: String,
    pub model_size_mb: Option<f64>,
    pub engine_ready: bool,
    pub processing_enabled: bool,
    pub queue_length: usize,
    pub stats: AnalysisStats,
}

#[derive(Debug, Serialize)]
pub struct AnalysisStats {
    pub total_frames: i64,
    pub pending_frames: i64,
    pub analyzed_frames: i64,
    pub failed_frames: i64,
    pub coverage_percent: f64,
}

#[derive(Debug, Deserialize)]
pub struct VisionSettingsRequest {
    pub enabled: Option<bool>,
    pub background_processing: Option<bool>,
    pub batch_size: Option<u32>,
    pub idle_cpu_threshold: Option<u32>,
    pub use_gpu: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct VisionSettingsResponse {
    pub enabled: bool,
    pub background_processing: bool,
    pub batch_size: u32,
    pub idle_cpu_threshold: u32,
    pub use_gpu: bool,
}

#[derive(Debug, Serialize)]
pub struct DownloadProgressResponse {
    pub status: DownloadStatus,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub progress_percent: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    NotStarted,
    Downloading,
    Completed,
    Failed,
}

// ============================================================================
// Handlers
// ============================================================================

/// GET /api/vision/status
///
/// Get the current status of the vision analysis system
#[instrument(skip(state))]
pub async fn get_vision_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<VisionStatusResponse>, AppError> {
    let model_info = ModelInfo::default();
    let download_status = get_download_status(&model_info).await;

    // Get analysis statistics from database
    let stats = get_analysis_stats(&state).await?;

    // Check if engine is ready
    let engine_ready = state
        .vision_engine
        .read()
        .await
        .as_ref()
        .map(|e| true) // TODO: e.is_ready()
        .unwrap_or(false);

    // Get queue length
    let queue_length = state.analysis_queue.len().await;

    // Get processing enabled setting
    let processing_enabled = state
        .db
        .get_vision_setting("enabled")
        .await
        .map(|v| v == "true")
        .unwrap_or(false);

    Ok(Json(VisionStatusResponse {
        model_downloaded: download_status.is_downloaded,
        model_path: download_status.path.to_string_lossy().to_string(),
        model_size_mb: download_status.file_size.map(|s| s as f64 / 1_000_000.0),
        engine_ready,
        processing_enabled,
        queue_length,
        stats,
    }))
}

/// POST /api/vision/analyze
///
/// Queue frames for analysis
#[instrument(skip(state))]
pub async fn analyze_frames(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AnalyzeRequest>,
) -> Result<Json<AnalyzeResponse>, AppError> {
    let frame_ids = if let Some(ids) = request.frame_ids {
        ids
    } else if let Some(range) = request.time_range {
        // Get frame IDs in time range
        state
            .db
            .get_pending_frame_ids_in_range(range.start, range.end)
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?
    } else {
        return Err(AppError::InvalidRequest(
            "Either frame_ids or time_range must be provided".to_string(),
        ));
    };

    let priority: QueuePriority = request.priority.into();
    let count = frame_ids.len();

    // Queue frames for processing
    for frame_id in frame_ids {
        let item = QueueItem::new(frame_id, priority);
        state.analysis_queue.push(item).await.map_err(|e| {
            AppError::InternalError(format!("Failed to queue frame: {}", e))
        })?;

        // Update frame status to 'queued'
        state
            .db
            .update_frame_analysis_status(frame_id, "queued")
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    }

    info!("Queued {} frames for analysis with priority {:?}", count, priority);

    Ok(Json(AnalyzeResponse {
        queued_count: count,
        message: format!("Queued {} frames for analysis", count),
    }))
}

/// POST /api/vision/settings
///
/// Update vision processing settings
#[instrument(skip(state))]
pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(request): Json<VisionSettingsRequest>,
) -> Result<Json<VisionSettingsResponse>, AppError> {
    // Update each provided setting
    if let Some(enabled) = request.enabled {
        state
            .db
            .set_vision_setting("enabled", &enabled.to_string())
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    }

    if let Some(background) = request.background_processing {
        state
            .db
            .set_vision_setting("background_processing", &background.to_string())
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    }

    if let Some(batch_size) = request.batch_size {
        state
            .db
            .set_vision_setting("batch_size", &batch_size.to_string())
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    }

    if let Some(idle_threshold) = request.idle_cpu_threshold {
        state
            .db
            .set_vision_setting("idle_cpu_threshold", &idle_threshold.to_string())
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    }

    if let Some(use_gpu) = request.use_gpu {
        state
            .db
            .set_vision_setting("use_gpu", &use_gpu.to_string())
            .await
            .map_err(|e| AppError::DatabaseError(e.to_string()))?;
    }

    // Return current settings
    get_settings(State(state)).await
}

/// GET /api/vision/settings
///
/// Get current vision settings
#[instrument(skip(state))]
pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<VisionSettingsResponse>, AppError> {
    let get_bool = |key: &str, default: bool| async {
        state
            .db
            .get_vision_setting(key)
            .await
            .map(|v| v == "true")
            .unwrap_or(default)
    };

    let get_u32 = |key: &str, default: u32| async {
        state
            .db
            .get_vision_setting(key)
            .await
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };

    Ok(Json(VisionSettingsResponse {
        enabled: get_bool("enabled", false).await,
        background_processing: get_bool("background_processing", true).await,
        batch_size: get_u32("batch_size", 5).await,
        idle_cpu_threshold: get_u32("idle_cpu_threshold", 20).await,
        use_gpu: get_bool("use_gpu", true).await,
    }))
}

/// POST /api/vision/download
///
/// Start model download
#[instrument(skip(state))]
pub async fn start_download(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DownloadProgressResponse>, AppError> {
    let model_info = ModelInfo::default();

    // Check if already downloaded
    let status = get_download_status(&model_info).await;
    if status.is_downloaded {
        return Ok(Json(DownloadProgressResponse {
            status: DownloadStatus::Completed,
            downloaded_bytes: status.file_size,
            total_bytes: status.file_size,
            progress_percent: Some(100.0),
        }));
    }

    // Start download in background
    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        match download_model(&model_info, None).await {
            Ok(path) => {
                info!("Model downloaded to: {:?}", path);
                let _ = state_clone
                    .db
                    .set_vision_setting("model_downloaded", "true")
                    .await;
            }
            Err(e) => {
                tracing::error!("Model download failed: {}", e);
            }
        }
    });

    Ok(Json(DownloadProgressResponse {
        status: DownloadStatus::Downloading,
        downloaded_bytes: Some(0),
        total_bytes: Some(model_info.size_bytes),
        progress_percent: Some(0.0),
    }))
}

/// GET /api/vision/download
///
/// Get download progress
#[instrument(skip(state))]
pub async fn get_download_progress(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DownloadProgressResponse>, AppError> {
    let model_info = ModelInfo::default();
    let status = get_download_status(&model_info).await;

    if status.is_downloaded {
        Ok(Json(DownloadProgressResponse {
            status: DownloadStatus::Completed,
            downloaded_bytes: status.file_size,
            total_bytes: status.file_size,
            progress_percent: Some(100.0),
        }))
    } else if status.path.with_extension("tmp").exists() {
        // Download in progress
        let downloaded = tokio::fs::metadata(status.path.with_extension("tmp"))
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let total = model_info.size_bytes;
        let percent = (downloaded as f64 / total as f64) * 100.0;

        Ok(Json(DownloadProgressResponse {
            status: DownloadStatus::Downloading,
            downloaded_bytes: Some(downloaded),
            total_bytes: Some(total),
            progress_percent: Some(percent),
        }))
    } else {
        Ok(Json(DownloadProgressResponse {
            status: DownloadStatus::NotStarted,
            downloaded_bytes: None,
            total_bytes: Some(model_info.size_bytes),
            progress_percent: None,
        }))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

async fn get_analysis_stats(state: &AppState) -> Result<AnalysisStats, AppError> {
    let total = state
        .db
        .get_total_frame_count()
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    let pending = state
        .db
        .get_frame_count_by_status("pending")
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    let analyzed = state
        .db
        .get_frame_count_by_status("analyzed")
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    let failed = state
        .db
        .get_frame_count_by_status("failed")
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    let coverage = if total > 0 {
        (analyzed as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    Ok(AnalysisStats {
        total_frames: total,
        pending_frames: pending,
        analyzed_frames: analyzed,
        failed_frames: failed,
        coverage_percent: coverage,
    })
}
```

### Add Routes: `screensearch-api/src/routes.rs`

Add to the router:

```rust
// Vision analysis routes
.route("/api/vision/status", get(vision::get_vision_status))
.route("/api/vision/analyze", post(vision::analyze_frames))
.route("/api/vision/settings", get(vision::get_settings).post(vision::update_settings))
.route("/api/vision/download", get(vision::get_download_progress).post(vision::start_download))
```

### New Ask Handler: `screensearch-api/src/handlers/ask.rs`

```rust
//! Natural language "Ask" endpoint for semantic queries

use crate::{
    error::AppError,
    handlers::rag_helpers::build_rag_context,
    state::AppState,
};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::instrument;

#[derive(Debug, Deserialize)]
pub struct AskRequest {
    /// Natural language question
    pub question: String,
    /// Optional time range filter
    pub time_range: Option<TimeRange>,
    /// Maximum frames to include in context
    #[serde(default = "default_max_frames")]
    pub max_frames: usize,
}

fn default_max_frames() -> usize {
    20
}

#[derive(Debug, Deserialize)]
pub struct TimeRange {
    pub start: Option<i64>,
    pub end: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AskResponse {
    /// AI-generated answer
    pub answer: String,
    /// Source frames used to generate the answer
    pub sources: Vec<SourceFrame>,
    /// Processing metadata
    pub metadata: AskMetadata,
}

#[derive(Debug, Serialize)]
pub struct SourceFrame {
    pub frame_id: i64,
    pub timestamp: i64,
    pub description: Option<String>,
    pub relevance_score: f64,
    pub thumbnail_url: String,
}

#[derive(Debug, Serialize)]
pub struct AskMetadata {
    /// Number of frames processed
    pub frames_searched: usize,
    /// Number of frames that needed analysis
    pub frames_analyzed: usize,
    /// Total processing time in ms
    pub processing_time_ms: u64,
}

/// POST /api/ask
///
/// Answer a natural language question using semantic search over screen history
#[instrument(skip(state))]
pub async fn ask_question(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AskRequest>,
) -> Result<Json<AskResponse>, AppError> {
    let start = std::time::Instant::now();

    // Validate question
    let question = request.question.trim();
    if question.is_empty() {
        return Err(AppError::InvalidRequest("Question cannot be empty".to_string()));
    }

    // Build time range filter
    let (start_time, end_time) = if let Some(range) = &request.time_range {
        (range.start, range.end)
    } else {
        (None, None)
    };

    // Get embedding engine
    let embedding_engine = state.get_embedding_engine().await.map_err(|e| {
        AppError::InternalError(format!("Embedding engine not available: {}", e))
    })?;

    // Generate query embedding
    let query_embedding = embedding_engine.embed_single(question).await.map_err(|e| {
        AppError::InternalError(format!("Failed to embed query: {}", e))
    })?;

    // Perform semantic search
    let search_results = state
        .db
        .semantic_search_descriptions(&query_embedding, request.max_frames, start_time, end_time)
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    // Build context for AI
    let context = search_results
        .iter()
        .map(|r| {
            format!(
                "[{}] {}: {}",
                r.timestamp,
                r.application.as_deref().unwrap_or("Unknown"),
                r.description.as_deref().unwrap_or("No description")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // Generate answer using AI provider
    let system_prompt = r#"You are a helpful assistant that answers questions about the user's screen activity history.
You have access to descriptions of screenshots taken over time.
Answer questions based ONLY on the provided context. If the answer isn't in the context, say so.
Be specific and cite timestamps when relevant."#;

    let user_prompt = format!(
        "Based on the following screen activity history:\n\n{}\n\nAnswer this question: {}",
        context, question
    );

    let answer = call_ai_provider(&state, system_prompt, &user_prompt).await?;

    // Build source frames response
    let sources: Vec<SourceFrame> = search_results
        .iter()
        .take(5) // Top 5 sources
        .map(|r| SourceFrame {
            frame_id: r.frame_id,
            timestamp: r.timestamp,
            description: r.description.clone(),
            relevance_score: r.score,
            thumbnail_url: format!("/api/frames/{}/image", r.frame_id),
        })
        .collect();

    let processing_time_ms = start.elapsed().as_millis() as u64;

    Ok(Json(AskResponse {
        answer,
        sources,
        metadata: AskMetadata {
            frames_searched: search_results.len(),
            frames_analyzed: 0, // TODO: Track frames that needed analysis
            processing_time_ms,
        },
    }))
}

async fn call_ai_provider(
    state: &AppState,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, AppError> {
    // Use existing AI provider integration
    // This reuses the infrastructure from the Intelligence feature

    let settings = state
        .db
        .get_ai_settings()
        .await
        .map_err(|e| AppError::DatabaseError(e.to_string()))?;

    if settings.provider_url.is_empty() {
        return Err(AppError::InvalidRequest(
            "AI provider not configured. Please configure in Settings.".to_string(),
        ));
    }

    // Call the AI provider (implementation in ai.rs)
    crate::handlers::ai::call_llm_provider(
        &settings.provider_url,
        &settings.model_name,
        settings.api_key.as_deref(),
        system_prompt,
        user_prompt,
    )
    .await
}
```

---

## 6. Background Processing Worker

### New File: `screensearch-api/src/workers/vision_worker.rs`

```rust
//! Background worker for vision analysis processing

use crate::state::AppState;
use screensearch_vision::{FrameAnalysis, QueuePriority, VisionEngine};
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{System, SystemExt, ProcessorExt};
use tokio::sync::broadcast;
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, instrument, warn};

/// Configuration for the vision worker
#[derive(Debug, Clone)]
pub struct VisionWorkerConfig {
    /// How often to check for work (seconds)
    pub poll_interval_secs: u64,
    /// Number of frames to process per batch
    pub batch_size: usize,
    /// CPU usage threshold below which to process (percentage)
    pub idle_cpu_threshold: f32,
    /// Maximum retries for failed frames
    pub max_retries: u32,
    /// Delay between batches (seconds)
    pub batch_delay_secs: u64,
}

impl Default for VisionWorkerConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 10,
            batch_size: 5,
            idle_cpu_threshold: 20.0,
            max_retries: 3,
            batch_delay_secs: 2,
        }
    }
}

/// Start the vision analysis background worker
#[instrument(skip_all)]
pub async fn start_vision_worker(
    state: Arc<AppState>,
    mut shutdown_rx: broadcast::Receiver<()>,
    config: VisionWorkerConfig,
) {
    info!("Starting vision analysis worker");

    let mut poll_interval = interval(Duration::from_secs(config.poll_interval_secs));
    let mut sys = System::new_all();

    loop {
        tokio::select! {
            _ = poll_interval.tick() => {
                // Check if processing is enabled
                if !is_processing_enabled(&state).await {
                    debug!("Vision processing disabled, skipping");
                    continue;
                }

                // Check if model is ready
                let engine = match state.vision_engine.read().await.as_ref() {
                    Some(e) => Arc::clone(e),
                    None => {
                        debug!("Vision engine not initialized, skipping");
                        continue;
                    }
                };

                // Check CPU usage (only process when idle)
                sys.refresh_cpu();
                let cpu_usage = sys.global_processor_info().cpu_usage();

                if cpu_usage > config.idle_cpu_threshold {
                    debug!("CPU usage {}% > threshold {}%, skipping batch",
                           cpu_usage, config.idle_cpu_threshold);
                    continue;
                }

                // Process a batch of frames
                if let Err(e) = process_batch(&state, &engine, &config).await {
                    error!("Error processing vision batch: {}", e);
                }

                // Brief delay between batches
                sleep(Duration::from_secs(config.batch_delay_secs)).await;
            }
            _ = shutdown_rx.recv() => {
                info!("Vision worker received shutdown signal");
                break;
            }
        }
    }

    info!("Vision worker stopped");
}

/// Process a batch of frames from the queue
async fn process_batch(
    state: &AppState,
    engine: &VisionEngine,
    config: &VisionWorkerConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut processed = 0;

    while processed < config.batch_size {
        // Get next item from queue
        let item = match state.analysis_queue.pop().await {
            Some(item) => item,
            None => {
                // Queue empty, try to queue pending frames
                queue_pending_frames(state, config.batch_size).await?;
                match state.analysis_queue.pop().await {
                    Some(item) => item,
                    None => break, // No work available
                }
            }
        };

        // Process the frame
        match process_frame(state, engine, item.frame_id).await {
            Ok(analysis) => {
                info!(
                    "Analyzed frame {}: {} ({})",
                    item.frame_id,
                    analysis.activity_type.as_str(),
                    analysis.description.chars().take(50).collect::<String>()
                );
                processed += 1;
            }
            Err(e) => {
                warn!("Failed to analyze frame {}: {}", item.frame_id, e);

                // Mark as failed or requeue based on retry count
                let attempts = state
                    .db
                    .get_frame_analysis_attempts(item.frame_id)
                    .await
                    .unwrap_or(0);

                if attempts < config.max_retries {
                    // Requeue with lower priority
                    state
                        .db
                        .update_frame_analysis_status(item.frame_id, "pending")
                        .await?;
                    state
                        .db
                        .increment_analysis_attempts(item.frame_id)
                        .await?;
                } else {
                    // Mark as permanently failed
                    state
                        .db
                        .update_frame_analysis_status(item.frame_id, "failed")
                        .await?;
                    state
                        .db
                        .set_frame_analysis_error(item.frame_id, &e.to_string())
                        .await?;
                }
            }
        }
    }

    if processed > 0 {
        debug!("Processed {} frames in batch", processed);
    }

    Ok(())
}

/// Process a single frame
async fn process_frame(
    state: &AppState,
    engine: &VisionEngine,
    frame_id: i64,
) -> Result<FrameAnalysis, Box<dyn std::error::Error + Send + Sync>> {
    // Update status to analyzing
    state
        .db
        .update_frame_analysis_status(frame_id, "analyzing")
        .await?;

    // Get frame image data
    let frame = state.db.get_frame_by_id(frame_id).await?;
    let image_path = &frame.file_path;
    let image_data = tokio::fs::read(image_path).await?;

    // Run AI analysis
    let analysis = engine.analyze(&image_data).await?;

    // Store results
    state
        .db
        .update_frame_analysis(
            frame_id,
            &analysis.description,
            analysis.activity_type.as_str(),
            &serde_json::to_string(&analysis.visible_text)?,
            analysis.application_hint.as_deref(),
            analysis.confidence,
            analysis.processing_time_ms as i64,
        )
        .await?;

    // Update status to analyzed
    state
        .db
        .update_frame_analysis_status(frame_id, "analyzed")
        .await?;

    // Trigger embedding update for this frame
    // The embedding worker will pick this up on its next cycle
    state
        .db
        .mark_frame_for_embedding(frame_id)
        .await?;

    Ok(analysis)
}

/// Queue pending frames for analysis
async fn queue_pending_frames(
    state: &AppState,
    limit: usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pending_ids = state
        .db
        .get_oldest_pending_frame_ids(limit)
        .await?;

    for frame_id in pending_ids {
        let item = screensearch_vision::QueueItem::new(frame_id, QueuePriority::Background);
        state.analysis_queue.push(item).await?;
        state
            .db
            .update_frame_analysis_status(frame_id, "queued")
            .await?;
    }

    Ok(())
}

/// Check if vision processing is enabled in settings
async fn is_processing_enabled(state: &AppState) -> bool {
    state
        .db
        .get_vision_setting("enabled")
        .await
        .map(|v| v == "true")
        .unwrap_or(false)
        && state
            .db
            .get_vision_setting("background_processing")
            .await
            .map(|v| v == "true")
            .unwrap_or(true)
}
```

---

## 7. Embedding Pipeline Modifications

### Update: `screensearch-api/src/workers/embedding_worker.rs`

Key changes to make the embedding worker use AI descriptions instead of raw OCR:

```rust
// In the process_frame function, change:

// OLD: Get raw OCR text
// let ocr_text = state.db.get_ocr_text_for_frame(frame_id).await?;

// NEW: Get AI description (falls back to OCR if not analyzed)
let text_for_embedding = get_embedding_text(state, frame_id).await?;

// ...

async fn get_embedding_text(
    state: &AppState,
    frame_id: i64,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let frame = state.db.get_frame_by_id(frame_id).await?;

    // Prefer AI description if available
    if let Some(description) = &frame.description {
        let mut text = description.clone();

        // Append visible text if available
        if let Some(visible_json) = &frame.visible_text_json {
            if let Ok(visible_text) = serde_json::from_str::<Vec<String>>(visible_json) {
                text.push_str("\n");
                text.push_str(&visible_text.join(" "));
            }
        }

        // Append activity type
        if let Some(activity) = &frame.activity_type {
            text.push_str(&format!("\nActivity: {}", activity));
        }

        // Append application
        if let Some(app) = &frame.ai_application_hint {
            text.push_str(&format!("\nApplication: {}", app));
        }

        return Ok(text);
    }

    // Fallback to raw OCR text
    state.db.get_ocr_text_for_frame(frame_id).await
}
```

---

## 8. Frontend UI Implementation

### New Component: `screensearch-ui/src/components/AskBar.tsx`

```tsx
import React, { useState, useRef, useEffect } from 'react';
import { Search, Loader2, X, Clock, Image } from 'lucide-react';
import { useQuery, useMutation } from '@tanstack/react-query';
import axios from 'axios';

interface AskResponse {
  answer: string;
  sources: {
    frame_id: number;
    timestamp: number;
    description: string | null;
    relevance_score: number;
    thumbnail_url: string;
  }[];
  metadata: {
    frames_searched: number;
    processing_time_ms: number;
  };
}

export const AskBar: React.FC = () => {
  const [query, setQuery] = useState('');
  const [isExpanded, setIsExpanded] = useState(false);
  const [showResults, setShowResults] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const resultsRef = useRef<HTMLDivElement>(null);

  const askMutation = useMutation({
    mutationFn: async (question: string) => {
      const { data } = await axios.post<AskResponse>(
        'http://localhost:3131/api/ask',
        { question, max_frames: 20 }
      );
      return data;
    },
    onSuccess: () => {
      setShowResults(true);
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (query.trim()) {
      askMutation.mutate(query.trim());
    }
  };

  const handleClear = () => {
    setQuery('');
    setShowResults(false);
    askMutation.reset();
    inputRef.current?.focus();
  };

  // Close results when clicking outside
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (resultsRef.current && !resultsRef.current.contains(e.target as Node)) {
        setShowResults(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  // Keyboard shortcut: Cmd/Ctrl + K to focus
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        setIsExpanded(true);
        inputRef.current?.focus();
      }
      if (e.key === 'Escape') {
        setIsExpanded(false);
        setShowResults(false);
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, []);

  return (
    <div className="relative" ref={resultsRef}>
      {/* Search Input */}
      <form onSubmit={handleSubmit} className="relative">
        <div
          className={`
            flex items-center bg-slate-800 border border-slate-700 rounded-lg
            transition-all duration-200 ease-in-out
            ${isExpanded ? 'w-96' : 'w-64'}
            focus-within:border-blue-500 focus-within:ring-1 focus-within:ring-blue-500
          `}
        >
          <Search className="w-4 h-4 text-slate-400 ml-3" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onFocus={() => setIsExpanded(true)}
            placeholder="Ask anything... (⌘K)"
            className="
              flex-1 bg-transparent border-none outline-none
              text-sm text-slate-200 placeholder-slate-500
              px-3 py-2
            "
          />
          {askMutation.isPending && (
            <Loader2 className="w-4 h-4 text-blue-400 animate-spin mr-2" />
          )}
          {query && !askMutation.isPending && (
            <button
              type="button"
              onClick={handleClear}
              className="p-1 mr-2 text-slate-400 hover:text-slate-200"
            >
              <X className="w-4 h-4" />
            </button>
          )}
        </div>
      </form>

      {/* Results Dropdown */}
      {showResults && askMutation.data && (
        <div className="
          absolute top-full left-0 right-0 mt-2
          bg-slate-800 border border-slate-700 rounded-lg shadow-xl
          max-h-[70vh] overflow-y-auto z-50
        ">
          {/* Answer */}
          <div className="p-4 border-b border-slate-700">
            <h4 className="text-xs font-medium text-slate-400 uppercase mb-2">
              Answer
            </h4>
            <p className="text-sm text-slate-200 whitespace-pre-wrap">
              {askMutation.data.answer}
            </p>
          </div>

          {/* Sources */}
          {askMutation.data.sources.length > 0 && (
            <div className="p-4">
              <h4 className="text-xs font-medium text-slate-400 uppercase mb-3">
                Sources ({askMutation.data.sources.length})
              </h4>
              <div className="grid grid-cols-2 gap-2">
                {askMutation.data.sources.map((source) => (
                  <a
                    key={source.frame_id}
                    href={`/frames/${source.frame_id}`}
                    className="
                      flex items-start gap-2 p-2 rounded
                      bg-slate-700/50 hover:bg-slate-700
                      transition-colors
                    "
                  >
                    <img
                      src={`http://localhost:3131${source.thumbnail_url}`}
                      alt=""
                      className="w-16 h-12 object-cover rounded"
                    />
                    <div className="flex-1 min-w-0">
                      <p className="text-xs text-slate-300 truncate">
                        {source.description || 'No description'}
                      </p>
                      <p className="text-xs text-slate-500 flex items-center gap-1 mt-1">
                        <Clock className="w-3 h-3" />
                        {new Date(source.timestamp * 1000).toLocaleString()}
                      </p>
                    </div>
                  </a>
                ))}
              </div>
            </div>
          )}

          {/* Metadata */}
          <div className="px-4 py-2 bg-slate-900/50 text-xs text-slate-500">
            Searched {askMutation.data.metadata.frames_searched} frames in{' '}
            {askMutation.data.metadata.processing_time_ms}ms
          </div>
        </div>
      )}

      {/* Error State */}
      {askMutation.isError && (
        <div className="
          absolute top-full left-0 right-0 mt-2
          bg-red-900/20 border border-red-800 rounded-lg p-4
        ">
          <p className="text-sm text-red-400">
            {(askMutation.error as Error).message || 'Failed to get answer'}
          </p>
        </div>
      )}
    </div>
  );
};
```

### New Component: `screensearch-ui/src/components/ProcessingStatus.tsx`

```tsx
import React from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Brain, Download, Play, Settings, AlertCircle } from 'lucide-react';
import axios from 'axios';

interface VisionStatus {
  model_downloaded: boolean;
  model_path: string;
  model_size_mb: number | null;
  engine_ready: boolean;
  processing_enabled: boolean;
  queue_length: number;
  stats: {
    total_frames: number;
    pending_frames: number;
    analyzed_frames: number;
    failed_frames: number;
    coverage_percent: number;
  };
}

interface DownloadProgress {
  status: 'not_started' | 'downloading' | 'completed' | 'failed';
  downloaded_bytes: number | null;
  total_bytes: number | null;
  progress_percent: number | null;
}

export const ProcessingStatus: React.FC = () => {
  const queryClient = useQueryClient();

  const { data: status, isLoading } = useQuery({
    queryKey: ['vision-status'],
    queryFn: async () => {
      const { data } = await axios.get<VisionStatus>(
        'http://localhost:3131/api/vision/status'
      );
      return data;
    },
    refetchInterval: 5000, // Refresh every 5s
  });

  const { data: downloadProgress } = useQuery({
    queryKey: ['vision-download'],
    queryFn: async () => {
      const { data } = await axios.get<DownloadProgress>(
        'http://localhost:3131/api/vision/download'
      );
      return data;
    },
    refetchInterval: (data) =>
      data?.status === 'downloading' ? 1000 : false,
  });

  const downloadMutation = useMutation({
    mutationFn: async () => {
      await axios.post('http://localhost:3131/api/vision/download');
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['vision-download'] });
    },
  });

  const toggleMutation = useMutation({
    mutationFn: async (enabled: boolean) => {
      await axios.post('http://localhost:3131/api/vision/settings', {
        enabled,
      });
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['vision-status'] });
    },
  });

  if (isLoading) {
    return (
      <div className="p-4 bg-slate-800 rounded-lg animate-pulse">
        <div className="h-4 bg-slate-700 rounded w-32"></div>
      </div>
    );
  }

  if (!status) return null;

  const coveragePercent = status.stats.coverage_percent.toFixed(1);

  return (
    <div className="p-4 bg-slate-800 rounded-lg space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Brain className="w-5 h-5 text-blue-400" />
          <span className="font-medium text-slate-200">AI Vision Analysis</span>
        </div>
        <button
          onClick={() => toggleMutation.mutate(!status.processing_enabled)}
          className={`
            px-3 py-1 rounded text-sm font-medium transition-colors
            ${status.processing_enabled
              ? 'bg-green-600 hover:bg-green-700 text-white'
              : 'bg-slate-700 hover:bg-slate-600 text-slate-300'
            }
          `}
        >
          {status.processing_enabled ? 'Enabled' : 'Disabled'}
        </button>
      </div>

      {/* Model Status */}
      {!status.model_downloaded ? (
        <div className="space-y-2">
          <p className="text-sm text-slate-400">
            Model not downloaded. Download DeepSeek-VL2-Tiny (~2GB) to enable AI analysis.
          </p>
          {downloadProgress?.status === 'downloading' ? (
            <div className="space-y-1">
              <div className="h-2 bg-slate-700 rounded-full overflow-hidden">
                <div
                  className="h-full bg-blue-500 transition-all duration-300"
                  style={{ width: `${downloadProgress.progress_percent || 0}%` }}
                />
              </div>
              <p className="text-xs text-slate-500">
                {((downloadProgress.downloaded_bytes || 0) / 1e9).toFixed(2)} GB /{' '}
                {((downloadProgress.total_bytes || 0) / 1e9).toFixed(2)} GB
              </p>
            </div>
          ) : (
            <button
              onClick={() => downloadMutation.mutate()}
              disabled={downloadMutation.isPending}
              className="
                flex items-center gap-2 px-4 py-2
                bg-blue-600 hover:bg-blue-700 disabled:opacity-50
                text-white text-sm font-medium rounded
                transition-colors
              "
            >
              <Download className="w-4 h-4" />
              Download Model
            </button>
          )}
        </div>
      ) : (
        <>
          {/* Coverage Stats */}
          <div className="space-y-2">
            <div className="flex justify-between text-sm">
              <span className="text-slate-400">Analysis Coverage</span>
              <span className="text-slate-200">{coveragePercent}%</span>
            </div>
            <div className="h-2 bg-slate-700 rounded-full overflow-hidden">
              <div
                className="h-full bg-green-500 transition-all duration-300"
                style={{ width: `${status.stats.coverage_percent}%` }}
              />
            </div>
            <div className="flex justify-between text-xs text-slate-500">
              <span>{status.stats.analyzed_frames.toLocaleString()} analyzed</span>
              <span>{status.stats.pending_frames.toLocaleString()} pending</span>
            </div>
          </div>

          {/* Queue Status */}
          {status.queue_length > 0 && (
            <div className="flex items-center gap-2 text-sm text-slate-400">
              <Play className="w-4 h-4 text-yellow-400" />
              Processing... {status.queue_length} in queue
            </div>
          )}

          {/* Failed Frames Warning */}
          {status.stats.failed_frames > 0 && (
            <div className="flex items-center gap-2 text-sm text-amber-400">
              <AlertCircle className="w-4 h-4" />
              {status.stats.failed_frames} frames failed analysis
            </div>
          )}
        </>
      )}
    </div>
  );
};
```

### Update Layout: `screensearch-ui/src/components/Layout.tsx`

Add AskBar to the header:

```tsx
import { AskBar } from './AskBar';

// In the header section:
<header className="bg-slate-900 border-b border-slate-800 px-6 py-3">
  <div className="flex items-center justify-between">
    <div className="flex items-center gap-4">
      <Logo />
      <nav>{/* existing nav */}</nav>
    </div>

    {/* Add AskBar */}
    <AskBar />

    <div className="flex items-center gap-2">
      {/* existing header items */}
    </div>
  </div>
</header>
```

### Update Settings Page: `screensearch-ui/src/pages/SettingsPage.tsx`

Add a new section for Vision settings:

```tsx
import { ProcessingStatus } from '../components/ProcessingStatus';

// Add to the settings sections:
<section className="space-y-4">
  <h2 className="text-lg font-semibold text-slate-200">
    AI Vision Analysis
  </h2>
  <p className="text-sm text-slate-400">
    Replace Windows OCR with AI-powered screenshot analysis for better
    semantic search and natural language queries.
  </p>
  <ProcessingStatus />

  {/* Additional settings */}
  <div className="space-y-4 pt-4 border-t border-slate-700">
    <SettingToggle
      label="Background Processing"
      description="Automatically analyze screenshots when system is idle"
      setting="vision.background_processing"
    />
    <SettingSelect
      label="Inference Device"
      description="Use GPU for faster processing if available"
      setting="vision.use_gpu"
      options={[
        { value: 'true', label: 'GPU (Recommended)' },
        { value: 'false', label: 'CPU Only' },
      ]}
    />
    <SettingNumber
      label="Batch Size"
      description="Number of frames to process per batch"
      setting="vision.batch_size"
      min={1}
      max={20}
    />
    <SettingNumber
      label="Idle CPU Threshold"
      description="Only process when CPU usage is below this percentage"
      setting="vision.idle_cpu_threshold"
      min={5}
      max={50}
      suffix="%"
    />
  </div>
</section>
```

---

## 9. Configuration System

### Update config.toml

Add vision section:

```toml
[vision]
# Enable AI vision analysis (requires model download)
enabled = false

# Process frames automatically when system is idle
background_processing = true

# Number of frames to process per batch
batch_size = 5

# CPU usage threshold (%) - only process when below this
idle_cpu_threshold = 20

# Use GPU for inference if available
use_gpu = true

# Model configuration
model_name = "deepseek-vl2-tiny"

# Maximum inference time per frame (seconds)
inference_timeout_secs = 30
```

### Update AppConfig in src/main.rs

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct VisionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_background_processing")]
    pub background_processing: bool,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_idle_threshold")]
    pub idle_cpu_threshold: f32,
    #[serde(default = "default_use_gpu")]
    pub use_gpu: bool,
    #[serde(default = "default_model_name")]
    pub model_name: String,
    #[serde(default = "default_inference_timeout")]
    pub inference_timeout_secs: u64,
}

fn default_background_processing() -> bool { true }
fn default_batch_size() -> usize { 5 }
fn default_idle_threshold() -> f32 { 20.0 }
fn default_use_gpu() -> bool { true }
fn default_model_name() -> String { "deepseek-vl2-tiny".to_string() }
fn default_inference_timeout() -> u64 { 30 }

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            background_processing: default_background_processing(),
            batch_size: default_batch_size(),
            idle_cpu_threshold: default_idle_threshold(),
            use_gpu: default_use_gpu(),
            model_name: default_model_name(),
            inference_timeout_secs: default_inference_timeout(),
        }
    }
}
```

---

## 10. Main Binary Integration

### Update src/main.rs

Add vision worker initialization:

```rust
use screensearch_vision::{AnalysisQueue, VisionEngine, download::is_model_downloaded, ModelInfo};

// In AppState or shared state:
pub struct AppState {
    // ... existing fields ...
    pub vision_engine: Arc<RwLock<Option<Arc<VisionEngine>>>>,
    pub analysis_queue: Arc<AnalysisQueue>,
}

// In main() or initialization:
async fn initialize_services(config: &AppConfig) -> Result<AppState> {
    // ... existing initialization ...

    // Initialize analysis queue
    let analysis_queue = Arc::new(AnalysisQueue::new(10000));

    // Initialize vision engine (if model downloaded)
    let vision_engine = Arc::new(RwLock::new(None));
    let model_info = ModelInfo::default();

    if is_model_downloaded(&model_info).await {
        match VisionEngine::with_config(VisionEngineConfig {
            use_gpu: config.vision.use_gpu,
            n_threads: 4,
            ..Default::default()
        }).await {
            Ok(engine) => {
                *vision_engine.write().await = Some(Arc::new(engine));
                info!("Vision engine initialized successfully");
            }
            Err(e) => {
                warn!("Failed to initialize vision engine: {}", e);
            }
        }
    } else {
        info!("Vision model not downloaded, engine disabled");
    }

    // ... return state ...
}

// Spawn vision worker:
if config.vision.enabled {
    let vision_state = Arc::clone(&state);
    let vision_shutdown = shutdown_tx.subscribe();
    let vision_config = VisionWorkerConfig {
        poll_interval_secs: 10,
        batch_size: config.vision.batch_size,
        idle_cpu_threshold: config.vision.idle_cpu_threshold,
        ..Default::default()
    };

    tokio::spawn(async move {
        workers::vision_worker::start_vision_worker(
            vision_state,
            vision_shutdown,
            vision_config,
        ).await;
    });

    info!("Vision worker started");
}
```

---

## 11. Model Management

### Model Download Flow

```
1. User clicks "Download Model" in Settings
   ↓
2. POST /api/vision/download
   ↓
3. Backend spawns download task
   ↓
4. Download from HuggingFace with progress
   ↓
5. Save to: %LOCALAPPDATA%/screensearch/models/deepseek-vl2-tiny.gguf
   ↓
6. Update vision_settings.model_downloaded = true
   ↓
7. Initialize VisionEngine with the model
   ↓
8. Vision worker can now process frames
```

### Model Storage Location

```
Windows: C:\Users\{user}\AppData\Local\screensearch\models\
Linux:   ~/.local/share/screensearch/models/
macOS:   ~/Library/Application Support/screensearch/models/
```

### Model Verification

Before loading:
1. Check file exists
2. Verify file size > 1GB (sanity check)
3. Optionally verify SHA256 hash

---

## 12. Error Handling Strategy

### Error Categories

| Category | Handling | User Feedback |
|----------|----------|---------------|
| Model not downloaded | Block processing, prompt download | "Download model to enable" |
| Model load failure | Log error, disable engine | "Vision unavailable" |
| Inference timeout | Mark frame as failed, retry later | "Analysis taking too long" |
| Image decode error | Skip frame, mark as failed | None (silent) |
| GPU out of memory | Fall back to CPU | "Using CPU (slower)" |
| Network error (download) | Retry with backoff | "Download failed, retrying..." |

### Retry Strategy

```rust
const MAX_RETRIES: u32 = 3;
const RETRY_DELAYS: [u64; 3] = [60, 300, 900]; // 1min, 5min, 15min

// On frame analysis failure:
if attempts < MAX_RETRIES {
    // Requeue with delay
    schedule_retry(frame_id, RETRY_DELAYS[attempts as usize]);
} else {
    // Mark as permanently failed
    mark_failed(frame_id, &error_message);
}
```

---

## 13. Performance Considerations

### Resource Usage Targets

| Metric | Target | Notes |
|--------|--------|-------|
| VRAM | 2-3 GB | DeepSeek-VL2-Tiny requirement |
| RAM | +500 MB | Model + inference buffers |
| CPU (inference) | 1 core | During active processing |
| Inference time | 1-3s/frame | GPU; 5-10s on CPU |
| Queue memory | ~40 KB | 10,000 items max |

### Optimization Strategies

1. **Batch processing**: Process multiple frames in sequence, keeping model loaded
2. **Image resizing**: Resize to 1024px max before inference
3. **Idle detection**: Only process when CPU < 20%
4. **Priority queue**: Search-triggered analysis takes precedence
5. **Lazy loading**: Don't load model until first use

### Memory Management

```rust
// Unload model when not in use for 30 minutes
let idle_timeout = Duration::from_secs(30 * 60);
if last_inference.elapsed() > idle_timeout {
    vision_engine.write().await.take(); // Drop the engine
}
```

---

## 14. Testing Strategy

### Unit Tests

```rust
// screensearch-vision/tests/

#[test]
fn test_prompt_parsing() {
    let response = r#"{"description": "Test", ...}"#;
    let analysis = PromptTemplate::parse_response(response, 100).unwrap();
    assert!(!analysis.description.is_empty());
}

#[test]
fn test_activity_type_classification() {
    assert_eq!(ActivityType::from_str("coding"), ActivityType::Coding);
}

#[test]
fn test_queue_priority_ordering() {
    // High priority items should come first
}

#[test]
fn test_embedding_text_generation() {
    let analysis = FrameAnalysis { ... };
    let text = analysis.to_embedding_text();
    assert!(text.contains(&analysis.description));
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_vision_api_endpoints() {
    // Start test server
    // POST /api/vision/analyze
    // GET /api/vision/status
    // Verify responses
}

#[tokio::test]
async fn test_ask_endpoint() {
    // Setup with some analyzed frames
    // POST /api/ask with a question
    // Verify answer and sources
}

#[tokio::test]
async fn test_embedding_uses_description() {
    // Create frame with description
    // Run embedding worker
    // Verify embedding text source
}
```

### Manual Testing Checklist

- [ ] Model download works from scratch
- [ ] Model download resumes after interruption
- [ ] Vision engine loads successfully
- [ ] Background processing respects idle threshold
- [ ] On-demand processing works via API
- [ ] Ask bar returns relevant results
- [ ] Processing status shows accurate counts
- [ ] Settings changes take effect immediately
- [ ] GPU inference works (if available)
- [ ] CPU fallback works
- [ ] Error states display correctly in UI

---

## 15. Migration Path

### For Existing Users

1. **Preserve existing data**: Raw OCR text remains in `ocr_text` table
2. **Gradual migration**: New frames get AI analysis; old frames can be reprocessed
3. **Fallback search**: If description is empty, search falls back to OCR text
4. **No forced re-indexing**: Embeddings regenerate naturally over time

### Migration Steps

```sql
-- Mark all existing frames as pending analysis
UPDATE frames SET analysis_status = 'pending' WHERE analysis_status IS NULL;

-- Optionally prioritize recent frames
UPDATE frames SET analysis_status = 'priority'
WHERE timestamp > strftime('%s', 'now') - 86400 * 7; -- Last 7 days
```

### Rollback Plan

If issues arise:
1. Disable vision processing in settings
2. Set `vision.enabled = false` in config.toml
3. Embedding worker falls back to OCR text
4. No data loss - original OCR preserved

---

## 16. File Change Summary

### New Files

| File | Purpose |
|------|---------|
| `screensearch-vision/Cargo.toml` | New crate manifest |
| `screensearch-vision/src/lib.rs` | Public API |
| `screensearch-vision/src/engine.rs` | VisionEngine implementation |
| `screensearch-vision/src/download.rs` | Model download logic |
| `screensearch-vision/src/prompt.rs` | Prompt templates |
| `screensearch-vision/src/queue.rs` | Analysis queue |
| `screensearch-vision/src/analysis.rs` | FrameAnalysis types |
| `screensearch-vision/src/error.rs` | Error types |
| `screensearch-db/migrations/005_add_vision_analysis.sql` | Schema migration |
| `screensearch-api/src/handlers/vision.rs` | Vision API handlers |
| `screensearch-api/src/handlers/ask.rs` | Ask endpoint handler |
| `screensearch-api/src/workers/vision_worker.rs` | Background worker |
| `screensearch-ui/src/components/AskBar.tsx` | Search bar component |
| `screensearch-ui/src/components/ProcessingStatus.tsx` | Status widget |

### Modified Files

| File | Changes |
|------|---------|
| `Cargo.toml` (workspace) | Add screensearch-vision member |
| `screensearch-db/src/migrations.rs` | Add migration 005 |
| `screensearch-db/src/queries.rs` | Add vision-related queries |
| `screensearch-api/src/routes.rs` | Add vision routes |
| `screensearch-api/src/state.rs` | Add vision engine to state |
| `screensearch-api/src/workers/embedding_worker.rs` | Use descriptions for embeddings |
| `screensearch-ui/src/components/Layout.tsx` | Add AskBar |
| `screensearch-ui/src/pages/SettingsPage.tsx` | Add vision settings |
| `src/main.rs` | Initialize vision engine and worker |
| `config.toml` | Add vision section |

---

## Appendix: Example Queries and Expected Results

### Query: "What was the name of that yellow design I saw Tuesday?"

**Process:**
1. Embed query → semantic search on descriptions
2. Filter by timestamp (Tuesday)
3. Match descriptions containing "yellow", "design", color references
4. Return: "Based on your screen history, you viewed 'Sunrise Dashboard' in Figma on Tuesday at 11:23 AM. It featured an orange/yellow color scheme with charts."

### Query: "How long did I spend on ScreenSearch project this week?"

**Process:**
1. Search descriptions for "ScreenSearch", code editors, relevant windows
2. Aggregate by activity_type = 'coding' and application hints
3. Calculate time from frame intervals
4. Return: "You spent approximately 4.5 hours on the ScreenSearch project this week, primarily in VS Code (3h) and the browser viewing documentation (1.5h)."

### Query: "Summarize what I read on Reddit today"

**Process:**
1. Search descriptions containing "Reddit", activity_type = 'browsing'
2. Filter by today's date
3. Aggregate visible_text from matching frames
4. Send to AI for summarization
5. Return: "Today on Reddit you read: a discussion about Rust async patterns in r/rust, several posts about AI coding assistants in r/programming, and a thread about home automation in r/homelab."
