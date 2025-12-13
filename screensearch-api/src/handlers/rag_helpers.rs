//! RAG helper functions for enhanced report generation

use crate::error::{AppError, Result};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use screensearch_db::{FrameFilter, Pagination};
use std::sync::Arc;
use tracing::{info, warn, error};

/// Weight for semantic results in hybrid search (0.0 to 1.0)
const SEMANTIC_WEIGHT: f32 = 0.3;
/// Maximum number of results to fetch for RAG context
const MAX_RAG_RESULTS: i64 = 50;

/// Build context for LLM using RAG-enhanced retrieval
pub async fn build_rag_context(
    state: &Arc<AppState>,
    user_query: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<(String, String)> {
    // Check if embeddings are enabled
    let embedding_status = state.db.get_embedding_status().await.ok();
    let use_rag = embedding_status
        .as_ref()
        .map(|s| s.enabled && s.frames_with_embeddings > 0)
        .unwrap_or(false);

    if use_rag {
        build_rag_enhanced_context(state, user_query, start_time, end_time).await
    } else {
        build_traditional_context(state, start_time, end_time).await
    }
}

/// Build context using RAG with hybrid search
async fn build_rag_enhanced_context(
    state: &Arc<AppState>,
    user_query: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<(String, String)> {
    info!("Using RAG-enhanced report generation");

    // Generate embedding for the query
    let engine = state.get_embedding_engine()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to load embedding engine: {}", e)))?;
        
    let query_embedding = engine
        .embed(user_query)
        .map_err(|e| AppError::Internal(format!("Failed to generate query embedding: {}", e)))?;

    // Perform hybrid search combining FTS5 and vector similarity
    let search_result = state
        .db
        .hybrid_search(
            user_query, 
            query_embedding, 
            SEMANTIC_WEIGHT, 
            MAX_RAG_RESULTS,
            start_time,
            end_time
        )
        .await;

    let mut relevant_results = match search_result {
        Ok(results) => {
            info!("Hybrid search found {} raw results", results.len());
            results
        },
        Err(e) => {
            error!("Hybrid search failed: {}", e);
            vec![]
        }
    };
    
    // Note: Time filtering is now done in the SQL query within hybrid_search/semantic_search
    // so we don't need to filter here.
    
    if relevant_results.is_empty() {
        warn!("No relevant results found after filtering. Context will be empty.");
        // Fallback to traditional context if RAG yields nothing? 
        // Or just let it be empty? 
        // Better to provide at least simple logs.
        return build_traditional_context(state, start_time, end_time).await;
    }

    // Apply keyword boosting for query terms
    super::reranker::boost_keyword_matches(&mut relevant_results, user_query, 0.2);

    // Rerank results for better relevance
    let config = super::reranker::RerankConfig {
        top_k: 20,
        recency_weight: 0.1,
        length_weight: 0.05,
        min_score: 0.0,
    };
    let reranked_results = super::reranker::rerank_results(relevant_results, &config);

    // Build rich context from relevant OCR text chunks
    let mut context = String::new();
    context.push_str(&format!(
        "Activity Period: {} to {}\n\n",
        start_time.format("%Y-%m-%d %H:%M"),
        end_time.format("%Y-%m-%d %H:%M")
    ));

    let mut ocr_chunks = Vec::new();

    for result in reranked_results.iter() {
        let app = result
            .frame
            .active_process
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());
        let window = result.frame.active_window.clone().unwrap_or_default();

        ocr_chunks.push(format!(
            "[{}] {} - {}: {}",
            result.frame.timestamp.format("%H:%M"),
            app,
            window,
            result.chunk_text.chars().take(200).collect::<String>()
        ));
    }

    context.push_str("Relevant Screen Content (OCR):\n");
    for chunk in ocr_chunks.iter().take(20) {
        context.push_str(&format!("- {}\n", chunk));
    }

    Ok((context, "Semantic Search".to_string()))
}

/// Build context using traditional frame-based approach
async fn build_traditional_context(
    state: &Arc<AppState>,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Result<(String, String)> {
    warn!("Embeddings not available, using traditional report generation");

    let filter = FrameFilter {
        start_time: Some(start_time),
        end_time: Some(end_time),
        app_name: None,
        device_name: None,
        tag_ids: None,
        monitor_index: None,
    };

    let pagination = Pagination {
        limit: 100,
        offset: 0,
    };

    let frames = state
        .db
        .get_frames_in_range(start_time, end_time, filter, pagination)
        .await
        .map_err(AppError::Database)?;

    // Summarize data for the prompt
    let total_frames = frames.len();
    let mut app_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut timeline_text = String::new();

    for frame in &frames {
        let app = frame
            .active_process
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());
        *app_counts.entry(app.clone()).or_insert(0) += 1;

        let window = frame.active_window.clone().unwrap_or_default();
        timeline_text.push_str(&format!(
            "- [{}] App: {}, Window: {}\n",
            frame.timestamp.format("%H:%M"),
            app,
            window
        ));
    }

    let most_used_apps = app_counts
        .iter()
        .map(|(k, v)| format!("{}: {} frames", k, v))
        .collect::<Vec<_>>()
        .join(", ");

    Ok((
        format!(
            "Activity Period: {} to {}\n\n\
            Summary Data:\n\
            - Total Snapshots: {}\n\
            - App Usage Distribution: {}\n\n\
            Detailed Log (Sample):\n\
            {}",
            start_time.format("%Y-%m-%d %H:%M"),
            end_time.format("%Y-%m-%d %H:%M"),
            total_frames,
            most_used_apps,
            timeline_text
        ),
        "Recent Activity (Fallback)".to_string(),
    ))
}
