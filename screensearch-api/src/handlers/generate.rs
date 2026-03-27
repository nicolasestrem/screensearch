use crate::error::{AppError, Result};
use crate::state::AppState;
use axum::{extract::State, Json};
use screensearch_vision::client::OllamaClient; // Use struct directly
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error};

#[derive(Debug, Deserialize)]
pub struct GenerateRequest {
    pub query: String,
}

#[derive(Debug, Serialize)]
pub struct GenerateResponse {
    pub answer: String,
    pub sources: Vec<i64>, // Frame IDs
}

/// POST /generate - Generate an answer based on screen context
pub async fn generate_answer(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>> {
    debug!("Generate answer request: {}", req.query);

    // 1. Get Settings for AI Provider
    let settings = state.db.get_settings().await.map_err(|e| {
        error!("Failed to get settings: {}", e);
        AppError::Database(e)
    })?;

    if settings.vision_enabled == 0 {
        return Err(AppError::InvalidRequest(
            "AI features are disabled in settings".to_string(),
        ));
    }

    // 2. Perform Semantic Search for Context
    // Initialize embedding engine
    let engine = state.get_embedding_engine().await.map_err(|e| {
        error!("Failed to initialize embedding engine: {}", e);
        AppError::Internal(e)
    })?;

    // Embed query
    let embedding = engine.embed(&req.query).map_err(|e| {
        error!("Failed to generate query embedding: {}", e);
        AppError::Internal(e.to_string())
    })?;

    // Search DB (limit 5 for context)
    let search_results = state
        .db
        .search_embeddings(embedding, 5, 0.4) // Threshold 0.4
        .await
        .map_err(AppError::Database)?;

    if search_results.is_empty() {
        return Ok(Json(GenerateResponse {
            answer: "I couldn't find any relevant screen content to answer your question.".to_string(),
            sources: vec![],
        }));
    }

    // 3. Construct Context
    let mut context_str = String::new();
    let mut source_ids = Vec::new();

    for (i, res) in search_results.iter().enumerate() {
        // Format timestamp
        let time = res.frame.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
        let app = res.frame.active_process.as_deref().unwrap_or("Unknown App");
        let window = res.frame.active_window.as_deref().unwrap_or("Unknown Window");
        let text = &res.chunk_text;

        context_str.push_str(&format!(
            "[{}] Time: {}, App: {}, Window: {}\nContent: {}\n\n",
            i + 1, time, app, window, text
        ));
        source_ids.push(res.frame.id);
    }

    // 4. Initialize AI Client
    let client = OllamaClient::new(
        settings.vision_endpoint.clone(),
        settings.vision_model.clone(),
        settings.vision_api_key.clone(),
        settings.vision_provider.clone(),
    );

    // 5. Call LLM
    let system_prompt = "You are ScreenSearch AI, a helpful assistant that answers questions based strictly on the user's screen history context provided. 
    If the context doesn't contain the answer, say so. 
    Be concise but helpful. 
    Cite sources by referring to the App or Time if relevant.";
    
    let user_prompt = format!(
        "User Question: {}\n\nContext from Screen History:\n{}",
        req.query, context_str
    );

    let answer = client
        .generate_text(&user_prompt, Some(system_prompt))
        .await
        .map_err(|e| {
            error!("LLM Generation failed: {}", e);
            AppError::Internal(format!("LLM generation failed: {}", e))
        })?;

    Ok(Json(GenerateResponse {
        answer,
        sources: source_ids,
    }))
}
