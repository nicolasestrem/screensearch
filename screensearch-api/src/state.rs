//! Application state management

use screensearch_automation::AutomationEngine;
use screensearch_db::DatabaseManager;
use screensearch_embeddings::EmbeddingEngine;
use screensearch_llm::LlamaServer;
use std::sync::Arc;
use tokio::sync::RwLock;

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
}
