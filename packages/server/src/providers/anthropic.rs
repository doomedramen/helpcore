#![allow(missing_docs)]

use std::collections::BTreeMap;

use async_trait::async_trait;
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::io::StreamReader;

use super::{
    error::{ProviderError, http_error},
    traits::{ChatProvider, ProviderStream},
    types::{ChatMessage, StreamChunk, ToolCall, ToolDefinition},
};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_CONTEXT_LIMIT: u32 = 200_000;
const DEFAULT_MAX_TOKENS: u32 = 4096;
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    id: String,
    name: String,
    base_url: String,
    api_key: String,
    default_model: String,
    context_limit: u32,
    max_tokens: u32,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(
        id: &str,
        name: &str,
        base_url: Option<&str>,
        api_key: &str,
        default_model: &str,
        context_limit: Option<u32>,
        max_tokens: Option<u32>,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            base_url: base_url
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/')
                .to_string(),
            api_key: api_key.to_string(),
            default_model: default_model.to_string(),
            context_limit: context_limit.unwrap_or(DEFAULT_CONTEXT_LIMIT),
            max_tokens: max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            client: Client::new(),
        }
    }

    fn messages_url(&self) -> String {
        if self.base_url.ends_with("/v1/messages") {
            self.base_url.clone()
        } else if self.base_url.ends_with("/v1") {
            format!("{}/messages", self.base_url)
        } else {
            format!("{}/v1/messages", self.base_url)
        }
    }
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<ContentBlock>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Default)]
struct ToolAccumulator {
    id: String,
    name: String,
    initial_input: Option<serde_json::Value>,
    partial_json: String,
}

