use std::sync::Arc;

use axum::{Json, extract::State};
use helpcore_api::{ProviderInfo, ProviderListResponse};

use crate::{
    api::{error::AppError, extractor::AuthUser},
    config::ProviderRole,
    state::AppState,
};

pub async fn list_providers(
    State(state): State<Arc<AppState>>,
    _auth_user: AuthUser,
) -> Result<Json<ProviderListResponse>, AppError> {
    let providers = state
        .providers
        .list_for_role(ProviderRole::Chat)
        .into_iter()
        .map(|provider| ProviderInfo {
            id: provider.id,
            name: provider.name,
            default_model: provider.default_model,
        })
        .collect();
    Ok(Json(ProviderListResponse { providers }))
}
