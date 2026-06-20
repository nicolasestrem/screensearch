//! RAG helper functions for enhanced report generation

use crate::error::{AppError, Result};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use screensearch_db::{FrameFilter, Pagination, SemanticResult};
use std::sync::Arc;
use tracing::{info, warn};

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

    let relevant_results =
        retrieve_rag_results(state, user_query, start_time, end_time, 20).await?;

    // Note: Time filtering is now done in the SQL query within hybrid_search/semantic_search
    // so we don't need to filter here.

    if relevant_results.is_empty() {
        warn!("No relevant results found after filtering. Context will be empty.");
        // Fallback to traditional context if RAG yields nothing?
        // Or just let it be empty?
        // Better to provide at least simple logs.
        return build_traditional_context(state, start_time, end_time).await;
    }

    // Build rich context from relevant OCR text chunks
    let mut context = String::new();
    context.push_str(&format!(
        "Activity Period: {} to {}\n\n",
        start_time.format("%Y-%m-%d %H:%M"),
        end_time.format("%Y-%m-%d %H:%M")
    ));

    // Build context directly, limiting to 20 items early to avoid unnecessary allocations
    context.push_str("Relevant Screen Content (OCR):\n");
    for result in relevant_results.iter().take(20) {
        let app = result.frame.active_process.as_deref().unwrap_or("Unknown");
        let window = result.frame.active_window.as_deref().unwrap_or("");
        let text: String = result.chunk_text.chars().take(200).collect();

        context.push_str(&format!(
            "- [frame:{}] [{}] {} - {}: {}\n",
            result.frame.id,
            result.frame.timestamp.format("%H:%M"),
            app,
            window,
            text
        ));
    }

    Ok((context, "Semantic Search".to_string()))
}

/// Shared quality retrieval used by reports and conversational answers.
pub async fn retrieve_rag_results(
    state: &Arc<AppState>,
    user_query: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    top_k: usize,
) -> Result<Vec<SemanticResult>> {
    let engine = state
        .get_embedding_engine()
        .await
        .map_err(|error| AppError::Internal(format!("Failed to load embedding engine: {error}")))?;
    let query_embedding = engine.embed(user_query).await.map_err(|error| {
        AppError::Internal(format!("Failed to generate query embedding: {error}"))
    })?;

    let candidates = state
        .db
        .hybrid_search(
            user_query,
            query_embedding,
            None,
            MAX_RAG_RESULTS,
            start_time,
            end_time,
        )
        .await
        .map_err(AppError::Database)?;
    info!("Hybrid retrieval found {} candidates", candidates.len());
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let documents: Vec<String> = candidates
        .iter()
        .map(|result| {
            format!(
                "{} | {} | {} | {}",
                result.frame.timestamp,
                result.frame.active_process.as_deref().unwrap_or("Unknown"),
                result.frame.active_window.as_deref().unwrap_or(""),
                result.chunk_text
            )
        })
        .collect();

    // With the cross-encoder enabled, this reorders by relevance; when it is
    // disabled (the default), the engine returns the candidates in their
    // incoming RRF order so this path simply trims to `top_k`.
    let source_suffix = if engine.reranker_enabled() {
        "+reranker"
    } else {
        ""
    };
    match engine.rerank(user_query, &documents).await {
        Ok(scores) => Ok(scores
            .into_iter()
            .take(top_k)
            .filter_map(|score| {
                candidates.get(score.index).cloned().map(|mut result| {
                    result.similarity_score = score.score;
                    result.retrieval_source =
                        format!("{}{}", result.retrieval_source, source_suffix);
                    result
                })
            })
            .collect()),
        Err(error) => {
            warn!(
                "Quality reranker unavailable; retaining RRF order: {}",
                error
            );
            Ok(candidates.into_iter().take(top_k).collect())
        }
    }
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
    let mut app_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
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
