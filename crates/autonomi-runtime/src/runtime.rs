use std::{collections::HashMap, sync::Arc};

use autonomi_agent::{BoxedAgent, RunAgent};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    agent_loop::run_agent_loop,
    error::RuntimeError,
    event::{EventBus, RuntimeEvent},
    hooks::{ArcHook, Hook, HookRegistry},
    shared::Shared,
};

const DEFAULT_BUS_CAPACITY: usize = 1024;
const DEFAULT_INPUT_CAPACITY: usize = 64;

// ---------------------------------------------------------------------------
// RuntimeBuilder
// ---------------------------------------------------------------------------

/// Fluent builder for [`Runtime`].
///
/// Register your pre-built agent plugins and global hooks here, then call
/// [`build()`][RuntimeBuilder::build] to start all agent loops.
///
/// # Example
///
/// ```rust,ignore
/// use autonomi_runtime::{Runtime, hooks::tracing::TracingHook};
///
/// let runtime = Runtime::builder()
///     .register(ResearchAgent::new())
///     .register(WriterAgent::new())
///     .hook(TracingHook::new())
///     .bus_capacity(512)
///     .build();
/// ```
pub struct RuntimeBuilder {
    agents: Vec<BoxedAgent>,
    global_hooks: Vec<ArcHook>,
    bus_capacity: usize,
}

impl Default for RuntimeBuilder {
    fn default() -> Self {
        Self { agents: Vec::new(), global_hooks: Vec::new(), bus_capacity: DEFAULT_BUS_CAPACITY }
    }
}

impl RuntimeBuilder {
    /// Register a pre-built agent plugin.
    ///
    /// All registered plugins will have their loops started when
    /// [`build()`][RuntimeBuilder::build] is called. The returned
    /// [`AgentId`] can be retrieved after build via
    /// [`Runtime::agent_id`].
    pub fn register<A: RunAgent>(mut self, agent: A) -> Self {
        self.agents.push(Box::new(agent));
        self
    }

    /// Register a global hook (fires for every agent, in registration order).
    pub fn hook<H: Hook>(mut self, hook: H) -> Self {
        self.global_hooks.push(Arc::new(hook));
        self
    }

    /// Set the broadcast channel capacity for the event bus (default: 1024).
    ///
    /// Slow subscribers that fall more than `capacity` events behind will
    /// receive [`tokio::sync::broadcast::error::RecvError::Lagged`] rather
    /// than stalling producing agents.
    pub fn bus_capacity(mut self, capacity: usize) -> Self {
        self.bus_capacity = capacity;
        self
    }

    /// Consume the builder, spawn all registered agent loops, and return a
    /// running [`Runtime`].
    pub fn build(self) -> Runtime {
        let event_bus = EventBus::new(self.bus_capacity);

        let mut registry = HookRegistry::new();
        for hook in self.global_hooks {
            registry.register_global_arc(hook);
        }
        let hooks = Shared::new(registry);

        let mut runtime = Runtime {
            senders: HashMap::new(),
            tasks: Vec::new(),
            name_to_ids: HashMap::new(),
            event_bus,
            hooks,
        };

        for agent in self.agents {
            runtime.spawn_boxed(agent);
        }

        runtime
    }
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

/// The orchestration layer that drives registered [`Agent`]s.
///
/// Each plugin runs in its own dedicated [`tokio::task`]. The runtime
/// schedules prompts, maintains per-agent conversation history, fires
/// registered hooks, and broadcasts all activity as [`RuntimeEvent`]s over
/// a shared event bus.
///
/// Create one via [`Runtime::builder()`].
pub struct Runtime {
    /// Prompt channels indexed by agent id (cheap `&self` access for send ops).
    senders: HashMap<AgentId, mpsc::Sender<AgentInput>>,
    /// Task handles for lifecycle management (shutdown_all, etc.).
    tasks: Vec<(AgentId, JoinHandle<()>)>,
    /// Reverse lookup: plugin name → assigned ids (in registration order).
    name_to_ids: HashMap<String, Vec<AgentId>>,
    event_bus: EventBus,
    hooks: Shared<HookRegistry>,
}

impl Runtime {
    /// Return a fluent builder for constructing a [`Runtime`].
    pub fn builder() -> RuntimeBuilder { RuntimeBuilder::default() }

    // -----------------------------------------------------------------------
    // Agent registration
    // -----------------------------------------------------------------------

    /// Dynamically spawn a new agent plugin at runtime and return its id.
    ///
    /// Use [`RuntimeBuilder::register`] for compile-time (startup)
    /// registration instead.
    pub fn spawn<A: RunAgent>(&mut self, plugin: A) -> AgentId { self.spawn_boxed(Box::new(plugin)) }

    fn spawn_boxed(&mut self, plugin: BoxedAgent) -> AgentId {
        let id = self.allocate_id(plugin.name());
        let (tx, rx) = mpsc::channel(DEFAULT_INPUT_CAPACITY);
        let event_tx = self.event_bus.sender();
        let hooks = self.hooks.clone();
        let id_clone = id.clone();

        let task = tokio::spawn(run_agent_loop(id_clone, plugin, rx, event_tx, hooks));

        self.senders.insert(id.clone(), tx);
        self.tasks.push((id.clone(), task));
        id
    }

