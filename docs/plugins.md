# helpcore — Plugin System

---

## Plugin tiers

**Tier 1 — WASM (in-process, sandboxed)**
Ships as a `.wasm` binary. Runs inside the server process with zero host
filesystem access. Can only call host functions the core explicitly exposes.
Suited for logic, compute, and AI tools from the store.

**Tier 2 — Bridge service (out-of-process, HTTP)**
A separate service running anywhere. The core calls it via HTTP using a scoped
token. The bridge cannot reach the core's internals. Suited for external
integrations: WhatsApp, Telegram, Home Assistant, Obsidian, custom APIs.

---

## Plugin anatomy

Every plugin ships with:

1. **`manifest.toml`** — id, name, version, tier, declared permissions, minimum
   core version, source reference
2. **Skill definition** — system prompt fragment + tool definitions (JSON Schema)
   + slash command declarations
3. **Execution** — either a `.wasm` module (Tier 1) or a bridge endpoint URL
   (Tier 2)

---

## Plugin states

| State | Meaning |
|---|---|
| Available | Listed in the registry; not on this server |
| Cached | WASM binary downloaded to the server; not enabled for any user |
| Enabled | A specific user has approved its permissions and activated it |

WASM binaries are shared across users — one copy per version on disk. Enablement
and permissions are per-user in the database.

---

## Installation flow

Triggered by natural language: "install the home-assistant plugin".

```
User: "install home-assistant"
  → AI calls install_plugin("home-assistant")
  → Core checks server blacklist — abort if listed
  → Core fetches manifest from registry
  → Core downloads + checksums WASM binary (Tier 1)
    OR registers bridge endpoint URL (Tier 2)
  → Core presents permissions to user in chat:
      "home-assistant needs:
         outbound_http → homeassistant.local
         read user_data
       Allow? (yes / no)"
  → User approves
  → Core writes plugin_installs row for this user
  → Core generates scoped bearer token for this user+plugin (Tier 2 only)
  → Plugin skill fragment is added to this user's AI context
```

The web admin UI currently lists installed plugins and the configured store
catalog. Installed plugins can be enabled or disabled immediately. Registry
installation remains future work: the core does not yet download, verify, or
activate store entries, so the UI deliberately does not present a misleading
Install action.

---

## Storage

**Filesystem**
```
{data_dir}/
  plugins/
    wasm/
      {plugin_id}/
        {version}/
          plugin.wasm
```

**Database tables**

`plugins` — registry cache
- id, name, version, manifest (JSON), checksum, source_url, cached_at

`plugin_installs` — per-user state
- user_id, plugin_id, version, approved_permissions (JSON), enabled, config (JSON), installed_at

`plugin_tokens` — scoped bearer tokens for Tier 2 bridges
- id, user_id, plugin_id, token_hash, permissions (JSON), created_at, revoked_at

---

## Permission enforcement

**Tier 1 (WASM):** the sandbox is constructed at load time with only the host
functions matching the user's approved permissions. The sandbox cannot call
anything else — it is physically impossible, not just policy.

**Tier 2 (bridge):** inbound requests carry a scoped bearer token. The core
validates the token and enforces the approved permission scope on every
response. Requests for data outside the approved scope are rejected.

---

## Uninstalling

- **Disable** — plugin stays cached on server, disabled for this user only.
  Other users' installs are unaffected.
- **Uninstall** — removes the user's `plugin_installs` row and revokes their
  scoped token. If no users have the plugin installed, the WASM binary may be
  garbage collected from disk.

---

## Server config

`~/.helpcore/config.toml` (overridden by `HELPCORE_CONFIG` env var).

```toml
[server]
name = "My helpcore"
url  = "https://helpcore.example.com"

[plugins]
blacklist = ["untrusted-plugin-id"]  # plugins that cannot be installed on this server

[registry]
url = "https://raw.githubusercontent.com/doomedramen/helpcore/main/registry/plugins.json"
```

The blacklist is enforced before any install attempt. Blacklisted plugins are
never downloaded, regardless of which user requests them.

Registry plugins are opt-in per user. Local plugins declared with
`[[plugins.local]]` are registered for users at startup and for the first admin
when setup completes.

---

## Store registry

Lives in `registry/` in this monorepo. It consists of:

- `plugins.json` — the curated list of available plugins

The core fetches the configured registry URL directly. Operators can point it
at another compatible JSON file to run a private catalog.

### `plugins.json` entry format

```json
{
  "id": "home-assistant",
  "name": "Home Assistant",
  "description": "Control your Home Assistant instance via the AI.",
  "version": "1.0.0",
  "tier": "bridge",
  "source": {
    "type": "github",
    "repo": "helpcore-plugins/home-assistant"
  },
  "permissions": ["outbound_http"],
  "author": "doomedramen"
}
```

The `source` field tells the core where to fetch the plugin from during
installation. `type` may be `github` (public repo), `url` (direct download),
or `local` (path on the server filesystem — for development).
