//! # demo_load_memory
//!
//! Demonstrates loading a previously-persisted memory file and using it to
//! answer questions that require recall of past sessions.
//!
//! ## What this demo does
//!
//! 1. Restores a [`MemoryStore`] from `demo_memory.json` (written by `demo`)
//!    via [`MemoryWorker::spawn_from_file`] — **no re-embedding happens**;
//!    vectors are read straight from disk.
//! 2. Prints a summary of every entry loaded from the file so you can see what
//!    the agent actually remembers.
//! 3. Performs a raw semantic search against the restored store with a
//!    representative query so you can observe the scored results before the
//!    agent even runs.
//! 4. Loads the recall agent config from `agents/recall.toml` and builds an
//!    agent that uses the restored store as `dynamic_context`, so recalled
//!    memories are injected into every prompt automatically.
//! 5. Sends the agent a task that can only be answered correctly if it
//!    successfully recalls the prior session (e.g. "what did the agent
//!    previously summarise about autonomi-utils?").
//! 6. Saves the updated memory (now containing this session's turns too) back
//!    to disk, extending the file for future runs.
//!
//! ## Prerequisites
//!
//! - Run `demo` at least once so that `demo_memory.json` exists in the
//!   workspace root.
//! - Ollama running locally with `qwen3:4b` and `nomic-embed-text` pulled.
//! - The `basic-toolbox` binary must be built and on `$PATH` (or pointed to via
//!   `BASIC_TOOLBOX_BIN`).
//!
//! ```sh
//! cargo build --release -p toolbox
//! BASIC_TOOLBOX_BIN=./target/release/basic-toolbox cargo run --bin demo_load_memory
//! ```

use std::{env, path::Path};

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

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const COMPLETION_MODEL: &str = "qwen3:4b";

/// Path to the memory file produced (and consumed) by this demo pair.
const MEMORY_FILE: &str = ".memory/demo_memory.json";

/// How many top-scored memories to inject into each prompt automatically.
const DYNAMIC_CONTEXT_N: usize = 5;

