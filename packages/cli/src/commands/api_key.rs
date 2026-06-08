//! `hc api-keys` — list, create, and revoke API keys.

use crate::{client::Client, config::Credentials};

/// Returns the leading "date" portion (up to 10 chars) of an RFC-3339-ish
/// timestamp for display, without panicking on short strings or strings whose
/// 10th byte falls inside a multi-byte UTF-8 character. The server is expected
/// to send RFC-3339 (ASCII) timestamps, but this must not crash on a malformed
/// or unexpected response from any server the user points the CLI at.
fn date_only(s: &str) -> &str {
    match s.char_indices().nth(10) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Lists API keys with their ID, name, prefix, and optional last-used/expiry dates.
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
            .map(|s| format!("  last used {}", date_only(s)))
            .unwrap_or_default();
        let expires = key
            .expires_at
            .as_deref()
            .map(|s| format!("  expires {}", date_only(s)))
            .unwrap_or_default();
        println!(
            "{:<36}  {:<24}  {}…{}{}",
            key.id, key.name, key.key_prefix, used, expires
        );
    }
    Ok(())
}

/// Creates a new API key and prints the full key (shown only once).
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

/// Revokes an API key by its ID.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_only_truncates_normal_rfc3339() {
        assert_eq!(date_only("2026-06-08T12:30:00Z"), "2026-06-08");
    }

    #[test]
    fn date_only_does_not_panic_on_short_strings() {
        assert_eq!(date_only(""), "");
        assert_eq!(date_only("2026"), "2026");
        assert_eq!(date_only("2026-06"), "2026-06");
    }

    #[test]
    fn date_only_does_not_panic_on_multibyte_strings() {
        // A malicious/buggy server could send anything here — make sure a
        // multi-byte character straddling byte offset 10 can't cause a
        // "byte index is not a char boundary" panic.
        let s = "123456789日本語テスト";
        // Should not panic, and should return a prefix on a char boundary.
        let truncated = date_only(s);
        assert!(s.starts_with(truncated));
    }
}
