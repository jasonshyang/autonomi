# autonomi-agent

Core agent abstractions for the Autonomi framework. This crate defines the building blocks used to construct, configure, and run LLM-backed agents. It is provider-agnostic and designed to be composed with `autonomi-runtime` for orchestration.

## Overview

`autonomi-agent` provides three things:

- **`AgentConfig`** — a struct that describes an agent's identity, system preamble, and sampling parameters. Load it from a TOML file or parse it from a string at runtime.
- **`Agent<M>`** — a concrete struct that couples a `rig` model backend with an `AgentConfig` and a set of tools, producing a multi-turn conversational agent.
- **`Provider`** — a factory for constructing `rig` `AgentBuilder`s from supported LLM providers using API keys read from the environment.

## Concepts

### AgentConfig

`AgentConfig` is a plain Rust struct derived from `serde::Deserialize`. Define an agent by writing a TOML file — no Rust code required. Load it at startup with `AgentConfig::from_file`.

Only `name` and `preamble` are required. All other fields are optional and fall back to the provider's own defaults when absent.

#### TOML shape

```toml
name     = "summarizer"
preamble = "You are a concise summarization assistant."

temperature = 0.2        # optional — sampling temperature
max_tokens  = 4096       # optional — maximum tokens to generate
max_turns   = 10         # optional — max tool-call rounds per prompt turn

# optional — zero or more static context snippets injected on every request
additional_context = [
    "Always respond in plain text, no markdown.",
]
```

#### Loading a config

```rust
use autonomi_agent::AgentConfig;

// from a file on disk
let config = AgentConfig::from_file("agents/summarizer.toml")?;

// or from a string (useful in tests)
let config = AgentConfig::from_toml_str(r#"
    name     = "summarizer"
    preamble = "You are a concise summarization assistant."
    temperature = 0.2
"#)?;
```

### Agent

`Agent<M>` wraps a `rig` model builder together with an `AgentConfig` and any registered tools. It maintains no state itself — conversation history is passed in and mutated on each call, making it straightforward to manage history externally or reset it on demand.

```rust
use autonomi_agent::{Agent, AgentConfig, provider::Provider};
use rig::providers::openai;

let config = AgentConfig::from_file("agents/summarizer.toml")?;
let agent  = Agent::new(Provider::openai(openai::GPT_4O), config, vec![]);

let mut history = vec![];
let response = agent.prompt("Summarize the Rust ownership model.", &mut history).await?;
```

### Provider

`Provider` is a static factory. Each method instantiates the appropriate `rig` client from environment variables and returns a bare `AgentBuilder`. Pass the builder to `Agent::new` to apply the config and tools.

| Method | Environment variable |
|---|---|
| `Provider::openai(model)` | `OPENAI_API_KEY` |
| `Provider::anthropic(model)` | `ANTHROPIC_API_KEY` |
| `Provider::ollama(model)` | *(no key — connects to `localhost:11434`)* |
| `Provider::gemini(model)` | `GEMINI_API_KEY` |
| `Provider::groq(model)` | `GROQ_API_KEY` |
| `Provider::cohere(model)` | `COHERE_API_KEY` |

## Built-in agent definitions

Ready-to-use TOML configs live in [`agents/`](../../agents) at the workspace root:

| File | Name | Purpose |
|---|---|---|
| `agents/research.toml` | `researcher` | Systematic research and information synthesis |
| `agents/demo.toml` | `demo` | Codebase exploration via filesystem tools |
| `agents/recall.toml` | `recall` | Answering questions from episodic memory |

Load any of them the same way:

```rust
let config = AgentConfig::from_file("agents/research.toml")?;
let agent  = Agent::new(Provider::openai(openai::GPT_4O), config, vec![]);
```

## Relationship to autonomi-runtime

`autonomi-agent` is a pure construction and execution layer. It has no concept of task queues, lifecycle management, or event broadcasting. Those concerns belong to `autonomi-runtime`, which accepts any `RunAgent` implementation — the object-safe interface automatically derived for every `Agent<M>` — and drives it inside a managed async task loop.