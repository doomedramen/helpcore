use anyhow::Context;
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Stored in `~/.helpcore/credentials` (mode 600).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Credentials {
    pub server_url: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

impl Credentials {
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self).context("failed to serialise credentials")?;
        std::fs::write(&path, &content)
            .with_context(|| format!("failed to write {}", path.display()))?;

        // Restrict to owner read/write only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    pub fn clear() -> anyhow::Result<()> {
        let path = Self::path();
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
        }
        Ok(())
    }

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
        // SAFETY: single-threaded test, no concurrent env readers.
        unsafe { std::env::set_var("HELPCORE_CREDENTIALS", path.to_str().unwrap()) };

        let creds = Credentials {
            server_url: Some("http://localhost:3000".to_string()),
            access_token: Some("tok_access".to_string()),
            refresh_token: Some("tok_refresh".to_string()),
        };
        creds.save().unwrap();
        let loaded = Credentials::load().unwrap();
        assert_eq!(loaded.server_url.as_deref(), Some("http://localhost:3000"));
        assert_eq!(loaded.access_token.as_deref(), Some("tok_access"));

        unsafe { std::env::remove_var("HELPCORE_CREDENTIALS") };
    }

    #[test]
    fn load_returns_default_when_missing() {
        // SAFETY: single-threaded test, no concurrent env readers.
        unsafe { std::env::set_var("HELPCORE_CREDENTIALS", "/tmp/no_such_helpcore_creds_xyz") };
        let creds = Credentials::load().unwrap();
        assert!(creds.server_url.is_none());
        unsafe { std::env::remove_var("HELPCORE_CREDENTIALS") };
    }

    #[test]
    fn resolve_server_flag_wins() {
        let creds = Credentials { server_url: Some("http://stored".to_string()), ..Default::default() };
        let resolved = creds.resolve_server(Some("http://flag"));
        assert_eq!(resolved.as_deref(), Some("http://flag"));
    }
}
