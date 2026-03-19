pub type AgentResult<T> = std::result::Result<T, AgentError>;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// A transient error.
    #[error("recoverable: {0}")]
    Recoverable(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),

    /// An unrecoverable error (e.g. auth failure).
    #[error("fatal: {0}")]
    Fatal(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl AgentError {
    pub fn recoverable(e: impl std::error::Error + Send + Sync + 'static) -> Self {
        AgentError::Recoverable(Box::new(e))
    }

    pub fn fatal(e: impl std::error::Error + Send + Sync + 'static) -> Self {
        AgentError::Fatal(Box::new(e))
    }

    pub fn is_fatal(&self) -> bool { matches!(self, AgentError::Fatal(_)) }
}
