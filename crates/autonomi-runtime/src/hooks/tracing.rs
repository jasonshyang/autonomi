use async_trait::async_trait;

use super::{ErrorContext, Hook, PostCompletionContext, PrePromptContext};
use crate::error::HookError;

/// A hook that emits [`tracing`] events at every agent lifecycle point.
///
/// Enable with the `tracing-hook` feature (on by default).
pub struct TracingHook;

impl TracingHook {
    pub fn new() -> Self { TracingHook }
}

impl Default for TracingHook {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Hook for TracingHook {
    async fn on_pre_prompt(&self, ctx: &mut PrePromptContext) -> Result<(), HookError> {
        tracing::debug!(
            agent_id = %ctx.agent_id,
            prompt = %ctx.prompt,
            "agent: pre_prompt"
        );
        Ok(())
    }

    async fn on_post_completion(&self, ctx: &PostCompletionContext) -> Result<(), HookError> {
        tracing::debug!(
            agent_id = %ctx.agent_id,
            total_tokens = ctx.usage.as_ref().map(|u| u.total_tokens),
            "agent: turn_complete"
        );
        Ok(())
    }

    async fn on_error(&self, ctx: &ErrorContext) -> Result<(), HookError> {
        if ctx.is_fatal {
            tracing::error!(
                agent_id = %ctx.agent_id,
                error = %ctx.error,
                "agent: fatal error"
            );
        } else {
            tracing::warn!(
                agent_id = %ctx.agent_id,
                error = %ctx.error,
                "agent: error"
            );
        }
        Ok(())
    }
}
