/// Errors that can occur while connecting to a [`Toolbox`] server.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The MCP transport connection or handshake failed.
    #[error("MCP connection failed: {0}")]
    Connect(Box<rmcp::service::ClientInitializeError>),

    /// Fetching the tool list from the server failed.
    #[error("Failed to fetch tool list: {0}")]
    ListTools(#[from] rmcp::ServiceError),

    /// Failed to spawn the toolbox server child process.
    #[error("Failed to spawn toolbox server process: {0}")]
    SpawnProcess(#[source] std::io::Error),
}

impl From<rmcp::service::ClientInitializeError> for ClientError {
    fn from(e: rmcp::service::ClientInitializeError) -> Self { ClientError::Connect(Box::new(e)) }
}

/// Errors that can occur while starting a [`Toolbox`] server.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// The MCP transport failed to initialise.
    #[error("Failed to start Toolbox server: {0}")]
    Start(#[from] rmcp::service::ServerInitializeError),
}
