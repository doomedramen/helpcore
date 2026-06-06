use axum::{
    Router,
    routing::{delete, get, patch, post, put},
};
use std::{path::Path, sync::Arc};
use tower_http::services::{ServeDir, ServeFile};

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
        .route(
            "/auth/me",
            get(handlers::auth::current_user).patch(handlers::auth::update_me),
        )
        .route("/auth/me/password", post(handlers::auth::change_password))
        // API keys
        .route(
            "/auth/api-keys",
            get(handlers::api_key::list_api_keys).post(handlers::api_key::create_api_key),
        )
        .route("/auth/api-keys/{id}", delete(handlers::api_key::revoke_api_key))
        // Admin
        .route(
            "/admin/config",
            get(handlers::admin::get_config).put(handlers::admin::update_config),
        )
        .route(
            "/admin/users",
            get(handlers::users::list_users).post(handlers::users::create_user),
        )
        .route(
            "/admin/users/{id}",
            patch(handlers::users::update_user),
        )
        .route(
            "/admin/users/{id}/password-reset",
            post(handlers::users::reset_password),
        )
        .route(
            "/admin/users/{id}/providers",
            get(handlers::users::list_provider_grants).put(handlers::users::set_provider_grants),
        )
        // Chat
        .route("/chat", post(handlers::chat::chat))
        .route("/providers", get(handlers::provider::list_providers))
        // Conversations
        .route("/conversations", get(handlers::chat::list_conversations))
        .route("/conversations/{id}/messages", get(handlers::chat::get_messages))
        .route(
            "/conversations/{id}/messages/{message_id}/retry",
            post(handlers::chat::retry_message),
        )
        .route("/conversations/{id}", delete(handlers::chat::delete_conversation))
        .route("/conversations/{id}/compact", post(handlers::chat::compact_conversation))
        .route("/conversations/{id}/cancel", post(handlers::chat::cancel_conversation))
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
        // TTS
        .route("/tts", post(handlers::tts::tts_handler))
        // Plugins
        .route("/plugins", get(handlers::plugin::list_plugins))
        .route("/plugins/store", get(handlers::plugin::list_store))
        .route("/plugins/{id}/install", post(handlers::plugin::install_plugin))
        .route("/plugins/{id}/update", post(handlers::plugin::update_plugin))
        .route("/plugins/{id}/rollback", post(handlers::plugin::rollback_plugin))
        .route("/plugins/{id}", delete(handlers::plugin::uninstall_plugin))
        .route("/plugins/{id}/config", put(handlers::plugin::configure_plugin))
        .route("/plugins/{id}/enable", put(handlers::plugin::set_enabled))
        .route("/plugins/{id}/tokens", post(handlers::plugin::create_token))
        .route(
            "/plugins/{id}/tokens/{token_id}",
            delete(handlers::plugin::revoke_token),
        )
        .with_state(state);

    let mut router = Router::new().nest("/api", api);

    // Serve the web UI from HELPCORE_WEB_DIR when it exists.
    // Falls back to index.html for any path not matched by a static file,
    // enabling client-side routing inside the Next.js SPA.
    let web_dir = std::env::var("HELPCORE_WEB_DIR")
        .unwrap_or_else(|_| "/usr/local/share/helpcore/web".to_string());

    if Path::new(&web_dir).is_dir() {
        router = router.fallback_service(
            ServeDir::new(&web_dir)
                .not_found_service(ServeFile::new(format!("{web_dir}/index.html"))),
        );
    }

    router
}