/// The task sent to the agent — deliberately requires recall of the prior run.
const TASK: &str = "\
    Without using any filesystem tools, answer the following questions \
    purely from your recalled memory of previous sessions: \
    \n\n\
    1. What crate did you explore in a previous session, and what does it do? \
    2. What was the exact one-sentence summary you produced for that crate? \
    \n\n\
    If you cannot find the answer in your memory, say so explicitly.";

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // -----------------------------------------------------------------------
    // 1. Initialise tracing
    // -----------------------------------------------------------------------
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .init();

    tracing::info!("demo_load_memory starting");

    // -----------------------------------------------------------------------
    // 2. Spawn the basic-toolbox MCP server (kept here so the agent can optionally
    //    look things up, but the task is designed to be answered from memory alone)
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
    // 3. Build the shared Ollama client (completions + embeddings)
    // -----------------------------------------------------------------------
    tracing::info!("initialising shared Ollama client");
    let ollama_client = Provider::ollama_client()
        .ok_or_else(|| anyhow::anyhow!("failed to initialise Ollama HTTP client"))?;

    // -----------------------------------------------------------------------
    // 4. Restore memory from disk
    //
    //    spawn_from_file reads the JSON, reconstructs every MemoryEntry, and
    //    re-hydrates the raw embedding vectors stored alongside them — no API
    //    calls required.  If the file does not exist the worker starts empty.
    // -----------------------------------------------------------------------
    let memory_path = Path::new(MEMORY_FILE);

    tracing::info!(
        path = %memory_path.display(),
        exists = memory_path.exists(),
        "restoring memory store from file"
    );

    let embedding_model = ollama_client.embedding_model(ollama::NOMIC_EMBED_TEXT);
    let memory_handle = MemoryWorker::spawn_from_file(embedding_model, memory_path, 256)
        .await
        .map_err(|e| anyhow::anyhow!("failed to restore memory from {MEMORY_FILE}: {e}"))?;

    // -----------------------------------------------------------------------
    // 5. Print a human-readable summary of everything loaded from the file
    // -----------------------------------------------------------------------
    println!("\n=== LOADED MEMORIES ===");
    println!("(path: {MEMORY_FILE})\n");

    // We do a broad search with a very generic query to surface all entries.
    // In a real app you might expose a `list_all()` method; here we reuse
    // the existing `search` API with a high result cap.
    let all_entries = memory_handle.search("", 1000).await.unwrap_or_default();

    if all_entries.is_empty() {
        println!("  (no memories found — run the `demo` binary first)\n");
    } else {
        for (i, (score, entry)) in all_entries.iter().enumerate() {
            let preview: String = entry
                .content
                .chars()
                .take(120)
                .collect::<String>()
                .replace('\n', " ");
            println!(
                "  [{i}] id={id}  agent={agent}  turn={turn}  ts={ts}  score={score:.4}\n       \
                 preview: {preview}…\n",
                id = entry.id,
                agent = entry.agent_id,
                turn = entry.turn,
                ts = entry.timestamp_secs,
                score = score,
                preview = preview,
            );
        }
    }

    // -----------------------------------------------------------------------
    // 6. Raw semantic search — show scored hits before the agent even runs
    // -----------------------------------------------------------------------
    let probe = "autonomi-utils crate summary";
    println!("=== SEMANTIC SEARCH PROBE ===");
    println!("Query: \"{probe}\"\n");

    let hits = memory_handle
        .search(probe, DYNAMIC_CONTEXT_N)
        .await
        .unwrap_or_default();

    if hits.is_empty() {
        println!("  (no hits)\n");
    } else {
        for (score, entry) in &hits {
            let preview: String = entry
                .content
                .chars()
                .take(200)
                .collect::<String>()
                .replace('\n', " ");
            println!("  score={score:.4}  id={id}\n  {preview}…\n", id = entry.id);
        }
    }

    // -----------------------------------------------------------------------
    // 7. Load the recall agent config from TOML and build the agent. The restored
    //    memory store is wired as dynamic_context so recalled memories are injected
    //    into every prompt automatically.
    // -----------------------------------------------------------------------
    tracing::info!(model = COMPLETION_MODEL, "loading recall agent config");
    let config = AgentConfig::from_file("agents/recall.toml")
        .map_err(|e| anyhow::anyhow!("failed to load agents/recall.toml: {e}"))?;

    let agent_name = config.name.clone();

    let builder = ollama_client
        .agent(COMPLETION_MODEL)
        .dynamic_context(DYNAMIC_CONTEXT_N, memory_handle.clone());

    let agent = Agent::new(builder, config, tools);

    // -----------------------------------------------------------------------
    // 8. Assemble the runtime
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

    tracing::info!(%agent_id, "runtime built");

    // -----------------------------------------------------------------------
    // 9. Background event logger
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
    // 10. Send the recall task and print the response
    // -----------------------------------------------------------------------
    println!("=== TASK ===\n{TASK}\n");
    tracing::info!("sending recall task to agent");

    let response = runtime
        .prompt(&agent_id, TASK.to_string())
        .await
        .map_err(|e| anyhow::anyhow!("prompt failed: {e}"))?;

    println!("\n=== AGENT RESPONSE ===\n{response}\n");

    // -----------------------------------------------------------------------
    // 11. Persist the extended memory (previous turns + this session's turns)
    // -----------------------------------------------------------------------
    tracing::info!(path = MEMORY_FILE, "saving updated memory");
    if let Err(e) = memory_handle.save(MEMORY_FILE).await {
        tracing::warn!(error = %e, "failed to save memory (non-fatal)");
    }

    // -----------------------------------------------------------------------
    // 12. Graceful shutdown
    // -----------------------------------------------------------------------
    tracing::info!("shutting down runtime");
    runtime.shutdown_all().await?;
    memory_handle.shutdown().await;

    tracing::info!("demo_load_memory done");
    Ok(())
}
