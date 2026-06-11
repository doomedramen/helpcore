//! Chat and conversation handlers — the core AI chat loop, conversation CRUD, feedback, and compaction.
//!
//! All endpoints under `/api/chat` and `/api/conversations`.

use std::{
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use serde::Deserialize;

use helpcore_api::{
    ChatRequest, CompactResponse, ConfigField, ConversationSummary, GenerateTitleResponse,
    InteractionPayload, InteractionQuestion, InteractionQuestionType, InteractionResponse,
    MessageSummary, PendingInteraction, PluginProposal, QuestionAnswer, RenameConversationRequest,
    RetryRequest, SseChunk, SseContext, SseDone, SseInputRequired, SseStarted, SseToolCall,
    SseToolResult,
};

use crate::{
    api::{error::AppError, extractor::AuthUser},
    config::ProviderRole,
    conversation::{compact, context, context::ContextOptions, history, interaction, memory},
    plugins::{
        registry::{self, PluginCatalogItem},
        runtime::{ToolCatalog, is_interactive_tool},
    },
    providers::traits::ChatProvider,
    providers::types::{ChatMessage, ToolCall, Usage},
    state::AppState,
};

/// Maximum characters of a tool result forwarded to the model.
/// Results beyond this length are truncated to avoid exhausting the output token budget
/// when the model re-emits content in downstream tool calls.
const MAX_TOOL_RESULT_CHARS: usize = 10_000;
const MAX_TOOL_ERROR_CHARS: usize = 4_000;
const MAX_TOOL_ROUNDS: u32 = 8;
const TOOL_PHASE_TIMEOUT: Duration = Duration::from_secs(180);
const FINAL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const IDENTICAL_CALL_LIMIT: u32 = 3;
const MAX_FAILED_CALLS: u32 = 4;
const MAX_CONSECUTIVE_FAILED_ROUNDS: u32 = 2;

const FINAL_RESPONSE_FALLBACK: &str = "Sorry, I couldn't verify that with the available tools. I stopped after a reasonable attempt rather than continuing to retry.";

type EventSender = tokio::sync::mpsc::Sender<Result<Event, Infallible>>;

// ── POST /api/chat ─────────────────────────────────────────────────────────────

/// POST /api/chat — starts a new chat turn and streams the assistant's response via SSE.
///
/// Emits `started`, `chunk`, `tool_call`, `tool_result`, `done`, and `error` events.
pub async fn chat(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, AppError> {
    let provider = state
        .providers
        .find_for_role(req.provider_id.as_deref(), ProviderRole::Chat)
        .ok_or_else(|| {
            AppError::BadRequest(
                req.provider_id
                    .as_deref()
                    .map(|id| format!("unknown provider: {id}"))
                    .unwrap_or_else(|| "no chat providers configured".to_string()),
            )
        })?;

    let user_id = auth_user.id.clone();
    let user_role = auth_user.role;
    let conv_id_req = req.conversation_id.clone();
    let user_content = req.message.trim().to_string();
    if user_content.is_empty() {
        return Err(AppError::BadRequest("message cannot be empty".into()));
    }

    // Check provider grant before accepting the turn.
    let uid_for_grant = user_id.clone();
    let pid_for_grant = provider.id().to_string();
    let is_admin = user_role == crate::model::user::UserRole::Admin;
    let allowed = state
        .db
        .call(move |conn| {
            if is_admin {
                return Ok(true);
            }
            let denied = crate::model::provider_grant::denied_providers(conn, &uid_for_grant)?;
            Ok(!denied.contains(&pid_for_grant))
        })
        .await?;
    if !allowed {
        return Err(AppError::Forbidden);
    }

    // Load user preferences.
    let uid_prefs = user_id.clone();
    let (user_timezone, user_memory_leaning) = state
        .db
        .call(move |conn| {
            let user = crate::model::user::find_by_id(conn, &uid_prefs)?;
            match user {
                Some(u) => Ok((u.timezone, u.memory_leaning)),
                None => Ok(("UTC".to_string(), "moderate".to_string())),
            }
        })
        .await?;

    let provider_id = provider.id().to_string();
    let model_used = req
        .model
        .clone()
        .unwrap_or_else(|| provider.default_model().to_string());
    let provider_id_for_db = provider_id.clone();
    let model_for_db = model_used.clone();
    let content_for_db = user_content.clone();
    let uid_for_turn = user_id.clone();
    let started = state
        .db
        .call(move |conn| {
            history::start_turn(
                conn,
                &uid_for_turn,
                conv_id_req.as_deref(),
                &content_for_db,
                &provider_id_for_db,
                &model_for_db,
            )
        })
        .await
        .map_err(turn_error)?;

    let conversation_id = started.conversation.id.clone();
    let assistant_message_id = started.assistant_message_id.clone();
    let user_message_id = started.user_message.id.clone();
    let user_sequence = started.user_message.sequence;
    let history = started.history;
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);
    let started_event = Event::default()
        .event("started")
        .json_data(SseStarted {
            conversation_id: conversation_id.clone(),
            user_message_id,
            message_id: assistant_message_id.clone(),
        })
        .unwrap_or_else(|_| Event::default());
    let _ = tx.try_send(Ok(started_event));

    tokio::spawn(run_generation(GenerationJob {
        state,
        provider,
        conversation_id,
        assistant_message_id,
        user_id: user_id.clone(),
        user_content,
        user_sequence,
        user_timezone,
        user_memory_leaning,
        history,
        model_used,
        tx,
        resume_from_history: false,
    }));

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}

// ── POST /api/conversations/:id/messages/:message_id/retry ────────────────────

/// POST /api/conversations/{id}/messages/{message_id}/retry — retries a failed assistant message and streams the result via SSE.
pub async fn retry_message(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((conversation_id, message_id)): Path<(String, String)>,
    Json(body): Json<RetryRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, AppError> {
    let uid = auth_user.id.clone();
    let cid = conversation_id.clone();
    let mid = message_id.clone();

    let override_provider = if let Some(ref pid) = body.provider_id {
        let provider = state
            .providers
            .find_for_role(Some(pid), ProviderRole::Chat)
            .ok_or_else(|| AppError::BadRequest(format!("provider {pid} is not available")))?;
        Some((pid.clone(), provider.default_model().to_string()))
    } else {
        None
    };

    let (conversation, history, user_message, provider_id, model_used, _retry_error) = state
        .db
        .call(move |conn| {
            history::retry_turn(
                conn,
                &uid,
                &cid,
                &mid,
                override_provider
                    .as_ref()
                    .map(|(pid, m)| (pid.as_str(), m.as_str())),
            )
        })
        .await
        .map_err(turn_error)?;

    let provider = state
        .providers
        .find_for_role(Some(&provider_id), ProviderRole::Chat)
        .ok_or_else(|| AppError::BadRequest(format!("provider {provider_id} is not available")))?;

    let uid_prefs = auth_user.id.clone();
    let (user_timezone, user_memory_leaning) = state
        .db
        .call(move |conn| {
            let user = crate::model::user::find_by_id(conn, &uid_prefs)?;
            match user {
                Some(u) => Ok((u.timezone, u.memory_leaning)),
                None => Ok(("UTC".to_string(), "moderate".to_string())),
            }
        })
        .await?;

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);
    let started_event = Event::default()
        .event("started")
        .json_data(SseStarted {
            conversation_id: conversation.id.clone(),
            user_message_id: user_message.id.clone(),
            message_id: message_id.clone(),
        })
        .unwrap_or_else(|_| Event::default());
    let _ = tx.try_send(Ok(started_event));

    tokio::spawn(run_generation(GenerationJob {
        state,
        provider,
        conversation_id: conversation.id,
        assistant_message_id: message_id,
        user_id: auth_user.id,
        user_content: user_message.content,
        user_sequence: user_message.sequence,
        user_timezone,
        user_memory_leaning,
        history,
        model_used,
        tx,
        resume_from_history: false,
    }));

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}

struct GenerationJob {
    state: Arc<AppState>,
    provider: Arc<dyn ChatProvider>,
    conversation_id: String,
    assistant_message_id: String,
    user_id: String,
    user_content: String,
    user_sequence: i64,
    user_timezone: String,
    user_memory_leaning: String,
    history: Vec<MessageSummary>,
    model_used: String,
    tx: EventSender,
    /// Whether the current user turn and prior tool calls are already in history.
    resume_from_history: bool,
}

#[derive(Debug, Clone)]
struct ToolStop {
    code: &'static str,
    message: String,
}

struct ToolLoopGuard {
    deadline: Instant,
    rounds: u32,
    failed_calls: u32,
    consecutive_failed_rounds: u32,
    last_signature: Option<String>,
    identical_calls: u32,
}

impl ToolLoopGuard {
    fn new() -> Self {
        Self {
            deadline: Instant::now() + TOOL_PHASE_TIMEOUT,
            rounds: 0,
            failed_calls: 0,
            consecutive_failed_rounds: 0,
            last_signature: None,
            identical_calls: 0,
        }
    }

    fn remaining(&self) -> Option<Duration> {
        self.deadline.checked_duration_since(Instant::now())
    }

    fn before_provider(&self) -> Option<ToolStop> {
        if self.rounds >= MAX_TOOL_ROUNDS {
            return Some(ToolStop {
                code: "TOOL_ROUND_LIMIT",
                message: format!("The fixed limit of {MAX_TOOL_ROUNDS} tool rounds was reached."),
            });
        }
        if self.remaining().is_none_or(|remaining| remaining.is_zero()) {
            return Some(ToolStop {
                code: "TOOL_TIME_LIMIT",
                message: format!(
                    "The {} second tool-use time budget was reached.",
                    TOOL_PHASE_TIMEOUT.as_secs()
                ),
            });
        }
        None
    }

    fn inspect_calls(&mut self, calls: &[ToolCall]) -> Option<ToolStop> {
        for call in calls {
            let signature = format!("{}:{}", call.name, canonical_json(&call.arguments));
            if self.last_signature.as_deref() == Some(signature.as_str()) {
                self.identical_calls += 1;
            } else {
                self.last_signature = Some(signature);
                self.identical_calls = 1;
            }
            if self.identical_calls >= IDENTICAL_CALL_LIMIT {
                return Some(ToolStop {
                    code: "REPEATED_TOOL_CALL",
                    message: format!(
                        "The same `{}` call was requested {} times consecutively.",
                        call.name, IDENTICAL_CALL_LIMIT
                    ),
                });
            }
        }
        None
    }

    fn record_failure(&mut self, code: &str) -> Option<ToolStop> {
        self.failed_calls += 1;
        if matches!(
            code,
            "AUTH_ERROR" | "CONFIG_ERROR" | "UNSUPPORTED" | "POLICY_DENIED"
        ) {
            return Some(ToolStop {
                code: "TERMINAL_TOOL_ERROR",
                message: format!("Tool use stopped after terminal error `{code}`."),
            });
        }
        if self.failed_calls >= MAX_FAILED_CALLS {
            return Some(ToolStop {
                code: "TOOL_FAILURE_LIMIT",
                message: format!("Tool use stopped after {} failed calls.", MAX_FAILED_CALLS),
            });
        }
        None
    }

    fn record_round(&mut self, successes: usize) -> Option<ToolStop> {
        self.rounds += 1;
        if successes == 0 {
            self.consecutive_failed_rounds += 1;
        } else {
            self.consecutive_failed_rounds = 0;
        }
        if self.consecutive_failed_rounds >= MAX_CONSECUTIVE_FAILED_ROUNDS {
            return Some(ToolStop {
                code: "FAILED_TOOL_ROUNDS",
                message: format!(
                    "Tool use stopped after {} consecutive rounds without a successful call.",
                    MAX_CONSECUTIVE_FAILED_ROUNDS
                ),
            });
        }
        None
    }
}

fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(items) => {
            let items = items.iter().map(canonical_json).collect::<Vec<_>>();
            format!("[{}]", items.join(","))
        }
        serde_json::Value::Object(map) => {
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            let entries = entries
                .into_iter()
                .map(|(key, value)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap_or_default(),
                        canonical_json(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", entries.join(","))
        }
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Build the text used to recall relevant memories for this turn.
///
/// Searching on the new message alone misses short, contextual replies like
/// "yes" or "what's her number again?" — they carry no topical keywords of
/// their own. Folding in the most recent exchange (the last assistant reply
/// and the user message that prompted it) gives `search_memory` something to
/// match against. `search_memory` ORs its terms and ranks by relevance, so
/// adding more text only broadens recall — it never excludes a file that
/// would have matched on the new message alone.
fn memory_recall_query(history: &[MessageSummary], current_message: &str) -> String {
    let mut text = current_message.to_string();
    for msg in history
        .iter()
        .rev()
        .filter(|msg| msg.role == "user" || msg.role == "assistant")
        .take(2)
    {
        text.push(' ');
        text.push_str(&msg.content);
    }
    text
}

async fn run_generation(mut job: GenerationJob) {
    if let Err(error) = generate(&mut job).await {
        tracing::error!(error = %error, "chat generation failed");
        if let Err(persist_error) = fail_job(&job, error.to_string()).await {
            tracing::error!(error = %persist_error, "failed to persist chat generation failure");
        }
    }
}

/// Classify a tool error message into a stable, machine-readable code so the
/// model can decide whether to retry, fix its input, or give up.
fn tool_error_code(msg: &str) -> &'static str {
    let lower = msg.to_lowercase();
    if lower.contains("must not") || lower.contains("must be") || lower.contains("must contain") {
        "INVALID_INPUT"
    } else if lower.contains("'..'")
        || lower.contains("absolute")
        || lower.contains("escape")
        || lower.contains("traverse")
        || lower.contains("symlinks")
    {
        "INVALID_PATH"
    } else if lower.contains("command exited with status") {
        "COMMAND_FAILED"
    } else if lower.contains("exceeds") || lower.contains("limit") {
        "TOO_LARGE"
    } else if lower.contains("not found") || lower.contains("unknown") {
        "NOT_FOUND"
    } else if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("expired")
    {
        "AUTH_ERROR"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "TIMEOUT"
    } else if lower.contains("duplicate") || lower.contains("already") {
        "CONFLICT"
    } else if lower.contains("unsupported") {
        "UNSUPPORTED"
    } else if lower.contains("permission denied")
        || lower.contains("not allowed")
        || lower.contains("blocked by server policy")
    {
        "POLICY_DENIED"
    } else if lower.contains("not declared")
        || lower.contains("not enabled")
        || lower.contains("docker is unavailable")
        || lower.contains("docker error")
    {
        "CONFIG_ERROR"
    } else if lower.contains("empty") {
        "INVALID_INPUT"
    } else {
        "UNKNOWN"
    }
}

async fn generate(job: &mut GenerationJob) -> anyhow::Result<()> {
    let user_id = job.user_id.clone();
    let recall_query = memory_recall_query(&job.history, &job.user_content);
    let (personality, mem_results, plugin_skills, installed_plugins) = job
        .state
        .db
        .call(move |conn| {
            let personality = memory::load_personality(conn, &user_id)?;
            let memories = memory::search_memory(conn, &user_id, &recall_query, 10)?;
            let skills = registry::enabled_skills(conn, &user_id)?;
            let installed = registry::list_enabled(conn, &user_id)?;
            Ok((personality, memories, skills, installed))
        })
        .await?;
    let plugin_catalog = registry::plugin_catalog(
        &job.state.registry_cache,
        installed_plugins,
        &job.state.config.registry.url,
        &job.state.config.plugins.blacklist,
    )
    .await;
    let tool_catalog = ToolCatalog::load(&job.state, &job.user_id).await?;

    let make_opts = |est: usize| ContextOptions {
        soul: personality.soul.as_deref(),
        identity: personality.identity.as_deref(),
        user_profile: personality.user_profile.as_deref(),
        memories: &mem_results,
        plugin_skills: &plugin_skills,
        plugin_catalog: &plugin_catalog,
        timezone: Some(job.user_timezone.as_str()),
        memory_leaning: Some(job.user_memory_leaning.as_str()),
        context_limit: Some(job.provider.context_limit()),
        estimated_tokens: Some(est),
        sandbox_enabled: job.state.sandbox.is_some(),
    };

    // Rough token estimate from history + user message before assembly.
    // Matches compact::estimate_tokens() heuristic (content.len() / 4 + 4 per msg).
    let initial_est = job
        .history
        .iter()
        .map(|m| m.content.len() / 4 + 4)
        .sum::<usize>()
        + job.user_content.len() / 4
        + 4;

    let probe = if job.resume_from_history {
        context::assemble_history(&job.history, make_opts(initial_est))
    } else {
        context::assemble(&job.history, &job.user_content, make_opts(initial_est))
    };
    if compact::needs_compaction(&probe, job.provider.context_limit()) {
        tracing::info!(
            conversation_id = %job.conversation_id,
            "context window approaching limit — auto-compacting"
        );
        if let Err(error) =
            compact::compact_conversation(&job.state.db, &job.conversation_id, &job.provider).await
        {
            tracing::error!(error = %error, "auto-compact failed — continuing without compaction");
        } else {
            let cid = job.conversation_id.clone();
            let sequence = job.user_sequence;
            if let Ok(history) = job
                .state
                .db
                .call(move |conn| history::load_messages_before(conn, &cid, sequence))
                .await
            {
                job.history = history;
            }
        }
    }

    // Re-estimate after possible compaction.
    let est = job
        .history
        .iter()
        .map(|m| m.content.len() / 4 + 4)
        .sum::<usize>()
        + job.user_content.len() / 4
        + 4;

    let mut messages = if job.resume_from_history {
        context::assemble_history(&job.history, make_opts(est))
    } else {
        context::assemble(&job.history, &job.user_content, make_opts(est))
    };

    // Emit context window usage to the frontend.
    let context_event = Event::default()
        .event("context")
        .json_data(SseContext {
            used_tokens: est,
            max_tokens: job.provider.context_limit(),
        })
        .unwrap_or_else(|_| Event::default());
    let _ = job.tx.send(Ok(context_event)).await;

    let mut guard = ToolLoopGuard::new();
    loop {
        if let Some(stop) = guard.before_provider() {
            return finalize_without_tools(job, &mut messages, stop).await;
        }

        let remaining = guard.remaining().unwrap_or(Duration::ZERO);
        let provider_round = tokio::time::timeout(
            remaining,
            complete_provider_round(job, &messages, tool_catalog.definitions()),
        )
        .await;
        let (assistant_content, tool_calls, usage) = match provider_round {
            Ok(result) => result?,
            Err(_) => {
                let message_id = job.assistant_message_id.clone();
                job.state
                    .db
                    .call(move |conn| history::clear_assistant_content(conn, &message_id))
                    .await?;
                return finalize_without_tools(
                    job,
                    &mut messages,
                    ToolStop {
                        code: "TOOL_TIME_LIMIT",
                        message: format!(
                            "The {} second tool-use time budget expired during a model response.",
                            TOOL_PHASE_TIMEOUT.as_secs()
                        ),
                    },
                )
                .await;
            }
        };
        if tool_calls.is_empty() {
            return finish_job(job, usage).await;
        }

        if let Some(stop) = guard.inspect_calls(&tool_calls) {
            let results = persist_rejected_tool_round(
                job,
                &tool_calls,
                usage.as_ref(),
                &stop.message,
                stop.code,
            )
            .await?;
            append_tool_round(&mut messages, assistant_content, tool_calls, results);
            return finalize_without_tools(job, &mut messages, stop).await;
        }

        let interactive_calls = tool_calls
            .iter()
            .filter(|call| is_interactive_tool(&call.name))
            .count();
        if interactive_calls > 0 {
            if tool_calls.len() != 1 || interactive_calls != 1 {
                let error = "Interactive tools must be called alone in their tool round. \
                    Retry the interaction without any other tool calls.";
                let results = persist_rejected_tool_round(
                    job,
                    &tool_calls,
                    usage.as_ref(),
                    error,
                    "INVALID_INTERACTION",
                )
                .await?;
                let failure_count = tool_calls.len();
                append_tool_round(&mut messages, assistant_content, tool_calls, results);
                let stop = record_failed_round(&mut guard, failure_count, "INVALID_INTERACTION");
                if let Some(stop) = stop {
                    return finalize_without_tools(job, &mut messages, stop).await;
                }
                continue;
            }

            let call = &tool_calls[0];
            let call_event = Event::default()
                .event("tool_call")
                .json_data(SseToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                })
                .unwrap_or_else(|_| Event::default());
            let _ = job.tx.send(Ok(call_event)).await;

            match build_interaction_payload(call, &plugin_catalog) {
                Ok(payload) => {
                    let uid = job.user_id.clone();
                    let cid = job.conversation_id.clone();
                    let mid = job.assistant_message_id.clone();
                    let persisted_call = call.clone();
                    let persisted_usage = usage;
                    let pending = job
                        .state
                        .db
                        .call(move |conn| {
                            interaction::create(
                                conn,
                                &uid,
                                &cid,
                                &mid,
                                &persisted_call,
                                &payload,
                                persisted_usage.as_ref(),
                            )
                        })
                        .await?;
                    let event = Event::default()
                        .event("input_required")
                        .json_data(SseInputRequired {
                            interaction: pending,
                        })
                        .unwrap_or_else(|_| Event::default());
                    let _ = job.tx.send(Ok(event)).await;
                    return Ok(());
                }
                Err(error) => {
                    let results = persist_rejected_tool_round(
                        job,
                        &tool_calls,
                        usage.as_ref(),
                        &error.to_string(),
                        "INVALID_INTERACTION",
                    )
                    .await?;
                    let failure_count = tool_calls.len();
                    append_tool_round(&mut messages, assistant_content, tool_calls, results);
                    let stop =
                        record_failed_round(&mut guard, failure_count, "INVALID_INTERACTION");
                    if let Some(stop) = stop {
                        return finalize_without_tools(job, &mut messages, stop).await;
                    }
                    continue;
                }
            }
        }

        let mut results = Vec::with_capacity(tool_calls.len());
        let mut successes = 0;
        let mut stop_after_round: Option<ToolStop> = None;
        for call in &tool_calls {
            let call_event = Event::default()
                .event("tool_call")
                .json_data(SseToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                })
                .unwrap_or_else(|_| Event::default());
            let _ = job.tx.send(Ok(call_event)).await;

            let (truncated, result, failure_code) = if let Some(stop) = &stop_after_round {
                (
                    false,
                    tool_failure_result(
                        &format!("Skipped because tool use is stopping: {}", stop.message),
                        "BUDGET_EXHAUSTED",
                    ),
                    Some("BUDGET_EXHAUSTED"),
                )
            } else if let Some(reason) = call.invalid.as_deref() {
                (
                    false,
                    tool_failure_result(reason, "INVALID_ARGS"),
                    Some("INVALID_ARGS"),
                )
            } else {
                let Some(remaining) = guard.remaining().filter(|value| !value.is_zero()) else {
                    let stop = ToolStop {
                        code: "TOOL_TIME_LIMIT",
                        message: format!(
                            "The {} second tool-use time budget was reached.",
                            TOOL_PHASE_TIMEOUT.as_secs()
                        ),
                    };
                    stop_after_round = Some(stop.clone());
                    let result = tool_failure_result(&stop.message, stop.code);
                    results.push((call.clone(), result.clone()));
                    emit_tool_result(job, call, result, false).await;
                    continue;
                };
                let execution_call = clamp_sandbox_timeout(call, remaining);
                match tokio::time::timeout(
                    remaining,
                    tool_catalog.execute(
                        &job.state,
                        &job.user_id,
                        Some(&job.conversation_id),
                        &execution_call,
                    ),
                )
                .await
                {
                    Ok(Ok(raw)) => {
                        let (body, trunc) = truncate_tool_text(&raw, MAX_TOOL_RESULT_CHARS);
                        successes += 1;
                        (
                            trunc,
                            serde_json::json!({"ok": true, "result": body, "truncated": trunc})
                                .to_string(),
                            None,
                        )
                    }
                    Ok(Err(error)) => {
                        let msg = error.to_string();
                        let code = tool_error_code(&msg);
                        (false, tool_failure_result(&msg, code), Some(code))
                    }
                    Err(_) => {
                        let stop = ToolStop {
                            code: "TOOL_TIME_LIMIT",
                            message: format!(
                                "The {} second tool-use time budget expired during `{}`.",
                                TOOL_PHASE_TIMEOUT.as_secs(),
                                call.name
                            ),
                        };
                        stop_after_round = Some(stop.clone());
                        (
                            false,
                            tool_failure_result(&stop.message, stop.code),
                            Some(stop.code),
                        )
                    }
                }
            };

            if let Some(code) = failure_code
                && code != "BUDGET_EXHAUSTED"
                && let Some(stop) = guard.record_failure(code)
            {
                stop_after_round.get_or_insert(stop);
            }
            emit_tool_result(job, call, result.clone(), truncated).await;
            results.push((call.clone(), result));
        }

        persist_tool_round(job, &tool_calls, &results, usage.as_ref()).await?;
        append_tool_round(&mut messages, assistant_content, tool_calls, results);

        let round_stop = guard.record_round(successes);
        if let Some(stop) = stop_after_round.or(round_stop) {
            return finalize_without_tools(job, &mut messages, stop).await;
        }

        messages.push(ChatMessage::system(format!(
            "[Tool round: {}/{} — {} remaining. Stop if the available tools cannot reliably answer the request.]",
            guard.rounds,
            MAX_TOOL_ROUNDS,
            MAX_TOOL_ROUNDS.saturating_sub(guard.rounds),
        )));
    }
}

