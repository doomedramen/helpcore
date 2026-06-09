#![allow(missing_docs)]

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Default URL for the plugin registry, pointing to the latest plugins release on GitHub.
pub const DEFAULT_REGISTRY_URL: &str =
    "https://github.com/doomedramen/helpcore-plugins/releases/download/plugins-latest/plugins.json";

/// Top-level configuration loaded from `config.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    #[serde(default)]
    pub registry: RegistryConfig,
    #[serde(default)]
    pub sandbox: SandboxConfig,
}

/// Docker sandbox configuration for safe command execution.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SandboxConfig {
    /// Whether the sandbox is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Docker image to use for sandbox containers.
    /// Defaults to ubuntu:latest.
    #[serde(default = "default_sandbox_image")]
    pub image: String,

    /// Max execution time in seconds per command.
    #[serde(default = "default_sandbox_timeout")]
    pub timeout: u64,

    /// Max memory in MB.
    #[serde(default = "default_sandbox_memory")]
    pub memory_mb: u64,

    /// Optional Docker host URL (e.g. unix:///var/run/docker.sock or tcp://1.2.3.4:2375).
    /// If omitted, defaults to local system defaults (respecting DOCKER_HOST env var).
    pub host: Option<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            image: default_sandbox_image(),
            timeout: default_sandbox_timeout(),
            memory_mb: default_sandbox_memory(),
            host: None,
        }
    }
}

fn default_sandbox_image() -> String {
    "ubuntu:latest".into()
}
fn default_sandbox_timeout() -> u64 {
    120
}
fn default_sandbox_memory() -> u64 {
    512
}

/// Plugin management configuration including blacklists and local plugins.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
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
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LocalPluginConfig {
    /// Must match the `id` field in the plugin's manifest.toml.
    pub id: String,
    /// Filesystem path to the plugin directory (containing manifest.toml).
    pub path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Server identity and networking configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub name: String,
    pub url: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    3000
}

/// Persistent data directory configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DataConfig {
    pub dir: Option<String>,
}

impl DataConfig {
    /// Resolves the data directory, preferring the `HELPCORE_DATA` env var,
    /// then the configured `dir`, then the OS-specific default.
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

/// Logging verbosity configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

/// Configuration for a single AI provider: credentials, model defaults, and capabilities.
#[derive(Debug, Deserialize, Serialize, Clone)]
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

/// Supported AI provider backends.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Anthropic,
    Deepseek,
    #[serde(alias = "chatgpt")]
    Openai,
    Ollama,
    OpenaiCompatible,
}

impl ProviderType {
    /// Returns the lowercase `snake_case` string representation of this variant.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::Deepseek => "deepseek",
            Self::Openai => "openai",
            Self::Ollama => "ollama",
            Self::OpenaiCompatible => "openai_compatible",
        }
    }
}

impl TryFrom<&str> for ProviderType {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> anyhow::Result<Self> {
        match value {
            "anthropic" => Ok(Self::Anthropic),
            "deepseek" => Ok(Self::Deepseek),
            "chatgpt" | "openai" => Ok(Self::Openai),
            "ollama" => Ok(Self::Ollama),
            "openai_compatible" => Ok(Self::OpenaiCompatible),
            other => anyhow::bail!("unknown provider type: {other}"),
        }
    }
}

/// Roles a provider can fulfill, determining which features it powers.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRole {
    Chat,
    Code,
    ImageGen,
    VideoGen,
    Embeddings,
}

impl ProviderRole {
    /// Returns the lowercase `snake_case` string representation of this variant.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Code => "code",
            Self::ImageGen => "image_gen",
            Self::VideoGen => "video_gen",
            Self::Embeddings => "embeddings",
        }
    }
}

impl TryFrom<&str> for ProviderRole {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> anyhow::Result<Self> {
        match value {
            "chat" => Ok(Self::Chat),
            "code" => Ok(Self::Code),
            "image_gen" => Ok(Self::ImageGen),
            "video_gen" => Ok(Self::VideoGen),
            "embeddings" => Ok(Self::Embeddings),
            other => anyhow::bail!("unknown provider role: {other}"),
        }
    }
}

