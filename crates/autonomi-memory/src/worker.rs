use std::path::Path;

use rig::embeddings::EmbeddingModel;
use tokio::sync::mpsc;

use crate::{MemoryCommand, MemoryError, MemoryHandle, MemoryStore};

/// Default inbox capacity for the worker channel.
const DEFAULT_CAPACITY: usize = 256;

// ---------------------------------------------------------------------------
// MemoryWorker
// ---------------------------------------------------------------------------

/// Spawns a background task that owns a [`MemoryStore`] and processes
/// [`MemoryCommand`]s over an MPSC channel.
///
/// All embedding API calls, cosine searches, and file I/O run inside the
/// worker task — callers only ever touch the returned [`MemoryHandle`].
///
/// # Example
///
/// ```rust,ignore
/// use autonomi_memory::{MemoryWorker, MemoryHook, MemoryIndex};
/// use autonomi_runtime::RuntimeBuilder;
/// use autonomi_agent::Provider;
/// use rig::providers::openai;
///
/// let model = openai::Client::from_env().embedding_model(openai::TEXT_EMBEDDING_ADA_002);
/// let handle = MemoryWorker::spawn(model, 256);
///
/// let hook  = MemoryHook::new(handle.clone());
/// let index = MemoryIndex::new(handle.clone());
///
/// let agent = Provider::openai(openai::GPT_4O)
///     .preamble("You are a helpful assistant.")
///     .dynamic_context(5, index)
///     .build();
///
/// let runtime = RuntimeBuilder::default()
///     .register(agent)
///     .hook(hook)
///     .build();
/// ```
pub struct MemoryWorker;

impl MemoryWorker {
    /// Spawn a fresh, empty worker and return a handle to it.
    pub fn spawn<E>(model: E, capacity: usize) -> MemoryHandle
    where
        E: EmbeddingModel + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel(capacity);
        tokio::spawn(run_worker(MemoryStore::new(model), rx));
        MemoryHandle::new(tx)
    }

    /// Restore the store from `path` (if it exists) then spawn the worker.
    ///
    /// If the file does not exist a fresh empty store is used — first-run
    /// initialisation requires no special handling.
    pub async fn spawn_from_file<E>(
        model: E,
        path: &Path,
        capacity: usize,
    ) -> Result<MemoryHandle, MemoryError>
    where
        E: EmbeddingModel + Send + Sync + 'static,
    {
        let store = MemoryStore::load_from_file(model, path).await?;
        let (tx, rx) = mpsc::channel(capacity);
        tokio::spawn(run_worker(store, rx));
        Ok(MemoryHandle::new(tx))
    }

    /// Spawn with the default inbox capacity ([`DEFAULT_CAPACITY`]).
    pub fn spawn_default<E>(model: E) -> MemoryHandle
    where
        E: EmbeddingModel + Send + Sync + 'static,
    {
        Self::spawn(model, DEFAULT_CAPACITY)
    }
}

// ---------------------------------------------------------------------------
// Worker event loop
// ---------------------------------------------------------------------------

async fn run_worker<E>(mut store: MemoryStore<E>, mut rx: mpsc::Receiver<MemoryCommand>)
where
    E: EmbeddingModel + Send + Sync + 'static,
{
    tracing::info!("memory worker: started");

    while let Some(cmd) = rx.recv().await {
        match cmd {
            MemoryCommand::Add { agent_id, content } => {
                if let Err(e) = store.add(&agent_id, content).await {
                    tracing::warn!(error = %e, "memory worker: failed to embed turn");
                }
            },

            MemoryCommand::Search { query, n, reply_tx } => {
                let result = store.search(&query, n).await;
                let _ = reply_tx.send(result);
            },

            MemoryCommand::Save { path, reply_tx } => {
                let result = store.save_to_file(&path).await;
                let _ = reply_tx.send(result);
            },

            MemoryCommand::Shutdown => {
                tracing::debug!("memory worker: shutting down");
                break;
            },
        }
    }

    tracing::info!("memory worker: stopped");
}