fn tool_failure_result(error: &str, code: &str) -> String {
    let (error, _) = truncate_tool_text(error, MAX_TOOL_ERROR_CHARS);
    serde_json::json!({"ok": false, "error": error, "code": code}).to_string()
}

fn truncate_tool_text(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = value[..end].to_string();
    truncated.push_str("\n[... truncated]");
    (truncated, true)
}

fn append_tool_round(
    messages: &mut Vec<ChatMessage>,
    assistant_content: String,
    tool_calls: Vec<ToolCall>,
    results: Vec<(ToolCall, String)>,
) {
    messages.push(ChatMessage::assistant_with_tools(
        assistant_content,
        tool_calls,
    ));
    for (call, result) in results {
        messages.push(ChatMessage::tool(call.id, call.name, result));
    }
}

fn record_failed_round(guard: &mut ToolLoopGuard, failures: usize, code: &str) -> Option<ToolStop> {
    let mut stop = None;
    for _ in 0..failures {
        if let Some(next) = guard.record_failure(code) {
            stop.get_or_insert(next);
        }
    }
    stop.or_else(|| guard.record_round(0))
}

fn clamp_sandbox_timeout(call: &ToolCall, remaining: Duration) -> ToolCall {
    if call.name != "sandbox_exec" {
        return call.clone();
    }
    let mut call = call.clone();
    let Some(arguments) = call.arguments.as_object_mut() else {
        return call;
    };
    let remaining_secs = remaining.as_secs().max(1);
    let requested = arguments
        .get("timeout")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(remaining_secs);
    arguments.insert(
        "timeout".to_string(),
        serde_json::Value::from(requested.min(remaining_secs)),
    );
    call
}

