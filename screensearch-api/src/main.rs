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

    // Initialize and run server
    let server = ApiServer::new(config).await?;

    tracing::info!("Server initialized, listening on port 3131");
    tracing::info!("Press Ctrl+C to shut down");

    server.run().await?;

    Ok(())
}
