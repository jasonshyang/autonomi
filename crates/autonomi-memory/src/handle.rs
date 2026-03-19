use std::path::PathBuf;

use rig::vector_store::{request::Filter, VectorSearchRequest, VectorStoreError, VectorStoreIndex};
use serde::de::DeserializeOwned;
use tokio::sync::{mpsc, oneshot};

use crate::{MemoryCommand, MemoryEntry, MemoryError};

/// A cheap-to-clone, non-generic handle to a running [`MemoryWorker`] task.
///
/// Callers hold a `MemoryHandle` and communicate with the worker exclusively
/// through message-passing — there is no shared state and no locks.
///
/// - [`add`][MemoryHandle::add] is fire-and-forget (non-blocking `try_send`).
/// - [`search`][MemoryHandle::search] and [`save`][MemoryHandle::save] are
///   async request/reply calls backed by oneshot channels.
#[derive(Clone)]
pub struct MemoryHandle {
    tx: mpsc::Sender<MemoryCommand>,
}

impl MemoryHandle {
    pub(crate) fn new(tx: mpsc::Sender<MemoryCommand>) -> Self { Self { tx } }

    /// Queue a write without waiting for the embedding to complete.
    ///
    /// Uses a non-blocking `try_send`. If the worker inbox is full the write
    /// is silently dropped rather than stalling the caller (e.g. the hook
    /// inside an agent loop).
    pub fn add(&self, agent_id: impl Into<String>, content: impl Into<String>) {
        let _ = self
            .tx
            .try_send(MemoryCommand::Add { agent_id: agent_id.into(), content: content.into() });
    }

    /// Return the top `n` entries most semantically similar to `query`.
    pub async fn search(
        &self,
        query: impl Into<String>,
        n: usize,
    ) -> Result<Vec<(f64, MemoryEntry)>, MemoryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(MemoryCommand::Search { query: query.into(), n, reply_tx })
            .await
            .map_err(|_| MemoryError::WorkerGone)?;
        reply_rx.await.map_err(|_| MemoryError::WorkerGone)?
    }

    /// Serialize the store to a JSON file at `path`.
    pub async fn save(&self, path: impl Into<PathBuf>) -> Result<(), MemoryError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(MemoryCommand::Save { path: path.into(), reply_tx })
            .await
            .map_err(|_| MemoryError::WorkerGone)?;
        reply_rx.await.map_err(|_| MemoryError::WorkerGone)?
    }

    /// Gracefully stop the worker task.
    pub async fn shutdown(&self) { let _ = self.tx.send(MemoryCommand::Shutdown).await; }
}

impl VectorStoreIndex for MemoryHandle {
    /// Use the canonical `Filter<serde_json::Value>` so rig's blanket
    /// `VectorStoreIndexDyn` impl applies automatically, satisfying the
    /// `dynamic_context()` trait bound.
    type Filter = Filter<serde_json::Value>;

    async fn top_n<T: DeserializeOwned + Send>(
        &self,
        req: VectorSearchRequest<Self::Filter>,
    ) -> Result<Vec<(f64, String, T)>, VectorStoreError> {
        let hits = self
            .search(req.query(), req.samples() as usize)
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        hits.into_iter()
            .map(|(score, entry)| {
                let id = entry.id.clone();
                let value = serde_json::to_value(&entry)?;
                let t: T = serde_json::from_value(value)?;
                Ok((score, id, t))
            })
            .collect()
    }

    async fn top_n_ids(
        &self,
        req: VectorSearchRequest<Self::Filter>,
    ) -> Result<Vec<(f64, String)>, VectorStoreError> {
        let hits = self
            .search(req.query(), req.samples() as usize)
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        Ok(hits
            .into_iter()
            .map(|(score, entry)| (score, entry.id))
            .collect())
    }
}
