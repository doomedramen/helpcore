use std::sync::Arc;

use crate::config::{ProviderConfig, ProviderType};

use super::{ollama::OllamaProvider, reliable::ReliableProvider, traits::ChatProvider};

const DEFAULT_MAX_RETRIES: u32 = 3;

/// Builds a `ChatProvider` from a config entry, wrapped in `ReliableProvider`.
pub fn build(config: &ProviderConfig) -> anyhow::Result<Arc<dyn ChatProvider>> {
    let inner: Arc<dyn ChatProvider> = match config.provider_type {
        ProviderType::Ollama => {
            let base_url = config
                .url
                .as_deref()
                .unwrap_or("http://localhost:11434");
            Arc::new(OllamaProvider::new(
                &config.id,
                &config.name,
                base_url,
                &config.default_model,
                config.num_ctx,
                config.num_predict,
            ))
        }
        ref t => anyhow::bail!("provider type {:?} is not yet implemented", t),
    };

    Ok(Arc::new(ReliableProvider::new(inner, DEFAULT_MAX_RETRIES)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderConfig, ProviderRole, ProviderType};

    fn ollama_config() -> ProviderConfig {
        ProviderConfig {
            id: "ollama-local".to_string(),
            name: "Ollama".to_string(),
            provider_type: ProviderType::Ollama,
            api_key: None,
            url: Some("http://localhost:11434".to_string()),
            default_model: "llama3".to_string(),
            roles: vec![ProviderRole::Chat],
            num_ctx: None,
            num_predict: None,
        }
    }

    #[test]
    fn builds_ollama_provider() {
        let provider = build(&ollama_config()).unwrap();
        assert_eq!(provider.id(), "ollama-local");
        assert_eq!(provider.default_model(), "llama3");
    }

    #[test]
    fn unknown_type_returns_error() {
        let mut cfg = ollama_config();
        cfg.provider_type = ProviderType::Anthropic; // not yet implemented
        let result = build(&cfg);
        assert!(result.is_err());
    }
}
