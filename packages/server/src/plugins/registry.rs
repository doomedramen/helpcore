/// Plugin registry: loading manifests from disk, registering plugins in the DB,
/// and querying installed plugins for a user.

use anyhow::Context;
use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
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
    pub version: String,
    pub tier: String,
    pub enabled: bool,
    pub skill_md: Option<String>,
    pub permissions: Vec<String>,
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
            "UPDATE plugin_installs SET skill_md = ?1, enabled = ?2, version = ?3
              WHERE user_id = ?4 AND plugin_id = ?5",
            params![skill_md, enabled as i32, manifest.version, user_id, manifest.id],
        )?;
        return Ok(id);
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO plugin_installs
           (id, user_id, plugin_id, version, permissions, enabled, skill_md, installed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, user_id, manifest.id, manifest.version, permissions_json, enabled as i32, skill_md, now],
    )
    .context("failed to insert plugin_installs")?;
    Ok(id)
}

/// Load all enabled plugins for a user (skill fragments included).
pub fn list_enabled(conn: &Connection, user_id: &str) -> anyhow::Result<Vec<InstalledPlugin>> {
    let mut stmt = conn.prepare_cached(
        "SELECT pi.id, pi.plugin_id, p.name, pi.version, p.tier,
                pi.enabled, pi.skill_md, pi.permissions
           FROM plugin_installs pi
           JOIN plugins p ON p.id = pi.plugin_id
          WHERE pi.user_id = ?1
          ORDER BY pi.installed_at",
    )?;
    let rows = stmt
        .query_map(params![user_id], |row| {
            let permissions_json: String = row.get(7)?;
            Ok(InstalledPlugin {
                install_id: row.get(0)?,
                plugin_id:  row.get(1)?,
                name:       row.get(2)?,
                version:    row.get(3)?,
                tier:       row.get(4)?,
                enabled:    row.get::<_, i32>(5)? != 0,
                skill_md:   row.get(6)?,
                permissions: serde_json::from_str(&permissions_json).unwrap_or_default(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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
id = "voice-kittentts"
name = "Voice"
version = "0.1.0"
description = "Voice plugin"
tier = "bridge"
permissions = ["outbound_http"]
"#;
        std::fs::write(dir.path().join("manifest.toml"), manifest_content).unwrap();
        let mut skill_file = std::fs::File::create(dir.path().join("skill.md")).unwrap();
        skill_file.write_all(b"Keep responses spoken-word-friendly.").unwrap();

        let (manifest, skill) = load_manifest(dir.path()).unwrap();
        assert_eq!(manifest.id, "voice-kittentts");
        assert_eq!(skill.as_deref(), Some("Keep responses spoken-word-friendly."));
    }
}
