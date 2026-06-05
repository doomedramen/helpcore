use std::{convert::Infallible, sync::Arc};

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use helpcore_api::{ChatRequest, ConversationSummary, MessageSummary, SseChunk, SseDone};

use helpcore_api::CompactResponse;

use crate::{
    api::{error::AppError, extractor::AuthUser},
    conversation::{compact, context, context::ContextOptions, history, memory},
    plugins::registry,
    state::AppState,
};

// ── POST /chat ────────────────────────────────────────────────────────────────

pub async fn chat(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, AppError> {
    // Select provider.
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

    // Load or create conversation, then load history.
    let user_id = auth_user.id.clone();
    let conv_id_req = req.conversation_id.clone();
    let (conversation, history) = state
        .db
        .call(move |conn| {
            let conv = history::get_or_create(conn, &user_id, conv_id_req.as_deref())?;
            let msgs = history::load_messages(conn, &conv.id)?;
            Ok((conv, msgs))
        })
        .await?;

    // Insert the user message immediately so it's persisted even if provider fails.
    let conv_id = conversation.id.clone();
    let user_content = req.message.clone();
    state
        .db
        .call(move |conn| history::insert_user_message(conn, &conv_id, &user_content))
        .await?;

    // Load personality, memory, and plugin skills for this user.
    let user_id_ctx = auth_user.id.clone();
    let user_message_ctx = req.message.clone();
    let (personality, mem_results, plugin_skills) = state
        .db
        .call(move |conn| {
            let p = memory::load_personality(conn, &user_id_ctx)?;
            let m = memory::search_memory(conn, &user_id_ctx, &user_message_ctx, 10)?;
            let s = registry::enabled_skills(conn, &user_id_ctx)?;
            Ok((p, m, s))
        })
        .await?;

    // Build context options (used twice below — once for compaction check, once for the call).
    let make_opts = || ContextOptions {
        soul:          personality.soul.as_deref(),
        identity:      personality.identity.as_deref(),
        user_profile:  personality.user_profile.as_deref(),
        memories:      &mem_results,
        plugin_skills: &plugin_skills,
    };

    // Auto-compact: if the assembled context would exceed 90% of the context
    // window, summarise the oldest third of the conversation first.
    let history = {
        let probe = context::assemble(&history, &req.message, make_opts());
        if compact::needs_compaction(&probe, provider.context_limit()) {
            tracing::info!(
                conversation_id = %conversation.id,
                "context window approaching limit — auto-compacting"
            );
            if let Err(e) = compact::compact_conversation(&state.db, &conversation.id, &provider).await {
                tracing::error!(error = %e, "auto-compact failed — continuing without compaction");
                history
            } else {
                // Reload history with the new summary in place.
                let cid = conversation.id.clone();
                state.db.call(move |conn| history::load_messages(conn, &cid)).await?
            }
        } else {
            history
        }
    };

    // Assemble final context and call provider.
    let messages = context::assemble(&history, &req.message, make_opts());
    let model_override = req.model.clone();
    let provider_stream = provider
        .complete(&messages, model_override.as_deref())
        .await
        .map_err(|e| AppError::BadRequest(format!("provider error: {e}")))?;

    // Build the SSE stream: forward chunks, save assistant message when done.
    let db = Arc::clone(&state.db);
    let conv_id = conversation.id.clone();
    let provider_id = provider.id().to_string();
    let model_used = model_override
        .unwrap_or_else(|| provider.default_model().to_string());

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);

    tokio::spawn(async move {
        let mut stream = provider_stream;
        let mut full_response = String::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) if !chunk.is_final => {
                    full_response.push_str(&chunk.delta);
                    let event = Event::default()
                        .event("chunk")
                        .json_data(SseChunk { delta: chunk.delta })
                        .unwrap_or_else(|_| Event::default());
                    if tx.send(Ok(event)).await.is_err() {
                        return; // client disconnected
                    }
                }
                Ok(_) => {
                    // Stream finished — persist the assistant message.
                    let response = full_response.clone();
                    let cid = conv_id.clone();
                    let pid = provider_id.clone();
                    let mdl = model_used.clone();
                    let msg_id = db
                        .call(move |conn| {
                            history::insert_assistant_message(conn, &cid, &response, &pid, &mdl)
                        })
                        .await;

                    let event = match msg_id {
                        Ok(id) => Event::default()
                            .event("done")
                            .json_data(SseDone {
                                conversation_id: conv_id.clone(),
                                message_id: id,
                            })
                            .unwrap_or_else(|_| Event::default()),
                        Err(e) => {
                            tracing::error!(error = %e, "failed to save assistant message");
                            Event::default()
                                .event("error")
                                .data(format!("{{\"message\":\"failed to save response\"}}"))
                        }
                    };
                    let _ = tx.send(Ok(event)).await;
                    return;
                }
                Err(e) => {
                    let event = Event::default()
                        .event("error")
                        .data(format!("{{\"message\":\"{e}\"}}"));
                    let _ = tx.send(Ok(event)).await;
                    return;
                }
            }
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
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
