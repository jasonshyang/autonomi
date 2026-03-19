use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::MemoryError;

/// Messages sent into the memory worker task.
pub(crate) enum MemoryCommand {
    /// Embed `content` and append it as a new turn for `agent_id`.
    Add { agent_id: String, content: String },

    /// Find the top `n` entries most similar to `query`.
    Search {
        query: String,
        n: usize,
        reply_tx: oneshot::Sender<Result<Vec<(f64, MemoryEntry)>, MemoryError>>,
    },

    /// Serialize the store to `path`.
    Save { path: PathBuf, reply_tx: oneshot::Sender<Result<(), MemoryError>> },

    /// Stop the worker task.
    Shutdown,
}

/// A single turn of conversation stored in the memory vector store.
///
/// The `content` field is the text that gets embedded and searched.
/// It contains both the user prompt and the agent response, formatted as:
/// `"User: {prompt}\nAssistant: {response}"`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier: `"turn_{agent_id}_{turn_number}"`.
    pub id: String,
    /// The full conversation turn text that is embedded for similarity search.
    pub content: String,
    /// The id of the agent that produced this turn.
    pub agent_id: String,
    /// Unix timestamp (seconds) when this turn was recorded.
    pub timestamp_secs: u64,
    /// Monotonically increasing turn number within this agent's history.
    pub turn: u64,
}
