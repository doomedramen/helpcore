//! Context assembly: building the system prompt and message list for the provider.

use chrono_tz::Tz;
use helpcore_api::MessageSummary;
use std::collections::{BTreeMap, HashMap};

use crate::{
    conversation::memory::MemoryResult,
    plugins::registry::PluginSkill,
    providers::types::{ChatMessage, ToolCall},
};

/// Baseline system prompt — edit `prompts/core.md` to change behaviour.
const CORE_INSTRUCTIONS: &str = include_str!("../../../../prompts/core.md");

/// Chart and diagram capabilities — injected as a non-negotiable capability section,
/// not personality. Edit `prompts/charts.md` to add or change rendering formats.
const CHARTS_CAPABILITIES: &str = include_str!("../../../../prompts/charts.md");

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
    /// Plugin skill metadata — brief index injected into prompt, full skill.md
    /// loaded on demand via the `skill_read` tool.
    pub plugin_skills: &'a [PluginSkill],
    /// IANA timezone for the current user (e.g. "America/New_York").
    /// Defaults to UTC when absent or unrecognised.
    pub timezone: Option<&'a str>,
    /// How heavily the assistant should lean on memory. One of: "off", "light", "moderate", "heavy".
    /// Defaults to "moderate" when absent.
    pub memory_leaning: Option<&'a str>,
    /// The provider's context window limit in tokens.
    pub context_limit: Option<u32>,
    /// Estimated token count of the assembled message list.
    pub estimated_tokens: Option<usize>,
}

/// Assembles the full message list sent to the provider.
///
/// Assembly order (per `docs/conversation.md` §Context assembly):
/// ```text
/// [system]  CORE_INSTRUCTIONS
///           ## Soul             (if opts.soul is set)
///           ## Identity         (if opts.identity is set)
///           ## User             (if opts.user_profile is set)
///           ## Memory system    (always — rules + memory_*/personality_write tools)
///           ## Current memories (if opts.memories is non-empty)
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
        let (role, content) = if msg.role == "summary" {
            (
                "user".to_string(),
                format!("[Earlier conversation summary]\n{}", msg.content),
            )
        } else {
            (msg.role.clone(), msg.content.clone())
        };
        messages.push(ChatMessage {
            role,
            content,
            tool_calls: calls,
            tool_call_id: msg.tool_call_id.clone(),
            tool_name,
        });
    }

    messages.push(ChatMessage::user(user_message));
    messages
}

