//! Chat and conversation handlers — the core AI chat loop, conversation CRUD, feedback, and compaction.
//!
//! All endpoints under `/api/chat` and `/api/conversations`.

use std::{convert::Infallible, sync::Arc, time::Duration};

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
    ChatRequest, CompactResponse, ConversationSummary, MessageSummary, RenameConversationRequest,
    RetryRequest, SseChunk, SseContext, SseDone, SseStarted, SseToolCall, SseToolResult,
};

use crate::{
    api::{error::AppError, extractor::AuthUser},
    config::ProviderRole,
    conversation::{compact, context, context::ContextOptions, history, memory},
    plugins::{registry, runtime::ToolCatalog},
    providers::traits::ChatProvider,
    providers::types::{ChatMessage, ToolCall},
    state::AppState,
};

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
        resume_context: None,
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

    let (conversation, history, user_message, provider_id, model_used, retry_error) = state
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

    let resume_context = retry_error.map(|previous_error| {
        format!(
            "[Note: The previous response was paused after reaching the tool call limit. \
             The user has clicked 'Continue' to let you proceed.\n\n\
             Previous tool call summary:\n{previous_error}\n\n\
             Resume from where you left off. If you were in the middle of gathering \
             information, continue doing so. You have a fresh tool-round budget.]"
        )
    });
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
        resume_context,
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
    /// When set, the model's turn was resumed after interruption. The string
    /// contains a human-readable summary of what happened (tool-call list) that
    /// is injected into the message list so the model knows its context.
    resume_context: Option<String>,
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
fn tool_error_code(msg: &str) -> &str {
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
    } else if lower.contains("not declared") || lower.contains("not allow") {
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
    let (personality, mem_results, plugin_skills) = job
        .state
        .db
        .call(move |conn| {
            let personality = memory::load_personality(conn, &user_id)?;
            let memories = memory::search_memory(conn, &user_id, &recall_query, 10)?;
            let skills = registry::enabled_skills(conn, &user_id)?;
            Ok((personality, memories, skills))
        })
        .await?;
    let tool_catalog = ToolCatalog::load(&job.state, &job.user_id).await?;

    let make_opts = |est: usize| ContextOptions {
        soul: personality.soul.as_deref(),
        identity: personality.identity.as_deref(),
        user_profile: personality.user_profile.as_deref(),
        memories: &mem_results,
        plugin_skills: &plugin_skills,
        timezone: Some(job.user_timezone.as_str()),
        memory_leaning: Some(job.user_memory_leaning.as_str()),
        context_limit: Some(job.provider.context_limit()),
        estimated_tokens: Some(est),
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

    let probe = context::assemble(&job.history, &job.user_content, make_opts(initial_est));
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

    let mut messages = context::assemble(&job.history, &job.user_content, make_opts(est));

    // Emit context window usage to the frontend.
    let context_event = Event::default()
        .event("context")
        .json_data(SseContext {
            used_tokens: est,
            max_tokens: job.provider.context_limit(),
        })
        .unwrap_or_else(|_| Event::default());
    let _ = job.tx.send(Ok(context_event)).await;

    // Inject resume context when the user clicked "Continue" on an
    // interrupted message so the model knows it was paused and can
    // pick up where it left off.
    if let Some(ref ctx) = job.resume_context {
        messages.push(ChatMessage::user(ctx.as_str()));
    }

    let mut round: u32 = 0;
    loop {
        let (assistant_content, tool_calls) =
            complete_provider_round(job, &messages, tool_catalog.definitions()).await?;
        if tool_calls.is_empty() {
            let message_id = job.assistant_message_id.clone();
            job.state
                .db
                .call(move |conn| history::finish_assistant_message(conn, &message_id))
                .await?;
            let event = Event::default()
                .event("done")
                .json_data(SseDone {
                    conversation_id: job.conversation_id.clone(),
                    message_id: job.assistant_message_id.clone(),
                })
                .unwrap_or_else(|_| Event::default());
            let _ = job.tx.send(Ok(event)).await;
            return Ok(());
        }

        // Re-check the limit each iteration — the model may have called
        // `request_rounds` which increases max_rounds dynamically.
        let curr_max = tool_catalog.max_rounds();
        if round >= curr_max.saturating_sub(1) {
            // Build a summary of all tool calls made so far so the user and
            // model both know what was done before the pause.
            let summary_items: Vec<String> = messages
                .iter()
                .filter(|m| m.role == "tool")
                .map(|m| {
                    let name = m.tool_name.as_deref().unwrap_or("unknown");
                    let ok = m.content.contains("\"ok\":true");
                    let status = if ok { "ok" } else { "error" };
                    format!("{name} → {status}")
                })
                .collect();
            let summary = if summary_items.is_empty() {
                format!(
                    "Tool round limit reached ({round} rounds). The model had tool calls but no results persisted yet."
                )
            } else {
                let indexed: Vec<String> = summary_items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| format!("{}. {}", i + 1, item))
                    .collect();
                format!("Tool calls made ({round} rounds):\n{}", indexed.join("\n"))
            };

            // Mark the message as interrupted (not failed) so the UI can show
            // a "Continue" button instead of "Retry".
            let message_id = job.assistant_message_id.clone();
            let persisted_summary = summary.clone();
            job.state
                .db
                .call(move |conn| {
                    history::interrupt_assistant_message(conn, &message_id, &persisted_summary)
                })
                .await?;

            let event = Event::default()
                .event("interrupted")
                .json_data(serde_json::json!({ "message": summary }))
                .unwrap_or_else(|_| Event::default());
            let _ = job.tx.send(Ok(event)).await;
            return Ok(());
        }

        let mut results = Vec::with_capacity(tool_calls.len());
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
            let result = match tool_catalog
                .execute(&job.state, &job.user_id, Some(&job.conversation_id), call)
                .await
            {
                Ok(result) => serde_json::json!({"ok": true, "result": result}).to_string(),
                Err(error) => {
                    let msg = error.to_string();
                    let code = tool_error_code(&msg);
                    serde_json::json!({"ok": false, "error": msg, "code": code}).to_string()
                }
            };
            let result_event = Event::default()
                .event("tool_result")
                .json_data(SseToolResult {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    result: result.clone(),
                })
                .unwrap_or_else(|_| Event::default());
            let _ = job.tx.send(Ok(result_event)).await;
            results.push((call.clone(), result));
        }

        let message_id = job.assistant_message_id.clone();
        let persisted_calls = tool_calls.clone();
        let persisted_results = results.clone();
        job.state
            .db
            .call(move |conn| {
                history::persist_tool_round(conn, &message_id, &persisted_calls, &persisted_results)
            })
            .await?;
        messages.push(ChatMessage::assistant_with_tools(
            assistant_content,
            tool_calls,
        ));
        for (call, result) in results {
            messages.push(ChatMessage::tool(call.id, call.name, result));
        }

        // Inject a running counter so the model knows how many rounds remain.
        let remaining = curr_max.saturating_sub(round + 1);
        messages.push(ChatMessage::system(format!(
            "[Tool round: {}/{} — {} remaining before a pause is required]",
            round + 1,
            curr_max,
            remaining,
        )));
        round += 1;
    }
}

async fn complete_provider_round(
    job: &GenerationJob,
    messages: &[ChatMessage],
    tools: &[crate::providers::types::ToolDefinition],
) -> anyhow::Result<(String, Vec<ToolCall>)> {
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
                            return Ok((assistant_content, tool_calls));
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn tool_error_code_defaults_to_unknown() {
        assert_eq!(tool_error_code("some unexpected failure"), "UNKNOWN");
    }
}
