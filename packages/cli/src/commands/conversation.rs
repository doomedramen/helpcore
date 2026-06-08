//! `hc conversation ls` and `hc conversation show` — browse conversation history.

use crate::{client::Client, config::Credentials};

/// Lists all conversations for the authenticated user, most recently updated first.
pub async fn list(server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    let mut conversations = client.list_conversations(token).await?;
    if conversations.is_empty() {
        eprintln!("(no conversations yet — use 'hc ask' to start one)");
        return Ok(());
    }

    // Most recently updated first.
    conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    for c in &conversations {
        let title = if c.title.trim().is_empty() {
            "Untitled"
        } else {
            c.title.as_str()
        };
        println!(
            "{:<16}  {:<5} msgs  {:<20}  {}",
            c.id, c.message_count, c.updated_at, title
        );
    }
    eprintln!("\nUse 'hc ask -c <id> \"...\"' or 'hc conversation show <id>' to continue.");
    Ok(())
}

/// Prints all messages in a conversation by ID.
pub async fn show(conversation_id: &str, server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    let messages = client
        .get_conversation_messages(token, conversation_id)
        .await?;

    if messages.is_empty() {
        eprintln!("(conversation has no messages yet)");
        return Ok(());
    }

    for m in &messages {
        if m.content.trim().is_empty() {
            continue;
        }
        let label = match m.role.as_str() {
            "user" => "You",
            "assistant" => "Assistant",
            "tool" => "Tool",
            other => other,
        };
        println!("── {label} ──────────────────────────────");
        println!("{}", m.content.trim());
        println!();
    }
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
