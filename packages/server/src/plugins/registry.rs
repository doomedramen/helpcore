/// Plugin registry: loading manifests from disk, registering plugins in the DB,
/// and querying installed plugins for a user.

use anyhow::Context;
use chrono::Utc;
use helpcore_api::ConfigField;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path, time::Duration};
use uuid::Uuid;

use crate::config::LocalPluginConfig;

// ── Manifest types ─────────────────────────────────────────────────────────────

/// Parsed content of a plugin's `manifest.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub tier: String,           // "wasm" | "bridge"
    #[serde(default)]
    pub permissions: Vec<String>,
    pub min_core_version: Option<String>,
    pub bridge: Option<BridgeConfig>,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// User-configurable fields declared by the plugin.
    #[serde(default)]
    pub config_schema: Vec<ConfigField>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BridgeConfig {
    pub default_port: Option<u16>,
    pub health_path: Option<String>,
}

/// Runtime representation of an installed plugin for a user.
#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    pub install_id: String,
    pub plugin_id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub previous_version: Option<String>,
    pub tier: String,
    pub enabled: bool,
    pub skill_md: Option<String>,
    pub permissions: Vec<String>,
    pub manifest: Manifest,
    pub tools: PluginTools,
    pub config: serde_json::Value,
    pub secrets: Option<String>,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InstalledState {
    pub enabled: bool,
    pub version: String,
    pub previous_version: Option<String>,
    pub configured: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StoreRegistry {
    pub plugins: Vec<StorePlugin>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorePlugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub tier: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub source: Option<StorePluginSource>,
    pub setup_guide: Option<String>,
    pub package: Option<StorePluginPackage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorePluginPackage {
    pub url: String,
    pub sha256: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorePluginSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub repo: Option<String>,
    #[serde(rename = "ref")]
    pub source_ref: Option<String>,
    pub wasm_asset: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PluginTools {
    #[serde(default)]
    pub tools: Vec<PluginTool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

// ── Config schema helpers ──────────────────────────────────────────────────────

/// Returns the value of the field with `role = "bridge_endpoint"` from the stored config,
/// or falls back to the legacy `"endpoint"` key for plugins without a schema.
pub fn bridge_endpoint<'a>(schema: &[ConfigField], config: &'a serde_json::Value) -> Option<&'a str> {
    let key = schema
        .iter()
        .find(|f| f.role.as_deref() == Some("bridge_endpoint"))
        .map(|f| f.key.as_str())
        .unwrap_or("endpoint");
    config.get(key)?.as_str().filter(|s| !s.is_empty())
}

/// Returns true when all required config fields have values.
/// Falls back to the legacy endpoint-only check for plugins with no schema.
pub fn is_configured(schema: &[ConfigField], tier: &str, config: &serde_json::Value) -> bool {
    if schema.is_empty() {
        return if tier == "bridge" {
            bridge_endpoint(schema, config).is_some()
        } else {
            true
        };
    }
    let configured_secrets: std::collections::HashSet<&str> = config
        .get("_secret_keys")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    schema.iter().filter(|f| f.required).all(|f| {
        if f.field_type == "secret" {
            configured_secrets.contains(f.key.as_str())
        } else {
            match config.get(&f.key) {
                None | Some(serde_json::Value::Null) => false,
                Some(v) => v.as_str().map(|s| !s.is_empty()).unwrap_or(true),
            }
        }
    })
}

/// Builds the `config_values` map for API responses. Non-secret fields return their
/// stored value; secret fields return `{"configured": bool}`.
pub fn config_values(schema: &[ConfigField], config: &serde_json::Value) -> serde_json::Value {
    let configured_secrets: std::collections::HashSet<&str> = config
        .get("_secret_keys")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let mut map = serde_json::Map::new();
    for field in schema {
        if field.field_type == "secret" {
            map.insert(
                field.key.clone(),
                serde_json::json!({ "configured": configured_secrets.contains(field.key.as_str()) }),
            );
        } else if let Some(value) = config.get(&field.key) {
            map.insert(field.key.clone(), value.clone());
        }
    }
    serde_json::Value::Object(map)
}

// ── Manifest loading ───────────────────────────────────────────────────────────

/// Reads a plugin directory, parses `manifest.toml`, and optionally reads
/// `skill.md`. Returns `(manifest, skill_content)`.
pub fn load_manifest(plugin_dir: &Path) -> anyhow::Result<(Manifest, Option<String>)> {
    let manifest_path = plugin_dir.join("manifest.toml");
    let raw = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: Manifest = toml::from_str(&raw)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let skill_path = plugin_dir.join("skill.md");
    let skill = if skill_path.exists() {
        Some(
            std::fs::read_to_string(&skill_path)
                .with_context(|| format!("failed to read {}", skill_path.display()))?,
        )
    } else {
        None
    };

    Ok((manifest, skill))
}

// ── DB helpers ─────────────────────────────────────────────────────────────────

/// Register (or update) a plugin in the `plugins` table.
pub fn upsert_plugin(conn: &Connection, manifest: &Manifest) -> anyhow::Result<()> {
    let manifest_json = serde_json::to_string(manifest)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO plugins (id, name, version, manifest, tier, cached_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
           name      = excluded.name,
           version   = excluded.version,
           manifest  = excluded.manifest,
           tier      = excluded.tier,
           cached_at = excluded.cached_at",
        params![manifest.id, manifest.name, manifest.version, manifest_json, manifest.tier, now],
    )
    .context("failed to upsert plugin")?;
    Ok(())
}

/// Enable a plugin for a user (creates the `plugin_installs` row if needed).
pub fn ensure_installed(
    conn: &Connection,
    user_id: &str,
    manifest: &Manifest,
    skill_md: Option<&str>,
    enabled: bool,
) -> anyhow::Result<String> {
    let permissions_json = serde_json::to_string(&manifest.permissions)?;
    let now = Utc::now().to_rfc3339();

    // Check if already installed.
    let existing: Option<String> = {
        let mut stmt = conn.prepare_cached(
            "SELECT id FROM plugin_installs WHERE user_id = ?1 AND plugin_id = ?2",
        )?;
        stmt.query_row(params![user_id, manifest.id], |row| row.get(0))
            .ok()
    };

    if let Some(id) = existing {
        // Update skill and enabled state.
        conn.execute(
            "UPDATE plugin_installs
             SET skill_md = ?1, enabled = ?2, version = ?3, manifest = ?4,
                 updated_at = ?5
              WHERE user_id = ?6 AND plugin_id = ?7",
            params![
                skill_md,
                enabled as i32,
                manifest.version,
                serde_json::to_string(manifest)?,
                now,
                user_id,
                manifest.id
            ],
        )?;
        return Ok(id);
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO plugin_installs
           (id, user_id, plugin_id, version, permissions, enabled, skill_md,
            manifest, installed_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            id,
            user_id,
            manifest.id,
            manifest.version,
            permissions_json,
            enabled as i32,
            skill_md,
            serde_json::to_string(manifest)?,
            now
        ],
    )
    .context("failed to insert plugin_installs")?;
    Ok(id)
}

/// Load all enabled plugins for a user (skill fragments included).
pub fn list_enabled(conn: &Connection, user_id: &str) -> anyhow::Result<Vec<InstalledPlugin>> {
    let mut stmt = conn.prepare_cached(
        "SELECT pi.id, pi.plugin_id, p.name, COALESCE(pi.manifest, p.manifest),
                pi.version, p.tier, pi.enabled, pi.skill_md, pi.permissions,
                pi.previous_version, pi.tools, pi.config, pi.secrets, p.source_url
           FROM plugin_installs pi
           JOIN plugins p ON p.id = pi.plugin_id
          WHERE pi.user_id = ?1
          ORDER BY pi.installed_at",
    )?;
    let rows = stmt
        .query_map(params![user_id], |row| {
            let manifest_json: String = row.get(3)?;
            let manifest: Option<Manifest> = serde_json::from_str(&manifest_json).ok();
            let permissions_json: String = row.get(8)?;
            let manifest = manifest.unwrap_or_else(|| Manifest {
                id: row.get(1).unwrap_or_default(),
                name: row.get(2).unwrap_or_default(),
                version: row.get(4).unwrap_or_default(),
                description: String::new(),
                tier: row.get(5).unwrap_or_default(),
                permissions: Vec::new(),
                min_core_version: None,
                bridge: None,
                allowed_hosts: Vec::new(),
                config_schema: Vec::new(),
            });
            let tools_json: String = row.get(10)?;
            let config_json: String = row.get(11)?;
            Ok(InstalledPlugin {
                install_id: row.get(0)?,
                plugin_id:  row.get(1)?,
                name:       row.get(2)?,
                description: manifest.description.clone(),
                version:    row.get(4)?,
                previous_version: row.get(9)?,
                tier:       row.get(5)?,
                enabled:    row.get::<_, i32>(6)? != 0,
                skill_md:   row.get(7)?,
                permissions: serde_json::from_str(&permissions_json).unwrap_or_default(),
                manifest,
                tools: serde_json::from_str(&tools_json).unwrap_or_default(),
                config: serde_json::from_str(&config_json).unwrap_or_default(),
                secrets: row.get(12)?,
                source_url: row.get(13)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn installed_states(
    conn: &Connection,
    user_id: &str,
) -> anyhow::Result<HashMap<String, InstalledState>> {
    let mut stmt = conn.prepare_cached(
        "SELECT plugin_id, enabled, version, previous_version, config, manifest
         FROM plugin_installs WHERE user_id = ?1",
    )?;
    let states = stmt
        .query_map(params![user_id], |row| {
            let config: String = row.get(4)?;
            let manifest: String = row.get(5)?;
            let config: serde_json::Value = serde_json::from_str(&config).unwrap_or_default();
            let manifest: Option<Manifest> = serde_json::from_str(&manifest).ok();
            let configured = match &manifest {
                Some(m) => is_configured(&m.config_schema, &m.tier, &config),
                None => true,
            };
            Ok((
                row.get::<_, String>(0)?,
                InstalledState {
                    enabled: row.get::<_, i32>(1)? != 0,
                    version: row.get(2)?,
                    previous_version: row.get(3)?,
                    configured,
                },
            ))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(states)
}

pub async fn fetch_store(registry_url: &str) -> anyhow::Result<StoreRegistry> {
    if let Some(path) = registry_url.strip_prefix("file://") {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read plugin registry {path}"))?;
        return serde_json::from_str(&raw).context("plugin registry returned invalid JSON");
    }

    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?
        .get(registry_url)
        .send()
        .await
        .with_context(|| format!("failed to fetch plugin registry {registry_url}"))?
        .error_for_status()
        .with_context(|| format!("plugin registry {registry_url} returned an error"))?;

    response
        .json::<StoreRegistry>()
        .await
        .context("plugin registry returned invalid JSON")
}

/// Returns only the skill_md fragments for enabled plugins (used in context assembly).
pub fn enabled_skills(conn: &Connection, user_id: &str) -> anyhow::Result<Vec<String>> {
    let plugins = list_enabled(conn, user_id)?;
    Ok(plugins
        .into_iter()
        .filter(|p| p.enabled)
        .filter_map(|p| p.skill_md)
        .filter(|s| !s.trim().is_empty())
        .collect())
}

// ── Startup loader ─────────────────────────────────────────────────────────────

/// Load local plugins from config and register them for all users (or just
/// the admin).  Called once at server startup.
pub fn load_local_plugins(
    conn: &Connection,
    local_plugins: &[LocalPluginConfig],
) -> anyhow::Result<()> {
    for lp in local_plugins {
        let path = std::path::PathBuf::from(&lp.path);
        if !path.exists() {
            tracing::warn!(id = lp.id, path = lp.path, "local plugin path not found — skipping");
            continue;
        }

        let (manifest, skill) = match load_manifest(&path) {
            Ok(m) => m,
            Err(e) => {
                tracing::error!(id = lp.id, error = %e, "failed to load plugin manifest");
                continue;
            }
        };

        if manifest.id != lp.id {
            tracing::error!(
                config_id = lp.id,
                manifest_id = manifest.id,
                "plugin id in config does not match manifest — skipping"
            );
            continue;
        }

        upsert_plugin(conn, &manifest)?;

        // Install for every existing user (personal server — all users get all plugins).
        let user_ids: Vec<String> = {
            let mut stmt = conn.prepare_cached("SELECT id FROM users")?;
            stmt.query_map([], |row| row.get(0))?
                .collect::<Result<_, _>>()?
        };

        for user_id in &user_ids {
            ensure_installed(conn, user_id, &manifest, skill.as_deref(), lp.enabled)?;
        }

        tracing::info!(id = lp.id, version = manifest.version, enabled = lp.enabled, "loaded local plugin");
    }
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use std::io::Write;

    fn make_user(conn: &Connection) -> String {
        let uid = "user-plugin-test";
        conn.execute(
            "INSERT INTO users (id, email, password_hash, role, created_at, updated_at)
             VALUES (?1, 'plugin@test.com', 'hash', 'admin', '2024-01-01', '2024-01-01')",
            params![uid],
        )
        .unwrap();
        uid.to_string()
    }

    fn fake_manifest(id: &str) -> Manifest {
        Manifest {
            id: id.to_string(),
            name: "Test Plugin".to_string(),
            version: "0.1.0".to_string(),
            description: "A test plugin".to_string(),
            tier: "bridge".to_string(),
            permissions: vec!["outbound_http".to_string()],
            min_core_version: None,
            bridge: None,
            allowed_hosts: Vec::new(),
            config_schema: Vec::new(),
        }
    }

    #[test]
    fn register_and_list_plugin() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = make_user(conn);
            let m = fake_manifest("test-plugin");
            upsert_plugin(conn, &m)?;
            ensure_installed(conn, &uid, &m, Some("You have test plugin enabled."), true)?;

            let plugins = list_enabled(conn, &uid)?;
            assert_eq!(plugins.len(), 1);
            assert_eq!(plugins[0].plugin_id, "test-plugin");
            assert_eq!(plugins[0].description, "A test plugin");
            assert!(plugins[0].enabled);
            assert_eq!(plugins[0].skill_md.as_deref(), Some("You have test plugin enabled."));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn skill_fragments_for_enabled_only() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = make_user(conn);
            let m1 = fake_manifest("plugin-a");
            let m2 = fake_manifest("plugin-b");
            upsert_plugin(conn, &m1)?;
            upsert_plugin(conn, &m2)?;
            ensure_installed(conn, &uid, &m1, Some("skill a"), true)?;
            ensure_installed(conn, &uid, &m2, Some("skill b"), false)?; // disabled

            let skills = enabled_skills(conn, &uid)?;
            assert_eq!(skills.len(), 1);
            assert_eq!(skills[0], "skill a");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn load_manifest_from_dir() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_content = r#"
id = "test-bridge-plugin"
name = "Test Bridge"
version = "0.1.0"
description = "A test bridge plugin"
tier = "bridge"
permissions = ["outbound_http"]
"#;
        std::fs::write(dir.path().join("manifest.toml"), manifest_content).unwrap();
        let mut skill_file = std::fs::File::create(dir.path().join("skill.md")).unwrap();
        skill_file.write_all(b"Test skill content.").unwrap();

        let (manifest, skill) = load_manifest(dir.path()).unwrap();
        assert_eq!(manifest.id, "test-bridge-plugin");
        assert_eq!(skill.as_deref(), Some("Test skill content."));
    }
}
