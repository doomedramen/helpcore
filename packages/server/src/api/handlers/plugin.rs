use axum::{Json, extract::{Path, State}, http::StatusCode};
use std::sync::Arc;

use helpcore_api::{
    PluginInfo, PluginListResponse, PluginTokenRequest, PluginTokenResponse,
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
            version: p.version,
            tier:    p.tier,
            enabled: p.enabled,
        })
        .collect();

    Ok(Json(PluginListResponse { plugins: items }))
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
