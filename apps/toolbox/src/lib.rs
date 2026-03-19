//! # toolbox
//!
//! Shared startup helpers for toolbox server binaries.
//!
//! Each binary under `src/bin/` calls [`init_tracing`] once at startup, then
//! calls [`ServerConfig::from_env`] to get the name / version / instructions
//! that should be reported to MCP clients.

/// Initialise tracing with logs directed to **stderr**.
///
/// Logs must never touch stdout because that stream is reserved for the
/// JSON-RPC message framing used by the MCP stdio transport.
///
/// The log filter is read from `RUST_LOG`; it falls back to `"info"` when the
/// variable is absent or malformed.
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
}

/// MCP server identity loaded from environment variables.
///
/// | Variable | Default | Description |
/// |----------|---------|-------------|
/// | `TOOLBOX_SERVER_NAME` | *(binary-specific)* | Server name reported to MCP clients |
/// | `TOOLBOX_SERVER_VERSION` | value of `CARGO_PKG_VERSION` in the calling crate | Version string reported to MCP clients |
/// | `TOOLBOX_SERVER_INSTRUCTIONS` | *(none)* | Human-readable instructions for MCP clients |
pub struct ServerConfig {
    pub name: String,
    pub version: String,
    pub instructions: Option<String>,
}

impl ServerConfig {
    /// Read server identity from environment variables.
    ///
    /// `default_name` and `default_version` are used when the corresponding
    /// environment variables are absent.  Pass the values of the binary's own
    /// `env!("CARGO_PKG_NAME")` and `env!("CARGO_PKG_VERSION")` macros so
    /// that each binary self-identifies correctly out of the box.
    pub fn from_env(default_name: &str, default_version: &str) -> Self {
        let name =
            std::env::var("TOOLBOX_SERVER_NAME").unwrap_or_else(|_| default_name.to_string());

        let version =
            std::env::var("TOOLBOX_SERVER_VERSION").unwrap_or_else(|_| default_version.to_string());

        let instructions = std::env::var("TOOLBOX_SERVER_INSTRUCTIONS").ok();

        Self { name, version, instructions }
    }
}
