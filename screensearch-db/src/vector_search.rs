//! Vector similarity search for RAG
//!
//! Provides sqlite-vec KNN retrieval and rank-based hybrid fusion.

use crate::{DatabaseManager, Result, SemanticResult};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Compute cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

impl DatabaseManager {
    /// Perform semantic KNN search using the persistent sqlite-vec index.
    pub async fn semantic_search(
        &self,
        query_embedding: Vec<f32>,
        limit: i64,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<SemanticResult>> {
        self.search_embeddings_with_time_range(
            query_embedding,
            limit.max(0) as usize,
            0.0,
            Some(start_time),
            Some(end_time),
        )
        .await
    }

    /// Hybrid search combining FTS5 and vector similarity.
    ///
    /// When `image_query_embedding` is provided (a nomic-text vector aligned with
    /// the image index), image-embedding hits are fused in as a third ranked list
    /// via the same Reciprocal Rank Fusion, giving non-OCR visual content a path
    /// into the results. Pass `None` to skip the image index entirely.
    pub async fn hybrid_search(
        &self,
        query: &str,
        query_embedding: Vec<f32>,
        image_query_embedding: Option<Vec<f32>>,
        limit: i64,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<SemanticResult>> {
        const RRF_K: f32 = 60.0;
        let candidate_limit = limit.saturating_mul(4);

        let semantic_results = match self
            .semantic_search(query_embedding, candidate_limit, start_time, end_time)
            .await
        {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("Semantic search failed: {}", e);
                Vec::new()
            }
        };

        let fts_results = self
            .search_ocr_text(
                query,
                crate::FrameFilter {
                    start_time: Some(start_time),
                    end_time: Some(end_time),
                    ..Default::default()
                },
                crate::Pagination {
                    limit: candidate_limit,
                    offset: 0,
                },
            )
            .await?;

        let mut merged: HashMap<(i64, String), SemanticResult> = HashMap::new();

        for (rank, res) in semantic_results.into_iter().enumerate() {
            let key = (res.frame.id, res.chunk_text.clone());
            let mut new_res = res.clone();
            new_res.similarity_score = 1.0 / (RRF_K + rank as f32 + 1.0);
            new_res.retrieval_source = "vector".to_string();
            merged.insert(key, new_res);
        }

        for (rank, fts) in fts_results.into_iter().enumerate() {
            for match_item in fts.ocr_matches.into_iter() {
                let key = (fts.frame.id, match_item.text.clone());
                let score_boost = 1.0 / (RRF_K + rank as f32 + 1.0);

                merged
                    .entry(key)
                    .and_modify(|result| {
                        result.similarity_score += score_boost;
                        result.retrieval_source = "hybrid".to_string();
                    })
                    .or_insert_with(|| SemanticResult {
                        frame: fts.frame.clone(),
                        chunk_text: match_item.text,
                        // FTS matches don't map to embedding chunk boundaries, so
                        // the real chunk_index is unknown for FTS-only hits. Use
                        // -1 as a sentinel rather than the FTS match position.
                        chunk_index: -1,
                        similarity_score: score_boost,
                        retrieval_source: "fts".to_string(),
                    });
            }
        }

        // Optional third list: image-embedding hits (queried with the aligned
        // nomic-text vector), fused with the same RRF formula. Image vectors live
        // in a separate space, so they come only from the image index.
        if let Some(image_embedding) = image_query_embedding {
            let image_results = match self
                .search_image_embeddings_with_time_range(
                    image_embedding,
                    candidate_limit.max(0) as usize,
                    0.0,
                    Some(start_time),
                    Some(end_time),
                )
                .await
            {
                Ok(res) => res,
                Err(e) => {
                    tracing::error!("Image search failed: {}", e);
                    Vec::new()
                }
            };

            for (rank, res) in image_results.into_iter().enumerate() {
                let key = (res.frame.id, res.chunk_text.clone());
                let score_boost = 1.0 / (RRF_K + rank as f32 + 1.0);
                merged
                    .entry(key)
                    .and_modify(|result| {
                        result.similarity_score += score_boost;
                        result.retrieval_source = "hybrid".to_string();
                    })
                    .or_insert_with(|| SemanticResult {
                        similarity_score: score_boost,
                        retrieval_source: "image".to_string(),
                        ..res
                    });
            }
        }

        // Convert to Vec and sort
        let mut final_results: Vec<SemanticResult> = merged.into_values().collect();
        final_results.sort_by(|a, b| {
            b.similarity_score
                .partial_cmp(&a.similarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if final_results.len() > limit as usize {
            final_results.truncate(limit as usize);
        }

        Ok(final_results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 0.001);

        let d = vec![0.5, 0.5, 0.0];
        let sim = cosine_similarity(&a, &d);
        assert!(sim > 0.0 && sim < 1.0);
    }
}
