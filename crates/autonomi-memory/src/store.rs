use autonomi_utils::Timestamp;
use rig::embeddings::{distance::VectorDistance, Embedding, EmbeddingModel};

use crate::{MemoryEntry, MemoryError};

// ---------------------------------------------------------------------------
// MemoryStore
// ---------------------------------------------------------------------------

/// In-process episodic memory store backed by dense embedding vectors.
///
/// Each call to [`add`][MemoryStore::add] embeds the conversation turn and
/// appends it to the store.  [`search`][MemoryStore::search] embeds the query
/// and returns the top-N most similar entries by cosine similarity.
///
/// The store is intended to be wrapped in `Arc<RwLock<MemoryStore<E>>>` and
/// shared between a [`MemoryHook`][crate::MemoryHook] (writer) and a
/// [`MemoryIndex`][crate::MemoryIndex] (reader).
pub struct MemoryStore<E: EmbeddingModel> {
    model: E,
    entries: Vec<(MemoryEntry, Embedding)>,
    turn_counter: u64,
}

impl<E: EmbeddingModel + Send + Sync> MemoryStore<E> {
    /// Create a new empty store using `model` for all embedding operations.
    pub fn new(model: E) -> Self { Self { model, entries: Vec::new(), turn_counter: 0 } }

    /// Embed `content` and append a new [`MemoryEntry`] to the store.
    ///
    /// `agent_id` is used to namespace the entry id, but all agents share
    /// the same store — pass an empty string if cross-agent retrieval is
    /// desired without filtering.
    pub async fn add(&mut self, agent_id: &str, content: String) -> Result<(), MemoryError> {
        self.turn_counter += 1;

        let timestamp_secs = Timestamp::now().as_secs();

        let entry = MemoryEntry {
            id: format!("turn_{agent_id}_{}", self.turn_counter),
            content: content.clone(),
            agent_id: agent_id.to_string(),
            timestamp_secs,
            turn: self.turn_counter,
        };

        let embedding = self.model.embed_text(&content).await?;

        self.entries.push((entry, embedding));
        Ok(())
    }

    /// Embed `query` and return the top-`n` most similar stored entries,
    /// sorted descending by cosine similarity score.
    pub async fn search(
        &self,
        query: &str,
        n: usize,
    ) -> Result<Vec<(f64, MemoryEntry)>, MemoryError> {
        if self.entries.is_empty() || n == 0 {
            return Ok(Vec::new());
        }

        let query_embedding = self.model.embed_text(query).await?;

        let mut scored: Vec<(f64, MemoryEntry)> = self
            .entries
            .iter()
            .map(|(entry, emb)| {
                let score = query_embedding.cosine_similarity(emb, false);
                (score, entry.clone())
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(n);

        Ok(scored)
    }

    /// Number of turns currently held in the store.
    pub fn len(&self) -> usize { self.entries.len() }

    /// Returns `true` if no turns have been stored yet.
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    // --- Persistence helpers (used by persistence.rs) ---

    pub(crate) fn raw_entries(&self) -> &[(MemoryEntry, Embedding)] { &self.entries }

    pub(crate) fn turn_counter(&self) -> u64 { self.turn_counter }

    pub(crate) fn restore(
        model: E,
        entries: Vec<(MemoryEntry, Embedding)>,
        turn_counter: u64,
    ) -> Self {
        Self { model, entries, turn_counter }
    }
}
