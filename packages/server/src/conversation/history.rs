use anyhow::Context;
use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use helpcore_api::{ConversationSummary, MessageStatus, MessageSummary};

pub struct StartedTurn {
    pub conversation: ConversationSummary,
    pub history: Vec<MessageSummary>,
    pub user_message: MessageSummary,
    pub assistant_message_id: String,
}

/// Loads a conversation that belongs to `user_id`. Returns `None` if not found
/// or if it belongs to another user (caller should 404 in both cases).
pub fn get_conversation(
    conn: &Connection,
    conversation_id: &str,
    user_id: &str,
) -> anyhow::Result<Option<ConversationSummary>> {
    match conn.query_row(
        "SELECT id, title, provider_id, model, message_count, created_at, updated_at
         FROM conversations WHERE id = ?1 AND user_id = ?2",
        rusqlite::params![conversation_id, user_id],
        row_to_summary,
    ) {
        Ok(c) => Ok(Some(c)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::Error::from(e)),
    }
}

/// Creates a new conversation with a placeholder title. The title is updated
/// to the first user message when `insert_user_message` is called.
pub fn create_conversation(
    conn: &Connection,
    user_id: &str,
) -> anyhow::Result<ConversationSummary> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO conversations (id, user_id, title, message_count, created_at, updated_at)
         VALUES (?1, ?2, 'New conversation', 0, ?3, ?3)",
        rusqlite::params![id, user_id, now],
    )
    .context("failed to create conversation")?;
    get_conversation(conn, &id, user_id)?.context("conversation missing after insert")
}

/// Returns an existing conversation (verified to belong to `user_id`) or
/// creates a new one if `conversation_id` is `None`.
pub fn get_or_create(
    conn: &Connection,
    user_id: &str,
    conversation_id: Option<&str>,
) -> anyhow::Result<ConversationSummary> {
    match conversation_id {
        Some(id) => get_conversation(conn, id, user_id)?
            .ok_or_else(|| anyhow::anyhow!("conversation not found")),
        None => create_conversation(conn, user_id),
    }
}

/// Loads all non-compacted messages for a conversation, ordered by sequence.
pub fn load_messages(
    conn: &Connection,
    conversation_id: &str,
) -> anyhow::Result<Vec<MessageSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, role, content, tool_call_id, tool_calls, sequence, created_at,
                status, error, COALESCE(updated_at, created_at)
         FROM messages
         WHERE conversation_id = ?1 AND compacted = 0
         ORDER BY sequence ASC",
    )?;
    let rows = stmt
        .query_map([conversation_id], row_to_message)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn load_messages_before(
    conn: &Connection,
    conversation_id: &str,
    sequence: i64,
) -> anyhow::Result<Vec<MessageSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, role, content, tool_call_id, tool_calls, sequence, created_at,
                status, error, COALESCE(updated_at, created_at)
         FROM messages
         WHERE conversation_id = ?1 AND compacted = 0 AND sequence < ?2
         ORDER BY sequence ASC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![conversation_id, sequence], row_to_message)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn start_turn(
    conn: &Connection,
    user_id: &str,
    conversation_id: Option<&str>,
    content: &str,
    provider_id: &str,
    model: &str,
) -> anyhow::Result<StartedTurn> {
    let tx = conn.unchecked_transaction()?;
    let conversation = get_or_create(&tx, user_id, conversation_id)?;
    ensure_no_active_generation(&tx, &conversation.id, None)?;
    let history = load_messages(&tx, &conversation.id)?;
    let user_message = insert_user_message(&tx, &conversation.id, content)?;
    let assistant_message_id =
        insert_assistant_placeholder(&tx, &conversation.id, provider_id, model)?;
    tx.commit()?;

    Ok(StartedTurn {
        conversation,
        history,
        user_message,
        assistant_message_id,
    })
}

