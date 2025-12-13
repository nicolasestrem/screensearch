//! Embedding Engine implementation
//!
//! Provides the main EmbeddingEngine struct for generating text embeddings
//! using ONNX Runtime and the multilingual MiniLM model.

use crate::{EmbeddingConfig, EmbeddingError, Result, EMBEDDING_DIM};
use ndarray::Array2;
use ort::{
    GraphOptimizationLevel, Session, Value,
};

use std::sync::Mutex; // Added Mutex
use tokenizers::Tokenizer;
use tracing::{info, warn};
use std::path::PathBuf;

/// Main embedding engine for generating text embeddings
///
/// Uses ONNX Runtime for efficient inference with the
/// paraphrase-multilingual-MiniLM-L12-v2 model.
pub struct EmbeddingEngine {
    config: EmbeddingConfig,
    session: Option<Mutex<Session>>, // Wrapped in Mutex
    tokenizer: Option<Tokenizer>,
}

// Note: We use Mutex<Session> which should be Send, and Tokenizer is Send/Sync.
// We rely on automatic trait derivation rather than unsafe manual implementation.

impl EmbeddingEngine {
    /// Create a new embedding engine
    pub async fn new() -> Result<Self> {
        Self::with_config(EmbeddingConfig::default()).await
    }

    /// Create an embedding engine with custom configuration
    pub async fn with_config(config: EmbeddingConfig) -> Result<Self> {
        info!("Initializing embedding engine...");

        // Determine models directory
        let models_dir = crate::download::get_models_dir();
        
        // Auto-download if needed
        if crate::download::needs_download(&models_dir) {
            info!("Model not found locally. Downloading from HuggingFace...");
            // Use retry logic or just improved download implementation
            if let Err(e) = crate::download::download_model(&models_dir).await {
                warn!("Failed to download model: {}. Running in fallback mode.", e);
                return Ok(Self {
                    config,
                    session: None,
                    tokenizer: None,
                });
            }
        }

        let (default_model, default_tokenizer) = crate::download::get_model_paths(&models_dir);
        
        let model_path = config.model_path.as_ref()
            .map(PathBuf::from)
            .unwrap_or(default_model);

        let tokenizer_path = config.tokenizer_path.as_ref()
            .map(PathBuf::from)
            .unwrap_or(default_tokenizer);

        if !model_path.exists() || !tokenizer_path.exists() {
            warn!("Model files missing. Running in fallback mode.");
            return Ok(Self {
                config,
                session: None,
                tokenizer: None,
            });
        }

        info!("Loading tokenizer from {:?}", tokenizer_path);
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EmbeddingError::ModelInitError(format!("Failed to load tokenizer: {}", e)))?;

