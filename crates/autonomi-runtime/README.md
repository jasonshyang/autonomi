# autonomi-runtime

Orchestration layer for the Autonomi framework. This crate drives one or more `autonomi-agent` agents concurrently, manages their lifecycle, routes prompts, and broadcasts observable events over a shared bus. It is the execution environment that agents run inside.

## Overview

`autonomi-runtime` provides:

- **`Runtime`** — the central orchestrator. Spawns agents into dedicated async tasks, routes prompts to them, and handles graceful shutdown.
- **`RuntimeBuilder`** — a fluent builder for constructing a `Runtime` with agents and hooks registered up front.
- **`EventBus`** — a broadcast channel that publishes `RuntimeEvent`s (turn completions, errors, stops) to any number of independent subscribers.
- **`Hook`** — a composable async callback trait invoked at key points in every agent loop (pre-prompt, post-completion, error).
- **`Shared<T>`** — a lock-free, cheaply-cloneable wrapper around `arc-swap` used internally for wait-free hook registry reads across concurrent agent tasks.

## Getting started

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
autonomi-runtime = { path = "../crates/autonomi-runtime" }
autonomi-agent   = { path = "../crates/autonomi-agent" }
```

Build a runtime, register agents, and start sending prompts:

```rust
use autonomi_agent::{agents::ResearchAgent, provider::Provider};
use autonomi_runtime::Runtime;
use rig::providers::openai;

#[tokio::main]
async fn main() {
    let agent = ResearchAgent::build(Provider::openai(openai::GPT_4O), vec![]);

    let mut runtime = Runtime::builder()
        .register(agent)
        .build();

    let id = runtime.agent_id("researcher").unwrap();

    // Fire-and-forget.
    runtime.send(&id, "What is the current state of fusion energy research?".into()).await.unwrap();

    // Or await the full response.
    let response = runtime.prompt(&id, "Summarize quantum computing advances in 2024.".into()).await.unwrap();
    println!("{response}");

    runtime.shutdown_all().await.unwrap();
}
```

## Core types

### Runtime and RuntimeBuilder

`RuntimeBuilder` is the entry point. Register agents with `.register()`, attach global hooks with `.hook()`, and call `.build()` to spawn all agent loops and get back a running `Runtime`.

Agents can also be spawned dynamically after build with `Runtime::spawn()`.

Each registered agent is assigned an `AgentId` derived from the agent's name. If the same name is registered more than once, subsequent instances receive a numeric suffix (`name-1`, `name-2`, ...).

### Messaging

| Method | Behavior |
|---|---|
| `runtime.send(&id, prompt)` | Enqueues a prompt, returns immediately |
| `runtime.prompt(&id, prompt)` | Enqueues a prompt, awaits the full response |
| `runtime.reset_history(&id)` | Clears the agent's conversation history |
| `runtime.shutdown(&id)` | Gracefully stops a single agent loop |
| `runtime.shutdown_all()` | Gracefully stops all agents and joins their tasks |

### EventBus

Every agent loop publishes structured events to a shared broadcast channel. Subscribe with `runtime.subscribe()` to receive an independent receiver. Slow subscribers that fall behind by more than the configured capacity receive a `Lagged` error rather than stalling any agent.

```rust
let mut rx = runtime.subscribe();
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        println!("{event:?}");
    }
});
```

Events:

| Variant | When |
|---|---|
| `TurnComplete { agent_id, response, usage }` | A prompt turn finished successfully |
| `AgentError { agent_id, error }` | A recoverable error occurred in the loop |
| `AgentStopped { agent_id, reason }` | The agent loop exited (gracefully or fatally) |

### Hooks

Hooks are async callbacks invoked at lifecycle points inside every agent loop. Implement the `Hook` trait and override only the methods you need.

```rust
use autonomi_runtime::hooks::{Hook, PrePromptContext, PostCompletionContext, ErrorContext};

struct LoggingHook;

#[async_trait::async_trait]
impl Hook for LoggingHook {
    async fn on_pre_prompt(&self, ctx: &mut PrePromptContext) -> Result<(), autonomi_runtime::HookError> {
        println!("[{}] prompt: {}", ctx.agent_id, ctx.prompt);
        Ok(())
    }

    async fn on_post_completion(&self, ctx: &PostCompletionContext) -> Result<(), autonomi_runtime::HookError> {
        println!("[{}] response: {}", ctx.agent_id, ctx.response);
        Ok(())
    }
}
```

Register globally (all agents) or scoped to a specific agent:

```rust
// At build time — applies to every agent.
let runtime = Runtime::builder()
    .register(agent)
    .hook(LoggingHook)
    .build();

// After build — wait-free, does not block running agents.
runtime.register_hook(LoggingHook);
runtime.register_hook_for(id, LoggingHook);
```

Hooks run in registration order. A `PrePromptContext` hook that returns `Err` cancels the current turn and emits an `AgentError` event. Post-completion and error hook failures are logged as warnings but do not affect the turn outcome.

The `tracing-hook` feature (enabled by default) includes a built-in `TracingHook` that logs all lifecycle events via the `tracing` crate.

## Concurrency model

Each agent runs in its own `tokio::task`. The runtime never blocks one agent waiting for another. Hook registrations after build use `Shared<T>` (backed by `arc-swap`) for wait-free reads — running agent tasks snapshot the hook list atomically without acquiring a lock and without being interrupted by concurrent registrations.

## Feature flags

| Feature | Default | Description |
|---|---|---|
| `tracing-hook` | yes | Enables the built-in `TracingHook` |