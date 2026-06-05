use anyhow::{Context, bail};
use futures_util::TryStreamExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::io::StreamReader;

use helpcore_api::{
    ChatRequest, LoginRequest, LoginResponse, LogoutRequest, RefreshRequest, RefreshResponse,
    SetupRequest, SetupStatusResponse, SseChunk, SseDone,
};

/// Thin HTTP client for the helpcore server API.
pub struct Client {
    inner: reqwest::Client,
    pub server_url: String,
}

impl Client {
    pub fn new(server_url: impl Into<String>) -> Self {
        Self {
            inner: reqwest::Client::new(),
            server_url: server_url.into().trim_end_matches('/').to_string(),
        }
    }

    // ── Auth ─────────────────────────────────────────────────────────────────

    pub async fn login(&self, email: &str, password: &str) -> anyhow::Result<LoginResponse> {
        let resp = self
            .inner
            .post(format!("{}/auth/login", self.server_url))
            .json(&LoginRequest { email: email.to_string(), password: password.to_string() })
            .send()
            .await
            .context("failed to reach server")?;

        require_success(resp).await?.json().await.context("invalid login response")
    }

    pub async fn logout(&self, refresh_token: &str) -> anyhow::Result<()> {
        let resp = self
            .inner
            .post(format!("{}/auth/logout", self.server_url))
            .json(&LogoutRequest { refresh_token: refresh_token.to_string() })
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp).await?;
        Ok(())
    }

    pub async fn refresh(&self, refresh_token: &str) -> anyhow::Result<RefreshResponse> {
        let resp = self
            .inner
            .post(format!("{}/auth/refresh", self.server_url))
            .json(&RefreshRequest { refresh_token: refresh_token.to_string() })
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp).await?.json().await.context("invalid refresh response")
    }

    // ── Setup ─────────────────────────────────────────────────────────────────

    pub async fn setup_status(&self) -> anyhow::Result<SetupStatusResponse> {
        let resp = self
            .inner
            .get(format!("{}/setup", self.server_url))
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp).await?.json().await.context("invalid setup response")
    }

    pub async fn setup(
        &self,
        token: &str,
        email: &str,
        password: &str,
        display_name: Option<String>,
    ) -> anyhow::Result<LoginResponse> {
        let resp = self
            .inner
            .post(format!("{}/setup", self.server_url))
            .json(&SetupRequest {
                token: token.to_string(),
                email: email.to_string(),
                password: password.to_string(),
                display_name,
            })
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp).await?.json().await.context("invalid setup response")
    }

    // ── Chat ──────────────────────────────────────────────────────────────────

    /// Sends a chat request and calls `on_chunk` for each streaming token.
    /// Returns the `SseDone` event (contains conversation_id and message_id).
    pub async fn chat(
        &self,
        access_token: &str,
        request: &ChatRequest,
        mut on_chunk: impl FnMut(&str),
    ) -> anyhow::Result<SseDone> {
        let resp = self
            .inner
            .post(format!("{}/chat", self.server_url))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(request)
            .send()
            .await
            .context("failed to reach server")?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            bail!("session expired — run 'helpcore login' to authenticate");
        }

        let resp = require_success(resp).await?;
        parse_sse(resp, &mut on_chunk).await
    }
}

// ── SSE parser ────────────────────────────────────────────────────────────────

async fn parse_sse(
    response: reqwest::Response,
    on_chunk: &mut impl FnMut(&str),
) -> anyhow::Result<SseDone> {
    let byte_stream = response
        .bytes_stream()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
    let reader = StreamReader::new(byte_stream);
    let mut lines = BufReader::new(reader).lines();

    let mut event_name = String::new();
    let mut event_data = String::new();

    loop {
        match lines.next_line().await? {
            None => bail!("SSE stream ended without a done event"),
            Some(line) if line.is_empty() => {
                // Blank line = dispatch current event.
                match event_name.as_str() {
                    "chunk" => {
                        if let Ok(chunk) = serde_json::from_str::<SseChunk>(&event_data) {
                            on_chunk(&chunk.delta);
                        }
                    }
                    "done" => {
                        return serde_json::from_str::<SseDone>(&event_data)
                            .context("failed to parse done event");
                    }
                    "error" => {
                        bail!("server error: {}", event_data);
                    }
                    _ => {}
                }
                event_name.clear();
                event_data.clear();
            }
            Some(line) => {
                if let Some(v) = line.strip_prefix("event: ") {
                    event_name = v.to_string();
                } else if let Some(v) = line.strip_prefix("data: ") {
                    event_data = v.to_string();
                }
            }
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

async fn require_success(resp: reqwest::Response) -> anyhow::Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    // Try to extract server's error message.
    let message = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v["message"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| body.clone());
    bail!("server returned {status}: {message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

    #[tokio::test]
    async fn login_returns_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "acc", "refresh_token": "ref", "token_type": "Bearer"
            })))
            .mount(&server).await;

        let client = Client::new(server.uri());
        let resp = client.login("a@b.com", "pass").await.unwrap();
        assert_eq!(resp.access_token, "acc");
    }

    #[tokio::test]
    async fn chat_streams_tokens_and_returns_done() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/chat"))
            .respond_with(ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(
                    "event: chunk\ndata: {\"delta\":\"Hello\"}\n\n\
                     event: chunk\ndata: {\"delta\":\" world\"}\n\n\
                     event: done\ndata: {\"conversation_id\":\"conv1\",\"message_id\":\"msg1\"}\n\n"
                ))
            .mount(&server).await;

        let client = Client::new(server.uri());
        let mut collected = String::new();
        let done = client.chat("token", &ChatRequest {
            conversation_id: None,
            message: "hi".to_string(),
            provider_id: None,
            model: None,
        }, |delta| collected.push_str(delta)).await.unwrap();

        assert_eq!(collected, "Hello world");
        assert_eq!(done.conversation_id, "conv1");
    }

    #[tokio::test]
    async fn server_error_returns_err() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/auth/login"))
            .respond_with(ResponseTemplate::new(401)
                .set_body_json(serde_json::json!({"code":"unauthorized","message":"bad creds"})))
            .mount(&server).await;

        let client = Client::new(server.uri());
        let err = client.login("a@b.com", "wrong").await.unwrap_err();
        assert!(err.to_string().contains("bad creds"));
    }
}
