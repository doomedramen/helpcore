# Running the core on macOS

The helpcore server and CLI run natively on Apple Silicon and Intel Macs. The
web application is separate and is not built or installed by this workflow.

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
make native-build
```

Run the server with the repository's `config.toml`:

```bash
make native-run
```

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

The installer builds the server and CLI, installs them under `~/.local/bin`,
and starts the server as a per-user LaunchAgent:

```bash
make macos-install
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
make macos-uninstall
```

Configuration and data are intentionally preserved.