async fn emit_tool_result(job: &GenerationJob, call: &ToolCall, result: String, truncated: bool) {
    let result_event = Event::default()
        .event("tool_result")
        .json_data(SseToolResult {
            id: call.id.clone(),
            name: call.name.clone(),
            result,
            truncated,
        })
        .unwrap_or_else(|_| Event::default());
    let _ = job.tx.send(Ok(result_event)).await;
}

async fn persist_tool_round(
    job: &GenerationJob,
    tool_calls: &[ToolCall],
    results: &[(ToolCall, String)],
    usage: Option<&Usage>,
) -> anyhow::Result<()> {
    let message_id = job.assistant_message_id.clone();
    let persisted_calls = tool_calls.to_vec();
    let persisted_results = results.to_vec();
    let persisted_usage = usage.copied();
    job.state
        .db
        .call(move |conn| {
            history::persist_tool_round(
                conn,
                &message_id,
                &persisted_calls,
                &persisted_results,
                persisted_usage.as_ref(),
            )
        })
        .await
}

async fn finalize_without_tools(
    job: &GenerationJob,
    messages: &mut Vec<ChatMessage>,
    stop: ToolStop,
) -> anyhow::Result<()> {
    messages.push(ChatMessage::system(format!(
        "[Tool use stopped: {} ({})]\n\
         Respond to the user now using only information already available. Do not call tools. \
         Be concise and candid about anything you could not verify. Do not suggest that you are \
         still working or ask the user to continue the same attempt.",
        stop.message, stop.code
    )));

    match tokio::time::timeout(
        FINAL_RESPONSE_TIMEOUT,
        complete_provider_round(job, messages, &[]),
    )
    .await
    {
        Ok(Ok((content, _, usage))) if !content.trim().is_empty() => finish_job(job, usage).await,
        Ok(Ok(_)) | Ok(Err(_)) | Err(_) => {
            let message_id = job.assistant_message_id.clone();
            job.state
                .db
                .call(move |conn| {
                    history::append_assistant_content(conn, &message_id, FINAL_RESPONSE_FALLBACK)
                })
                .await?;
            let event = Event::default()
                .event("chunk")
                .json_data(SseChunk {
                    delta: FINAL_RESPONSE_FALLBACK.to_string(),
                })
                .unwrap_or_else(|_| Event::default());
            let _ = job.tx.send(Ok(event)).await;
            finish_job(job, None).await
        }
    }
}

