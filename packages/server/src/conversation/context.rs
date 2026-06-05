use helpcore_api::MessageSummary;

use crate::providers::types::ChatMessage;

/// Minimal system prompt injected into every request.
/// Expanded in later milestones with soul, identity, memories, and plugin skills.
const CORE_INSTRUCTIONS: &str = "\
You are a helpful, knowledgeable personal AI assistant. \
Be concise and accurate. Think step by step when answering complex questions. \
If you are unsure about something, say so rather than guessing.";

/// Assembles the full message list sent to the provider:
/// core instructions → conversation history → new user message.
pub fn assemble(history: &[MessageSummary], user_message: &str) -> Vec<ChatMessage> {
    let mut messages = Vec::with_capacity(history.len() + 2);

    messages.push(ChatMessage::system(CORE_INSTRUCTIONS));

    for msg in history {
        messages.push(ChatMessage { role: msg.role.clone(), content: msg.content.clone() });
    }

    messages.push(ChatMessage::user(user_message));

    messages
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
            sequence: 1,
            created_at: "now".to_string(),
        }
    }

    #[test]
    fn empty_history_has_system_plus_user() {
        let messages = assemble(&[], "Hello");
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
        let messages = assemble(&history, "follow up");
        assert_eq!(messages.len(), 4); // system + 2 history + user
        assert_eq!(messages[1].content, "first");
        assert_eq!(messages[2].content, "reply");
        assert_eq!(messages[3].content, "follow up");
    }
}
