//! Context compaction — summarises the oldest portion of a conversation so it
//! fits in the model's context window.
//!
//! Design: `docs/conversation.md` §Context overflow handling.

use std::sync::Arc;

use anyhow::Context;
use futures_util::StreamExt;

use crate::{
    conversation::history,
    db::DbPool,
    providers::{traits::ChatProvider, types::ChatMessage},
};

/// Prompt used when asking the model to summarise a segment.
const COMPACT_INSTRUCTIONS: &str = include_str!("../../../../prompts/compact.md");

/// Approximate token count for an assembled message list.
/// Uses ~4 chars/token + 4 tokens of role-framing overhead per message,
/// matching the design doc's estimation heuristic.
pub fn estimate_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(|m| m.content.len() / 4 + 4).sum()
}

/// Returns true when the assembled context exceeds 90 % of the provider's limit
/// and compaction should be triggered before calling the model.
pub fn needs_compaction(messages: &[ChatMessage], context_limit: u32) -> bool {
    estimate_tokens(messages) > (context_limit as usize) * 9 / 10
}

/// Result returned by a compaction run.
pub struct CompactResult {
    /// Number of messages that were replaced by the summary.
    pub messages_compacted: usize,
    /// Character length of the generated summary.
    pub summary_length: usize,
}

/// Compact a conversation by summarising its oldest third.
///
/// Algorithm:
/// 1. Load all non-compacted, non-summary messages ordered by sequence.
/// 2. Skip the first message (the conversation anchor — never compacted).
/// 3. Take the oldest ⌊N/3⌋ of the remaining messages as the "segment".
/// 4. Ask the provider to summarise the segment (non-streaming, collects all chunks).
/// 5. Insert a `summary` role message at the sequence of the first compacted message.
/// 6. Mark the segment as compacted.
///
/// If the conversation is too short to compact (< 6 compactable messages, i.e.
/// fewer than 3 non-anchor messages) this is a no-op.
pub async fn compact_conversation(
    db: &DbPool,
    conversation_id: &str,
    provider: &Arc<dyn ChatProvider>,
) -> anyhow::Result<CompactResult> {
    let conv_id = conversation_id.to_string();

    // 1. Load compactable messages.
    let messages = db
        .call(move |conn| history::load_compactable_messages(conn, &conv_id))
        .await?;

    // Need at least anchor + 3 more to produce a meaningful compaction.
    if messages.len() < 4 {
        return Ok(CompactResult {
            messages_compacted: 0,
            summary_length: 0,
        });
    }

    // 2. Skip the anchor (first message). The rest are candidates.
    let candidates = &messages[1..];

    // 3. Take the oldest third (rounded down).
    let segment_len = (candidates.len() / 3).max(1);

    // 3a. Ensure the cut point does not split a tool round — if the segment
    //     ends with an assistant that has tool_calls, extend the segment to
    //     include all of its tool results. This prevents orphaned tool messages
    //     that would cause provider errors.
    let segment_len = {
        let mut ids_in_segment = std::collections::HashSet::new();
        for msg in &candidates[..segment_len] {
            if msg.role == "assistant" {
                if let Some(ref tc_json) = msg.tool_calls {
                    if let Ok(calls) = serde_json::from_str::<serde_json::Value>(tc_json) {
                        if let Some(arr) = calls.as_array() {
                            for call in arr {
                                if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
                                    ids_in_segment.insert(id.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut len = segment_len;
        for msg in &candidates[segment_len..] {
            if msg.role == "tool" {
                let belongs = msg
                    .tool_call_id
                    .as_ref()
                    .is_some_and(|id| ids_in_segment.contains(id));
                if belongs {
                    len += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        len
    };

    let segment = &candidates[..segment_len];

    // 4. Format the segment for the summarisation call.
    let formatted = segment
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    let summarise_messages = vec![
        ChatMessage::system(COMPACT_INSTRUCTIONS),
        ChatMessage::user(&formatted),
    ];

    let mut stream = provider
        .complete(&summarise_messages, &[], None)
        .await
        .context("provider failed during compaction")?;

    let mut summary = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("stream error during compaction")?;
        if !chunk.is_final {
            summary.push_str(&chunk.delta);
        }
    }

    if summary.trim().is_empty() {
        anyhow::bail!("provider returned an empty summary — compaction aborted");
    }

    // 5 & 6. Write to DB atomically.
    let first_seq = segment[0].sequence;
    let ids: Vec<String> = segment.iter().map(|m| m.id.clone()).collect();
    let summary_clone = summary.clone();
    let conv_id = conversation_id.to_string();

    db.call(move |conn| {
        history::insert_summary_message(conn, &conv_id, &summary_clone, first_seq)?;
        history::mark_compacted(conn, &ids)?;
        Ok(())
    })
    .await?;

    tracing::info!(
        conversation_id,
        messages_compacted = segment_len,
        summary_length = summary.len(),
        "conversation compacted"
    );

    Ok(CompactResult {
        messages_compacted: segment_len,
        summary_length: summary.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{error::ProviderError, types::StreamChunk};
    use async_trait::async_trait;
    use futures_util::stream;

    struct SummaryProvider;

    #[async_trait]
    impl ChatProvider for SummaryProvider {
        fn id(&self) -> &str {
            "summary"
        }
        fn name(&self) -> &str {
            "Summary"
        }
        fn default_model(&self) -> &str {
            "test"
        }
        fn context_limit(&self) -> u32 {
            4096
        }
        async fn complete(
            &self,
            _: &[ChatMessage],
            _: &[crate::providers::types::ToolDefinition],
            _: Option<&str>,
        ) -> Result<crate::providers::traits::ProviderStream, ProviderError> {
            Ok(Box::pin(stream::iter(vec![
                Ok(StreamChunk::delta(
                    "This was a discussion about Rust async patterns.",
                )),
                Ok(StreamChunk::done()),
            ])))
        }
    }

    #[test]
    fn estimate_tokens_scales_with_content() {
        let msgs = vec![
            ChatMessage::system("You are helpful."), // 17 chars → ~8 tokens
            ChatMessage::user("Hello world"),        // 11 chars → ~7 tokens
        ];
        let est = estimate_tokens(&msgs);
        assert!(est > 10 && est < 50);
    }

    #[test]
    fn needs_compaction_threshold() {
        // 1000 messages × 400 chars each = 400 000 chars → ~100 000 tokens
        let msgs: Vec<ChatMessage> = (0..1000)
            .map(|_| ChatMessage::user("x".repeat(400)))
            .collect();
        assert!(needs_compaction(&msgs, 4096));

        // Small context — no compaction needed
        let small = vec![ChatMessage::user("hi")];
        assert!(!needs_compaction(&small, 4096));
    }

    #[tokio::test]
    async fn compact_replaces_oldest_third() {
        use crate::conversation::history::*;
        use crate::db::open_in_memory;
        use chrono::Utc;
        use uuid::Uuid;

        let pool = open_in_memory();
        let uid = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        pool.call_sync(|conn| {
            conn.execute(
                "INSERT INTO users (id, email, password_hash, role, created_at, updated_at)
                 VALUES (?1, 'c@t.com', 'h', 'admin', ?2, ?2)",
                rusqlite::params![uid, now],
            )?;
            Ok(())
        })
        .unwrap();

        let provider: Arc<dyn ChatProvider> = Arc::new(SummaryProvider);

        // Create a conversation with 9 messages (1 anchor + 8 regular).
        let conv_id = pool
            .call_sync(|conn| {
                let conv = create_conversation(conn, &uid)?;
                for i in 0..9 {
                    if i % 2 == 0 {
                        insert_user_message(conn, &conv.id, &format!("user message {i}"))?;
                    } else {
                        insert_assistant_message(conn, &conv.id, &format!("reply {i}"), "p", "m")?;
                    }
                }
                Ok(conv.id)
            })
            .unwrap();

        let result = compact_conversation(&pool, &conv_id, &provider)
            .await
            .unwrap();

        assert!(
            result.messages_compacted > 0,
            "should have compacted some messages"
        );

        // After compaction, load_messages should include the summary.
        let msgs = pool
            .call_sync(|conn| load_messages(conn, &conv_id))
            .unwrap();

        let has_summary = msgs.iter().any(|m| m.role == "summary");
        assert!(has_summary, "summary message should appear in history");
    }
}
