use anyhow::Context;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const PREFIX: &str = "hc_";
const DISPLAY_LEN: usize = 12; // "hc_" (3) + 8 random chars

/// Generates a new API key: `hc_` followed by 64 hex chars (32 random bytes).
pub fn generate_key() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("OS RNG unavailable");
    format!("{PREFIX}{}", hex::encode(bytes))
}

/// Returns the SHA-256 hex digest of an API key. Used for storage.
pub fn hash_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

/// Returns the UI-visible prefix (`hc_` + first 8 chars) stored in the DB.
pub fn display_prefix(key: &str) -> String {
    key.chars().take(DISPLAY_LEN).collect()
}

pub struct CreatedApiKey {
    pub id: String,
    /// The full key — shown to the user once, never stored in the DB.
    pub full_key: String,
    /// The prefix shown in the key list UI.
    pub key_prefix: String,
}

pub fn create_api_key(
    conn: &Connection,
    user_id: &str,
    name: &str,
    scopes: Option<&[&str]>,
    expires_at: Option<DateTime<Utc>>,
) -> anyhow::Result<CreatedApiKey> {
    let key = generate_key();
    let id = Uuid::new_v4().to_string();
    let prefix = display_prefix(&key);
    let hash = hash_key(&key);
    let now = Utc::now().to_rfc3339();
    let scopes_json = scopes.map(|s| serde_json::to_string(s).unwrap_or_default());

    conn.execute(
        "INSERT INTO api_keys
             (id, user_id, name, key_prefix, key_hash, scopes, expires_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            &id,
            user_id,
            name,
            &prefix,
            &hash,
            scopes_json,
            expires_at.map(|d| d.to_rfc3339()),
            now,
        ],
    )
    .context("failed to insert API key")?;

    Ok(CreatedApiKey {
        id,
        full_key: key,
        key_prefix: prefix,
    })
}

/// Validates an API key and returns the `user_id` if it is active.
/// Also updates `last_used_at` on a successful match.
pub fn validate_api_key(conn: &Connection, key: &str) -> anyhow::Result<Option<String>> {
    if !key.starts_with(PREFIX) {
        return Ok(None);
    }

    let hash = hash_key(key);
    let now = Utc::now().to_rfc3339();

    let result = conn.query_row(
        "SELECT user_id FROM api_keys
         WHERE key_hash = ?1
           AND revoked_at IS NULL
           AND (expires_at IS NULL OR expires_at > ?2)",
        rusqlite::params![&hash, &now],
        |row| row.get::<_, String>(0),
    );

    match result {
        Ok(user_id) => {
            // Best-effort: update last_used_at (non-fatal if it fails)
            let _ = conn.execute(
                "UPDATE api_keys SET last_used_at = ?1 WHERE key_hash = ?2",
                rusqlite::params![&now, &hash],
            );
            Ok(Some(user_id))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::Error::from(e)),
    }
}

/// Revokes the key identified by `key_id`, restricted to `user_id`.
/// Returns `true` if a key was actually revoked.
pub fn revoke_api_key(conn: &Connection, key_id: &str, user_id: &str) -> anyhow::Result<bool> {
    let now = Utc::now().to_rfc3339();
    let n = conn
        .execute(
            "UPDATE api_keys SET revoked_at = ?1
         WHERE id = ?2 AND user_id = ?3 AND revoked_at IS NULL",
            rusqlite::params![now, key_id, user_id],
        )
        .context("failed to revoke API key")?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::open_test_db;
    use chrono::Utc;
    use uuid::Uuid;

    fn insert_test_user(conn: &Connection) -> anyhow::Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO users (id, email, password_hash, role, status, created_at, updated_at)
             VALUES (?1, ?2, 'hash', 'member', 'active', ?3, ?3)",
            rusqlite::params![&id, format!("{id}@test.local"), now],
        )?;
        Ok(id)
    }

    #[test]
    fn generated_key_has_hc_prefix() {
        let key = generate_key();
        assert!(key.starts_with("hc_"), "key must start with hc_: {key}");
        assert_eq!(key.len(), 3 + 64, "unexpected key length");
    }

    #[test]
    fn display_prefix_length() {
        let key = generate_key();
        let prefix = display_prefix(&key);
        assert_eq!(prefix.len(), DISPLAY_LEN);
        assert!(prefix.starts_with("hc_"));
    }

    #[test]
    fn hash_is_deterministic() {
        let key = generate_key();
        assert_eq!(hash_key(&key), hash_key(&key));
    }

    #[test]
    fn create_and_validate_key() {
        let pool = open_test_db();
        let user_id = pool.call_sync(insert_test_user).unwrap();

        pool.call_sync(|conn| {
            let created = create_api_key(conn, &user_id, "test-key", None, None)?;
            let found = validate_api_key(conn, &created.full_key)?;
            assert_eq!(found.as_deref(), Some(user_id.as_str()));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn key_without_prefix_is_rejected() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let found = validate_api_key(conn, "no_prefix_key")?;
            assert!(found.is_none());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn unknown_key_returns_none() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let found = validate_api_key(conn, &generate_key())?;
            assert!(found.is_none());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn revoked_key_is_rejected() {
        let pool = open_test_db();
        let user_id = pool.call_sync(insert_test_user).unwrap();

        pool.call_sync(|conn| {
            let created = create_api_key(conn, &user_id, "test-key", None, None)?;
            let revoked = revoke_api_key(conn, &created.id, &user_id)?;
            assert!(revoked);

            let found = validate_api_key(conn, &created.full_key)?;
            assert!(found.is_none(), "revoked key must not validate");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn revoke_returns_false_for_wrong_user() {
        let pool = open_test_db();
        let user_id = pool.call_sync(insert_test_user).unwrap();

        pool.call_sync(|conn| {
            let created = create_api_key(conn, &user_id, "test-key", None, None)?;
            let revoked = revoke_api_key(conn, &created.id, "other-user-id")?;
            assert!(!revoked, "revoke with wrong user must return false");
            Ok(())
        })
        .unwrap();
    }
}
