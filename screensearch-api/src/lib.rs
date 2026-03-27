//! REST API Module for ScreenSearch
//!
//! This crate provides a REST API server on localhost:3131 for querying captured
//! screen content and controlling computer automation.
//!
//! # API Endpoints
//!
//! ## Context Retrieval
//! - `GET /search` - Full-text search with filters (time, app, keywords)
//! - `GET /search/keywords` - Keyword search with BM25 ranking
//! - `GET /frames` - Retrieve captured frames with filters
//! - `GET /health` - System health check
//!
//! ## Computer Automation
//! - `POST /automation/find-elements` - Locate UI elements
//! - `POST /automation/click` - Click at coordinates
//! - `POST /automation/type` - Type text into active element
//! - `POST /automation/scroll` - Scroll action
//! - `POST /automation/press-key` - Press keyboard key
//! - `POST /automation/get-text` - Extract text from UI element
//! - `POST /automation/list-elements` - List interactive elements
//! - `POST /automation/open-app` - Launch application
//! - `POST /automation/open-url` - Open URL in browser
//!
//! ## System Management
//! - `GET /tags` - List all tags
//! - `POST /tags` - Create new tag
//! - `DELETE /tags/{id}` - Delete tag
//! - `GET /frames/{id}/tags` - Get tags for frame
//! - `POST /frames/{id}/tags` - Add tag to frame
//! - `DELETE /frames/{id}/tags/{tag_id}` - Remove tag from frame
//!
//! # Example
//!
//! ```no_run
//! use screen_api::{ApiConfig, ApiServer};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Initialize tracing
//!     tracing_subscriber::fmt::init();
//!
//!     // Create server with default config
//!     let config = ApiConfig::default();
//!     let server = ApiServer::new(config).await?;
//!
//!     // Run server
//!     server.run().await?;
//!
//!     Ok(())
//! }
//! ```

pub mod embedded;
pub mod error;
pub mod handlers;
pub mod models;
pub mod routes;
pub mod server;
pub mod state;
pub mod workers;

pub use embedded::Assets;
pub use error::{AppError, Result};
pub use server::{ApiConfig, ApiServer};
pub use state::AppState;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_config_default() {
        let config = ApiConfig::default();
        assert_eq!(config.port, 3131);
        assert_eq!(config.host, "127.0.0.1");
    }
}
