# helpcore — Initial Setup Flow

---

## Overview

Setup has four stages, identical for both distribution methods:

1. **Install** — get the server running
2. **Create admin** — first-run wizard creates the first admin account
3. **Configure** — add providers through the admin UI, or edit `config.toml`
4. **Connect clients** — log in from CLI, web, or any other interface

---

## Directory layout

| Path | Purpose | Notes |
|---|---|---|
| `~/.helpcore/config.toml` | Server config | Override with `HELPCORE_CONFIG` env var |
| `~/.helpcore/data/helpcore.db` | SQLite database | Created on first start |
| `~/.helpcore/data/users/{user_id}/plugins/` | Store plugin versions | Scoped to the installing user |

Docker uses volume mounts for both (see below).

---

## Minimum `config.toml`

The server starts with only a `[server]` section. Providers are optional at
first start but required before the AI is useful.

```toml
[server]
name = "My helpcore"
url  = "http://localhost:3000"   # public URL — used in links and CORS

[registry]
url = "https://registry.helpcore.dev"

[plugins]
blacklist = []
```

Add providers after the admin account is created (see Stage 3).

---

## Stage 1 — Install

### Docker (recommended)

Create a `docker-compose.yml` wherever you want to run the server:

```yaml
services:
  helpcore:
    image: ghcr.io/doomedramen/helpcore:latest
    ports:
      - "3000:3000"
    volumes:
      - ./config:/config
      - helpcore-data:/data
    environment:
      HELPCORE_CONFIG: /config/config.toml
      HELPCORE_DATA: /data
    restart: unless-stopped

volumes:
  helpcore-data:
```

Start the stack. The image creates `./config/config.toml` on first boot:

```bash
docker compose up -d
${EDITOR:-vi} config/config.toml
docker compose restart helpcore
docker compose logs -f helpcore   # watch for the setup URL
```

### Native binary

Download the latest Linux binary from the releases page, or build from source:

```bash
# Prebuilt
curl -L https://github.com/doomedramen/helpcore/releases/latest/download/helpcore-server-linux-x86_64 \
  -o helpcore-server && chmod +x helpcore-server

# From source (requires Rust + cross for Linux target on macOS)
cross build --release --target x86_64-unknown-linux-musl -p helpcore-server

# Run
./helpcore-server
# or with explicit config path
./helpcore-server --config /etc/helpcore/config.toml
```

To run as a systemd service:

```ini
# /etc/systemd/system/helpcore.service
[Unit]
Description=helpcore server
After=network.target

[Service]
ExecStart=/usr/local/bin/helpcore-server
Restart=on-failure
User=helpcore
Environment=HELPCORE_CONFIG=/etc/helpcore/config.toml
Environment=HELPCORE_DATA=/var/lib/helpcore

[Install]
WantedBy=multi-user.target
```

```bash
systemctl enable --now helpcore
journalctl -u helpcore -f   # watch for the setup URL
```

---

## Stage 2 — Create admin account

On first start with an empty database the server prints to stdout:

```
╔══════════════════════════════════════════════════════════╗
║  helpcore — setup required                               ║
║                                                          ║
║  Web:  http://localhost:3000/setup?token=<token>         ║
║  CLI:  hc setup --server http://localhost:3000     ║
║                                                          ║
║  Token expires in 15 minutes.                            ║
╚══════════════════════════════════════════════════════════╝
```

**Web path** — open the URL in a browser, fill in name, email, and password.

**CLI / headless path** — run from any machine that can reach the server:

```bash
helpcore setup --server http://localhost:3000
```

The wizard prompts for name, email, and password, then calls the setup
endpoint. The setup endpoint is permanently disabled once the first admin
account exists.

---

## Stage 3 — Configure providers

Use the admin web interface to add AI providers and apply them immediately.
Alternatively, edit `config.toml` directly and restart the server.
Provider credentials live in this file only — they never touch the database.

```toml
[[providers]]
id            = "local-ollama"
name          = "Local Ollama"
type          = "ollama"
roles         = ["chat"]
url           = "http://localhost:11434"
default_model = "llama3.2"
```

Restart to load the new providers:

```bash
# Docker
docker-compose restart helpcore

# systemd
systemctl restart helpcore

# Native (foreground)
Ctrl+C, then re-run
```

Provider changes saved through `/admin/` are applied immediately. Direct
`config.toml` edits require a restart.

---

## Stage 4 — Connect clients

### CLI

Install the `helpcore` CLI binary on your local machine:

```bash
# macOS (native) — binary is named `hc`
curl -L https://github.com/doomedramen/helpcore/releases/latest/download/hc-aarch64-apple-darwin \
  -o /usr/local/bin/hc && chmod +x /usr/local/bin/hc
```

Log in:

```bash
hc login --server https://your-server.com
# Email: martin@example.com
# Password: ···············
# ✓ Logged in. Credentials saved to ~/.helpcore/credentials
```

Send a message:

```bash
hc ask "hello"
```

### Web

Visit `https://your-server.com` in a browser and log in with email + password.

### Adding more users

User management is available through the web admin UI. CLI user and provider
management commands are not yet implemented.

---

## Post-setup checklist

After the above stages are complete:

- [ ] Admin account created
- [ ] At least one provider configured
- [ ] Admin granted access to provider(s) via web UI
- [ ] CLI connected and `helpcore` opens TUI successfully
- [ ] Web UI accessible at server URL
- [ ] Other users created and granted provider access
- [ ] Soul / personality files reviewed (`helpcore ask "show me my SOUL.md"`)
- [ ] First plugin installed via web UI or `hc ask "install the ... plugin"`
