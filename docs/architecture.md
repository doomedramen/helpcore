# helpcore — Architecture

This document captures the architectural decisions made during the design phase.
Update it when decisions change; do not let it drift from reality.

**Reference implementation:** `references/zeroclaw/` contains the full zeroclaw source,
used as architectural inspiration. If that directory is missing, clone it:

```bash
git clone https://github.com/zeroclaw-labs/zeroclaw references/zeroclaw
```

---

## What helpcore is

A self-hosted server that is the secure foundation for a personal AI assistant.
It handles security, auth, permissions, users, AI provider routing, and the
plugin system. Everything else — web UI, iOS app, CLI, integrations — is built
on top of it.

The server has no hard-defined usage. It exposes a well-defined API; clients
and plugins determine what the system actually does.

---

## Monorepo layout

```
helpcore/               ← this repo (Cargo workspace root)
  Cargo.toml            ← workspace manifest
  Dockerfile            ← server image (Linux)
  docker-compose.yml    ← local dev environment
  core/                 ← server binary (Linux target in production)
  cli/                  ← CLI binary (macOS native in dev, Linux in prod)
  crates/
    helpcore-api/       ← shared types and traits (core + CLI both depend on this)
  apps/
    ios/                ← Swift client (outside Cargo workspace)
    web/                ← Next.js client (outside Cargo workspace)
  registry/             ← plugin store registry app + plugins.json
  plugins/              ← first-party plugin examples
  docs/                 ← architecture decisions (here)
```

---

## Core responsibilities

The Rust server owns exactly these concerns:

| Responsibility | Notes |
|---|---|
| Auth & users | Registration, sessions, tokens, per-user data isolation |
| Permissions | What each user and plugin is allowed to access |
| Database | Conversations, user settings, plugin registry, sessions, audit log |
| AI provider routing | Abstraction over Anthropic, OpenAI, Ollama; streaming; fallback chains |
| Plugin registry | WASM module loading + sandboxed execution; bridge service registration |
| Tool dispatch | Route LLM tool calls to the correct WASM plugin or bridge service |
| Slash command registry | Clients query this to discover available slash commands |
| Audit log | Every action logged per user — non-optional, cannot be disabled |

The server does **not** implement channels (WhatsApp, Telegram), integrations
(Home Assistant, Obsidian), or any domain-specific features. Those are plugins.

---

## Plugin model

### Security principle

The server never executes untrusted code with host OS access. Plugins cannot
read or write the server's filesystem, spawn processes, or make arbitrary
syscalls. All plugin interaction goes through the core's API.

### Two tiers

**Tier 1 — WASM (in-process, sandboxed)**

- Plugin ships as a `.wasm` module
- Runs inside the server process with zero host filesystem access
- Can only call host functions the core explicitly exposes:
  - Outbound HTTP to registered endpoints
  - Read/write the installing user's own data
  - Return a tool result to the LLM
- Suited for: logic, compute, AI tools, formatters, anything from the plugin store

**Tier 2 — Bridge services (out-of-process, HTTP)**

- Plugin is a separate service running anywhere (LAN, another container, another machine)
- Core calls it via HTTP; it cannot reach the core server's internals
- The bridge has full access to whatever it needs on its own end
- Suited for: WhatsApp, Telegram, Home Assistant, Obsidian, custom project APIs

### Plugin anatomy

Every plugin (either tier) ships with three things:

1. **Manifest** (`manifest.toml`) — name, version, tier, required permissions, minimum core version
2. **Skill definition** — system prompt fragment + tool definitions (JSON Schema) + slash command declarations
3. **Execution** — either a `.wasm` module (Tier 1) or a bridge endpoint URL (Tier 2)

The core assembles each LLM call by composing:
- Base system prompt
- Each installed plugin's skill fragment (for the requesting user)
- Tool definitions from all installed plugins

---

## API protocol

**REST** for everything:
- Auth, user management, plugin install/config, conversation management, admin

**Server-Sent Events (SSE)** for streaming LLM responses only:
- `POST /api/chat` returns `text/event-stream`
- Clients (iOS `URLSession`, browser `EventSource`, CLI `curl`) all support SSE natively
- Works through proxies and load balancers without special configuration

No WebSocket. No gRPC. Any HTTP client — including `curl` and a basic `fetch`
call — can interact with the entire API. Plugins do not need special libraries.

---

## Multi-user isolation

- Each user has an isolated data partition
- No user can access another user's conversations, plugin configuration, or data
- Plugins installed by one user do not apply to other users
- The audit log is per-user and tamper-evident

---

## Database

**SQLite** — embedded, single file, no separate process, bundled into the server binary.
Appropriate given the small expected user count (personal + family).

Tables owned by the core: users, sessions, conversations, messages, plugin registry,
plugin installs (per user), slash commands, audit log.

Migration tool: `sqlx` with compile-time checked queries and versioned migrations.

---

## Development setup

**Target:** Linux (production server)
**Dev machine:** macOS

**Local dev workflow:**
- OrbStack runs the server in a Linux container or VM
- `docker-compose.yml` at repo root provides one-command dev startup
- CLI is a native macOS binary pointing at `http://localhost:<PORT>`
- Linux-only features (landlock, seccomp) are gated behind `#[cfg(target_os = "linux")]` and compile away silently on macOS

**Building Linux release binaries from macOS:** `cross` (wraps Docker to cross-compile for the Linux target).

---

## Out of scope (intentionally deferred)

| Topic | Decision |
|---|---|
| Push notifications | Not in scope. Delivery is an external concern. |
| Plugin store infrastructure | Deferred — design when first plugins exist |
| Binary / native plugins | Excluded — no plugin execution with host OS access |
