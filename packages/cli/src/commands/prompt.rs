//! Small helpers for reading interactive input from the terminal.
//! Shared by commands that need to collect values that aren't worth their
//! own `clap` flags (e.g. setup and login wizards).

use std::io::Write;

/// Prints `label`, then reads and trims a line from stdin.
pub fn prompt(label: &str) -> anyhow::Result<String> {
    print!("{label}");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

/// Like [`prompt`], but shows `default` and falls back to it on empty input.
pub fn prompt_with_default(label: &str, default: &str) -> anyhow::Result<String> {
    print!("{label} [{default}]: ");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    let trimmed = buf.trim();
    Ok(if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    })
}
