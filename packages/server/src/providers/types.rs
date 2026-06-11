#![allow(missing_docs)]

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
    /// Set when the provider could not assemble usable arguments for this
    /// call — most commonly because the response was cut off by the output
    /// token limit partway through the argument JSON. Invalid calls must not
    /// be executed; the chat loop feeds the reason back to the model as a
    /// failed tool result so it can adjust and re-issue the call, instead of
    /// the whole turn failing (and any retry failing identically).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalid: Option<String>,
}

impl ToolCall {
    /// Create a valid tool call.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
            invalid: None,
        }
    }

    /// Create a tool call that must not be executed, carrying the reason the
    /// provider rejected it. The reason is written for the model: it should
    /// say what went wrong and how to recover.
    pub fn invalid(id: impl Into<String>, name: impl Into<String>, reason: String) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments: serde_json::json!({}),
            invalid: Some(reason),
        }
    }
}

/// Builds the model-facing reason for a tool call whose argument JSON did not
/// parse. When the provider reported that generation stopped at the output
/// token limit, says so explicitly — the model's recovery is different
/// (split the work) than for plain malformed JSON (fix the syntax).
pub(crate) fn invalid_arguments_reason(
    parse_error: &serde_json::Error,
    output_limit_hit: bool,
) -> String {
    if output_limit_hit {
        format!(
            "The arguments for this tool call were cut off because the response hit \
             the output token limit ({parse_error}). Re-issue the call with smaller \
             arguments: split large content across multiple calls and build up the \
             result in steps."
        )
    } else {
        format!(
            "The arguments for this tool call were not valid JSON ({parse_error}). \
             Re-issue the call with corrected arguments."
        )
    }
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

/// Token usage reported by a provider for a single generation turn.
#[derive(Debug, Clone, Copy)]
pub struct Usage {
    /// Input (prompt) tokens consumed.
    pub input_tokens: u32,
    /// Output (completion) tokens generated.
    pub output_tokens: u32,
    /// Input tokens read from the provider's cache (Anthropic prompt caching, etc.).
    pub cache_read_tokens: Option<u32>,
    /// Input tokens written to the provider's cache.
    pub cache_write_tokens: Option<u32>,
}

/// A token delta from a streaming provider response.
///
/// Carries text content, optional tool calls, a final flag that
/// signals the end of the stream, and optional token usage data.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// Text content of this chunk.
    pub delta: String,
    /// Tool calls requested by the model in this chunk.
    pub tool_calls: Vec<ToolCall>,
    /// True on the final (empty) chunk that signals end-of-stream.
    pub is_final: bool,
    /// Token usage reported by the provider for this turn, when available.
    pub usage: Option<Usage>,
}

impl StreamChunk {
    /// Create a text delta chunk.
    pub fn delta(text: impl Into<String>) -> Self {
        Self {
            delta: text.into(),
            tool_calls: Vec::new(),
            is_final: false,
            usage: None,
        }
    }
    /// Create a chunk carrying tool calls from the model.
    pub fn tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            delta: String::new(),
            tool_calls,
            is_final: false,
            usage: None,
        }
    }
    /// Create the terminal chunk that signals the end of the stream.
    pub fn done() -> Self {
        Self {
            delta: String::new(),
            tool_calls: Vec::new(),
            is_final: true,
            usage: None,
        }
    }
    /// Create the terminal chunk with token usage data.
    pub fn done_with_usage(usage: Usage) -> Self {
        Self {
            delta: String::new(),
            tool_calls: Vec::new(),
            is_final: true,
            usage: Some(usage),
        }
    }
}
