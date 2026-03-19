# Autonomi

Autonomi is a Rust workspace for building and running multi-agent LLM systems.

## Crates

### autonomi-agent

Defines what an agent is. Provides the `AgentConfig` trait for describing an agent's identity and system prompt, the `Agent<M, C>` struct for building a multi-turn conversational agent on top of any `rig`-compatible model, and the `Provider` factory for constructing model clients from environment variables. Ships a built-in `ResearchAgent` as a ready-to-use reference implementation.

See [`crates/autonomi-agent`](crates/autonomi-agent/README.md).

### autonomi-runtime

Drives agents. Accepts any number of `autonomi-agent` agents, spawns each into a dedicated `tokio` task, and manages prompt routing, conversation history, graceful shutdown, and event broadcasting. Exposes a `Hook` trait for attaching async callbacks at pre-prompt, post-completion, and error lifecycle points without blocking running agents.

See [`crates/autonomi-runtime`](crates/autonomi-runtime/README.md).

### autonomi-tools

A collection of reusable tools that can be passed to agents at construction time. Tools are compatible with the `rig` tool interface and cover common agentic use cases.

See [`crates/autonomi-tools`](crates/autonomi-tools/README.md).

### autonomi-memory

Persistent episodic memory for autonomi agents powered by `rig` RAG. Embeds conversation turns into dense vectors and stores them in-process. Exposes a `MemoryHandle` — a cheap-to-clone, message-passing facade to a background `MemoryWorker` task — with three operations: fire-and-forget `add`, async vector `search` (top-N by cosine similarity), and `save`/`load` to persist and restore the store from a JSON file without re-embedding.

See [`crates/autonomi-memory`](crates/autonomi-memory).

### autonomi-utils

Shared primitives used across the workspace.

- **`Shared<T>`** — a cheaply-cloneable, lock-free shared value built on [`arc_swap`](https://docs.rs/arc-swap). Reads are wait-free (single atomic pointer load); writes atomically swap in a mutated clone via a closure.
- **`Timestamp`** — a thin `u64` UTC Unix-seconds wrapper with `Serialize`/`Deserialize` support, used to timestamp memory entries and events.

See [`crates/autonomi-utils`](crates/autonomi-utils).

---

## Quick-start demo

The `demo` binary in [`apps/agent`](apps/agent/src/bin/demo.rs) shows the full stack end-to-end: an Ollama-backed agent with filesystem tools and episodic memory that navigates the repo and summarises a crate's README in one sentence.

### Dependencies

| Dependency | Purpose | Install |
|---|---|---|
| [Ollama](https://ollama.com) | Local LLM server | [ollama.com/download](https://ollama.com/download) |
| `qwen3:4b` model | Agent completions | `ollama pull qwen3:4b` |
| `nomic-embed-text` model | Memory embeddings | `ollama pull nomic-embed-text` |

### Run

```sh
ollama serve

cargo build --release -p toolbox

BASIC_TOOLBOX_BIN=./target/release/basic-toolbox cargo run --bin demo
```

The agent will use the filesystem tools to locate `crates/autonomi-agent/README.md` and print a one-sentence summary. Logs go to stderr; the agent response goes to stdout.
