use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use std::sync::Arc;

use crate::{api::error::AppError, model::user::UserRole, state::AppState};

/// Injected into handlers that require an authenticated user.
pub struct AuthUser {
    pub id: String,
    pub role: UserRole,
}

/// Injected into handlers restricted to server administrators.
pub struct AdminUser(pub AuthUser);

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, AppError> {
        // Bearer token takes precedence over API key.
        if let Some(user) = try_bearer(parts, state).await? {
            return Ok(user);
        }
        if let Some(user) = try_api_key(parts, state).await? {
            return Ok(user);
        }
        Err(AppError::Unauthorized)
    }
}

impl FromRequestParts<Arc<AppState>> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, AppError> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if user.role != UserRole::Admin {
            return Err(AppError::Forbidden);
        }
        Ok(Self(user))
    }
}

async fn try_bearer(parts: &Parts, state: &Arc<AppState>) -> Result<Option<AuthUser>, AppError> {
    let token = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_owned());

    let Some(token) = token else { return Ok(None) };

    let t = token.clone();
    let user_id = state
        .db
        .call(move |conn| crate::auth::token::validate_access_token(conn, &t))
        .await
        .map_err(AppError::Internal)?;

    let Some(uid) = user_id else { return Ok(None) };

    let user = state
        .db
        .call(move |conn| crate::model::user::find_by_id(conn, &uid))
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    Ok(Some(AuthUser {
        id: user.id,
        role: user.role,
    }))
}

async fn try_api_key(parts: &Parts, state: &Arc<AppState>) -> Result<Option<AuthUser>, AppError> {
    let key = parts
        .headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    let Some(key) = key else { return Ok(None) };

    let k = key.clone();
    let user_id = state
        .db
        .call(move |conn| crate::auth::api_key::validate_api_key(conn, &k))
        .await
        .map_err(AppError::Internal)?;

    let Some(uid) = user_id else { return Ok(None) };

    let user = state
        .db
        .call(move |conn| crate::model::user::find_by_id(conn, &uid))
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    Ok(Some(AuthUser {
        id: user.id,
        role: user.role,
    }))
}
