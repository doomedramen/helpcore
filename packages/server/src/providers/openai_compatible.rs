#![allow(missing_docs)]

use std::collections::BTreeMap;

use async_trait::async_trait;
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::io::StreamReader;

use super::{
    error::{ProviderError, http_error},
    traits::{ChatProvider, ProviderStream},
    types::{ChatMessage, StreamChunk, ToolCall, ToolDefinition},
};

const DEFAULT_CONTEXT_LIMIT: u32 = 128_000;
const DEFAULT_MAX_TOKENS: u32 = 4096;

#[derive(Debug, Clone, Copy)]
pub enum OutputTokenField {
    MaxTokens,
    MaxCompletionTokens,
}

pub struct OpenAiCompatibleProvider {
    id: String,
    name: String,
    base_url: String,
    api_key: Option<String>,
    default_model: String,
    context_limit: u32,
    max_tokens: u32,
    output_token_field: OutputTokenField,
    client: Client,
}

impl OpenAiCompatibleProvider {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &str,
        name: &str,
        base_url: &str,
        api_key: Option<&str>,
        default_model: &str,
        context_limit: Option<u32>,
        max_tokens: Option<u32>,
        output_token_field: OutputTokenField,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.map(str::to_string),
            default_model: default_model.to_string(),
            context_limit: context_limit.unwrap_or(DEFAULT_CONTEXT_LIMIT),
            max_tokens: max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            output_token_field,
            client: Client::new(),
        }
    }

    fn chat_completions_url(&self) -> String {
        if self.base_url.ends_with("/chat/completions") {
            self.base_url.clone()
        } else {
            format!("{}/chat/completions", self.base_url)
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<WireMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<WireTool>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct WireMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<WireToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct WireToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireFunctionCall,
}

#[derive(Debug, Serialize)]
struct WireFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct WireTool {
    #[serde(rename = "type")]
    kind: &'static str,
    function: WireToolDefinition,
}

#[derive(Debug, Serialize)]
struct WireToolDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct StreamEnvelope {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    error: Option<ApiErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
}

#[derive(Debug, Default, Deserialize)]
struct StreamDelta {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct ToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<FunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct FunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct ToolAccumulator {
    id: String,
    name: String,
    arguments: String,
}

#[async_trait]
impl ChatProvider for OpenAiCompatibleProvider {
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
        let body = ChatRequest {
            model: model.unwrap_or(&self.default_model).to_string(),
            messages: messages.iter().map(convert_message).collect(),
            tools: tools
                .iter()
                .map(|tool| WireTool {
                    kind: "function",
                    function: WireToolDefinition {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        parameters: tool.input_schema.clone(),
                    },
                })
                .collect(),
            stream: true,
            max_tokens: matches!(self.output_token_field, OutputTokenField::MaxTokens)
                .then_some(self.max_tokens),
            max_completion_tokens: matches!(
                self.output_token_field,
                OutputTokenField::MaxCompletionTokens
            )
            .then_some(self.max_tokens),
        };

        let mut request = self
            .client
            .post(self.chat_completions_url())
            .header("accept", "text/event-stream")
            .json(&body);
        if let Some(api_key) = self.api_key.as_deref() {
            request = request.bearer_auth(api_key);
        }
        let response = request
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

fn convert_message(message: &ChatMessage) -> WireMessage {
    let tool_calls = message.tool_calls.as_ref().map(|calls| {
        calls
            .iter()
            .map(|call| WireToolCall {
                id: call.id.clone(),
                kind: "function",
                function: WireFunctionCall {
                    name: call.name.clone(),
                    arguments: serde_json::to_string(&call.arguments)
                        .unwrap_or_else(|_| "{}".to_string()),
                },
            })
            .collect()
    });
    let content =
        if message.role == "assistant" && message.content.is_empty() && tool_calls.is_some() {
            None
        } else {
            Some(message.content.clone())
        };

    WireMessage {
        role: message.role.clone(),
        content,
        tool_calls,
        tool_call_id: message.tool_call_id.clone(),
        name: message.tool_name.clone(),
    }
}

async fn handle_event(
    data: &str,
    tool_calls: &mut BTreeMap<usize, ToolAccumulator>,
    tx: &tokio::sync::mpsc::Sender<Result<StreamChunk, ProviderError>>,
) -> Result<bool, ProviderError> {
    if data.trim() == "[DONE]" {
        let calls = finish_tool_calls(std::mem::take(tool_calls))?;
        if !calls.is_empty() {
            let _ = tx.send(Ok(StreamChunk::tool_calls(calls))).await;
        }
        let _ = tx.send(Ok(StreamChunk::done())).await;
        return Ok(true);
    }

    let envelope: StreamEnvelope = serde_json::from_str(data).map_err(|error| {
        ProviderError::Request(format!(
            "failed to parse OpenAI-compatible response: {error} - data: {data}"
        ))
    })?;
    if let Some(error) = envelope.error {
        return Err(ProviderError::Request(
            error
                .message
                .unwrap_or_else(|| "provider returned an error".to_string()),
        ));
    }

    for choice in envelope.choices {
        if let Some(content) = choice.delta.content.filter(|content| !content.is_empty()) {
            let _ = tx.send(Ok(StreamChunk::delta(content))).await;
        }
        for delta in choice.delta.tool_calls {
            let call = tool_calls.entry(delta.index).or_default();
            if let Some(id) = delta.id {
                call.id.push_str(&id);
            }
            if let Some(function) = delta.function {
                if let Some(name) = function.name {
                    call.name.push_str(&name);
                }
                if let Some(arguments) = function.arguments {
                    call.arguments.push_str(&arguments);
                }
            }
        }
    }
    Ok(false)
}

fn finish_tool_calls(
    tool_calls: BTreeMap<usize, ToolAccumulator>,
) -> Result<Vec<ToolCall>, ProviderError> {
    tool_calls
        .into_values()
        .map(|call| {
            if call.name.is_empty() {
                return Err(ProviderError::Request(
                    "provider returned a tool call without a name".to_string(),
                ));
            }
            let arguments = if call.arguments.trim().is_empty() {
                serde_json::json!({})
            } else {
                serde_json::from_str(&call.arguments).map_err(|error| {
                    ProviderError::Request(format!(
                        "provider returned invalid tool arguments: {error}"
                    ))
                })?
            };
            Ok(ToolCall {
                id: if call.id.is_empty() {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    call.id
                },
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

    fn provider(server: &MockServer, field: OutputTokenField) -> OpenAiCompatibleProvider {
        OpenAiCompatibleProvider::new(
            "openai",
            "OpenAI",
            &format!("{}/v1", server.uri()),
            Some("secret"),
            "gpt-test",
            None,
            Some(1234),
            field,
        )
    }

    #[tokio::test]
    async fn streams_text_and_uses_bearer_auth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer secret"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n\
                 data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\n\
                 data: [DONE]\n\n",
            ))
            .mount(&server)
            .await;

        let mut stream = provider(&server, OutputTokenField::MaxCompletionTokens)
            .complete(&[ChatMessage::user("hi")], &[], None)
            .await
            .unwrap();
        let mut text = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            text.push_str(&chunk.delta);
        }
        assert_eq!(text, "Hello world");

        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["max_completion_tokens"], 1234);
        assert!(body.get("max_tokens").is_none());
    }

    #[tokio::test]
    async fn assembles_fragmented_tool_calls() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"weather\",\"arguments\":\"{\\\"city\\\":\"}}]}}]}\n\n\
                 data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"London\\\"}\"}}]}}]}\n\n\
                 data: [DONE]\n\n",
            ))
            .mount(&server)
            .await;

        let mut stream = provider(&server, OutputTokenField::MaxTokens)
            .complete(&[ChatMessage::user("weather")], &[], None)
            .await
            .unwrap();
        let mut calls = Vec::new();
        while let Some(chunk) = stream.next().await {
            calls.extend(chunk.unwrap().tool_calls);
        }
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].name, "weather");
        assert_eq!(calls[0].arguments["city"], "London");
    }

    #[tokio::test]
    async fn sends_tool_history_in_openai_format() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("data: [DONE]\n\n"))
            .mount(&server)
            .await;

        let call = ToolCall {
            id: "call_1".to_string(),
            name: "weather".to_string(),
            arguments: serde_json::json!({"city": "London"}),
        };
        let messages = vec![
            ChatMessage::assistant_with_tools("", vec![call]),
            ChatMessage::tool("call_1", "weather", "{\"ok\":true}"),
        ];
        let mut stream = provider(&server, OutputTokenField::MaxTokens)
            .complete(&messages, &[], None)
            .await
            .unwrap();
        while stream.next().await.is_some() {}

        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["messages"][0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(body["messages"][1]["tool_call_id"], "call_1");
    }

    #[tokio::test]
    async fn errors_when_stream_ends_without_done() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
                ),
            )
            .mount(&server)
            .await;

        let mut stream = provider(&server, OutputTokenField::MaxTokens)
            .complete(&[ChatMessage::user("hi")], &[], None)
            .await
            .unwrap();
        assert!(stream.next().await.unwrap().is_ok());
        assert!(stream.next().await.unwrap().is_err());
    }
}