async fn finish_job(job: &GenerationJob, usage: Option<Usage>) -> anyhow::Result<()> {
    let message_id = job.assistant_message_id.clone();
    let usage_for_done = usage.map(|value| helpcore_api::SseUsage {
        input_tokens: value.input_tokens,
        output_tokens: value.output_tokens,
        cache_read_tokens: value.cache_read_tokens,
        cache_write_tokens: value.cache_write_tokens,
    });
    if let Some(value) = usage {
        let mid = message_id.clone();
        job.state
            .db
            .call(move |conn| history::set_message_usage(conn, &mid, &value))
            .await?;
    }
    job.state
        .db
        .call(move |conn| history::finish_assistant_message(conn, &message_id))
        .await?;
    let event = Event::default()
        .event("done")
        .json_data(SseDone {
            conversation_id: job.conversation_id.clone(),
            message_id: job.assistant_message_id.clone(),
            usage: usage_for_done,
        })
        .unwrap_or_else(|_| Event::default());
    let _ = job.tx.send(Ok(event)).await;
    Ok(())
}

fn build_interaction_payload(
    call: &ToolCall,
    catalog: &[PluginCatalogItem],
) -> anyhow::Result<InteractionPayload> {
    match call.name.as_str() {
        "request_user_input" => {
            #[derive(Deserialize)]
            struct Arguments {
                questions: Vec<InteractionQuestion>,
            }
            let arguments: Arguments = serde_json::from_value(call.arguments.clone())
                .context("request_user_input arguments are invalid")?;
            if !(1..=3).contains(&arguments.questions.len()) {
                anyhow::bail!("request_user_input requires one to three questions");
            }
            let mut question_ids = std::collections::HashSet::new();
            for question in &arguments.questions {
                if question.id.trim().is_empty()
                    || question.header.trim().is_empty()
                    || question.question.trim().is_empty()
                {
                    anyhow::bail!("question id, header, and text must not be empty");
                }
                if !question_ids.insert(question.id.as_str()) {
                    anyhow::bail!("question ids must be unique");
                }
                match question.question_type {
                    InteractionQuestionType::Text => {
                        if !question.options.is_empty() {
                            anyhow::bail!("text questions must not include options");
                        }
                    }
                    InteractionQuestionType::SingleSelect
                    | InteractionQuestionType::MultiSelect => {
                        if !(2..=3).contains(&question.options.len()) {
                            anyhow::bail!("select questions require two or three options");
                        }
                        let mut option_ids = std::collections::HashSet::new();
                        for option in &question.options {
                            if option.id.trim().is_empty()
                                || option.label.trim().is_empty()
                                || option.description.trim().is_empty()
                            {
                                anyhow::bail!(
                                    "every option requires a non-empty id, label, and help description"
                                );
                            }
                            if !option_ids.insert(option.id.as_str()) {
                                anyhow::bail!("option ids must be unique within a question");
                            }
                        }
                    }
                }
            }
            Ok(InteractionPayload::Questions {
                questions: arguments.questions,
            })
        }
        "request_plugin_action" => {
            #[derive(Deserialize)]
            struct Arguments {
                plugin_id: String,
                rationale: String,
            }
            let arguments: Arguments = serde_json::from_value(call.arguments.clone())
                .context("request_plugin_action arguments are invalid")?;
            let plugin = catalog
                .iter()
                .find(|plugin| plugin.id == arguments.plugin_id)
                .with_context(|| {
                    format!("plugin '{}' is not in the catalog", arguments.plugin_id)
                })?;
            if !plugin.user_managed {
                anyhow::bail!(
                    "plugin '{}' is server-managed and cannot be changed by the user",
                    plugin.id
                );
            }
            let action = match plugin.state.as_str() {
                "available" => "install_configure_enable",
                "disabled" => "enable",
                "needs_configuration" => "configure_enable",
                "enabled" => anyhow::bail!("plugin '{}' is already enabled", plugin.id),
                "blocked" => anyhow::bail!("plugin '{}' is blocked by server policy", plugin.id),
                "unavailable" => anyhow::bail!("plugin '{}' is not installable", plugin.id),
                state => anyhow::bail!("plugin '{}' has unsupported state '{state}'", plugin.id),
            };
            if arguments.rationale.trim().is_empty() {
                anyhow::bail!("plugin rationale must not be empty");
            }
            Ok(InteractionPayload::PluginApproval {
                plugin: PluginProposal {
                    id: plugin.id.clone(),
                    name: plugin.name.clone(),
                    rationale: arguments.rationale,
                    action: action.to_string(),
                    state: plugin.state.clone(),
                    permissions: plugin.permissions.clone(),
                    allowed_hosts: plugin.allowed_hosts.clone(),
                    setup_guide: plugin.setup_guide.clone(),
                },
            })
        }
        _ => anyhow::bail!("{} is not an interactive tool", call.name),
    }
}