/// Plugin registry configuration, specifying where to fetch available plugins.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryConfig {
    #[serde(default = "default_registry_url")]
    pub url: String,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: default_registry_url(),
        }
    }
}

fn default_registry_url() -> String {
    DEFAULT_REGISTRY_URL.to_string()
}

impl Config {
    /// Loads configuration from a TOML file, applying any necessary migrations.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config from {}", path.display()))?;
        let mut cfg: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse config from {}", path.display()))?;
        cfg.migrate(path);
        Ok(cfg)
    }

    fn migrate(&mut self, path: &Path) {
        const OLD_REGISTRY_URL: &str =
            "https://raw.githubusercontent.com/doomedramen/helpcore/main/registry/plugins.json";
        const STALE_REGISTRY_URL: &str = "https://raw.githubusercontent.com/doomedramen/helpcore-plugins/main/registry/plugins.json";
        if self.registry.url == OLD_REGISTRY_URL || self.registry.url == STALE_REGISTRY_URL {
            self.registry.url = DEFAULT_REGISTRY_URL.to_string();
            let _ = self.save(path);
        }
    }

    /// Migrate provider IDs from user-chosen strings to UUIDs.
    /// Returns a map of old_id → new_id for providers that were migrated.
    /// Updates config.toml in place.
    pub fn migrate_provider_ids(
        path: &Path,
    ) -> anyhow::Result<std::collections::HashMap<String, String>> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config from {}", path.display()))?;
        let mut cfg: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse config from {}", path.display()))?;

        let mut mapping = std::collections::HashMap::new();
        let mut changed = false;

        for provider in &mut cfg.providers {
            if uuid::Uuid::parse_str(&provider.id).is_err() {
                let old_id = provider.id.clone();
                let new_id = uuid::Uuid::new_v4().to_string();
                provider.id = new_id.clone();
                mapping.insert(old_id, new_id);
                changed = true;
            }
        }

        if changed {
            cfg.save(path)?;
        }

        Ok(mapping)
    }

    /// Returns the config file path, respecting `HELPCORE_CONFIG`,
    /// falling back to `~/.helpcore/config.toml`.
    pub fn config_path() -> PathBuf {
        if let Ok(path) = std::env::var("HELPCORE_CONFIG") {
            return PathBuf::from(path);
        }
        directories::BaseDirs::new()
            .map(|d| d.home_dir().join(".helpcore").join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("config.toml"))
    }

    /// Validates server, provider, registry, and logging settings.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.server.name.trim().is_empty() {
            anyhow::bail!("server name cannot be empty");
        }
        if self.server.url.trim().is_empty() {
            anyhow::bail!("server URL cannot be empty");
        }
        if self.server.port == 0 {
            anyhow::bail!("server port must be greater than zero");
        }
        if !matches!(
            self.logging.level.as_str(),
            "trace" | "debug" | "info" | "warn" | "error"
        ) {
            anyhow::bail!("logging level must be trace, debug, info, warn, or error");
        }
        if self.registry.url.trim().is_empty() {
            anyhow::bail!("registry URL cannot be empty");
        }

        let mut ids = HashSet::new();
        for provider in &self.providers {
            if provider.id.trim().is_empty() {
                anyhow::bail!("provider ID cannot be empty");
            }
            if !ids.insert(provider.id.as_str()) {
                anyhow::bail!("provider IDs must be unique");
            }
            if provider.name.trim().is_empty() {
                anyhow::bail!("provider {} name cannot be empty", provider.id);
            }
            if provider.default_model.trim().is_empty() {
                anyhow::bail!("provider {} default model cannot be empty", provider.id);
            }
            if provider.roles.is_empty() {
                anyhow::bail!("provider {} must have at least one role", provider.id);
            }
            if provider.num_ctx == Some(0) {
                anyhow::bail!(
                    "provider {} context tokens must be greater than zero",
                    provider.id
                );
            }
            if provider.num_predict == Some(0) {
                anyhow::bail!(
                    "provider {} maximum response tokens must be greater than zero",
                    provider.id
                );
            }
            if matches!(
                provider.provider_type,
                ProviderType::Anthropic | ProviderType::Deepseek | ProviderType::Openai
            ) && provider
                .api_key
                .as_deref()
                .is_none_or(|key| key.trim().is_empty())
            {
                anyhow::bail!("provider {} API key is required", provider.id);
            }
            if provider.provider_type == ProviderType::OpenaiCompatible
                && provider
                    .url
                    .as_deref()
                    .is_none_or(|url| url.trim().is_empty())
            {
                anyhow::bail!("provider {} base URL is required", provider.id);
            }
            if let Some(url) = provider.url.as_deref() {
                let parsed = reqwest::Url::parse(url)
                    .with_context(|| format!("provider {} base URL is invalid", provider.id))?;
                if !matches!(parsed.scheme(), "http" | "https") {
                    anyhow::bail!("provider {} base URL must use http or https", provider.id);
                }
            }
        }

        if self.sandbox.image.trim().is_empty() {
            anyhow::bail!("sandbox image cannot be empty");
        }
        if self.sandbox.timeout == 0 {
            anyhow::bail!("sandbox timeout must be greater than zero");
        }
        if self.sandbox.memory_mb < 64 {
            anyhow::bail!("sandbox memory limit must be at least 64MB");
        }

        Ok(())
    }

    /// Atomically writes the configuration to disk after validation.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let content = toml::to_string_pretty(self).context("failed to serialise config")?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.toml");
        let temp_path = parent.join(format!(".{file_name}.tmp-{}", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut temp = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp_path)
                .with_context(|| {
                    format!("failed to write temporary config {}", temp_path.display())
                })?;
            temp.write_all(content.as_bytes())?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                temp.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            }

            temp.flush()?;
            temp.sync_all()?;
            std::fs::rename(&temp_path, path)
                .with_context(|| format!("failed to replace config {}", path.display()))?;
            std::fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp_path);
        }
        result
    }

    /// Checks whether the config file and its parent directory are writable.
    pub fn writability(path: &Path) -> Result<(), String> {
        if path.is_dir() {
            return Err(format!(
                "{} is a directory, not a config file",
                path.display()
            ));
        }
        if path.exists()
            && let Err(error) = std::fs::OpenOptions::new().write(true).open(path)
        {
            return Err(format!("{} is not writable: {error}", path.display()));
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        if !parent.exists() {
            return Err(format!(
                "config directory {} does not exist",
                parent.display()
            ));
        }
        let probe = parent.join(format!(".helpcore-write-test-{}", uuid::Uuid::new_v4()));
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&probe)
        {
            Ok(file) => {
                drop(file);
                let _ = std::fs::remove_file(probe);
                Ok(())
            }
            Err(error) => Err(format!(
                "config directory {} is not writable: {error}. Mount the config directory read-write or edit the file outside helpcore.",
                parent.display()
            )),
        }
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
        assert_eq!(cfg.registry.url, DEFAULT_REGISTRY_URL);
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
        assert_eq!(
            cfg.providers[1].url.as_deref(),
            Some("http://localhost:11434")
        );
    }

    #[test]
    fn hosted_provider_validation_requires_api_key() {
        let mut cfg = parse(
            r#"
            [server]
            name = "Test"
            url  = "http://localhost:3000"

            [[providers]]
            id            = "deepseek"
            name          = "DeepSeek"
            type          = "deepseek"
            default_model = "deepseek-chat"
            roles         = ["chat"]
            "#,
        )
        .unwrap();
        assert!(cfg.validate().is_err());

        cfg.providers[0].api_key = Some("secret".to_string());
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn openai_compatible_validation_requires_url() {
        let cfg = parse(
            r#"
            [server]
            name = "Test"
            url  = "http://localhost:3000"

            [[providers]]
            id            = "compatible"
            name          = "Compatible"
            type          = "openai_compatible"
            default_model = "local-model"
            roles         = ["chat"]
            "#,
        )
        .unwrap();
        assert!(cfg.validate().is_err());
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

    #[test]
    fn config_round_trips_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = parse(
            r#"
            [server]
            name = "Test"
            url = "http://localhost:3000"

            [registry]
            url = "https://example.com/plugins.json"
            "#,
        )
        .unwrap();

        cfg.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.registry.url, "https://example.com/plugins.json");
    }
}
