use axum::{
    Router,
    routing::{delete, get, post, put},
};
use std::sync::Arc;

use crate::{api::handlers, state::AppState};

pub fn create(state: Arc<AppState>) -> Router {
    Router::new()
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
        .with_state(state)
}
