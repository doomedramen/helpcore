//! Durable user interactions that pause and resume assistant tool calls.

use anyhow::Context;
use chrono::Utc;
use helpcore_api::{InteractionPayload, PendingInteraction};
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

use crate::providers::types::{ToolCall, Usage};

/// Creates a pending interaction and changes the active assistant placeholder
/// to `awaiting_input`.
pub fn create(
    conn: &Connection,
    user_id: &str,
    conversation_id: &str,
    assistant_message_id: &str,
    call: &ToolCall,
    payload: &InteractionPayload,
    usage: Option<&Usage>,
) -> anyhow::Result<PendingInteraction> {
    let tx = conn.unchecked_transaction()?;
    let row = tx
        .query_row(
            "SELECT sequence, content, provider_id, model
             FROM messages
             WHERE id = ?1 AND conversation_id = ?2 AND role = 'assistant'
               AND status IN ('pending', 'streaming')",
            rusqlite::params![assistant_message_id, conversation_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?
        .context("assistant message is no longer active")?;
    let (sequence, content, provider_id, model) = row;
    let now = Utc::now().to_rfc3339();
    let input_tokens = usage.map(|value| value.input_tokens as i64);
    let output_tokens = usage.map(|value| value.output_tokens as i64);
    let cache_read = usage.and_then(|value| value.cache_read_tokens.map(|count| count as i64));
    let cache_write = usage.and_then(|value| value.cache_write_tokens.map(|count| count as i64));

    tx.execute(
        "UPDATE messages
         SET sequence = sequence + 1, content = '', status = 'awaiting_input',
             updated_at = ?1
         WHERE id = ?2",
        rusqlite::params![now, assistant_message_id],
    )?;
    tx.execute(
        "INSERT INTO messages
         (id, conversation_id, role, content, tool_calls, provider_id, model,
          sequence, status, created_at, updated_at,
          input_tokens, output_tokens, cache_read_tokens, cache_write_tokens)
         VALUES (?1, ?2, 'assistant', ?3, ?4, ?5, ?6, ?7, 'complete', ?8, ?8,
                 ?9, ?10, ?11, ?12)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            conversation_id,
            content,
            serde_json::to_string(&[call])?,
            provider_id,
            model,
            sequence,
            now,
            input_tokens,
            output_tokens,
            cache_read,
            cache_write,
        ],
    )?;

    let interaction = PendingInteraction {
        id: Uuid::new_v4().to_string(),
        conversation_id: conversation_id.to_string(),
        message_id: assistant_message_id.to_string(),
        tool_call_id: call.id.clone(),
        payload: payload.clone(),
        created_at: now.clone(),
    };
    tx.execute(
        "INSERT INTO interactions
         (id, conversation_id, user_id, assistant_message_id, tool_call_id,
          tool_name, payload, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, ?8)",
        rusqlite::params![
            interaction.id,
            interaction.conversation_id,
            user_id,
            interaction.message_id,
            interaction.tool_call_id,
            call.name,
            serde_json::to_string(payload)?,
            now,
        ],
    )?;
    tx.execute(
        "UPDATE conversations
         SET message_count = message_count + 1, updated_at = ?1
         WHERE id = ?2",
        rusqlite::params![now, conversation_id],
    )?;
    tx.commit()?;
    Ok(interaction)
}

/// Returns the pending interaction for a user-owned conversation.
pub fn get_pending(
    conn: &Connection,
    user_id: &str,
    conversation_id: &str,
) -> anyhow::Result<Option<PendingInteraction>> {
    conn.query_row(
        "SELECT id, conversation_id, assistant_message_id, tool_call_id,
                payload, created_at
         FROM interactions
         WHERE user_id = ?1 AND conversation_id = ?2 AND status = 'pending'",
        rusqlite::params![user_id, conversation_id],
        row_to_interaction,
    )
    .optional()
    .map_err(Into::into)
}

/// Returns a specific pending interaction owned by the user.
pub fn get_pending_by_id(
    conn: &Connection,
    user_id: &str,
    conversation_id: &str,
    interaction_id: &str,
) -> anyhow::Result<Option<PendingInteraction>> {
    conn.query_row(
        "SELECT id, conversation_id, assistant_message_id, tool_call_id,
                payload, created_at
         FROM interactions
         WHERE id = ?1 AND user_id = ?2 AND conversation_id = ?3
           AND status = 'pending'",
        rusqlite::params![interaction_id, user_id, conversation_id],
        row_to_interaction,
    )
    .optional()
    .map_err(Into::into)
}

/// Replaces the public payload while keeping the interaction pending.
pub fn update_payload(
    conn: &Connection,
    interaction_id: &str,
    payload: &InteractionPayload,
) -> anyhow::Result<()> {
    let changed = conn.execute(
        "UPDATE interactions SET payload = ?1, updated_at = ?2
         WHERE id = ?3 AND status = 'pending'",
        rusqlite::params![
            serde_json::to_string(payload)?,
            Utc::now().to_rfc3339(),
            interaction_id
        ],
    )?;
    if changed == 0 {
        anyhow::bail!("interaction is no longer pending");
    }
    Ok(())
}

/// Inserts the interaction's tool result and reactivates the assistant placeholder.
pub fn complete(
    conn: &Connection,
    interaction: &PendingInteraction,
    result: &str,
    dismissed: bool,
) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    let sequence: i64 = tx
        .query_row(
            "SELECT sequence FROM messages
             WHERE id = ?1 AND status = 'awaiting_input'",
            [&interaction.message_id],
            |row| row.get(0),
        )
        .context("assistant message is not awaiting input")?;
    let now = Utc::now().to_rfc3339();
    tx.execute(
        "UPDATE messages
         SET sequence = sequence + 1, status = 'pending', updated_at = ?1
         WHERE id = ?2",
        rusqlite::params![now, interaction.message_id],
    )?;
    tx.execute(
        "INSERT INTO messages
         (id, conversation_id, role, content, tool_call_id, sequence,
          status, created_at, updated_at)
         VALUES (?1, ?2, 'tool', ?3, ?4, ?5, 'complete', ?6, ?6)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            interaction.conversation_id,
            result,
            interaction.tool_call_id,
            sequence,
            now,
        ],
    )?;
    tx.execute(
        "UPDATE interactions SET status = ?1, updated_at = ?2
         WHERE id = ?3 AND status = 'pending'",
        rusqlite::params![
            if dismissed { "dismissed" } else { "completed" },
            now,
            interaction.id
        ],
    )?;
    tx.execute(
        "UPDATE conversations
         SET message_count = message_count + 1, updated_at = ?1
         WHERE id = ?2",
        rusqlite::params![now, interaction.conversation_id],
    )?;
    tx.commit()?;
    Ok(())
}

