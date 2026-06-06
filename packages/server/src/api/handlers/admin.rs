use axum::{extract::State, Json};
use helpcore_api::{
    AdminConfigResponse, AdminConfigUpdateRequest, AdminProviderConfig, AdminServerConfig,
};
use std::{collections::HashSet, sync::Arc};

use crate::{
    api::{error::AppError, extractor::AdminUser},
    config::{Config, ProviderConfig, ProviderRole, ProviderType},
    state::AppState,
};

pub async fn get_config(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
) -> Result<Json<AdminConfigResponse>, AppError> {
    let config = load_persisted_config(&state).await?;
    Ok(Json(config_response(&state, &config)))
}

pub async fn update_config(
    State(state): State<Arc<AppState>>,
    _admin: AdminUser,
    Json(req): Json<AdminConfigUpdateRequest>,
) -> Result<Json<AdminConfigResponse>, AppError> {
    let _update_guard = state.config_update_lock.lock().await;
    let current = load_persisted_config(&state).await?;
    let mut config = current.clone();

    config.server.name = req.server.name.trim().to_string();
    config.server.url = req.server.url.trim().trim_end_matches('/').to_string();
    config.server.port = req.server.port;
    config.logging.level = req.logging_level.trim().to_lowercase();
    config.registry.url = req.registry_url.trim().to_string();

    let mut blacklist = req
        .plugin_blacklist
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    blacklist.sort();
    blacklist.dedup();
    config.plugins.blacklist = blacklist;

    let existing = current
        .providers
        .iter()
        .map(|provider| (provider.id.as_str(), provider))
        .collect::<std::collections::HashMap<_, _>>();

    let mut provider_ids = HashSet::new();
    let mut providers = Vec::with_capacity(req.providers.len());
    for provider in req.providers {
        let id = provider.id.trim().to_string();
        if !provider_ids.insert(id.clone()) {
            return Err(AppError::BadRequest("provider IDs must be unique".into()));
        }

        let provider_type = ProviderType::try_from(provider.provider_type.as_str())
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        if provider_type != ProviderType::Ollama {
            return Err(AppError::BadRequest(
                "only Ollama providers are currently supported".into(),
            ));
        }
        let roles = provider
            .roles
            .iter()
            .map(|role| {
                ProviderRole::try_from(role.as_str())
                    .map_err(|e| AppError::BadRequest(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let api_key = if provider.clear_api_key {
            None
        } else if let Some(key) = provider.api_key.filter(|key| !key.trim().is_empty()) {
            Some(key)
        } else {
            existing.get(id.as_str()).and_then(|item| item.api_key.clone())
        };

        let url = provider
            .url
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty());
        if url.is_none() {
            return Err(AppError::BadRequest(format!(
                "provider {id} base URL is required"
            )));
        }
        let parsed_url = reqwest::Url::parse(url.as_deref().unwrap())
            .map_err(|_| AppError::BadRequest(format!("provider {id} base URL is invalid")))?;
        if !matches!(parsed_url.scheme(), "http" | "https") {
            return Err(AppError::BadRequest(format!(
                "provider {id} base URL must use http or https"
            )));
        }

        providers.push(ProviderConfig {
            id,
            name: provider.name.trim().to_string(),
            provider_type,
            api_key,
            url,
            default_model: provider.default_model.trim().to_string(),
            roles,
            num_ctx: provider.num_ctx,
            num_predict: provider.num_predict,
        });
    }
    config.providers = providers;

    config
        .validate()
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let prepared = crate::providers::registry::ProviderRegistry::prepare(&config.providers)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    Config::writability(&state.config_path).map_err(AppError::Conflict)?;

    let path = state.config_path.clone();
    let saved = config.clone();
    tokio::task::spawn_blocking(move || saved.save(&path))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("config save task failed: {e}")))??;

    state.providers.replace(prepared);

    let blacklist = config.plugins.blacklist.clone();
    state
        .db
        .call(move |conn| {
            let transaction = conn.unchecked_transaction()?;
            for plugin_id in blacklist {
                transaction.execute(
                    "UPDATE plugin_installs
                     SET enabled = 0, updated_at = ?1
                     WHERE plugin_id = ?2",
                    rusqlite::params![chrono::Utc::now().to_rfc3339(), plugin_id],
                )?;
            }
            transaction.commit()?;
            Ok(())
        })
        .await?;

    Ok(Json(config_response(&state, &config)))
}

async fn load_persisted_config(state: &AppState) -> Result<Config, AppError> {
    let path = state.config_path.clone();
    let fallback = state.config.as_ref().clone();
    tokio::task::spawn_blocking(move || {
        if path.is_file() {
            Config::load(&path)
        } else {
            Ok(fallback)
        }
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("config load task failed: {e}")))?
    .map_err(AppError::Internal)
}

fn config_response(
    state: &AppState,
    config: &Config,
) -> AdminConfigResponse {
    let writability = Config::writability(&state.config_path);
    AdminConfigResponse {
        config_path: state.config_path.display().to_string(),
        config_writable: writability.is_ok(),
        config_writability_error: writability.err(),
        server: AdminServerConfig {
            name: config.server.name.clone(),
            url: config.server.url.clone(),
            port: config.server.port,
        },
        logging_level: config.logging.level.clone(),
        registry_url: config.registry.url.clone(),
        plugin_blacklist: config.plugins.blacklist.clone(),
        providers: config
            .providers
            .iter()
            .map(|provider| AdminProviderConfig {
                id: provider.id.clone(),
                name: provider.name.clone(),
                provider_type: provider.provider_type.as_str().to_string(),
                api_key_configured: provider.api_key.is_some(),
                url: provider.url.clone(),
                default_model: provider.default_model.clone(),
                roles: provider
                    .roles
                    .iter()
                    .map(|role| role.as_str().to_string())
                    .collect(),
                num_ctx: provider.num_ctx,
                num_predict: provider.num_predict,
            })
            .collect(),
        restart_required: config.server.port != state.config.server.port
            || config.logging.level != state.config.logging.level,
    }
}
