use std::io::Write;

use anyhow::bail;
use helpcore_api::ChatRequest;

use crate::{client::Client, config::Credentials};

pub async fn run(
    query: &[String],
    conversation_id: Option<&str>,
    provider: Option<&str>,
    model: Option<&str>,
    server_flag: Option<&str>,
) -> anyhow::Result<()> {
    let mut creds = Credentials::load()?;

    let server = creds
        .resolve_server(server_flag)
        .ok_or_else(|| anyhow::anyhow!("no server configured — run 'helpcore login' first"))?;

    let access_token = creds
        .access_token
        .clone()
        .ok_or_else(|| anyhow::anyhow!("not logged in — run 'helpcore login' first"))?;

    let message = query.join(" ");
    if message.trim().is_empty() {
        bail!("message cannot be empty");
    }

    let client = Client::new(&server);
    let request = ChatRequest {
        conversation_id: conversation_id.map(|s| s.to_string()),
        message,
        provider_id: provider.map(|s| s.to_string()),
        model: model.map(|s| s.to_string()),
    };

    let is_new_conversation = conversation_id.is_none();

    // Attempt chat; if the access token has expired, try refreshing once.
    let done = match client.chat(&access_token, &request, emit_chunk).await {
        Ok(done) => done,
        Err(e) if Client::is_token_expired(&e) => {
            let refresh_token = creds
                .refresh_token
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("session expired — run 'hc login'"))?;
            let tokens = client
                .refresh(refresh_token)
                .await
                .map_err(|_| anyhow::anyhow!("session expired — run 'hc login'"))?;
            let new_access = tokens.access_token.clone();
            creds.access_token = Some(new_access.clone());
            creds.refresh_token = Some(tokens.refresh_token);
            let _ = creds.save(); // best-effort — don't fail the whole request on disk error
            client.chat(&new_access, &request, emit_chunk).await?
        }
        Err(e) => return Err(e),
    };

    // Trailing newline after the streamed response.
    println!();

    // Show conversation ID hint so the user can continue.
    if is_new_conversation {
        eprintln!(
            "\n→ conversation {} (use -c {} to continue)",
            done.conversation_id, done.conversation_id
        );
    }

    Ok(())
}

/// Prints a streaming token to stdout immediately (no newline).
fn emit_chunk(delta: &str) {
    print!("{delta}");
    let _ = std::io::stdout().flush();
}
