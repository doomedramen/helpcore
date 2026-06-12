//! Context compaction — summarises the oldest portion of a conversation so it
//! fits in the model's context window.
//!
//! Design: `docs/conversation.md` §Context overflow handling.

use std::sync::Arc;

use anyhow::Context;
use futures_util::StreamExt;

use crate::{
    conversation::{context, history},
    db::DbPool,
    providers::{traits::ChatProvider, types::ChatMessage},
};

/// Prompt used when asking the model to summarise a segment.
const COMPACT_INSTRUCTIONS: &str = include_str!("../../../../prompts/compact.md");

/// Target token count to preserve at the end of the conversation.
const PRESERVE_RECENT_TOKENS: usize = 8000;
/// Minimum number of messages to preserve at the end (to maintain flow).
const MIN_PRESERVE_MESSAGES: usize = 4;

/// Extracts summary content from the model's response, handling `<summary>` wrapper
/// tags robustly. Falls back to raw text when tags are absent or malformed.
fn extract_summary(raw: &str) -> String {
    let raw = raw.trim();

    // Full tags present — extract content between them, even with
    // preamble or postamble.
    if let (Some(start), Some(end)) = (raw.find("<summary>"), raw.rfind("</summary>")) {
        let content = &raw[start + "<summary>".len()..end];
        return content.trim().to_string();
    }

    // Only opening tag — strip it, take everything after.
    if let Some(rest) = raw.strip_prefix("<summary>") {
        return rest.trim().to_string();
    }

    // Only closing tag — strip it, take everything before.
    if let Some(content) = raw.strip_suffix("</summary>") {
        return content.trim().to_string();
    }

    // No tags at all — return raw text as-is.
    raw.to_string()
}

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

