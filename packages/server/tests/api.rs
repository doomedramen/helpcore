/// End-to-end HTTP tests — no real network, uses axum's in-process service.
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use helpcore_api::{
    ConversationSummary, LoginResponse, MessageSummary, RefreshResponse, SetupStatusResponse,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

// ── helpers ───────────────────────────────────────────────────────────────────

fn test_state() -> Arc<helpcore_server::state::AppState> {
    test_state_with_providers(vec![])
}

fn test_state_with_providers(
    providers: Vec<Arc<dyn helpcore_server::providers::traits::ChatProvider>>,
) -> Arc<helpcore_server::state::AppState> {
    use helpcore_server::{config, db, state};

    let db_pool = Arc::new(db::open_in_memory());
    let cfg: config::Config =
        toml::from_str(r#"[server]
name = "test"
url  = "http://localhost:3000""#)
            .unwrap();
    let data_dir = std::env::temp_dir().join(format!(
        "helpcore-test-data-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&data_dir).unwrap();
    Arc::new(state::AppState {
        config: Arc::new(cfg),
        config_path: std::env::temp_dir().join(format!(
            "helpcore-test-config-{}.toml",
            uuid::Uuid::new_v4()
        )),
        data_dir,
        db: db_pool,
        providers,
    })
}

fn app(state: Arc<helpcore_server::state::AppState>) -> axum::Router {
    helpcore_server::api::router::create(state)
}

async fn json_body<T: DeserializeOwned>(body: Body) -> T {
    let bytes = to_bytes(body, usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn post_json(app: axum::Router, uri: &str, body: Value) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn authed_get(app: axum::Router, uri: &str, token: &str) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn authed_post_json(
    app: axum::Router,
    uri: &str,
    token: &str,
    body: Value,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn authed_put_json(
    app: axum::Router,
    uri: &str,
    token: &str,
    body: Value,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("PUT")
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn do_setup(state: Arc<helpcore_server::state::AppState>) -> LoginResponse {
    use helpcore_server::auth::setup;
    let token = state.db.call_sync(|conn| setup::generate_setup_token(conn)).unwrap();
    let resp = post_json(
        app(Arc::clone(&state)),
        "/api/setup",
        json!({ "token": token, "email": "admin@example.com", "password": "password123" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    json_body(resp.into_body()).await
}

// ── auth tests ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_200() {
    let state = test_state();
    let resp = app(state)
        .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn setup_status_true_on_fresh_db() {
    let state = test_state();
    let resp = app(Arc::clone(&state))
        .oneshot(Request::builder().uri("/api/setup").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: SetupStatusResponse = json_body(resp.into_body()).await;
    assert!(body.setup_required);
}

#[tokio::test]
async fn setup_creates_admin_and_returns_tokens() {
    let state = test_state();
    let tokens = do_setup(Arc::clone(&state)).await;
    assert!(!tokens.access_token.is_empty());
    assert_eq!(tokens.token_type, "Bearer");
}

#[tokio::test]
async fn setup_status_false_after_setup() {
    let state = test_state();
    do_setup(Arc::clone(&state)).await;
    let resp = app(Arc::clone(&state))
        .oneshot(Request::builder().uri("/api/setup").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body: SetupStatusResponse = json_body(resp.into_body()).await;
    assert!(!body.setup_required);
}

#[tokio::test]
async fn setup_rejects_invalid_token() {
    let state = test_state();
    let resp = post_json(
        app(state),
        "/api/setup",
        json!({ "token": "bad-token", "email": "a@b.com", "password": "password123" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn login_returns_tokens() {
    let state = test_state();
    do_setup(Arc::clone(&state)).await;
    let resp = post_json(
        app(Arc::clone(&state)),
        "/api/auth/login",
        json!({ "email": "admin@example.com", "password": "password123" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let tokens: LoginResponse = json_body(resp.into_body()).await;
    assert!(!tokens.access_token.is_empty());
}

#[tokio::test]
async fn login_rejects_wrong_password() {
    let state = test_state();
    do_setup(Arc::clone(&state)).await;
    let resp = post_json(
        app(state),
        "/api/auth/login",
        json!({ "email": "admin@example.com", "password": "wrong" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_rejects_unknown_email() {
    let state = test_state();
    do_setup(Arc::clone(&state)).await;
    let resp = post_json(
        app(state),
        "/api/auth/login",
        json!({ "email": "nobody@example.com", "password": "password123" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_rotates_tokens() {
    let state = test_state();
    let first = do_setup(Arc::clone(&state)).await;
    let resp = post_json(
        app(Arc::clone(&state)),
        "/api/auth/refresh",
        json!({ "refresh_token": first.refresh_token }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let second: RefreshResponse = json_body(resp.into_body()).await;
    assert_ne!(second.access_token, first.access_token);
}

#[tokio::test]
async fn refresh_rejects_invalid_token() {
    let state = test_state();
    let resp = post_json(
        app(state),
        "/api/auth/refresh",
        json!({ "refresh_token": "not-a-real-token" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_revokes_session() {
    let state = test_state();
    let tokens = do_setup(Arc::clone(&state)).await;
    let resp = post_json(
        app(Arc::clone(&state)),
        "/api/auth/logout",
        json!({ "refresh_token": tokens.refresh_token }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let resp = post_json(
        app(state),
        "/api/auth/refresh",
        json!({ "refresh_token": tokens.refresh_token }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn current_user_returns_admin_role() {
    let state = test_state();
    let tokens = do_setup(Arc::clone(&state)).await;
    let resp = authed_get(app(state), "/api/auth/me", &tokens.access_token).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: helpcore_api::CurrentUserResponse = json_body(resp.into_body()).await;
    assert_eq!(body.email, "admin@example.com");
    assert_eq!(body.role, "admin");
}

#[tokio::test]
async fn admin_can_read_and_update_config_without_exposing_api_key() {
    let state = test_state();
    let original = r#"
[server]
name = "test"
url = "http://localhost:3000"

[registry]
url = "https://example.com/plugins.json"

[[providers]]
id = "openai"
name = "OpenAI"
type = "openai"
api_key = "secret-key"
default_model = "gpt-4o"
roles = ["chat"]
"#;
    std::fs::write(&state.config_path, original).unwrap();
    let tokens = do_setup(Arc::clone(&state)).await;

    let resp = authed_get(
        app(Arc::clone(&state)),
        "/api/admin/config",
        &tokens.access_token,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: helpcore_api::AdminConfigResponse = json_body(resp.into_body()).await;
    assert!(body.providers[0].api_key_configured);

    let resp = authed_put_json(
        app(Arc::clone(&state)),
        "/api/admin/config",
        &tokens.access_token,
        json!({
            "server": { "name": "Updated", "url": "http://localhost:4000", "port": 4000 },
            "logging_level": "debug",
            "registry_url": "https://example.com/new-registry.json",
            "plugin_blacklist": ["blocked-plugin"],
            "providers": [{
                "id": "openai",
                "name": "OpenAI",
                "provider_type": "openai",
                "api_key": null,
                "clear_api_key": false,
                "url": null,
                "default_model": "gpt-4.1",
                "roles": ["chat"],
                "num_ctx": null,
                "num_predict": null
            }]
        }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let saved = helpcore_server::config::Config::load(&state.config_path).unwrap();
    assert_eq!(saved.server.name, "Updated");
    assert_eq!(saved.providers[0].api_key.as_deref(), Some("secret-key"));
    assert_eq!(saved.plugins.blacklist, vec!["blocked-plugin"]);
}

#[tokio::test]
async fn member_cannot_read_admin_config() {
    let state = test_state();
    let access_token = state
        .db
        .call_sync(|conn| {
            conn.execute(
                "INSERT INTO users
                    (id, email, password_hash, role, status, created_at, updated_at)
                 VALUES ('member-1', 'member@example.com', 'hash', 'member', 'active', '2024-01-01', '2024-01-01')",
                [],
            )?;
            Ok(helpcore_server::auth::token::create_session(conn, "member-1")?.access_token)
        })
        .unwrap();

    let resp = authed_get(app(state), "/api/admin/config", &access_token).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn plugin_store_marks_blocked_plugins() {
    let state = test_state();
    let registry_path = std::env::temp_dir().join(format!(
        "helpcore-test-registry-{}.json",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(
        &registry_path,
        serde_json::to_vec(&json!({
            "plugins": [{
                "id": "weather",
                "name": "Weather",
                "description": "Weather forecasts.",
                "version": "1.0.0",
                "tier": "bridge",
                "author": "test",
                "homepage": "https://example.com/weather",
                "permissions": ["outbound_http"],
                "source": { "type": "github", "repo": "test/weather", "ref": "1.0.0" }
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        &state.config_path,
        format!(
            r#"
[server]
name = "test"
url = "http://localhost:3000"

[plugins]
blacklist = ["weather"]

[registry]
url = "file://{}"
"#,
            registry_path.display()
        ),
    )
    .unwrap();

    let tokens = do_setup(Arc::clone(&state)).await;
    let resp = authed_get(
        app(state),
        "/api/plugins/store",
        &tokens.access_token,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: helpcore_api::PluginStoreResponse = json_body(resp.into_body()).await;
    assert_eq!(body.plugins.len(), 1);
    assert!(body.plugins[0].blocked);
    assert!(!body.plugins[0].installed);
}

// ── conversation tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn list_conversations_empty_initially() {
    let state = test_state();
    let tokens = do_setup(Arc::clone(&state)).await;
    let resp = authed_get(app(Arc::clone(&state)), "/api/conversations", &tokens.access_token).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let convs: Vec<ConversationSummary> = json_body(resp.into_body()).await;
    assert!(convs.is_empty());
}

#[tokio::test]
async fn get_messages_returns_404_for_unknown() {
    let state = test_state();
    let tokens = do_setup(Arc::clone(&state)).await;
    let resp = authed_get(
        app(state),
        "/api/conversations/no-such-id/messages",
        &tokens.access_token,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn chat_requires_authentication() {
    let state = test_state();
    let resp = post_json(
        app(state),
        "/api/chat",
        json!({ "message": "hello" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn chat_returns_error_when_no_providers() {
    let state = test_state(); // no providers
    let tokens = do_setup(Arc::clone(&state)).await;
    let resp = authed_post_json(
        app(state),
        "/api/chat",
        &tokens.access_token,
        json!({ "message": "hello" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn chat_streams_and_persists_conversation() {
    use helpcore_server::providers::{
        error::ProviderError,
        traits::{ChatProvider, ProviderStream},
        types::{ChatMessage, StreamChunk},
    };
    use async_trait::async_trait;
    use futures_util::stream;

    struct MockProvider;

    #[async_trait]
    impl ChatProvider for MockProvider {
        fn id(&self) -> &str { "mock" }
        fn name(&self) -> &str { "Mock" }
        fn default_model(&self) -> &str { "mock-model" }
        fn context_limit(&self) -> u32 { 8192 }
        async fn complete(&self, _: &[ChatMessage], _: Option<&str>) -> Result<ProviderStream, ProviderError> {
            Ok(Box::pin(stream::iter(vec![
                Ok(StreamChunk::delta("Hello")),
                Ok(StreamChunk::delta(" world")),
                Ok(StreamChunk::done()),
            ])))
        }
    }

    let state = test_state_with_providers(vec![Arc::new(MockProvider)]);
    let tokens = do_setup(Arc::clone(&state)).await;

    // POST /chat — consume the full SSE body so the DB write completes.
    let resp = authed_post_json(
        app(Arc::clone(&state)),
        "/api/chat",
        &tokens.access_token,
        json!({ "message": "Hi!" }),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8_lossy(&body_bytes);

    // SSE body should contain chunk events and a done event.
    assert!(body.contains("event: chunk"), "expected chunk events");
    assert!(body.contains("event: done"), "expected done event");
    assert!(body.contains("Hello"), "expected token content");

    // Conversation must now appear in the list.
    let resp = authed_get(app(Arc::clone(&state)), "/api/conversations", &tokens.access_token).await;
    let convs: Vec<ConversationSummary> = json_body(resp.into_body()).await;
    assert_eq!(convs.len(), 1, "expected one conversation");
    assert_eq!(convs[0].message_count, 2, "expected user + assistant messages");

    // Messages endpoint should return both messages.
    let conv_id = &convs[0].id;
    let resp = authed_get(
        app(Arc::clone(&state)),
        &format!("/api/conversations/{conv_id}/messages"),
        &tokens.access_token,
    )
    .await;
    let msgs: Vec<MessageSummary> = json_body(resp.into_body()).await;
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].content, "Hello world");
}

#[tokio::test]
async fn conversation_continues_with_existing_id() {
    use helpcore_server::providers::{
        error::ProviderError, traits::{ChatProvider, ProviderStream},
        types::{ChatMessage, StreamChunk},
    };
    use async_trait::async_trait;
    use futures_util::stream;

    struct MockProvider;
    #[async_trait]
    impl ChatProvider for MockProvider {
        fn id(&self) -> &str { "mock" }
        fn name(&self) -> &str { "Mock" }
        fn default_model(&self) -> &str { "mock-model" }
        fn context_limit(&self) -> u32 { 8192 }
        async fn complete(&self, _: &[ChatMessage], _: Option<&str>) -> Result<ProviderStream, ProviderError> {
            Ok(Box::pin(stream::iter(vec![Ok(StreamChunk::delta("reply")), Ok(StreamChunk::done())])))
        }
    }

    let state = test_state_with_providers(vec![Arc::new(MockProvider)]);
    let tokens = do_setup(Arc::clone(&state)).await;

    // First message — creates conversation.
    let resp = authed_post_json(app(Arc::clone(&state)), "/api/chat", &tokens.access_token, json!({ "message": "first" })).await;
    to_bytes(resp.into_body(), usize::MAX).await.unwrap(); // drain

    let resp = authed_get(app(Arc::clone(&state)), "/api/conversations", &tokens.access_token).await;
    let convs: Vec<ConversationSummary> = json_body(resp.into_body()).await;
    let conv_id = convs[0].id.clone();

    // Second message — continues same conversation.
    let resp = authed_post_json(
        app(Arc::clone(&state)), "/api/chat", &tokens.access_token,
        json!({ "message": "second", "conversation_id": conv_id }),
    ).await;
    to_bytes(resp.into_body(), usize::MAX).await.unwrap(); // drain

    let resp = authed_get(
        app(Arc::clone(&state)),
        &format!("/api/conversations/{conv_id}/messages"),
        &tokens.access_token,
    ).await;
    let msgs: Vec<MessageSummary> = json_body(resp.into_body()).await;
    assert_eq!(msgs.len(), 4, "should have 2 user + 2 assistant messages");
    assert_eq!(msgs[0].content, "first");
    assert_eq!(msgs[2].content, "second");
}
