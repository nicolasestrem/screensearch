//! In-process embedding + reranking engine backed by fastembed (ONNX Runtime).

use crate::chunker::TextChunker;
use crate::{EmbeddingConfig, EmbeddingError, Result, EMBEDDING_DIM};
use fastembed::{
    EmbeddingModel, RerankInitOptions, RerankerModel, TextEmbedding, TextInitOptions, TextRerank,
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// EmbeddingGemma retrieval prompt for queries (per the model card).
const GEMMA_QUERY_PREFIX: &str = "task: search result | query: ";
/// EmbeddingGemma retrieval prompt for documents (per the model card).
const GEMMA_DOCUMENT_PREFIX: &str = "title: none | text: ";

/// Point the dynamically-loaded ONNX Runtime at a copy of the shared library
/// shipped next to the executable, when one is present and the caller hasn't
/// already set `ORT_DYLIB_PATH`. If neither is set, `ort` falls back to its
/// default platform search (e.g. the system `PATH`).
pub(crate) fn configure_ort_dylib_path() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if std::env::var_os("ORT_DYLIB_PATH").is_some() {
            return;
        }
        let lib_name = if cfg!(windows) {
            "onnxruntime.dll"
        } else if cfg!(target_os = "macos") {
            "libonnxruntime.dylib"
        } else {
            "libonnxruntime.so"
        };
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join(lib_name);
                if candidate.exists() {
                    std::env::set_var("ORT_DYLIB_PATH", &candidate);
                }
            }
        }
    });
}

/// Result returned by the cross-encoder reranker.
#[derive(Debug, Clone)]
pub struct RerankScore {
    pub index: usize,
    pub score: f32,
}

/// In-process embedding engine.
///
/// The underlying ONNX models are not `Sync`, so they live behind a `Mutex`.
/// Inference is CPU-bound and runs on the blocking pool, serialized through the
/// mutex (which also avoids oversubscribing CPU cores from concurrent requests).
pub struct EmbeddingEngine {
    config: EmbeddingConfig,
    model: Arc<Mutex<TextEmbedding>>,
    reranker: Option<Arc<Mutex<TextRerank>>>,
    chunker: TextChunker,
}

impl EmbeddingEngine {
    /// Create an engine with default configuration (EmbeddingGemma-300M).
    pub async fn new() -> Result<Self> {
        Self::with_config(EmbeddingConfig::default()).await
    }

    /// Create an engine with explicit configuration. Loading (and, on first run,
    /// downloading) the model happens on the blocking pool.
    pub async fn with_config(config: EmbeddingConfig) -> Result<Self> {
        configure_ort_dylib_path();
        let load_config = config.clone();
        let (model, reranker) = tokio::task::spawn_blocking(move || {
            let mut init = TextInitOptions::new(EmbeddingModel::EmbeddingGemma300MQ)
                .with_show_download_progress(load_config.show_download_progress);
            if let Some(dir) = &load_config.cache_dir {
                init = init.with_cache_dir(dir.clone());
            }
            let model = TextEmbedding::try_new(init)
                .map_err(|error| EmbeddingError::ModelInitError(error.to_string()))?;

            let reranker = if load_config.reranker_enabled {
                let mut rinit = RerankInitOptions::new(RerankerModel::BGERerankerV2M3)
                    .with_show_download_progress(load_config.show_download_progress);
                if let Some(dir) = &load_config.cache_dir {
                    rinit = rinit.with_cache_dir(dir.clone());
                }
                let reranker = TextRerank::try_new(rinit)
                    .map_err(|error| EmbeddingError::ModelInitError(error.to_string()))?;
                Some(Arc::new(Mutex::new(reranker)))
            } else {
                None
            };

            Ok::<_, EmbeddingError>((Arc::new(Mutex::new(model)), reranker))
        })
        .await
        .map_err(|error| EmbeddingError::ModelInitError(error.to_string()))??;

        info!(
            model = %config.model,
            reranker_enabled = config.reranker_enabled,
            "Initialized in-process embedding engine"
        );

        Ok(Self {
            config,
            model,
            reranker,
            chunker: TextChunker::default(),
        })
    }

