# Embedded LLM System (screensearch-llm)

**Version**: 0.3.0
**Last Updated**: 2025-12-28
**Status**: Production Ready

This document provides comprehensive documentation for the embedded LLM system in ScreenSearch, including the Ministral-3B model, llama-server management, and integration with the AI intelligence features.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture](#2-architecture)
3. [Configuration](#3-configuration)
4. [Model Management](#4-model-management)
5. [Server Lifecycle](#5-server-lifecycle)
6. [Inference Engine](#6-inference-engine)
7. [API Endpoints](#7-api-endpoints)
8. [Frontend Integration](#8-frontend-integration)
9. [GPU Acceleration](#9-gpu-acceleration)
10. [Troubleshooting](#10-troubleshooting)
11. [Future Enhancements](#11-future-enhancements)

---

## 1. Overview

ScreenSearch includes a fully embedded LLM system that runs entirely on the user's machine. This enables:

- **Privacy**: No data sent to external APIs
- **Offline Operation**: Works without internet after initial model download
- **GPU Acceleration**: Vulkan-based acceleration for NVIDIA, AMD, and Intel GPUs
- **Zero Configuration**: Works out of the box with sensible defaults

### 1.1 Technology Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| **Model** | Ministral-3B-Instruct | 3B parameter instruction-following model |
| **Format** | GGUF (Q4_K_M) | Quantized format for efficient inference |
| **Server** | llama.cpp (llama-server) | High-performance inference server |
| **GPU Backend** | Vulkan | Cross-platform GPU acceleration |
| **API Protocol** | OpenAI-compatible | Standard `/v1/chat/completions` endpoint |

### 1.2 Key Features

- **Lazy Loading**: Server starts only when first AI request is made
- **Auto-Shutdown**: Stops after configurable idle timeout (default: 5 minutes)
- **Crash Recovery**: Auto-restarts up to 3 times if process crashes
- **Health Monitoring**: Background task checks server health every 30 seconds
- **Port Fallback**: Automatically finds available port (31130, 31131, 31132)

---

## 2. Architecture

### 2.1 Component Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                         screensearch-llm                            │
│                                                                     │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐  │
│  │   LlmConfig      │  │   LlamaServer    │  │   LlmEngine      │  │
│  │                  │  │                  │  │                  │  │
│  │ - temperature    │  │ - process mgmt   │  │ - HTTP client    │  │
│  │ - max_tokens     │  │ - health check   │  │ - chat/complete  │  │
│  │ - context_length │  │ - idle TTL       │  │ - vision support │  │
│  │ - gpu settings   │  │ - crash recovery │  │ - profiles       │  │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘  │
│                                                                     │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐  │
│  │   Download       │  │   Profiles       │  │   Error Types    │  │
│  │                  │  │                  │  │                  │  │
│  │ - model GGUF     │  │ - VisionAnalysis │  │ - ModelNotFound  │  │
│  │ - llama-server   │  │ - RagAnswer      │  │ - DownloadFailed │  │
│  │ - progress track │  │ - Custom         │  │ - GenerationError│  │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
                                   │
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         llama-server process                        │
│                                                                     │
│  - Runs on 127.0.0.1:31130 (local only)                            │
│  - Loads Ministral-3B-Instruct-2512-Q4_K_M.gguf                    │
│  - GPU acceleration via Vulkan (Windows)                           │
│  - OpenAI-compatible /v1/chat/completions endpoint                  │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Integration with screensearch-api

```
┌─────────────────────────────────────────────────────────────────────┐
│                         screensearch-api                            │
│                                                                     │
│  AppState                                                           │
│  └─> llama_server: Arc<RwLock<Option<Arc<LlamaServer>>>>           │
│                                                                     │
│  Handlers (ai.rs)                                                   │
│  ├─> POST /api/ai/validate       → Check local model + server      │
│  ├─> POST /api/ai/generate       → Generate report using local LLM │
│  ├─> GET  /api/ai/model/status   → Check model download status     │
│  ├─> POST /api/ai/model/download → Start model download            │
│  ├─> GET  /api/ai/server/status  → Check llama-server status       │
│  ├─> POST /api/ai/server/start   → Start llama-server              │
│  ├─> POST /api/ai/server/stop    → Stop llama-server               │
│  ├─> POST /api/ai/server/ttl     → Update idle timeout             │
│  └─> POST /api/ai/server/download→ Download llama-server binary    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 3. Configuration

### 3.1 LlmConfig Structure

```rust
pub struct LlmConfig {
    pub model_path: Option<PathBuf>,    // Auto-detected if None
    pub temperature: f32,                // Default: 0.7 (range: 0.0-2.0)
    pub max_tokens: usize,               // Default: 2048
    pub context_length: usize,           // Default: 8192 (up to 256k supported)
    pub threads: usize,                  // Default: 0 (auto-detect CPU cores)
    pub use_gpu: bool,                   // Default: true (Vulkan acceleration)
    pub top_p: f32,                      // Default: 0.95 (nucleus sampling)
    pub top_k: usize,                    // Default: 40
    pub repetition_penalty: f32,         // Default: 1.1
    pub idle_ttl: Duration,              // Default: 300s (5 minutes)
    pub server_port: u16,                // Default: 31130
}
```

### 3.2 Configuration Constants

```rust
pub const DEFAULT_TEMPERATURE: f32 = 0.7;
pub const DEFAULT_MAX_TOKENS: usize = 2048;
pub const DEFAULT_CONTEXT_LENGTH: usize = 8192;
pub const DEFAULT_THREADS: usize = 0;           // Auto-detect
pub const DEFAULT_IDLE_TTL_SECS: u64 = 300;     // 5 minutes
pub const DEFAULT_LLAMA_PORT: u16 = 31130;
```

### 3.3 Builder Pattern

```rust
let config = LlmConfig::new()
    .with_temperature(0.5)
    .with_max_tokens(1024)
    .with_gpu(true)
    .with_idle_ttl_secs(600)
    .with_server_port(31130);
```

---

## 4. Model Management

### 4.1 Model Details

| Property | Value |
|----------|-------|
| **Name** | Ministral-3B-Instruct-2512-Q4_K_M |
| **Parameters** | 3 billion |
| **Quantization** | Q4_K_M (4-bit) |
| **Format** | GGUF |
| **Size** | ~2.15 GB |
| **Context Length** | Up to 256k tokens |
| **Download URL** | `https://leophir.com/models/Ministral-3-3B-Instruct-2512-Q4_K_M.gguf` |

### 4.2 Model Path Resolution

The model is located using this priority order:

1. **Current working directory**: `./models/`
2. **Next to executable**: `<exe_dir>/models/`
3. **User app data**: `%APPDATA%\ScreenSearch\models\` (Windows)
4. **Default fallback**: `./models/`

### 4.3 Download Functions

```rust
// Check if model exists
pub fn model_exists(models_dir: &Path) -> bool

// Check if download is needed
pub fn needs_download(models_dir: &Path) -> bool

// Download model (simple)
pub async fn download_model(models_dir: &Path) -> Result<PathBuf>

// Download with progress tracking
pub async fn download_model_with_progress(
    models_dir: &Path,
    progress_tx: Option<tokio::sync::mpsc::Sender<DownloadProgress>>,
) -> Result<PathBuf>
```

### 4.4 Download Progress Tracking

```rust
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub eta_seconds: u64,
}

impl DownloadProgress {
    pub fn percentage(&self) -> f64  // Returns 0.0-100.0
}
```

---

## 5. Server Lifecycle

### 5.1 llama-server Binary

| Property | Value |
|----------|-------|
| **Version** | b7562 |
| **Source** | llama.cpp GitHub releases |
| **Size** | ~22.5 MB (compressed) |
| **GPU Backend** | Vulkan (Windows) |
| **Default Port** | 31130 |

### 5.2 Platform-Specific Downloads

| Platform | URL |
|----------|-----|
| **Windows x64 (Vulkan)** | `llama-b7562-bin-win-vulkan-x64.zip` |
| **Linux x64** | `llama-b7562-bin-ubuntu-x64.zip` |
| **macOS ARM64** | `llama-b7562-bin-macos-arm64.zip` |
| **macOS x64** | `llama-b7562-bin-macos-x64.zip` |

### 5.3 Binary Path Resolution

1. **Same directory as exe**: `<exe_dir>/llama-server.exe`
2. **Bin subdirectory**: `<exe_dir>/bin/llama-server.exe`
3. **User app data**: `%APPDATA%\ScreenSearch\bin\`
4. **Default**: `./bin/`

### 5.4 Server Lifecycle States

```rust
pub enum ServerStatus {
    Stopped,              // Not running
    Starting,             // Loading model
    Running,              // Ready for requests
    Error(String),        // Fatal error
}
```

### 5.5 Lifecycle Flow

```
┌─────────────┐
│   Stopped   │◄──────────────────────────────────────────┐
└──────┬──────┘                                           │
       │ ensure_started()                                 │
       ▼                                                  │
┌─────────────┐                                           │
│  Starting   │──────────────────┐                        │
└──────┬──────┘                  │ health check fails     │
       │ health check OK         │ (max 60s timeout)      │
       ▼                         ▼                        │
┌─────────────┐           ┌─────────────┐                │
│   Running   │           │    Error    │                │
└──────┬──────┘           └─────────────┘                │
       │                                                  │
       │ idle timeout (5 min)                            │
       │ or stop() called                                │
       └──────────────────────────────────────────────────┘
```

### 5.6 LlamaServer API

```rust
impl LlamaServer {
    // Creation
    pub fn new(config: LlamaServerConfig) -> Self

    // Lifecycle
    pub async fn ensure_started(&self) -> Result<()>   // Lazy start
    pub async fn start(&self) -> Result<()>            // Force start
    pub async fn stop(&self) -> Result<()>             // Graceful stop
    pub async fn shutdown(&self)                        // Force shutdown

    // Status
    pub async fn status(&self) -> ServerStatus
    pub async fn active_port(&self) -> u16
    pub async fn endpoint(&self) -> String
    pub async fn is_process_alive(&self) -> bool

    // Idle management
    pub async fn idle_duration(&self) -> Duration
    pub async fn touch(&self)                           // Update last request time
    pub async fn check_idle_timeout(&self) -> bool
    pub async fn set_idle_ttl(&self, ttl: Duration)

    // Recovery
    pub async fn handle_crash(&self) -> Result<()>     // Auto-restart
}
```

### 5.7 Server Startup Command

```bash
llama-server \
  -m <model_path> \
  --port <port> \
  --host 127.0.0.1 \
  -c <context_length> \
  -ngl 99 \           # GPU layers (if use_gpu=true)
  -t <threads>        # CPU threads (if specified)
```

### 5.8 Port Fallback System

If the default port is busy, the server tries:
1. Port 31130 (default)
2. Port 31131
3. Port 31132

### 5.9 Auto-Restart Logic

- **Max restart attempts**: 3
- **Delay between restarts**: 2 seconds
- **Health check timeout**: 60 seconds
- **Health check interval**: 500ms
- **Health endpoint**: `GET http://127.0.0.1:{port}/health`

---

## 6. Inference Engine

### 6.1 LlmEngine API

```rust
impl LlmEngine {
    // Creation
    pub async fn new() -> Result<Self>
    pub async fn with_config(config: LlmConfig) -> Result<Self>

    // Text generation
    pub async fn generate_with_profile(
        &self,
        prompt: &str,
        profile: InferenceProfile
    ) -> Result<String>

    // Vision/multimodal
    pub async fn generate_with_image_and_profile(
        &self,
        prompt: &str,
        image: &DynamicImage,
        profile: InferenceProfile
    ) -> Result<String>

    // Utilities
    pub fn is_loaded(&self) -> bool
    pub fn model_path(&self) -> &PathBuf
    pub fn config(&self) -> &LlmConfig
    pub fn endpoint(&self) -> &str
}
```

### 6.2 TextGenerator Trait

```rust
pub trait TextGenerator: Send + Sync {
    async fn generate(&self, prompt: &str, system: Option<&str>) -> Result<String>;

    async fn generate_with_image(
        &self,
        prompt: &str,
        image: &DynamicImage,
        system: Option<&str>,
    ) -> Result<String>;

    fn is_ready(&self) -> bool;
    fn provider_name(&self) -> &str;
}
```

### 6.3 Inference Profiles

#### VisionAnalysis Profile
- **Purpose**: Screen content extraction (precise, structured)
- **Temperature**: 0.2 (deterministic)
- **Max tokens**: 512 (brief responses)
- **System prompt**: Extracts app name, content type, key elements, user actions, topics

#### RagAnswer Profile
- **Purpose**: Conversational RAG responses (natural, source-citing)
- **Temperature**: 0.4 (slightly creative)
- **Max tokens**: 1024 (longer responses)
- **System prompt**: Answers based on screen activity, cites sources with timestamps

#### Custom Profile
- Uses default inference parameters from config
- No system prompt injected

### 6.4 InferenceParameters

```rust
pub struct InferenceParameters {
    pub temperature: f32,       // 0.0-2.0
    pub top_k: usize,           // Token count
    pub top_p: f32,             // 0.0-1.0 (nucleus sampling)
    pub max_tokens: usize,      // Output length
    pub repeat_penalty: f32,    // >= 1.0
}

pub fn validate_parameters(params: &InferenceParameters) -> Result<(), String>
```

---

## 7. API Endpoints

### 7.1 Provider Validation

**POST /api/ai/validate**

Tests connection to AI provider (local or remote).

```json
// Request
{
  "provider_url": "local",    // or "http://localhost:11434/v1"
  "api_key": "",              // Optional for local
  "model": "ministral-3b"
}

// Response
{
  "success": true,
  "message": "Local Ministral-3B model is ready. llama-server is running."
}
```

### 7.2 Report Generation

**POST /api/ai/generate**

Generates intelligence report using RAG context.

```json
// Request
{
  "provider_url": "local",
  "api_key": "",
  "model": "ministral-3b",
  "start_time": "2025-12-27T00:00:00Z",  // Optional
  "end_time": "2025-12-28T00:00:00Z",    // Optional
  "prompt": "What did I work on yesterday?" // Optional
}

// Response
{
  "report": "# Executive Summary\n\n...",
  "model_used": "ministral-3b",
  "tokens_used": 1523,
  "context_source": "Semantic Search"
}
```

### 7.3 Model Status

**GET /api/ai/model/status**

```json
{
  "downloaded": true,
  "downloading": false,
  "model_name": "Ministral-3B-Instruct-2512-Q4_K_M",
  "model_size_bytes": 2150000000,
  "model_path": "C:\\Users\\...\\models\\Ministral-3-3B-Instruct-2512-Q4_K_M.gguf"
}
```

### 7.4 Model Download

**POST /api/ai/model/download**

```json
{
  "success": true,
  "message": "Download started. Model size: 2.15 GB"
}
```

### 7.5 Server Status

**GET /api/ai/server/status**

```json
{
  "status": "running",
  "port": 31130,
  "idle_seconds": 45,
  "model_loaded": true,
  "server_binary_available": true
}
```

### 7.6 Server Control

**POST /api/ai/server/start**
```json
{
  "success": true,
  "message": "Server running on port 31130",
  "status": "running"
}
```

**POST /api/ai/server/stop**
```json
{
  "success": true,
  "message": "Server stopped",
  "status": "stopped"
}
```

**POST /api/ai/server/ttl**
```json
// Request
{ "ttl_seconds": 600 }

// Response
{
  "success": true,
  "message": "TTL updated to 600 seconds",
  "status": "running"
}
```

**POST /api/ai/server/download**
```json
{
  "success": true,
  "message": "Download started. Binary size: ~22.5 MB"
}
```

---

## 8. Frontend Integration

### 8.1 Store Configuration (useStore.ts)

```typescript
interface AppStore {
  aiConfig: {
    providerUrl: string;      // Default: 'local'
    apiKey: string;           // Default: ''
    model: string;            // Default: 'ministral-3b'
  };
  setAiConfig: (config: Partial<AiConfig>) => void;
}
```

### 8.2 Provider Options (AiSettings.tsx)

```typescript
const PROVIDER_OPTIONS = [
  { value: 'local', label: 'Local (Ministral-3B)', description: 'Built-in GPU-accelerated model' },
  { value: 'http://localhost:11434/v1', label: 'Ollama', description: 'Local Ollama server' },
  { value: 'custom', label: 'Custom API', description: 'OpenAI-compatible endpoint' },
];
```

### 8.3 UI Flow

1. User selects provider from dropdown
2. For "local" provider:
   - API key and model fields are hidden
   - GPU info card is displayed
   - Button shows "Check Local Model"
3. Test button validates connection via `/api/ai/validate`
4. Configuration persisted in localStorage via Zustand

### 8.4 SettingsPanel Integration

The SettingsPanel (Data & AI tab) provides additional controls:

- **Vision toggle**: Enable/disable AI features
- **Model download**: Trigger model download with progress
- **Server management**: Start/stop/TTL controls
- **Status monitoring**: Real-time server status polling

---

## 9. GPU Acceleration

### 9.1 Vulkan Backend

ScreenSearch uses Vulkan for GPU acceleration because:

- **Cross-platform**: Works on NVIDIA, AMD, and Intel GPUs
- **No extra drivers**: Vulkan is included with modern GPU drivers
- **Automatic fallback**: Falls back to CPU if no GPU available

### 9.2 GPU Configuration

```rust
// In LlmConfig
pub use_gpu: bool,  // Default: true

// In llama-server startup
if config.use_gpu {
    args.push("-ngl".to_string());
    args.push("99".to_string());  // Offload all layers to GPU
}
```

### 9.3 Expected GPU Output

When llama-server starts with GPU acceleration:
```
ggml_vulkan: Using AMD Radeon RX 7900 XTX (RADV)
ggml_vulkan: Device memory: 24576 MB
```

Or for NVIDIA:
```
ggml_vulkan: Using NVIDIA GeForce RTX 4090
ggml_vulkan: Device memory: 24564 MB
```

### 9.4 CPU Fallback

If no Vulkan-compatible GPU is found, llama-server automatically falls back to CPU inference. Performance will be slower but functional.

---

## 10. Troubleshooting

### 10.1 Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Model not found | Model not downloaded | Click "Download Model" in settings |
| Server won't start | Binary not downloaded | Click "Download Server" in settings |
| Port in use | Another process on 31130 | Server will auto-fallback to 31131 |
| Slow inference | CPU fallback | Verify GPU drivers are installed |
| DLL errors (Windows) | Missing dependencies | Re-download llama-server binary |

### 10.2 Log Messages

**Successful startup:**
```
INFO  Downloading llama-server from: https://github.com/...
INFO  llama-server extracted to C:\Users\...\bin\llama-server.exe
INFO  Starting llama-server on port 31130
INFO  llama-server started successfully
```

**GPU detection:**
```
INFO  ggml_vulkan: Using AMD Radeon RX 7900 XTX
```

**Idle shutdown:**
```
INFO  Server idle for 300s, initiating shutdown
INFO  llama-server stopped
```

### 10.3 Verification Steps

1. **Check model status**: `GET /api/ai/model/status`
2. **Check server status**: `GET /api/ai/server/status`
3. **Test connection**: `POST /api/ai/validate` with `provider_url: "local"`
4. **Check logs**: Look for llama-server startup messages

---

## 11. Future Enhancements

### 11.1 Known Stubs and TODOs

| Location | Issue | Description |
|----------|-------|-------------|
| `ai.rs:456` | `downloading: false` | Download state tracking not implemented |
| Native inference | Dependency conflicts | Direct Rust LLM inference blocked by windows crate conflicts |

### 11.2 Planned Features

1. **Download Progress API**: Real-time progress tracking for model/binary downloads
2. **Multiple Model Support**: Allow switching between different local models
3. **Native Inference**: Direct Rust-based inference without external process
4. **Model Fine-tuning**: Support for LoRA adapters
5. **Streaming Responses**: Stream tokens as they're generated

### 11.3 Contributing

When extending the LLM system:

1. Update `screensearch-llm/src/lib.rs` for new exports
2. Add tests in the respective module
3. Update this documentation
4. Update `docs/api-reference.md` for new endpoints
5. Update `docs/STUBS.md` if adding new stubs

---

## Summary

The embedded LLM system provides a fully local, privacy-preserving AI experience:

| Aspect | Details |
|--------|---------|
| **Model** | Ministral-3B-Instruct-2512-Q4_K_M (2.15GB) |
| **Server** | llama.cpp b7562 with Vulkan GPU |
| **Default Port** | 31130 (fallback: 31131, 31132) |
| **Idle Timeout** | 300 seconds (configurable 60-3600s) |
| **API Protocol** | OpenAI-compatible `/v1/chat/completions` |
| **GPU Support** | NVIDIA, AMD, Intel via Vulkan |
| **Auto-management** | Lazy start, idle shutdown, crash recovery |

---

*Last updated: 2025-12-28 (v0.3.0)*
