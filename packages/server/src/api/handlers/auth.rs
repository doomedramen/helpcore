//! Authentication handlers — login, logout, token refresh, and current-user profile.
//!
//! All endpoints under `/api/auth`.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use helpcore_api::{
    ChangePasswordRequest, CurrentUserResponse, LoginRequest, LoginResponse, LogoutRequest,
    RefreshRequest, RefreshResponse, UpdateMeRequest,
};

use crate::{
    api::{error::AppError, extractor::AuthUser},
    state::AppState,
};

pub(super) fn set_refresh_cookie(token: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "helpcore_refresh={token}; HttpOnly; SameSite=Strict; Path=/api/auth; Max-Age=2592000"
    ))
    .expect("cookie value is always valid ASCII")
}

const CLEAR_REFRESH_COOKIE: &str =
    "helpcore_refresh=; HttpOnly; SameSite=Strict; Path=/api/auth; Max-Age=0";

fn cookie_refresh_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split("; ")
                .find(|c| c.starts_with("helpcore_refresh="))
                .map(|c| c["helpcore_refresh=".len()..].to_string())
        })
}

const VALID_MEMORY_LEANING: &[&str] = &["off", "light", "moderate", "heavy"];

fn validate_memory_leaning(value: &str) -> Result<(), AppError> {
    if VALID_MEMORY_LEANING.contains(&value) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "invalid memory_leaning value: {value}. Must be one of: {}",
            VALID_MEMORY_LEANING.join(", ")
        )))
    }
}

/// GET /api/auth/me — returns the authenticated user's profile.
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
        memory_leaning: user.memory_leaning,
    }))
}

/// PATCH /api/auth/me — updates the authenticated user's display name, timezone, and memory_leaning.
pub async fn update_me(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Json(req): Json<UpdateMeRequest>,
) -> Result<StatusCode, AppError> {
    if let Some(tz) = &req.timezone {
        tz.parse::<chrono_tz::Tz>()
            .map_err(|_| AppError::BadRequest(format!("unknown timezone: {tz}")))?;
    }

    if let Some(ml) = &req.memory_leaning {
        validate_memory_leaning(ml)?;
    }

    let user_id = auth_user.id;
    let dn = req.display_name.clone();
    let tz = req.timezone.clone();
    let ml = req.memory_leaning.clone();
    state
        .db
        .call(move |conn| {
            crate::model::user::update_me(
                conn,
                &user_id,
                dn.as_deref(),
                tz.as_deref(),
                ml.as_deref(),
            )
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/auth/me/password — changes the authenticated user's password.
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
        return Err(AppError::BadRequest(
            "new password must be at least 8 characters".into(),
        ));
    }

    let new_hash = crate::auth::password::hash_password(&req.new_password)?;
    let user_id = auth_user.id;
    state
        .db
        .call(move |conn| crate::model::user::set_password(conn, &user_id, &new_hash))
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/auth/login — authenticates a user with email/password and returns access/refresh tokens.
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, AppError> {
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

    let mut resp = Json(LoginResponse {
        access_token: session.access_token,
        refresh_token: session.refresh_token.clone(),
        token_type: "Bearer".to_string(),
        force_password_change: user.force_password_change,
    })
    .into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        set_refresh_cookie(&session.refresh_token),
    );
    Ok(resp)
}

/// POST /api/auth/refresh — exchanges a refresh token for a new access/refresh token pair.
///
/// Accepts the token from the `helpcore_refresh` HttpOnly cookie (web) or from the
/// JSON request body (CLI / programmatic clients).
pub async fn refresh(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Response, AppError> {
    let token = cookie_refresh_token(&headers)
        .or(req.refresh_token)
        .ok_or(AppError::Unauthorized)?;

    let result = state
        .db
        .call(move |conn| crate::auth::token::rotate_refresh_token(conn, &token))
        .await?
        .ok_or(AppError::Unauthorized)?;

    let (_user_id, session) = result;

    let mut resp = Json(RefreshResponse {
        access_token: session.access_token,
        refresh_token: session.refresh_token.clone(),
        token_type: "Bearer".to_string(),
    })
    .into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        set_refresh_cookie(&session.refresh_token),
    );
    Ok(resp)
}

/// POST /api/auth/logout — revokes a refresh token session.
///
/// Accepts the token from the `helpcore_refresh` HttpOnly cookie (web) or from the
/// JSON request body (CLI / programmatic clients). Always clears the cookie.
pub async fn logout(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(req): Json<LogoutRequest>,
) -> Result<Response, AppError> {
    if let Some(token) = cookie_refresh_token(&headers).or(req.refresh_token) {
        state
            .db
            .call(move |conn| crate::auth::token::revoke_session_by_token(conn, &token))
            .await?;
    }

    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(CLEAR_REFRESH_COOKIE),
    );
    Ok(resp)
}
