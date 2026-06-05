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
    })
}

pub fn find_by_email(conn: &Connection, email: &str) -> anyhow::Result<Option<User>> {
    match conn.query_row(
        "SELECT id, email, password_hash, display_name, role, status, force_password_change
         FROM users WHERE email = ?1 AND status = 'active'",
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
        "SELECT id, email, password_hash, display_name, role, status, force_password_change
         FROM users WHERE id = ?1 AND status = 'active'",
        [id],
        row_to_user,
    ) {
        Ok(u) => Ok(Some(u)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(anyhow::Error::from(e)),
    }
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
              force_password_change, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'admin', 'active', 0, ?5, ?5)",
        rusqlite::params![id, email, password_hash, display_name, now],
    )
    .context("failed to create admin user")?;
    Ok(id)
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
