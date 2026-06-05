# Getting started

This guide takes you from zero to a working helpcore server with a connected CLI client. It covers three scenarios:

- [Docker on a home server](#home-server)
- [Docker on your local machine (dev / testing)](#local-docker)
- [Building from source](#building-from-source)

---

## Requirements

| Component | Minimum |
|---|---|
| Docker | 24+ |
| RAM (server) | 2 GB without Ollama; 4 GB with `qwen2.5:3b` |
| Disk (server) | 1 GB for helpcore + ~2 GB per Ollama model |
| `hc` CLI | any machine with network access to the server |

No Rust toolchain is needed if you use Docker.

---

## Home server

This is the recommended production setup. Pre-built images are published to GHCR on every push to `main`.

### 1. Clone the repo on the server

```bash
git clone https://github.com/martin/helpcore
cd helpcore
```

### 2. Edit config.toml

The repo includes a working `config.toml`. The only value you may want to change before first start:

```toml
[server]
url = "http://YOUR_SERVER_IP:3000"   # used in the setup wizard URL
```

See [configuration.md](configuration.md) for all options.

### 3. Start the stack

```bash
docker compose -f docker-compose.prod.yml --profile with-ollama up -d
```

This pulls `ghcr.io/martin/helpcore:latest` from GHCR — no build step needed.

Watch the logs to confirm the server is ready:

```bash
docker compose -f docker-compose.prod.yml logs -f
# → INFO helpcore_server: listening on 0.0.0.0:3000
# → INFO helpcore_server: first-run setup required — visit: http://YOUR_IP:3000/setup?token=...
```

### 4. Pull an Ollama model

```bash
docker compose -f docker-compose.prod.yml exec ollama ollama pull qwen2.5:3b
```


Recommended models for a 4 GB RAM budget:

| Model | RAM | Notes |
|---|---|---|
| `qwen2.5:3b` | ~2 GB | Good all-rounder, fast |
| `phi3.5:mini` | ~2.2 GB | Strong reasoning |
| `gemma2:2b` | ~1.6 GB | Tightest fit |
| `llama3.2:3b` | ~2 GB | General purpose |

### 5. Install the CLI on your laptop

```bash
# macOS Apple Silicon
curl -L https://github.com/martin/helpcore/releases/latest/download/hc-aarch64-apple-darwin \
  -o /usr/local/bin/hc && chmod +x /usr/local/bin/hc
```

See the [CLI reference](cli-reference.md) for other platforms and build-from-source instructions.

### 6. Run the first-admin setup wizard

Copy the setup URL from the server logs (or navigate to `http://YOUR_SERVER_IP:3000/setup`), then:

```bash
hc setup --server http://YOUR_SERVER_IP:3000
```

The wizard will:
1. Verify the one-time setup token
2. Prompt for your admin email and password
3. Log you in and store credentials at `~/.helpcore/credentials`

### 7. Send your first message

```bash
hc ask "hello, are you there?"
```

### Updating

```bash
docker compose -f docker-compose.prod.yml pull
docker compose -f docker-compose.prod.yml up -d
```

---

## Local Docker

For development or testing on your own machine.

```bash
git clone https://github.com/martin/helpcore
cd helpcore

# Start (helpcore + Ollama sidecar)
docker compose --profile with-ollama up -d

# Or just helpcore (if you already have Ollama running elsewhere)
docker compose up -d

# Pull a model (if using the sidecar)
docker compose exec ollama ollama pull qwen2.5:3b

# Setup
hc setup --server http://localhost:3000

# Chat
hc ask "test message"
```

Useful commands:

```bash
docker compose logs -f          # tail helpcore logs
docker compose exec helpcore sh  # shell inside the helpcore container
docker compose down              # stop everything
```

---

## Building from source

Requirements: Rust 1.87+.

```bash
git clone https://github.com/martin/helpcore
cd helpcore

# Build everything
cargo build --release --workspace

# Run the server (uses config.toml in the current directory)
./target/release/helpcore-server

# CLI
./target/release/helpcore ask "hello"
# Optionally symlink: ln -s $(pwd)/target/release/helpcore /usr/local/bin/hc
```

### Cross-compiling for Linux (musl, from macOS)

The Docker image is built as a static musl binary. To build the same locally:

```bash
# Install musl target and tools
rustup target add x86_64-unknown-linux-musl
brew install filosottile/musl-cross/musl-cross

# Build
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-linux-musl-gcc \
CC_x86_64_unknown_linux_musl=x86_64-linux-musl-gcc \
cargo build --release --target x86_64-unknown-linux-musl -p helpcore-server
```

---

## Connecting to a remote server

If the server is already set up and you just need to connect the CLI:

```bash
hc login --server http://YOUR_SERVER:3000
# Enter your email and password when prompted
```

Credentials are stored at `~/.helpcore/credentials` (mode 600). The CLI auto-refreshes the access token when it expires — you only need to log in once.

---

## Troubleshooting

**"config.toml is a directory, not a file"**  
An older Compose file created a directory when the host config was missing.
Update the repository, stop the restart loop, and repair the empty directory:

```bash
docker compose -f docker-compose.prod.yml down
rmdir config.toml   # succeeds only when the old Docker-created directory is empty
git pull
make config
docker compose -f docker-compose.prod.yml up -d
```

If `rmdir` reports that the directory is non-empty, move it aside and inspect
its contents before running `git pull`.

**"no provider configured"**  
helpcore requires at least one `[[providers]]` block with `roles = ["chat"]` in config.toml. Restart after editing.

**Ollama connection refused**  
Check `url` in the provider config. Common values:
- Native Ollama on the same host: `http://localhost:11434`
- Docker sidecar on the same compose network: `http://ollama:11434`
- Docker Desktop host: `http://host.docker.internal:11434`

**Model not found in Ollama**  
Pull it first: `ollama pull qwen2.5:3b` (or `docker compose exec ollama ollama pull qwen2.5:3b`).

**CLI "not logged in" after token expiry**  
Token auto-refresh should handle this. If it fails, re-run `hc login`.