pub fn retry_turn(
    conn: &Connection,
    user_id: &str,
    conversation_id: &str,
    assistant_message_id: &str,
    override_provider: Option<(&str, &str)>,
) -> anyhow::Result<(
    ConversationSummary,
    Vec<MessageSummary>,
    MessageSummary,
    String,
    String,
)> {
    let tx = conn.unchecked_transaction()?;
    let conversation =
        get_conversation(&tx, conversation_id, user_id)?.context("conversation not found")?;
    ensure_no_active_generation(&tx, conversation_id, Some(assistant_message_id))?;

    let (assistant_sequence, status, provider_id, model): (
        i64,
        String,
        Option<String>,
        Option<String>,
    ) = tx
        .query_row(
            "SELECT sequence, status, provider_id, model
             FROM messages
             WHERE id = ?1 AND conversation_id = ?2 AND role = 'assistant'",
            rusqlite::params![assistant_message_id, conversation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .context("assistant message not found")?;

    if matches!(status.as_str(), "pending" | "streaming" | "complete") {
        anyhow::bail!("only failed or interrupted responses can be retried");
    }

    let user_message = tx
        .query_row(
            "SELECT id, role, content, tool_call_id, tool_calls, sequence, created_at,
                    status, error, COALESCE(updated_at, created_at)
             FROM messages
             WHERE conversation_id = ?1 AND role = 'user' AND sequence < ?2
             ORDER BY sequence DESC LIMIT 1",
            rusqlite::params![conversation_id, assistant_sequence],
            row_to_message,
        )
        .context("original user message not found")?;
    let history = load_messages_before(&tx, conversation_id, user_message.sequence)?;
    let now = Utc::now().to_rfc3339();
    if let Some((new_provider_id, new_model)) = override_provider {
        tx.execute(
            "UPDATE messages
             SET content = '', status = 'pending', error = NULL,
                 provider_id = ?2, model = ?3, updated_at = ?1
             WHERE id = ?4",
            rusqlite::params![now, new_provider_id, new_model, assistant_message_id],
        )?;
    } else {
        tx.execute(
            "UPDATE messages
             SET content = '', status = 'pending', error = NULL, updated_at = ?1
             WHERE id = ?2",
            rusqlite::params![now, assistant_message_id],
        )?;
    }
    tx.commit()?;

    let final_provider = override_provider
        .map(|(pid, _)| pid.to_string())
        .or(provider_id);
    let final_model = override_provider.map(|(_, m)| m.to_string()).or(model);

    Ok((
        conversation,
        history,
        user_message,
        final_provider.context("retry response has no provider")?,
        final_model.context("retry response has no model")?,
    ))
}

fn ensure_no_active_generation(
    conn: &Connection,
    conversation_id: &str,
    except_message_id: Option<&str>,
) -> anyhow::Result<()> {
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages
         WHERE conversation_id = ?1
           AND role = 'assistant'
           AND status IN ('pending', 'streaming')
           AND (?2 IS NULL OR id != ?2)",
        rusqlite::params![conversation_id, except_message_id],
        |row| row.get(0),
    )?;
    if active > 0 {
        anyhow::bail!("a response is already in progress for this conversation");
    }
    Ok(())
}

/// Inserts a user message. On the first message in the conversation, updates
/// the conversation title to the first ~60 chars of the content.
/// Returns the inserted message.
pub fn insert_user_message(
    conn: &Connection,
    conversation_id: &str,
    content: &str,
) -> anyhow::Result<MessageSummary> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    // Sequence = current message_count + 1 (message_count is incremented below).
    let current_count: i64 = conn.query_row(
        "SELECT message_count FROM conversations WHERE id = ?1",
        [conversation_id],
        |r| r.get(0),
    )?;
    let sequence = current_count + 1;

    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, sequence, created_at)
         VALUES (?1, ?2, 'user', ?3, ?4, ?5)",
        rusqlite::params![id, conversation_id, content, sequence, now],
    )
    .context("failed to insert user message")?;

    conn.execute(
        "UPDATE conversations SET message_count = message_count + 1, updated_at = ?1 WHERE id = ?2",
        rusqlite::params![now, conversation_id],
    )?;

    // Set title from first user message.
    if current_count == 0 {
        let title = truncate_title(content);
        conn.execute(
            "UPDATE conversations SET title = ?1 WHERE id = ?2",
            rusqlite::params![title, conversation_id],
        )?;
    }

    Ok(MessageSummary {
        id,
        role: "user".to_string(),
        content: content.to_string(),
        tool_call_id: None,
        tool_calls: None,
        sequence,
        created_at: now.clone(),
        status: MessageStatus::Complete,
        error: None,
        updated_at: now,
    })
}

