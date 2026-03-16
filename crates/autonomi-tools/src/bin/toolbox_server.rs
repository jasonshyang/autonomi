//! # toolbox-server
//!
//! A stdio MCP server binary built on top of [`Toolbox`] with all built-in
//! tools pre-registered.
//!
//! ## Tools
//!
//! | Tool | Module |
//! |------|--------|
//! | `read_file` | [`readonly_filesystem`][autonomi_tools::tools::readonly_filesystem] |
//! | `list_dir` | [`readonly_filesystem`][autonomi_tools::tools::readonly_filesystem] |
//! | `file_metadata` | [`readonly_filesystem`][autonomi_tools::tools::readonly_filesystem] |
//! | `http_fetch` | [`http_fetch`][autonomi_tools::tools::http_fetch] |
//! | `env_vars` | [`process_info`][autonomi_tools::tools::process_info] |
//! | `working_dir` | [`process_info`][autonomi_tools::tools::process_info] |
//! | `hostname` | [`process_info`][autonomi_tools::tools::process_info] |
//!
//! ## Environment variables
//!
//! | Variable | Default | Description |
//! |----------|---------|-------------|
//! | `TOOLBOX_SERVER_NAME` | `toolbox-server` | Server name reported to MCP clients |
//! | `TOOLBOX_SERVER_VERSION` | crate version | Version string reported to MCP clients |
//! | `TOOLBOX_SERVER_INSTRUCTIONS` | *(none)* | Human-readable instructions for MCP clients |
//! | `RUST_LOG` | `info` | Tracing filter (logs go to **stderr**) |

use autonomi_tools::{
    toolbox::Toolbox,
    tools::{
        EnvVarsTool, FileMetadataTool, HostnameTool, HttpFetchTool, ListDirTool, ReadFileTool,
        WorkingDirTool,
    },
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logs MUST go to stderr — stdout is reserved for the JSON-RPC stream.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "toolbox-server starting on stdio"
    );

    // ── Configuration via environment variables ───────────────────────────────
    let name =
        std::env::var("TOOLBOX_SERVER_NAME").unwrap_or_else(|_| "toolbox-server".to_string());

    let version = std::env::var("TOOLBOX_SERVER_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());

    let instructions = std::env::var("TOOLBOX_SERVER_INSTRUCTIONS").ok();

    // ── Build and start the Toolbox ───────────────────────────────────────────
    let mut builder = Toolbox::builder()
        .name(name)
        .version(version)
        // readonly filesystem
        .with_sync_tool::<ReadFileTool>()
        .with_sync_tool::<ListDirTool>()
        .with_sync_tool::<FileMetadataTool>()
        // http
        .with_async_tool::<HttpFetchTool>()
        // process info
        .with_sync_tool::<EnvVarsTool>()
        .with_sync_tool::<WorkingDirTool>()
        .with_sync_tool::<HostnameTool>();

    if let Some(inst) = instructions {
        builder = builder.instructions(inst);
    }

    let service = builder.build().start().await?;

    tracing::info!("toolbox-server ready, waiting for client");

    service.waiting().await?;

    tracing::info!("toolbox-server shutting down");
    Ok(())
}
