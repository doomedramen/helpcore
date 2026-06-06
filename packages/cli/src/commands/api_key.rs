use crate::{client::Client, config::Credentials};

pub async fn list(server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    let keys = client.list_api_keys(token).await?;
    if keys.is_empty() {
        eprintln!("(no API keys)");
        return Ok(());
    }
    for key in &keys {
        let used = key
            .last_used_at
            .as_deref()
            .map(|s| format!("  last used {}", &s[..10]))
            .unwrap_or_default();
        let expires = key
            .expires_at
            .as_deref()
            .map(|s| format!("  expires {}", &s[..10]))
            .unwrap_or_default();
        println!(
            "{:<36}  {:<24}  {}…{}{}",
            key.id, key.name, key.key_prefix, used, expires
        );
    }
    Ok(())
}

pub async fn create(name: &str, server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    let created = client.create_api_key(token, name).await?;
    println!("{}", created.key);
    eprintln!("id:     {}", created.id);
    eprintln!("prefix: {}…", created.key_prefix);
    eprintln!("Store this key securely — it will not be shown again.");
    Ok(())
}

pub async fn revoke(key_id: &str, server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    client.revoke_api_key(token, key_id).await?;
    eprintln!("✓ Key {key_id} revoked.");
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
