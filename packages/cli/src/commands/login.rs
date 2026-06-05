use anyhow::bail;

use crate::{client::Client, config::Credentials};

pub async fn run(server_flag: Option<&str>, email_flag: Option<&str>) -> anyhow::Result<()> {
    let mut creds = Credentials::load()?;

    let server = creds
        .resolve_server(server_flag)
        .unwrap_or_else(|| "http://localhost:3000".to_string());

    let email = match email_flag {
        Some(e) => e.to_string(),
        None => prompt("Email: ")?,
    };

    let password = rpassword::prompt_password("Password: ")?;
    if password.is_empty() {
        bail!("password cannot be empty");
    }

    let client = Client::new(&server);
    let resp = client.login(&email, &password).await?;

    creds.server_url = Some(server.clone());
    creds.access_token = Some(resp.access_token);
    creds.refresh_token = Some(resp.refresh_token);
    creds.save()?;

    println!("✓ Logged in to {server}");
    Ok(())
}

fn prompt(label: &str) -> anyhow::Result<String> {
    use std::io::Write;
    print!("{label}");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}
