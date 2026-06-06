use axum::{Json, extract::State, http::StatusCode};
use std::sync::Arc;

use helpcore_api::{
    ChangePasswordRequest, CurrentUserResponse, LoginRequest, LoginResponse, LogoutRequest,
    RefreshRequest, RefreshResponse, UpdateMeRequest,
};

use crate::{api::{error::AppError, extractor::AuthUser}, state::AppState};

pub async fn current_user(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<Json<CurrentUserResponse>, AppError> {
    let user_id = auth_user.id;
    let user = state
        .db
        .call(move |conn| crate::model::user::find_by_id(conn, &user_id))
        .await?
        .ok_or(AppError::Unauthorized)?;

    Ok(Json(CurrentUserResponse {
        id: user.id,
        email: user.email,
        display_name: user.display_name,
        role: user.role.as_str().to_string(),
        timezone: user.timezone,
    }))
}

pub async fn update_me(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Json(req): Json<UpdateMeRequest>,
) -> Result<StatusCode, AppError> {
    if let Some(tz) = &req.timezone {
        tz.parse::<chrono_tz::Tz>()
            .map_err(|_| AppError::BadRequest(format!("unknown timezone: {tz}")))?;
    }

    let user_id = auth_user.id;
    let dn = req.display_name.clone();
    let tz = req.timezone.clone();
    state
        .db
        .call(move |conn| {
            crate::model::user::update_me(
                conn,
                &user_id,
                dn.as_deref(),
                tz.as_deref(),
            )
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_password(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    let user_id = auth_user.id.clone();
    let user = state
        .db
        .call(move |conn| crate::model::user::find_by_id(conn, &user_id))
        .await?
        .ok_or(AppError::Unauthorized)?;

    if !crate::auth::password::verify_password(&req.current_password, &user.password_hash)? {
        return Err(AppError::BadRequest("current password is incorrect".into()));
    }

    if req.new_password.len() < 8 {
        return Err(AppError::BadRequest("new password must be at least 8 characters".into()));
    }

    let new_hash = crate::auth::password::hash_password(&req.new_password)?;
    let user_id = auth_user.id;
    state
        .db
        .call(move |conn| crate::model::user::set_password(conn, &user_id, &new_hash))
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    let email = req.email.trim().to_lowercase();

    // Find user — same generic error regardless of whether email exists or password is wrong.
    let user = state
        .db
        .call(move |conn| crate::model::user::find_by_email(conn, &email))
        .await?
        .ok_or(AppError::Unauthorized)?;

    if !crate::auth::password::verify_password(&req.password, &user.password_hash)? {
        return Err(AppError::Unauthorized);
    }

    let user_id = user.id.clone();
    let uid_audit = user_id.clone();
    state
        .db
        .call(move |conn| {
            crate::db::audit::log_event(
                conn,
                "auth.login",
                Some(&uid_audit),
                Some(&uid_audit),
                serde_json::json!({}),
            )
        })
        .await?;

    let session = state
        .db
        .call(move |conn| crate::auth::token::create_session(conn, &user_id))
        .await?;

    Ok(Json(LoginResponse {
        access_token: session.access_token,
        refresh_token: session.refresh_token,
        token_type: "Bearer".to_string(),
        force_password_change: user.force_password_change,
    }))
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<RefreshResponse>, AppError> {
    let token = req.refresh_token.clone();

    let result = state
        .db
        .call(move |conn| crate::auth::token::rotate_refresh_token(conn, &token))
        .await?
        .ok_or(AppError::Unauthorized)?;

    let (_user_id, session) = result;

    Ok(Json(RefreshResponse {
        access_token: session.access_token,
        refresh_token: session.refresh_token,
        token_type: "Bearer".to_string(),
    }))
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LogoutRequest>,
) -> Result<StatusCode, AppError> {
    let token = req.refresh_token.clone();

    state
        .db
        .call(move |conn| crate::auth::token::revoke_session_by_token(conn, &token))
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
