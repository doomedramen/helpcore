use axum::{
    Router,
    body::Body,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use std::sync::Arc;

use crate::{api::handlers, state::AppState};

pub fn create(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/health", get(handlers::health))
        // Setup
        .route("/setup", get(handlers::setup::status).post(handlers::setup::create_admin))
        // Auth
        .route("/auth/login", post(handlers::auth::login))
        .route("/auth/refresh", post(handlers::auth::refresh))
        .route("/auth/logout", post(handlers::auth::logout))
        // Chat
        .route("/chat", post(handlers::chat::chat))
        // Conversations
        .route("/conversations", get(handlers::chat::list_conversations))
        .route("/conversations/{id}/messages", get(handlers::chat::get_messages))
        .route("/conversations/{id}", delete(handlers::chat::delete_conversation))
        .route("/conversations/{id}/compact", post(handlers::chat::compact_conversation))
        // Personality
        .route(
            "/personality/{name}",
            get(handlers::memory::get_personality).put(handlers::memory::put_personality),
        )
        // Memory
        .route("/memory", get(handlers::memory::list_memory))
        .route(
            "/memory/{*path}",
            get(handlers::memory::get_memory)
                .put(handlers::memory::put_memory)
                .delete(handlers::memory::delete_memory),
        )
        // Plugins
        .route("/plugins", get(handlers::plugin::list_plugins))
        .route("/plugins/{id}/enable", put(handlers::plugin::set_enabled))
        .route("/plugins/{id}/tokens", post(handlers::plugin::create_token))
        .route(
            "/plugins/{id}/tokens/{token_id}",
            delete(handlers::plugin::revoke_token),
        )
        .with_state(state);

    Router::new().nest("/api", api).fallback(serve_web_ui)
}

// Serve the Next.js static export from HELPCORE_WEB_DIR.
// Tries: exact path → path/index.html → root index.html (SPA fallback).
// Returns 404 if the web dir doesn't exist (dev mode without a built UI).
async fn serve_web_ui(uri: Uri) -> Response {
    let web_dir = std::env::var("HELPCORE_WEB_DIR")
        .unwrap_or_else(|_| "/usr/local/share/helpcore/web".to_string());

    let rel = uri.path().trim_start_matches('/');
    let base = std::path::Path::new(&web_dir);

    // 1. Exact file match
    let candidate = base.join(rel);
    if let Ok(bytes) = tokio::fs::read(&candidate).await {
        let mime = mime_for_path(&candidate);
        let cache = if rel.starts_with("_next/static/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        return Response::builder()
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, cache)
            .body(Body::from(bytes))
            .unwrap();
    }

    // 2. Directory index (e.g. /chat/ → /chat/index.html)
    let index = candidate.join("index.html");
    if let Ok(bytes) = tokio::fs::read(&index).await {
        return html_response(bytes);
    }

    // 3. SPA fallback — all unmatched routes get the root index.html
    match tokio::fs::read(base.join("index.html")).await {
        Ok(bytes) => html_response(bytes),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn html_response(bytes: Vec<u8>) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(bytes))
        .unwrap()
}

fn mime_for_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
