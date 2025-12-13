//! API server implementation

use crate::routes;
use crate::state::AppState;
use axum::http::{HeaderName, Method};
use screensearch_automation::AutomationEngine;
use screensearch_db::DatabaseManager;
use std::sync::Arc;
use tower_http::cors::{AllowHeaders, CorsLayer};
use tower_http::trace::TraceLayer;

/// API server configuration
#[derive(Debug, Clone)]
pub struct ApiConfig {
    /// Host address to bind to
    pub host: String,

    /// Port to listen on
    pub port: u16,

    /// Path to SQLite database file
    pub database_path: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3131,
            database_path: "screensearch.db".to_string(),
        }
    }
}

/// Main API server
pub struct ApiServer {
    config: ApiConfig,
    state: Arc<AppState>,
}

impl ApiServer {
    /// Create a new API server
    ///
    /// Initializes database connection and automation engine.
    pub async fn new(config: ApiConfig) -> anyhow::Result<Self> {
        tracing::info!("Initializing API server at {}:{}", config.host, config.port);

        // Initialize database manager
        let db = DatabaseManager::new(&config.database_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize database: {}", e))?;

        tracing::info!("Database initialized: {}", config.database_path);

        // Initialize automation engine
        let automation = AutomationEngine::new()
            .map_err(|e| anyhow::anyhow!("Failed to initialize automation engine: {}", e))?;

        tracing::info!("Automation engine initialized");

        // Create application state
        let state = Arc::new(AppState::new(db, automation));

        Ok(Self { config, state })
    }

    /// Build the Axum router with middleware
    fn build_router(&self) -> axum::Router {
        use axum::extract::DefaultBodyLimit;
        use tower_http::cors::AllowOrigin;

        // Restrict CORS to localhost origins for security
        // Allows dev server (3100) and production server (3131) on localhost only
        let cors = CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(|origin, _| {
                origin
                    .to_str()
                    .map(|s| {
                        s.starts_with("http://localhost:")
                            || s.starts_with("http://127.0.0.1:")
                            || s.starts_with("http://[::1]:")
                    })
                    .unwrap_or(false)
            }))
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers(AllowHeaders::list(vec![
                HeaderName::from_static("content-type"),
                HeaderName::from_static("authorization"),
                HeaderName::from_static("accept"),
                HeaderName::from_static("origin"),
                HeaderName::from_static("user-agent"),
                HeaderName::from_static("cache-control"),
                HeaderName::from_static("pragma"),
            ]))
            .allow_credentials(true);

        routes::build_router(Arc::clone(&self.state))
            .layer(DefaultBodyLimit::max(1024 * 1024)) // 1 MiB (1024 * 1024 bytes) max request body
            .layer(cors)
            .layer(TraceLayer::new_for_http())
    }

    /// Run the API server
    ///
    /// Starts the HTTP server and blocks until shutdown.
    pub async fn run(self) -> anyhow::Result<()> {
        let app = self.build_router();
        let addr = format!("{}:{}", self.config.host, self.config.port);

        tracing::info!("Starting API server on http://{}", addr);
        tracing::info!("API documentation available at http://{}/health", addr);

        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;

        axum::serve(listener, app)
            .await
            .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

        Ok(())
    }

    /// Start the background embedding worker
    pub async fn start_embedding_worker(
        &self,
        config: crate::workers::embedding_worker::EmbeddingWorkerConfig,
    ) -> anyhow::Result<()> {
        if !config.enabled {
            return Ok(());
        }

        tracing::info!("Initializing embedding engine for background worker...");
        
        // Force initialization of embedding engine
        let engine = self.state.get_embedding_engine().await
            .map_err(|e| anyhow::anyhow!("Failed to initialize embedding engine: {}", e))?;

        tracing::info!("Starting background embedding worker...");
        
        // Spawn worker
        crate::workers::embedding_worker::spawn_embedding_worker(
            std::sync::Arc::clone(&self.state.db),
            engine,
            config,
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ApiConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 3131);
    }

    #[test]
    fn test_custom_config() {
        let config = ApiConfig {
            host: "0.0.0.0".to_string(),
            port: 8080,
            database_path: "test.db".to_string(),
        };
        assert_eq!(config.port, 8080);
        assert_eq!(config.database_path, "test.db");
    }
}
