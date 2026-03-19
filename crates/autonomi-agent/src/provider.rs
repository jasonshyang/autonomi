//! Provider factory for building rig [`AgentBuilder`]s.
//!
//! [`Provider`] exposes one associated function per supported LLM provider.
//! Each returns an `AgentBuilder<M>` with no preamble or config applied —
//! those are the responsibility of the [`AgentConfig`][crate::AgentConfig]
//! passed to [`Agent::new`][crate::Agent::new].
//!
//! # Example
//!
//! ```rust,ignore
//! use autonomi_agent::{Agent, provider::Provider};
//! use rig::providers::openai;
//!
//! let agent = Agent::new(Provider::openai(openai::GPT_4O), MyConfig, vec![]);
//! ```

use rig::{
    agent::AgentBuilder,
    client::{CompletionClient, ProviderClient},
    providers::{anthropic, cohere, gemini, groq, ollama, openai},
};

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Factory for creating provider-specific rig [`AgentBuilder`]s.
///
/// Each method reads the relevant API key from the environment and returns a
/// bare `AgentBuilder<M>`. Pass the builder to
/// [`Agent::new`][crate::Agent::new] together with an
/// [`AgentConfig`][crate::AgentConfig] to apply the preamble and all other
/// settings.
pub struct Provider;

impl Provider {
    /// `AgentBuilder` backed by OpenAI's completions API.
    /// Reads `OPENAI_API_KEY` from the environment.
    pub fn openai(model: &str) -> AgentBuilder<openai::completion::CompletionModel> {
        openai::Client::from_env()
            .completions_api()
            .agent(model)
    }

    /// `AgentBuilder` backed by Anthropic.
    /// Reads `ANTHROPIC_API_KEY` from the environment.
    pub fn anthropic(model: &str) -> AgentBuilder<anthropic::completion::CompletionModel> {
        anthropic::Client::from_env().agent(model)
    }

    /// `AgentBuilder` backed by a local Ollama instance.
    /// Reads `OLLAMA_HOST` from the environment (defaults to `http://localhost:11434`).
    pub fn ollama(model: &str) -> AgentBuilder<ollama::CompletionModel> {
        ollama::Client::from_env().agent(model)
    }

    /// `AgentBuilder` backed by Google Gemini.
    /// Reads `GEMINI_API_KEY` from the environment.
    pub fn gemini(model: &str) -> AgentBuilder<gemini::CompletionModel> {
        gemini::Client::from_env().agent(model)
    }

    /// `AgentBuilder` backed by Groq.
    /// Reads `GROQ_API_KEY` from the environment.
    pub fn groq(model: &str) -> AgentBuilder<groq::CompletionModel> {
        groq::Client::from_env().agent(model)
    }

    /// `AgentBuilder` backed by Cohere.
    /// Reads `COHERE_API_KEY` from the environment.
    pub fn cohere(model: &str) -> AgentBuilder<cohere::CompletionModel> {
        cohere::Client::from_env().agent(model)
    }
}