async fn persist_rejected_tool_round(
    job: &GenerationJob,
    tool_calls: &[ToolCall],
    usage: Option<&Usage>,
    error: &str,
    code: &str,
) -> anyhow::Result<Vec<(ToolCall, String)>> {
    let mut results = Vec::with_capacity(tool_calls.len());
    for call in tool_calls {
        let call_event = Event::default()
            .event("tool_call")
            .json_data(SseToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            })
            .unwrap_or_else(|_| Event::default());
        let _ = job.tx.send(Ok(call_event)).await;
        let result = tool_failure_result(error, code);
        emit_tool_result(job, call, result.clone(), false).await;
        results.push((call.clone(), result));
    }
    persist_tool_round(job, tool_calls, &results, usage).await?;
    Ok(results)
}

async fn complete_provider_round(
    job: &GenerationJob,
    messages: &[ChatMessage],
    tools: &[crate::providers::types::ToolDefinition],
) -> anyhow::Result<(String, Vec<ToolCall>, Option<Usage>)> {
    let mut stream = job
        .provider
        .complete(messages, tools, Some(&job.model_used))
        .await
        .map_err(|error| anyhow::anyhow!("provider error: {error}"))?;
    let mut pending = String::new();
    let mut assistant_content = String::new();
    let mut tool_calls = Vec::new();
    let mut flush_tick = tokio::time::interval(Duration::from_millis(100));
    flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = flush_tick.tick(), if !pending.is_empty() => {
                flush_pending(job, &mut pending).await?;
            }
            item = stream.next() => {
                match item {
                    Some(Ok(chunk)) => {
                        if !chunk.delta.is_empty() {
                            assistant_content.push_str(&chunk.delta);
                            pending.push_str(&chunk.delta);
                            if pending.len() >= 256 {
                                flush_pending(job, &mut pending).await?;
                            }
                        }
                        tool_calls.extend(chunk.tool_calls);
                        if chunk.is_final {
                            flush_pending(job, &mut pending).await?;
                            let usage = chunk.usage;
                            return Ok((assistant_content, tool_calls, usage));
                        }
                    }
                    Some(Err(error)) => {
                        flush_pending(job, &mut pending).await?;
                        return Err(error.into());
                    }
                    None => {
                        flush_pending(job, &mut pending).await?;
                        anyhow::bail!("provider stream ended before completion");
                    }
                }
            }
        }
    }
}

async fn flush_pending(job: &GenerationJob, pending: &mut String) -> anyhow::Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let delta = std::mem::take(pending);
    let message_id = job.assistant_message_id.clone();
    let persisted = delta.clone();
    job.state
        .db
        .call(move |conn| history::append_assistant_content(conn, &message_id, &persisted))
        .await?;
    let event = Event::default()
        .event("chunk")
        .json_data(SseChunk { delta })
        .unwrap_or_else(|_| Event::default());
    let _ = job.tx.send(Ok(event)).await;
    Ok(())
}

async fn fail_job(job: &GenerationJob, message: String) -> anyhow::Result<()> {
    let message_id = job.assistant_message_id.clone();
    let persisted_error = message.clone();
    job.state
        .db
        .call(move |conn| history::fail_assistant_message(conn, &message_id, &persisted_error))
        .await?;
    let event = Event::default()
        .event("error")
        .json_data(serde_json::json!({ "message": message }))
        .unwrap_or_else(|_| Event::default());
    let _ = job.tx.send(Ok(event)).await;
    Ok(())
}

fn turn_error(error: anyhow::Error) -> AppError {
    let message = error.to_string();
    if message.contains("already in progress") {
        AppError::Conflict(message)
    } else if message.contains("not found") {
        AppError::NotFound(message)
    } else {
        AppError::BadRequest(message)
    }
}

// ── GET /api/conversations ─────────────────────────────────────────────────────

/// GET /api/conversations — lists all conversations for the authenticated user.
pub async fn list_conversations(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<Json<Vec<ConversationSummary>>, AppError> {
    let user_id = auth_user.id.clone();
    let convs = state
        .db
        .call(move |conn| history::list_conversations(conn, &user_id))
        .await?;
    Ok(Json(convs))
}

// ── GET /api/conversations/:id/messages ────────────────────────────────────────

/// GET /api/conversations/{id}/messages — loads all messages for a conversation.
pub async fn get_messages(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(conv_id): Path<String>,
) -> Result<Json<Vec<MessageSummary>>, AppError> {
    let user_id = auth_user.id.clone();
    let cid = conv_id.clone();

    let exists = state
        .db
        .call(move |conn| {
            let c: Option<ConversationSummary> = history::get_conversation(conn, &cid, &user_id)?;
            Ok(c.is_some())
        })
        .await?;

    if !exists {
        return Err(AppError::NotFound("conversation not found".into()));
    }

    let msgs = state
        .db
        .call(move |conn| history::load_messages(conn, &conv_id))
        .await?;

    Ok(Json(msgs))
}

/// GET /api/conversations/{id}/interaction — restores a pending user interaction.
pub async fn get_interaction(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(conversation_id): Path<String>,
) -> Result<Json<Option<PendingInteraction>>, AppError> {
    let uid = auth_user.id;
    let pending = state
        .db
        .call(move |conn| interaction::get_pending(conn, &uid, &conversation_id))
        .await?;
    Ok(Json(pending))
}

/// POST /api/conversations/{id}/interactions/{interaction_id}/respond — answers
/// a durable interaction and streams either the resumed assistant response or
/// the next plugin-configuration interaction.
pub async fn respond_interaction(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((conversation_id, interaction_id)): Path<(String, String)>,
    Json(response): Json<InteractionResponse>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, AppError> {
    let uid = auth_user.id.clone();
    let cid = conversation_id.clone();
    let iid = interaction_id.clone();
    let pending = state
        .db
        .call(move |conn| interaction::get_pending_by_id(conn, &uid, &cid, &iid))
        .await?
        .ok_or_else(|| AppError::NotFound("pending interaction not found".into()))?;

    let dismissed = matches!(&response, InteractionResponse::Dismiss);
    let outcome = handle_interaction_response(&state, &auth_user.id, &pending, response).await?;

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);
    match outcome {
        InteractionOutcome::Awaiting(next) => {
            let event = Event::default()
                .event("input_required")
                .json_data(SseInputRequired { interaction: *next })
                .unwrap_or_else(|_| Event::default());
            let _ = tx.try_send(Ok(event));
        }
        InteractionOutcome::Resume(result) => {
            let interaction_for_db = pending.clone();
            state
                .db
                .call(move |conn| {
                    interaction::complete(conn, &interaction_for_db, &result, dismissed)
                })
                .await?;

            let resume = load_interaction_resume(&state, &auth_user.id, &pending).await?;
            let provider = state
                .providers
                .find_for_role(Some(&resume.provider_id), ProviderRole::Chat)
                .ok_or_else(|| {
                    AppError::BadRequest(format!(
                        "provider {} is not available",
                        resume.provider_id
                    ))
                })?;
            let started_event = Event::default()
                .event("started")
                .json_data(SseStarted {
                    conversation_id: pending.conversation_id.clone(),
                    user_message_id: resume.user_message_id.clone(),
                    message_id: pending.message_id.clone(),
                })
                .unwrap_or_else(|_| Event::default());
            let _ = tx.try_send(Ok(started_event));

            tokio::spawn(run_generation(GenerationJob {
                state,
                provider,
                conversation_id: pending.conversation_id,
                assistant_message_id: pending.message_id,
                user_id: auth_user.id,
                user_content: resume.user_content,
                user_sequence: resume.placeholder_sequence,
                user_timezone: resume.timezone,
                user_memory_leaning: resume.memory_leaning,
                history: resume.history,
                model_used: resume.model,
                tx,
                resume_from_history: true,
            }));
            return Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()));
        }
    }
    drop(tx);
    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}

enum InteractionOutcome {
    Awaiting(Box<PendingInteraction>),
    Resume(String),
}

