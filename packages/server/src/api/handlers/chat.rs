use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use helpcore_api::{
    ChatRequest, ConversationSummary, MessageSummary, RetryRequest, SseChunk, SseDone, SseStarted,
    SseToolCall, SseToolResult,
};

use helpcore_api::CompactResponse;

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

// ── POST /chat ────────────────────────────────────────────────────────────────

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
    }));

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}

// ── POST /conversations/:id/messages/:message_id/retry ───────────────────────

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

    let (conversation, history, user_message, provider_id, model_used) = state
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

    let make_opts = || ContextOptions {
        soul: personality.soul.as_deref(),
        identity: personality.identity.as_deref(),
        user_profile: personality.user_profile.as_deref(),
        memories: &mem_results,
        plugin_skills: &plugin_skills,
        timezone: Some(job.user_timezone.as_str()),
        memory_leaning: Some(job.user_memory_leaning.as_str()),
    };

    let probe = context::assemble(&job.history, &job.user_content, make_opts());
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

    let mut messages = context::assemble(&job.history, &job.user_content, make_opts());
    for round in 0..=8 {
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
        if round == 8 {
            anyhow::bail!("plugin tool loop exceeded the maximum of eight rounds");
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
            let result = match tool_catalog.execute(&job.state, &job.user_id, call).await {
                Ok(result) => serde_json::json!({"ok": true, "result": result}).to_string(),
                Err(error) => {
                    serde_json::json!({"ok": false, "error": error.to_string()}).to_string()
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
    }
    unreachable!()
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
        AppError::NotFound
    } else {
        AppError::BadRequest(message)
    }
}

// ── GET /conversations ────────────────────────────────────────────────────────

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

// ── GET /conversations/:id/messages ──────────────────────────────────────────

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
        return Err(AppError::NotFound);
    }

    let msgs = state
        .db
        .call(move |conn| history::load_messages(conn, &conv_id))
        .await?;

    Ok(Json(msgs))
}

// ── DELETE /conversations/:id ─────────────────────────────────────────────────

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
        Err(AppError::NotFound)
    }
}

// ── POST /conversations/:id/cancel ───────────────────────────────────────────

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
        return Err(AppError::NotFound);
    }

    let interrupted = state
        .db
        .call(move |conn| history::interrupt_active_messages_for_conversation(conn, &cid2))
        .await?;

    Ok(Json(serde_json::json!({ "interrupted": interrupted })))
}

// ── POST /conversations/:id/compact ──────────────────────────────────────────

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
        .ok_or(AppError::NotFound)?;

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
}
