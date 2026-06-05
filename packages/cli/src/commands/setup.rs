use anyhow::{Context, bail};

use crate::{client::Client, config::Credentials};

/// Runs the first-admin setup wizard.
///
/// If `setup_url` is provided (the full URL from server stdout, e.g.
/// `http://localhost:3000/setup?token=abc123`), the server URL and token are
/// extracted from it. Otherwise the user is prompted interactively.
pub async fn run(setup_url: Option<&str>, server_flag: Option<&str>) -> anyhow::Result<()> {
    let (server, token) = if let Some(url_str) = setup_url {
        extract_server_and_token(url_str)?
    } else {
        let creds = Credentials::load()?;
        let server = creds
            .resolve_server(server_flag)
            .unwrap_or_else(|| {
                prompt_with_default("Server URL", "http://localhost:3000")
                    .unwrap_or_else(|_| "http://localhost:3000".to_string())
            });
        let token = prompt("Setup token (from server logs): ")?;
        (server, token)
    };

    // Verify setup is actually needed before prompting for credentials.
    let client = Client::new(&server);
    let status = client.setup_status().await?;
    if !status.setup_required {
        println!("This server already has an admin account. Use 'helpcore login' instead.");
        return Ok(());
    }

    let email = prompt("Admin email: ")?;
    if email.is_empty() || !email.contains('@') {
        bail!("invalid email address");
    }

    let password = rpassword::prompt_password("Admin password: ")?;
    if password.len() < 8 {
        bail!("password must be at least 8 characters");
    }
    let confirm = rpassword::prompt_password("Confirm password: ")?;
    if password != confirm {
        bail!("passwords do not match");
    }

    let display_name = {
        let n = prompt("Display name (optional): ")?;
        if n.is_empty() { None } else { Some(n) }
    };

    let resp = client.setup(&token, &email, &password, display_name).await?;

    let mut creds = Credentials::default();
    creds.server_url = Some(server.clone());
    creds.access_token = Some(resp.access_token);
    creds.refresh_token = Some(resp.refresh_token);
    creds.save()?;

    println!("✓ Admin account created. Logged in to {server} as {email}");
    Ok(())
}

/// Parses `http://host:port/setup?token=XYZ` into `("http://host:port", "XYZ")`.
fn extract_server_and_token(url_str: &str) -> anyhow::Result<(String, String)> {
    let url = reqwest::Url::parse(url_str)
        .with_context(|| format!("invalid setup URL: {url_str}"))?;

    let token = url
        .query_pairs()
        .find(|(k, _)| k == "token")
        .map(|(_, v)| v.to_string())
        .ok_or_else(|| anyhow::anyhow!("no token parameter in setup URL"))?;

    let mut server = format!("{}://{}", url.scheme(), url.host_str().unwrap_or("localhost"));
    if let Some(port) = url.port() {
        server.push(':');
        server.push_str(&port.to_string());
    }

    Ok((server, token))
}

fn prompt(label: &str) -> anyhow::Result<String> {
    use std::io::Write;
    print!("{label}");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

fn prompt_with_default(label: &str, default: &str) -> anyhow::Result<String> {
    use std::io::Write;
    print!("{label} [{default}]: ");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    let trimmed = buf.trim();
    Ok(if trimmed.is_empty() { default.to_string() } else { trimmed.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_server_and_token_parses_full_url() {
        let (server, token) =
            extract_server_and_token("http://localhost:3000/setup?token=abc123").unwrap();
        assert_eq!(server, "http://localhost:3000");
        assert_eq!(token, "abc123");
    }

    #[test]
    fn extract_server_and_token_no_port() {
        let (server, token) =
            extract_server_and_token("https://helpcore.example.com/setup?token=xyz").unwrap();
        assert_eq!(server, "https://helpcore.example.com");
        assert_eq!(token, "xyz");
    }

    #[test]
    fn extract_server_and_token_missing_token_fails() {
        let result = extract_server_and_token("http://localhost:3000/setup");
        assert!(result.is_err());
    }
}
