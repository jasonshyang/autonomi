use std::path::Path;

use rig::{
    agent::AgentBuilder,
    completion::{CompletionModel, Prompt},
    tool::ToolDyn,
};
use serde::Deserialize;

use crate::{AgentError, AgentResult};

// ---------------------------------------------------------------------------
// AgentConfig
// ---------------------------------------------------------------------------

/// Full configuration for an [`Agent`], loadable from a TOML file or string.
///
/// Only `name` and `preamble` are required; all other fields are optional and
/// fall back to sensible defaults (or the provider's own defaults) when absent.
///
/// # TOML shape
///
/// ```toml
/// name     = "researcher"
/// preamble = "You are a thorough research assistant."
///
/// temperature = 0.3        # optional
/// max_tokens  = 4096       # optional
/// max_turns   = 10         # optional
///
/// # Zero or more extra context snippets injected on every request.
/// additional_context = [
///     "Always cite your sources.",
/// ]
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    /// The display name of this agent.
    ///
    /// Used as the basis for `AgentId` generation in the runtime. If two
    /// agents share the same name, subsequent ones receive a numeric suffix
    /// (`name-1`, `name-2`, …).
    pub name: String,

    /// The system preamble (instructions) sent to the model on every turn.
    pub preamble: String,

    /// Sampling temperature. `None` defers to the provider default.
    #[serde(default)]
    pub temperature: Option<f64>,

    /// Maximum number of tokens to generate. `None` defers to the provider
    /// default.
    #[serde(default)]
    pub max_tokens: Option<u64>,

    /// Maximum number of agentic tool-call rounds per prompt turn before the
    /// agent returns. `None` defers to the rig default.
    #[serde(default)]
    pub max_turns: Option<usize>,

    /// Additional static context snippets injected into every request.
    ///
    /// Each element is passed as a separate `.context()` call on the builder,
    /// so documents remain distinct in the context window.
    #[serde(default)]
    pub additional_context: Vec<String>,
}

impl AgentConfig {
    /// Load an [`AgentConfig`] from a TOML file on disk.
    ///
    /// # Errors
    ///
    /// Returns an [`AgentError::Fatal`] if the file cannot be read or if the
    /// TOML cannot be parsed into an [`AgentConfig`].
    pub fn from_file(path: impl AsRef<Path>) -> AgentResult<Self> {
        let raw = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            AgentError::fatal(std::io::Error::new(
                e.kind(),
                format!("failed to read agent config '{}': {e}", path.as_ref().display()),
            ))
        })?;
        Self::from_toml_str(&raw)
    }

    /// Parse an [`AgentConfig`] from a TOML string.
    ///
    /// # Errors
    ///
    /// Returns an [`AgentError::Fatal`] if the TOML is invalid or missing
    /// required fields.
    pub fn from_toml_str(s: &str) -> AgentResult<Self> {
        toml::from_str(s).map_err(|e| AgentError::fatal(e))
    }
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

/// A concrete agent that couples a rig [`AgentBuilder<M>`] with an
/// [`AgentConfig`] and a set of tools, building the underlying
/// [`rig::agent::Agent<M>`] internally.
///
/// `M` is the rig completion model.
///
/// # Example
///
/// ```rust,ignore
/// use autonomi_agent::{Agent, AgentConfig, provider::Provider};
/// use rig::providers::openai;
///
/// let config = AgentConfig::from_file("agents/research.toml")?;
/// let agent  = Agent::new(Provider::openai(openai::GPT_4O), config, vec![]);
/// ```
pub struct Agent<M: CompletionModel> {
    inner: rig::agent::Agent<M>,
    config: AgentConfig,
}

impl<M: CompletionModel> Agent<M> {
    /// Build the agent from an [`AgentBuilder<M>`], a config, and any tools.
    ///
    /// All config fields (`preamble`, `temperature`, `max_tokens`, `max_turns`,
    /// `additional_context`) are applied to `builder` before calling
    /// `.build()`. Tools are registered via `.tools()`.
    pub fn new(
        builder: AgentBuilder<M>,
        config: AgentConfig,
        tools: Vec<Box<dyn ToolDyn>>,
    ) -> Self {
        let mut b = builder.preamble(&config.preamble);

        if let Some(t) = config.temperature {
            b = b.temperature(t);
        }
        if let Some(n) = config.max_tokens {
            b = b.max_tokens(n);
        }
        if let Some(n) = config.max_turns {
            b = b.default_max_turns(n);
        }
        for ctx in &config.additional_context {
            b = b.context(ctx);
        }

        let inner = if tools.is_empty() { b.build() } else { b.tools(tools).build() };

        Self { inner, config }
    }

    /// The display name of this agent, as reported by its configuration.
    pub fn name(&self) -> &str { &self.config.name }

    /// Execute one prompt turn, mutating `history` in place.
    ///
    /// Forwards the prompt and history to the underlying rig agent via
    /// `.with_history(history)` to maintain multi-turn context.
    pub async fn prompt(
        &self,
        prompt: &str,
        history: &mut Vec<rig::message::Message>,
    ) -> AgentResult<String> {
        self.inner
            .prompt(prompt)
            .with_history(history)
            .await
            .map_err(AgentError::recoverable)
    }
}

// ---------------------------------------------------------------------------
// RunAgent
// ---------------------------------------------------------------------------

/// Type alias for a boxed [`RunAgent`] trait object used by the runtime.
pub type BoxedAgent = Box<dyn RunAgent>;

/// Object-safe interface used by the runtime for dynamic agent dispatch.
///
/// This trait is automatically implemented for any [`Agent<M>`]; you never
/// need to implement it manually.
#[async_trait::async_trait]
pub trait RunAgent: Send + Sync + 'static {
    /// The display name of this agent.
    fn name(&self) -> &str;

    /// Execute one prompt turn, mutating `history` in place.
    async fn process(
        &self,
        prompt: &str,
        history: &mut Vec<rig::message::Message>,
    ) -> AgentResult<String>;
}

#[async_trait::async_trait]
impl<M> RunAgent for Agent<M>
where
    M: CompletionModel + Send + Sync + 'static,
{
    fn name(&self) -> &str { Agent::name(self) }

    async fn process(
        &self,
        prompt: &str,
        history: &mut Vec<rig::message::Message>,
    ) -> AgentResult<String> {
        self.prompt(prompt, history).await
    }
}
