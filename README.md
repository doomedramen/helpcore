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
- **Web UI** — Next.js chat, per-user plugin store, and admin settings served from the same port as the API
- **Pre-built Docker images** — CI pushes to GHCR on every merge; `docker compose pull && docker compose up` to update
- **Token auto-refresh** — CLI and web UI both handle access token expiry silently

---

## Quick start (Docker)

**Requirements:** Docker 24+.

All optional services are behind profiles so you only run what you need:

```bash
docker compose up -d                                               # helpcore only
docker compose --profile with-ollama up -d                        # + Ollama
docker compose --profile voice up -d                              # + voice (STT/TTS)
docker compose --profile with-ollama --profile voice up -d        # + both
```

<details>
<summary><strong>docker-compose.yml</strong></summary>

```yaml
services:
  helpcore:
    image: ghcr.io/doomedramen/helpcore:latest
    ports:
      - "3000:3000"
    volumes:
      - ./config:/config
      - helpcore_data:/data
    environment:
      HELPCORE_CONFIG: /config/config.toml
      HELPCORE_DATA: /data
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "wget", "-q", "-O", "/dev/null", "http://localhost:3000/api/health"]
      interval: 30s
      timeout: 5s
      start_period: 10s
      retries: 3

  # docker compose --profile with-ollama up
  # Then: docker compose exec ollama ollama pull llama3
  # Set url = "http://ollama:11434" in [[providers]] in config.toml
  ollama:
    profiles: [with-ollama]
    image: ollama/ollama
    ports:
      - "11434:11434"
    environment:
      OLLAMA_DEBUG: "${OLLAMA_DEBUG:--1}"
    volumes:
      - ollama_data:/root/.ollama
    logging:
      options:
        max-size: "10m"
        max-file: "3"
    restart: unless-stopped

  # docker compose --profile voice up
  # Speaches: OpenAI-compatible STT (faster-whisper) + TTS (Piper/Kokoro) — CPU only
  # https://github.com/speaches-ai/speaches
  speaches:
    profiles: [voice]
    image: ghcr.io/speaches-ai/speaches:latest-cpu
    ports:
      - "8000:8000"
    volumes:
      - speaches_data:/home/user/.cache
    restart: unless-stopped

  # docker compose --profile voice-piper up
  # Standalone Piper TTS — use when you want TTS only without speaches.
  # Set TTS_BACKEND=piper_http, TTS_URL=http://localhost:5000 in the voice bridge.
  # Voice list: https://github.com/rhasspy/piper/blob/master/VOICES.md
  piper:
    profiles: [voice-piper]
    image: artibex/piper-http:latest
    ports:
      - "5000:5000"
    environment:
      MODEL_DOWNLOAD_LINK: "${PIPER_MODEL_URL:-https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium/en_US-lessac-medium.onnx}"
    restart: unless-stopped

volumes:
  helpcore_data:
  ollama_data:
  speaches_data:
```

</details>

The Docker image bundles both the Rust API server and the Next.js web UI — no separate container needed. The web UI and API are both served on port 3000.

```bash
# First run: pull the image and start
docker compose pull
docker compose up -d

# Edit config/config.toml if needed, then restart
docker compose restart helpcore

# Pull an Ollama model (if using the with-ollama profile)
docker compose exec ollama ollama pull qwen2.5:3b

# Complete first-time setup — helpcore prints a setup URL on first run:
docker compose logs helpcore | grep "Setup required"
# Open that URL in your browser to create the admin account, or:
hc setup --server http://localhost:3000

# Open the web UI
open http://localhost:3000

# Or chat from the terminal
hc ask "hello, what can you do?"
```

### Voice

The `voice` profile starts [speaches](https://github.com/speaches-ai/speaches) — a CPU-only container providing OpenAI-compatible STT (faster-whisper) and TTS (Piper/Kokoro) on port 8000.

Run the voice bridge separately (it forwards audio through helpcore's AI):

```bash
cd plugins/voice/bridge
cargo build --release
HELPCORE_URL=http://localhost:3000 \
STT_BACKEND=openai_compat STT_URL=http://localhost:8000 \
TTS_BACKEND=openai_compat TTS_URL=http://localhost:8000 \
./target/release/voice-bridge
```

Then install the **Voice** plugin in helpcore and set the bridge URL to `http://localhost:8765`.

Alternatively, use `artibex/piper-http` for standalone Piper TTS:

```bash
docker compose --profile voice-piper up -d
# Set TTS_BACKEND=piper_http TTS_URL=http://localhost:5000 in the bridge
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
id      = "my-plugin"
path    = "plugins/my-plugin"
enabled = true
```

Manage via CLI:
```bash
hc plugin ls            # list installed plugins
hc plugin token my-plugin   # generate scoped bridge token
hc plugin enable  my-plugin
hc plugin disable my-plugin
```

---

## Documentation

| Doc | Contents |
|---|---|
| [Getting started](docs/getting-started.md) | Full Docker + home server setup walkthrough |
| [Configuration](docs/configuration.md) | config.toml reference |
| [CLI reference](docs/cli-reference.md) | Every `hc` command |
| [Memory & personality](docs/memory.md) | Soul, identity, memory files |


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
