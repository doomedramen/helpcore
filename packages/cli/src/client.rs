use anyhow::{Context, bail};
use futures_util::TryStreamExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::io::StreamReader;

use helpcore_api::{
    ApiKeyInfo, ChatRequest, CompactResponse, ConversationSummary, CreateApiKeyRequest,
    CreateApiKeyResponse, CurrentUserResponse, ListApiKeysResponse, LoginRequest, LoginResponse,
    LogoutRequest, MemoryEntry, MemoryListResponse, MemoryReadResponse, MemoryWriteRequest,
    MessageSummary, PersonalityResponse, PersonalityWriteRequest, PluginInfo, PluginListResponse,
    PluginTokenRequest, PluginTokenResponse, RefreshRequest, RefreshResponse, SetupRequest,
    SetupStatusResponse, SseChunk, SseDone,
};

/// Sentinel string returned by `chat()` on a 401 so callers can detect an
/// expired access token and attempt a refresh without string-matching on
/// localised error messages.
const TOKEN_EXPIRED: &str = "access_token_expired";

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
            .post(format!("{}/api/auth/login", self.server_url))
            .json(&LoginRequest {
                email: email.to_string(),
                password: password.to_string(),
            })
            .send()
            .await
            .context("failed to reach server")?;

        require_success(resp)
            .await?
            .json()
            .await
            .context("invalid login response")
    }

    pub async fn logout(&self, refresh_token: &str) -> anyhow::Result<()> {
        let resp = self
            .inner
            .post(format!("{}/api/auth/logout", self.server_url))
            .json(&LogoutRequest {
                refresh_token: refresh_token.to_string(),
            })
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp).await?;
        Ok(())
    }

    pub async fn current_user(&self, access_token: &str) -> anyhow::Result<CurrentUserResponse> {
        let resp = self
            .inner
            .get(format!("{}/api/auth/me", self.server_url))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            bail!(TOKEN_EXPIRED);
        }
        require_success(resp)
            .await?
            .json()
            .await
            .context("invalid current-user response")
    }

    pub async fn refresh(&self, refresh_token: &str) -> anyhow::Result<RefreshResponse> {
        let resp = self
            .inner
            .post(format!("{}/api/auth/refresh", self.server_url))
            .json(&RefreshRequest {
                refresh_token: refresh_token.to_string(),
            })
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp)
            .await?
            .json()
            .await
            .context("invalid refresh response")
    }

    // ── Setup ─────────────────────────────────────────────────────────────────

    pub async fn setup_status(&self) -> anyhow::Result<SetupStatusResponse> {
        let resp = self
            .inner
            .get(format!("{}/api/setup", self.server_url))
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp)
            .await?
            .json()
            .await
            .context("invalid setup response")
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
            .post(format!("{}/api/setup", self.server_url))
            .json(&SetupRequest {
                token: token.to_string(),
                email: email.to_string(),
                password: password.to_string(),
                display_name,
            })
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp)
            .await?
            .json()
            .await
            .context("invalid setup response")
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
            .post(format!("{}/api/chat", self.server_url))
            .header("Authorization", format!("Bearer {access_token}"))
            .json(request)
            .send()
            .await
            .context("failed to reach server")?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            bail!(TOKEN_EXPIRED);
        }

        let resp = require_success(resp).await?;
        parse_sse(resp, &mut on_chunk).await
    }

    /// Returns true when `e` is the sentinel error emitted by `chat()` on a 401.
    /// Use this to decide whether to attempt a token refresh and retry.
    pub fn is_token_expired(e: &anyhow::Error) -> bool {
        e.to_string() == TOKEN_EXPIRED
    }

    pub async fn list_conversations(
        &self,
        access_token: &str,
    ) -> anyhow::Result<Vec<ConversationSummary>> {
        let resp = self
            .inner
            .get(format!("{}/api/conversations", self.server_url))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp)
            .await?
            .json()
            .await
            .context("invalid conversation list response")
    }

    pub async fn get_conversation_messages(
        &self,
        access_token: &str,
        conversation_id: &str,
    ) -> anyhow::Result<Vec<MessageSummary>> {
        let resp = self
            .inner
            .get(format!(
                "{}/api/conversations/{conversation_id}/messages",
                self.server_url
            ))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("conversation not found: {conversation_id}");
        }
        require_success(resp)
            .await?
            .json()
            .await
            .context("invalid messages response")
    }

    // ── Memory ────────────────────────────────────────────────────────────────

    pub async fn list_memory(&self, access_token: &str) -> anyhow::Result<Vec<MemoryEntry>> {
        let resp = self
            .inner
            .get(format!("{}/api/memory", self.server_url))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        let body: MemoryListResponse = require_success(resp)
            .await?
            .json()
            .await
            .context("invalid memory list response")?;
        Ok(body.files)
    }

    pub async fn get_memory(
        &self,
        access_token: &str,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        let resp = self
            .inner
            .get(format!("{}/api/memory/{path}", self.server_url))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let body: MemoryReadResponse = require_success(resp)
            .await?
            .json()
            .await
            .context("invalid memory response")?;
        Ok(Some(body.content))
    }

    pub async fn set_memory(
        &self,
        access_token: &str,
        path: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        let resp = self
            .inner
            .put(format!("{}/api/memory/{path}", self.server_url))
            .bearer_auth(access_token)
            .json(&MemoryWriteRequest {
                content: content.to_string(),
            })
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp).await?;
        Ok(())
    }

    pub async fn delete_memory(&self, access_token: &str, path: &str) -> anyhow::Result<bool> {
        let resp = self
            .inner
            .delete(format!("{}/api/memory/{path}", self.server_url))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        require_success(resp).await?;
        Ok(true)
    }

    // ── Personality ───────────────────────────────────────────────────────────

    pub async fn get_personality(
        &self,
        access_token: &str,
        name: &str,
    ) -> anyhow::Result<Option<String>> {
        let resp = self
            .inner
            .get(format!("{}/api/personality/{name}", self.server_url))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let body: PersonalityResponse = require_success(resp)
            .await?
            .json()
            .await
            .context("invalid personality response")?;
        Ok(Some(body.content))
    }

    // ── Compact ───────────────────────────────────────────────────────────────

    pub async fn compact(
        &self,
        access_token: &str,
        conversation_id: &str,
    ) -> anyhow::Result<CompactResponse> {
        let resp = self
            .inner
            .post(format!(
                "{}/api/conversations/{conversation_id}/compact",
                self.server_url
            ))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp)
            .await?
            .json()
            .await
            .context("invalid compact response")
    }

    // ── Plugins ───────────────────────────────────────────────────────────────

    pub async fn list_plugins(&self, access_token: &str) -> anyhow::Result<Vec<PluginInfo>> {
        let resp = self
            .inner
            .get(format!("{}/api/plugins", self.server_url))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        let body: PluginListResponse = require_success(resp)
            .await?
            .json()
            .await
            .context("invalid plugin list response")?;
        Ok(body.plugins)
    }

    pub async fn create_plugin_token(
        &self,
        access_token: &str,
        plugin_id: &str,
        permissions: Vec<String>,
    ) -> anyhow::Result<PluginTokenResponse> {
        let resp = self
            .inner
            .post(format!(
                "{}/api/plugins/{plugin_id}/tokens",
                self.server_url
            ))
            .bearer_auth(access_token)
            .json(&PluginTokenRequest { permissions })
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp)
            .await?
            .json()
            .await
            .context("invalid token response")
    }

    pub async fn set_plugin_enabled(
        &self,
        access_token: &str,
        plugin_id: &str,
        enabled: bool,
    ) -> anyhow::Result<()> {
        let resp = self
            .inner
            .put(format!(
                "{}/api/plugins/{plugin_id}/enable",
                self.server_url
            ))
            .bearer_auth(access_token)
            .json(&serde_json::json!({ "enabled": enabled }))
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp).await?;
        Ok(())
    }

    // ── API keys ──────────────────────────────────────────────────────────────

    pub async fn list_api_keys(&self, access_token: &str) -> anyhow::Result<Vec<ApiKeyInfo>> {
        let resp = self
            .inner
            .get(format!("{}/api/auth/api-keys", self.server_url))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        let body: ListApiKeysResponse = require_success(resp)
            .await?
            .json()
            .await
            .context("invalid api-keys response")?;
        Ok(body.keys)
    }

    pub async fn create_api_key(
        &self,
        access_token: &str,
        name: &str,
    ) -> anyhow::Result<CreateApiKeyResponse> {
        let resp = self
            .inner
            .post(format!("{}/api/auth/api-keys", self.server_url))
            .bearer_auth(access_token)
            .json(&CreateApiKeyRequest {
                name: name.to_string(),
                expires_at: None,
            })
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp)
            .await?
            .json()
            .await
            .context("invalid create-key response")
    }

    pub async fn revoke_api_key(&self, access_token: &str, key_id: &str) -> anyhow::Result<()> {
        let resp = self
            .inner
            .delete(format!("{}/api/auth/api-keys/{key_id}", self.server_url))
            .bearer_auth(access_token)
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp).await?;
        Ok(())
    }

    pub async fn set_personality(
        &self,
        access_token: &str,
        name: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        let resp = self
            .inner
            .put(format!("{}/api/personality/{name}", self.server_url))
            .bearer_auth(access_token)
            .json(&PersonalityWriteRequest {
                content: content.to_string(),
            })
            .send()
            .await
            .context("failed to reach server")?;
        require_success(resp).await?;
        Ok(())
    }
}

