use std::sync::Arc;

use parking_lot::RwLock;

use crate::config::{ProviderConfig, ProviderRole};

use super::{factory, traits::ChatProvider};

#[derive(Clone)]
struct RuntimeProvider {
    provider: Arc<dyn ChatProvider>,
    roles: Vec<ProviderRole>,
}

pub struct PreparedProviders(Vec<RuntimeProvider>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub id: String,
    pub name: String,
    pub default_model: String,
}

#[derive(Clone, Default)]
pub struct ProviderRegistry {
    inner: Arc<RwLock<Vec<RuntimeProvider>>>,
}

impl ProviderRegistry {
    pub fn from_providers(providers: Vec<(Arc<dyn ChatProvider>, Vec<ProviderRole>)>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(
                providers
                    .into_iter()
                    .map(|(provider, roles)| RuntimeProvider { provider, roles })
                    .collect(),
            )),
        }
    }

    pub fn prepare(configs: &[ProviderConfig]) -> anyhow::Result<PreparedProviders> {
        let providers = configs
            .iter()
            .map(|config| {
                let provider = factory::build(config)
                    .map_err(|error| anyhow::anyhow!("provider {}: {error}", config.id))?;
                Ok(RuntimeProvider {
                    provider,
                    roles: config.roles.clone(),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(PreparedProviders(providers))
    }

    pub fn new(prepared: PreparedProviders) -> Self {
        Self {
            inner: Arc::new(RwLock::new(prepared.0)),
        }
    }

    pub fn replace(&self, prepared: PreparedProviders) {
        *self.inner.write() = prepared.0;
    }

    pub fn find_for_role(
        &self,
        provider_id: Option<&str>,
        role: ProviderRole,
    ) -> Option<Arc<dyn ChatProvider>> {
        self.inner
            .read()
            .iter()
            .find(|entry| {
                entry.roles.contains(&role)
                    && provider_id.is_none_or(|id| entry.provider.id() == id)
            })
            .map(|entry| Arc::clone(&entry.provider))
    }

    pub fn list_for_role(&self, role: ProviderRole) -> Vec<ProviderDescriptor> {
        self.inner
            .read()
            .iter()
            .filter(|entry| entry.roles.contains(&role))
            .map(|entry| ProviderDescriptor {
                id: entry.provider.id().to_string(),
                name: entry.provider.name().to_string(),
                default_model: entry.provider.default_model().to_string(),
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderType;

    fn config(id: &str, roles: Vec<ProviderRole>) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            name: id.to_string(),
            provider_type: ProviderType::Ollama,
            api_key: None,
            url: Some("http://localhost:11434".to_string()),
            default_model: "llama3".to_string(),
            roles,
            num_ctx: None,
            num_predict: None,
        }
    }

    #[test]
    fn filters_and_replaces_providers_by_role() {
        let registry = ProviderRegistry::new(
            ProviderRegistry::prepare(&[
                config("chat", vec![ProviderRole::Chat]),
                config("code", vec![ProviderRole::Code]),
            ])
            .unwrap(),
        );

        assert!(
            registry
                .find_for_role(Some("chat"), ProviderRole::Chat)
                .is_some()
        );
        assert!(
            registry
                .find_for_role(Some("code"), ProviderRole::Chat)
                .is_none()
        );
        assert_eq!(registry.list_for_role(ProviderRole::Chat)[0].id, "chat");

        let in_flight = registry
            .find_for_role(Some("chat"), ProviderRole::Chat)
            .unwrap();
        registry.replace(
            ProviderRegistry::prepare(&[config("new", vec![ProviderRole::Chat])]).unwrap(),
        );
        assert_eq!(in_flight.id(), "chat");
        assert!(
            registry
                .find_for_role(Some("chat"), ProviderRole::Chat)
                .is_none()
        );
        assert!(
            registry
                .find_for_role(Some("new"), ProviderRole::Chat)
                .is_some()
        );
    }
}
