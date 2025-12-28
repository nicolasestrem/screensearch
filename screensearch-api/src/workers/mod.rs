//! Background workers module

pub mod embedding_worker;
pub mod vision_worker;

pub use embedding_worker::{spawn_embedding_worker, EmbeddingWorker, EmbeddingWorkerConfig};
