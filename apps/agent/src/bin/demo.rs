//! # demo
//!
//! End-to-end demonstration of the autonomi agent stack:
//!
//! 1. Spins up the `basic-toolbox` MCP server as a child process.
//! 2. Connects a [`ToolboxClient`] and pulls its tool list (includes
//!    `http_fetch`, filesystem tools, and process-info tools).
//! 3. Creates a memory worker backed by Ollama `nomic-embed-text`.
//! 4. Loads the demo agent config from `agents/demo.toml` and builds an agent
//!    powered by Ollama `qwen3:4b`.
//! 5. Wires everything into a [`Runtime`] with a [`MemoryHook`] and a
//!    [`TracingHook`].
//! 6. Sends the agent a simple task: navigate the repo, find the autonomi-agent
//!    README and report a one-sentence summary.
//! 7. Prints the response, then shuts the runtime down cleanly.
//!
//! ## Prerequisites
//!
//! - Ollama running locally with `qwen3:4b` and `nomic-embed-text` pulled.
//! - The `basic-toolbox` binary must be built and discoverable.  Either put it
//!   on `$PATH` or point to it with `BASIC_TOOLBOX_BIN`:
//!
//! ```sh
//! cargo build --release -p toolbox
//! BASIC_TOOLBOX_BIN=./target/release/basic-toolbox cargo run --bin demo
//! ```

use std::env;

use autonomi_agent::{provider::Provider, Agent, AgentConfig};
use autonomi_memory::MemoryWorker;
use autonomi_runtime::{
    hooks::{memory::MemoryHook, tracing::TracingHook},
    Runtime, RuntimeEvent, StopReason,
};
use autonomi_tools::{client::ToolboxClient, SpawnConfig};
use rig::{
    client::{CompletionClient, EmbeddingsClient},
    providers::ollama,
};

// The Ollama model used for completions.
const COMPLETION_MODEL: &str = "qwen3:4b";

// The task we give the agent.
const TASK: &str = "\
    Starting from the workspace root, use the list_dir and read_file tools to \
    navigate the directory tree and locate the autonomi-utils crate. \
    Read its README.md, then respond with exactly one sentence summarising what \
    autonomi-utils does.";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // -----------------------------------------------------------------------
    // 1. Initialise tracing (logs go to stderr so stdout stays clean)
    // -----------------------------------------------------------------------
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .init();

    tracing::info!("demo agent starting");

    // -----------------------------------------------------------------------
    // 2. Spawn the basic-toolbox MCP server and collect its tools
    // -----------------------------------------------------------------------
    let toolbox_bin = env::var("BASIC_TOOLBOX_BIN").unwrap_or_else(|_| "basic-toolbox".to_string());

    tracing::info!(binary = %toolbox_bin, "spawning basic-toolbox MCP server");

    let spawn_config = SpawnConfig::new(&toolbox_bin);

    let mut toolbox_client = ToolboxClient::spawn(spawn_config)
        .await
        .map_err(|e| anyhow::anyhow!("failed to spawn toolbox server: {e}"))?;

    let tools = toolbox_client.take_tools();
    tracing::info!(count = tools.len(), "loaded tools from toolbox");

    // -----------------------------------------------------------------------
    // 3. Create a single Ollama client shared by memory (embedding) and the
    //    completion agent.  Both use the same underlying HTTP connection pool.
    // -----------------------------------------------------------------------
    tracing::info!("initialising shared Ollama client");
    let ollama_client = Provider::ollama_client()
        .ok_or_else(|| anyhow::anyhow!("failed to initialise Ollama HTTP client"))?;

    // -----------------------------------------------------------------------
    // 4a. Memory worker — nomic-embed-text via the shared client
    // -----------------------------------------------------------------------
    tracing::info!("spawning memory worker (nomic-embed-text)");
    let embedding_model = ollama_client.embedding_model(ollama::NOMIC_EMBED_TEXT);
    let memory_handle = MemoryWorker::spawn_default(embedding_model);

    // -----------------------------------------------------------------------
    // 4b. Load the demo agent config from TOML and build the agent.
    //     qwen3:4b completion via the same Ollama client, with memory wired
    //     as dynamic context so past turns are retrieved and injected into
    //     each prompt automatically.
    // -----------------------------------------------------------------------
    tracing::info!(model = COMPLETION_MODEL, "loading demo agent config");
    let config = AgentConfig::from_file("agents/demo.toml")
        .map_err(|e| anyhow::anyhow!("failed to load agents/demo.toml: {e}"))?;

    let agent_name = config.name.clone();

    let builder = ollama_client
        .agent(COMPLETION_MODEL)
        .dynamic_context(5, memory_handle.clone());
    let agent = Agent::new(builder, config, tools);

    // -----------------------------------------------------------------------
    // 5. Assemble the runtime with hooks
    // -----------------------------------------------------------------------
    let memory_hook = MemoryHook::new(memory_handle.clone());
    let tracing_hook = TracingHook::new();

    let mut runtime = Runtime::builder()
        .register(agent)
        .hook(tracing_hook)
        .hook(memory_hook)
        .build();

    let agent_id = runtime
        .agent_id(&agent_name)
        .ok_or_else(|| anyhow::anyhow!("agent '{agent_name}' not found in runtime"))?;

    tracing::info!(%agent_id, "runtime built, agent registered");

    // -----------------------------------------------------------------------
    // 6. Subscribe to runtime events and log them in the background
    // -----------------------------------------------------------------------
    let mut events = runtime.subscribe();

    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(RuntimeEvent::TurnComplete { agent_id, usage, .. }) => {
                    tracing::info!(
                        %agent_id,
                        total_tokens = usage.as_ref().map(|u| u.total_tokens),
                        "turn complete"
                    );
                },
                Ok(RuntimeEvent::AgentError { agent_id, error }) => {
                    tracing::warn!(%agent_id, %error, "agent error");
                },
                Ok(RuntimeEvent::AgentStopped { agent_id, reason }) => {
                    let label = match reason {
                        StopReason::Requested => "requested".to_string(),
                        StopReason::Fatal(msg) => format!("fatal: {msg}"),
                    };
                    tracing::info!(%agent_id, reason = %label, "agent stopped");
                    break;
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "event bus lagged");
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // -----------------------------------------------------------------------
    // 7. Send the task and await the full response
    // -----------------------------------------------------------------------
    tracing::info!("sending task to agent");
    println!("\n=== TASK ===\n{TASK}\n");

    let response = runtime
        .prompt(&agent_id, TASK.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("prompt failed: {e}"))?;

    println!("\n=== AGENT RESPONSE ===\n{response}\n");

    // -----------------------------------------------------------------------
    // 8. Persist memory and shut everything down cleanly
    // -----------------------------------------------------------------------
    tracing::info!("saving memory to .memory/demo_memory.json");
    if let Err(e) = memory_handle.save(".memory/demo_memory.json").await {
        tracing::warn!(error = %e, "failed to save memory (non-fatal)");
    }

    tracing::info!("shutting down runtime");
    runtime.shutdown_all().await?;
    memory_handle.shutdown().await;

    tracing::info!("demo agent done");
    Ok(())
}
