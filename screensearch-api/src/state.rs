//! Application state management

use screensearch_automation::AutomationEngine;
use screensearch_db::DatabaseManager;
use screensearch_embeddings::{EmbeddingEngine, EMBEDDING_DIM};
use screensearch_llm::LlamaServer;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Unified download progress structure
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    /// Bytes downloaded so far
    pub bytes_downloaded: u64,
    /// Total bytes to download
    pub total_bytes: u64,
    /// Download speed in bytes per second
    pub speed_bps: u64,
    /// Estimated time remaining in seconds
    pub eta_seconds: u64,
    /// Error message if download failed
    pub error: Option<String>,
}

impl DownloadProgress {
    /// Get progress as a percentage (0.0 - 100.0)
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.bytes_downloaded as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

// Conversion from screensearch_llm::DownloadProgress
impl From<screensearch_llm::DownloadProgress> for DownloadProgress {
    fn from(p: screensearch_llm::DownloadProgress) -> Self {
        Self {
            bytes_downloaded: p.bytes_downloaded,
            total_bytes: p.total_bytes,
            speed_bps: p.speed_bps,
            eta_seconds: p.eta_seconds,
            error: None,
        }
    }
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Database manager for querying captured data
    pub db: Arc<DatabaseManager>,

    /// Automation engine for UI control
    pub automation: Arc<AutomationEngine>,

    /// Embedding engine for semantic search (lazy initialized)
    pub embedding_engine: Arc<RwLock<Option<Arc<EmbeddingEngine>>>>,

    /// Shared capture interval in milliseconds (atomic for thread safety)
    pub capture_interval_ms: Arc<std::sync::atomic::AtomicU64>,

    /// Desired capture monitor indices (`[]` = all monitors). Updating settings
    /// sends the new selection here so the capture engine can reconfigure live.
    pub monitor_config_tx: tokio::sync::watch::Sender<Vec<usize>>,

    /// Local LLM server (auto-managed llama-server process)
    pub llama_server: Arc<RwLock<Option<Arc<LlamaServer>>>>,

    /// Download progress tracking
    /// Keys: "llm_model", "llama_server"
    /// Note: Embeddings downloads happen automatically on first use without explicit progress tracking
    pub download_progress: Arc<RwLock<HashMap<String, DownloadProgress>>>,
}

impl AppState {
    /// Create new application state
    pub fn new(
        db: DatabaseManager,
        automation: AutomationEngine,
        capture_interval_ms: Arc<std::sync::atomic::AtomicU64>,
        monitor_config_tx: tokio::sync::watch::Sender<Vec<usize>>,
    ) -> Self {
        Self {
            db: Arc::new(db),
            automation: Arc::new(automation),
            embedding_engine: Arc::new(RwLock::new(None)),
            capture_interval_ms,
            monitor_config_tx,
            llama_server: Arc::new(RwLock::new(None)),
            download_progress: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Whether the in-process embedding engine has already been loaded.
    /// Cheap, non-blocking check that never triggers a model download.
    pub async fn embedding_engine_initialized(&self) -> bool {
        self.embedding_engine.read().await.is_some()
    }

    /// Get or initialize the embedding engine
    pub async fn get_embedding_engine(&self) -> Result<Arc<EmbeddingEngine>, String> {
        // Fast path: already initialized.
        {
            let guard = self.embedding_engine.read().await;
            if let Some(engine) = guard.as_ref() {
                return Ok(Arc::clone(engine));
            }
        }

        // Slow path: hold the write lock across initialization so that
        // concurrent first-use callers serialize here instead of racing to
        // build and health-check duplicate engines. Re-check inside the write
        // lock in case another task initialized while we waited for it.
        let mut guard = self.embedding_engine.write().await;
        if let Some(engine) = guard.as_ref() {
            return Ok(Arc::clone(engine));
        }

        // Loads (and, on first run, downloads + caches) the in-process ONNX
        // embedding model. No network health check is needed any more.
        let engine = EmbeddingEngine::new().await.map_err(|e| e.to_string())?;
        self.db
            .ensure_embedding_model(
                engine.provider(),
                engine.model(),
                engine.model_version(),
                EMBEDDING_DIM,
            )
            .await
            .map_err(|e| e.to_string())?;
        let engine_arc = Arc::new(engine);
        *guard = Some(Arc::clone(&engine_arc));

        Ok(engine_arc)
    }

    /// Resolve which model (and optional vision projector) the unified
    /// llama-server should run.
    ///
    /// When vision is enabled with the local provider and a vision-capable
    /// model + projector is available, the single server runs that model with
    /// its `mmproj` so it serves both text generation and vision (Option B:
    /// unify on gemma-4). Otherwise it runs the first discovered GGUF text-only.
    async fn resolve_server_models(&self) -> (PathBuf, Option<PathBuf>) {
        use screensearch_llm::{resolve_model_path, resolve_vision_model};

        if let Ok(settings) = self.db.get_settings().await {
            if settings.vision_enabled != 0 && settings.vision_provider == "local" {
                if let Some((model, mmproj)) = resolve_vision_model(&settings.vision_model) {
                    return (model, Some(mmproj));
                }
            }
        }

        (resolve_model_path(), None)
    }

    /// Get or initialize the LlamaServer.
    ///
    /// The server is (re)built when the desired model/projector differs from
    /// the cached one — e.g. when vision is toggled, which switches the unified
    /// server to a gemma-4 model loaded with `--mmproj`. The whole compare/
    /// rebuild runs under the write lock so concurrent callers (AI reports and
    /// the vision worker) converge on a single server instance.
    pub async fn get_llama_server(&self) -> Result<Arc<LlamaServer>, String> {
        use screensearch_llm::LlamaServerConfig;

        let (model_path, mmproj_path) = self.resolve_server_models().await;

        let mut guard = self.llama_server.write().await;

        // Reuse the cached server when it already targets the same model + projector.
        if let Some(server) = guard.as_ref() {
            if server.current_models().await == (model_path.clone(), mmproj_path.clone()) {
                return Ok(Arc::clone(server));
            }
        }

        // Desired model/projector changed: stop the stale server and rebuild.
        if let Some(previous) = guard.take() {
            previous.shutdown().await;
        }

        let config = LlamaServerConfig {
            model_path,
            mmproj_path,
            ..Default::default()
        };

        let server_arc = Arc::new(LlamaServer::new(config));
        *guard = Some(Arc::clone(&server_arc));

        Ok(server_arc)
    }

    /// Shutdown the LlamaServer if running
    pub async fn shutdown_llama_server(&self) {
        let guard = self.llama_server.read().await;
        if let Some(server) = guard.as_ref() {
            server.shutdown().await;
        }
    }

    /// Update download progress for a specific download
    pub async fn update_download_progress(&self, key: String, progress: DownloadProgress) {
        let mut guard = self.download_progress.write().await;
        guard.insert(key, progress);
    }

    /// Remove download progress when complete
    pub async fn clear_download_progress(&self, key: &str) {
        let mut guard = self.download_progress.write().await;
        guard.remove(key);
    }

    /// Get all current download progress statuses
    pub async fn get_all_download_progress(&self) -> HashMap<String, DownloadProgress> {
        let guard = self.download_progress.read().await;
        guard.clone()
    }
}