// ── SSE parser ────────────────────────────────────────────────────────────────

async fn parse_sse(
    response: reqwest::Response,
    on_chunk: &mut impl FnMut(&str),
) -> anyhow::Result<SseDone> {
    let byte_stream = response.bytes_stream().map_err(std::io::Error::other);
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
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    #[tokio::test]
    async fn login_returns_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "acc", "refresh_token": "ref", "token_type": "Bearer"
            })))
            .mount(&server)
            .await;

        let client = Client::new(server.uri());
        let resp = client.login("a@b.com", "pass").await.unwrap();
        assert_eq!(resp.access_token, "acc");
    }

    #[tokio::test]
    async fn chat_streams_tokens_and_returns_done() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/api/chat"))
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
        let done = client
            .chat(
                "token",
                &ChatRequest {
                    conversation_id: None,
                    message: "hi".to_string(),
                    provider_id: None,
                    model: None,
                },
                |delta| collected.push_str(delta),
            )
            .await
            .unwrap();

        assert_eq!(collected, "Hello world");
        assert_eq!(done.conversation_id, "conv1");
    }

    #[tokio::test]
    async fn server_error_returns_err() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(
                    serde_json::json!({"code":"unauthorized","message":"bad creds"}),
                ),
            )
            .mount(&server)
            .await;

        let client = Client::new(server.uri());
        let err = client.login("a@b.com", "wrong").await.unwrap_err();
        assert!(err.to_string().contains("bad creds"));
    }

    #[tokio::test]
    async fn chat_401_produces_token_expired_sentinel() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = Client::new(server.uri());
        let err = client
            .chat(
                "expired",
                &ChatRequest {
                    conversation_id: None,
                    message: "hi".to_string(),
                    provider_id: None,
                    model: None,
                },
                |_| {},
            )
            .await
            .unwrap_err();

        assert!(
            Client::is_token_expired(&err),
            "expected token-expired sentinel, got: {err}"
        );
    }

    #[tokio::test]
    async fn refresh_returns_new_tokens() {
        let server = MockServer::start().await;
        // Simulate first chat → 401, then refresh → new tokens, then chat → 200.
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .and(header("authorization", "Bearer old-token"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "new-token",
                "refresh_token": "new-refresh",
                "token_type": "Bearer",
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .and(header("authorization", "Bearer new-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(
                        "event: done\ndata: {\"conversation_id\":\"c1\",\"message_id\":\"m1\"}\n\n",
                    ),
            )
            .mount(&server)
            .await;

        let client = Client::new(server.uri());
        let request = ChatRequest {
            conversation_id: None,
            message: "hi".to_string(),
            provider_id: None,
            model: None,
        };

        // Caller detects 401, refreshes, retries.
        let first_err = client
            .chat("old-token", &request, |_| {})
            .await
            .unwrap_err();
        assert!(Client::is_token_expired(&first_err));

        let tokens = client.refresh("old-refresh").await.unwrap();
        assert_eq!(tokens.access_token, "new-token");

        let done = client.chat("new-token", &request, |_| {}).await.unwrap();
        assert_eq!(done.conversation_id, "c1");
    }
}
