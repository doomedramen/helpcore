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

/// Default context window — Ollama's built-in default is 2048 which silently
/// truncates prompts. 8192 gives useful context without per-call config.
const DEFAULT_NUM_CTX: u32 = 8192;
/// Default max output tokens — Ollama's built-in default is 128, far too short.
const DEFAULT_NUM_PREDICT: i32 = 2048;

pub struct OllamaProvider {
    id: String,
    name: String,
    base_url: String,
    default_model: String,
    num_ctx: u32,
    num_predict: i32,
    client: Client,
}

impl OllamaProvider {
    pub fn new(
        id: &str,
        name: &str,
        base_url: &str,
        default_model: &str,
        num_ctx: Option<u32>,
        num_predict: Option<u32>,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            default_model: default_model.to_string(),
            num_ctx: num_ctx.unwrap_or(DEFAULT_NUM_CTX),
            num_predict: num_predict.map(|n| n as i32).unwrap_or(DEFAULT_NUM_PREDICT),
            client: Client::new(),
        }
    }
}

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaChatMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OllamaTool<'a>>,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Debug, Serialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaOutgoingToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
}

#[derive(Debug, Serialize)]
struct OllamaOutgoingToolCall {
    function: OllamaOutgoingFunction,
}

#[derive(Debug, Serialize)]
struct OllamaOutgoingFunction {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OllamaTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: OllamaFunction<'a>,
}

#[derive(Debug, Serialize)]
struct OllamaFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    num_ctx: u32,
    num_predict: i32,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamLine {
    message: OllamaMessage,
    done: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
    #[serde(default)]
    tool_calls: Vec<OllamaToolCall>,
}

#[derive(Debug, Deserialize)]
struct OllamaToolCall {
    function: OllamaToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct OllamaToolCallFunction {
    name: String,
    arguments: serde_json::Value,
}

// ── Provider impl ─────────────────────────────────────────────────────────────

#[async_trait]
impl ChatProvider for OllamaProvider {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn default_model(&self) -> &str { &self.default_model }
    fn context_limit(&self) -> u32 { self.num_ctx }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        model: Option<&str>,
    ) -> Result<ProviderStream, ProviderError> {
        let model = model.unwrap_or(&self.default_model);
        let url = format!("{}/api/chat", self.base_url);

        let body = OllamaChatRequest {
            model,
            messages: messages
                .iter()
                .map(|message| OllamaChatMessage {
                    role: message.role.clone(),
                    content: message.content.clone(),
                    tool_calls: message.tool_calls.as_ref().map(|calls| {
                        calls
                            .iter()
                            .map(|call| OllamaOutgoingToolCall {
                                function: OllamaOutgoingFunction {
                                    name: call.name.clone(),
                                    arguments: call.arguments.clone(),
                                },
                            })
                            .collect()
                    }),
                    tool_name: message.tool_name.clone(),
                })
                .collect(),
            tools: tools
                .iter()
                .map(|tool| OllamaTool {
                    kind: "function",
                    function: OllamaFunction {
                        name: &tool.name,
                        description: &tool.description,
                        parameters: &tool.input_schema,
                    },
                })
                .collect(),
            stream: true,
            options: OllamaOptions {
                num_ctx: self.num_ctx,
                num_predict: self.num_predict,
            },
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(http_error(status, &body_text));
        }

        // Convert the byte stream to a line-by-line reader, then spawn a task
        // that sends parsed chunks into a channel.
        let byte_stream = response
            .bytes_stream()
            .map_err(std::io::Error::other);
        let reader = StreamReader::new(byte_stream);
        let mut lines = BufReader::new(reader).lines();

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<StreamChunk, ProviderError>>(32);

        tokio::spawn(async move {
            loop {
                match lines.next_line().await {
                    Ok(None) => break,
                    Ok(Some(line)) if line.is_empty() => continue,
                    Ok(Some(line)) => {
                        match serde_json::from_str::<OllamaStreamLine>(&line) {
                            Ok(parsed) => {
                                if !parsed.message.content.is_empty() {
                                    let _ = tx
                                        .send(Ok(StreamChunk::delta(parsed.message.content)))
                                        .await;
                                }
                                if !parsed.message.tool_calls.is_empty() {
                                    let calls = parsed
                                        .message
                                        .tool_calls
                                        .into_iter()
                                        .map(|call| ToolCall {
                                            id: uuid::Uuid::new_v4().to_string(),
                                            name: call.function.name,
                                            arguments: call.function.arguments,
                                        })
                                        .collect();
                                    let _ = tx.send(Ok(StreamChunk::tool_calls(calls))).await;
                                }
                                if parsed.done {
                                    let _ = tx.send(Ok(StreamChunk::done())).await;
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(Err(ProviderError::Request(format!(
                                        "failed to parse Ollama response: {e} — line: {line}"
                                    ))))
                                    .await;
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(ProviderError::Request(format!(
                                "stream read error: {e}"
                            ))))
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    fn ndjson(lines: &[&str]) -> String {
        lines.join("\n") + "\n"
    }

    #[tokio::test]
    async fn streams_tokens_from_ollama() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ndjson(&[
                r#"{"message":{"role":"assistant","content":"Hello"},"done":false}"#,
                r#"{"message":{"role":"assistant","content":" world"},"done":false}"#,
                r#"{"message":{"role":"assistant","content":""},"done":true}"#,
            ])))
            .mount(&server)
            .await;

        let provider = OllamaProvider::new("ollama", "Ollama", &server.uri(), "llama3", None, None);
        let messages = vec![ChatMessage::user("hi")];
        let mut stream = provider.complete(&messages, &[], None).await.unwrap();

        let mut text = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            if !chunk.is_final {
                text.push_str(&chunk.delta);
            }
        }
        assert_eq!(text, "Hello world");
    }

    #[tokio::test]
    async fn model_override_is_sent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ndjson(&[
                r#"{"message":{"role":"assistant","content":"ok"},"done":false}"#,
                r#"{"message":{"role":"assistant","content":""},"done":true}"#,
            ])))
            .mount(&server)
            .await;

        let provider =
            OllamaProvider::new("ollama", "Ollama", &server.uri(), "llama3", None, None);
        let messages = vec![ChatMessage::user("hello")];
        let mut stream = provider.complete(&messages, &[], Some("mistral")).await.unwrap();

        // Just consume the stream — the key assertion is no error.
        while let Some(chunk) = stream.next().await {
            chunk.unwrap();
        }

        // Verify the request body had the overridden model.
        let received = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
        assert_eq!(body["model"].as_str(), Some("mistral"));
    }

    #[tokio::test]
    async fn returns_error_on_non_200() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let provider = OllamaProvider::new("ollama", "Ollama", &server.uri(), "llama3", None, None);
        let result = provider.complete(&[ChatMessage::user("hi")], &[], None).await;
        assert!(matches!(result, Err(ProviderError::Unavailable)));
    }

