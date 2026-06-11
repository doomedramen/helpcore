//! SQLite database layer: connection pool, migrations, and audit logging.

#![allow(missing_docs)]

pub mod audit;

use anyhow::Context;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("migrations/0001_users.sql")),
    (2, include_str!("migrations/0002_sessions.sql")),
    (3, include_str!("migrations/0003_api_keys.sql")),
    (4, include_str!("migrations/0004_conversations.sql")),
    (5, include_str!("migrations/0005_messages.sql")),
    (6, include_str!("migrations/0006_audit_log.sql")),
    (7, include_str!("migrations/0007_setup_tokens.sql")),
    (8, include_str!("migrations/0008_memory.sql")),
    (9, include_str!("migrations/0009_plugins.sql")),
    (10, include_str!("migrations/0010_message_status.sql")),
    (11, include_str!("migrations/0011_plugin_lifecycle.sql")),
    (12, include_str!("migrations/0012_user_timezone.sql")),
    (13, include_str!("migrations/0013_provider_grants.sql")),
    (14, include_str!("migrations/0014_memory_leaning.sql")),
    (15, include_str!("migrations/0015_skill_brief.sql")),
    (16, include_str!("migrations/0016_message_tokens.sql")),
    (17, include_str!("migrations/0017_interactions.sql")),
];

/// Thread-safe SQLite connection pool wrapping a single WAL-mode connection.
pub struct DbPool {
    conn: Arc<Mutex<Connection>>,
}

impl DbPool {
    /// Opens (or creates) a SQLite database at `path` and applies all migrations.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create DB directory {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database at {}", path.display()))?;

        apply_pragmas(&conn)?;
        run_migrations(&conn)?;

        tracing::info!(path = %path.display(), "database opened");

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Run a closure against the connection on a blocking thread.
    ///
    /// Use this from async handlers. The closure executes in `spawn_blocking`
    /// so it must be `Send + 'static`.
    pub async fn call<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let guard = conn.lock();
            f(&guard)
        })
        .await
        .map_err(|e| anyhow::anyhow!("DB task panicked: {e}"))?
    }

    /// Synchronous version for startup code and tests.
    pub fn call_sync<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let guard = self.conn.lock();
        f(&guard)
    }
}

fn apply_pragmas(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA synchronous  = NORMAL;
         PRAGMA mmap_size    = 8388608;
         PRAGMA cache_size   = -2000;
         PRAGMA temp_store   = MEMORY;",
    )
    .context("failed to set connection PRAGMAs")
}

fn run_migrations(conn: &Connection) -> anyhow::Result<()> {
    let current: u32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .context("failed to read schema version")?;

    for (version, sql) in MIGRATIONS {
        if *version > current {
            conn.execute_batch(sql)
                .with_context(|| format!("migration {version} failed"))?;
            conn.execute_batch(&format!("PRAGMA user_version = {version}"))
                .with_context(|| format!("failed to advance schema version to {version}"))?;
            tracing::info!(version, "applied migration");
        }
    }

    Ok(())
}

/// Opens an in-memory DB with migrations applied. Used by unit and integration
/// tests — compiled unconditionally so integration tests (which live outside
/// this crate's `cfg(test)` scope) can access it.
#[doc(hidden)]
pub fn open_in_memory() -> DbPool {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_migrations(&conn).unwrap();
    DbPool {
        conn: Arc::new(Mutex::new(conn)),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn open_test_db() -> DbPool {
        super::open_in_memory()
    }

    #[test]
    fn migrations_advance_schema_version() {
        let pool = open_test_db();
        let version: u32 = pool
            .call_sync(|conn| Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as u32);
    }

    #[test]
    fn all_tables_exist_after_migrations() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let mut stmt =
                conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
            let tables: Vec<String> = stmt
                .query_map([], |row| row.get(0))?
                .collect::<Result<_, _>>()?;

            for expected in &[
                "users",
                "sessions",
                "api_keys",
                "conversations",
                "messages",
                "audit_log",
                "setup_tokens",
                "user_personality",
                "memory_files",
                "plugins",
                "plugin_installs",
                "plugin_tokens",
                "interactions",
            ] {
                assert!(
                    tables.contains(&expected.to_string()),
                    "missing table: {expected}"
                );
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn migrations_are_idempotent() {
        let pool = open_test_db();
        // Run again — must not error, version must stay the same
        pool.call_sync(run_migrations).unwrap();
        let version: u32 = pool
            .call_sync(|conn| Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as u32);
    }

    #[test]
    fn foreign_keys_are_enabled() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let enabled: i32 = conn.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
            assert_eq!(enabled, 1, "foreign_keys should be ON");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn foreign_key_constraint_is_enforced() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            // Inserting a session that references a non-existent user must fail
            let result = conn.execute(
                "INSERT INTO sessions (id, user_id, access_token_hash, refresh_token_hash,
                                      access_expires_at, refresh_expires_at, created_at)
                 VALUES ('s1', 'no-such-user', 'ath', 'rth', '2099-01-01', '2099-01-01', '2024-01-01')",
                [],
            );
            assert!(result.is_err(), "FK violation should be rejected");
            Ok(())
        })
        .unwrap();
    }
}
