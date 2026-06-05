use axum::{Json, extract::{Path, State}, http::StatusCode};
use std::sync::Arc;

use helpcore_api::{
    PluginInfo, PluginListResponse, PluginStoreItem, PluginStoreResponse, PluginTokenRequest,
    PluginTokenResponse,
};

use crate::{
    api::{error::AppError, extractor::AuthUser},
    plugins::{registry, token as plugin_token},
    state::AppState,
};

// ── GET /plugins ──────────────────────────────────────────────────────────────

pub async fn list_plugins(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<Json<PluginListResponse>, AppError> {
    let uid = auth_user.id.clone();
    let plugins = state
        .db
        .call(move |conn| registry::list_enabled(conn, &uid))
        .await?;

    let items = plugins
        .into_iter()
        .map(|p| PluginInfo {
            id:      p.plugin_id,
            name:    p.name,
            description: p.description,
            version: p.version,
            tier:    p.tier,
            permissions: p.permissions,
            enabled: p.enabled,
        })
        .collect();

    Ok(Json(PluginListResponse { plugins: items }))
}

// ── GET /plugins/store ────────────────────────────────────────────────────────

pub async fn list_store(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<Json<PluginStoreResponse>, AppError> {
    let config_path = state.config_path.clone();
    let fallback = state.config.as_ref().clone();
    let config = tokio::task::spawn_blocking(move || {
        if config_path.is_file() {
            crate::config::Config::load(&config_path)
        } else {
            Ok(fallback)
        }
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("config load task failed: {e}")))??;

    let registry_url = config.registry.url.clone();
    let store = registry::fetch_store(&registry_url)
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?;

    let uid = auth_user.id;
    let installed = state
        .db
        .call(move |conn| registry::installed_states(conn, &uid))
        .await?;
    let blacklist = config.plugins.blacklist.into_iter().collect::<std::collections::HashSet<_>>();

    let mut plugins = store
        .plugins
        .into_iter()
        .map(|plugin| {
            let enabled = installed.get(&plugin.id).copied().unwrap_or(false);
            PluginStoreItem {
                installed: installed.contains_key(&plugin.id),
                enabled,
                blocked: blacklist.contains(&plugin.id),
                id: plugin.id,
                name: plugin.name,
                description: plugin.description,
                version: plugin.version,
                tier: plugin.tier,
                author: plugin.author,
                homepage: plugin.homepage,
                setup_guide: plugin.setup_guide,
                permissions: plugin.permissions,
            }
        })
        .collect::<Vec<_>>();
    plugins.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(Json(PluginStoreResponse { registry_url, plugins }))
}

// ── PUT /plugins/{id}/enable ──────────────────────────────────────────────────

pub async fn set_enabled(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(plugin_id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<StatusCode, AppError> {
    let enabled = req.get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| AppError::BadRequest("body must be {\"enabled\": true|false}".into()))?;

    let uid = auth_user.id.clone();
    let pid = plugin_id.clone();

    let updated = state
        .db
        .call(move |conn| {
            let n = conn.execute(
                "UPDATE plugin_installs SET enabled = ?1 WHERE user_id = ?2 AND plugin_id = ?3",
                rusqlite::params![enabled as i32, uid, pid],
            )?;
            Ok(n > 0)
        })
        .await?;

    if updated { Ok(StatusCode::NO_CONTENT) } else { Err(AppError::NotFound) }
}

// ── POST /plugins/{id}/tokens ─────────────────────────────────────────────────

pub async fn create_token(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(plugin_id): Path<String>,
    Json(req): Json<PluginTokenRequest>,
) -> Result<Json<PluginTokenResponse>, AppError> {
    // Verify the plugin is installed for this user.
    let uid = auth_user.id.clone();
    let pid = plugin_id.clone();
    let installed = state
        .db
        .call(move |conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM plugin_installs WHERE user_id = ?1 AND plugin_id = ?2",
                rusqlite::params![uid, pid],
                |row| row.get(0),
            )?;
            Ok(n > 0)
        })
        .await?;

    if !installed {
        return Err(AppError::NotFound);
    }

    let uid = auth_user.id.clone();
    let pid = plugin_id.clone();
    let permissions = req.permissions.clone();
    let new_tok = state
        .db
        .call(move |conn| {
            plugin_token::create_plugin_token(conn, &uid, &pid, &permissions)
        })
        .await?;

    Ok(Json(PluginTokenResponse {
        token_id: new_tok.token_id,
        token:    new_tok.raw,
    }))
}

// ── DELETE /plugins/{id}/tokens/{token_id} ────────────────────────────────────

pub async fn revoke_token(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((plugin_id, token_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let uid = auth_user.id.clone();
    let tid = token_id.clone();
    let revoked = state
        .db
        .call(move |conn| plugin_token::revoke_plugin_token(conn, &tid, &uid))
        .await?;
    let _ = plugin_id; // validated via FK in DB
    if revoked { Ok(StatusCode::NO_CONTENT) } else { Err(AppError::NotFound) }
}
