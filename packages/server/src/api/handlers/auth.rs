use axum::{Json, extract::State, http::StatusCode};
use std::sync::Arc;

use helpcore_api::{LoginRequest, LoginResponse, LogoutRequest, RefreshRequest, RefreshResponse};

use crate::{api::error::AppError, state::AppState};

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
    let session = state
        .db
        .call(move |conn| crate::auth::token::create_session(conn, &user_id))
        .await?;

    Ok(Json(LoginResponse {
        access_token: session.access_token,
        refresh_token: session.refresh_token,
        token_type: "Bearer".to_string(),
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
