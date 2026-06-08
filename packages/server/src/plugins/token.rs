/// Scoped bearer tokens for Tier 2 bridge plugins.
///
/// Bridge plugins (like the voice plugin) need a long-lived credential to call
/// the helpcore API on behalf of a user. These are separate from user session
/// tokens and can be independently revoked.
use anyhow::Context;
use chrono::Utc;
use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::auth::token::{generate_token, hash_token};

/// A newly-created plugin token (raw value shown once, then only the hash is stored).
pub struct NewPluginToken {
    /// The raw token to hand to the bridge plugin. Prefix `hcp_` marks it as
    /// a plugin token (distinct from user API keys which use `hc_`).
    pub raw: String,
    pub token_id: String,
}

/// Create a new scoped token for a bridge plugin and store its hash.
pub fn create_plugin_token(
    conn: &Connection,
    user_id: &str,
    plugin_id: &str,
    permissions: &[String],
) -> anyhow::Result<NewPluginToken> {
    let raw_suffix = generate_token();
    let raw = format!("hcp_{raw_suffix}");
    let hash = hash_token(&raw);
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let permissions_json = serde_json::to_string(permissions)?;

    conn.execute(
        "INSERT INTO plugin_tokens (id, user_id, plugin_id, token_hash, permissions, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, user_id, plugin_id, hash, permissions_json, now],
    )
    .context("failed to create plugin token")?;

    Ok(NewPluginToken { raw, token_id: id })
}

/// Look up the user_id and plugin_id for a valid (non-revoked) plugin token.
/// Returns `None` if the token is invalid or revoked.
pub fn validate_plugin_token(
    conn: &Connection,
    raw_token: &str,
) -> anyhow::Result<Option<(String, String)>> {
    let hash = hash_token(raw_token);
    let mut stmt = conn.prepare_cached(
        "SELECT user_id, plugin_id FROM plugin_tokens
          WHERE token_hash = ?1 AND revoked_at IS NULL",
    )?;
    match stmt.query_row(params![hash], |row| Ok((row.get(0)?, row.get(1)?))) {
        Ok(pair) => Ok(Some(pair)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Revoke a plugin token by its ID.
pub fn revoke_plugin_token(
    conn: &Connection,
    token_id: &str,
    user_id: &str,
) -> anyhow::Result<bool> {
    let now = Utc::now().to_rfc3339();
    let n = conn.execute(
        "UPDATE plugin_tokens SET revoked_at = ?1
          WHERE id = ?2 AND user_id = ?3 AND revoked_at IS NULL",
        params![now, token_id, user_id],
    )?;
    Ok(n > 0)
}

/// List active (non-revoked) tokens for a user+plugin pair.
pub struct TokenInfo {
    pub id: String,
    pub plugin_id: String,
    pub created_at: String,
}

pub fn list_plugin_tokens(
    conn: &Connection,
    user_id: &str,
    plugin_id: &str,
) -> anyhow::Result<Vec<TokenInfo>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, plugin_id, created_at FROM plugin_tokens
          WHERE user_id = ?1 AND plugin_id = ?2 AND revoked_at IS NULL
          ORDER BY created_at DESC",
    )?;
    let rows = stmt
        .query_map(params![user_id, plugin_id], |row| {
            Ok(TokenInfo {
                id: row.get(0)?,
                plugin_id: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::open_in_memory,
        plugins::registry::{Manifest, ensure_installed, upsert_plugin},
    };

    fn make_manifest(id: &str) -> Manifest {
        Manifest {
            id: id.to_string(),
            name: "Test Bridge".to_string(),
            version: "0.1.0".to_string(),
            description: "test".to_string(),
            tier: "bridge".to_string(),
            permissions: vec![],
            provides: Vec::new(),
            min_core_version: None,
            bridge: None,
            allowed_hosts: Vec::new(),
            config_schema: Vec::new(),
            brief: "Use when testing.".to_string(),
        }
    }

    fn setup(conn: &Connection) -> (String, String) {
        conn.execute(
            "INSERT INTO users (id, email, password_hash, role, created_at, updated_at)
             VALUES ('u1', 'tok@test.com', 'hash', 'admin', '2024-01-01', '2024-01-01')",
            [],
        )
        .unwrap();
        let m = make_manifest("bridge-plugin");
        upsert_plugin(conn, &m).unwrap();
        ensure_installed(conn, "u1", &m, None, true).unwrap();
        ("u1".into(), "bridge-plugin".into())
    }

    #[test]
    fn create_and_validate_token() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let (uid, pid) = setup(conn);
            let tok = create_plugin_token(conn, &uid, &pid, &["outbound_http".to_string()])?;
            assert!(tok.raw.starts_with("hcp_"));

            let result = validate_plugin_token(conn, &tok.raw)?;
            assert!(result.is_some());
            let (got_uid, got_pid) = result.unwrap();
            assert_eq!(got_uid, uid);
            assert_eq!(got_pid, pid);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn revoked_token_is_rejected() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let (uid, pid) = setup(conn);
            let tok = create_plugin_token(conn, &uid, &pid, &[])?;
            assert!(revoke_plugin_token(conn, &tok.token_id, &uid)?);
            assert!(validate_plugin_token(conn, &tok.raw)?.is_none());
            Ok(())
        })
        .unwrap();
    }
}
