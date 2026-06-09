//! `hc admin` — server administration commands.

use helpcore_api::AdminConfigUpdateRequest;

use crate::{client::Client, config::Credentials};

/// Shows the current sandbox status.
pub async fn sandbox_status(server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    let config = client.get_admin_config(token).await?;
    let sandbox = config.sandbox;

    println!("Sandbox Enabled: {}", sandbox.enabled);
    println!("Docker Image:    {}", sandbox.image);
    println!("Timeout:         {}s", sandbox.timeout);
    println!("Memory Limit:    {}MB", sandbox.memory_mb);

    Ok(())
}

/// Enables or disables the sandbox.
pub async fn sandbox_enable(enabled: bool, server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    let mut config = client.get_admin_config(token).await?;
    config.sandbox.enabled = enabled;

    let request = AdminConfigUpdateRequest {
        server: config.server,
        logging_level: config.logging_level,
        registry_url: config.registry_url,
        plugin_blacklist: config.plugin_blacklist,
        providers: config
            .providers
            .into_iter()
            .map(|p| helpcore_api::AdminProviderUpdate {
                id: p.id,
                name: p.name,
                provider_type: p.provider_type,
                api_key: None,
                clear_api_key: false,
                url: p.url,
                default_model: p.default_model,
                roles: p.roles,
                num_ctx: p.num_ctx,
                num_predict: p.num_predict,
            })
            .collect(),
        sandbox: config.sandbox,
    };

    client.update_admin_config(token, &request).await?;

    let state = if enabled { "enabled" } else { "disabled" };
    eprintln!("✓ Sandbox {state}");

    Ok(())
}

fn resolve_server(creds: &Credentials, flag: Option<&str>) -> anyhow::Result<String> {
    creds
        .resolve_server(flag)
        .ok_or_else(|| anyhow::anyhow!("no server configured — run 'hc login' first"))
}

fn require_token(creds: &Credentials) -> anyhow::Result<&str> {
    creds
        .access_token
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("not logged in — run 'hc login' first"))
}
