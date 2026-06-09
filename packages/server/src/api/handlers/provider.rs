//! Provider listing handler — returns available AI providers for the authenticated user.
//!
//! Endpoint: GET /api/providers

use std::sync::Arc;

use axum::{Json, extract::State};
use helpcore_api::{ProviderInfo, ProviderListResponse};

use crate::{
    api::{error::AppError, extractor::AuthUser},
    config::ProviderRole,
    model::user::UserRole,
    state::AppState,
};

/// GET /api/providers — lists available chat providers, filtered by provider grants for non-admin users.
pub async fn list_providers(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<Json<ProviderListResponse>, AppError> {
    let all = state.providers.list_for_role(ProviderRole::Chat);

    // Admins always see everything; members see only non-denied providers.
    let providers = if auth_user.role == UserRole::Admin {
        all
    } else {
        let user_id = auth_user.id.clone();
        let denied = state
            .db
            .call(move |conn| crate::model::provider_grant::denied_providers(conn, &user_id))
            .await?;
        all.into_iter()
            .filter(|p| !denied.contains(&p.id))
            .collect()
    };

    Ok(Json(ProviderListResponse {
        providers: providers
            .into_iter()
            .map(|provider| ProviderInfo {
                id: provider.id,
                name: provider.name,
                default_model: provider.default_model,
                context_limit: provider.context_limit,
            })
            .collect(),
    }))
}
