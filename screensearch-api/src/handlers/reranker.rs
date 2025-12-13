//! Reranking module for RAG pipeline
//!
//! Provides reranking functions to improve the ordering of search results
//! before they're sent to the LLM for report generation.

use screensearch_db::SemanticResult;
use tracing::debug;

/// Reranking configuration
#[derive(Debug, Clone)]
pub struct RerankConfig {
    /// Maximum number of results to return after reranking
    pub top_k: usize,
    /// Weight for recency (0.0 = ignore time, 1.0 = heavily favor recent)
    pub recency_weight: f32,
    /// Weight for text length (prefer chunks with more content)
    pub length_weight: f32,
    /// Minimum similarity score threshold
    pub min_score: f32,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            top_k: 20,
            recency_weight: 0.1,
            length_weight: 0.05,
            min_score: 0.0,
        }
    }
}

/// Rerank search results based on multiple signals
///
/// Combines the original similarity score with:
/// - Recency: more recent frames get a boost
/// - Length: longer chunks get a small boost (more context)
/// - Diversity: penalizes repeated content from same frame
pub fn rerank_results(
    mut results: Vec<SemanticResult>,
    config: &RerankConfig,
) -> Vec<SemanticResult> {
    if results.is_empty() {
        return results;
    }

    debug!("Reranking {} results", results.len());

    // Filter by minimum score
    results.retain(|r| r.similarity_score >= config.min_score);

    // Find time range for normalization
    let timestamps: Vec<i64> = results
        .iter()
        .map(|r| r.frame.timestamp.timestamp())
        .collect();
    let min_time = *timestamps.iter().min().unwrap_or(&0);
    let max_time = *timestamps.iter().max().unwrap_or(&0);
    let time_range = (max_time - min_time).max(1) as f32;

    // Find max text length for normalization
    let max_len = results
        .iter()
        .map(|r| r.chunk_text.len())
        .max()
        .unwrap_or(1) as f32;

    // Calculate combined scores
    let mut scored_results: Vec<(f32, SemanticResult)> = results
        .into_iter()
        .map(|r| {
            let base_score = r.similarity_score;

            // Recency boost: newer = higher
            let recency_normalized =
                (r.frame.timestamp.timestamp() - min_time) as f32 / time_range;
            let recency_boost = recency_normalized * config.recency_weight;

            // Length boost: longer = higher (normalized)
            let length_normalized = r.chunk_text.len() as f32 / max_len;
            let length_boost = length_normalized * config.length_weight;

            let combined_score = base_score + recency_boost + length_boost;
            (combined_score, r)
        })
        .collect();

    // Sort by combined score (descending)
    scored_results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Deduplicate: keep only one chunk per frame (the best scoring one)
    let mut seen_frames = std::collections::HashSet::new();
    let mut deduped: Vec<SemanticResult> = Vec::new();

    for (score, mut result) in scored_results {
        if seen_frames.contains(&result.frame.id) {
            continue;
        }
        seen_frames.insert(result.frame.id);
        // Update the score to the reranked score
        result.similarity_score = score;
        deduped.push(result);

        if deduped.len() >= config.top_k {
            break;
        }
    }

    debug!("Reranked to {} results", deduped.len());
    deduped
}

/// Simple keyword-based reranking boost
///
/// Boosts results that contain query keywords in the chunk text.
pub fn boost_keyword_matches(
    results: &mut [SemanticResult],
    query: &str,
    boost_factor: f32,
) {
    let keywords: Vec<&str> = query
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();

    for result in results.iter_mut() {
        let text_lower = result.chunk_text.to_lowercase();
        let mut matches = 0;

        for keyword in &keywords {
            if text_lower.contains(&keyword.to_lowercase()) {
                matches += 1;
            }
        }

        if matches > 0 {
            let boost = (matches as f32 / keywords.len().max(1) as f32) * boost_factor;
            result.similarity_score += boost;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use screensearch_db::FrameRecord;

    fn make_result(id: i64, score: f32, text: &str) -> SemanticResult {
        SemanticResult {
            frame: FrameRecord {
                id,
                timestamp: Utc::now(),
                image_path: String::new(),
                active_process: None,
                active_window: None,
                device_name: None,
                monitor_index: None,
            },
            chunk_text: text.to_string(),
            chunk_index: 0,
            similarity_score: score,
        }
    }

    #[test]
    fn test_rerank_empty() {
        let results: Vec<SemanticResult> = vec![];
        let config = RerankConfig::default();
        let reranked = rerank_results(results, &config);
        assert!(reranked.is_empty());
    }

    #[test]
    fn test_rerank_sorts_by_score() {
        let results = vec![
            make_result(1, 0.5, "low score"),
            make_result(2, 0.9, "high score"),
            make_result(3, 0.7, "medium score"),
        ];
        let config = RerankConfig { top_k: 10, ..Default::default() };
        let reranked = rerank_results(results, &config);
        
        assert_eq!(reranked.len(), 3);
        // Highest score first (with some tolerance for boosts)
        assert!(reranked[0].similarity_score >= reranked[1].similarity_score);
    }

    #[test]
    fn test_rerank_dedupes_frames() {
        let results = vec![
            make_result(1, 0.9, "first from frame 1"),
            make_result(1, 0.8, "second from frame 1"),
            make_result(2, 0.7, "from frame 2"),
        ];
        let config = RerankConfig::default();
        let reranked = rerank_results(results, &config);
        
        // Should only have 2 results (one per frame)
        assert_eq!(reranked.len(), 2);
    }
}
