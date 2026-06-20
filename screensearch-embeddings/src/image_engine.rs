//! In-process image-embedding engine for visual recall.
//!
//! Embeds screenshots with `nomic-embed-vision-v1.5` and image-search queries
//! with `nomic-embed-text-v1.5`. The two models share a 768-dim latent space, so
//! a text query can retrieve a matching screenshot directly from pixels — giving
//! non-OCR visual content (charts, canvases, icon-heavy UIs) a recall path.
//!
//! Like [`crate::EmbeddingEngine`], inference runs on the blocking pool behind a
//! `Mutex` (the ONNX models are not `Sync`), and the models download from
//! HuggingFace on first use. Loaded only when image embeddings are enabled.

use crate::{
    EmbeddingError, Result, EMBEDDING_DIM, IMAGE_MODEL_NAME, IMAGE_MODEL_VERSION,
    IMAGE_QUERY_MODEL_NAME,
};
use fastembed::{
    EmbeddingModel, ImageEmbedding, ImageEmbeddingModel, ImageInitOptions, TextEmbedding,
    TextInitOptions,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::info;

/// nomic-text retrieval prefix for queries (per the model card).
const NOMIC_QUERY_PREFIX: &str = "search_query: ";

/// Image-embedding batch size (image encoding is heavier than text).
const IMAGE_BATCH_SIZE: usize = 8;

/// In-process image + image-query embedding engine.
pub struct ImageEmbeddingEngine {
    image_model: Arc<Mutex<ImageEmbedding>>,
    query_model: Arc<Mutex<TextEmbedding>>,
}

impl ImageEmbeddingEngine {
    /// Load (and, on first run, download + cache) the nomic vision + text models.
    pub async fn new() -> Result<Self> {
        crate::engine::configure_ort_dylib_path();
        let cache_dir: Option<PathBuf> =
            std::env::var_os("SCREENSEARCH_MODEL_CACHE").map(PathBuf::from);

        let (image_model, query_model) = tokio::task::spawn_blocking(move || {
            let mut img_init = ImageInitOptions::new(ImageEmbeddingModel::NomicEmbedVisionV15)
                .with_show_download_progress(true);
            if let Some(dir) = &cache_dir {
                img_init = img_init.with_cache_dir(dir.clone());
            }
            let image_model = ImageEmbedding::try_new(img_init)
                .map_err(|error| EmbeddingError::ModelInitError(error.to_string()))?;

            let mut txt_init = TextInitOptions::new(EmbeddingModel::NomicEmbedTextV15)
                .with_show_download_progress(true);
            if let Some(dir) = &cache_dir {
                txt_init = txt_init.with_cache_dir(dir.clone());
            }
            let query_model = TextEmbedding::try_new(txt_init)
                .map_err(|error| EmbeddingError::ModelInitError(error.to_string()))?;

            Ok::<_, EmbeddingError>((
                Arc::new(Mutex::new(image_model)),
                Arc::new(Mutex::new(query_model)),
            ))
        })
        .await
        .map_err(|error| EmbeddingError::ModelInitError(error.to_string()))??;

        info!(
            image_model = IMAGE_MODEL_NAME,
            query_model = IMAGE_QUERY_MODEL_NAME,
            "Initialized in-process image embedding engine"
        );

        Ok(Self {
            image_model,
            query_model,
        })
    }

    /// Embed screenshot files (by path) into 768-dim vectors.
    pub async fn embed_images(&self, paths: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let model = Arc::clone(&self.image_model);
        let embeddings = tokio::task::spawn_blocking(move || {
            let mut guard = model.lock().map_err(|_| {
                EmbeddingError::InferenceError("image embedding model lock poisoned".into())
            })?;
            guard
                .embed(paths, Some(IMAGE_BATCH_SIZE))
                .map_err(|error| EmbeddingError::InferenceError(error.to_string()))
        })
        .await
        .map_err(|error| EmbeddingError::InferenceError(error.to_string()))??;

        for embedding in &embeddings {
            if embedding.len() != EMBEDDING_DIM {
                return Err(EmbeddingError::InferenceError(format!(
                    "expected {EMBEDDING_DIM}-dim image embeddings, got {}",
                    embedding.len()
                )));
            }
        }
        Ok(embeddings)
    }

    /// Embed a text query into the shared image/text space (with the nomic query
    /// prefix), for searching the image index.
    pub async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let prompted = format!("{NOMIC_QUERY_PREFIX}{text}");
        let model = Arc::clone(&self.query_model);
        let mut embeddings = tokio::task::spawn_blocking(move || {
            let mut guard = model.lock().map_err(|_| {
                EmbeddingError::InferenceError("query embedding model lock poisoned".into())
            })?;
            guard
                .embed(vec![prompted.as_str()], None)
                .map_err(|error| EmbeddingError::InferenceError(error.to_string()))
        })
        .await
        .map_err(|error| EmbeddingError::InferenceError(error.to_string()))??;

        let embedding = embeddings
            .pop()
            .ok_or_else(|| EmbeddingError::InferenceError("model returned no embedding".into()))?;
        if embedding.len() != EMBEDDING_DIM {
            return Err(EmbeddingError::InferenceError(format!(
                "expected {EMBEDDING_DIM}-dim query embeddings, got {}",
                embedding.len()
            )));
        }
        Ok(embedding)
    }

    pub fn provider(&self) -> &'static str {
        "fastembed"
    }

    pub fn model(&self) -> &'static str {
        IMAGE_MODEL_NAME
    }

    pub fn model_version(&self) -> &'static str {
        IMAGE_MODEL_VERSION
    }
}
