//! Application state management

use screensearch_automation::AutomationEngine;
use screensearch_db::DatabaseManager;
use screensearch_embeddings::EmbeddingEngine;
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

    /// Flag indicating if background embedding generation is running
    pub is_generating_embeddings: Arc<std::sync::atomic::AtomicBool>,
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
            is_generating_embeddings: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
}


