use anyhow::Context;
use chrono::Utc;
use rusqlite::{Connection, params};
use std::collections::HashSet;

// ── Public types ──────────────────────────────────────────────────────────────

/// Soul, identity, and user-profile personality files for a user.
#[derive(Debug, Default)]
pub struct PersonalityFiles {
    pub soul: Option<String>,
    pub identity: Option<String>,
    pub user_profile: Option<String>,
}

/// A memory file returned from a search or direct read.
#[derive(Debug, Clone)]
pub struct MemoryResult {
    pub path: String,
    pub content: String,
}

/// Metadata returned when listing memory files.
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub path: String,
    pub updated_at: String,
}

// ── Path sanitisation ─────────────────────────────────────────────────────────

/// Validates a client-supplied memory path, returning the normalised version.
///
/// Rejects:
/// - absolute paths (start with `/`)
/// - any `..` component
/// - embedded null bytes
/// - empty strings
pub fn sanitize_path(path: &str) -> anyhow::Result<String> {
    if path.is_empty() {
        anyhow::bail!("memory path must not be empty");
    }
    if path.starts_with('/') {
        anyhow::bail!("memory path must be relative, not absolute");
    }
    if path.contains('\0') {
        anyhow::bail!("memory path must not contain null bytes");
    }
    // Walk components and reject any `..` segment.
    for part in path.split('/') {
        if part == ".." {
            anyhow::bail!("memory path must not contain '..' components");
        }
    }
    Ok(path.to_string())
}

// ── Personality ───────────────────────────────────────────────────────────────

