use std::{collections::HashMap, sync::Arc};

use crate::{error::HookError, event::TokenUsage, runtime::AgentId};

// ---------------------------------------------------------------------------
// Type Alias
// ---------------------------------------------------------------------------

/// A type alias for a reference-counted, dynamically-dispatched [`Hook`].
pub type ArcHook = Arc<dyn Hook>;

// ---------------------------------------------------------------------------
// Hook trait
// ---------------------------------------------------------------------------

/// A composable async callback invoked at key lifecycle points in the agent
/// loop.
///
/// All methods have default no-op implementations; implement only the ones you
/// need.
///
/// Register hooks globally (all agents) via
/// [`RuntimeBuilder::hook`][crate::RuntimeBuilder::hook] or
/// [`Runtime::register_hook`][crate::Runtime::register_hook], or scoped to a
/// specific agent via
/// [`Runtime::register_hook_for`][crate::Runtime::register_hook_for].
#[async_trait::async_trait]
pub trait Hook: Send + Sync + 'static {
    /// Called immediately before a prompt is sent to the agent plugin.
    ///
    /// The hook may rewrite [`PrePromptContext::prompt`]. Returning `Err`
    /// cancels the current turn and emits an `AgentError` event.
    async fn on_pre_prompt(&self, _ctx: &mut PrePromptContext) -> Result<(), HookError> { Ok(()) }

    /// Called after a complete response has been assembled.
    ///
    /// Errors from this hook are logged but do not cancel the turn or prevent
    /// the `TurnComplete` event from being emitted.
    async fn on_post_completion(&self, _ctx: &PostCompletionContext) -> Result<(), HookError> {
        Ok(())
    }

    /// Called when a non-fatal or fatal error occurs in the agent loop.
    async fn on_error(&self, _ctx: &ErrorContext) -> Result<(), HookError> { Ok(()) }
}

// ---------------------------------------------------------------------------
// HookRegistry
// ---------------------------------------------------------------------------

/// Stores and dispatches hooks registered with the runtime.
#[derive(Clone)]
pub struct HookRegistry {
    global: Vec<ArcHook>,
    per_agent: HashMap<AgentId, Vec<ArcHook>>,
}

impl HookRegistry {
    pub fn new() -> Self { Self { global: Vec::new(), per_agent: HashMap::new() } }

    /// Register a hook that fires for every agent.
    pub fn register_global(&mut self, hook: impl Hook) { self.global.push(Arc::new(hook)); }

    /// Register a pre-boxed hook that fires for every agent (used internally
    /// by the builder to avoid a double-wrap).
    pub(crate) fn register_global_arc(&mut self, hook: ArcHook) { self.global.push(hook); }

    /// Register a hook scoped to one specific agent.
    pub fn register_for(&mut self, id: AgentId, hook: impl Hook) {
        self.per_agent.entry(id).or_default().push(Arc::new(hook));
    }

    /// Returns a snapshot of all hooks applicable to `id` (global first, then
    /// per-agent). The `Arc` clones are cheap and allow callers to release
    /// the registry lock before executing any async hook methods.
    pub(crate) fn snapshot_for(&self, id: &AgentId) -> Vec<ArcHook> {
        let mut result: Vec<ArcHook> = self.global.clone();
        if let Some(per) = self.per_agent.get(id) {
            result.extend(per.iter().cloned());
        }
        result
    }
}

impl Default for HookRegistry {
    fn default() -> Self { Self::new() }
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Context passed to [`Hook::on_pre_prompt`]. Hooks may rewrite `prompt`.
pub struct PrePromptContext {
    pub agent_id: AgentId,
    /// The prompt about to be sent to the agent. Hooks may rewrite this field.
    pub prompt: String,
}

/// Context passed to [`Hook::on_post_completion`].
pub struct PostCompletionContext {
    pub agent_id: AgentId,
    pub prompt: String,
    pub response: String,
    pub usage: Option<TokenUsage>,
}

/// Context passed to [`Hook::on_error`].
pub struct ErrorContext {
    pub agent_id: AgentId,
    pub error: String,
    pub is_fatal: bool,
}
