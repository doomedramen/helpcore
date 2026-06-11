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

Store packages, versions, configuration, enablement, and permissions are
per-user. Local plugins declared in server config remain administrator
provisioned.

---

## Installation flow

Every authenticated user can use the web `/plugins` page to install, update,
configure, enable, disable, roll back, and uninstall their own store plugins.

```
User: "install home-assistant"
  → AI calls install_plugin("home-assistant")
  → Core checks server blacklist — abort if listed
  → Core fetches manifest from registry
  → Core downloads the bounded .tar.gz package and verifies SHA-256
  → Core rejects traversal, symlinks, manifest mismatches, incompatible core
    versions, undeclared permissions, and blacklisted IDs
  → User approves a subset of the declared permissions
  → Core installs the plugin disabled in the user's versioned directory
  → Bridge plugins are configured and health-checked before enablement
  → Plugin skill fragment is added to this user's AI context
```

Registry URL, blacklist, and `[[plugins.local]]` provisioning are admin-only.
Saving a blacklist immediately disables matching installs for every user.

The AI receives a compact catalog containing enabled, disabled,
configuration-required, available, unavailable, and blocked plugins. It may
propose one materially useful user-managed plugin at a time. Approval,
configuration, health validation, and enablement use a durable interaction
that survives reloads; plugin secrets go directly to encrypted storage and are
never added to conversation history.

---

## Storage

**Filesystem**
```
{data_dir}/
  users/
    {user_id}/
      workspace/
      plugins/
        {plugin_id}/
          versions/
            {version}/
              manifest.toml
              skill.md
              tools.json
              plugin.wasm   # WASM tier only
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

**Tier 1 (WASM):** Wasmtime's Component Model runs without WASI, with fuel,
wall-clock, memory, table, instance, and result-size limits. The JSON call ABI
is `call(tool, input-json) -> result<string, string>`. Approved host functions
provide allowlisted HTTP and path-validated access to only the requesting
user's workspace.

**Tier 2 (bridge):** the core invokes `POST /tools/{tool}` with timeouts,
allowlisted endpoints, per-install encrypted credentials, settings, and
permission checks. Enabling requires a successful health check.

---

## Uninstalling

- **Disable** — plugin stays cached on server, disabled for this user only.
  Other users' installs are unaffected.
- **Update** — verifies into a temporary directory, atomically installs the new
  version, and retains the prior version for rollback.
- **Uninstall** — disables the plugin, revokes its user tokens, and removes that
  user's versions, configuration, and install row without touching other users.

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
url = "https://github.com/doomedramen/helpcore-plugins/releases/download/plugins-latest/plugins.json"
```

The blacklist is enforced before any install attempt. Blacklisted plugins are
never downloaded, regardless of which user requests them.

Registry plugins are opt-in per user. Local plugins declared with
`[[plugins.local]]` are registered for users at startup and for the first admin
when setup completes.

---

## Store registry

Published as a release asset from the
[helpcore-plugins](https://github.com/doomedramen/helpcore-plugins) repository.
`plugins.json` is auto-generated from plugin manifests and uploaded to the
`plugins-latest` release alongside the package archives.

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
  "package": {
    "url": "https://example.com/home-assistant-1.0.0.tar.gz",
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  },
  "author": "doomedramen"
}
```

The `source` field identifies the upstream project. Installation uses the
versioned package URL and SHA-256. Legacy entries without package metadata
remain visible but cannot be installed.
