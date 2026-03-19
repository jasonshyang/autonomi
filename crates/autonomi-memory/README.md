# autonomi-memory

Persistent episodic memory for the Autonomi framework. This crate gives agents the ability to remember past conversation turns across sessions by embedding them into a vector store and retrieving the most semantically relevant context at query time.

## Overview

`autonomi-memory` provides:

- **`MemoryWorker`** — spawns a background task that exclusively owns the vector store. All embedding API calls, cosine similarity searches, and file I/O run inside this task so callers are never blocked.
- **`MemoryHandle`** — a cheap-to-clone, non-generic handle to a running `MemoryWorker`. All interaction with the store happens through message-passing — there is no shared state and no locks. Also implements `VectorStoreIndex` so it can be passed directly to `rig`'s `dynamic_context()` builder.
- **`MemoryStore`** — the in-process vector store. Holds `(MemoryEntry, Embedding)` pairs and performs cosine-similarity search over them. Owned exclusively by the worker task.
- **`MemoryEntry`** — a single stored conversation turn. Contains the embedded text, the originating agent id, a unix timestamp, and a monotonically increasing turn number.

## Getting started

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
autonomi-memory  = { path = "../crates/autonomi-memory" }
autonomi-runtime = { path = "../crates/autonomi-runtime" }
autonomi-agent   = { path = "../crates/autonomi-agent" }
```

Spawn a worker, wire it into an agent as both a hook and a dynamic context source, then run it inside a `Runtime`:

```rust
use autonomi_memory::MemoryWorker;
use autonomi_agent::Provider;
use autonomi_runtime::Runtime;
use rig::providers::openai;

#[tokio::main]
async fn main() {
    let openai = openai::Client::from_env();

    // One embedding model for the memory store.
    let embed_model = openai.embedding_model(openai::TEXT_EMBEDDING_ADA_002);

    // Spawn the background worker and get back a handle.
    let handle = MemoryWorker::spawn_default(embed_model);

    // Build an agent that reads the top-5 most relevant past turns as context
    // and writes every new turn back into the store automatically.
    let agent = Provider::openai(openai::GPT_4O)
        .preamble("You are a helpful assistant with long-term memory.")
        .dynamic_context(5, handle.clone())   // MemoryHandle implements VectorStoreIndex
        .build();

    let mut runtime = Runtime::builder()
        .register(agent)
        .build();

    let id = runtime.agent_id("assistant").unwrap();

    let response = runtime.prompt(&id, "What do you remember about me?".into()).await.unwrap();
    println!("{response}");

    // Persist the store so it survives restarts.
    handle.save("memory.json").await.unwrap();

    handle.shutdown().await;
    runtime.shutdown_all().await.unwrap();
}
```

### Resuming from a previous session

Use `MemoryWorker::spawn_from_file` to restore the store without re-embedding anything. If the file does not yet exist a fresh empty store is used, so no special first-run handling is required:

```rust
let handle = MemoryWorker::spawn_from_file(embed_model, "memory.json".as_ref(), 256)
    .await
    .unwrap();
```

## Core types

### MemoryWorker

`MemoryWorker` is the entry point. It owns the `MemoryStore` and exposes three constructor functions:

| Constructor | Description |
|---|---|
| `MemoryWorker::spawn(model, capacity)` | Spawn a fresh empty store with the given inbox capacity |
| `MemoryWorker::spawn_default(model)` | Spawn with the default inbox capacity (256) |
| `MemoryWorker::spawn_from_file(model, path, capacity)` | Restore from a JSON file, or start fresh if the file does not exist |

All three return a `MemoryHandle`. The `MemoryStore` is never accessible directly by callers after the worker is spawned.

### MemoryHandle

`MemoryHandle` is the only type callers interact with at runtime. It is `Clone` and cheap to pass around.

| Method | Behavior |
|---|---|
| `handle.add(agent_id, content)` | Fire-and-forget write. Non-blocking `try_send`; silently dropped if the inbox is full |
| `handle.search(query, n)` | Async request — embeds `query` and returns the top-`n` entries by cosine similarity |
| `handle.save(path)` | Async request — serializes the store to a JSON file at `path` |
| `handle.shutdown()` | Gracefully stop the worker task |

`MemoryHandle` also implements `rig::vector_store::VectorStoreIndex`, which means it can be passed directly to `agent_builder.dynamic_context(n, handle)` without any adapter.

### MemoryStore

`MemoryStore<E>` is the in-process vector store. It is generic over any `rig::embeddings::EmbeddingModel` and is exclusively owned by the worker task — callers never touch it directly.

Key operations:

- **`add(agent_id, content)`** — embeds `content` and appends a new `MemoryEntry`. All agents share one store; pass an empty string for `agent_id` if cross-agent retrieval without filtering is desired.
- **`search(query, n)`** — embeds `query` and returns the top-`n` entries sorted descending by cosine similarity score.

### MemoryEntry

A `MemoryEntry` represents a single stored conversation turn.

| Field | Type | Description |
|---|---|---|
| `id` | `String` | Unique id in the format `turn_{agent_id}_{turn_number}` |
| `content` | `String` | The embedded text (e.g. `"User: …\nAssistant: …"`) |
| `agent_id` | `String` | The agent that produced this turn |
| `timestamp_secs` | `u64` | Unix timestamp (seconds) when the turn was recorded |
| `turn` | `u64` | Monotonically increasing turn number across the whole store |

## Persistence

The store is serialized to **JSON** via `serde_json`. Embedding vectors are written alongside each entry so that `load_from_file` can restore the full store without making any API calls on startup.

```rust
// Save
handle.save("memory.json").await?;

// Restore (no API calls made — vectors are read from disk)
let handle = MemoryWorker::spawn_from_file(embed_model, "memory.json".as_ref(), 256).await?;
```

The parent directory of `path` must already exist. The file is overwritten atomically on each `save` call.

## Concurrency model

The worker task is the sole owner of the `MemoryStore`. All access is serialized through an MPSC channel with a configurable inbox capacity (default: 256).

- `add` uses a non-blocking `try_send`. If the inbox is full the write is silently dropped rather than stalling the caller — this is intentional so that an agent hook inside a latency-sensitive loop is never blocked by a slow embedding API.
- `search` and `save` are async request/reply calls backed by `tokio::sync::oneshot` channels. They await the worker's response.

Because there are no `Mutex` or `RwLock` guards on the hot path, multiple `MemoryHandle` clones can call `add` concurrently without any contention.

## Error handling

All fallible operations return `MemoryError`:

| Variant | Cause |
|---|---|
| `MemoryError::Embedding` | The embedding model returned an error |
| `MemoryError::Io` | A file read or write failed |
| `MemoryError::Serialization` | JSON serialization or deserialization failed |
| `MemoryError::WorkerGone` | The worker task has already exited and the channel is closed |