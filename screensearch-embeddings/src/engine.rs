//! Quality sidecar client for embeddings and reranking.

use crate::{EmbeddingConfig, EmbeddingError, Result, EMBEDDING_DIM};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tracing::warn;

#[derive(Debug, Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    texts: &'a [&'a str],
    task: &'a str,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
    model: String,
    version: String,
    dimension: usize,
}

fn validate_embedding_response(
    payload: &EmbeddingResponse,
    expected_model: &str,
    expected_version: &str,
    expected_count: usize,
) -> Result<()> {
    // Model identity is part of the fixed persisted-vector contract. Accepting
    // a compatible-looking variant here could mix vectors from two models.
    if payload.model != expected_model
        || payload.version != expected_version
        || payload.dimension != EMBEDDING_DIM
        || payload.embeddings.len() != expected_count
        || payload
            .embeddings
            .iter()
            .any(|embedding| embedding.len() != EMBEDDING_DIM)
    {
        return Err(EmbeddingError::InferenceError(format!(
            "unexpected embedding model response: model={}, version={}, dimension={}, count={}/{}",
            payload.model,
            payload.version,
            payload.dimension,
            payload.embeddings.len(),
            expected_count
        )));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct RerankRequest<'a> {
    model: &'a str,
    query: &'a str,
    documents: &'a [String],
}

#[derive(Debug, Deserialize)]
struct RerankResponse {
    scores: Vec<f32>,
}

#[derive(Debug, Serialize)]
struct ChunkRequest<'a> {
    model: &'a str,
    text: &'a str,
    max_tokens: usize,
    overlap_tokens: usize,
}

#[derive(Debug, Deserialize)]
struct ChunkResponse {
    chunks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPreparationStatus {
    pub state: String,
    pub current_component: Option<String>,
    pub ready_components: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
struct ModelPreparationRequest<'a> {
    components: &'a [&'a str],
    ocr_language: &'a str,
}

/// Result returned by the quality reranker.
#[derive(Debug, Clone)]
pub struct RerankScore {
    pub index: usize,
    pub score: f32,
}

/// Client for the local quality sidecar.
pub struct EmbeddingEngine {
    config: EmbeddingConfig,
    client: Client,
}

impl EmbeddingEngine {
    pub async fn new() -> Result<Self> {
        Self::with_config(EmbeddingConfig::default()).await
    }