fn build_system_prompt(opts: &ContextOptions<'_>) -> String {
    let mut out = CORE_INSTRUCTIONS.to_string();

    // Capabilities — non-negotiable, always present regardless of personality
    out.push_str("\n\n");
    out.push_str(CHARTS_CAPABILITIES);

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

    // Memory leaning — tells the AI how aggressively to use memories.
    let leaning = opts.memory_leaning.unwrap_or("moderate");
    out.push_str("\n\n## Memory leaning\n");
    match leaning {
        "off" => {
            out.push_str(
                "Memory is DISABLED. You MUST NOT call any memory_* or personality_write tools \
                 under any circumstances — not even if the user asks you to remember something. \
                 If the user says \"remember this\", acknowledge the request but explain that \
                 memory is turned off. If any memory results appear in the context below, \
                 ignore them entirely — they are stale and should not influence your responses. \
                 Treat every conversation as a blank slate with no knowledge of the user.\n",
            );
        }
        "light" => {
            out.push_str(
                "Use memory only when the user explicitly asks you to remember or recall \
                 something (e.g. \"remember that\", \"what do you know about me\", \
                 \"save this for later\"). Do NOT proactively write memory files — even if \
                 you learn something interesting about the user, do not save it unless they \
                 explicitly tell you to. You may search memory and use existing memories to \
                 answer questions, but never create or update memory files on your own \
                 initiative.\n",
            );
        }
        "heavy" => {
            out.push_str(
                "Memory is extremely important. Actively look for useful facts, preferences, \
                 and context worth remembering. Proactively write memory files whenever you \
                 learn something durable about the user, their projects, or their preferences. \
                 Save preferences and meaningful facts, but skip one-off details or passing \
                 comments — the user wants rich context, not noise. Search memory aggressively \
                 for relevant context before responding.\n",
            );
        }
        _ => {
            out.push_str(
                "Use memory thoughtfully. Write down useful facts, preferences, and project \
                 context when they seem durable and worth keeping. Recall relevant memories \
                 before responding. Strike a balance — don't save trivia, but don't let \
                 useful context slip.\n",
            );
        }
    }

    // Always inject the memory rules — the memory_* and personality_write
    // tools are available to every user regardless of whether they have any
    // memory files yet (e.g. the very first thing they say may be worth saving).
    out.push_str("\n\n## Memory system\n");
    out.push_str(MEMORY_RULES);
    if !opts.memories.is_empty() {
        out.push_str("\n\n## Current memories\n");
        for mem in opts.memories {
            out.push_str(&format!("### {}\n{}\n", mem.path, mem.content));
        }
    }

    if !opts.plugin_skills.is_empty() {
        out.push_str("\n\n## Skills\n\n");
        out.push_str(
            "The following plugins are installed and enabled. Each has a one-line summary \
             of when to use it. To get full instructions for a plugin (required before \
             calling its tools for the first time), use the `skill_read` tool with the \
             plugin name exactly as shown.\n\n",
        );

        // Group skills by prefix (category). Multi-tool plugins share a prefix
        // separated by _ or -; single-tool plugins go under "Other".
        let mut groups: BTreeMap<String, Vec<&PluginSkill>> = BTreeMap::new();
        for skill in opts.plugin_skills {
            let category = skill
                .name
                .split_once('_')
                .or_else(|| skill.name.split_once('-'))
                .map(|(prefix, _)| capitalize(prefix))
                .unwrap_or_else(|| "Other".to_string());
            groups.entry(category).or_default().push(skill);
        }

        for (category, skills) in &groups {
            out.push_str(&format!("**{}**\n", category));
            for skill in skills {
                out.push_str(&format!("- `{}`: {}\n", skill.name, skill.brief));
            }
            out.push('\n');
        }
    }

    // Always include the current date so the assistant can reason about time.
    let tz: Tz = opts
        .timezone
        .and_then(|s| s.parse().ok())
        .unwrap_or(Tz::UTC);
    let today = chrono::Utc::now()
        .with_timezone(&tz)
        .format("%A, %B %-d, %Y");
    out.push_str(&format!("\n\n## Current date\n{today}"));

    if let (Some(limit), Some(tokens)) = (opts.context_limit, opts.estimated_tokens) {
        let pct = (tokens as f64 / limit as f64 * 100.0) as u32;
        if pct > 60 {
            out.push_str(&format!(
                "\n\n## Context budget\n~{pct}% used (limit {limit} tokens)."
            ));
        }
    }

    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
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
        let history = vec![msg("user", "first"), msg("assistant", "reply")];
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
                soul: Some("Be direct."),
                user_profile: Some("I am Martin."),
                ..Default::default()
            },
        );
        let system = &messages[0].content;
        assert!(
            system.contains("## Soul\nBe direct."),
            "missing soul section"
        );
        assert!(
            system.contains("## User\nI am Martin."),
            "missing user section"
        );
    }

    #[test]
    fn memory_system_section_present_even_without_memories() {
        let messages = assemble(&[], "hi", ContextOptions::default());
        let system = &messages[0].content;
        assert!(
            system.contains("## Memory system"),
            "memory rules should always be injected so the AI knows the memory_* \
             and personality_write tools exist, even before any memory files do"
        );
        assert!(
            !system.contains("## Current memories"),
            "current-memories section should be omitted when there are no results"
        );
    }

    #[test]
    fn memories_are_injected_with_headers() {
        let mems = vec![MemoryResult {
            path: "rust.md".into(),
            content: "Rust is great.".into(),
        }];
        let messages = assemble(
            &[],
            "hi",
            ContextOptions {
                memories: &mems,
                ..Default::default()
            },
        );
        let system = &messages[0].content;
        assert!(
            system.contains("## Current memories"),
            "missing memories section"
        );
        assert!(system.contains("### rust.md"), "missing memory file header");
        assert!(system.contains("Rust is great."), "missing memory content");
    }

    #[test]
    fn summary_role_is_converted_to_user() {
        let history = vec![msg("summary", "We discussed Rust async.")];
        let messages = assemble(&history, "continue", ContextOptions::default());
        let summary_msg = messages
            .iter()
            .find(|m| m.content.contains("We discussed Rust async."))
            .unwrap();
        assert_eq!(summary_msg.role, "user");
        assert!(
            summary_msg
                .content
                .starts_with("[Earlier conversation summary]")
        );
    }

    #[test]
    fn current_date_is_always_present() {
        let messages = assemble(&[], "hi", ContextOptions::default());
        assert!(
            messages[0].content.contains("## Current date"),
            "missing current date"
        );
    }
}
