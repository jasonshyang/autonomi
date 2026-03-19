# autonomi-agent

Core agent abstractions for the Autonomi framework. This crate defines the building blocks used to construct, configure, and run LLM-backed agents. It is provider-agnostic and designed to be composed with `autonomi-runtime` for orchestration.

## Overview

`autonomi-agent` provides three things:

- **`AgentConfig`** — a trait that describes an agent's identity, system preamble, sampling parameters, and optional response post-processing.
- **`Agent<M, C>`** — a concrete struct that couples a `rig` model backend with an `AgentConfig` and a set of tools, producing a multi-turn conversational agent.
- **`Provider`** — a factory for constructing `rig` `AgentBuilder`s from supported LLM providers using API keys read from the environment.

It also ships a **`ResearchAgent`** as a ready-to-use reference implementation.

## Concepts

### AgentConfig

`AgentConfig` is a compile-time description of an agent. Implement it to define the agent's name, system prompt, and optional settings such as temperature, max tokens, max tool-call turns per prompt, and static context snippets.

```rust
use autonomi_agent::AgentConfig;

struct SummarizerConfig;

impl AgentConfig for SummarizerConfig {
    fn name(&self) -> &str { "summarizer" }
    fn preamble(&self) -> &str { "You are a concise summarization assistant." }
    fn temperature(&self) -> Option<f64> { Some(0.2) }
}
```

### Agent

`Agent<M, C>` wraps a `rig` model builder together with an `AgentConfig` and any registered tools. It maintains no state itself — conversation history is passed in and mutated on each call, making it straightforward to manage history externally or reset it on demand.

```rust
use autonomi_agent::{Agent, provider::Provider};
use rig::providers::openai;

let agent = Agent::new(Provider::openai(openai::GPT_4O), SummarizerConfig, vec![]);
let mut history = vec![];
let response = agent.prompt("Summarize the Rust ownership model.", &mut history).await?;
```

### Provider

`Provider` is a static factory. Each method instantiates the appropriate `rig` client from environment variables and returns a bare `AgentBuilder`. Pass the builder to `Agent::new` to apply config and tools.

| Method | Environment variable |
|---|---|
| `Provider::openai(model)` | `OPENAI_API_KEY` |
| `Provider::anthropic(model)` | `ANTHROPIC_API_KEY` |
| `Provider::ollama(model)` | `OLLAMA_HOST` |
| `Provider::gemini(model)` | `GEMINI_API_KEY` |
| `Provider::groq(model)` | `GROQ_API_KEY` |
| `Provider::cohere(model)` | `COHERE_API_KEY` |

### Built-in agents

#### ResearchAgent

A pre-configured agent tuned for systematic research and information synthesis. It breaks questions into sub-questions, reasons through evidence, flags uncertainty, and returns structured summaries.

```rust
use autonomi_agent::agents::ResearchAgent;
use autonomi_agent::provider::Provider;
use rig::providers::openai;

let agent = ResearchAgent::build(Provider::openai(openai::GPT_4O), vec![]);
```

## Relationship to autonomi-runtime

`autonomi-agent` is a pure construction and execution layer. It has no concept of task queues, lifecycle management, or event broadcasting. Those concerns belong to `autonomi-runtime`, which accepts any `RunAgent` implementation — the object-safe interface automatically derived for every `Agent<M, C>` — and drives it inside a managed async task loop.