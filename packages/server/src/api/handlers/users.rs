use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use std::sync::Arc;

use helpcore_api::{
    AdminCreateUserRequest, AdminListUsersResponse, AdminResetPasswordRequest,
    AdminUpdateUserRequest, AdminUserSummary, ListProviderGrantsResponse, ProviderGrantInfo,
    SetProviderGrantsRequest,
};

use crate::{
    api::{error::AppError, extractor::AdminUser},
    config::ProviderRole,
    model::user::UserStatus,
    state::AppState,
};

pub async fn list_users(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
) -> Result<Json<AdminListUsersResponse>, AppError> {
    let users = state
        .db
        .call(crate::model::user::list_users)
        .await?
        .into_iter()
        .map(|u| AdminUserSummary {
            id: u.id,
            email: u.email,
            display_name: u.display_name,
            role: u.role.as_str().to_string(),
            status: u.status.as_str().to_string(),
            created_at: u.created_at,
        })
        .collect();

    Ok(Json(AdminListUsersResponse { users }))
}

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    admin: AdminUser,
    Json(req): Json<AdminCreateUserRequest>,
) -> Result<(StatusCode, Json<AdminUserSummary>), AppError> {
    let email = req.email.trim().to_lowercase();
    if email.is_empty() {
        return Err(AppError::BadRequest("email is required".into()));
    }
    if req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }

    let email_check = email.clone();
    let exists = state
        .db
        .call(move |conn| crate::model::user::email_exists(conn, &email_check))
        .await?;
    if exists {
        return Err(AppError::Conflict("email already in use".into()));
    }

    let hash = crate::auth::password::hash_password(&req.password)?;
    let admin_id = admin.0.id.clone();
    let dn = req.display_name.clone();
    let email2 = email.clone();

    let user_id = state
        .db
        .call(move |conn| {
            crate::model::user::create_member(conn, &email2, &hash, dn.as_deref(), &admin_id)
        })
        .await?;

    // Seed default personality files for the new member.
    let uid_personality = user_id.clone();
    state
        .db
        .call(move |conn| {
            crate::conversation::memory::seed_default_personality(conn, &uid_personality)
        })
        .await?;

    // Log the creation event
    let actor_id = admin.0.id.clone();
    let target_id = user_id.clone();
    let payload = serde_json::json!({ "email": email });
    state
        .db
        .call(move |conn| {
            crate::db::audit::log_event(
                conn,
                "user.create",
                Some(&actor_id),
                Some(&target_id),
                payload,
            )
        })
        .await?;

    // Re-read the user to get created_at
    let uid = user_id.clone();
    let user = state
        .db
        .call(move |conn| {
            conn.query_row(
                "SELECT id, email, display_name, role, status, created_at FROM users WHERE id = ?1",
                [&uid],
                |row| {
                    Ok(AdminUserSummary {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        display_name: row.get(2)?,
                        role: row.get(3)?,
                        status: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )
            .map_err(anyhow::Error::from)
        })
        .await?;

    Ok((StatusCode::CREATED, Json(user)))
}

pub async fn update_user(
    State(state): State<Arc<AppState>>,
    admin: AdminUser,
    Path(user_id): Path<String>,
    Json(req): Json<AdminUpdateUserRequest>,
) -> Result<StatusCode, AppError> {
    let Some(status_str) = &req.status else {
        return Ok(StatusCode::NO_CONTENT);
    };

    let new_status = match status_str.as_str() {
        "active" => UserStatus::Active,
        "deactivated" => UserStatus::Deactivated,
        other => return Err(AppError::BadRequest(format!("invalid status: {other}"))),
    };

    // Prevent an admin from deactivating themselves
    if user_id == admin.0.id && matches!(new_status, UserStatus::Deactivated) {
        return Err(AppError::BadRequest(
            "cannot deactivate your own account".into(),
        ));
    }

    let uid = user_id.clone();
    let changed = state
        .db
        .call(move |conn| crate::model::user::set_status(conn, &uid, new_status))
        .await?;

    if !changed {
        return Err(AppError::NotFound("user not found".into()));
    }

    let actor_id = admin.0.id.clone();
    let event = format!("user.{}", status_str);
    let payload = serde_json::json!({});
    state
        .db
        .call(move |conn| {
            crate::db::audit::log_event(conn, &event, Some(&actor_id), Some(&user_id), payload)
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn reset_password(
    State(state): State<Arc<AppState>>,
    admin: AdminUser,
    Path(user_id): Path<String>,
    Json(req): Json<AdminResetPasswordRequest>,
) -> Result<StatusCode, AppError> {
    if req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }

    let hash = crate::auth::password::hash_password(&req.password)?;
    let uid = user_id.clone();
    state
        .db
        .call(move |conn| crate::model::user::set_password(conn, &uid, &hash))
        .await?;

    let actor_id = admin.0.id.clone();
    let payload = serde_json::json!({});
    state
        .db
        .call(move |conn| {
            crate::db::audit::log_event(
                conn,
                "user.password_reset",
                Some(&actor_id),
                Some(&user_id),
                payload,
            )
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Provider grants ────────────────────────────────────────────────────────────

pub async fn list_provider_grants(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
    Path(user_id): Path<String>,
) -> Result<Json<ListProviderGrantsResponse>, AppError> {
    let uid = user_id.clone();
    let grants = state
        .db
        .call(move |conn| crate::model::provider_grant::list_grants(conn, &uid))
        .await?;

    let all_provider_ids: Vec<String> = state
        .providers
        .list_for_role(ProviderRole::Chat)
        .into_iter()
        .map(|d| d.id)
        .collect();

    let granted_ids: std::collections::HashSet<_> =
        grants.iter().map(|g| g.provider_id.clone()).collect();

    let ungrated_providers = all_provider_ids
        .iter()
        .filter(|id| !granted_ids.contains(*id))
        .cloned()
        .collect();

    let grants = grants
        .into_iter()
        .map(|g| ProviderGrantInfo {
            provider_id: g.provider_id,
            enabled: g.enabled,
        })
        .collect();

    Ok(Json(ListProviderGrantsResponse {
        grants,
        ungrated_providers,
    }))
}

pub async fn set_provider_grants(
    State(state): State<Arc<AppState>>,
    admin: AdminUser,
    Path(user_id): Path<String>,
    Json(req): Json<SetProviderGrantsRequest>,
) -> Result<StatusCode, AppError> {
    let actor_id = admin.0.id.clone();
    let uid = user_id.clone();

    state
        .db
        .call(move |conn| {
            for grant in &req.grants {
                crate::model::provider_grant::set_grant(
                    conn,
                    &uid,
                    &grant.provider_id,
                    grant.enabled,
                    &actor_id,
                )?;
            }
            Ok(())
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
