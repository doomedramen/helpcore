use std::io::Read;

use anyhow::{Context, bail};

use crate::{client::Client, config::Credentials};

// ── Personality helpers ───────────────────────────────────────────────────────

/// `hc soul`, `hc identity`, `hc me` — show or set a personality file.
pub async fn personality(
    name: &str,               // "soul" | "identity" | "user"
    set_content: Option<&str>,
    edit: bool,
    server_flag: Option<&str>,
) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    if let Some(content) = set_content {
        // Write directly from --set argument.
        client.set_personality(token, name, content).await?;
        eprintln!("✓ {name} updated");
        return Ok(());
    }

    if edit {
        // Open $EDITOR (or $VISUAL) with current content pre-populated.
        let current = client
            .get_personality(token, name)
            .await?
            .unwrap_or_default();
        let updated = open_in_editor(name, &current)?;
        if updated == current {
            eprintln!("(no changes)");
        } else {
            client.set_personality(token, name, &updated).await?;
            eprintln!("✓ {name} updated");
        }
        return Ok(());
    }

    // Default: print current content.
    match client.get_personality(token, name).await? {
        Some(content) => print!("{content}"),
        None => eprintln!("(no {name} set — use --set or --edit to create one)"),
    }
    Ok(())
}

// ── Memory commands ───────────────────────────────────────────────────────────

pub async fn memory_ls(server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    let files = client.list_memory(token).await?;
    if files.is_empty() {
        eprintln!("(no memory files)");
    } else {
        for f in &files {
            println!("{}", f.path);
        }
    }
    Ok(())
}

pub async fn memory_get(path: &str, server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    match client.get_memory(token, path).await? {
        Some(content) => print!("{content}"),
        None => bail!("memory file not found: {path}"),
    }
    Ok(())
}

pub async fn memory_set(
    path: &str,
    content_arg: Option<&str>,
    edit: bool,
    server_flag: Option<&str>,
) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    let content = if let Some(c) = content_arg {
        c.to_string()
    } else if edit {
        let current = client.get_memory(token, path).await?.unwrap_or_default();
        let updated = open_in_editor(path, &current)?;
        if updated == current {
            eprintln!("(no changes)");
            return Ok(());
        }
        updated
    } else {
        // Read from stdin.
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf).context("failed to read stdin")?;
        buf
    };

    client.set_memory(token, path, &content).await?;
    eprintln!("✓ {path} saved");
    Ok(())
}

pub async fn memory_rm(path: &str, server_flag: Option<&str>) -> anyhow::Result<()> {
    let creds = Credentials::load()?;
    let server = resolve_server(&creds, server_flag)?;
    let token = require_token(&creds)?;
    let client = Client::new(&server);

    if client.delete_memory(token, path).await? {
        eprintln!("✓ {path} deleted");
    } else {
        bail!("memory file not found: {path}");
    }
    Ok(())
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn resolve_server<'a>(creds: &'a Credentials, flag: Option<&'a str>) -> anyhow::Result<String> {
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

/// Opens `$EDITOR` (fallback: `$VISUAL`, then `vi`) with `initial` content in a
/// temp file, waits for the editor to exit, and returns the edited content.
fn open_in_editor(hint: &str, initial: &str) -> anyhow::Result<String> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());

    let tmp = tempfile::Builder::new()
        .prefix(&format!("hc-{hint}-"))
        .suffix(".md")
        .tempfile()
        .context("failed to create temp file for editor")?;

    std::fs::write(tmp.path(), initial)
        .context("failed to write to temp file")?;

    let status = std::process::Command::new(&editor)
        .arg(tmp.path())
        .status()
        .with_context(|| format!("failed to launch editor '{editor}'"))?;

    if !status.success() {
        bail!("editor exited with non-zero status");
    }

    let content = std::fs::read_to_string(tmp.path())
        .context("failed to read temp file after editing")?;
    Ok(content)
}
