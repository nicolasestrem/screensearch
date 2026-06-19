//! Request handlers module

pub mod automation;
pub mod search;
pub mod system;

pub use automation::*;
pub use search::*;
pub use system::*;
pub mod ai;
pub use ai::*;
pub mod embeddings;
pub use embeddings::*;
pub mod generate;
pub mod rag_helpers;
pub mod reranker;
pub use generate::*;
pub mod downloads;
pub use downloads::*;
pub mod vision;
pub use vision::*;
