//! Vector similarity search for RAG
//!
//! Provides sqlite-vec KNN retrieval and rank-based hybrid fusion.

use crate::{DatabaseManager, Result, SemanticResult};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Vector index for fast similarity search
pub struct VectorIndex {
    /// Map of embedding_id to (frame_id, chunk_index, vector)
    vectors: HashMap<i64, (i64, i32, Vec<f32>)>,
    /// Dimension of vectors
    dimension: usize,
}

impl VectorIndex {
    /// Create a new empty vector index
    pub fn new(dimension: usize) -> Self {
        Self {
            vectors: HashMap::new(),
            dimension,
        }
    }

    /// Add a vector to the index
    pub fn add(&mut self, embedding_id: i64, frame_id: i64, chunk_index: i32, vector: Vec<f32>) {
        if vector.len() != self.dimension {
            tracing::warn!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimension,
                vector.len()
            );
            return;
        }
        self.vectors
            .insert(embedding_id, (frame_id, chunk_index, vector));
    }

    /// Find K nearest neighbors using cosine similarity
    pub fn search_knn(&self, query: &[f32], k: usize) -> Vec<(i64, i64, i32, f32)> {
        if query.len() != self.dimension {
            tracing::error!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            );
            return Vec::new();
        }

        let mut scores: Vec<(i64, i64, i32, f32)> = self
            .vectors
            .iter()
            .map(|(embedding_id, (frame_id, chunk_index, vector))| {
                let similarity = cosine_similarity(query, vector);
                (*embedding_id, *frame_id, *chunk_index, similarity)
            })
            .collect();

        // Sort by similarity (descending)
        scores.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));

        // Take top K
        scores.truncate(k);
        scores
    }

    /// Get the number of vectors in the index
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

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
    /// Legacy utility retained for API compatibility.
    pub async fn build_vector_index(&self) -> Result<VectorIndex> {
        let index = VectorIndex::new(1024);

        tracing::info!("Building vector index from database...");

        tracing::info!("Vector index built with {} vectors", index.len());
        Ok(index)
    }

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

    /// Hybrid search combining FTS5 and vector similarity
    pub async fn hybrid_search(
        &self,
        query: &str,
        query_embedding: Vec<f32>,
        _alpha: f32,
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
            for (idx, match_item) in fts.ocr_matches.into_iter().enumerate() {
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
                        chunk_index: idx as i32,
                        similarity_score: score_boost,
                        retrieval_source: "fts".to_string(),
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

    #[test]
    fn test_vector_index() {
        let mut index = VectorIndex::new(3);
        assert!(index.is_empty());

        index.add(1, 100, 0, vec![1.0, 0.0, 0.0]);
        index.add(2, 101, 0, vec![0.0, 1.0, 0.0]);
        index.add(3, 102, 0, vec![0.5, 0.5, 0.0]);

        assert_eq!(index.len(), 3);

        let query = vec![1.0, 0.0, 0.0];
        let results = index.search_knn(&query, 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1, 100); // First result should be exact match
        assert!((results[0].3 - 1.0).abs() < 0.001);
    }
}
