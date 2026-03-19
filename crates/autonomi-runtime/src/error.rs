use thiserror::Error;

/// Errors returned by [`Runtime`][crate::Runtime] operations.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("agent not found: '{0}'")]
    AgentNotFound(String),

    #[error("failed to send to agent '{0}': channel closed")]
    SendFailed(String),
}

/// Errors returned by [`Hook`][crate::Hook] implementations.
#[derive(Debug, Error, Clone)]
pub enum HookError {
    #[error("{0}")]
    Custom(String),
}

impl HookError {
    pub fn custom(msg: impl Into<String>) -> Self { HookError::Custom(msg.into()) }
}
