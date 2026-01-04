//! Application state management

use screensearch_automation::AutomationEngine;
use screensearch_db::DatabaseManager;
use screensearch_embeddings::EmbeddingEngine;
use screensearch_llm::LlamaServer;
use std::collections::HashMap;
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

// Conversion from screensearch_embeddings::DownloadProgress
impl From<screensearch_embeddings::DownloadProgress> for DownloadProgress {
    fn from(p: screensearch_embeddings::DownloadProgress) -> Self {
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
    ) -> Self {
        Self {
            db: Arc::new(db),
            automation: Arc::new(automation),
            embedding_engine: Arc::new(RwLock::new(None)),
            capture_interval_ms,
            llama_server: Arc::new(RwLock::new(None)),
            download_progress: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or initialize the embedding engine
    pub async fn get_embedding_engine(&self) -> Result<Arc<EmbeddingEngine>, String> {
        // Check if already initialized
        {
            let guard = self.embedding_engine.read().await;
            if let Some(engine) = guard.as_ref() {
                return Ok(Arc::clone(engine));
            }
        }

        // Initialize the engine
        let engine = EmbeddingEngine::new().await.map_err(|e| e.to_string())?;
        let engine_arc = Arc::new(engine);

        // Store it
        {
            let mut guard = self.embedding_engine.write().await;
            *guard = Some(Arc::clone(&engine_arc));
        }

        Ok(engine_arc)
    }

    /// Get or initialize the LlamaServer
    pub async fn get_llama_server(&self) -> Result<Arc<LlamaServer>, String> {
        use screensearch_llm::{LlamaServerConfig, get_model_path, get_models_dir};

        // Check if already initialized
        {
            let guard = self.llama_server.read().await;
            if let Some(server) = guard.as_ref() {
                return Ok(Arc::clone(server));
            }
        }

        // Initialize the server
        let models_dir = get_models_dir();
        let model_path = get_model_path(&models_dir);

        let config = LlamaServerConfig {
            model_path,
            ..Default::default()
        };

        let server = LlamaServer::new(config);
        let server_arc = Arc::new(server);

        // Store it
        {
            let mut guard = self.llama_server.write().await;
            *guard = Some(Arc::clone(&server_arc));
        }

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
