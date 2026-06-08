//! Tamper-evident audit logging with chained SHA-256 hashes.

use anyhow::Context;
use chrono::Utc;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Writes a tamper-evident audit log entry with chained SHA-256 hashes.
pub fn log_event(
    conn: &Connection,
    event_type: &str,
    actor_id: Option<&str>,
    target_id: Option<&str>,
    payload: serde_json::Value,
) -> anyhow::Result<()> {
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    let payload_str = payload.to_string();

    let prev_hash: Option<String> = conn
        .query_row(
            "SELECT row_hash FROM audit_log ORDER BY created_at DESC, id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();

    let hash_input = format!(
        "{}{}{}{}{}{}{}",
        id,
        event_type,
        actor_id.unwrap_or(""),
        target_id.unwrap_or(""),
        payload_str,
        created_at,
        prev_hash.as_deref().unwrap_or(""),
    );
    let row_hash = hex::encode(Sha256::digest(hash_input.as_bytes()));

    conn.execute(
        "INSERT INTO audit_log
             (id, event_type, actor_id, target_id, payload, row_hash, prev_hash, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            id,
            event_type,
            actor_id,
            target_id,
            payload_str,
            row_hash,
            prev_hash,
            created_at
        ],
    )
    .context("failed to write audit log entry")?;

    Ok(())
}
