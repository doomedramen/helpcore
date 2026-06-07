# Running the core on macOS

The helpcore server and CLI run natively on Apple Silicon and Intel Macs.

## Requirements

- macOS 13 or newer
- Rust 1.87 or newer
- An AI provider such as Ollama, Anthropic, or OpenAI

Install Rust when needed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Run from the repository

Build only the core binaries:

```bash
make build-headless
```

Build the server with the static web application embedded:

```bash
make build
```

This runs the Next.js static export and embeds its output directly in
`target/release/helpcore-server`. No web files are needed beside the resulting
executable.

After `make build`, run the bundled server with the repository's `config.toml`:

```bash
make run
```

The server serves an embedded UI by default when one was included at build
time. To run the API without any web UI routes:

```bash
./target/release/helpcore-server --headless
```

For development, `HELPCORE_WEB_DIR` can point to an external static export. It
takes precedence over embedded assets unless `--headless` is used.

The server listens on port 3000. Its API health endpoint is:

```bash
curl http://localhost:3000/api/health
```

For a native Ollama installation, use this provider URL:

```toml
url = "http://localhost:11434"
```

Do not use the Docker-only hostname `http://ollama:11434`.

## Install as a background service

The installer builds the server with its embedded web UI plus the CLI, installs
them under `~/.local/bin`, and starts the server as a per-user LaunchAgent:

```bash
make install
```

Installed files:

| Path | Purpose |
|---|---|
| `~/.local/bin/helpcore-server` | Core server |
| `~/.local/bin/hc` | Short CLI command |
| `~/.helpcore/config.toml` | Server configuration |
| `~/.helpcore/data/` | SQLite database and user data |
| `~/.helpcore/logs/` | Server output |
| `~/Library/LaunchAgents/dev.helpcore.server.plist` | LaunchAgent definition |

The installer preserves an existing `~/.helpcore/config.toml`.

Inspect the service:

```bash
launchctl print gui/$(id -u)/dev.helpcore.server
tail -f ~/.helpcore/logs/server.log
```

Restart it after changing configuration:

```bash
launchctl kickstart -k gui/$(id -u)/dev.helpcore.server
```

Remove the LaunchAgent and installed binaries:

```bash
make uninstall
```

Configuration and data are intentionally preserved.
