use std::sync::Arc;

use crate::config::{ProviderConfig, ProviderType};

use super::{
    anthropic::AnthropicProvider,
    ollama::OllamaProvider,
    openai_compatible::{OpenAiCompatibleProvider, OutputTokenField},
    reliable::ReliableProvider,
    traits::ChatProvider,
};

const DEFAULT_MAX_RETRIES: u32 = 3;
const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";

/// Builds a `ChatProvider` from a config entry, wrapped in `ReliableProvider`.
pub fn build(config: &ProviderConfig) -> anyhow::Result<Arc<dyn ChatProvider>> {
    let inner: Arc<dyn ChatProvider> = match config.provider_type {
        ProviderType::Ollama => {
            let base_url = config.url.as_deref().unwrap_or("http://localhost:11434");
            Arc::new(OllamaProvider::new(
                &config.id,
                &config.name,
                base_url,
                &config.default_model,
                config.num_ctx,
                config.num_predict,
            ))
        }
        ProviderType::Openai => Arc::new(OpenAiCompatibleProvider::new(
            &config.id,
            &config.name,
            config.url.as_deref().unwrap_or(OPENAI_BASE_URL),
            Some(required_api_key(config)?),
            &config.default_model,
            config.num_ctx,
            config.num_predict,
            OutputTokenField::MaxCompletionTokens,
        )),
        ProviderType::Deepseek => Arc::new(OpenAiCompatibleProvider::new(
            &config.id,
            &config.name,
            config.url.as_deref().unwrap_or(DEEPSEEK_BASE_URL),
            Some(required_api_key(config)?),
            &config.default_model,
            config.num_ctx,
            config.num_predict,
            OutputTokenField::MaxTokens,
        )),
        ProviderType::OpenaiCompatible => Arc::new(OpenAiCompatibleProvider::new(
            &config.id,
            &config.name,
            config
                .url
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("base URL is required"))?,
            config.api_key.as_deref(),
            &config.default_model,
            config.num_ctx,
            config.num_predict,
            OutputTokenField::MaxTokens,
        )),
        ProviderType::Anthropic => Arc::new(AnthropicProvider::new(
            &config.id,
            &config.name,
            config.url.as_deref(),
            required_api_key(config)?,
            &config.default_model,
            config.num_ctx,
            config.num_predict,
        )),
    };

    Ok(Arc::new(ReliableProvider::new(inner, DEFAULT_MAX_RETRIES)))
}

fn required_api_key(config: &ProviderConfig) -> anyhow::Result<&str> {
    config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("API key is required"))
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
    fn hosted_provider_without_key_returns_error() {
        let mut cfg = ollama_config();
        cfg.provider_type = ProviderType::Anthropic;
        cfg.url = None;
        let result = build(&cfg);
        assert!(result.is_err());
    }

    #[test]
    fn builds_all_hosted_provider_types() {
        for provider_type in [
            ProviderType::Openai,
            ProviderType::Deepseek,
            ProviderType::Anthropic,
        ] {
            let mut cfg = ollama_config();
            cfg.provider_type = provider_type;
            cfg.api_key = Some("secret".to_string());
            cfg.url = None;
            assert!(build(&cfg).is_ok());
        }
    }

    #[test]
    fn builds_openai_compatible_provider() {
        let mut cfg = ollama_config();
        cfg.provider_type = ProviderType::OpenaiCompatible;
        cfg.api_key = None;
        cfg.url = Some("http://localhost:1234/v1".to_string());
        assert!(build(&cfg).is_ok());
    }
}