/// Inserts an empty durable assistant row before generation starts.
pub fn insert_assistant_placeholder(
    conn: &Connection,
    conversation_id: &str,
    provider_id: &str,
    model: &str,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let current_count: i64 = conn.query_row(
        "SELECT message_count FROM conversations WHERE id = ?1",
        [conversation_id],
        |r| r.get(0),
    )?;
    let sequence = current_count + 1;

    conn.execute(
        "INSERT INTO messages
             (id, conversation_id, role, content, provider_id, model, sequence,
              status, created_at, updated_at)
         VALUES (?1, ?2, 'assistant', '', ?3, ?4, ?5, 'pending', ?6, ?6)",
        rusqlite::params![id, conversation_id, provider_id, model, sequence, now],
    )
    .context("failed to insert assistant placeholder")?;

    conn.execute(
        "UPDATE conversations
         SET message_count = message_count + 1,
             updated_at    = ?1,
             provider_id   = ?2,
             model         = ?3
         WHERE id = ?4",
        rusqlite::params![now, provider_id, model, conversation_id],
    )?;

    Ok(id)
}

pub fn append_assistant_content(
    conn: &Connection,
    message_id: &str,
    content: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    let changed = conn.execute(
        "UPDATE messages
         SET content = content || ?1, status = 'streaming', updated_at = ?2
         WHERE id = ?3 AND role = 'assistant' AND status IN ('pending', 'streaming')",
        rusqlite::params![content, now, message_id],
    )?;
    if changed == 0 {
        anyhow::bail!("assistant message is not active");
    }
    Ok(())
}

pub fn finish_assistant_message(conn: &Connection, message_id: &str) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE messages SET status = 'complete', error = NULL, updated_at = ?1
         WHERE id = ?2 AND role = 'assistant'",
        rusqlite::params![now, message_id],
    )?;
    Ok(())
}

pub fn persist_tool_round(
    conn: &Connection,
    assistant_message_id: &str,
    tool_calls: &[crate::providers::types::ToolCall],
    results: &[(crate::providers::types::ToolCall, String)],
) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    let (conversation_id, sequence, content, provider_id, model): (
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
    ) = tx
        .query_row(
            "SELECT conversation_id, sequence, content, provider_id, model
             FROM messages
             WHERE id = ?1 AND role = 'assistant' AND status IN ('pending', 'streaming')",
            [assistant_message_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .context("assistant message is not active")?;
    let inserted_count = 1_i64 + results.len() as i64;
    let now = Utc::now().to_rfc3339();
    tx.execute(
        "UPDATE messages
         SET sequence = sequence + ?1, content = '', status = 'pending', updated_at = ?2
         WHERE id = ?3",
        rusqlite::params![inserted_count, now, assistant_message_id],
    )?;
    tx.execute(
        "INSERT INTO messages
         (id, conversation_id, role, content, tool_calls, provider_id, model,
          sequence, status, created_at, updated_at)
         VALUES (?1, ?2, 'assistant', ?3, ?4, ?5, ?6, ?7, 'complete', ?8, ?8)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            conversation_id,
            content,
            serde_json::to_string(tool_calls)?,
            provider_id,
            model,
            sequence,
            now
        ],
    )?;
    for (index, (call, result)) in results.iter().enumerate() {
        tx.execute(
            "INSERT INTO messages
             (id, conversation_id, role, content, tool_call_id, sequence,
              status, created_at, updated_at)
             VALUES (?1, ?2, 'tool', ?3, ?4, ?5, 'complete', ?6, ?6)",
            rusqlite::params![
                Uuid::new_v4().to_string(),
                conversation_id,
                result,
                call.id,
                sequence + 1 + index as i64,
                now
            ],
        )?;
    }
    tx.execute(
        "UPDATE conversations
         SET message_count = message_count + ?1, updated_at = ?2
         WHERE id = ?3",
        rusqlite::params![inserted_count, now, conversation_id],
    )?;
    tx.commit()?;
    Ok(())
}

/// Convenience helper for non-streaming internal writes and tests.
pub fn insert_assistant_message(
    conn: &Connection,
    conversation_id: &str,
    content: &str,
    provider_id: &str,
    model: &str,
) -> anyhow::Result<String> {
    let id = insert_assistant_placeholder(conn, conversation_id, provider_id, model)?;
    if !content.is_empty() {
        append_assistant_content(conn, &id, content)?;
    }
    finish_assistant_message(conn, &id)?;
    Ok(id)
}

pub fn fail_assistant_message(
    conn: &Connection,
    message_id: &str,
    error: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE messages SET status = 'failed', error = ?1, updated_at = ?2
         WHERE id = ?3 AND role = 'assistant'",
        rusqlite::params![error, now, message_id],
    )?;
    Ok(())
}

pub fn interrupt_active_messages(conn: &Connection) -> anyhow::Result<usize> {
    let now = Utc::now().to_rfc3339();
    Ok(conn.execute(
        "UPDATE messages
         SET status = 'interrupted',
              error = 'The server restarted while this response was being generated.',
              updated_at = ?1
         WHERE role = 'assistant' AND status IN ('pending', 'streaming')",
        [now],
    )?)
}