    /// Embed a single query string (uses the EmbeddingGemma query prompt).
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let prompted = format!("{GEMMA_QUERY_PREFIX}{text}");
        let mut embeddings = self.run_embed(vec![prompted]).await?;
        embeddings
            .pop()
            .ok_or_else(|| EmbeddingError::InferenceError("model returned no embedding".into()))
    }

    /// Embed a batch of documents (uses the EmbeddingGemma document prompt).
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let non_empty: Vec<String> = texts
            .iter()
            .copied()
            .filter(|text| !text.trim().is_empty())
            .map(|text| format!("{GEMMA_DOCUMENT_PREFIX}{text}"))
            .collect();
        if non_empty.len() != texts.len() {
            warn!(
                "Ignoring {} empty text(s) in embedding batch",
                texts.len() - non_empty.len()
            );
        }
        if non_empty.is_empty() {
            return Ok(Vec::new());
        }
        self.run_embed(non_empty).await
    }

    async fn run_embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let model = Arc::clone(&self.model);
        let embeddings = tokio::task::spawn_blocking(move || {
            let mut guard = model.lock().map_err(|_| {
                EmbeddingError::InferenceError("embedding model lock poisoned".into())
            })?;
            // Embed one input at a time. The quantized EmbeddingGemma model
            // (`EmbeddingGemma300MQ`) errors on any multi-input `embed` call —
            // "Dynamic quantization cannot be used with batching" — which
            // previously aborted the whole batch and left long-OCR frames (and
            // the on-demand /embeddings/generate endpoint) permanently
            // un-indexed. A batch of one avoids the dynamic-quant batch path; the
            // model lock is acquired once for the whole loop. (`config.batch_size`
            // no longer applies to this quantized model.)
            let mut out: Vec<Vec<f32>> = Vec::with_capacity(texts.len());
            for text in &texts {
                let mut one = guard
                    .embed(vec![text.as_str()], Some(1))
                    .map_err(|error| EmbeddingError::InferenceError(error.to_string()))?;
                let embedding = one.pop().ok_or_else(|| {
                    EmbeddingError::InferenceError("model returned no embedding".into())
                })?;
                out.push(embedding);
            }
            Ok::<Vec<Vec<f32>>, EmbeddingError>(out)
        })
        .await
        .map_err(|error| EmbeddingError::InferenceError(error.to_string()))??;

        for embedding in &embeddings {
            if embedding.len() != EMBEDDING_DIM {
                return Err(EmbeddingError::InferenceError(format!(
                    "expected {EMBEDDING_DIM}-dim embeddings, got {}",
                    embedding.len()
                )));
            }
        }
        Ok(embeddings)
    }

    /// Rerank `documents` against `query` with the cross-encoder, returning
    /// scores sorted descending. If the reranker is disabled, returns the
    /// documents in their original order with descending placeholder scores so
    /// callers can fall back to fusion order transparently.
    pub async fn rerank(&self, query: &str, documents: &[String]) -> Result<Vec<RerankScore>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let Some(reranker) = self.reranker.clone() else {
            // Reranker disabled: preserve incoming (RRF) order.
            return Ok(documents
                .iter()
                .enumerate()
                .map(|(index, _)| RerankScore {
                    index,
                    score: 1.0 - (index as f32) / (documents.len() as f32),
                })
                .collect());
        };

        let query = query.to_string();
        let docs = documents.to_vec();
        let expected = documents.len();
        let mut scores = tokio::task::spawn_blocking(move || {
            let mut guard = reranker
                .lock()
                .map_err(|_| EmbeddingError::InferenceError("reranker lock poisoned".into()))?;
            // `query` and `documents` share the generic `S`; use `&str` for both.
            let refs: Vec<&str> = docs.iter().map(String::as_str).collect();
            let results = guard
                .rerank(query.as_str(), &refs, false, None)
                .map_err(|error| EmbeddingError::InferenceError(error.to_string()))?;
            Ok::<_, EmbeddingError>(
                results
                    .into_iter()
                    .map(|r| RerankScore {
                        index: r.index,
                        score: r.score,
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .await
        .map_err(|error| EmbeddingError::InferenceError(error.to_string()))??;

        if scores.len() != expected {
            return Err(EmbeddingError::InferenceError(format!(
                "reranker returned {} scores for {expected} documents",
                scores.len()
            )));
        }
        scores.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(scores)
    }

    /// Split text into overlapping chunks (token-approximate) for embedding.
    /// Signature preserved from the previous sidecar client for caller parity.
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
        let chunker = TextChunker::new(max_tokens, overlap_tokens);
        Ok(chunker
            .chunk_text(text)
            .into_iter()
            .filter(|chunk| !chunk.trim().is_empty())
            .collect())
    }

    /// Stable content hash used to deduplicate chunks.
    pub fn content_hash(text: &str) -> String {
        format!("{:x}", Sha256::digest(text.as_bytes()))
    }

    pub fn provider(&self) -> &'static str {
        "fastembed"
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

    /// Whether the cross-encoder reranker is loaded.
    pub fn reranker_enabled(&self) -> bool {
        self.reranker.is_some()
    }

    /// Reference to the configured chunker (default settings).
    pub fn chunker(&self) -> &TextChunker {
        &self.chunker
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
}
