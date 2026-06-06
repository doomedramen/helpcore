use anyhow::Context;
use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserRole {
    Admin,
    Member,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::Member => "member",
        }
    }
}

impl TryFrom<&str> for UserRole {
    type Error = anyhow::Error;
    fn try_from(s: &str) -> anyhow::Result<Self> {
        match s {
            "admin" => Ok(UserRole::Admin),
            "member" => Ok(UserRole::Member),
            other => anyhow::bail!("unknown role: {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserStatus {
    Active,
    Deactivated,
    Deleted,
}

impl UserStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserStatus::Active => "active",
            UserStatus::Deactivated => "deactivated",
            UserStatus::Deleted => "deleted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub role: UserRole,
    pub status: UserStatus,
    pub force_password_change: bool,
    pub timezone: String,
}

#[derive(Debug, Clone)]
pub struct UserSummary {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub role: UserRole,
    pub status: UserStatus,
    pub created_at: String,
}

fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    let role_str: String = row.get(4)?;
    let status_str: String = row.get(5)?;
    Ok(User {
        id: row.get(0)?,
        email: row.get(1)?,
        password_hash: row.get(2)?,
        display_name: row.get(3)?,
        role: UserRole::try_from(role_str.as_str()).unwrap_or(UserRole::Member),
        status: match status_str.as_str() {
            "active" => UserStatus::Active,
            "deactivated" => UserStatus::Deactivated,
            _ => UserStatus::Deleted,
        },
        force_password_change: row.get::<_, i64>(6)? != 0,
        timezone: row
            .get::<_, Option<String>>(7)?
            .unwrap_or_else(|| "UTC".into()),
    })
}

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<UserSummary> {
    let role_str: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    Ok(UserSummary {
        id: row.get(0)?,
        email: row.get(1)?,
        display_name: row.get(2)?,
        role: UserRole::try_from(role_str.as_str()).unwrap_or(UserRole::Member),
        status: match status_str.as_str() {
            "active" => UserStatus::Active,
            "deactivated" => UserStatus::Deactivated,
            _ => UserStatus::Deleted,
        },
        created_at: row.get(5)?,
    })
}

pub fn find_by_email(conn: &Connection, email: &str) -> anyhow::Result<Option<User>> {
    match conn.query_row(
        "SELECT id, email, password_hash, display_name, role, status, force_password_change, timezone
         FROM users WHERE email = ?1 AND status != 'deleted'",
        [email],
        row_to_user,
    ) {
        Ok(u) => Ok(Some(u)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::Error::from(e)),
    }
}

pub fn find_by_id(conn: &Connection, id: &str) -> anyhow::Result<Option<User>> {
    match conn.query_row(
        "SELECT id, email, password_hash, display_name, role, status, force_password_change, timezone
         FROM users WHERE id = ?1 AND status = 'active'",
        [id],
        row_to_user,
    ) {
        Ok(u) => Ok(Some(u)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::Error::from(e)),
    }
}

pub fn list_users(conn: &Connection) -> anyhow::Result<Vec<UserSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, email, display_name, role, status, created_at
         FROM users WHERE status != 'deleted'
         ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([], row_to_summary)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn create_admin(
    conn: &Connection,
    email: &str,
    password_hash: &str,
    display_name: Option<&str>,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO users
             (id, email, password_hash, display_name, role, status,
              force_password_change, timezone, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'admin', 'active', 0, 'UTC', ?5, ?5)",
        rusqlite::params![id, email, password_hash, display_name, now],
    )
    .context("failed to create admin user")?;
    Ok(id)
}

pub fn create_member(
    conn: &Connection,
    email: &str,
    password_hash: &str,
    display_name: Option<&str>,
    created_by: &str,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO users
             (id, email, password_hash, display_name, role, status,
              force_password_change, timezone, created_by, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'member', 'active', 1, 'UTC', ?5, ?6, ?6)",
        rusqlite::params![id, email, password_hash, display_name, created_by, now],
    )
    .context("failed to create member user")?;
    Ok(id)
}

pub fn set_status(conn: &Connection, user_id: &str, status: UserStatus) -> anyhow::Result<bool> {
    let now = Utc::now().to_rfc3339();
    let n = conn.execute(
        "UPDATE users SET status = ?1, updated_at = ?2 WHERE id = ?3 AND status != 'deleted'",
        rusqlite::params![status.as_str(), now, user_id],
    )?;
    Ok(n > 0)
}

pub fn set_password(conn: &Connection, user_id: &str, password_hash: &str) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE users SET password_hash = ?1, force_password_change = 0, updated_at = ?2
         WHERE id = ?3",
        rusqlite::params![password_hash, now, user_id],
    )
    .context("failed to update password")?;
    Ok(())
}

pub fn update_me(
    conn: &Connection,
    user_id: &str,
    display_name: Option<&str>,
    timezone: Option<&str>,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE users SET
             display_name = COALESCE(?1, display_name),
             timezone     = COALESCE(?2, timezone),
             updated_at   = ?3
         WHERE id = ?4",
        rusqlite::params![display_name, timezone, now, user_id],
    )
    .context("failed to update user profile")?;
    Ok(())
}

pub fn email_exists(conn: &Connection, email: &str) -> anyhow::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM users WHERE email = ?1 AND status != 'deleted'",
        [email],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::open_test_db;

    #[test]
    fn create_and_find_admin() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let id = create_admin(conn, "admin@example.com", "hash", Some("Admin"))?;
            let user = find_by_id(conn, &id)?.expect("user should exist");
            assert_eq!(user.email, "admin@example.com");
            assert_eq!(user.role, UserRole::Admin);
            assert_eq!(user.display_name.as_deref(), Some("Admin"));
            assert_eq!(user.timezone, "UTC");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn create_member_sets_force_password_change() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let admin_id = create_admin(conn, "admin@example.com", "hash", None)?;
            let id = create_member(conn, "member@example.com", "hash", None, &admin_id)?;
            let user = find_by_id(conn, &id)?.expect("user should exist");
            assert!(user.force_password_change);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn set_status_deactivates_user() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let id = create_admin(conn, "admin@example.com", "hash", None)?;
            let changed = set_status(conn, &id, UserStatus::Deactivated)?;
            assert!(changed);
            let user = find_by_id(conn, &id)?;
            assert!(
                user.is_none(),
                "deactivated user must not be found by find_by_id"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn update_me_changes_timezone() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let id = create_admin(conn, "tz@example.com", "hash", None)?;
            update_me(conn, &id, None, Some("America/New_York"))?;
            let user = find_by_id(conn, &id)?.unwrap();
            assert_eq!(user.timezone, "America/New_York");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn find_by_email_returns_none_for_unknown() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let u = find_by_email(conn, "nobody@example.com")?;
            assert!(u.is_none());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn find_by_id_returns_none_for_unknown() {
        let pool = open_test_db();
        pool.call_sync(|conn| {
            let u = find_by_id(conn, "no-such-id")?;
            assert!(u.is_none());
            Ok(())
        })
        .unwrap();
    }
}