/// Load all three personality files for a user in one query.
pub fn load_personality(conn: &Connection, user_id: &str) -> anyhow::Result<PersonalityFiles> {
    let mut stmt =
        conn.prepare_cached("SELECT name, content FROM user_personality WHERE user_id = ?1")?;
    let rows = stmt.query_map(params![user_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut files = PersonalityFiles::default();
    for row in rows {
        let (name, content) = row?;
        match name.as_str() {
            "soul" => files.soul = Some(content),
            "identity" => files.identity = Some(content),
            "user" => files.user_profile = Some(content),
            _ => {}
        }
    }
    Ok(files)
}

/// Read one personality file by name (`soul`, `identity`, or `user`).
pub fn get_personality(
    conn: &Connection,
    user_id: &str,
    name: &str,
) -> anyhow::Result<Option<String>> {
    let mut stmt = conn
        .prepare_cached("SELECT content FROM user_personality WHERE user_id = ?1 AND name = ?2")?;
    let result = stmt.query_row(params![user_id, name], |row| row.get(0));
    match result {
        Ok(content) => Ok(Some(content)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

const DEFAULT_SOUL: &str = "\
You are a helpful, direct, and thoughtful personal assistant. \
You adapt your tone to the context — concise for quick questions, \
detailed when depth is needed. You do not pad responses with \
unnecessary affirmations or filler phrases.";

const DEFAULT_IDENTITY: &str = "\
You are a personal AI assistant running on the user's own server. \
You have no name by default — the user can give you one here.";

const DEFAULT_USER_PROFILE: &str = "\
The user has not filled in this section yet. \
Ask them about themselves when it feels natural, \
and suggest they complete this file at /personality.";

/// Seed the three default personality files for a newly created user.
/// Uses INSERT OR IGNORE so it never overwrites an existing value.
pub fn seed_default_personality(conn: &Connection, user_id: &str) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    for (name, content) in [
        ("soul", DEFAULT_SOUL),
        ("identity", DEFAULT_IDENTITY),
        ("user", DEFAULT_USER_PROFILE),
    ] {
        conn.execute(
            "INSERT OR IGNORE INTO user_personality (user_id, name, content, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![user_id, name, content, now],
        )
        .context("failed to seed personality")?;
    }
    Ok(())
}

/// Create or replace a personality file.
pub fn set_personality(
    conn: &Connection,
    user_id: &str,
    name: &str,
    content: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO user_personality (user_id, name, content, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id, name) DO UPDATE SET content = excluded.content,
                                                   updated_at = excluded.updated_at",
        params![user_id, name, content, now],
    )
    .context("failed to upsert personality")?;
    Ok(())
}

// ── Memory CRUD ───────────────────────────────────────────────────────────────

/// List all memory files for a user (path + timestamp, no content).
pub fn list_memory(conn: &Connection, user_id: &str) -> anyhow::Result<Vec<MemoryEntry>> {
    let mut stmt = conn.prepare_cached(
        "SELECT path, updated_at FROM memory_files WHERE user_id = ?1 ORDER BY path",
    )?;
    let entries = stmt
        .query_map(params![user_id], |row| {
            Ok(MemoryEntry {
                path: row.get(0)?,
                updated_at: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

/// Read one memory file by path. Returns `None` if the file does not exist.
pub fn read_memory(conn: &Connection, user_id: &str, path: &str) -> anyhow::Result<Option<String>> {
    let mut stmt =
        conn.prepare_cached("SELECT content FROM memory_files WHERE user_id = ?1 AND path = ?2")?;
    match stmt.query_row(params![user_id, path], |row| row.get(0)) {
        Ok(content) => Ok(Some(content)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Create or overwrite a memory file.
///
/// The `memory_files_ai` / `memory_files_au` triggers keep the `memory_fts`
/// FTS5 index up to date automatically — no explicit FTS writes needed here.
pub fn write_memory(
    conn: &Connection,
    user_id: &str,
    path: &str,
    content: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    // INSERT OR REPLACE fires AFTER DELETE + AFTER INSERT triggers on conflict,
    // which keeps the FTS index correctly maintained.
    conn.execute(
        "INSERT OR REPLACE INTO memory_files (user_id, path, content, updated_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![user_id, path, content, now],
    )
    .context("failed to write memory file")?;
    Ok(())
}

/// Append content to a memory file, creating it if it does not exist yet.
///
/// Joins existing and new content with a newline so appended notes don't run
/// together with what was already there.
pub fn append_memory(
    conn: &Connection,
    user_id: &str,
    path: &str,
    content: &str,
) -> anyhow::Result<()> {
    let existing = read_memory(conn, user_id, path)?;
    let combined = match existing {
        Some(existing) if !existing.is_empty() => format!("{existing}\n{content}"),
        _ => content.to_string(),
    };
    write_memory(conn, user_id, path, &combined)
}

/// Rename or move a memory file. Returns `true` if a row was moved, `false`
/// if the source path did not exist. Fails if the destination already exists.
///
/// The `memory_files_au` trigger keeps the FTS5 index up to date automatically.
pub fn move_memory(conn: &Connection, user_id: &str, from: &str, to: &str) -> anyhow::Result<bool> {
    let now = Utc::now().to_rfc3339();
    let n = conn
        .execute(
            "UPDATE memory_files SET path = ?1, updated_at = ?2 WHERE user_id = ?3 AND path = ?4",
            params![to, now, user_id, from],
        )
        .context("failed to move memory file")?;
    Ok(n > 0)
}

/// Delete a memory file. Returns `true` if a row was deleted, `false` if it
/// did not exist.
///
/// The `memory_files_ad` trigger keeps the FTS5 index up to date automatically.
pub fn delete_memory(conn: &Connection, user_id: &str, path: &str) -> anyhow::Result<bool> {
    let n = conn
        .execute(
            "DELETE FROM memory_files WHERE user_id = ?1 AND path = ?2",
            params![user_id, path],
        )
        .context("failed to delete memory file")?;
    Ok(n > 0)
}

// ── Memory search ─────────────────────────────────────────────────────────────

/// Turn free text into a safe, broad FTS5 MATCH expression.
///
/// FTS5 has its own query parser that treats `!`, `"`, `*`, `^`, `(`, `)`,
/// `-` and similar characters as operators or syntax. Passing a raw user
/// message directly causes parse errors (e.g. "Hi!" → `fts5: syntax error
/// near "!"`, and a hyphen-only word like "--" between two terms produces
/// `fts5: syntax error near "OR"` once joined below). This keeps only
/// alphanumerics — the characters that can appear in FTS5 bareword terms
/// without special meaning — lowercases each word (matching is
/// case-insensitive anyway, and this keeps a stray "OR"/"AND" in the input
/// from being parsed as an operator), drops duplicates, and joins the result
/// with `OR`.
///
/// `OR` rather than FTS5's default `AND`: a bare `term1 term2` query only
/// matches files containing *every* term, which is far too strict for
/// natural-language recall — a multi-word message, let alone one enriched
/// with recent conversation turns (see `memory_recall_query` in chat.rs),
/// would rarely find every word in a single memory file. `OR` casts a wide
/// net and lets FTS5's relevance ranking surface the best matches instead.
fn sanitize_fts_query(query: &str) -> String {
    let mut seen = HashSet::new();
    query
        .split_whitespace()
        .filter_map(|word| {
            let term: String = word
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            (!term.is_empty() && seen.insert(term.clone())).then_some(term)
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// FTS5 search across a user's memory files.
///
/// Searches both `content` and `path` columns. Returns up to `limit` results
/// ordered by FTS5 relevance rank.
///
/// Falls back to returning the most-recently-updated files when `query`
/// contains no usable search terms after sanitisation.
pub fn search_memory(
    conn: &Connection,
    user_id: &str,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<MemoryResult>> {
    let clean = sanitize_fts_query(query);
    // Use the empty-query fallback if nothing usable remains after sanitisation.
    let query = if clean.is_empty() { "" } else { &clean };

    if query.trim().is_empty() {
        // No query — return the most recently updated files up to `limit`.
        let mut stmt = conn.prepare_cached(
            "SELECT path, content FROM memory_files
              WHERE user_id = ?1
              ORDER BY updated_at DESC
              LIMIT ?2",
        )?;
        let results = stmt
            .query_map(params![user_id, limit as i64], |row| {
                Ok(MemoryResult {
                    path: row.get(0)?,
                    content: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(results);
    }

    // FTS5 search: join against memory_files so user_id filtering is handled
    // by the base table rather than inside the virtual table (more robust
    // across SQLite versions).
    let mut stmt = conn.prepare_cached(
        "SELECT mf.path, mf.content
           FROM memory_fts  AS fts
           JOIN memory_files AS mf ON mf.id = fts.rowid
          WHERE fts.memory_fts MATCH ?1
            AND mf.user_id = ?2
          ORDER BY fts.rank
          LIMIT ?3",
    )?;
    let results = stmt
        .query_map(params![query, user_id, limit as i64], |row| {
            Ok(MemoryResult {
                path: row.get(0)?,
                content: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(results)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn setup_user(conn: &Connection) -> String {
        let uid = "user-mem-test";
        conn.execute(
            "INSERT INTO users (id, email, password_hash, role, created_at, updated_at)
             VALUES (?1, 'mem@test.com', 'hash', 'admin', '2024-01-01', '2024-01-01')",
            params![uid],
        )
        .unwrap();
        uid.to_string()
    }

    #[test]
    fn sanitize_path_rejects_absolute() {
        assert!(sanitize_path("/etc/passwd").is_err());
    }

    #[test]
    fn sanitize_path_rejects_dotdot() {
        assert!(sanitize_path("../escape").is_err());
        assert!(sanitize_path("foo/../bar").is_err());
    }

    #[test]
    fn sanitize_path_accepts_valid() {
        assert!(sanitize_path("notes.md").is_ok());
        assert!(sanitize_path("home/devices.md").is_ok());
        assert!(sanitize_path("project-eeva.md").is_ok());
    }

    #[test]
    fn personality_round_trip() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);
            set_personality(conn, &uid, "soul", "Be helpful.")?;
            let got = get_personality(conn, &uid, "soul")?.unwrap();
            assert_eq!(got, "Be helpful.");

            // Overwrite
            set_personality(conn, &uid, "soul", "Be direct.")?;
            let got2 = get_personality(conn, &uid, "soul")?.unwrap();
            assert_eq!(got2, "Be direct.");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn load_personality_all_fields() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);
            set_personality(conn, &uid, "soul", "soul content")?;
            set_personality(conn, &uid, "identity", "identity content")?;
            set_personality(conn, &uid, "user", "user content")?;

            let p = load_personality(conn, &uid)?;
            assert_eq!(p.soul.as_deref(), Some("soul content"));
            assert_eq!(p.identity.as_deref(), Some("identity content"));
            assert_eq!(p.user_profile.as_deref(), Some("user content"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn memory_write_read_delete() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);

            write_memory(conn, &uid, "notes.md", "hello world")?;
            let content = read_memory(conn, &uid, "notes.md")?.unwrap();
            assert_eq!(content, "hello world");

            // Overwrite
            write_memory(conn, &uid, "notes.md", "updated")?;
            assert_eq!(read_memory(conn, &uid, "notes.md")?.unwrap(), "updated");

            // Delete
            assert!(delete_memory(conn, &uid, "notes.md")?);
            assert!(read_memory(conn, &uid, "notes.md")?.is_none());
            assert!(!delete_memory(conn, &uid, "notes.md")?); // second delete = false
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn append_memory_creates_and_appends() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);

            append_memory(conn, &uid, "log.md", "first entry")?;
            assert_eq!(read_memory(conn, &uid, "log.md")?.unwrap(), "first entry");

            append_memory(conn, &uid, "log.md", "second entry")?;
            assert_eq!(
                read_memory(conn, &uid, "log.md")?.unwrap(),
                "first entry\nsecond entry"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn move_memory_renames_file() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);
            write_memory(conn, &uid, "old.md", "content")?;

            assert!(move_memory(conn, &uid, "old.md", "new.md")?);
            assert!(read_memory(conn, &uid, "old.md")?.is_none());
            assert_eq!(read_memory(conn, &uid, "new.md")?.unwrap(), "content");

            // Moving a non-existent file returns false.
            assert!(!move_memory(conn, &uid, "old.md", "another.md")?);

            // Search index follows the move (old path no longer matches).
            let results = search_memory(conn, &uid, "content", 10)?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].path, "new.md");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn list_memory_returns_entries() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);
            write_memory(conn, &uid, "a.md", "alpha")?;
            write_memory(conn, &uid, "b.md", "beta")?;

            let entries = list_memory(conn, &uid)?;
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].path, "a.md");
            assert_eq!(entries[1].path, "b.md");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn search_memory_finds_relevant_file() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);
            write_memory(
                conn,
                &uid,
                "rust.md",
                "Rust is a systems programming language.",
            )?;
            write_memory(
                conn,
                &uid,
                "cooking.md",
                "My favourite recipe is carbonara.",
            )?;

            let results = search_memory(conn, &uid, "rust programming", 10)?;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].path, "rust.md");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn search_memory_ors_terms_instead_of_anding() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);
            write_memory(
                conn,
                &uid,
                "rust.md",
                "Rust is a systems programming language.",
            )?;
            write_memory(
                conn,
                &uid,
                "cooking.md",
                "My favourite recipe is carbonara.",
            )?;

            // Neither file contains every word of this query — under FTS5's
            // default implicit-AND join this would return nothing. Recall
            // queries are enriched with extra context (see memory_recall_query
            // in chat.rs) and must broaden, not narrow, so search_memory joins
            // terms with OR and ranks by relevance instead.
            let results = search_memory(conn, &uid, "rust carbonara dinner", 10)?;
            assert_eq!(results.len(), 2, "OR join should surface both files");

            // A query containing "OR"/"or" as ordinary words must not be
            // parsed as the FTS5 operator or break the query.
            let results = search_memory(conn, &uid, "carbonara or rust OR sandwich", 10)?;
            assert_eq!(results.len(), 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn search_empty_table_returns_empty_not_error() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);
            // FTS5 search against a completely empty table must return [] not error.
            let results = search_memory(conn, &uid, "Hi!", 10)?;
            assert!(results.is_empty(), "expected empty, got {}", results.len());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn search_memory_strips_hyphens_that_break_fts5_syntax() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);
            write_memory(conn, &uid, "notes.md", "Rust is great")?;

            // A hyphen-only "word" (e.g. an em-dash typed as "--") sits between
            // sanitised terms once joined with " OR " and previously produced
            // `fts5: syntax error near "OR"`; internally hyphenated words like
            // "well-known" hit `fts5: syntax error near` / "no such column" for
            // the same reason. Neither should reach FTS5 at all.
            assert!(search_memory(conn, &uid, "wait -- really?", 10).is_ok());
            assert!(search_memory(conn, &uid, "a well-known fact", 10).is_ok());
            assert!(search_memory(conn, &uid, "trailing- hyphen", 10).is_ok());
            assert!(search_memory(conn, &uid, "-leading hyphen", 10).is_ok());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn search_memory_empty_query_returns_recent() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let uid = setup_user(conn);
            write_memory(conn, &uid, "a.md", "alpha")?;
            write_memory(conn, &uid, "b.md", "beta")?;

            let results = search_memory(conn, &uid, "", 10)?;
            assert_eq!(results.len(), 2);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn memory_is_user_scoped() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            // Create a second user
            conn.execute(
                "INSERT INTO users (id, email, password_hash, role, created_at, updated_at)
                 VALUES ('user-2', 'other@test.com', 'hash', 'user', '2024-01-01', '2024-01-01')",
                [],
            )?;

            let uid1 = setup_user(conn);
            write_memory(conn, &uid1, "notes.md", "user 1 notes")?;

            // user-2 should see nothing
            let entries = list_memory(conn, "user-2")?;
            assert!(entries.is_empty());
            Ok(())
        })
        .unwrap();
    }

    /// Locks in that NO memory or personality operation can read, search,
    /// modify, or delete another user's data — regardless of which path the
    /// caller knows about. This is the guarantee the built-in AI tools and
    /// HTTP handlers both rely on (every call site threads the authenticated
    /// `user_id` straight into these functions; the isolation must hold here,
    /// at the data layer, since that's the last line of defence).
    #[test]
    fn cross_user_access_is_blocked_for_every_operation() {
        let pool = open_in_memory();
        pool.call_sync(|conn| {
            let owner = setup_user(conn);
            conn.execute(
                "INSERT INTO users (id, email, password_hash, role, created_at, updated_at)
                 VALUES ('user-intruder', 'intruder@test.com', 'hash', 'user', '2024-01-01', '2024-01-01')",
                [],
            )?;
            let intruder = "user-intruder";

            write_memory(conn, &owner, "secret.md", "owner's secret pasta recipe")?;
            set_personality(conn, &owner, "soul", "owner's private soul")?;

            // Reads: nothing comes back, and nothing about the file's
            // existence leaks (search returns no hits either).
            assert!(read_memory(conn, intruder, "secret.md")?.is_none());
            assert!(list_memory(conn, intruder)?.is_empty());
            assert!(
                search_memory(conn, intruder, "owner secret pasta recipe", 10)?.is_empty(),
                "FTS5 search must not surface another user's content"
            );
            assert!(get_personality(conn, intruder, "soul")?.is_none());

            // Mutations: report "not found" / no-op rather than touching the
            // owner's row.
            assert!(!delete_memory(conn, intruder, "secret.md")?);
            assert!(!move_memory(conn, intruder, "secret.md", "stolen.md")?);

            // The owner's data must be completely untouched by the above.
            assert_eq!(
                read_memory(conn, &owner, "secret.md")?.unwrap(),
                "owner's secret pasta recipe"
            );
            assert_eq!(
                get_personality(conn, &owner, "soul")?.unwrap(),
                "owner's private soul"
            );
            assert_eq!(list_memory(conn, &owner)?.len(), 1);
            Ok(())
        })
        .unwrap();
    }
}