async fn handle_interaction_response(
    state: &AppState,
    user_id: &str,
    pending: &PendingInteraction,
    response: InteractionResponse,
) -> Result<InteractionOutcome, AppError> {
    if matches!(response, InteractionResponse::Dismiss) {
        return Ok(InteractionOutcome::Resume(tool_success(
            serde_json::json!({
                "dismissed": true
            }),
        )));
    }

    match (&pending.payload, response) {
        (
            InteractionPayload::Questions { questions },
            InteractionResponse::Questions { answers },
        ) => {
            validate_question_answers(questions, &answers)?;
            Ok(InteractionOutcome::Resume(tool_success(
                serde_json::json!({
                    "dismissed": false,
                    "answers": answers
                }),
            )))
        }
        (
            InteractionPayload::PluginApproval { plugin },
            InteractionResponse::PluginApproval { approved },
        ) => {
            if !approved {
                return Ok(InteractionOutcome::Resume(tool_success(
                    serde_json::json!({
                        "approved": false
                    }),
                )));
            }
            if plugin.action == "install_configure_enable" {
                crate::api::handlers::plugin::install_for_user(
                    state,
                    user_id,
                    &plugin.id,
                    plugin.permissions.clone(),
                )
                .await?;
            }

            let installed = load_installed_plugin(state, user_id, &plugin.id).await?;
            if !registry::is_configured(
                &installed.manifest.config_schema,
                &installed.tier,
                &installed.config,
            ) {
                let fields = plugin_config_fields(&installed);
                if fields.is_empty() {
                    return Err(AppError::Conflict(
                        "plugin requires configuration but declares no configurable fields".into(),
                    ));
                }
                let payload = InteractionPayload::PluginConfig {
                    plugin: plugin.clone(),
                    current_values: registry::config_values(&fields, &installed.config),
                    fields,
                    error: None,
                };
                let iid = pending.id.clone();
                let payload_for_db = payload.clone();
                state
                    .db
                    .call(move |conn| interaction::update_payload(conn, &iid, &payload_for_db))
                    .await?;
                let mut next = pending.clone();
                next.payload = payload;
                return Ok(InteractionOutcome::Awaiting(Box::new(next)));
            }

            crate::api::handlers::plugin::set_enabled_for_user(state, user_id, &plugin.id, true)
                .await?;
            Ok(InteractionOutcome::Resume(tool_success(
                serde_json::json!({
                    "approved": true,
                    "installed": true,
                    "configured": true,
                    "enabled": true,
                    "plugin_id": plugin.id
                }),
            )))
        }
        (
            InteractionPayload::PluginConfig { plugin, fields, .. },
            InteractionResponse::PluginConfig { values },
        ) => {
            let configure = crate::api::handlers::plugin::configure_for_user(
                state, user_id, &plugin.id, values,
            )
            .await;
            if let Err(error) = configure {
                persist_plugin_config_error(
                    state,
                    user_id,
                    pending,
                    plugin,
                    fields,
                    &error.to_string(),
                )
                .await?;
                return Err(error);
            }
            if let Err(error) =
                crate::api::handlers::plugin::set_enabled_for_user(state, user_id, &plugin.id, true)
                    .await
            {
                persist_plugin_config_error(
                    state,
                    user_id,
                    pending,
                    plugin,
                    fields,
                    &error.to_string(),
                )
                .await?;
                return Err(error);
            }
            Ok(InteractionOutcome::Resume(tool_success(
                serde_json::json!({
                    "approved": true,
                    "installed": true,
                    "configured": true,
                    "enabled": true,
                    "plugin_id": plugin.id
                }),
            )))
        }
        _ => Err(AppError::Conflict(
            "response type does not match the pending interaction".into(),
        )),
    }
}

fn validate_question_answers(
    questions: &[InteractionQuestion],
    answers: &[QuestionAnswer],
) -> Result<(), AppError> {
    if questions.len() != answers.len() {
        return Err(AppError::BadRequest(
            "provide exactly one answer for every question".into(),
        ));
    }
    for question in questions {
        let answer = answers
            .iter()
            .find(|answer| answer.question_id == question.id)
            .ok_or_else(|| AppError::BadRequest(format!("missing answer for {}", question.id)))?;
        let valid_options = answer.option_ids.iter().all(|option_id| {
            question
                .options
                .iter()
                .any(|option| option.id == *option_id)
        });
        if !valid_options {
            return Err(AppError::BadRequest(format!(
                "unknown option for {}",
                question.id
            )));
        }
        match question.question_type {
            InteractionQuestionType::SingleSelect
                if (answer.option_ids.len() == 1 && answer.custom_response.is_none())
                    || (answer.option_ids.is_empty()
                        && answer
                            .custom_response
                            .as_deref()
                            .is_some_and(|value| !value.trim().is_empty())) => {}
            InteractionQuestionType::MultiSelect
                if (!answer.option_ids.is_empty()
                    || answer
                        .custom_response
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty()))
                    && answer
                        .option_ids
                        .iter()
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                        == answer.option_ids.len() => {}
            InteractionQuestionType::Text
                if answer.option_ids.is_empty()
                    && answer
                        .custom_response
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty()) => {}
            _ => {
                return Err(AppError::BadRequest(format!(
                    "answer shape does not match question type for {}",
                    question.id
                )));
            }
        }
    }
    Ok(())
}

async fn load_installed_plugin(
    state: &AppState,
    user_id: &str,
    plugin_id: &str,
) -> Result<registry::InstalledPlugin, AppError> {
    let uid = user_id.to_string();
    let pid = plugin_id.to_string();
    state
        .db
        .call(move |conn| {
            registry::list_enabled(conn, &uid)?
                .into_iter()
                .find(|plugin| plugin.plugin_id == pid)
                .context("plugin not installed")
        })
        .await
        .map_err(|error| AppError::NotFound(error.to_string()))
}

fn plugin_config_fields(plugin: &registry::InstalledPlugin) -> Vec<ConfigField> {
    if !plugin.manifest.config_schema.is_empty() {
        return plugin.manifest.config_schema.clone();
    }
    if plugin.tier == "bridge" {
        return vec![ConfigField {
            key: "endpoint".into(),
            label: "Bridge endpoint".into(),
            field_type: "url".into(),
            required: true,
            hint: Some("URL of the running bridge service".into()),
            default: plugin
                .manifest
                .bridge
                .as_ref()
                .and_then(|bridge| bridge.default_port)
                .map(|port| format!("http://localhost:{port}")),
            options: Vec::new(),
            min: None,
            max: None,
            role: Some("bridge_endpoint".into()),
        }];
    }
    Vec::new()
}

async fn persist_plugin_config_error(
    state: &AppState,
    user_id: &str,
    pending: &PendingInteraction,
    plugin: &PluginProposal,
    fields: &[ConfigField],
    error: &str,
) -> Result<(), AppError> {
    let installed = load_installed_plugin(state, user_id, &plugin.id).await?;
    let payload = InteractionPayload::PluginConfig {
        plugin: plugin.clone(),
        fields: fields.to_vec(),
        current_values: registry::config_values(fields, &installed.config),
        error: Some(error.to_string()),
    };
    let iid = pending.id.clone();
    state
        .db
        .call(move |conn| interaction::update_payload(conn, &iid, &payload))
        .await?;
    Ok(())
}

fn tool_success(result: serde_json::Value) -> String {
    serde_json::json!({"ok": true, "result": result, "truncated": false}).to_string()
}

struct InteractionResume {
    provider_id: String,
    model: String,
    user_message_id: String,
    user_content: String,
    placeholder_sequence: i64,
    timezone: String,
    memory_leaning: String,
    history: Vec<MessageSummary>,
}

