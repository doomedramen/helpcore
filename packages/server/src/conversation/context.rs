use helpcore_api::MessageSummary;
use std::collections::HashMap;

use crate::{
    conversation::memory::MemoryResult,
    providers::types::{ChatMessage, ToolCall},
};

/// Baseline system prompt — edit `prompts/core.md` to change behaviour.
const CORE_INSTRUCTIONS: &str = include_str!("../../../../prompts/core.md");

/// Rules injected when the user has memory files — edit `prompts/memory_rules.md`.
const MEMORY_RULES: &str = include_str!("../../../../prompts/memory_rules.md");

/// Optional context layers injected between the core instructions and history.
///
/// All fields default to empty / None so callers that don't have memory yet
/// can use `ContextOptions::default()` without change.
#[derive(Debug, Default)]
pub struct ContextOptions<'a> {
    /// Contents of the user's SOUL.md (assistant personality / tone).
    pub soul: Option<&'a str>,
    /// Contents of the user's IDENTITY.md (assistant identity / background).
    pub identity: Option<&'a str>,
    /// Contents of the user's USER.md (facts about the human).
    pub user_profile: Option<&'a str>,
    /// Memory search results to inject under a `## Memories` section.
    pub memories: &'a [MemoryResult],
    /// Skill fragments from enabled plugins (each plugin's skill.md content).
    pub plugin_skills: &'a [String],
}

/// Assembles the full message list sent to the provider.
///
/// Assembly order (per `docs/conversation.md` §Context assembly):
/// ```text
/// [system]  CORE_INSTRUCTIONS
///           ## Soul          (if opts.soul is set)
///           ## Identity      (if opts.identity is set)
///           ## User          (if opts.user_profile is set)
///           ## Memories      (if opts.memories is non-empty)
///           ## Current date
/// [user/assistant…]  conversation history
/// [user]    new user message
/// ```
pub fn assemble(
    history: &[MessageSummary],
    user_message: &str,
    opts: ContextOptions<'_>,
) -> Vec<ChatMessage> {
    let system_prompt = build_system_prompt(&opts);

    let mut messages = Vec::with_capacity(history.len() + 2);
    messages.push(ChatMessage::system(&system_prompt));

    let mut tool_names = HashMap::new();
    for msg in history {
        let calls = msg
            .tool_calls
            .as_ref()
            .and_then(|value| serde_json::from_value::<Vec<ToolCall>>(value.clone()).ok());
        if let Some(calls) = calls.as_ref() {
            for call in calls {
                tool_names.insert(call.id.clone(), call.name.clone());
            }
        }
        let tool_name = msg
            .tool_call_id
            .as_ref()
            .and_then(|id| tool_names.get(id))
            .cloned();
        messages.push(ChatMessage {
            role: msg.role.clone(),
            content: msg.content.clone(),
            tool_calls: calls,
            tool_name,
        });
    }

    messages.push(ChatMessage::user(user_message));
    messages
}

fn build_system_prompt(opts: &ContextOptions<'_>) -> String {
    let mut out = CORE_INSTRUCTIONS.to_string();

    if let Some(soul) = opts.soul {
        out.push_str("\n\n## Soul\n");
        out.push_str(soul);
    }
    if let Some(identity) = opts.identity {
        out.push_str("\n\n## Identity\n");
        out.push_str(identity);
    }
    if let Some(user_profile) = opts.user_profile {
        out.push_str("\n\n## User\n");
        out.push_str(user_profile);
    }
    if !opts.memories.is_empty() {
        // Inject the memory rules so the AI knows how to manage memory files.
        out.push_str("\n\n## Memory system\n");
        out.push_str(MEMORY_RULES);
        out.push_str("\n\n## Current memories\n");
        for mem in opts.memories {
            out.push_str(&format!("### {}\n{}\n", mem.path, mem.content));
        }
    }

    if !opts.plugin_skills.is_empty() {
        out.push_str("\n\n## Skills\n");
        for skill in opts.plugin_skills {
            out.push_str(skill.trim_end());
            out.push('\n');
        }
    }

    // Always include the current date so the assistant can reason about time.
    let today = chrono::Local::now().format("%A, %B %-d, %Y");
    out.push_str(&format!("\n\n## Current date\n{today}"));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use helpcore_api::MessageSummary;

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
    fn empty_history_has_system_plus_user() {
        let messages = assemble(&[], "Hello", ContextOptions::default());
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, "Hello");
    }

    #[test]
    fn history_is_preserved_in_order() {
        let history = vec![
            msg("user", "first"),
            msg("assistant", "reply"),
        ];
        let messages = assemble(&history, "follow up", ContextOptions::default());
        assert_eq!(messages.len(), 4); // system + 2 history + user
        assert_eq!(messages[1].content, "first");
        assert_eq!(messages[2].content, "reply");
        assert_eq!(messages[3].content, "follow up");
    }

    #[test]
    fn soul_and_user_profile_appear_in_system_message() {
        let messages = assemble(
            &[],
            "hi",
            ContextOptions {
                soul:         Some("Be direct."),
                user_profile: Some("I am Martin."),
                ..Default::default()
            },
        );
        let system = &messages[0].content;
        assert!(system.contains("## Soul\nBe direct."), "missing soul section");
        assert!(system.contains("## User\nI am Martin."), "missing user section");
    }

    #[test]
    fn memories_are_injected_with_headers() {
        let mems = vec![
            MemoryResult { path: "rust.md".into(), content: "Rust is great.".into() },
        ];
        let messages = assemble(
            &[],
            "hi",
            ContextOptions { memories: &mems, ..Default::default() },
        );
        let system = &messages[0].content;
        assert!(system.contains("## Current memories"), "missing memories section");
        assert!(system.contains("### rust.md"), "missing memory file header");
        assert!(system.contains("Rust is great."), "missing memory content");
    }

    #[test]
    fn current_date_is_always_present() {
        let messages = assemble(&[], "hi", ContextOptions::default());
        assert!(messages[0].content.contains("## Current date"), "missing current date");
    }
}
