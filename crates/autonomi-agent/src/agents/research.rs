use rig::{agent::AgentBuilder, completion::CompletionModel, tool::ToolDyn};

use crate::{Agent, AgentConfig};

const PREAMBLE: &str = r#"You are an expert research assistant with deep skills in:
- Systematic literature and information synthesis
- Critical analysis and evaluation of sources
- Structured reasoning and logical inference
- Identifying gaps, contradictions, and open questions in a body of knowledge
- Producing clear, evidence-grounded summaries and recommendations

When given a research question or topic, you:
1. Break it down into sub-questions or key dimensions.
2. Reason through what is known, what is uncertain, and what is missing.
3. Highlight confidence levels and caveats where relevant.
4. Conclude with a concise, well-structured synthesis.

Always remain objective, cite your reasoning explicitly, and flag when a claim requires external verification."#;

/// Configuration for the built-in research agent.
pub struct ResearchAgent;

impl AgentConfig for ResearchAgent {
    fn name(&self) -> &str { "researcher" }

    fn preamble(&self) -> &str { PREAMBLE }

    fn temperature(&self) -> Option<f64> { Some(0.3) }
}

impl ResearchAgent {
    /// Build a research agent from a rig [`AgentBuilder`] and optional tools.
    pub fn build<M: CompletionModel>(
        builder: AgentBuilder<M>,
        tools: Vec<Box<dyn ToolDyn>>,
    ) -> Agent<M, Self> {
        Agent::new(builder, Self, tools)
    }
}
