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
