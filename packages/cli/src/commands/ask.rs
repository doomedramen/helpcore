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
    let creds = Credentials::load()?;

    let server = creds
        .resolve_server(server_flag)
        .ok_or_else(|| anyhow::anyhow!("no server configured — run 'helpcore login' first"))?;

    let access_token = creds
        .access_token
        .as_deref()
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
    let mut stdout = std::io::stdout();

    let done = client
        .chat(access_token, &request, |delta| {
            print!("{delta}");
            let _ = stdout.flush();
        })
        .await?;

    // Trailing newline after the streamed response.
    println!();

    // Show conversation ID hint so the user can continue.
    if is_new_conversation {
        eprintln!("\n→ conversation {} (use -c {} to continue)", done.conversation_id, done.conversation_id);
    }

    Ok(())
}
