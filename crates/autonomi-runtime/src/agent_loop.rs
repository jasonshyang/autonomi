use autonomi_agent::BoxedAgent;
use rig::message::Message;
use tokio::sync::{broadcast, mpsc};

use crate::{
    event::{RuntimeEvent, StopReason},
    hooks::{ErrorContext, HookRegistry, PostCompletionContext, PrePromptContext},
    runtime::{AgentId, AgentInput},
    shared::Shared,
};

/// The per-agent async task.  Runs until a `Shutdown` message is received or
/// the input channel is dropped.
pub(crate) async fn run_agent_loop(
    id: AgentId,
    agent: BoxedAgent,
    mut input_rx: mpsc::Receiver<AgentInput>,
    event_tx: broadcast::Sender<RuntimeEvent>,
    hooks: Shared<HookRegistry>,
) {
    tracing::info!(
        agent_id = %id,
        "agent loop starting"
    );

    let mut history: Vec<Message> = Vec::new();

    loop {
        let input = match input_rx.recv().await {
            Some(input) => input,
            // Sender was dropped — runtime shut down without an explicit signal.
            None => {
                event_tx
                    .send(RuntimeEvent::AgentStopped {
                        agent_id: id.clone(),
                        reason: StopReason::Requested,
                    })
                    .ok();

                break;
            },
        };

        match input {
            AgentInput::Shutdown => {
                event_tx
                    .send(RuntimeEvent::AgentStopped {
                        agent_id: id.clone(),
                        reason: StopReason::Requested,
                    })
                    .ok();

                break;
            },

            AgentInput::Reset => {
                history.clear();
                continue;
            },

            AgentInput::Prompt { prompt, reply_tx } => {
                // Snapshot hooks for this turn — wait-free, no lock held across awaits.
                let hook_list = hooks.read(|r| r.snapshot_for(&id));

                // --- Pre-prompt hooks ---
                let mut pre_ctx = PrePromptContext { agent_id: id.clone(), prompt: prompt.clone() };

                let mut cancelled = false;
                for hook in &hook_list {
                    if let Err(e) = hook.on_pre_prompt(&mut pre_ctx).await {
                        event_tx
                            .send(RuntimeEvent::AgentError {
                                agent_id: id.clone(),
                                error: format!("pre_prompt hook failed: {e}"),
                            })
                            .ok();
                        cancelled = true;

                        break;
                    }
                }
                if cancelled {
                    continue;
                }

                // Use the (potentially rewritten) prompt from the hook context.
                let effective_prompt = pre_ctx.prompt;

                // --- Invoke the plugin ---
                match agent.process(&effective_prompt, &mut history).await {
                    Ok(response) => {
                        // Post-completion hooks (non-cancelling).
                        let post_ctx = PostCompletionContext {
                            agent_id: id.clone(),
                            prompt: effective_prompt,
                            response: response.clone(),
                            usage: None,
                        };
                        for hook in &hook_list {
                            if let Err(e) = hook.on_post_completion(&post_ctx).await {
                                tracing::warn!(
                                    agent_id = %id,
                                    error = %e,
                                    "post_completion hook failed"
                                );
                            }
                        }

                        event_tx
                            .send(RuntimeEvent::TurnComplete {
                                agent_id: id.clone(),
                                response: response.clone(),
                                usage: None,
                            })
                            .ok();

                        if let Some(tx) = reply_tx {
                            tx.send(response).ok();
                        }
                    },

                    Err(err) => {
                        let err_str = err.to_string();
                        let is_fatal = err.is_fatal();

                        let err_ctx =
                            ErrorContext { agent_id: id.clone(), error: err_str.clone(), is_fatal };
                        for hook in &hook_list {
                            hook.on_error(&err_ctx).await.ok();
                        }

                        if is_fatal {
                            event_tx
                                .send(RuntimeEvent::AgentStopped {
                                    agent_id: id.clone(),
                                    reason: StopReason::Fatal(err_str),
                                })
                                .ok();

                            break;
                        } else {
                            event_tx
                                .send(RuntimeEvent::AgentError {
                                    agent_id: id.clone(),
                                    error: err_str,
                                })
                                .ok();
                        }
                    },
                }
            },
        }
    }

    tracing::info!(
        agent_id = %id,
        "agent loop exiting"
    );
}