async fn load_interaction_resume(
    state: &AppState,
    user_id: &str,
    pending: &PendingInteraction,
) -> Result<InteractionResume, AppError> {
    let uid = user_id.to_string();
    let cid = pending.conversation_id.clone();
    let mid = pending.message_id.clone();
    state
        .db
        .call(move |conn| {
            let (provider_id, model, sequence): (String, String, i64) = conn.query_row(
                "SELECT provider_id, model, sequence FROM messages
                 WHERE id = ?1 AND conversation_id = ?2 AND status = 'pending'",
                rusqlite::params![mid, cid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            let history = history::load_messages_before(conn, &cid, sequence)?;
            let user_message = history
                .iter()
                .rev()
                .find(|message| message.role == "user")
                .context("interaction has no preceding user message")?;
            let user = crate::model::user::find_by_id(conn, &uid)?
                .context("interaction user not found")?;
            Ok(InteractionResume {
                provider_id,
                model,
                user_message_id: user_message.id.clone(),
                user_content: user_message.content.clone(),
                placeholder_sequence: sequence,
                timezone: user.timezone,
                memory_leaning: user.memory_leaning,
                history,
            })
        })
        .await
        .map_err(AppError::Internal)
}

// ── PATCH /api/conversations/:id ───────────────────────────────────────────────

/// PATCH /api/conversations/{id} — renames a conversation.
pub async fn rename_conversation(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(conv_id): Path<String>,
    Json(req): Json<RenameConversationRequest>,
) -> Result<StatusCode, AppError> {
    let user_id = auth_user.id.clone();
    let cid = conv_id.clone();
    let title = req.title.trim().to_string();

    if title.is_empty() {
        return Err(AppError::BadRequest("title cannot be empty".into()));
    }

    state
        .db
        .call(move |conn| history::update_conversation_title(conn, &cid, &user_id, &title))
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ── DELETE /api/conversations/:id ───────────────────────────────────────────────

/// DELETE /api/conversations/{id} — deletes a conversation and all its messages.
pub async fn delete_conversation(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(conv_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let user_id = auth_user.id.clone();

    let deleted = state
        .db
        .call(move |conn| {
            let n = conn.execute(
                "DELETE FROM conversations WHERE id = ?1 AND user_id = ?2",
                rusqlite::params![conv_id, user_id],
            )?;
            Ok(n > 0)
        })
        .await?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound("conversation not found".into()))
    }
}

// ── POST /api/conversations/:id/feedback ───────────────────────────────────────
//
// Injected by the frontend when chart/mermaid rendering errors are detected.
// Stored as a "user" message so the AI sees the error and can self-correct.

/// Request body for rendering-error feedback.
#[derive(Deserialize)]
pub struct FeedbackBody {
    #[serde(rename = "type")]
    type_: String,
    message: String,
}

/// POST /api/conversations/{id}/feedback — submits rendering-error feedback as a user message.
pub async fn submit_feedback(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(conv_id): Path<String>,
    Json(body): Json<FeedbackBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = auth_user.id.clone();
    let content = format!("[Rendering error: {}]\n{}", body.type_, body.message);
    let cid1 = conv_id.clone();
    let cid2 = conv_id.clone();
    let exists = state
        .db
        .call(move |conn| {
            let c: Option<ConversationSummary> = history::get_conversation(conn, &cid1, &user_id)?;
            Ok(c.is_some())
        })
        .await?;
    if !exists {
        return Err(AppError::NotFound("conversation not found".into()));
    }
    state
        .db
        .call(move |conn| history::append_user_feedback(conn, &cid2, &content))
        .await?;
    Ok(Json(serde_json::json!({"status": "ok"})))
}

// ── POST /api/conversations/:id/cancel ─────────────────────────────────────────

/// POST /api/conversations/{id}/cancel — interrupts any in-progress messages for a conversation.
pub async fn cancel_conversation(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(conv_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = auth_user.id.clone();
    let cid = conv_id.clone();

    let cid2 = cid.clone();
    let exists = state
        .db
        .call(move |conn| {
            let c: Option<ConversationSummary> = history::get_conversation(conn, &cid, &user_id)?;
            Ok(c.is_some())
        })
        .await?;

    if !exists {
        return Err(AppError::NotFound("conversation not found".into()));
    }

    let interrupted = state
        .db
        .call(move |conn| history::interrupt_active_messages_for_conversation(conn, &cid2))
        .await?;

    Ok(Json(serde_json::json!({ "interrupted": interrupted })))
}

// ── POST /api/conversations/:id/compact ────────────────────────────────────────

/// POST /api/conversations/{id}/compact — compacts a conversation's history into a summary.
pub async fn compact_conversation(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(conv_id): Path<String>,
) -> Result<Json<CompactResponse>, AppError> {
    // Verify the conversation belongs to this user.
    let user_id = auth_user.id.clone();
    let cid = conv_id.clone();
    let conv = state
        .db
        .call(move |conn| history::get_conversation(conn, &cid, &user_id))
        .await?
        .ok_or(AppError::NotFound("conversation not found".into()))?;

    // Use the conversation's last provider, falling back to the first available.
    let provider = conv
        .provider_id
        .as_deref()
        .and_then(|pid| state.providers.find_for_role(Some(pid), ProviderRole::Chat))
        .or_else(|| state.providers.find_for_role(None, ProviderRole::Chat))
        .ok_or_else(|| AppError::BadRequest("no chat providers configured".into()))?;

    let result = compact::compact_conversation(&state.db, &conv.id, &provider)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "manual compact failed");
            AppError::BadRequest(format!("compaction failed: {e}"))
        })?;

    Ok(Json(CompactResponse {
        conversation_id: conv.id,
        messages_compacted: result.messages_compacted,
        summary_length: result.summary_length,
    }))
}

// ── POST /api/conversations/:id/generate-title ────────────────────────────────

/// Prompt used when asking the model to generate a conversation title.
const RENAME_INSTRUCTIONS: &str = include_str!("../../../../../prompts/rename.md");

/// Extracts title content from the model's response, handling `<title>` wrapper
/// tags robustly. Falls back to raw text when tags are absent or malformed.
fn extract_title(raw: &str) -> String {
    let raw = raw.trim();

    if let (Some(start), Some(end)) = (raw.find("<title>"), raw.rfind("</title>")) {
        let content = &raw[start + "<title>".len()..end];
        return content.trim().to_string();
    }

    if let Some(rest) = raw.strip_prefix("<title>") {
        return rest.trim().to_string();
    }

    if let Some(content) = raw.strip_suffix("</title>") {
        return content.trim().to_string();
    }

    raw.to_string()
}

