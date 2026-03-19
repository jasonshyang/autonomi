use tokio::sync::broadcast;

use crate::runtime::AgentId;

// ---------------------------------------------------------------------------
// EventBus
// ---------------------------------------------------------------------------

/// Shared broadcast channel that all agent loops write to and all
/// subscribers read from.
pub struct EventBus {
    tx: broadcast::Sender<RuntimeEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Create an independent subscriber receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<RuntimeEvent> { self.tx.subscribe() }

    /// Clone the sender so agent loop tasks can publish without holding a
    /// reference to the bus.
    pub(crate) fn sender(&self) -> broadcast::Sender<RuntimeEvent> { self.tx.clone() }

    /// Publish an event, ignoring send errors (lagged/dead receivers).
    pub fn publish(&self, event: RuntimeEvent) { let _ = self.tx.send(event); }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// All observable events emitted by running agent loops.
#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    /// The agent completed one full response turn.
    TurnComplete {
        agent_id: AgentId,
        /// The full assembled response text.
        response: String,
        /// Token usage reported by the provider, if available.
        usage: Option<TokenUsage>,
    },

    /// A non-fatal error occurred inside the agent loop.
    AgentError { agent_id: AgentId, error: String },

    /// The agent loop has exited (gracefully or fatally).
    AgentStopped { agent_id: AgentId, reason: StopReason },
}

/// Why an agent loop exited.
#[derive(Clone, Debug)]
pub enum StopReason {
    /// The runtime requested a graceful shutdown.
    Requested,
    /// An unrecoverable error caused the loop to exit.
    Fatal(String),
}

/// Token-usage statistics for a single response turn.
#[derive(Clone, Debug)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}
