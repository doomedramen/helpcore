//! API key management handlers — create, list, and revoke personal API keys.
//!
//! All endpoints under `/api/auth/api-keys`.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use std::sync::Arc;

use helpcore_api::{ApiKeyInfo, CreateApiKeyRequest, CreateApiKeyResponse, ListApiKeysResponse};

use crate::{
    api::{error::AppError, extractor::AuthUser},
    state::AppState,
};

/// GET /api/auth/api-keys — lists the authenticated user's active API keys.
pub async fn list_api_keys(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<Json<ListApiKeysResponse>, AppError> {
    let user_id = auth_user.id;
    let keys = state
        .db
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, key_prefix, created_at, last_used_at, expires_at
                 FROM api_keys
                 WHERE user_id = ?1 AND revoked_at IS NULL
                 ORDER BY created_at DESC",
            )?;
            let rows = stmt
                .query_map([&user_id], |row| {
                    Ok(ApiKeyInfo {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        key_prefix: row.get(2)?,
                        created_at: row.get(3)?,
                        last_used_at: row.get(4)?,
                        expires_at: row.get(5)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;

    Ok(Json(ListApiKeysResponse { keys }))
}

/// POST /api/auth/api-keys — creates a new API key for the authenticated user.
///
/// Returns the full key only once; the caller must store it.
pub async fn create_api_key(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<CreateApiKeyResponse>), AppError> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    let expires_at = if let Some(s) = &req.expires_at {
        Some(
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|d| d.with_timezone(&chrono::Utc))
                .map_err(|_| {
                    AppError::BadRequest("invalid expires_at: expected RFC-3339".into())
                })?,
        )
    } else {
        None
    };

    let user_id = auth_user.id;
    let uid_audit = user_id.clone();
    let created = state
        .db
        .call(move |conn| {
            let key =
                crate::auth::api_key::create_api_key(conn, &user_id, &name, None, expires_at)?;
            crate::db::audit::log_event(
                conn,
                "api_key.create",
                Some(&user_id),
                Some(&key.id),
                serde_json::json!({ "name": name }),
            )?;
            Ok(key)
        })
        .await?;
    let _ = uid_audit; // already captured above

    Ok((
        StatusCode::CREATED,
        Json(CreateApiKeyResponse {
            id: created.id,
            name: req.name,
            key_prefix: created.key_prefix,
            key: created.full_key,
        }),
    ))
}

/// DELETE /api/auth/api-keys/{id} — revokes an API key by ID.
pub async fn revoke_api_key(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(key_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let user_id = auth_user.id;
    let kid = key_id.clone();
    let revoked = state
        .db
        .call(move |conn| {
            let did_revoke = crate::auth::api_key::revoke_api_key(conn, &key_id, &user_id)?;
            if did_revoke {
                crate::db::audit::log_event(
                    conn,
                    "api_key.revoke",
                    Some(&user_id),
                    Some(&key_id),
                    serde_json::json!({}),
                )?;
            }
            Ok(did_revoke)
        })
        .await?;
    let _ = kid;

    if revoked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound("api key not found".into()))
    }
}
