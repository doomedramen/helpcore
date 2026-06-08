use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use reqwest::header::{AUTHORIZATION, HeaderName, HeaderValue};
use semver::Version;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use helpcore_api::{
    PluginConfigureRequest, PluginEnableRequest, PluginInfo, PluginInstallRequest,
    PluginListResponse, PluginStoreItem, PluginStoreResponse, PluginTokenRequest,
    PluginTokenResponse,
};

use crate::{
    api::{error::AppError, extractor::AuthUser},
    config::Config,
    plugins::{
        package::{self, InstalledPackage},
        registry::{self, Manifest, PluginTools, StorePlugin},
        runtime::is_builtin_tool,
        secrets, token as plugin_token,
    },
    state::AppState,
};

pub async fn list_plugins(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<Json<PluginListResponse>, AppError> {
    let config = load_config(&state).await?;
    let blacklist = config.plugins.blacklist.into_iter().collect::<HashSet<_>>();
    let available = registry::fetch_store(&config.registry.url)
        .await
        .map(|store| {
            store
                .plugins
                .into_iter()
                .map(|plugin| (plugin.id.clone(), plugin))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    let uid = auth_user.id;
    let installed = state
        .db
        .call(move |conn| registry::list_enabled(conn, &uid))
        .await?;

    let mut capabilities = Vec::new();
    for p in &installed {
        if p.enabled {
            for feature in &p.manifest.provides {
                if !capabilities.contains(feature) {
                    capabilities.push(feature.clone());
                }
            }
        }
    }

    let plugins = installed
        .into_iter()
        .map(|plugin| {
            let available_version = available
                .get(&plugin.plugin_id)
                .map(|item| item.version.clone());
            let update_available = available_version
                .as_deref()
                .is_some_and(|version| is_newer(version, &plugin.version));
            let configured = registry::is_configured(
                &plugin.manifest.config_schema,
                &plugin.tier,
                &plugin.config,
            );
            let config_schema = plugin.manifest.config_schema.clone();
            let config_values = registry::config_values(&config_schema, &plugin.config);
            PluginInfo {
                blocked: blacklist.contains(&plugin.plugin_id),
                id: plugin.plugin_id,
                name: plugin.name,
                description: plugin.description,
                active_version: plugin.version,
                previous_version: plugin.previous_version,
                available_version,
                tier: plugin.tier,
                permissions: plugin.permissions,
                provides: plugin.manifest.provides.clone(),
                enabled: plugin.enabled,
                configured,
                update_available,
                user_managed: plugin.source_url.is_some(),
                config_schema,
                config_values,
            }
        })
        .collect();

    Ok(Json(PluginListResponse {
        plugins,
        capabilities,
    }))
}

pub async fn list_store(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<Json<PluginStoreResponse>, AppError> {
    let config = load_config(&state).await?;
    let registry_url = config.registry.url.clone();
    let store = registry::fetch_store(&registry_url)
        .await
        .map_err(|error| AppError::Upstream(error.to_string()))?;

    let uid = auth_user.id;
    let installed = state
        .db
        .call(move |conn| registry::installed_states(conn, &uid))
        .await?;
    let blacklist = config.plugins.blacklist.into_iter().collect::<HashSet<_>>();

    let mut plugins = store
        .plugins
        .into_iter()
        .map(|plugin| {
            let state = installed.get(&plugin.id);
            let blocked = blacklist.contains(&plugin.id);
            PluginStoreItem {
                installable: plugin.package.is_some() && !blocked,
                installed: state.is_some(),
                enabled: state.is_some_and(|item| item.enabled),
                blocked,
                active_version: state.map(|item| item.version.clone()),
                previous_version: state.and_then(|item| item.previous_version.clone()),
                configured: state.is_some_and(|item| item.configured),
                update_available: state
                    .is_some_and(|item| is_newer(&plugin.version, &item.version)),
                id: plugin.id,
                name: plugin.name,
                description: plugin.description,
                version: plugin.version,
                tier: plugin.tier,
                author: plugin.author,
                homepage: plugin.homepage,
                setup_guide: plugin.setup_guide,
                permissions: plugin.permissions,
                provides: plugin.provides,
            }
        })
        .collect::<Vec<_>>();
    plugins.sort_by_key(|left| left.name.to_lowercase());

    Ok(Json(PluginStoreResponse {
        registry_url,
        plugins,
    }))
}

pub async fn install_plugin(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(plugin_id): Path<String>,
    Json(request): Json<PluginInstallRequest>,
) -> Result<StatusCode, AppError> {
    let config = load_config(&state).await?;
    ensure_not_blocked(&config, &plugin_id)?;
    let plugin = fetch_store_plugin(&config, &plugin_id).await?;
    let permissions = approved_permissions(&plugin, request.permissions)?;

    let uid = auth_user.id.clone();
    let pid = plugin_id.clone();
    let exists = state
        .db
        .call(move |conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM plugin_installs WHERE user_id = ?1 AND plugin_id = ?2",
                rusqlite::params![uid, pid],
                |row| row.get::<_, i64>(0),
            )? > 0)
        })
        .await?;
    if exists {
        return Err(AppError::Conflict("plugin is already installed".into()));
    }

    let package = package::install_store_package(&state.data_dir, &auth_user.id, &plugin)
        .await
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    if let Err(error) =
        persist_install(&state, &auth_user.id, &plugin, &package, &permissions).await
    {
        let _ = std::fs::remove_dir_all(&package.version_path);
        return Err(error);
    }
    Ok(StatusCode::CREATED)
}

pub async fn update_plugin(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(plugin_id): Path<String>,
    Json(request): Json<PluginInstallRequest>,
) -> Result<StatusCode, AppError> {
    ensure_user_managed(&state, &auth_user.id, &plugin_id).await?;
    let config = load_config(&state).await?;
    ensure_not_blocked(&config, &plugin_id)?;
    let plugin = fetch_store_plugin(&config, &plugin_id).await?;
    let permissions = approved_permissions(&plugin, request.permissions)?;

    let uid = auth_user.id.clone();
    let pid = plugin_id.clone();
    let current: Option<(String, Option<String>)> = state
        .db
        .call(move |conn| {
            match conn.query_row(
                "SELECT version, previous_version FROM plugin_installs
                 WHERE user_id = ?1 AND plugin_id = ?2",
                rusqlite::params![uid, pid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ) {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
        .await?;
    let Some((current_version, older_previous)) = current else {
        return Err(AppError::NotFound("plugin not installed".into()));
    };
    if !is_newer(&plugin.version, &current_version) {
        return Err(AppError::Conflict(
            "no newer plugin version is available".into(),
        ));
    }

    let version_path = package::plugin_root(&state.data_dir, &auth_user.id, &plugin_id)
        .join("versions")
        .join(&plugin.version);
    if version_path.exists() {
        let _ = std::fs::remove_dir_all(&version_path);
    }

    let package = package::install_store_package(&state.data_dir, &auth_user.id, &plugin)
        .await
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    let manifest_json = serde_json::to_string(&package.manifest)?;
    let tools_json = serde_json::to_string(&package.tools)?;
    let permissions_json = serde_json::to_string(&permissions)?;
    let uid = auth_user.id.clone();
    let pid = plugin_id.clone();
    let version = package.manifest.version.clone();
    let skill = package.skill_md.clone();
    let previous = current_version.clone();
    let update_result = state
        .db
        .call(move |conn| {
            registry::upsert_plugin(conn, &package.manifest)?;
            conn.execute(
                "UPDATE plugin_installs
                 SET previous_version = ?1, version = ?2, permissions = ?3,
                     skill_md = ?4, manifest = ?5, tools = ?6,
                     updated_at = ?7
                 WHERE user_id = ?8 AND plugin_id = ?9",
                rusqlite::params![
                    previous,
                    version,
                    permissions_json,
                    skill,
                    manifest_json,
                    tools_json,
                    chrono::Utc::now().to_rfc3339(),
                    uid,
                    pid
                ],
            )?;
            Ok(())
        })
        .await;
    if let Err(error) = update_result {
        let _ = std::fs::remove_dir_all(&package.version_path);
        return Err(error.into());
    }

    if let Some(old) = older_previous {
        let path = package::plugin_root(&state.data_dir, &auth_user.id, &plugin_id)
            .join("versions")
            .join(old);
        let _ = std::fs::remove_dir_all(path);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rollback_plugin(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(plugin_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_user_managed(&state, &auth_user.id, &plugin_id).await?;
    let config = load_config(&state).await?;
    ensure_not_blocked(&config, &plugin_id)?;
    let uid = auth_user.id.clone();
    let pid = plugin_id.clone();
    let versions: Option<(String, String)> = state
        .db
        .call(move |conn| {
            match conn.query_row(
                "SELECT version, previous_version FROM plugin_installs
                 WHERE user_id = ?1 AND plugin_id = ?2 AND previous_version IS NOT NULL",
                rusqlite::params![uid, pid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ) {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
        .await?;
    let Some((current, previous)) = versions else {
        return Err(AppError::Conflict(
            "no previous plugin version is available".into(),
        ));
    };
    let version_path = package::plugin_root(&state.data_dir, &auth_user.id, &plugin_id)
        .join("versions")
        .join(&previous);
    let restored = package::load_installed_package(&version_path)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    let manifest_json = serde_json::to_string(&restored.manifest)?;
    let tools_json = serde_json::to_string(&restored.tools)?;
    let uid = auth_user.id;
    let pid = plugin_id;
    state
        .db
        .call(move |conn| {
            conn.execute(
                "UPDATE plugin_installs
                 SET version = ?1, previous_version = ?2, skill_md = ?3,
                     manifest = ?4, tools = ?5, updated_at = ?6
                 WHERE user_id = ?7 AND plugin_id = ?8",
                rusqlite::params![
                    previous,
                    current,
                    restored.skill_md,
                    manifest_json,
                    tools_json,
                    chrono::Utc::now().to_rfc3339(),
                    uid,
                    pid
                ],
            )?;
            Ok(())
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn uninstall_plugin(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(plugin_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_user_managed(&state, &auth_user.id, &plugin_id).await?;
    let root = package::plugin_root(&state.data_dir, &auth_user.id, &plugin_id);
    let trash = root.with_file_name(format!(".uninstall-{}-{}", plugin_id, uuid::Uuid::new_v4()));
    if root.exists() {
        std::fs::rename(&root, &trash).map_err(|error| AppError::Internal(error.into()))?;
    }

    let uid = auth_user.id;
    let pid = plugin_id;
    let deleted = state
        .db
        .call(move |conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute(
                "DELETE FROM plugin_tokens WHERE user_id = ?1 AND plugin_id = ?2",
                rusqlite::params![uid, pid],
            )?;
            let deleted = tx.execute(
                "DELETE FROM plugin_installs WHERE user_id = ?1 AND plugin_id = ?2",
                rusqlite::params![uid, pid],
            )?;
            tx.commit()?;
            Ok(deleted > 0)
        })
        .await;
    match deleted {
        Ok(true) => {
            let _ = std::fs::remove_dir_all(trash);
            Ok(StatusCode::NO_CONTENT)
        }
        Ok(false) => {
            if trash.exists() {
                let _ = std::fs::rename(trash, root);
            }
            Err(AppError::NotFound("plugin not installed".into()))
        }
        Err(error) => {
            if trash.exists() {
                let _ = std::fs::rename(trash, root);
            }
            Err(error.into())
        }
    }
}

pub async fn configure_plugin(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(plugin_id): Path<String>,
    Json(request): Json<PluginConfigureRequest>,
) -> Result<StatusCode, AppError> {
    let uid = auth_user.id.clone();
    let pid = plugin_id.clone();
    let install: Option<(Manifest, serde_json::Value, Option<String>)> = state
        .db
        .call(move |conn| {
            match conn.query_row(
                "SELECT manifest, config, secrets FROM plugin_installs
                 WHERE user_id = ?1 AND plugin_id = ?2",
                rusqlite::params![uid, pid],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            ) {
                Ok((manifest_raw, config_raw, secrets)) => {
                    let manifest: Manifest = serde_json::from_str(&manifest_raw)?;
                    let config: serde_json::Value =
                        serde_json::from_str(&config_raw).unwrap_or_default();
                    Ok(Some((manifest, config, secrets)))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
        .await?;
    let Some((manifest, existing_config, existing_secrets)) = install else {
        return Err(AppError::NotFound("plugin not installed".into()));
    };

    let values = request
        .values
        .as_object()
        .ok_or_else(|| AppError::BadRequest("values must be a JSON object".into()))?;

    // Split values by field type based on the declared schema.
    let mut config_map = serde_json::Map::new();
    let mut new_secret_map = serde_json::Map::new();

    for field in &manifest.config_schema {
        if let Some(value) = values.get(&field.key) {
            if field.field_type == "secret" {
                if let Some(s) = value.as_str() {
                    if !s.is_empty() {
                        new_secret_map.insert(field.key.clone(), value.clone());
                    }
                    // empty string = clear this secret (omit from map)
                }
                // null = keep existing (handled below)
            } else {
                config_map.insert(field.key.clone(), value.clone());
            }
        }
    }

    // Determine updated secret_keys and secrets blob.
    // If any new secret values were submitted, treat as a full secrets replacement.
    // Otherwise, preserve the existing secrets and _secret_keys unchanged.
    let has_new_secrets = !new_secret_map.is_empty();
    let (secrets_json, secret_keys): (Option<String>, Vec<serde_json::Value>) = if has_new_secrets {
        let keys = new_secret_map
            .keys()
            .map(|k| serde_json::Value::String(k.clone()))
            .collect();
        let value = serde_json::Value::Object(new_secret_map.clone());
        let encrypted = tokio::task::spawn_blocking({
            let data_dir = state.data_dir.clone();
            let uid = auth_user.id.clone();
            let pid = plugin_id.clone();
            move || secrets::encrypt(&data_dir, &uid, &pid, &value)
        })
        .await
        .map_err(|error| AppError::Internal(anyhow::anyhow!("plugin secret task failed: {error}")))?
        .map_err(AppError::Internal)?;
        (Some(encrypted), keys)
    } else {
        let keys = existing_config
            .get("_secret_keys")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        (existing_secrets, keys)
    };

    // Store the configured secret key names alongside the config (never the values).
    config_map.insert("_secret_keys".into(), serde_json::Value::Array(secret_keys));

    // Bridge health check using schema-driven endpoint lookup.
    if manifest.tier == "bridge" {
        let config_so_far = serde_json::Value::Object(config_map.clone());
        let endpoint = registry::bridge_endpoint(&manifest.config_schema, &config_so_far)
            .ok_or_else(|| AppError::BadRequest("bridge endpoint is required".into()))?;
        let effective_secrets = if has_new_secrets {
            Some(serde_json::Value::Object(new_secret_map))
        } else if let Some(encrypted) = secrets_json.as_deref() {
            Some(
                decrypt_plugin_secrets(
                    state.data_dir.clone(),
                    auth_user.id.clone(),
                    plugin_id.clone(),
                    encrypted.to_string(),
                )
                .await?,
            )
        } else {
            None
        };
        check_bridge_health(endpoint, &manifest, effective_secrets.as_ref()).await?;
    }

    let config_json = serde_json::to_string(&serde_json::Value::Object(config_map))?;
    let uid = auth_user.id;
    let pid = plugin_id;
    state
        .db
        .call(move |conn| {
            conn.execute(
                "UPDATE plugin_installs
                 SET config = ?1, secrets = ?2, updated_at = ?3
                 WHERE user_id = ?4 AND plugin_id = ?5",
                rusqlite::params![
                    config_json,
                    secrets_json,
                    chrono::Utc::now().to_rfc3339(),
                    uid,
                    pid
                ],
            )?;
            Ok(())
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn set_enabled(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(plugin_id): Path<String>,
    Json(request): Json<PluginEnableRequest>,
) -> Result<StatusCode, AppError> {
    ensure_user_managed(&state, &auth_user.id, &plugin_id).await?;
    let config = load_config(&state).await?;
    if request.enabled {
        ensure_not_blocked(&config, &plugin_id)?;
    }

    let uid = auth_user.id.clone();
    let pid = plugin_id.clone();
    let install: Option<(String, String, String, Option<String>)> = state
        .db
        .call(move |conn| {
            match conn.query_row(
                "SELECT manifest, config, tools, secrets FROM plugin_installs
                 WHERE user_id = ?1 AND plugin_id = ?2",
                rusqlite::params![uid, pid],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            ) {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
        .await?;
    let Some((manifest_raw, config_raw, tools_raw, encrypted_secrets)) = install else {
        return Err(AppError::NotFound("plugin not installed".into()));
    };
    let manifest: Manifest = serde_json::from_str(&manifest_raw)?;
    let plugin_config: serde_json::Value = serde_json::from_str(&config_raw)?;

    if request.enabled
        && !registry::is_configured(&manifest.config_schema, &manifest.tier, &plugin_config)
    {
        return Err(AppError::Conflict(
            "complete configuration before enabling this plugin".into(),
        ));
    }

    if request.enabled && manifest.tier == "bridge" {
        let endpoint = registry::bridge_endpoint(&manifest.config_schema, &plugin_config)
            .ok_or_else(|| {
                AppError::Conflict("configure the bridge endpoint before enabling".into())
            })?;
        let effective_secrets = if let Some(encrypted) = encrypted_secrets {
            Some(
                decrypt_plugin_secrets(
                    state.data_dir.clone(),
                    auth_user.id.clone(),
                    plugin_id.clone(),
                    encrypted,
                )
                .await?,
            )
        } else {
            None
        };
        check_bridge_health(endpoint, &manifest, effective_secrets.as_ref()).await?;
    }
    if request.enabled {
        ensure_no_tool_conflicts(&state, &auth_user.id, &plugin_id, &tools_raw).await?;
    }

    let uid = auth_user.id;
    let pid = plugin_id;
    let updated = state
        .db
        .call(move |conn| {
            Ok(conn.execute(
                "UPDATE plugin_installs SET enabled = ?1, updated_at = ?2
                 WHERE user_id = ?3 AND plugin_id = ?4",
                rusqlite::params![
                    request.enabled as i32,
                    chrono::Utc::now().to_rfc3339(),
                    uid,
                    pid
                ],
            )? > 0)
        })
        .await?;
    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound("plugin not installed".into()))
    }
}

pub async fn create_token(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(plugin_id): Path<String>,
    Json(request): Json<PluginTokenRequest>,
) -> Result<Json<PluginTokenResponse>, AppError> {
    ensure_user_managed(&state, &auth_user.id, &plugin_id).await?;
    let uid = auth_user.id.clone();
    let pid = plugin_id.clone();
    let installed = state
        .db
        .call(move |conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM plugin_installs WHERE user_id = ?1 AND plugin_id = ?2",
                rusqlite::params![uid, pid],
                |row| row.get::<_, i64>(0),
            )? > 0)
        })
        .await?;
    if !installed {
        return Err(AppError::NotFound("plugin not installed".into()));
    }

    let uid = auth_user.id;
    let pid = plugin_id;
    let permissions = request.permissions;
    let token = state
        .db
        .call(move |conn| plugin_token::create_plugin_token(conn, &uid, &pid, &permissions))
        .await?;
    Ok(Json(PluginTokenResponse {
        token_id: token.token_id,
        token: token.raw,
    }))
}

pub async fn revoke_token(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path((plugin_id, token_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    ensure_user_managed(&state, &auth_user.id, &plugin_id).await?;
    let uid = auth_user.id;
    let revoked = state
        .db
        .call(move |conn| plugin_token::revoke_plugin_token(conn, &token_id, &uid))
        .await?;
    if revoked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound("token not found".into()))
    }
}

async fn persist_install(
    state: &AppState,
    user_id: &str,
    plugin: &StorePlugin,
    package: &InstalledPackage,
    permissions: &[String],
) -> Result<(), AppError> {
    let manifest = package.manifest.clone();
    let manifest_json = serde_json::to_string(&manifest)?;
    let tools_json = serde_json::to_string(&package.tools)?;
    let permissions_json = serde_json::to_string(permissions)?;
    let skill = package.skill_md.clone();
    let uid = user_id.to_string();
    let source_url = plugin.package.as_ref().map(|item| item.url.clone());
    state
        .db
        .call(move |conn| {
            registry::upsert_plugin(conn, &manifest)?;
            if let Some(source_url) = source_url {
                conn.execute(
                    "UPDATE plugins SET source_url = ?1 WHERE id = ?2",
                    rusqlite::params![source_url, manifest.id],
                )?;
            }
            conn.execute(
                "INSERT INTO plugin_installs
                 (id, user_id, plugin_id, version, permissions, enabled, config,
                  skill_md, manifest, tools, installed_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, '{}', ?6, ?7, ?8, ?9, ?9)",
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    uid,
                    manifest.id,
                    manifest.version,
                    permissions_json,
                    skill,
                    manifest_json,
                    tools_json,
                    chrono::Utc::now().to_rfc3339()
                ],
            )?;
            Ok(())
        })
        .await?;
    Ok(())
}

async fn ensure_user_managed(
    state: &AppState,
    user_id: &str,
    plugin_id: &str,
) -> Result<(), AppError> {
    let uid = user_id.to_string();
    let pid = plugin_id.to_string();
    let source_url: Option<Option<String>> = state
        .db
        .call(move |conn| {
            match conn.query_row(
                "SELECT p.source_url
                 FROM plugin_installs pi
                 JOIN plugins p ON p.id = pi.plugin_id
                 WHERE pi.user_id = ?1 AND pi.plugin_id = ?2",
                rusqlite::params![uid, pid],
                |row| row.get(0),
            ) {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
        .await?;
    match source_url {
        Some(Some(_)) => Ok(()),
        Some(None) => Err(AppError::Forbidden),
        None => Err(AppError::NotFound("plugin not installed".into())),
    }
}

async fn load_config(state: &AppState) -> Result<Config, AppError> {
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
    .map_err(|error| AppError::Internal(anyhow::anyhow!("config load task failed: {error}")))?
    .map_err(AppError::Internal)
}

async fn fetch_store_plugin(config: &Config, plugin_id: &str) -> Result<StorePlugin, AppError> {
    registry::fetch_store(&config.registry.url)
        .await
        .map_err(|error| AppError::Upstream(error.to_string()))?
        .plugins
        .into_iter()
        .find(|plugin| plugin.id == plugin_id)
        .ok_or(AppError::NotFound("plugin not found in store".into()))
}

fn ensure_not_blocked(config: &Config, plugin_id: &str) -> Result<(), AppError> {
    if config
        .plugins
        .blacklist
        .iter()
        .any(|blocked| blocked == plugin_id)
    {
        Err(AppError::Forbidden)
    } else {
        Ok(())
    }
}

fn approved_permissions(
    plugin: &StorePlugin,
    requested: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let declared = plugin.permissions.iter().collect::<HashSet<_>>();
    let mut approved = requested;
    approved.sort();
    approved.dedup();
    if approved
        .iter()
        .any(|permission| !declared.contains(permission))
    {
        return Err(AppError::BadRequest(
            "approved permissions must be declared by the plugin".into(),
        ));
    }
    Ok(approved)
}

fn is_newer(available: &str, active: &str) -> bool {
    match (Version::parse(available), Version::parse(active)) {
        (Ok(available), Ok(active)) => available > active,
        _ => available != active,
    }
}

async fn check_bridge_health(
    endpoint: &str,
    manifest: &Manifest,
    credentials: Option<&serde_json::Value>,
) -> Result<(), AppError> {
    let base = reqwest::Url::parse(endpoint)
        .map_err(|error| AppError::BadRequest(format!("invalid bridge endpoint: {error}")))?;
    if !matches!(base.scheme(), "http" | "https") {
        return Err(AppError::BadRequest(
            "bridge endpoint must use http or https".into(),
        ));
    }
    let host = base
        .host_str()
        .ok_or_else(|| AppError::BadRequest("bridge endpoint must include a host".into()))?;
    if !manifest.allowed_hosts.iter().any(|allowed| {
        allowed == host
            || allowed
                .strip_prefix("*.")
                .is_some_and(|suffix| host.ends_with(&format!(".{suffix}")))
    }) {
        return Err(AppError::BadRequest(
            "bridge endpoint host is not declared in the plugin manifest".into(),
        ));
    }
    let health_path = manifest
        .bridge
        .as_ref()
        .and_then(|item| item.health_path.as_deref())
        .unwrap_or("/health");
    let url = base
        .join(health_path.trim_start_matches('/'))
        .map_err(|error| AppError::BadRequest(format!("invalid bridge health path: {error}")))?;
    let health_host = url
        .host_str()
        .ok_or_else(|| AppError::BadRequest("bridge health URL must include a host".into()))?;
    if !manifest.allowed_hosts.iter().any(|allowed| {
        allowed == health_host
            || allowed
                .strip_prefix("*.")
                .is_some_and(|suffix| health_host.ends_with(&format!(".{suffix}")))
    }) {
        return Err(AppError::BadRequest(
            "bridge health URL host is not declared in the plugin manifest".into(),
        ));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|error| AppError::Internal(error.into()))?;
    apply_bridge_credentials(client.get(url), credentials)?
        .send()
        .await
        .map_err(|error| AppError::Upstream(format!("bridge health check failed: {error}")))?
        .error_for_status()
        .map_err(|error| AppError::Upstream(format!("bridge health check failed: {error}")))?;
    Ok(())
}

fn apply_bridge_credentials(
    mut request: reqwest::RequestBuilder,
    credentials: Option<&serde_json::Value>,
) -> Result<reqwest::RequestBuilder, AppError> {
    let Some(credentials) = credentials else {
        return Ok(request);
    };
    if let Some(token) = credentials.get("token").and_then(serde_json::Value::as_str) {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(headers) = credentials
        .get("headers")
        .and_then(serde_json::Value::as_object)
    {
        for (name, value) in headers {
            let value = value.as_str().ok_or_else(|| {
                AppError::BadRequest("bridge secret headers must contain string values".into())
            })?;
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
                AppError::BadRequest(format!("invalid bridge header name: {error}"))
            })?;
            let value = HeaderValue::from_str(value).map_err(|error| {
                AppError::BadRequest(format!("invalid bridge header value: {error}"))
            })?;
            request = request.header(name, value);
        }
    }
    Ok(request)
}

async fn decrypt_plugin_secrets(
    data_dir: std::path::PathBuf,
    user_id: String,
    plugin_id: String,
    encrypted: String,
) -> Result<serde_json::Value, AppError> {
    tokio::task::spawn_blocking(move || {
        secrets::decrypt(&data_dir, &user_id, &plugin_id, &encrypted)
    })
    .await
    .map_err(|error| AppError::Internal(anyhow::anyhow!("plugin secret task failed: {error}")))?
    .map_err(AppError::Internal)
}

async fn ensure_no_tool_conflicts(
    state: &AppState,
    user_id: &str,
    plugin_id: &str,
    tools_raw: &str,
) -> Result<(), AppError> {
    let target: PluginTools = serde_json::from_str(tools_raw)?;
    let target_names = target
        .tools
        .into_iter()
        .map(|tool| tool.name)
        .collect::<HashSet<_>>();
    if let Some(builtin) = target_names.iter().find(|name| is_builtin_tool(name)) {
        return Err(AppError::Conflict(format!(
            "plugin tool name conflicts with a built-in tool: {builtin}"
        )));
    }
    let uid = user_id.to_string();
    let pid = plugin_id.to_string();
    let conflicts = state
        .db
        .call(move |conn| {
            let mut statement = conn.prepare(
                "SELECT tools FROM plugin_installs
                 WHERE user_id = ?1 AND plugin_id != ?2 AND enabled = 1",
            )?;
            let rows =
                statement.query_map(rusqlite::params![uid, pid], |row| row.get::<_, String>(0))?;
            let mut conflicts = Vec::new();
            for row in rows {
                let tools: PluginTools = serde_json::from_str(&row?).unwrap_or_default();
                for tool in tools.tools {
                    if target_names.contains(&tool.name) {
                        conflicts.push(tool.name);
                    }
                }
            }
            Ok(conflicts)
        })
        .await?;
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(AppError::Conflict(format!(
            "plugin tool names conflict with enabled plugins: {}",
            conflicts.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_credentials_are_applied_to_health_requests() {
        let request = reqwest::Client::new().get("https://bridge.example/health");
        let credentials = serde_json::json!({
            "token": "secret-token",
            "headers": {
                "x-plugin-key": "secret-key"
            }
        });
        let request = apply_bridge_credentials(request, Some(&credentials))
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(
            request.headers().get(AUTHORIZATION).unwrap(),
            "Bearer secret-token"
        );
        assert_eq!(request.headers().get("x-plugin-key").unwrap(), "secret-key");
    }

    #[test]
    fn bridge_credentials_reject_non_string_headers() {
        let request = reqwest::Client::new().get("https://bridge.example/health");
        let credentials = serde_json::json!({
            "headers": {
                "x-plugin-key": 42
            }
        });

        assert!(apply_bridge_credentials(request, Some(&credentials)).is_err());
    }
}
