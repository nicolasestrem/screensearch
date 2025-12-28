//! Route definitions

use crate::handlers;
use crate::state::AppState;
use crate::Assets;
use axum::http::Uri;
use axum::response::IntoResponse;
use axum::{
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

/// Build the complete API router
pub fn build_router(state: Arc<AppState>) -> Router {
    // API routes (prefixed with /api)
    let api_routes = Router::new()
        // Search endpoints
        .nest("/search", search_routes())
        // Frame endpoints
        .nest("/frames", frame_routes())
        // Automation endpoints
        .nest("/automation", automation_routes())
        // Tag endpoints
        .nest("/tags", tag_routes())
        // Settings endpoints
        .nest("/settings", settings_routes())
        // AI endpoints
        .nest("/ai", ai_routes())
        // Embeddings endpoints (RAG)
        .nest("/embeddings", embeddings_routes())
        // RAG Answer generation
        .route("/generate", post(handlers::generate_answer));

    // Root level routes (no prefix)
    Router::new()
        // Nest API routes under /api
        .nest("/api", api_routes
            .route("/health", get(handlers::system::health))
            .route("/test-vision", post(handlers::system::test_vision_config))
        )
        // Serve embedded static files for all other routes (SPA fallback)
        .fallback(serve_embedded)
        .with_state(state)
}

/// Serve embedded static assets
///
/// This handler serves files embedded in the binary at compile time using rust-embed.
/// Files are served from memory with proper MIME types and SPA fallback support.
async fn serve_embedded(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Empty path = index.html
    let path = if path.is_empty() { "index.html" } else { path };

    // Try to serve the file from embedded assets
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_type(path);
            axum::response::Response::builder()
                .header("content-type", mime)
                .body(axum::body::Body::from(content.data.into_owned()))
                .unwrap()
        }
        None => {
            // SPA fallback: serve index.html for non-API, non-asset routes
            // This enables client-side routing (e.g., /search, /settings)
            if !path.starts_with("api/") && !path.starts_with("assets/") {
                match Assets::get("index.html") {
                    Some(content) => axum::response::Response::builder()
                        .header("content-type", "text/html")
                        .body(axum::body::Body::from(content.data.into_owned()))
                        .unwrap(),
                    None => not_found_response(),
                }
            } else {
                not_found_response()
            }
        }
    }
}

fn not_found_response() -> axum::response::Response<axum::body::Body> {
    axum::response::Response::builder()
        .status(404)
        .body(axum::body::Body::from("Not found"))
        .unwrap()
}

fn mime_type(path: &str) -> &'static str {
    match path.split('.').next_back() {
        Some("html") => "text/html",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") | Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

/// Search-related routes
fn search_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(handlers::search))
        .route("/keywords", get(handlers::search_keywords))
}

/// Frame-related routes
fn frame_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(handlers::get_frames))
        .route("/:id", get(handlers::get_single_frame))
        .route("/:id/image", get(handlers::get_frame_image))
        .route("/:id/tags", post(handlers::add_tag_to_frame))
        .route("/:id/tags", get(handlers::get_frame_tags))
        .route("/:id/tags/:tag_id", delete(handlers::remove_tag_from_frame))
}

/// Automation routes
fn automation_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/find-elements", post(handlers::find_elements))
        .route("/click", post(handlers::click))
        .route("/type", post(handlers::type_text))
        .route("/scroll", post(handlers::scroll))
        .route("/press-key", post(handlers::press_key))
        .route("/get-text", post(handlers::get_text))
        .route("/list-elements", post(handlers::list_elements))
        .route("/open-app", post(handlers::open_app))
        .route("/open-url", post(handlers::open_url))
}

/// Tag management routes
fn tag_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(handlers::create_tag))
        .route("/", get(handlers::list_tags))
        .route("/:id", put(handlers::update_tag))
        .route("/:id", delete(handlers::delete_tag))
}

/// Settings routes
fn settings_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(handlers::get_settings))
        .route("/", post(handlers::update_settings))
}

/// AI routes
fn ai_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/validate", post(handlers::validate_connection))
        .route("/generate", post(handlers::generate_report))
}

/// Embeddings routes for RAG
fn embeddings_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status", get(handlers::get_embedding_status))
        .route("/generate", post(handlers::generate_embeddings))
        .route("/enable", post(handlers::toggle_embeddings))
}

