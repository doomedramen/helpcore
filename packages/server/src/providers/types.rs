use serde::{Deserialize, Serialize};

/// A single message in a conversation, using role strings to match provider
/// wire formats directly (no conversion needed at the API boundary).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".to_string(), content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".to_string(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".to_string(), content: content.into() }
    }
}

/// A token delta from a streaming provider response.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// Text content of this chunk.
    pub delta: String,
    /// True on the final (empty) chunk that signals end-of-stream.
    pub is_final: bool,
}

impl StreamChunk {
    pub fn delta(text: impl Into<String>) -> Self {
        Self { delta: text.into(), is_final: false }
    }
    pub fn done() -> Self {
        Self { delta: String::new(), is_final: true }
    }
}
