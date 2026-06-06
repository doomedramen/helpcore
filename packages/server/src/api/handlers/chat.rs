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
    ChatRequest, ConversationSummary, MessageSummary, SseChunk, SseDone, SseStarted,
};

use helpcore_api::CompactResponse;

use crate::{
    api::{error::AppError, extractor::AuthUser},
    conversation::{compact, context, context::ContextOptions, history, memory},
    plugins::registry,
    providers::traits::ChatProvider,
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
        .iter()
        .find(|p| {
            req.provider_id
                .as_deref()
                .map_or(true, |id| p.id() == id)
        })
        .cloned()
        .ok_or_else(|| {
            AppError::BadRequest(
                req.provider_id
                    .as_deref()
                    .map(|id| format!("unknown provider: {id}"))
                    .unwrap_or_else(|| "no providers configured".to_string()),
            )
        })?;

    let user_id = auth_user.id.clone();
    let conv_id_req = req.conversation_id.clone();
    let user_content = req.message.trim().to_string();
    if user_content.is_empty() {
        return Err(AppError::BadRequest("message cannot be empty".into()));
    }
    let provider_id = provider.id().to_string();
    let model_used = req
        .model
        .clone()
        .unwrap_or_else(|| provider.default_model().to_string());
    let provider_id_for_db = provider_id.clone();
    let model_for_db = model_used.clone();
    let content_for_db = user_content.clone();
    let started = state
        .db
        .call(move |conn| {
            history::start_turn(
                conn,
                &user_id,
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
        user_id: auth_user.id,
        user_content,
        user_sequence,
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
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, AppError> {
    let uid = auth_user.id.clone();
    let cid = conversation_id.clone();
    let mid = message_id.clone();
    let (conversation, history, user_message, provider_id, model_used) = state
        .db
        .call(move |conn| history::retry_turn(conn, &uid, &cid, &mid))
        .await
        .map_err(turn_error)?;
    let provider = state
        .providers
        .iter()
        .find(|provider| provider.id() == provider_id)
        .cloned()
        .ok_or_else(|| AppError::BadRequest(format!("provider {provider_id} is not available")))?;

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
    history: Vec<MessageSummary>,
    model_used: String,
    tx: EventSender,
}

async fn run_generation(job: GenerationJob) {
    if let Err(error) = generate(job).await {
        tracing::error!(error = %error, "chat generation failed");
    }
}

async fn generate(mut job: GenerationJob) -> anyhow::Result<()> {
    let user_id = job.user_id.clone();
    let user_message = job.user_content.clone();
    let (personality, mem_results, plugin_skills) = match job
        .state
        .db
        .call(move |conn| {
            let personality = memory::load_personality(conn, &user_id)?;
            let memories = memory::search_memory(conn, &user_id, &user_message, 10)?;
            let skills = registry::enabled_skills(conn, &user_id)?;
            Ok((personality, memories, skills))
        })
        .await
    {
        Ok(context) => context,
        Err(error) => return fail_job(&job, error.to_string()).await,
    };

    let make_opts = || ContextOptions {
        soul: personality.soul.as_deref(),
        identity: personality.identity.as_deref(),
        user_profile: personality.user_profile.as_deref(),
        memories: &mem_results,
        plugin_skills: &plugin_skills,
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

    let messages = context::assemble(&job.history, &job.user_content, make_opts());
    let mut stream = match job
        .provider
        .complete(&messages, Some(&job.model_used))
        .await
    {
        Ok(stream) => stream,
        Err(error) => return fail_job(&job, format!("provider error: {error}")).await,
    };

    let mut pending = String::new();
    let mut flush_tick = tokio::time::interval(Duration::from_millis(100));
    flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = flush_tick.tick(), if !pending.is_empty() => {
                flush_pending(&job, &mut pending).await?;
            }
            item = stream.next() => {
                match item {
                    Some(Ok(chunk)) if !chunk.is_final => {
                        pending.push_str(&chunk.delta);
                        if pending.len() >= 256 {
                            flush_pending(&job, &mut pending).await?;
                        }
                    }
                    Some(Ok(_)) => {
                        flush_pending(&job, &mut pending).await?;
                        let message_id = job.assistant_message_id.clone();
                        job.state.db.call(move |conn| {
                            history::finish_assistant_message(conn, &message_id)
                        }).await?;
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
                    Some(Err(error)) => {
                        flush_pending(&job, &mut pending).await?;
                        return fail_job(&job, error.to_string()).await;
                    }
                    None => {
                        flush_pending(&job, &mut pending).await?;
                        return fail_job(
                            &job,
                            "provider stream ended before completion".to_string(),
                        ).await;
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
        .call(move |conn| {
            history::fail_assistant_message(conn, &message_id, &persisted_error)
        })
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
            let c: Option<ConversationSummary> =
                history::get_conversation(conn, &cid, &user_id)?;
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

    if deleted { Ok(StatusCode::NO_CONTENT) } else { Err(AppError::NotFound) }
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
        .and_then(|pid| state.providers.iter().find(|p| p.id() == pid).cloned())
        .or_else(|| state.providers.first().cloned())
        .ok_or_else(|| AppError::BadRequest("no providers configured".into()))?;

    let result = compact::compact_conversation(&state.db, &conv.id, &provider)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "manual compact failed");
            AppError::BadRequest(format!("compaction failed: {e}"))
        })?;

    Ok(Json(CompactResponse {
        conversation_id:   conv.id,
        messages_compacted: result.messages_compacted,
        summary_length:     result.summary_length,
    }))
}
