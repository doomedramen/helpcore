use crate::{client::Client, config::Credentials};

pub async fn list(server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    let plugins = client.list_plugins(token).await?;
    if plugins.is_empty() {
        eprintln!("(no plugins installed)");
        return Ok(());
    }
    for p in &plugins {
        let state = if p.enabled { "enabled" } else { "disabled" };
        println!("{:<24} v{}  [{}]  {}", p.id, p.active_version, p.tier, state);
    }
    Ok(())
}

pub async fn token(
    plugin_id: &str,
    permissions: Vec<String>,
    server_flag: Option<&str>,
) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let tok = require_token(&creds)?;
    let client = Client::new(&server);

    let resp = client.create_plugin_token(tok, plugin_id, permissions).await?;

    println!("{}", resp.token);
    eprintln!("token id: {}", resp.token_id);
    eprintln!("Store this token in the plugin's HELPCORE_TOKEN environment variable.");
    eprintln!("It will not be shown again.");
    Ok(())
}

pub async fn enable(
    plugin_id: &str,
    enabled: bool,
    server_flag: Option<&str>,
) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let tok = require_token(&creds)?;
    let client = Client::new(&server);

    client.set_plugin_enabled(tok, plugin_id, enabled).await?;
    let action = if enabled { "enabled" } else { "disabled" };
    eprintln!("✓ {plugin_id} {action}");
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
