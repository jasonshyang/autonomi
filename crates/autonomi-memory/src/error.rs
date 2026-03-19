use thiserror::Error;

/// Errors returned by memory operations.
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("embedding error: {0}")]
    Embedding(#[from] rig::embeddings::EmbeddingError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// The worker task has exited and the channel is closed.
    #[error("memory worker has shut down")]
    WorkerGone,
}
