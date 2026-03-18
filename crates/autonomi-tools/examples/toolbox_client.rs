use std::path::PathBuf;

use autonomi_tools::client::{SpawnConfig, ToolboxClient};
use rig::{client::CompletionClient, completion::Prompt, providers::ollama};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .init();

    // ── 1. Spawn the toolbox-server ───────────────────────────────────────────
    //
    // Examples land in `target/{profile}/examples/` while regular binaries land
    // in `target/{profile}/`.  Step up one level to find `toolbox-server`.
    let server_bin: PathBuf = std::env::current_exe()?
        .parent() // .../target/debug/examples
        .and_then(|p| p.parent()) // .../target/debug
        .expect("could not resolve target directory")
        .join("toolbox-server");

    tracing::info!(bin = %server_bin.display(), "spawning toolbox-server");

    let mut client = ToolboxClient::spawn(SpawnConfig::new(server_bin)).await?;

    tracing::info!("MCP handshake complete");

    // ── 2. Build the agent (tools are shared across all prompts) ─────────────
    let tools = client.take_tools();
    tracing::info!(count = tools.len(), "tools registered");

    let ollama = ollama::Client::new(rig::client::Nothing)?;

    let agent = ollama
        .agent("qwen3:4b")
        .preamble(
            "You are a helpful assistant with access to several tools.
             Always use the available tools to answer rather than guessing. Be concise.",
        )
        .tools(tools)
        .build();

    // ── 3. Filesystem prompt ──────────────────────────────────────────────────
    println!("\n═══ Filesystem ═══════════════════════════════════════════\n");

    // Use the workspace root (two levels up from this example's manifest dir)
    // so the LLM always sees a directory that actually contains a Cargo.toml.
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // autonomi/crates
        .and_then(|p| p.parent()) // autonomi/
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().expect("no current dir"));

    let fs_prompt = format!(
        "List the files and directories inside '{}', then read the contents \
         of Cargo.toml found there.",
        workspace_root.display()
    );

    tracing::info!(%fs_prompt, "sending filesystem prompt");
    let fs_response = agent.prompt(fs_prompt.as_str()).await?;
    println!("{fs_response}\n");

    // ── 4. HTTP fetch prompt ──────────────────────────────────────────────────
    println!("═══ HTTP Fetch ════════════════════════════════════════════\n");

    let http_prompt = "Fetch https://www.google.com and summarise the response — \
         what is the HTTP status code, and what does the page appear to be about?";

    tracing::info!(%http_prompt, "sending HTTP prompt");
    let http_response = agent.prompt(http_prompt).await?;
    println!("{http_response}\n");

    // ── 5. Process info prompt ────────────────────────────────────────────────
    println!("═══ Process Info ══════════════════════════════════════════\n");

    let proc_prompt = "What is the hostname of this machine, what is the current working \
         directory, and are there any environment variables that start with \
         'CARGO'? List them.";

    tracing::info!(%proc_prompt, "sending process-info prompt");
    let proc_response = agent.prompt(proc_prompt).await?;
    println!("{proc_response}\n");

    Ok(())
}
