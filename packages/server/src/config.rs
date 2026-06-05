use anyhow::Context;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    #[serde(default)]
    pub data: DataConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub plugins: PluginsConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct PluginsConfig {
    /// Plugin IDs that cannot be installed on this server.
    #[serde(default)]
    pub blacklist: Vec<String>,
    /// Local plugins loaded from the filesystem at startup.
    /// Useful for development and self-hosted plugins.
    #[serde(default)]
    pub local: Vec<LocalPluginConfig>,
}

/// A plugin shipped alongside the helpcore installation or referenced by path.
#[derive(Debug, Deserialize, Clone)]
pub struct LocalPluginConfig {
    /// Must match the `id` field in the plugin's manifest.toml.
    pub id: String,
    /// Filesystem path to the plugin directory (containing manifest.toml).
    pub path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub url: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    3000
}

#[derive(Debug, Deserialize, Default)]
pub struct DataConfig {
    dir: Option<String>,
}

impl DataConfig {
    pub fn resolved_dir(&self) -> PathBuf {
        // HELPCORE_DATA env var takes precedence (used by Docker Compose).
        if let Ok(env_dir) = std::env::var("HELPCORE_DATA") {
            return PathBuf::from(env_dir);
        }
        match &self.dir {
            Some(dir) => PathBuf::from(shellexpand::tilde(dir.as_str()).as_ref()),
            None => default_data_dir(),
        }
    }
}

fn default_data_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().join(".helpcore").join("data"))
        .unwrap_or_else(|| PathBuf::from(".helpcore/data"))
}

#[derive(Debug, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self { level: default_log_level() }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    pub api_key: Option<String>,
    pub url: Option<String>,
    pub default_model: String,
    #[serde(default)]
    pub roles: Vec<ProviderRole>,
    /// Context window size in tokens. Defaults to provider-specific values
    /// when omitted. Set this explicitly for small models: most 3B models
    /// have a 4096-token window; the server default of 8192 wastes RAM.
    pub num_ctx: Option<u32>,
    /// Maximum number of tokens to generate per response. Leave unset to use
    /// the provider's default.
    pub num_predict: Option<u32>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Anthropic,
    Openai,
    Ollama,
    OpenaiCompatible,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRole {
    Chat,
    Code,
    ImageGen,
    VideoGen,
    Embeddings,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config from {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("failed to parse config from {}", path.display()))
    }

    pub fn config_path() -> PathBuf {
        if let Ok(path) = std::env::var("HELPCORE_CONFIG") {
            return PathBuf::from(path);
        }
        directories::BaseDirs::new()
            .map(|d| d.home_dir().join(".helpcore").join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> anyhow::Result<Config> {
        toml::from_str(toml).map_err(anyhow::Error::from)
    }

    #[test]
    fn minimal_config_parses() {
        let cfg = parse(
            r#"
            [server]
            name = "My helpcore"
            url  = "http://localhost:3000"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.server.name, "My helpcore");
        assert_eq!(cfg.server.url, "http://localhost:3000");
        assert_eq!(cfg.server.port, 3000);
        assert!(cfg.providers.is_empty());
        assert_eq!(cfg.logging.level, "info");
    }

    #[test]
    fn custom_port_and_log_level() {
        let cfg = parse(
            r#"
            [server]
            name = "Test"
            url  = "http://localhost:8080"
            port = 8080

            [logging]
            level = "debug"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.logging.level, "debug");
    }

    #[test]
    fn provider_config_parses() {
        let cfg = parse(
            r#"
            [server]
            name = "Test"
            url  = "http://localhost:3000"

            [[providers]]
            id            = "anthropic-main"
            name          = "Anthropic Claude"
            type          = "anthropic"
            api_key       = "sk-ant-test"
            default_model = "claude-opus-4-5"
            roles         = ["chat"]
            "#,
        )
        .unwrap();
        assert_eq!(cfg.providers.len(), 1);
        let p = &cfg.providers[0];
        assert_eq!(p.id, "anthropic-main");
        assert_eq!(p.provider_type, ProviderType::Anthropic);
        assert_eq!(p.roles, vec![ProviderRole::Chat]);
        assert_eq!(p.api_key.as_deref(), Some("sk-ant-test"));
    }

    #[test]
    fn multiple_providers_parse() {
        let cfg = parse(
            r#"
            [server]
            name = "Test"
            url  = "http://localhost:3000"

            [[providers]]
            id            = "anthropic"
            name          = "Anthropic"
            type          = "anthropic"
            api_key       = "sk-ant-test"
            default_model = "claude-opus-4-5"
            roles         = ["chat", "code"]

            [[providers]]
            id            = "ollama-local"
            name          = "Ollama"
            type          = "ollama"
            url           = "http://localhost:11434"
            default_model = "llama3"
            roles         = ["chat"]
            "#,
        )
        .unwrap();
        assert_eq!(cfg.providers.len(), 2);
        assert_eq!(cfg.providers[1].provider_type, ProviderType::Ollama);
        assert_eq!(cfg.providers[1].url.as_deref(), Some("http://localhost:11434"));
    }

    #[test]
    fn missing_server_section_fails() {
        assert!(parse("[data]\ndir = \"/tmp\"").is_err());
    }

    #[test]
    fn data_dir_defaults_when_omitted() {
        let cfg = parse(
            r#"
            [server]
            name = "Test"
            url  = "http://localhost:3000"
            "#,
        )
        .unwrap();
        let dir = cfg.data.resolved_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn data_dir_expands_tilde() {
        let cfg = parse(
            r#"
            [server]
            name = "Test"
            url  = "http://localhost:3000"

            [data]
            dir = "~/custom/data"
            "#,
        )
        .unwrap();
        let dir = cfg.data.resolved_dir();
        assert!(!dir.to_string_lossy().contains('~'));
    }
}
