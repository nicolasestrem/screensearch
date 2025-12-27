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
pub mod rag_helpers;
pub mod reranker;
pub mod generate;
pub use generate::*;



