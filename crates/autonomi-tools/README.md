# autonomi-tools

A composable toolkit for building [MCP](https://modelcontextprotocol.io/) servers from individually
implemented tools, and connecting to them from [rig](https://github.com/0xPlaygrounds/rig) agents.

## Overview

The crate has two sides:

**Server side** — `Toolbox` is an MCP server builder. Register tools against it, call `.start()`,
and it serves the MCP protocol over stdio. The included `toolbox-server` binary ships with three
built-in tool groups pre-registered and ready to use.

**Client side** — `ToolboxClient` connects to any running MCP server, fetches its tool list, and
returns a `Vec<Box<dyn ToolDyn>>` that slots directly into any rig agent via `.tools(...)`.

```text
rig Agent
  └── ToolboxClient
        └── MCP JSON-RPC (stdio)
              └── toolbox-server
                    ├── readonly_filesystem  (read_file, list_dir, file_metadata)
                    ├── http_fetch           (http_fetch)
                    └── process_info         (env_vars, working_dir, hostname)
```

## Built-in tools

### readonly_filesystem

Read-only access to the local filesystem. Writing is not supported.

| Tool            | Description                                          |
|-----------------|------------------------------------------------------|
| `read_file`     | Read the full UTF-8 contents of a file               |
| `list_dir`      | List entries inside a directory                      |
| `file_metadata` | Stat a path: size, kind, read-only flag, timestamps  |

### http_fetch

| Tool         | Description                                          |
|--------------|------------------------------------------------------|
| `http_fetch` | Perform an HTTP GET and return the status and body   |

### process_info

Read-only access to the server process's environment.

| Tool          | Description                                          |
|---------------|------------------------------------------------------|
| `env_vars`    | List environment variables, with optional prefix filter |
| `working_dir` | Return the current working directory                 |
| `hostname`    | Return the machine hostname                          |

## Usage

### Spawn the server and connect

```rust
use autonomi_tools::client::{SpawnConfig, ToolboxClient};
use rig::{client::CompletionClient, completion::Prompt, providers::ollama};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = ToolboxClient::spawn(SpawnConfig::new("/path/to/toolbox-server")).await?;

    let agent = ollama::Client::new(rig::client::Nothing)?
        .agent("qwen3:4b")
        .preamble("You are a helpful assistant.")
        .tools(client.tools())
        .build();

    println!("{}", agent.prompt("What files are in the current directory?").await?);
    Ok(())
}
```

### Connect to an already-running server

```rust
use autonomi_tools::client::ToolboxClient;
use rmcp::transport::child_process::TokioChildProcess;

let transport = TokioChildProcess::new(tokio::process::Command::new("./toolbox-server"))?;
let client = ToolboxClient::connect((), transport).await?;
```

### Build a custom server

Implement `ToolBase` and `SyncTool` (or `AsyncTool`) on your tool struct, then register it with
the builder:

```rust
use std::borrow::Cow;
use autonomi_tools::{toolbox::Toolbox, ToolBase, SyncTool};
use rmcp::{schemars, ErrorData};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct EchoParams { message: String }

#[derive(Serialize, schemars::JsonSchema)]
struct EchoOutput { echoed: String }

struct EchoTool;

impl ToolBase for EchoTool {
    type Parameter = EchoParams;
    type Output    = EchoOutput;
    type Error     = ErrorData;
    fn name() -> Cow<'static, str> { "echo".into() }
    fn description() -> Option<Cow<'static, str>> { Some("Echo a message back.".into()) }
}

impl SyncTool<Toolbox> for EchoTool {
    fn invoke(_ctx: &Toolbox, p: EchoParams) -> Result<EchoOutput, ErrorData> {
        Ok(EchoOutput { echoed: p.message })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Toolbox::builder()
        .name("my-server")
        .with_sync_tool::<EchoTool>()
        .build()
        .start()
        .await?
        .waiting()
        .await?;
    Ok(())
}
```

## toolbox-server configuration

The `toolbox-server` binary reads the following environment variables at startup:

| Variable                      | Default          | Description                              |
|-------------------------------|------------------|------------------------------------------|
| `TOOLBOX_SERVER_NAME`         | `toolbox-server` | Server name reported to MCP clients      |
| `TOOLBOX_SERVER_VERSION`      | crate version    | Version string reported to MCP clients   |
| `TOOLBOX_SERVER_INSTRUCTIONS` | *(none)*         | Human-readable instructions for clients  |
| `RUST_LOG`                    | `info`           | Tracing filter (output goes to stderr)   |

These can be set via `SpawnConfig::with_env` when spawning from a client:

```rust
let client = ToolboxClient::spawn(
    SpawnConfig::new("/path/to/toolbox-server")
        .with_env("TOOLBOX_SERVER_NAME", "my-server")
        .with_env("RUST_LOG", "warn"),
).await?;
```

## Example

See [`examples/toolbox_client.rs`](examples/toolbox_client.rs) for a full walkthrough that spawns
the server and runs three sequential prompts — one per tool group.

```bash
cargo build --bin toolbox-server
cargo run --example toolbox_client
```