    #[tokio::test]
    async fn streams_tool_calls_from_ollama() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ndjson(&[
                r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"weather","arguments":{"city":"London"}}}]},"done":false}"#,
                r#"{"message":{"role":"assistant","content":""},"done":true}"#,
            ])))
            .mount(&server)
            .await;

        let provider = OllamaProvider::new("ollama", "Ollama", &server.uri(), "llama3", None, None);
        let tools = vec![ToolDefinition {
            name: "weather".into(),
            description: "Weather".into(),
            input_schema: serde_json::json!({"type": "object"}),
        }];
        let mut stream = provider
            .complete(&[ChatMessage::user("weather")], &tools, None)
            .await
            .unwrap();
        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk.tool_calls[0].name, "weather");

        let received = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
        assert_eq!(body["tools"][0]["function"]["name"], "weather");
    }

    #[tokio::test]
    async fn sends_tool_history_in_ollama_format() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ndjson(&[
                r#"{"message":{"role":"assistant","content":"done"},"done":false}"#,
                r#"{"message":{"role":"assistant","content":""},"done":true}"#,
            ])))
            .mount(&server)
            .await;

        let call = ToolCall {
            id: "call-1".to_string(),
            name: "weather".to_string(),
            arguments: serde_json::json!({"city": "London"}),
        };
        let messages = vec![
            ChatMessage::assistant_with_tools("", vec![call]),
            ChatMessage::tool("call-1", "weather", "{\"ok\":true}"),
        ];
        let provider = OllamaProvider::new("ollama", "Ollama", &server.uri(), "llama3", None, None);
        let mut stream = provider.complete(&messages, &[], None).await.unwrap();
        while stream.next().await.is_some() {}

        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            body["messages"][0]["tool_calls"][0]["function"]["name"],
            "weather"
        );
        assert!(body["messages"][0]["tool_calls"][0].get("id").is_none());
        assert_eq!(body["messages"][1]["tool_name"], "weather");
        assert!(body["messages"][1].get("tool_call_id").is_none());
    }
}