    fn allocate_id(&mut self, name: &str) -> AgentId {
        let ids = self.name_to_ids.entry(name.to_string()).or_default();
        let count = ids.len() as u64;
        let id = if count == 0 { AgentId::new(name) } else { AgentId::with_suffix(name, count) };
        ids.push(id.clone());
        id
    }

    // -----------------------------------------------------------------------
    // Querying
    // -----------------------------------------------------------------------

    /// Look up the [`AgentId`] of the first plugin registered under `name`.
    pub fn agent_id(&self, name: &str) -> Option<AgentId> {
        self.name_to_ids.get(name)?.first().cloned()
    }

    /// Look up all [`AgentId`]s registered under `name` (useful when the same
    /// plugin name was registered more than once).
    pub fn agent_ids_for(&self, name: &str) -> Vec<AgentId> {
        self.name_to_ids.get(name).cloned().unwrap_or_default()
    }

    /// Returns an iterator over every running agent id.
    pub fn agent_ids(&self) -> impl Iterator<Item = &AgentId> { self.senders.keys() }

    // -----------------------------------------------------------------------
    // Event bus
    // -----------------------------------------------------------------------

    /// Subscribe to all runtime events.
    ///
    /// Returns an independent [`tokio::sync::broadcast::Receiver`]; callers
    /// that fall behind by more than the bus capacity will receive a
    /// `Lagged` error rather than stalling the runtime.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<RuntimeEvent> {
        self.event_bus.subscribe()
    }

    // -----------------------------------------------------------------------
    // Messaging
    // -----------------------------------------------------------------------

    /// Enqueue a prompt for the named agent (fire-and-forget).
    pub async fn send(&self, id: &AgentId, prompt: String) -> Result<(), RuntimeError> {
        let tx = self.sender_for(id)?;
        tx.send(AgentInput::Prompt { prompt, reply_tx: None })
            .await
            .map_err(|_| RuntimeError::SendFailed(id.to_string()))
    }

    /// Send a prompt and await the full response text.
    pub async fn prompt(&self, id: &AgentId, prompt: String) -> Result<String, RuntimeError> {
        let tx = self.sender_for(id)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        tx.send(AgentInput::Prompt { prompt, reply_tx: Some(reply_tx) })
            .await
            .map_err(|_| RuntimeError::SendFailed(id.to_string()))?;

        reply_rx
            .await
            .map_err(|_| RuntimeError::SendFailed(id.to_string()))
    }

    /// Clear the conversation history for the given agent.
    pub async fn reset_history(&self, id: &AgentId) -> Result<(), RuntimeError> {
        let tx = self.sender_for(id)?;
        tx.send(AgentInput::Reset)
            .await
            .map_err(|_| RuntimeError::SendFailed(id.to_string()))
    }

    /// Gracefully stop a single agent loop.
    ///
    /// The loop emits [`RuntimeEvent::AgentStopped`] before exiting.
    pub async fn shutdown(&self, id: &AgentId) -> Result<(), RuntimeError> {
        let tx = self.sender_for(id)?;
        tx.send(AgentInput::Shutdown)
            .await
            .map_err(|_| RuntimeError::SendFailed(id.to_string()))
    }

    /// Gracefully stop all agent loops and await their tasks.
    pub async fn shutdown_all(&mut self) -> Result<(), RuntimeError> {
        for tx in self.senders.values() {
            let _ = tx.send(AgentInput::Shutdown).await;
        }
        for (_, task) in self.tasks.drain(..) {
            let _ = task.await;
        }
        self.senders.clear();
        self.name_to_ids.clear();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Hook registration (post-build)
    // -----------------------------------------------------------------------

    /// Register a global hook after the runtime has been built.
    ///
    /// This is a synchronous, wait-free operation: it atomically swaps in a
    /// new registry snapshot without blocking any running agent loops.
    pub fn register_hook<H: Hook>(&self, hook: H) {
        self.hooks.update_once(|mut registry| {
            registry.register_global(hook);
            registry
        });
    }

    /// Register a per-agent hook after the runtime has been built.
    ///
    /// This is a synchronous, wait-free operation: it atomically swaps in a
    /// new registry snapshot without blocking any running agent loops.
    pub fn register_hook_for<H: Hook>(&self, id: AgentId, hook: H) {
        self.hooks.update_once(|mut registry| {
            registry.register_for(id, hook);
            registry
        });
    }

    // -----------------------------------------------------------------------
    // Internals
    // -----------------------------------------------------------------------

    fn sender_for(&self, id: &AgentId) -> Result<&mpsc::Sender<AgentInput>, RuntimeError> {
        self.senders
            .get(id)
            .ok_or_else(|| RuntimeError::AgentNotFound(id.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A stable, cheaply-clonable identifier for a running agent plugin.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AgentId(pub Arc<str>);

impl AgentId {
    pub fn new(name: &str) -> Self { AgentId(Arc::from(name)) }

    pub fn with_suffix(name: &str, counter: u64) -> Self {
        AgentId(Arc::from(format!("{name}-{counter}").as_str()))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}

/// Messages sent from the runtime into an agent loop task.
pub(crate) enum AgentInput {
    /// A new prompt turn with an optional one-shot reply channel.
    Prompt {
        prompt: String,
        /// If `Some`, the full response text is sent here when the turn ends.
        reply_tx: Option<oneshot::Sender<String>>,
    },
    /// Clear the agent's conversation history.
    Reset,
    /// Gracefully stop the agent loop.
    Shutdown,
}