/// Compact a conversation by summarising its oldest portion while preserving recent context.
///
/// Algorithm:
/// 1. Load all non-compacted messages and the latest summary.
/// 2. Preserve a "tail" of recent messages (at least 4 messages or ~8000 tokens).
/// 3. Take the remaining messages (excluding anchor) as the "segment" to be compacted.
/// 4. If a previous summary exists, provide it to the LLM for an incremental update.
/// 5. Ask the provider to summarise the segment.
/// 6. Insert a `summary` role message and mark the segment as compacted.
pub async fn compact_conversation(
    db: &DbPool,
    conversation_id: &str,
    provider: &Arc<dyn ChatProvider>,
) -> anyhow::Result<CompactResult> {
    let conv_id = conversation_id.to_string();

    // 1. Load compactable messages and latest summary.
    let (messages, previous_summary) = db
        .call(move |conn| {
            let msgs = history::load_compactable_messages(conn, &conv_id)?;
            let summary = history::load_latest_summary(conn, &conv_id)?;
            Ok((msgs, summary))
        })
        .await?;

    // Need at least anchor + 3 more to produce a meaningful compaction.
    if messages.len() < 4 {
        return Ok(CompactResult {
            messages_compacted: 0,
            summary_length: 0,
        });
    }

    // 2. Identify the segment to compact by calculating the tail to preserve.
    let mut tail_tokens = 0;
    let mut tail_len = 0;
    for msg in messages.iter().rev() {
        tail_tokens += context::model_visible_content(&msg.role, &msg.content).len() / 4 + 4;
        tail_len += 1;
        // Keep at least MIN_PRESERVE_MESSAGES and at least PRESERVE_RECENT_TOKENS.
        if tail_tokens >= PRESERVE_RECENT_TOKENS && tail_len >= MIN_PRESERVE_MESSAGES {
            break;
        }
    }

    // We preserve the anchor (messages[0]) and the tail.
    let segment_end = messages.len().saturating_sub(tail_len).max(1);

    // Ensure we don't compact the anchor.
    let segment = &messages[1..segment_end];
    let segment_len = segment.len();

    if segment_len == 0 {
        return Ok(CompactResult {
            messages_compacted: 0,
            summary_length: 0,
        });
    }

    // 3. Format the segment and summary for the call.
    let formatted_history = segment
        .iter()
        .map(|m| {
            format!(
                "{}: {}",
                m.role,
                context::model_visible_content(&m.role, &m.content)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut user_prompt = String::new();
    if let Some(prev) = previous_summary {
        user_prompt.push_str("<previous-summary>\n");
        user_prompt.push_str(&prev);
        user_prompt.push_str("\n</previous-summary>\n\n");
        user_prompt
            .push_str("Update the summary above with the following new conversation history:\n\n");
    } else {
        user_prompt.push_str("Create a new summary from the following conversation history:\n\n");
    }
    user_prompt.push_str(&formatted_history);

    let summarise_messages = vec![
        ChatMessage::system(COMPACT_INSTRUCTIONS),
        ChatMessage::user(&user_prompt),
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

    let summary = extract_summary(&summary);

    if summary.is_empty() {
        anyhow::bail!("provider returned an empty summary — compaction aborted");
    }

    // 4. Write to DB atomically.
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
            messages: &[ChatMessage],
            _: &[crate::providers::types::ToolDefinition],
            _: Option<&str>,
        ) -> Result<crate::providers::traits::ProviderStream, ProviderError> {
            let user_msg = messages
                .iter()
                .find(|m| m.role == "user")
                .map(|m| &m.content)
                .cloned()
                .unwrap_or_default();

            let response = if user_msg.contains("<previous-summary>") {
                "<summary>## Goal\n- Updated summary</summary>"
            } else {
                "<summary>## Goal\n- New summary</summary>"
            };

            Ok(Box::pin(stream::iter(vec![
                Ok(StreamChunk::delta(response)),
                Ok(StreamChunk::done()),
            ])))
        }
    }

    #[test]
    fn estimate_tokens_scales_with_content() {
        let msgs = vec![
            ChatMessage::system("You are helpful."),
            ChatMessage::user("Hello world"),
        ];
        let est = estimate_tokens(&msgs);
        assert!(est > 10 && est < 50);
    }

    #[test]
    fn extract_summary_with_full_tags() {
        let raw = "<summary>The user asked about Rust async patterns.</summary>";
        assert_eq!(
            extract_summary(raw),
            "The user asked about Rust async patterns."
        );
    }

    #[tokio::test]
    async fn compact_incremental_update() {
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

        let conv_id = pool
            .call_sync(|conn| {
                let conv = create_conversation(conn, &uid)?;
                // Create many messages. 100 messages * 500 chars = 50000 chars (~12500 tokens).
                // Tail preservation is 8000 tokens.
                // So ~4500 tokens should be compacted.
                for i in 0..100 {
                    insert_user_message(conn, &conv.id, &format!("msg {i}: {}", "x".repeat(500)))?;
                }
                Ok(conv.id)
            })
            .unwrap();

        // First compaction
        let result = compact_conversation(&pool, &conv_id, &provider)
            .await
            .unwrap();
        assert!(
            result.messages_compacted > 0,
            "first compaction should have happened"
        );

        let msgs = pool
            .call_sync(|conn| load_messages(conn, &conv_id))
            .unwrap();
        let summary = msgs.iter().find(|m| m.role == "summary").unwrap();
        assert_eq!(summary.content, "## Goal\n- New summary");

        // Add more messages
        pool.call_sync(|conn| {
            for i in 100..200 {
                insert_user_message(conn, &conv_id, &format!("msg {i}: {}", "x".repeat(500)))?;
            }
            Ok(())
        })
        .unwrap();

        // Second compaction (incremental)
        let result = compact_conversation(&pool, &conv_id, &provider)
            .await
            .unwrap();
        assert!(
            result.messages_compacted > 0,
            "second compaction should have happened"
        );

        let msgs = pool
            .call_sync(|conn| load_messages(conn, &conv_id))
            .unwrap();
        // Should find the LATEST summary
        let latest_summary = msgs.iter().rfind(|m| m.role == "summary").unwrap();
        assert_eq!(latest_summary.content, "## Goal\n- Updated summary");
    }
}
