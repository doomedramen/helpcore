//! `hc status` — verifies the session and prints connection info.

use crate::{client::Client, config::Credentials};

/// Checks connectivity to the server (with token refresh if needed) and prints
/// the current user, role, and server URL.
pub async fn run(server_flag: Option<&str>) -> anyhow::Result<()> {
    let mut creds = Credentials::load()?;

    let Some(server) = creds.resolve_server(server_flag) else {
        println!("Not configured. Run 'hc login' to connect to a server.");
        return Ok(());
    };
    println!("Server: {server}");

    let Some(access_token) = creds.access_token.clone() else {
        println!("Status: not logged in (run 'hc login')");
        return Ok(());
    };

    let client = Client::new(&server);

    // Verify the session actually works rather than trusting the local file —
    // the token may have been revoked or the server may be unreachable.
    let user = match client.current_user(&access_token).await {
        Ok(user) => user,
        Err(e) if Client::is_token_expired(&e) => match try_refresh(&client, &mut creds).await {
            Some(fresh) => match client.current_user(&fresh).await {
                Ok(user) => user,
                Err(e) => return print_unreachable(&e),
            },
            None => {
                println!("Status: session expired (run 'hc login')");
                return Ok(());
            }
        },
        Err(e) => return print_unreachable(&e),
    };

    println!("Status: connected");
    println!(
        "User:   {} ({})",
        user.display_name.as_deref().unwrap_or(&user.email),
        user.email
    );
    println!("Role:   {}", user.role);
    Ok(())
}

/// Attempts a refresh-token exchange, persisting the new tokens on success.
/// Returns the fresh access token, or `None` if the refresh token is missing
/// or the exchange fails (session has expired and needs a fresh login).
async fn try_refresh(client: &Client, creds: &mut Credentials) -> Option<String> {
    let refresh_token = creds.refresh_token.clone()?;
    let tokens = client.refresh(&refresh_token).await.ok()?;
    creds.access_token = Some(tokens.access_token.clone());
    creds.refresh_token = Some(tokens.refresh_token);
    let _ = creds.save(); // best-effort — don't fail status on a disk error
    Some(tokens.access_token)
}

fn print_unreachable(e: &anyhow::Error) -> anyhow::Result<()> {
    println!("Status: unreachable — {e:#}");
    Ok(())
}