#[async_trait]
impl ChatProvider for AnthropicProvider {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn default_model(&self) -> &str {
        &self.default_model
    }
    fn context_limit(&self) -> u32 {
        self.context_limit
    }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        model: Option<&str>,
    ) -> Result<ProviderStream, ProviderError> {
        let (system, messages) = convert_messages(messages)?;
        let body = AnthropicRequest {
            model: model.unwrap_or(&self.default_model).to_string(),
            max_tokens: self.max_tokens,
            system,
            messages,
            tools: tools
                .iter()
                .map(|tool| AnthropicTool {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    input_schema: tool.input_schema.clone(),
                })
                .collect(),
            stream: true,
        };

        let response = self
            .client
            .post(self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|error| ProviderError::Request(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(http_error(status, &body));
        }

        let byte_stream = response.bytes_stream().map_err(std::io::Error::other);
        let reader = StreamReader::new(byte_stream);
        let mut lines = BufReader::new(reader).lines();
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<StreamChunk, ProviderError>>(32);

        tokio::spawn(async move {
            let mut data_lines = Vec::new();
            let mut tool_calls = BTreeMap::<usize, ToolAccumulator>::new();
            let mut terminated = false;

            loop {
                match lines.next_line().await {
                    Ok(Some(line)) if line.is_empty() => {
                        if data_lines.is_empty() {
                            continue;
                        }
                        let data = data_lines.join("\n");
                        data_lines.clear();
                        match handle_event(&data, &mut tool_calls, &tx).await {
                            Ok(done) => {
                                if done {
                                    terminated = true;
                                    break;
                                }
                            }
                            Err(error) => {
                                let _ = tx.send(Err(error)).await;
                                return;
                            }
                        }
                    }
                    Ok(Some(line)) => {
                        if let Some(data) = line.strip_prefix("data:") {
                            data_lines.push(data.trim_start().to_string());
                        }
                    }
                    Ok(None) => {
                        if !data_lines.is_empty() {
                            let data = data_lines.join("\n");
                            match handle_event(&data, &mut tool_calls, &tx).await {
                                Ok(done) => terminated = done,
                                Err(error) => {
                                    let _ = tx.send(Err(error)).await;
                                    return;
                                }
                            }
                        }
                        break;
                    }
                    Err(error) => {
                        let _ = tx
                            .send(Err(ProviderError::Request(format!(
                                "stream read error: {error}"
                            ))))
                            .await;
                        return;
                    }
                }
            }

            if !terminated {
                let _ = tx
                    .send(Err(ProviderError::Request(
                        "provider stream ended before completion".to_string(),
                    )))
                    .await;
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

fn convert_messages(
    messages: &[ChatMessage],
) -> Result<(Option<String>, Vec<AnthropicMessage>), ProviderError> {
    let mut system = Vec::new();
    let mut converted = Vec::new();

    for message in messages {
        match message.role.as_str() {
            "system" => {
                if !message.content.trim().is_empty() {
                    system.push(message.content.clone());
                }
            }
            "assistant" => {
                let mut blocks = Vec::new();
                if !message.content.is_empty() {
                    blocks.push(ContentBlock::Text {
                        text: message.content.clone(),
                    });
                }
                if let Some(calls) = message.tool_calls.as_ref() {
                    blocks.extend(calls.iter().map(|call| ContentBlock::ToolUse {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        input: call.arguments.clone(),
                    }));
                }
                push_message(&mut converted, "assistant", blocks);
            }
            "tool" => {
                let tool_use_id = message.tool_call_id.clone().ok_or_else(|| {
                    ProviderError::InvalidRequest(
                        "Anthropic tool results require a tool call ID".to_string(),
                    )
                })?;
                push_message(
                    &mut converted,
                    "user",
                    vec![ContentBlock::ToolResult {
                        tool_use_id,
                        content: message.content.clone(),
                    }],
                );
            }
            "user" => {
                push_message(
                    &mut converted,
                    "user",
                    vec![ContentBlock::Text {
                        text: message.content.clone(),
                    }],
                );
            }
            role => {
                return Err(ProviderError::InvalidRequest(format!(
                    "unsupported message role for Anthropic: {role}"
                )));
            }
        }
    }

    Ok(((!system.is_empty()).then(|| system.join("\n\n")), converted))
}

fn push_message(messages: &mut Vec<AnthropicMessage>, role: &str, blocks: Vec<ContentBlock>) {
    if blocks.is_empty() {
        return;
    }
    if let Some(last) = messages.last_mut().filter(|last| last.role == role) {
        last.content.extend(blocks);
    } else {
        messages.push(AnthropicMessage {
            role: role.to_string(),
            content: blocks,
        });
    }
}

async fn handle_event(
    data: &str,
    tool_calls: &mut BTreeMap<usize, ToolAccumulator>,
    tx: &tokio::sync::mpsc::Sender<Result<StreamChunk, ProviderError>>,
) -> Result<bool, ProviderError> {
    let event: serde_json::Value = serde_json::from_str(data).map_err(|error| {
        ProviderError::Request(format!(
            "failed to parse Anthropic response: {error} - data: {data}"
        ))
    })?;
    match event.get("type").and_then(serde_json::Value::as_str) {
        Some("content_block_start") => {
            let index = event
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default() as usize;
            let content = event
                .get("content_block")
                .unwrap_or(&serde_json::Value::Null);
            match content.get("type").and_then(serde_json::Value::as_str) {
                Some("text") => {
                    if let Some(text) = content
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .filter(|text| !text.is_empty())
                    {
                        let _ = tx.send(Ok(StreamChunk::delta(text))).await;
                    }
                }
                Some("tool_use") => {
                    tool_calls.insert(
                        index,
                        ToolAccumulator {
                            id: content
                                .get("id")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            name: content
                                .get("name")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            initial_input: content.get("input").cloned(),
                            partial_json: String::new(),
                        },
                    );
                }
                _ => {}
            }
        }
        Some("content_block_delta") => {
            let index = event
                .get("index")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default() as usize;
            let delta = event.get("delta").unwrap_or(&serde_json::Value::Null);
            match delta.get("type").and_then(serde_json::Value::as_str) {
                Some("text_delta") => {
                    if let Some(text) = delta
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .filter(|text| !text.is_empty())
                    {
                        let _ = tx.send(Ok(StreamChunk::delta(text))).await;
                    }
                }
                Some("input_json_delta") => {
                    if let Some(json) = delta
                        .get("partial_json")
                        .and_then(serde_json::Value::as_str)
                    {
                        tool_calls
                            .entry(index)
                            .or_default()
                            .partial_json
                            .push_str(json);
                    }
                }
                _ => {}
            }
        }
        Some("message_stop") => {
            let calls = finish_tool_calls(std::mem::take(tool_calls))?;
            if !calls.is_empty() {
                let _ = tx.send(Ok(StreamChunk::tool_calls(calls))).await;
            }
            let _ = tx.send(Ok(StreamChunk::done())).await;
            return Ok(true);
        }
        Some("error") => {
            let message = event
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Anthropic returned an error");
            return Err(ProviderError::Request(message.to_string()));
        }
        _ => {}
    }
    Ok(false)
}

fn finish_tool_calls(
    tool_calls: BTreeMap<usize, ToolAccumulator>,
) -> Result<Vec<ToolCall>, ProviderError> {
    tool_calls
        .into_values()
        .map(|call| {
            if call.id.is_empty() || call.name.is_empty() {
                return Err(ProviderError::Request(
                    "Anthropic returned an incomplete tool call".to_string(),
                ));
            }
            let arguments = if call.partial_json.trim().is_empty() {
                call.initial_input.unwrap_or_else(|| serde_json::json!({}))
            } else {
                serde_json::from_str(&call.partial_json).map_err(|error| {
                    ProviderError::Request(format!(
                        "Anthropic returned invalid tool arguments: {error}"
                    ))
                })?
            };
            Ok(ToolCall {
                id: call.id,
                name: call.name,
                arguments,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    use super::*;

    fn provider(server: &MockServer) -> AnthropicProvider {
        AnthropicProvider::new(
            "anthropic",
            "Anthropic",
            Some(&server.uri()),
            "secret",
            "claude-test",
            None,
            Some(1234),
        )
    }

    #[tokio::test]
    async fn streams_text_with_anthropic_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "secret"))
            .and(header("anthropic-version", ANTHROPIC_VERSION))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "event: content_block_delta\n\
                 data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n\
                 event: message_stop\n\
                 data: {\"type\":\"message_stop\"}\n\n",
            ))
            .mount(&server)
            .await;

        let mut stream = provider(&server)
            .complete(&[ChatMessage::user("hi")], &[], None)
            .await
            .unwrap();
        let mut text = String::new();
        while let Some(chunk) = stream.next().await {
            text.push_str(&chunk.unwrap().delta);
        }
        assert_eq!(text, "Hello");

        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["max_tokens"], 1234);
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[tokio::test]
    async fn assembles_streamed_tool_use() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tool_1\",\"name\":\"weather\",\"input\":{}}}\n\n\
                 data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\":\\\"London\\\"}\"}}\n\n\
                 data: {\"type\":\"message_stop\"}\n\n",
            ))
            .mount(&server)
            .await;

        let mut stream = provider(&server)
            .complete(&[ChatMessage::user("weather")], &[], None)
            .await
            .unwrap();
        let mut calls = Vec::new();
        while let Some(chunk) = stream.next().await {
            calls.extend(chunk.unwrap().tool_calls);
        }
        assert_eq!(calls[0].id, "tool_1");
        assert_eq!(calls[0].name, "weather");
        assert_eq!(calls[0].arguments["city"], "London");
    }

    #[tokio::test]
    async fn converts_system_and_tool_history() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("data: {\"type\":\"message_stop\"}\n\n"),
            )
            .mount(&server)
            .await;

        let call = ToolCall {
            id: "tool_1".to_string(),
            name: "weather".to_string(),
            arguments: serde_json::json!({"city": "London"}),
        };
        let messages = vec![
            ChatMessage::system("Be concise."),
            ChatMessage::assistant_with_tools("", vec![call]),
            ChatMessage::tool("tool_1", "weather", "{\"ok\":true}"),
        ];
        let mut stream = provider(&server)
            .complete(&messages, &[], None)
            .await
            .unwrap();
        while stream.next().await.is_some() {}

        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["system"], "Be concise.");
        assert_eq!(body["messages"][0]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_result");
        assert_eq!(body["messages"][1]["content"][0]["tool_use_id"], "tool_1");
    }
}
