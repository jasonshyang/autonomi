use rig::{
    agent::AgentBuilder,
    completion::{CompletionModel, Prompt},
    tool::ToolDyn,
};

use crate::{AgentError, AgentResult};

// ---------------------------------------------------------------------------
// AgentConfig
// ---------------------------------------------------------------------------

/// Provides a compile time full configuration for a concrete [`Agent`].
///
/// Only `name` and `preamble` are required.
///
/// # Example
///
/// ```rust,ignore
/// use autonomi_agent::{AgentConfig, AgentResult};
///
/// struct ResearchConfig;
///
/// impl AgentConfig for ResearchConfig {
///     fn name(&self) -> &str { "researcher" }
///     fn preamble(&self) -> &str { "You are a thorough research assistant." }
///     fn temperature(&self) -> Option<f64> { Some(0.3) }
/// }
/// ```
pub trait AgentConfig: Send + Sync + 'static {
    /// The display name of this agent.
    ///
    /// Used as the basis for `AgentId` generation in the runtime. If two
    /// agents share the same name, subsequent ones receive a numeric suffix
    /// (`name-1`, `name-2`, …).
    fn name(&self) -> &str;

    /// The system preamble (instructions) sent to the model on every turn.
    fn preamble(&self) -> &str;

    /// Sampling temperature. `None` defers to the provider default.
    fn temperature(&self) -> Option<f64> { None }

    /// Maximum number of tokens to generate. `None` defers to the provider
    /// default.
    fn max_tokens(&self) -> Option<u64> { None }

    /// Maximum number of agentic tool-call rounds per prompt turn before the
    /// agent returns. `None` defers to the rig default.
    fn max_turns(&self) -> Option<usize> { None }

    /// Additional static context snippets injected into every request.
    ///
    /// Each element is passed as a separate `.context()` call on the builder,
    /// so documents remain distinct in the context window.
    fn additional_context(&self) -> Vec<String> { vec![] }

    /// Called with the raw string response from the LLM before returning it.
    ///
    /// Override to post-process, validate, or transform the output.
    /// Defaults to passing the response through unchanged.
    fn handle_result(&self, result: String) -> AgentResult<String> { Ok(result) }
}

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

/// A concrete agent that couples a rig [`AgentBuilder<M>`] with a typed
/// configuration [`C`] and a set of tools, building the underlying
/// [`rig::agent::Agent<M>`] internally.
///
/// `M` is the rig completion model; `C` provides the full agent config via
/// [`AgentConfig`].
///
/// # Example
///
/// ```rust,ignore
/// use autonomi_agent::{Agent, provider::Provider};
/// use rig::providers::openai;
///
/// let builder = Provider::openai(openai::GPT_4O);
/// let agent = Agent::new(builder, ResearchConfig, vec![]);
/// ```
pub struct Agent<M: CompletionModel, C: AgentConfig> {
    inner: rig::agent::Agent<M>,
    config: C,
}

impl<M: CompletionModel, C: AgentConfig> Agent<M, C> {
    /// Build the agent from an [`AgentBuilder<M>`], a config, and any tools.
    ///
    /// All config fields (`preamble`, `temperature`, `max_tokens`, `max_turns`,
    /// `additional_context`) are applied to `builder` before calling
    /// `.build()`. Tools are registered via `.tools()`.
    pub fn new(builder: AgentBuilder<M>, config: C, tools: Vec<Box<dyn ToolDyn>>) -> Self {
        let mut b = builder.preamble(config.preamble());

        if let Some(t) = config.temperature() {
            b = b.temperature(t);
        }
        if let Some(n) = config.max_tokens() {
            b = b.max_tokens(n);
        }
        if let Some(n) = config.max_turns() {
            b = b.default_max_turns(n);
        }
        for ctx in config.additional_context() {
            b = b.context(&ctx);
        }

        let inner = if tools.is_empty() { b.build() } else { b.tools(tools).build() };

        Self { inner, config }
    }

    /// The display name of this agent, as reported by its configuration.
    pub fn name(&self) -> &str { self.config.name() }

    /// Execute one prompt turn, mutating `history` in place.
    ///
    /// Forwards the prompt and history to the underlying rig agent via
    /// `.with_history(history)` to maintain multi-turn context, then passes
    /// the raw response through [`AgentConfig::handle_result`].
    pub async fn prompt(
        &self,
        prompt: &str,
        history: &mut Vec<rig::message::Message>,
    ) -> AgentResult<String> {
        let result = self
            .inner
            .prompt(prompt)
            .with_history(history)
            .await
            .map_err(AgentError::recoverable)?;
        self.config.handle_result(result)
    }
}

// ---------------------------------------------------------------------------
// RunAgent
// ---------------------------------------------------------------------------

/// Type alias for a boxed [`RunAgent`] trait object used by the runtime.
pub type BoxedAgent = Box<dyn RunAgent>;

/// Object-safe interface used by the runtime for dynamic agent dispatch.
///
/// This trait is automatically implemented for any [`Agent<M, C>`]; you never
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
impl<M, C> RunAgent for Agent<M, C>
where
    M: CompletionModel + Send + Sync + 'static,
    C: AgentConfig,
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
