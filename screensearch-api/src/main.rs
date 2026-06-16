//! ScreenSearch REST API Server
//!
//! Main entry point for the API server that provides search and automation
//! capabilities for captured screen content.

use screensearch_api::{ApiConfig, ApiServer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing with environment filter
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "screensearch_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("ScreenSearch API Server starting...");

    // Create server configuration
    let config = ApiConfig::default();

    // Create dummy share state for standalone API server
    let capture_interval = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(3000));
    // No capture engine runs in the standalone server, so the monitor selection
    // receiver is never read; keep the sender alive to satisfy the signature.
    let (monitor_config_tx, _monitor_config_rx) = tokio::sync::watch::channel(Vec::new());

    // Initialize and run server
    let server = ApiServer::new(config, capture_interval, monitor_config_tx).await?;

    tracing::info!("Server initialized, listening on port 3131");
    tracing::info!("Press Ctrl+C to shut down");

    server.run().await?;

    Ok(())
}
