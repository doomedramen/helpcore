use axum::{Json, extract::State, http::StatusCode};
use std::sync::Arc;

use helpcore_api::{LoginResponse, SetupRequest, SetupStatusResponse};

use crate::{api::error::AppError, state::AppState};

pub async fn status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SetupStatusResponse>, AppError> {
    let needed = state
        .db
        .call(|conn| crate::auth::setup::needs_setup(conn))
        .await?;

    Ok(Json(SetupStatusResponse { setup_required: needed }))
}

pub async fn create_admin(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetupRequest>,
) -> Result<(StatusCode, Json<LoginResponse>), AppError> {
    // Validate and consume the setup token atomically.
    let token = req.token.clone();
    let valid = state
        .db
        .call(move |conn| crate::auth::setup::consume_setup_token(conn, &token))
        .await?;

    if !valid {
        return Err(AppError::BadRequest(
            "Invalid or expired setup token".to_string(),
        ));
    }

    // Validate email format (basic check — a real validation can come later).
    let email = req.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest("Invalid email address".to_string()));
    }

    if req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".to_string(),
        ));
    }

    let password_hash = crate::auth::password::hash_password(&req.password)?;
    let display_name = req.display_name.clone();

    let user_id = state
        .db
        .call(move |conn| {
            crate::model::user::create_admin(
                conn,
                &email,
                &password_hash,
                display_name.as_deref(),
            )
        })
        .await?;

    // Local plugins are loaded before first-run setup, when there may be no
    // users yet. Run the loader again so the newly created admin receives them.
    let local_plugins = state.config.plugins.local.clone();
    state
        .db
        .call(move |conn| crate::plugins::registry::load_local_plugins(conn, &local_plugins))
        .await?;

    let session = state
        .db
        .call(move |conn| crate::auth::token::create_session(conn, &user_id))
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(LoginResponse {
            access_token: session.access_token,
            refresh_token: session.refresh_token,
            token_type: "Bearer".to_string(),
        }),
    ))
}