    pub async fn with_config(config: EmbeddingConfig) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|error| EmbeddingError::ModelInitError(error.to_string()))?;

        Ok(Self { config, client })
    }

    pub async fn health_check(&self) -> Result<()> {
        let request = self.authorized(self.client.get(format!(
            "{}/health",
            self.config.sidecar_url.trim_end_matches('/')
        )));
        let response = request
            .send()
            .await
            .map_err(|error| EmbeddingError::SidecarUnavailable(error.to_string()))?;
        if !response.status().is_success() {
            return Err(EmbeddingError::SidecarUnavailable(format!(
                "health check returned {}",
                response.status()
            )));
        }
        Ok(())
    }

    pub async fn model_preparation_status(&self) -> Result<ModelPreparationStatus> {
        let response = self
            .authorized(self.client.get(format!(
                "{}/v1/models/status",
                self.config.sidecar_url.trim_end_matches('/')
            )))
            .send()
            .await
            .map_err(|error| EmbeddingError::SidecarUnavailable(error.to_string()))?;
        self.parse_model_preparation_response(response).await
    }

    pub async fn prepare_models(&self) -> Result<ModelPreparationStatus> {
        let response = self
            .authorized(self.client.post(format!(
                "{}/v1/models/prepare",
                self.config.sidecar_url.trim_end_matches('/')
            )))
            .json(&ModelPreparationRequest {
                components: &["ocr", "embeddings", "reranker"],
                ocr_language: "en",
            })
            .send()
            .await
            .map_err(|error| EmbeddingError::SidecarUnavailable(error.to_string()))?;
        self.parse_model_preparation_response(response).await
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut embeddings = self.embed_texts(&[text], "query").await?;
        embeddings
            .pop()
            .ok_or_else(|| EmbeddingError::InferenceError("sidecar returned no embedding".into()))
    }

    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        self.embed_texts(texts, "document").await
    }

    async fn embed_texts(&self, texts: &[&str], task: &str) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let non_empty_texts: Vec<&str> = texts
            .iter()
            .copied()
            .filter(|text| !text.trim().is_empty())
            .collect();
        if non_empty_texts.len() != texts.len() {
            warn!(
                "Ignoring {} empty text(s) in embedding batch",
                texts.len() - non_empty_texts.len()
            );
        }
        if non_empty_texts.is_empty() {
            return Ok(Vec::new());
        }

        let request = EmbeddingRequest {
            model: &self.config.model,
            texts: &non_empty_texts,
            task,
        };
        let response = self
            .authorized(self.client.post(format!(
                "{}/v1/embeddings",
                self.config.sidecar_url.trim_end_matches('/')
            )))
            .json(&request)
            .send()
            .await
            .map_err(|error| EmbeddingError::SidecarUnavailable(error.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::InferenceError(format!(
                "embedding request failed ({status}): {body}"
            )));
        }

        let payload: EmbeddingResponse = response
            .json()
            .await
            .map_err(|error| EmbeddingError::InferenceError(error.to_string()))?;
        validate_embedding_response(
            &payload,
            &self.config.model,
            &self.config.model_version,
            non_empty_texts.len(),
        )?;

        Ok(payload.embeddings)
    }

    pub async fn rerank(&self, query: &str, documents: &[String]) -> Result<Vec<RerankScore>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let response = self
            .authorized(self.client.post(format!(
                "{}/v1/rerank",
                self.config.sidecar_url.trim_end_matches('/')
            )))
            .json(&RerankRequest {
                model: &self.config.reranker_model,
                query,
                documents,
            })
            .send()
            .await
            .map_err(|error| EmbeddingError::SidecarUnavailable(error.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::InferenceError(format!(
                "rerank request failed ({status}): {body}"
            )));
        }

        let payload: RerankResponse = response
            .json()
            .await
            .map_err(|error| EmbeddingError::InferenceError(error.to_string()))?;
        if payload.scores.len() != documents.len() {
            return Err(EmbeddingError::InferenceError(format!(
                "reranker returned {} scores for {} documents",
                payload.scores.len(),
                documents.len()
            )));
        }

        let mut scores: Vec<RerankScore> = payload
            .scores
            .into_iter()
            .enumerate()
            .map(|(index, score)| RerankScore { index, score })
            .collect();
        scores.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(scores)
    }

    pub async fn chunk_text(
        &self,
        text: &str,
        max_tokens: usize,
        overlap_tokens: usize,
    ) -> Result<Vec<String>> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        if max_tokens == 0 || overlap_tokens >= max_tokens {
            return Err(EmbeddingError::TokenizationError(
                "chunk overlap must be smaller than the maximum chunk size".to_string(),
            ));
        }

        let response = self
            .authorized(self.client.post(format!(
                "{}/v1/chunk",
                self.config.sidecar_url.trim_end_matches('/')
            )))
            .json(&ChunkRequest {
                model: &self.config.model,
                text,
                max_tokens,
                overlap_tokens,
            })
            .send()
            .await
            .map_err(|error| EmbeddingError::SidecarUnavailable(error.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::TokenizationError(format!(
                "chunking request failed ({status}): {body}"
            )));
        }
        let chunks = response
            .json::<ChunkResponse>()
            .await
            .map_err(|error| EmbeddingError::TokenizationError(error.to_string()))?
            .chunks;
        let non_empty_chunks: Vec<String> = chunks
            .into_iter()
            .filter(|chunk| !chunk.trim().is_empty())
            .collect();
        Ok(non_empty_chunks)
    }

    pub fn content_hash(text: &str) -> String {
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    pub fn provider(&self) -> &'static str {
        "quality-sidecar"
    }

    pub fn model(&self) -> &str {
        &self.config.model
    }

    pub fn model_version(&self) -> &str {
        &self.config.model_version
    }

    pub fn config(&self) -> &EmbeddingConfig {
        &self.config
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.config.sidecar_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    async fn parse_model_preparation_response(
        &self,
        response: reqwest::Response,
    ) -> Result<ModelPreparationStatus> {
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::ModelInitError(format!(
                "model preparation request failed ({status}): {body}"
            )));
        }
        response
            .json()
            .await
            .map_err(|error| EmbeddingError::ModelInitError(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_stable() {
        assert_eq!(
            EmbeddingEngine::content_hash("screen search"),
            EmbeddingEngine::content_hash("screen search")
        );
        assert_ne!(
            EmbeddingEngine::content_hash("screen search"),
            EmbeddingEngine::content_hash("screen-search")
        );
    }

    #[test]
    fn embedding_response_rejects_truncated_batches() {
        let payload = EmbeddingResponse {
            embeddings: vec![vec![0.0; EMBEDDING_DIM]],
            model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
            version: "main".to_string(),
            dimension: EMBEDDING_DIM,
        };

        assert!(
            validate_embedding_response(&payload, "Qwen/Qwen3-Embedding-0.6B", "main", 2).is_err()
        );
    }
}
