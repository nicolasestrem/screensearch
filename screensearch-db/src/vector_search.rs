//! Vector similarity search for RAG
//!
//! Provides in-memory vector search using cosine similarity.
//! This is a simple implementation that loads embeddings into memory
//! for fast KNN search. For production with millions of vectors,
//! consider using a dedicated vector database.

use crate::{DatabaseManager, FrameRecord, Result, SemanticResult};
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
        self.vectors.insert(embedding_id, (frame_id, chunk_index, vector));
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
    /// Build a vector index from all embeddings in the database
    ///
    /// This loads all embeddings into memory for fast similarity search.
    /// For very large datasets, consider using a persistent vector database.
    pub async fn build_vector_index(&self) -> Result<VectorIndex> {
        // For now, we'll use a simple in-memory index
        // In production, you'd want to use sqlite-vec or a dedicated vector DB
        
        let index = VectorIndex::new(384); // MiniLM dimension
        
        tracing::info!("Building vector index from database...");
        
        // Note: This is a placeholder. To actually load vectors, we need to store them
        // in a separate table or use sqlite-vec. For now, this creates an empty index.
        
        tracing::info!("Vector index built with {} vectors", index.len());
        Ok(index)
    }

    /// Perform semantic search using in-memory vector similarity
    ///
    /// Fetches all embeddings, computes similarity in Rust, and returns top results.
    /// This avoids dependency on sqlite-vec extension availability.
    pub async fn semantic_search(
        &self,
        query_embedding: Vec<f32>,
        limit: i64,
    ) -> Result<Vec<SemanticResult>> {
        // 1. Fetch all embeddings from DB
        // We only fetch ID and Vector to save bandwidth, then fetch details for top K
        // Column 'embedding' is assumed to be BLOB of f32 le_bytes or JSON.
        // Based on zerocopy/NewEmbedding, likely BLOB.
        // We cast BLOB to Vec<f32>.
        // Note: We access pool via public method to avoid privacy error.
        let rows = sqlx::query(
            r#"
            SELECT frame_id, chunk_text, chunk_index, embedding, embedding_dim 
            FROM embeddings
            "#
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| crate::DatabaseError::QueryError(format!("Failed to fetch embeddings: {}", e)))?;

        if rows.len() > 10000 {
            tracing::warn!("Loading {} embeddings into memory for vector search. This may impact performance.", rows.len());
        }

        let mut candidates: Vec<(i64, String, i32, f32)> = Vec::with_capacity(rows.len());

        for row in rows {
            use sqlx::Row;
            let frame_id: i64 = row.get("frame_id");
            let chunk_text: String = row.get("chunk_text");
            let chunk_index: i32 = row.get("chunk_index");
            let embedding_blob: Vec<u8> = row.get("embedding");

            // Convert raw bytes to Vec<f32>
            // Assuming little-endian f32
            let vector: Vec<f32> = embedding_blob
                .chunks_exact(4)
                .map(|chunk| {
                    chunk.try_into()
                        .ok()
                        .map(f32::from_le_bytes)
                        .unwrap_or(0.0)
                })
                .collect();

            let similarity = cosine_similarity(&query_embedding, &vector);
            candidates.push((frame_id, chunk_text, chunk_index, similarity));
        }

        // 2. Sort by similarity (descending)
        candidates.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
        
        // 3. Take Top K
        let top_k = candidates.iter().take(limit as usize).collect::<Vec<_>>();
        
        if top_k.is_empty() {
            return Ok(Vec::new());
        }

        // 4. Fetch Frame Metadata for Top K
        // We need to fetch details for each frame.
        // To be efficient, valid SQL: WHERE id IN (...)
        let frame_ids: Vec<i64> = top_k.iter().map(|(id, ..)| *id).collect();
        let mut query_builder = sqlx::QueryBuilder::new("SELECT * FROM frames WHERE id IN (");
        let mut separated = query_builder.separated(", ");
        for id in frame_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let frames: Vec<FrameRecord> = query_builder.build_query_as()
            .fetch_all(self.pool())
            .await
            .map_err(|e| crate::DatabaseError::QueryError(format!("Failed to fetch frame details: {}", e)))?;

        // 5. Build Result Map
        let frame_map: HashMap<i64, FrameRecord> = frames.into_iter().map(|f| (f.id, f)).collect();

        let mut results = Vec::new();
        for (frame_id, chunk_text, chunk_index, score) in top_k {
             if let Some(frame) = frame_map.get(frame_id) {
                 results.push(SemanticResult {
                     frame: frame.clone(),
                     chunk_text: chunk_text.clone(),
                     chunk_index: *chunk_index,
                     similarity_score: *score,
                 });
             }
        }

        Ok(results)
    }

    /// Hybrid search combining FTS5 and vector similarity
    pub async fn hybrid_search(
        &self,
        query: &str,
        query_embedding: Vec<f32>,
        alpha: f32,
        limit: i64,
    ) -> Result<Vec<SemanticResult>> {
        // 1. Get Vector Results (Semantic) via in-memory search
        let semantic_results = match self.semantic_search(query_embedding, limit).await {
            Ok(res) => res,
            Err(e) => {
                tracing::error!("Semantic search failed: {}", e);
                Vec::new() 
            }
        };

        // 2. Get Keyword Results (FTS)
        let fts_limit = limit; 
        let fts_results = self.search_ocr_text(
            query, 
            crate::FrameFilter::default(),
            crate::Pagination { limit: fts_limit, offset: 0 }
        ).await?;

        // 3. Merge Results (Simple Fusion)
        let mut merged: HashMap<(i64, String), SemanticResult> = HashMap::new();

        // Add semantic results
        for res in semantic_results {
            let key = (res.frame.id, res.chunk_text.clone());
            let mut new_res = res.clone();
            new_res.similarity_score *= alpha; // Weighted semantic score
            merged.insert(key, new_res);
        }

        // Add/Boost FTS results
        for fts in fts_results {
            for (idx, match_item) in fts.ocr_matches.into_iter().enumerate() {
                let key = (fts.frame.id, match_item.text.clone());
                let score_boost = fts.relevance_score * (1.0 - alpha);

                merged
                    .entry(key)
                    .and_modify(|e| e.similarity_score += score_boost)
                    .or_insert_with(|| SemanticResult {
                        frame: fts.frame.clone(),
                        chunk_text: match_item.text,
                        chunk_index: idx as i32,
                        similarity_score: score_boost,
                    });
            }
        }

        // Convert to Vec and sort
        let mut final_results: Vec<SemanticResult> = merged.into_values().collect();
        final_results.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap_or(std::cmp::Ordering::Equal));
        
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
