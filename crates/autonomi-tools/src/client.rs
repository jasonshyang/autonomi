use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use rmcp::ClientHandler;
use rmcp::{
    model::{CallToolRequestParams, CallToolResult, RawContent, Tool as McpTool},
    service::{RunningService, ServerSink},
    transport::child_process::TokioChildProcess,
    RoleClient, ServiceExt,
};

use crate::ClientError;

#[derive(Debug, Clone, Default)]
pub struct SpawnConfig {
    /// Path to the `toolbox-server` binary.
    ///
    /// `None` means "resolve automatically next to the current executable".
    pub binary_path: Option<PathBuf>,

    /// Extra command-line arguments forwarded verbatim to the server process.
    pub args: Vec<String>,

    /// Environment variables overlaid on the server process's inherited
    /// environment.  The child inherits the parent's env; these are added on
    /// top (or override existing keys).
    pub env: Vec<(String, String)>,
}

impl SpawnConfig {
    /// Use an explicit path to the server binary.
    pub fn new(binary_path: impl Into<PathBuf>) -> Self {
        Self {
            binary_path: Some(binary_path.into()),
            ..Default::default()
        }
    }

    /// Append a command-line argument that will be passed to the server.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Set an environment variable on the server process.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Resolve the binary path, falling back to auto-resolution when none was
    /// provided.
    fn build_command(&self) -> Result<tokio::process::Command, ClientError> {
        let binary = self.resolve_binary()?;
        let mut cmd = tokio::process::Command::new(binary);
        cmd.args(&self.args);
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        Ok(cmd)
    }

    fn resolve_binary(&self) -> Result<PathBuf, ClientError> {
        if let Some(ref path) = self.binary_path {
            return Ok(path.clone());
        }

        // Auto-resolve: look next to the current executable.
        let exe = std::env::current_exe().map_err(|e| ClientError::ResolveBinary(e.to_string()))?;

        let path = exe
            .parent()
            .ok_or_else(|| {
                ClientError::ResolveBinary("current executable has no parent directory".to_string())
            })?
            .join("toolbox-server");

        Ok(path)
    }
}

// ── ToolboxClient ─────────────────────────────────────────────────────────────

pub struct ToolboxClient<H: ClientHandler> {
    /// Held to keep the MCP connection open.
    service: RunningService<RoleClient, H>,
    /// The wrapped tools, ready for `.tools(client.tools())`.
    tools: Vec<Box<dyn ToolDyn>>,
}

impl ToolboxClient<()> {
    /// Spawn a `toolbox-server` child process and connect to it over stdio.
    pub async fn spawn(config: SpawnConfig) -> Result<Self, ClientError> {
        let cmd = config.build_command()?;
        let transport = TokioChildProcess::new(cmd).map_err(ClientError::SpawnProcess)?;
        ToolboxClient::connect((), transport).await
    }
}

impl<H: ClientHandler> ToolboxClient<H> {
    /// Connect to an MCP server over `transport`, perform the MCP handshake,
    /// and fetch the initial tool list.
    pub async fn connect<T, E, A>(handler: H, transport: T) -> Result<Self, ClientError>
    where
        T: rmcp::transport::IntoTransport<RoleClient, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let service: RunningService<RoleClient, H> = handler.serve(transport).await?;
        let sink = service.peer();

        let tools = sink
            .list_all_tools()
            .await?
            .into_iter()
            .map(|tool| {
                Box::new(McpToolAdaptor {
                    tool,
                    server: sink.clone(),
                }) as Box<dyn ToolDyn>
            })
            .collect();

        Ok(Self { service, tools })
    }

    /// Return a clone of the underlying [`ServerSink`] for advanced use cases
    /// such as calling tools directly or inspecting server state.
    pub fn server_sink(&self) -> ServerSink {
        self.service.peer().clone()
    }

    /// Fetch the latest tool list from the server, replacing any previously fetched tools.
    pub async fn fetch_tools(&mut self) -> Result<(), ClientError> {
        let sink = self.service.peer();

        let tools = sink
            .list_all_tools()
            .await?
            .into_iter()
            .map(|tool| {
                Box::new(McpToolAdaptor {
                    tool,
                    server: sink.clone(),
                }) as Box<dyn ToolDyn>
            })
            .collect();

        self.tools = tools;
        Ok(())
    }

    /// Return the list of tools fetched from the server, consuming them from the client.
    ///
    /// Do not consume the Client so the server can remain connected.
    pub fn take_tools(&mut self) -> Vec<Box<dyn ToolDyn>> {
        if self.tools.is_empty() {
            tracing::warn!("take_tools called but no tools are registered");
        }

        std::mem::take(&mut self.tools)
    }
}

// ── McpToolAdaptor ────────────────────────────────────────────────────────────

/// Bridges a single rmcp [`McpTool`] definition + [`ServerSink`] into rig's
/// [`ToolDyn`] trait so it can live inside any rig agent's tool list.
struct McpToolAdaptor {
    tool: McpTool,
    server: ServerSink,
}

impl ToolDyn for McpToolAdaptor {
    fn name(&self) -> String {
        self.tool.name.to_string()
    }

    fn definition(
        &self,
        _prompt: String,
    ) -> Pin<Box<dyn Future<Output = ToolDefinition> + Send + '_>> {
        Box::pin(std::future::ready(ToolDefinition {
            name: self.name(),
            description: self
                .tool
                .description
                .as_deref()
                .unwrap_or_default()
                .to_string(),
            parameters: self.tool.schema_as_json_value(),
        }))
    }

    fn call(
        &self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + '_>> {
        let server = self.server.clone();
        let name = self.tool.name.clone();

        Box::pin(async move {
            // The LLM produces a JSON object string; parse it into the map
            // that `with_arguments` expects.
            let arguments = match serde_json::from_str::<serde_json::Value>(&args)
                .map_err(ToolError::JsonError)?
            {
                serde_json::Value::Object(map) => map,
                other => {
                    // Wrap scalars / arrays under an "input" key as a fallback.
                    let mut m = serde_json::Map::new();
                    m.insert("input".to_string(), other);
                    m
                }
            };

            let result: CallToolResult = server
                .call_tool(CallToolRequestParams::new(name).with_arguments(arguments))
                .await
                .map_err(|e| ToolError::ToolCallError(Box::new(e)))?;

            Ok(call_tool_result_to_string(result))
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Flatten every content piece of an MCP [`CallToolResult`] into one string.
fn call_tool_result_to_string(result: CallToolResult) -> String {
    result
        .content
        .into_iter()
        .map(|c| match c.raw {
            RawContent::Text(t) => t.text,
            other => serde_json::to_string(&other).unwrap_or_default(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
