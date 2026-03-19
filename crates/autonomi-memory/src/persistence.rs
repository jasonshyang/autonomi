use std::path::Path;

use rig::embeddings::{Embedding, EmbeddingModel};
use serde::{Deserialize, Serialize};

use crate::{MemoryEntry, MemoryError, MemoryStore};

// ---------------------------------------------------------------------------
// On-disk format
// ---------------------------------------------------------------------------

/// Wire format for a single persisted entry.
///
/// Embedding vectors are stored alongside the entry so the store can be
/// restored without making any API calls on startup.
#[derive(Serialize, Deserialize)]
struct PersistedEntry {
    entry: MemoryEntry,
    /// Raw `f64` values from [`Embedding::vec`].
    embedding_vec: Vec<f64>,
}

#[derive(Serialize, Deserialize)]
struct PersistedStore {
    entries: Vec<PersistedEntry>,
    turn_counter: u64,
}

// ---------------------------------------------------------------------------
// MemoryStore persistence methods
// ---------------------------------------------------------------------------

impl<E: EmbeddingModel + Send + Sync> MemoryStore<E> {
    /// Serialize the store to a JSON file at `path`.
    ///
    /// The parent directory must already exist.  Embedding vectors are written
    /// alongside each entry so that
    /// [`load_from_file`][MemoryStore::load_from_file] does not need to
    /// re-embed anything.
    pub async fn save_to_file(&self, path: &Path) -> Result<(), MemoryError> {
        let persisted = PersistedStore {
            entries: self
                .raw_entries()
                .iter()
                .map(|(entry, emb)| PersistedEntry {
                    entry: entry.clone(),
                    embedding_vec: emb.vec.clone(),
                })
                .collect(),
            turn_counter: self.turn_counter(),
        };

        let json = serde_json::to_string_pretty(&persisted)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    /// Restore a [`MemoryStore`] from a previously saved JSON file.
    ///
    /// If `path` does not exist, an empty store is returned — this makes
    /// first-run initialisation seamless without extra error handling in the
    /// caller.
    pub async fn load_from_file(model: E, path: &Path) -> Result<Self, MemoryError> {
        if !path.exists() {
            return Ok(Self::new(model));
        }

        let json = tokio::fs::read_to_string(path).await?;
        let persisted: PersistedStore = serde_json::from_str(&json)?;

        let entries = persisted
            .entries
            .into_iter()
            .map(|p| {
                let embedding =
                    Embedding { document: p.entry.content.clone(), vec: p.embedding_vec };
                (p.entry, embedding)
            })
            .collect();

        Ok(Self::restore(model, entries, persisted.turn_counter))
    }
}