/// POST /api/conversations/{id}/generate-title — generates an AI title for a conversation.
pub async fn generate_title(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(conv_id): Path<String>,
) -> Result<Json<GenerateTitleResponse>, AppError> {
    let user_id = auth_user.id.clone();
    let uid = user_id.clone();
    let cid = conv_id.clone();
    let conv = state
        .db
        .call(move |conn| history::get_conversation(conn, &cid, &uid))
        .await?
        .ok_or(AppError::NotFound("conversation not found".into()))?;

    let provider = conv
        .provider_id
        .as_deref()
        .and_then(|pid| state.providers.find_for_role(Some(pid), ProviderRole::Chat))
        .or_else(|| state.providers.find_for_role(None, ProviderRole::Chat))
        .ok_or_else(|| AppError::BadRequest("no chat providers configured".into()))?;

    let conv_id = conv.id.clone();
    let messages = state
        .db
        .call(move |conn| history::load_messages(conn, &conv_id))
        .await?;

    // If there are no messages, use a fallback title.
    if messages.is_empty() {
        let title = "New conversation".to_string();
        let cid = conv.id.clone();
        let uid = user_id.clone();
        let title_clone = title.clone();
        state
            .db
            .call(move |conn| history::update_conversation_title(conn, &cid, &uid, &title_clone))
            .await?;
        return Ok(Json(GenerateTitleResponse { title }));
    }

    // Format a sample of messages (first few and last few for context).
    let formatted = messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt_messages = vec![
        ChatMessage::system(RENAME_INSTRUCTIONS),
        ChatMessage::user(&formatted),
    ];

    let mut stream = provider
        .complete(&prompt_messages, &[], None)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "title generation failed");
            AppError::BadRequest(format!("title generation failed: {e}"))
        })?;

    let mut raw_title = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            tracing::error!(error = %e, "stream error during title generation");
            AppError::BadRequest(format!("stream error: {e}"))
        })?;
        if !chunk.is_final {
            raw_title.push_str(&chunk.delta);
        }
    }

    let title = extract_title(&raw_title);
    let title = history::truncate_title(&title);

    if title.is_empty() {
        // Fallback: use truncated first user message.
        let first_msg = &messages[0];
        let title = history::truncate_title(&first_msg.content);
        let cid = conv.id.clone();
        let uid = user_id.clone();
        let title_clone = title.clone();
        state
            .db
            .call(move |conn| history::update_conversation_title(conn, &cid, &uid, &title_clone))
            .await?;
        return Ok(Json(GenerateTitleResponse { title }));
    }

    let cid = conv.id.clone();
    let uid = user_id.clone();
    let title_clone = title.clone();
    state
        .db
        .call(move |conn| history::update_conversation_title(conn, &cid, &uid, &title_clone))
        .await?;

    Ok(Json(GenerateTitleResponse { title }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use helpcore_api::QuestionOption;

    fn msg(role: &str, content: &str) -> MessageSummary {
        MessageSummary {
            id: "id".to_string(),
            role: role.to_string(),
            content: content.to_string(),
            tool_call_id: None,
            tool_calls: None,
            sequence: 1,
            created_at: "now".to_string(),
            status: helpcore_api::MessageStatus::Complete,
            error: None,
            updated_at: "now".to_string(),
            usage: None,
        }
    }

    fn question(
        id: &str,
        question_type: InteractionQuestionType,
        options: Vec<QuestionOption>,
    ) -> InteractionQuestion {
        InteractionQuestion {
            id: id.to_string(),
            header: "Choice".to_string(),
            question: "What should happen?".to_string(),
            question_type,
            options,
        }
    }

    fn option(id: &str, description: &str) -> QuestionOption {
        QuestionOption {
            id: id.to_string(),
            label: id.to_uppercase(),
            description: description.to_string(),
        }
    }

    #[test]
    fn recall_query_is_just_the_message_without_history() {
        assert_eq!(
            memory_recall_query(&[], "what's Alice's number?"),
            "what's Alice's number?"
        );
    }

    #[test]
    fn recall_query_folds_in_the_most_recent_exchange() {
        let history = vec![
            msg("user", "tell me about my trip to Berlin"),
            msg(
                "assistant",
                "Sure — you flew out on the 3rd and stayed near Mitte.",
            ),
        ];
        let query = memory_recall_query(&history, "yes, that one");
        // New message first (it's weighted highest by being listed first and
        // by search_memory ranking), then the most recent exchange.
        assert_eq!(
            query,
            "yes, that one Sure — you flew out on the 3rd and stayed near Mitte. \
             tell me about my trip to Berlin"
        );
    }

    #[test]
    fn recall_query_ignores_tool_messages() {
        let history = vec![
            msg("user", "what's the weather"),
            msg("tool", "{\"temp\": 12}"),
            msg("assistant", "It's 12 degrees and cloudy."),
        ];
        let query = memory_recall_query(&history, "should I bring a coat?");
        assert!(
            !query.contains("temp"),
            "tool message content should be excluded"
        );
        assert!(query.contains("It's 12 degrees and cloudy."));
        assert!(query.contains("what's the weather"));
    }

    #[test]
    fn tool_error_code_classifies_exceeds_limit() {
        assert_eq!(
            tool_error_code("result exceeds the allowed limit"),
            "TOO_LARGE"
        );
        assert_eq!(
            tool_error_code("HTTP response exceeds the 1 MiB limit"),
            "TOO_LARGE"
        );
    }

    #[test]
    fn tool_error_code_classifies_not_found() {
        assert_eq!(
            tool_error_code("model requested unknown tool foo"),
            "NOT_FOUND"
        );
    }

    #[test]
    fn tool_error_code_classifies_invalid_input() {
        assert_eq!(
            tool_error_code("the path must be within the workspace"),
            "INVALID_INPUT"
        );
        assert_eq!(
            tool_error_code("tool call must contain a valid identifier"),
            "INVALID_INPUT"
        );
    }

    #[test]
    fn tool_error_code_classifies_command_and_configuration_failures() {
        assert_eq!(
            tool_error_code("command exited with status 22\ncurl: page not found"),
            "COMMAND_FAILED"
        );
        assert_eq!(
            tool_error_code("sandbox is not enabled or Docker is unavailable"),
            "CONFIG_ERROR"
        );
    }

    #[test]
    fn tool_error_code_defaults_to_unknown() {
        assert_eq!(tool_error_code("some unexpected failure"), "UNKNOWN");
    }

    #[test]
    fn canonical_json_ignores_object_key_order() {
        let left = serde_json::json!({"query": "near me", "options": {"b": 2, "a": 1}});
        let right = serde_json::json!({"options": {"a": 1, "b": 2}, "query": "near me"});
        assert_eq!(canonical_json(&left), canonical_json(&right));
    }

    #[test]
    fn guard_stops_the_third_identical_call() {
        let mut guard = ToolLoopGuard::new();
        let call = ToolCall::new("call-1", "lookup", serde_json::json!({"query": "near me"}));
        assert!(guard.inspect_calls(std::slice::from_ref(&call)).is_none());
        assert!(guard.inspect_calls(std::slice::from_ref(&call)).is_none());
        let stop = guard.inspect_calls(&[call]).unwrap();
        assert_eq!(stop.code, "REPEATED_TOOL_CALL");
    }

    #[test]
    fn guard_enforces_round_and_failure_budgets() {
        let mut round_guard = ToolLoopGuard::new();
        for _ in 0..MAX_TOOL_ROUNDS {
            assert!(round_guard.record_round(1).is_none());
        }
        assert_eq!(
            round_guard.before_provider().unwrap().code,
            "TOOL_ROUND_LIMIT"
        );

        let mut call_guard = ToolLoopGuard::new();
        for _ in 0..MAX_FAILED_CALLS - 1 {
            assert!(call_guard.record_failure("UNKNOWN").is_none());
        }
        assert_eq!(
            call_guard.record_failure("UNKNOWN").unwrap().code,
            "TOOL_FAILURE_LIMIT"
        );

        let mut failed_round_guard = ToolLoopGuard::new();
        assert!(failed_round_guard.record_round(0).is_none());
        assert_eq!(
            failed_round_guard.record_round(0).unwrap().code,
            "FAILED_TOOL_ROUNDS"
        );
    }

    #[test]
    fn guard_stops_terminal_errors_immediately() {
        let mut guard = ToolLoopGuard::new();
        assert_eq!(
            guard.record_failure("AUTH_ERROR").unwrap().code,
            "TERMINAL_TOOL_ERROR"
        );
    }

    #[test]
    fn tool_text_truncation_preserves_utf8_boundaries() {
        let (value, truncated) = truncate_tool_text("ééé", 5);
        assert!(truncated);
        assert!(value.starts_with("éé"));
    }

    #[test]
    fn question_interaction_supports_all_input_types_and_requires_option_help() {
        let call = ToolCall::new(
            "call-1",
            "request_user_input",
            serde_json::json!({
                "questions": [
                    {
                        "id": "single",
                        "header": "Single",
                        "question": "Pick one",
                        "question_type": "single_select",
                        "options": [
                            {"id": "a", "label": "A", "description": "Use A"},
                            {"id": "b", "label": "B", "description": "Use B"}
                        ]
                    },
                    {
                        "id": "multi",
                        "header": "Multiple",
                        "question": "Pick several",
                        "question_type": "multi_select",
                        "options": [
                            {"id": "a", "label": "A", "description": "Include A"},
                            {"id": "b", "label": "B", "description": "Include B"}
                        ]
                    },
                    {
                        "id": "text",
                        "header": "Text",
                        "question": "Explain",
                        "question_type": "text"
                    }
                ]
            }),
        );
        let payload = build_interaction_payload(&call, &[]).unwrap();
        let InteractionPayload::Questions { questions } = payload else {
            panic!("expected questions payload");
        };
        assert_eq!(questions.len(), 3);

        let missing_help = ToolCall::new(
            "call-2",
            "request_user_input",
            serde_json::json!({
                "questions": [{
                    "id": "single",
                    "header": "Single",
                    "question": "Pick one",
                    "question_type": "single_select",
                    "options": [
                        {"id": "a", "label": "A", "description": ""},
                        {"id": "b", "label": "B", "description": "Use B"}
                    ]
                }]
            }),
        );
        assert!(build_interaction_payload(&missing_help, &[]).is_err());
    }

    #[test]
    fn question_answers_accept_selection_custom_multi_and_text() {
        let questions = vec![
            question(
                "single",
                InteractionQuestionType::SingleSelect,
                vec![option("a", "Use A"), option("b", "Use B")],
            ),
            question(
                "multi",
                InteractionQuestionType::MultiSelect,
                vec![option("a", "Use A"), option("b", "Use B")],
            ),
            question("text", InteractionQuestionType::Text, Vec::new()),
        ];
        let answers = vec![
            QuestionAnswer {
                question_id: "single".into(),
                option_ids: Vec::new(),
                custom_response: Some("A different approach".into()),
            },
            QuestionAnswer {
                question_id: "multi".into(),
                option_ids: vec!["a".into(), "b".into()],
                custom_response: Some("Plus logging".into()),
            },
            QuestionAnswer {
                question_id: "text".into(),
                option_ids: Vec::new(),
                custom_response: Some("Keep it concise".into()),
            },
        ];
        validate_question_answers(&questions, &answers).unwrap();

        let duplicate_multi = vec![QuestionAnswer {
            question_id: "multi".into(),
            option_ids: vec!["a".into(), "a".into()],
            custom_response: None,
        }];
        assert!(validate_question_answers(&questions[1..2], &duplicate_multi).is_err());
    }

    #[test]
    fn plugin_action_rejects_blocked_and_server_managed_plugins() {
        let call = ToolCall::new(
            "call-1",
            "request_plugin_action",
            serde_json::json!({"plugin_id": "blocked", "rationale": "Useful"}),
        );
        let blocked = PluginCatalogItem {
            id: "blocked".into(),
            name: "Blocked".into(),
            brief: "Blocked plugin".into(),
            state: "blocked".into(),
            user_managed: true,
            permissions: Vec::new(),
            allowed_hosts: Vec::new(),
            setup_guide: None,
        };
        assert!(build_interaction_payload(&call, &[blocked]).is_err());

        let managed = PluginCatalogItem {
            id: "blocked".into(),
            name: "Managed".into(),
            brief: "Managed plugin".into(),
            state: "disabled".into(),
            user_managed: false,
            permissions: Vec::new(),
            allowed_hosts: Vec::new(),
            setup_guide: None,
        };
        assert!(build_interaction_payload(&call, &[managed]).is_err());
    }
}
