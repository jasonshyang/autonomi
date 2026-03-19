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