pub fn interrupt_active_messages_for_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> anyhow::Result<usize> {
    let now = Utc::now().to_rfc3339();
    Ok(conn.execute(
        "UPDATE messages
         SET status = 'interrupted',
             error = 'Generation was cancelled by the user.',
             updated_at = ?1
         WHERE conversation_id = ?2
           AND role = 'assistant'
           AND status IN ('pending', 'streaming')",
        rusqlite::params![now, conversation_id],
    )?)
}

/// A message row returned for compaction analysis (includes the compacted flag).
pub struct CompactableMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub sequence: i64,
}

/// Returns all non-compacted, non-summary messages ordered by sequence.
/// The first message (the conversation anchor) is tagged — it must not be compacted.
pub fn load_compactable_messages(
    conn: &Connection,
    conversation_id: &str,
) -> anyhow::Result<Vec<CompactableMessage>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, role, content, sequence
           FROM messages
          WHERE conversation_id = ?1
            AND compacted = 0
            AND role != 'summary'
            AND status = 'complete'
          ORDER BY sequence ASC",
    )?;
    let rows = stmt
        .query_map([conversation_id], |row| {
            Ok(CompactableMessage {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                sequence: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Marks a set of messages as compacted (soft-delete — rows are retained for export).
pub fn mark_compacted(conn: &Connection, ids: &[String]) -> anyhow::Result<()> {
    for id in ids {
        conn.execute("UPDATE messages SET compacted = 1 WHERE id = ?1", [id])?;
    }
    Ok(())
}

/// Inserts a `summary` role message at an explicit sequence position.
/// Does NOT use message_count + 1 for the sequence — the caller provides it
/// so the summary slots in where the compacted segment started.
pub fn insert_summary_message(
    conn: &Connection,
    conversation_id: &str,
    content: &str,
    sequence: i64,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, sequence, created_at)
         VALUES (?1, ?2, 'summary', ?3, ?4, ?5)",
        rusqlite::params![id, conversation_id, content, sequence, now],
    )?;
    // Bump message_count so future sequence numbers stay monotonically increasing.
    conn.execute(
        "UPDATE conversations SET message_count = message_count + 1, updated_at = ?1 WHERE id = ?2",
        rusqlite::params![now, conversation_id],
    )?;
    Ok(id)
}

/// Lists conversations for a user, newest first.
pub fn list_conversations(
    conn: &Connection,
    user_id: &str,
) -> anyhow::Result<Vec<ConversationSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, provider_id, model, message_count, created_at, updated_at
         FROM conversations WHERE user_id = ?1 ORDER BY updated_at DESC",
    )?;
    let rows = stmt
        .query_map([user_id], row_to_summary)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationSummary> {
    Ok(ConversationSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        provider_id: row.get(2)?,
        model: row.get(3)?,
        message_count: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageSummary> {
    let tool_calls: Option<String> = row.get(4)?;
    let status: String = row.get(7)?;
    Ok(MessageSummary {
        id: row.get(0)?,
        role: row.get(1)?,
        content: row.get(2)?,
        tool_call_id: row.get(3)?,
        tool_calls: tool_calls.and_then(|value| serde_json::from_str(&value).ok()),
        sequence: row.get(5)?,
        created_at: row.get(6)?,
        status: match status.as_str() {
            "pending" => MessageStatus::Pending,
            "streaming" => MessageStatus::Streaming,
            "failed" => MessageStatus::Failed,
            "interrupted" => MessageStatus::Interrupted,
            _ => MessageStatus::Complete,
        },
        error: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

/// Truncates text to ≤60 chars at a word boundary, appending "…" if cut.
fn truncate_title(text: &str) -> String {
    let text = text.lines().next().unwrap_or("").trim();
    if text.chars().count() <= 60 {
        return text.to_string();
    }
    // Find byte offset of the 60th character boundary.
    let cut = text
        .char_indices()
        .nth(60)
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    let truncated = &text[..cut];
    match truncated.rfind(char::is_whitespace) {
        Some(pos) => format!("{}…", text[..pos].trim_end()),
        None => format!("{}…", truncated),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::open_test_db;
    use chrono::Utc;
    use uuid::Uuid;

    fn insert_user(pool: &crate::db::DbPool) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        pool.call_sync(|conn| {
            conn.execute(
                "INSERT INTO users (id, email, password_hash, role, status, created_at, updated_at)
                 VALUES (?1, ?2, 'h', 'member', 'active', ?3, ?3)",
                rusqlite::params![id, format!("{id}@t.local"), now],
            )?;
            Ok(())
        })
        .unwrap();
        id
    }

    #[test]
    fn create_and_list_conversation() {
        let pool = open_test_db();
        let uid = insert_user(&pool);
        pool.call_sync(|conn| {
            create_conversation(conn, &uid)?;
            let convs = list_conversations(conn, &uid)?;
            assert_eq!(convs.len(), 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn first_message_sets_title() {
        let pool = open_test_db();
        let uid = insert_user(&pool);
        pool.call_sync(|conn| {
            let conv = create_conversation(conn, &uid)?;
            insert_user_message(conn, &conv.id, "What is Rust?")?;
            let updated = list_conversations(conn, &uid)?;
            assert_eq!(updated[0].title, "What is Rust?");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn long_first_message_truncates_title() {
        let pool = open_test_db();
        let uid = insert_user(&pool);
        let long = "a".repeat(80);
        pool.call_sync(|conn| {
            let conv = create_conversation(conn, &uid)?;
            insert_user_message(conn, &conv.id, &long)?;
            let updated = list_conversations(conn, &uid)?;
            // 60 chars + "…" (1 char) = 61 chars
            assert!(updated[0].title.chars().count() <= 62);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn messages_load_in_order() {
        let pool = open_test_db();
        let uid = insert_user(&pool);
        pool.call_sync(|conn| {
            let conv = create_conversation(conn, &uid)?;
            insert_user_message(conn, &conv.id, "Hello")?;
            let assistant = insert_assistant_placeholder(conn, &conv.id, "p", "m")?;
            append_assistant_content(conn, &assistant, "Hi there!")?;
            finish_assistant_message(conn, &assistant)?;
            insert_user_message(conn, &conv.id, "How are you?")?;
            let msgs = load_messages(conn, &conv.id)?;
            assert_eq!(msgs.len(), 3);
            assert_eq!(msgs[0].role, "user");
            assert_eq!(msgs[1].role, "assistant");
            assert_eq!(msgs[2].role, "user");
            assert!(msgs[0].sequence < msgs[1].sequence);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn message_count_increments() {
        let pool = open_test_db();
        let uid = insert_user(&pool);
        pool.call_sync(|conn| {
            let conv = create_conversation(conn, &uid)?;
            insert_user_message(conn, &conv.id, "msg 1")?;
            let assistant = insert_assistant_placeholder(conn, &conv.id, "p", "m")?;
            append_assistant_content(conn, &assistant, "reply")?;
            finish_assistant_message(conn, &assistant)?;
            let convs = list_conversations(conn, &uid)?;
            assert_eq!(convs[0].message_count, 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn tool_round_is_persisted_before_active_placeholder() {
        let pool = open_test_db();
        let uid = insert_user(&pool);
        pool.call_sync(|conn| {
            let turn = start_turn(conn, &uid, None, "Weather?", "p", "m")?;
            append_assistant_content(conn, &turn.assistant_message_id, "Checking.")?;
            let call = crate::providers::types::ToolCall {
                id: "call-1".into(),
                name: "weather".into(),
                arguments: serde_json::json!({"city": "London"}),
            };
            persist_tool_round(
                conn,
                &turn.assistant_message_id,
                std::slice::from_ref(&call),
                &[(call.clone(), r#"{"ok":true,"result":"rain"}"#.into())],
            )?;
            let messages = load_messages(conn, &turn.conversation.id)?;
            assert_eq!(messages.len(), 4);
            assert_eq!(messages[1].role, "assistant");
            assert_eq!(messages[1].content, "Checking.");
            assert!(messages[1].tool_calls.is_some());
            assert_eq!(messages[2].role, "tool");
            assert_eq!(messages[2].tool_call_id.as_deref(), Some("call-1"));
            assert_eq!(messages[3].id, turn.assistant_message_id);
            assert_eq!(messages[3].status, MessageStatus::Pending);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn get_conversation_rejects_wrong_user() {
        let pool = open_test_db();
        let uid = insert_user(&pool);
        pool.call_sync(|conn| {
            let conv = create_conversation(conn, &uid)?;
            let result = get_conversation(conn, &conv.id, "other-user-id")?;
            assert!(result.is_none());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn truncate_title_at_word_boundary() {
        let title = truncate_title(
            "hello world this is a very long title that exceeds sixty characters in total",
        );
        assert!(title.ends_with('…'));
        assert!(title.len() <= 62);
    }
}
