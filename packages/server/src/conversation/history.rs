use anyhow::Context;
use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use helpcore_api::{ConversationSummary, MessageSummary};

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
pub fn create_conversation(conn: &Connection, user_id: &str) -> anyhow::Result<ConversationSummary> {
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
pub fn load_messages(conn: &Connection, conversation_id: &str) -> anyhow::Result<Vec<MessageSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, role, content, sequence, created_at
         FROM messages
         WHERE conversation_id = ?1 AND compacted = 0
         ORDER BY sequence ASC",
    )?;
    let rows = stmt
        .query_map([conversation_id], row_to_message)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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
        sequence,
        created_at: now,
    })
}

/// Inserts the assistant response after streaming completes.
/// Returns the message id.
pub fn insert_assistant_message(
    conn: &Connection,
    conversation_id: &str,
    content: &str,
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
             (id, conversation_id, role, content, provider_id, model, sequence, created_at)
         VALUES (?1, ?2, 'assistant', ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, conversation_id, content, provider_id, model, sequence, now],
    )
    .context("failed to insert assistant message")?;

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
          ORDER BY sequence ASC",
    )?;
    let rows = stmt
        .query_map([conversation_id], |row| {
            Ok(CompactableMessage {
                id:       row.get(0)?,
                role:     row.get(1)?,
                content:  row.get(2)?,
                sequence: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Marks a set of messages as compacted (soft-delete — rows are retained for export).
pub fn mark_compacted(conn: &Connection, ids: &[String]) -> anyhow::Result<()> {
    for id in ids {
        conn.execute(
            "UPDATE messages SET compacted = 1 WHERE id = ?1",
            [id],
        )?;
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
    Ok(MessageSummary {
        id: row.get(0)?,
        role: row.get(1)?,
        content: row.get(2)?,
        sequence: row.get(3)?,
        created_at: row.get(4)?,
    })
}

/// Truncates text to ≤60 chars at a word boundary, appending "…" if cut.
fn truncate_title(text: &str) -> String {
    let text = text.lines().next().unwrap_or("").trim();
    if text.chars().count() <= 60 {
        return text.to_string();
    }
    // Find byte offset of the 60th character boundary.
    let cut = text.char_indices().nth(60).map(|(i, _)| i).unwrap_or(text.len());
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
            insert_assistant_message(conn, &conv.id, "Hi there!", "p", "m")?;
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
            insert_assistant_message(conn, &conv.id, "reply", "p", "m")?;
            let convs = list_conversations(conn, &uid)?;
            assert_eq!(convs[0].message_count, 2);
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
        let title = truncate_title("hello world this is a very long title that exceeds sixty characters in total");
        assert!(title.ends_with('…'));
        assert!(title.len() <= 62);
    }
}
