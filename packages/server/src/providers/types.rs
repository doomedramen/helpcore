use serde::{Deserialize, Serialize};

/// A tool (function) definition that can be sent to a model to enable tool use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A tool call request emitted by a model during a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// A single message in a conversation, using role strings to match provider
/// wire formats directly (no conversion needed at the API boundary).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl ChatMessage {
    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::plain("system", content)
    }
    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::plain("user", content)
    }
    /// Create a plain assistant message without tool calls.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::plain("assistant", content)
    }
    /// Create an assistant message that requests tool calls.
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
            tool_name: None,
        }
    }
    /// Create a tool result message with a call ID and tool name.
    pub fn tool(
        id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(id.into()),
            tool_name: Some(name.into()),
        }
    }
    fn plain(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
        }
    }
}

/// A token delta from a streaming provider response.
///
/// Carries text content, optional tool calls, and a final flag that
/// signals the end of the stream.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// Text content of this chunk.
    pub delta: String,
    /// Tool calls requested by the model in this chunk.
    pub tool_calls: Vec<ToolCall>,
    /// True on the final (empty) chunk that signals end-of-stream.
    pub is_final: bool,
}

impl StreamChunk {
    /// Create a text delta chunk.
    pub fn delta(text: impl Into<String>) -> Self {
        Self {
            delta: text.into(),
            tool_calls: Vec::new(),
            is_final: false,
        }
    }
    /// Create a chunk carrying tool calls from the model.
    pub fn tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            delta: String::new(),
            tool_calls,
            is_final: false,
        }
    }
    /// Create the terminal chunk that signals the end of the stream.
    pub fn done() -> Self {
        Self {
            delta: String::new(),
            tool_calls: Vec::new(),
            is_final: true,
        }
    }
}
