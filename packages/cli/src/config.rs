//! Credential store persisted to `~/.helpcore/credentials` with restrictive
//! file permissions (Unix mode 600).

use anyhow::Context;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Stored in `~/.helpcore/credentials` (mode 600).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Credentials {
    /// Base URL of the helpcore server (e.g. `http://localhost:3000`).
    pub server_url: Option<String>,
    /// Bearer access token for API requests.
    pub access_token: Option<String>,
    /// Long-lived refresh token for obtaining new access tokens.
    pub refresh_token: Option<String>,
}

impl Credentials {
    /// Loads credentials from the default location (`~/.helpcore/credentials`).
    pub fn load() -> anyhow::Result<Self> {
        Self::load_from(&Self::path())
    }

    /// Loads credentials from a specific `path`. Returns defaults if the file
    /// does not exist.
    pub fn load_from(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
    }

    /// Persists the credentials to the default path. Parent directories are
    /// created if needed, and the file is set to mode 600 on Unix.
    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&Self::path())
    }

    /// Persists the credentials to a specific `path`.
    pub fn save_to(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self).context("failed to serialise credentials")?;

        // On Unix, create the file with owner-only permissions from the very
        // first `open()` so the access/refresh tokens are never briefly
        // readable by other local users — writing first and `chmod`ing after
        // (the previous approach) leaves a TOCTOU window where the file is
        // created at the default mode (typically 0644 under umask 022).
        #[cfg(unix)]
        {
            use std::io::Write as _;
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)
                .with_context(|| format!("failed to open {}", path.display()))?;
            file.write_all(content.as_bytes())
                .with_context(|| format!("failed to write {}", path.display()))?;
            // `mode()` only governs newly-created files (subject to umask); if
            // the file already existed with looser permissions, tighten them.
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("failed to chmod {}", path.display()))?;
        }

        #[cfg(not(unix))]
        {
            std::fs::write(path, &content)
                .with_context(|| format!("failed to write {}", path.display()))?;
        }

        Ok(())
    }

    /// Removes the credentials file from disk if it exists.
    pub fn clear() -> anyhow::Result<()> {
        let path = Self::path();
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
        Ok(())
    }

    /// Returns the default credentials path. Respects the `HELPCORE_CREDENTIALS`
    /// env var; otherwise falls back to `~/.helpcore/credentials`.
    pub fn path() -> PathBuf {
        if let Ok(p) = std::env::var("HELPCORE_CREDENTIALS") {
            return PathBuf::from(p);
        }
        BaseDirs::new()
            .map(|d| d.home_dir().join(".helpcore").join("credentials"))
            .unwrap_or_else(|| PathBuf::from(".helpcore/credentials"))
    }

    /// Resolves the server base URL from (in priority order):
    /// `flag` argument → `HELPCORE_SERVER` env var → stored credentials
    pub fn resolve_server(&self, flag: Option<&str>) -> Option<String> {
        flag.map(|s| s.trim_end_matches('/').to_string())
            .or_else(|| std::env::var("HELPCORE_SERVER").ok())
            .or_else(|| self.server_url.clone())
    }

    /// Returns `true` when an access token is present.
    pub fn is_logged_in(&self) -> bool {
        self.access_token.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trips_credentials() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("credentials");

        let creds = Credentials {
            server_url: Some("http://localhost:3000".to_string()),
            access_token: Some("tok_access".to_string()),
            refresh_token: Some("tok_refresh".to_string()),
        };
        creds.save_to(&path).unwrap();
        let loaded = Credentials::load_from(&path).unwrap();
        assert_eq!(loaded.server_url.as_deref(), Some("http://localhost:3000"));
        assert_eq!(loaded.access_token.as_deref(), Some("tok_access"));
        assert_eq!(loaded.refresh_token.as_deref(), Some("tok_refresh"));
    }

    #[cfg(unix)]
    #[test]
    fn save_to_creates_file_with_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let path = dir.path().join("credentials");

        let creds = Credentials {
            server_url: Some("http://localhost:3000".to_string()),
            access_token: Some("tok_access".to_string()),
            refresh_token: Some("tok_refresh".to_string()),
        };
        creds.save_to(&path).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "credentials file must be owner-read/write only, got {mode:o}"
        );

        // Re-saving over an existing file (e.g. created with looser
        // permissions by an older binary) must still tighten the mode.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        creds.save_to(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn load_returns_default_when_missing() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("no_such_file");
        let creds = Credentials::load_from(&path).unwrap();
        assert!(creds.server_url.is_none());
        assert!(creds.access_token.is_none());
    }

    #[test]
    fn resolve_server_flag_wins() {
        let creds = Credentials {
            server_url: Some("http://stored".to_string()),
            ..Default::default()
        };
        let resolved = creds.resolve_server(Some("http://flag"));
        assert_eq!(resolved.as_deref(), Some("http://flag"));
    }
}
