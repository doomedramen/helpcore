use anyhow::Context;
use chrono::Utc;
use rusqlite::Connection;

#[derive(Debug, Clone)]
pub struct ProviderGrant {
    pub provider_id: String,
    pub enabled: bool,
    pub granted_by: Option<String>,
    pub granted_at: String,
}

pub fn list_grants(conn: &Connection, user_id: &str) -> anyhow::Result<Vec<ProviderGrant>> {
    let mut stmt = conn.prepare(
        "SELECT provider_id, enabled, granted_by, granted_at
         FROM user_provider_grants WHERE user_id = ?1
         ORDER BY granted_at ASC",
    )?;
    let rows = stmt
        .query_map([user_id], |row| {
            Ok(ProviderGrant {
                provider_id: row.get(0)?,
                enabled: row.get::<_, i64>(1)? != 0,
                granted_by: row.get(2)?,
                granted_at: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Returns the set of explicitly disabled provider IDs for a user.
/// Providers with no grant row default to enabled.
pub fn denied_providers(conn: &Connection, user_id: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT provider_id FROM user_provider_grants
         WHERE user_id = ?1 AND enabled = 0",
    )?;
    let rows = stmt
        .query_map([user_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_grant(
    conn: &Connection,
    user_id: &str,
    provider_id: &str,
    enabled: bool,
    granted_by: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO user_provider_grants (user_id, provider_id, enabled, granted_by, granted_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (user_id, provider_id) DO UPDATE
         SET enabled = excluded.enabled, granted_by = excluded.granted_by, granted_at = excluded.granted_at",
        rusqlite::params![user_id, provider_id, enabled as i64, granted_by, now],
    )
    .context("failed to upsert provider grant")?;
    Ok(())
}

/// Grant access to all providers in `provider_ids` for a new user.
pub fn grant_all(
    conn: &Connection,
    user_id: &str,
    provider_ids: &[String],
    granted_by: &str,
) -> anyhow::Result<()> {
    for provider_id in provider_ids {
        set_grant(conn, user_id, provider_id, true, granted_by)?;
    }
    Ok(())
}