        info!("Loading ONNX model from {:?}", model_path);
        let session = Session::builder()
            .map_err(|e| EmbeddingError::ModelInitError(format!("Failed to create session builder: {}", e)))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| EmbeddingError::ModelInitError(format!("Failed to set opt level: {}", e)))?
            .with_intra_threads(4)
            .map_err(|e| EmbeddingError::ModelInitError(format!("Failed to set threads: {}", e)))?
            .with_model_from_file(&model_path)
            .map_err(|e| EmbeddingError::ModelInitError(format!("Failed to load model: {}", e)))?;

        info!("Embedding engine initialized successfully!");

        Ok(Self {
            config,
            session: Some(Mutex::new(session)),
            tokenizer: Some(tokenizer),
        })
    }

    /// Check if the engine is fully initialized with a loaded model
    pub fn is_initialized(&self) -> bool {
        self.session.is_some() && self.tokenizer.is_some()
    }

    /// Generate embedding for a single text
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        if text.is_empty() {
            return Ok(vec![0.0; EMBEDDING_DIM]);
        }
        self.embed_batch(&[text]).map(|v| v.into_iter().next().unwrap())
    }

    /// Generate embeddings for multiple texts (batch processing)
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let (session_mutex, tokenizer) = match (self.session.as_ref(), self.tokenizer.as_ref()) {
            (Some(s), Some(t)) => (s, t),
            _ => {
                warn!("Embedding engine not fully initialized. Using fallback hash embeddings.");
                return Ok(texts.iter().map(|t| self.fallback_embed(t)).collect());
            }
        };

        // 1. Tokenize
        let encodings = tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| EmbeddingError::InferenceError(format!("Tokenization failed: {}", e)))?;

        let batch_size = texts.len();
        let seq_len = encodings[0].len(); // Assuming padding makes them equal length
        
        let mut input_ids = Vec::with_capacity(batch_size * seq_len);
        let mut attention_mask = Vec::with_capacity(batch_size * seq_len);
        let mut token_type_ids = Vec::with_capacity(batch_size * seq_len);

        for encoding in &encodings {
            input_ids.extend(encoding.get_ids().iter().map(|&x| x as i64));
            attention_mask.extend(encoding.get_attention_mask().iter().map(|&x| x as i64));
            token_type_ids.extend(encoding.get_type_ids().iter().map(|&x| x as i64));
        }

        let input_ids_array = Array2::from_shape_vec((batch_size, seq_len), input_ids)
            .map_err(|e| EmbeddingError::InferenceError(e.to_string()))?;
        let attention_mask_array = Array2::from_shape_vec((batch_size, seq_len), attention_mask)
            .map_err(|e| EmbeddingError::InferenceError(e.to_string()))?;
        let token_type_ids_array = Array2::from_shape_vec((batch_size, seq_len), token_type_ids)
            .map_err(|e| EmbeddingError::InferenceError(e.to_string()))?;

        // 2. Prepare Inputs
        // ort usually requires creating Value separately if standard traits don't match or for explicit control


        let inputs = ort::inputs![
            "input_ids" => Value::from_array(input_ids_array).map_err(|e| EmbeddingError::InferenceError(format!("Input error: {}", e)))?,
            "attention_mask" => Value::from_array(attention_mask_array.clone()).map_err(|e| EmbeddingError::InferenceError(format!("Input error: {}", e)))?,
            "token_type_ids" => Value::from_array(token_type_ids_array).map_err(|e| EmbeddingError::InferenceError(format!("Input error: {}", e)))?
        ].map_err(|e| EmbeddingError::InferenceError(format!("Failed to create inputs: {}", e)))?;

        // 3. Run Inference
        // Lock session for mutable access
        let session = session_mutex.lock().map_err(|_| EmbeddingError::InferenceError("Failed to acquire session lock".to_string()))?;
        
        let outputs = session
            .run(inputs)
            .map_err(|e| EmbeddingError::InferenceError(format!("Inference failed: {}", e)))?;

        // 4. Mean Pooling & Normalization
        // Output usually "last_hidden_state" (batch, seq, hidden_size)
        // Or if simple model, maybe just "last_hidden_state"
        let output_tensor = outputs.get("last_hidden_state").ok_or_else(|| {
             EmbeddingError::InferenceError("Model output 'last_hidden_state' not found".to_string())
        })?;
        
        let embeddings_view = output_tensor.extract_tensor::<f32>()
            .map_err(|e| EmbeddingError::InferenceError(format!("Failed to extract tensor: {}", e)))?;
        
        // Create view to extend lifetime
        let embeddings_data = embeddings_view.view();
        
        // Dimensions: [batch, seq, hidden]
        let shape = embeddings_data.shape();
        if shape.len() != 3 {
             return Err(EmbeddingError::InferenceError(format!("Unexpected output shape: {:?}", shape)));
        }
        let hidden_dim = shape[2];
        
        // Manual Mean Pooling
        let mut final_embeddings = Vec::with_capacity(batch_size);
        
        // We use the attention mask to average only real tokens
        let mask = attention_mask_array; // (batch, seq)
        
        // Iterate over batch
        for i in 0..batch_size {
            let mut sum_vec = vec![0.0f32; hidden_dim];
            let mut count = 0.0f32;
            
            for j in 0..seq_len {
                 if mask[[i, j]] == 1 {
                     let token_emb = embeddings_data.slice(ndarray::s![i, j, ..]);
                     for (k, &val) in token_emb.iter().enumerate() {
                         sum_vec[k] += val;
                     }
                     count += 1.0;
                 }
            }
            
            // Average
            if count > 0.0 {
                for x in sum_vec.iter_mut() {
                    *x /= count;
                }
            }
            
            // Normalize
            let norm: f32 = sum_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 1e-9 {
                for x in sum_vec.iter_mut() {
                    *x /= norm;
                }
            }
            
            final_embeddings.push(sum_vec);
        }

        Ok(final_embeddings)
    }

    /// Compute cosine similarity between two embeddings
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

    /// Fallback embedding using simple hashing (deprecated but kept for fallback)
    fn fallback_embed(&self, text: &str) -> Vec<f32> {
        let mut embedding = vec![0.0f32; EMBEDDING_DIM];
        for (i, ch) in text.chars().enumerate() {
            let idx = (ch as usize + i * 7) % EMBEDDING_DIM;
            embedding[idx] += 1.0;
        }
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in embedding.iter_mut() { *x /= norm; }
        }
        embedding
    }

    pub fn config(&self) -> &EmbeddingConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fallback_when_model_missing() {
        // This test runs without model download, so should default to fallback
        // We force a non-existent path to ensure
        let mut config = EmbeddingConfig::default();
        config.model_path = Some("non_existent_folder".to_string());
        let engine = EmbeddingEngine::with_config(config).await.unwrap();
        assert!(!engine.is_initialized());
        
        let res = engine.embed("test");
        assert!(res.is_ok());
        assert_eq!(res.unwrap().len(), EMBEDDING_DIM);
    }
}
