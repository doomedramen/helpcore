use crate::{client::Client, config::Credentials};

pub async fn run() -> anyhow::Result<()> {
    let creds = Credentials::load()?;

    if !creds.is_logged_in() {
        println!("Not logged in.");
        return Ok(());
    }

    // Best-effort: revoke the session on the server.
    if let (Some(server), Some(refresh)) = (&creds.server_url, &creds.refresh_token) {
        let client = Client::new(server);
        if let Err(e) = client.logout(refresh).await {
            // Don't fail on server error — credentials are cleared locally regardless.
            eprintln!("warning: server logout failed ({e}), clearing local credentials anyway");
        }
    }

    Credentials::clear()?;
    println!("✓ Logged out");
    Ok(())
}
