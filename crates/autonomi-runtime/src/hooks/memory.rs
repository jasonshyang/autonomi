use autonomi_memory::MemoryHandle;

use crate::{
    hooks::{Hook, PostCompletionContext},
    HookError,
};

// ---------------------------------------------------------------------------
// MemoryHook
// ---------------------------------------------------------------------------

/// A [`Hook`] that writes every completed agent turn into the memory worker.
///
/// Register this hook with the runtime via
/// [`RuntimeBuilder::hook`][autonomi_runtime::RuntimeBuilder::hook] (global)
/// or [`Runtime::register_hook_for`][autonomi_runtime::Runtime::register_hook_for]
/// (per-agent).  Pass the same [`MemoryHandle`] to both this hook and a
/// [`MemoryIndex`][crate::MemoryIndex] so they share the same backing worker.
///
/// The `on_post_completion` call is **non-blocking**: it enqueues the write
/// via a `try_send` and returns immediately.
pub struct MemoryHook {
    handle: MemoryHandle,
}

impl MemoryHook {
    pub fn new(handle: MemoryHandle) -> Self { Self { handle } }
}

// ---------------------------------------------------------------------------
// Hook impl
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl Hook for MemoryHook {
    async fn on_post_completion(&self, ctx: &PostCompletionContext) -> Result<(), HookError> {
        let content = format!("User: {}\nAssistant: {}", ctx.prompt, ctx.response);
        self.handle.add(ctx.agent_id.to_string(), content);
        Ok(())
    }
}
