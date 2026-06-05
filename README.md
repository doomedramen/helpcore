# helpcore

A self-hosted personal AI assistant server. Run it on a home server or VPS, connect from anywhere with the `hc` CLI, and talk to any model you like — local (Ollama) or cloud (Anthropic, OpenAI).

```
$ hc ask "what's on my to-do list today?"
Based on your notes, you have three things: the home server network config, 
following up with the dentist, and reviewing the server monitoring setup.
```

---

## Features

- **Local-first** — runs on a Raspberry Pi or a 4 GB container; works fine with a 3B parameter Ollama model
- **Memory system** — persistent markdown files the AI reads on every message; soul, identity, and per-user context files
- **Conversation history** — all conversations stored in SQLite with full-text search; auto-compaction when context fills up
- **Multiple providers** — Ollama, Anthropic, OpenAI, or any OpenAI-compatible server; switch per-request
- **Plugin system** — extend via bridge plugins (standalone services); scoped tokens, skill injection into context
- **Voice plugin** — Whisper STT + KittenTTS in a Docker sidecar; full voice chat loop
- **Pre-built Docker images** — CI pushes to GHCR on every merge; `docker compose pull && docker compose up` to update
- **Token auto-refresh** — CLI handles access token expiry silently

---

## Quick start (Docker)

**Requirements:** Docker 24+.

Save this as `docker-compose.yml` (or use the one included in the repo):

```yaml
services:
  helpcore:
    image: ghcr.io/doomedramen/helpcore:latest
    ports:
      - "3000:3000"
    volumes:
      - ./config.toml:/config.toml:ro
      - helpcore_data:/data
    environment:
      HELPCORE_CONFIG: /config.toml
      HELPCORE_DATA: /data
    restart: unless-stopped

  ollama:
    image: ollama/ollama
    ports:
      - "11434:11434"
    volumes:
      - ollama_data:/root/.ollama
    restart: unless-stopped

volumes:
  helpcore_data:
  ollama_data:
```

Then:

```bash
# 1. Create a minimal config.toml
cat > config.toml << 'EOF'
[server]
name = "helpcore"
url  = "http://localhost:3000"

[[providers]]
id            = "ollama"
type          = "ollama"
default_model = "qwen2.5:3b"
roles         = ["chat"]
url           = "http://ollama:11434"
EOF

# 2. Start helpcore + Ollama
docker compose up -d

# 3. Pull a model
docker compose exec ollama ollama pull qwen2.5:3b

# 4. Run the first-admin wizard
hc setup --server http://localhost:3000

# 5. Chat
hc ask "hello, what can you do?"
```

---

## CLI install

```bash
# macOS (Apple Silicon)
curl -L https://github.com/DoomedRamen/helpcore/releases/latest/download/hc-aarch64-apple-darwin \
  -o /usr/local/bin/hc && chmod +x /usr/local/bin/hc

# macOS (Intel)
curl -L https://github.com/DoomedRamen/helpcore/releases/latest/download/hc-x86_64-apple-darwin \
  -o /usr/local/bin/hc && chmod +x /usr/local/bin/hc

# Linux x86_64
curl -L https://github.com/DoomedRamen/helpcore/releases/latest/download/hc-x86_64-unknown-linux-musl \
  -o /usr/local/bin/hc && chmod +x /usr/local/bin/hc
```

Or build from source:
```bash
cargo build --release -p helpcore-cli
cp target/release/helpcore /usr/local/bin/hc
```

---

## Memory and personality

helpcore injects several markdown files into the AI context on every message:

```bash
hc soul   --edit        # tone, values, communication style
hc identity --edit      # who the assistant is, its name and role
hc me     --edit        # facts about you the AI should remember

hc memory set notes/work.md --edit   # arbitrary memory files
hc memory ls                          # list all memory files
hc memory get notes/work.md           # print one
hc memory rm notes/work.md            # delete one
```

See [docs/memory.md](docs/memory.md) for details.

---

## Plugins

Plugins are registered in `config.toml` and auto-loaded at startup:

```toml
[[plugins.local]]
id      = "voice-kittentts"
path    = "plugins/voice-kittentts"
enabled = true
```

Manage via CLI:
```bash
hc plugin ls                          # list installed plugins
hc plugin token voice-kittentts       # generate scoped bridge token
hc plugin enable  voice-kittentts
hc plugin disable voice-kittentts
```

See [docs/voice-plugin.md](docs/voice-plugin.md) for the voice plugin setup.

---

## Documentation

| Doc | Contents |
|---|---|
| [Getting started](docs/getting-started.md) | Full Docker + home server setup walkthrough |
| [Configuration](docs/configuration.md) | config.toml reference |
| [CLI reference](docs/cli-reference.md) | Every `hc` command |
| [Memory & personality](docs/memory.md) | Soul, identity, memory files |
| [Voice plugin](docs/voice-plugin.md) | Whisper + KittenTTS setup |

---

## Building from source

```bash
# Requirements: Rust 1.87+, musl-tools (for Linux cross-compile)

cargo build --workspace                # debug
cargo build --release --workspace      # release
cargo test --workspace                 # run tests
```

See [docs/getting-started.md#building-from-source](docs/getting-started.md#building-from-source) for cross-compilation notes.

---

## License

MIT
