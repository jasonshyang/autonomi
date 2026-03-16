use std::sync::Arc;

use rmcp::{
    handler::server::{
        router::tool::{AsyncTool, SyncTool},
        tool::{ToolCallContext, ToolRouter},
    },
    model::{
        Implementation, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo,
    },
    service::{RequestContext, RunningService},
    ErrorData, RoleServer, ServerHandler,
};

use crate::ServerError;

// ── Toolbox ───────────────────────────────────────────────────────────────────

/// An assembled MCP server.
///
/// `S` is the shared state type. Use the default (`S = ()`) for stateless
/// servers, or supply your own type and access it from tools via
/// [`Toolbox::state`].
///
/// Implements [`rmcp::ServerHandler`]. Build one with [`Toolbox::builder()`],
/// then call [`Toolbox::serve()`] to start listening on stdio.
pub struct Toolbox<S = ()> {
    router: ToolRouter<Self>,
    server_info: ServerInfo,
    state: Arc<S>,
}

impl<S: Send + Sync + 'static> Toolbox<S> {
    /// Return a reference to the shared state.
    ///
    /// Call this from inside a `SyncTool` or `AsyncTool` implementation to
    /// access the value supplied via [`ToolboxBuilder::with_state`].
    pub fn state(&self) -> &S {
        &self.state
    }

    /// Serve this [`Toolbox`] over **stdio** (stdin / stdout).
    ///
    /// Stdout must stay clean of any non-JSON-RPC bytes, so ensure tracing
    /// output is directed to stderr before calling this.
    pub async fn start(self) -> Result<RunningService<RoleServer, Self>, ServerError> {
        let transport = (tokio::io::stdin(), tokio::io::stdout());
        let service = rmcp::serve_server(self, transport).await?;

        Ok(service)
    }
}

impl Toolbox<()> {
    /// Start building a new [`Toolbox`].
    pub fn builder() -> ToolboxBuilder<()> {
        ToolboxBuilder::new()
    }
}

impl<S: Send + Sync + 'static> ServerHandler for Toolbox<S> {
    fn get_info(&self) -> ServerInfo {
        self.server_info.clone()
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(self.router.list_all())))
    }

    fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<rmcp::model::CallToolResult, ErrorData>> + Send + '_
    {
        let ctx = ToolCallContext::new(self, request, context);
        async move { self.router.call(ctx).await }
    }

    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        self.router.get(name).cloned()
    }
}

// ── ToolboxBuilder ────────────────────────────────────────────────────────────

/// Fluent builder for [`Toolbox<S>`].
pub struct ToolboxBuilder<S: Send + Sync + 'static = ()> {
    name: String,
    version: String,
    instructions: Option<String>,
    state: Arc<S>,
    router: ToolRouter<Toolbox<S>>,
}

impl ToolboxBuilder<()> {
    fn new() -> Self {
        Self {
            name: "Toolbox-server".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            instructions: None,
            state: Arc::new(()),
            router: ToolRouter::new(),
        }
    }
}

impl<S: Send + Sync + 'static> ToolboxBuilder<S> {
    /// Set the server name reported to MCP clients.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the server version string.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set the human-readable instructions shown to MCP clients.
    pub fn instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// Register a synchronous tool.
    ///
    /// `T` must implement `SyncTool<Toolbox<S>>`.
    pub fn with_sync_tool<T>(mut self) -> Self
    where
        T: SyncTool<Toolbox<S>> + 'static,
    {
        self.router = self.router.with_sync_tool::<T>();
        self
    }

    /// Register an asynchronous tool.
    ///
    /// `T` must implement `AsyncTool<Toolbox<S>>`.
    pub fn with_async_tool<T>(mut self) -> Self
    where
        T: AsyncTool<Toolbox<S>> + 'static,
    {
        self.router = self.router.with_async_tool::<T>();
        self
    }

    /// Finish building and return the ready-to-serve [`Toolbox<S>`].
    pub fn build(self) -> Toolbox<S> {
        Toolbox {
            router: self.router,
            server_info: make_server_info(&self.name, &self.version, self.instructions.as_deref()),
            state: self.state,
        }
    }
}

impl ToolboxBuilder<()> {
    /// Attach shared state, transitioning the builder to [`ToolboxBuilder<S>`].
    ///
    /// After this call, register tools that implement `SyncTool<Toolbox<S>>`
    /// or `AsyncTool<Toolbox<S>>`, then call `.build()`.
    pub fn with_state<S: Send + Sync + 'static>(self, state: Arc<S>) -> ToolboxBuilder<S> {
        ToolboxBuilder {
            name: self.name,
            version: self.version,
            instructions: self.instructions,
            state,
            router: ToolRouter::new(),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_server_info(name: &str, version: &str, instructions: Option<&str>) -> ServerInfo {
    let caps = ServerCapabilities::builder().enable_tools().build();
    let server_impl = Implementation::new(name, version);
    let mut info = ServerInfo::new(caps).with_server_info(server_impl);
    if let Some(inst) = instructions {
        info = info.with_instructions(inst);
    }
    info
}
