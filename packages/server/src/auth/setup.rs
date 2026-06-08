//! First-admin setup: one-time tokens for creating the initial admin user.

use anyhow::Context;
use chrono::{Duration, Utc};
use rusqlite::Connection;

const SETUP_TOKEN_TTL_MINUTES: i64 = 15;

/// Returns `true` if no users exist — the server needs first-admin setup.
pub fn needs_setup(conn: &Connection) -> anyhow::Result<bool> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
        .context("failed to check user count")?;
    Ok(count == 0)
}

/// Generates a one-time setup token, stores it in the DB, and returns the raw
/// token. Any previous unused tokens are cleared first (only one active at a time).
pub fn generate_setup_token(conn: &Connection) -> anyhow::Result<String> {
    conn.execute("DELETE FROM setup_tokens WHERE used_at IS NULL", [])
        .context("failed to clear stale setup tokens")?;

    let token = crate::auth::token::generate_token();
    let expires_at = (Utc::now() + Duration::minutes(SETUP_TOKEN_TTL_MINUTES)).to_rfc3339();

    conn.execute(
        "INSERT INTO setup_tokens (token, expires_at) VALUES (?1, ?2)",
        rusqlite::params![&token, expires_at],
    )
    .context("failed to store setup token")?;

    Ok(token)
}

/// Validates and atomically consumes a setup token.
/// Returns `true` if the token was valid and has been marked as used.
/// Returns `false` if the token is unknown, expired, or already consumed.
pub fn consume_setup_token(conn: &Connection, token: &str) -> anyhow::Result<bool> {
    let now = Utc::now().to_rfc3339();

    let found = conn.query_row(
        "SELECT token FROM setup_tokens
         WHERE token = ?1 AND used_at IS NULL AND expires_at > ?2",
        rusqlite::params![token, &now],
        |row| row.get::<_, String>(0),
    );

    match found {
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
        Err(e) => Err(anyhow::Error::from(e)),
        Ok(_) => {
            conn.execute(
                "UPDATE setup_tokens SET used_at = ?1 WHERE token = ?2",
                rusqlite::params![now, token],
            )
            .context("failed to mark setup token as used")?;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::open_test_db;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn fresh_db_needs_setup() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            assert!(needs_setup(conn)?);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn db_with_a_user_does_not_need_setup() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO users (id, email, password_hash, role, status, created_at, updated_at)
                 VALUES (?1, 'admin@test.local', 'hash', 'admin', 'active', ?2, ?2)",
                rusqlite::params![id, now],
            )?;
            assert!(!needs_setup(conn)?);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn generate_and_consume_token() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let token = generate_setup_token(conn)?;
            assert!(!token.is_empty());

            let consumed = consume_setup_token(conn, &token)?;
            assert!(consumed, "valid token should be consumed");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn token_cannot_be_consumed_twice() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let token = generate_setup_token(conn)?;
            consume_setup_token(conn, &token)?;

            let second = consume_setup_token(conn, &token)?;
            assert!(!second, "already-used token must not be consumable again");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn unknown_token_returns_false() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let result = consume_setup_token(conn, "not-a-real-token")?;
            assert!(!result);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn generating_new_token_clears_old_one() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let first = generate_setup_token(conn)?;
            let _second = generate_setup_token(conn)?;

            // First token was cleared and can no longer be consumed
            let result = consume_setup_token(conn, &first)?;
            assert!(!result, "superseded token must not be consumable");
            Ok(())
        })
        .unwrap();
    }
}
