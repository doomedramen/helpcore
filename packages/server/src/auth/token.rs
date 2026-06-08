//! Session token creation, validation, and rotation.

use anyhow::Context;
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const ACCESS_TTL_MINUTES: i64 = 15;
const REFRESH_TTL_DAYS: i64 = 30;

/// Generates a cryptographically random 64-char hex token (32 random bytes).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("OS RNG unavailable");
    hex::encode(bytes)
}

/// Returns the SHA-256 hex digest of a token. Used for storage — raw tokens
/// are never written to the database.
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Result of creating a new session — raw tokens and their expiry timestamps.
pub struct CreatedSession {
    /// Short-lived bearer token for API requests (15 min TTL).
    pub access_token: String,
    /// Long-lived token used to obtain a new access token (30 day TTL).
    pub refresh_token: String,
    /// UTC timestamp when the access token expires.
    pub access_expires_at: DateTime<Utc>,
    /// UTC timestamp when the refresh token expires.
    pub refresh_expires_at: DateTime<Utc>,
}

/// Inserts a new session for `user_id` and returns the raw (unhashed) tokens.
pub fn create_session(conn: &Connection, user_id: &str) -> anyhow::Result<CreatedSession> {
    let access_token = generate_token();
    let refresh_token = generate_token();
    let now = Utc::now();
    let access_expires = now + Duration::minutes(ACCESS_TTL_MINUTES);
    let refresh_expires = now + Duration::days(REFRESH_TTL_DAYS);

    conn.execute(
        "INSERT INTO sessions
             (id, user_id, access_token_hash, refresh_token_hash,
              access_expires_at, refresh_expires_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            user_id,
            hash_token(&access_token),
            hash_token(&refresh_token),
            access_expires.to_rfc3339(),
            refresh_expires.to_rfc3339(),
            now.to_rfc3339(),
        ],
    )
    .context("failed to insert session")?;

    Ok(CreatedSession {
        access_token,
        refresh_token,
        access_expires_at: access_expires,
        refresh_expires_at: refresh_expires,
    })
}

/// Validates an access token. Returns the `user_id` if the session is live.
pub fn validate_access_token(conn: &Connection, token: &str) -> anyhow::Result<Option<String>> {
    let hash = hash_token(token);
    let now = Utc::now().to_rfc3339();

    match conn.query_row(
        "SELECT user_id FROM sessions
         WHERE access_token_hash = ?1
           AND revoked_at IS NULL
           AND access_expires_at > ?2",
        rusqlite::params![&hash, &now],
        |row| row.get::<_, String>(0),
    ) {
        Ok(user_id) => Ok(Some(user_id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::Error::from(e)),
    }
}

/// Validates a refresh token and performs single-use rotation: revokes the old
/// session and creates a new one atomically. Returns `None` if the token is
/// invalid, expired, or already revoked.
pub fn rotate_refresh_token(
    conn: &Connection,
    refresh_token: &str,
) -> anyhow::Result<Option<(String, CreatedSession)>> {
    let hash = hash_token(refresh_token);
    let now = Utc::now().to_rfc3339();

    let row = conn.query_row(
        "SELECT id, user_id FROM sessions
         WHERE refresh_token_hash = ?1
           AND revoked_at IS NULL
           AND refresh_expires_at > ?2",
        rusqlite::params![&hash, &now],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    );

    match row {
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::Error::from(e)),
        Ok((session_id, user_id)) => {
            conn.execute(
                "UPDATE sessions SET revoked_at = ?1 WHERE id = ?2",
                rusqlite::params![&now, &session_id],
            )
            .context("failed to revoke old session")?;

            let new_session = create_session(conn, &user_id)?;
            Ok(Some((user_id, new_session)))
        }
    }
}

/// Revokes the session that owns `token` (works for either access or refresh
/// token). No-op if the session is already revoked or the token is unknown.
pub fn revoke_session_by_token(conn: &Connection, token: &str) -> anyhow::Result<()> {
    let hash = hash_token(token);
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE sessions SET revoked_at = ?1
         WHERE (access_token_hash = ?2 OR refresh_token_hash = ?2)
           AND revoked_at IS NULL",
        rusqlite::params![&now, &hash],
    )
    .context("failed to revoke session")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::open_test_db;

    #[test]
    fn token_is_64_hex_chars() {
        let t = generate_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn token_hash_is_deterministic() {
        let t = "abc123";
        assert_eq!(hash_token(t), hash_token(t));
    }

    #[test]
    fn different_tokens_have_different_hashes() {
        assert_ne!(hash_token("a"), hash_token("b"));
    }

    #[test]
    fn generate_produces_unique_tokens() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2);
    }

    #[test]
    fn create_and_validate_session() {
        let pool = open_test_db();
        let user_id = pool.call_sync(insert_test_user).unwrap();

        pool.call_sync(|conn| {
            let session = create_session(conn, &user_id)?;
            let found = validate_access_token(conn, &session.access_token)?;
            assert_eq!(found.as_deref(), Some(user_id.as_str()));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn unknown_access_token_returns_none() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let found = validate_access_token(conn, &generate_token())?;
            assert!(found.is_none());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn rotate_refresh_token_issues_new_session() {
        let pool = open_test_db();
        let user_id = pool.call_sync(insert_test_user).unwrap();

        pool.call_sync(|conn| {
            let session = create_session(conn, &user_id)?;

            let rotated = rotate_refresh_token(conn, &session.refresh_token)?;
            let (uid, new_session) = rotated.expect("rotation should succeed");
            assert_eq!(uid, user_id);

            // New access token must be valid
            let found = validate_access_token(conn, &new_session.access_token)?;
            assert_eq!(found.as_deref(), Some(user_id.as_str()));

            // Old access token must now be invalid (session revoked)
            let old = validate_access_token(conn, &session.access_token)?;
            assert!(old.is_none(), "old token should be revoked after rotation");

            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn refresh_token_cannot_be_reused() {
        let pool = open_test_db();
        let user_id = pool.call_sync(insert_test_user).unwrap();

        pool.call_sync(|conn| {
            let session = create_session(conn, &user_id)?;
            rotate_refresh_token(conn, &session.refresh_token)?;

            // Second rotation with same token must fail
            let second = rotate_refresh_token(conn, &session.refresh_token)?;
            assert!(
                second.is_none(),
                "already-used refresh token must be rejected"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn revoke_session() {
        let pool = open_test_db();
        let user_id = pool.call_sync(insert_test_user).unwrap();

        pool.call_sync(|conn| {
            let session = create_session(conn, &user_id)?;
            revoke_session_by_token(conn, &session.access_token)?;

            let found = validate_access_token(conn, &session.access_token)?;
            assert!(found.is_none(), "revoked token must not validate");
            Ok(())
        })
        .unwrap();
    }

    // ── test helpers ────────────────────────────────────────────────────────

    fn insert_test_user(conn: &Connection) -> anyhow::Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO users (id, email, password_hash, role, status, created_at, updated_at)
             VALUES (?1, ?2, 'hash', 'admin', 'active', ?3, ?3)",
            rusqlite::params![id, format!("{id}@test.local"), now],
        )?;
        Ok(id)
    }
}
