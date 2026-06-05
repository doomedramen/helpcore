use axum::{
    Router,
    routing::{delete, get, post},
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
        .with_state(state)
}