fn row_to_interaction(row: &rusqlite::Row<'_>) -> rusqlite::Result<PendingInteraction> {
    let payload: String = row.get(4)?;
    let payload = serde_json::from_str(&payload).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            payload.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    Ok(PendingInteraction {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        message_id: row.get(2)?,
        tool_call_id: row.get(3)?,
        payload,
        created_at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{conversation::history, db::open_in_memory};
    use helpcore_api::{InteractionQuestion, InteractionQuestionType, QuestionOption};

    fn setup_turn() -> (crate::db::DbPool, String, String, String) {
        let pool = open_in_memory();
        let user_id = "interaction-user".to_string();
        let user_for_db = user_id.clone();
        let (conversation_id, assistant_message_id) = pool
            .call_sync(move |conn| {
                conn.execute(
                    "INSERT INTO users
                     (id, email, password_hash, role, status, created_at, updated_at)
                     VALUES (?1, 'interaction@example.com', 'hash', 'member', 'active',
                             '2026-01-01', '2026-01-01')",
                    [&user_for_db],
                )?;
                let turn =
                    history::start_turn(conn, &user_for_db, None, "Help me", "provider", "model")?;
                Ok((turn.conversation.id, turn.assistant_message_id))
            })
            .unwrap();
        (pool, user_id, conversation_id, assistant_message_id)
    }

    fn payload() -> InteractionPayload {
        InteractionPayload::Questions {
            questions: vec![InteractionQuestion {
                id: "choice".into(),
                header: "Choice".into(),
                question: "Pick one".into(),
                question_type: InteractionQuestionType::SingleSelect,
                options: vec![
                    QuestionOption {
                        id: "a".into(),
                        label: "A".into(),
                        description: "Use A".into(),
                    },
                    QuestionOption {
                        id: "b".into(),
                        label: "B".into(),
                        description: "Use B".into(),
                    },
                ],
            }],
        }
    }

    #[test]
    fn interaction_survives_reads_and_resumes_the_same_placeholder() {
        let (pool, user_id, conversation_id, assistant_message_id) = setup_turn();
        let call = ToolCall::new("tool-call", "request_user_input", serde_json::json!({}));
        let pending = pool
            .call_sync(|conn| {
                create(
                    conn,
                    &user_id,
                    &conversation_id,
                    &assistant_message_id,
                    &call,
                    &payload(),
                    None,
                )
            })
            .unwrap();

        pool.call_sync(|conn| {
            let restored = get_pending(conn, &user_id, &conversation_id)?.unwrap();
            assert_eq!(restored.id, pending.id);
            let messages = history::load_messages(conn, &conversation_id)?;
            assert!(messages.iter().any(|message| {
                message.id == assistant_message_id
                    && message.status == helpcore_api::MessageStatus::AwaitingInput
            }));
            assert!(
                history::start_turn(
                    conn,
                    &user_id,
                    Some(&conversation_id),
                    "Another turn",
                    "provider",
                    "model"
                )
                .is_err()
            );
            assert!(
                history::retry_turn(
                    conn,
                    &user_id,
                    &conversation_id,
                    &assistant_message_id,
                    None
                )
                .is_err()
            );
            complete(conn, &pending, r#"{"ok":true}"#, false)?;
            assert!(get_pending(conn, &user_id, &conversation_id)?.is_none());
            let messages = history::load_messages(conn, &conversation_id)?;
            assert!(messages.iter().any(|message| {
                message.id == assistant_message_id
                    && message.status == helpcore_api::MessageStatus::Pending
            }));
            assert!(messages.iter().any(|message| {
                message.tool_call_id.as_deref() == Some("tool-call")
                    && message.content == r#"{"ok":true}"#
            }));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn pending_interactions_are_user_scoped_and_unique_per_conversation() {
        let (pool, user_id, conversation_id, assistant_message_id) = setup_turn();
        let call = ToolCall::new("tool-call", "request_user_input", serde_json::json!({}));
        pool.call_sync(|conn| {
            create(
                conn,
                &user_id,
                &conversation_id,
                &assistant_message_id,
                &call,
                &payload(),
                None,
            )?;
            assert!(get_pending(conn, "another-user", &conversation_id)?.is_none());
            assert!(
                create(
                    conn,
                    &user_id,
                    &conversation_id,
                    &assistant_message_id,
                    &call,
                    &payload(),
                    None,
                )
                .is_err()
            );
            Ok(())
        })
        .unwrap();
    }
}
