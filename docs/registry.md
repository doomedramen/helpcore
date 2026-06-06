# helpcore — Plugin Registry

---

## How it works

The registry is a single JSON file in this repository:

```
registry/plugins.json
```

The core fetches it directly from the raw GitHub URL. No server, no hosting
costs, no maintenance overhead.

Default registry URL (set in `config.toml`):

```
https://raw.githubusercontent.com/doomedramen/helpcore/main/registry/plugins.json
```

Operators can point `[registry] url` at their own JSON file to run a private
registry (self-hosted plugins, corporate use, etc.).

The authenticated `/plugins` page fetches this file through the core and shows
available, installed, enabled, updateable, and server-blocked entries. Registry
URL and blacklist policy remain admin-only.

---

## Plugin entry format

```json
{
  "id": "home-assistant",
  "name": "Home Assistant",
  "description": "Control your Home Assistant instance through the AI.",
  "version": "1.0.0",
  "tier": "bridge",
  "author": "doomedramen",
  "homepage": "https://github.com/doomedramen/helpcore-plugin-home-assistant",
  "permissions": ["outbound_http"],
  "source": {
    "type": "github",
    "repo": "doomedramen/helpcore-plugin-home-assistant",
    "ref": "1.1.1",
    "wasm_asset": "plugin.wasm"
  },
  "package": {
    "url": "https://example.com/home-assistant-1.0.0.tar.gz",
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "size": 12345
  },
  "setup_guide": "https://github.com/doomedramen/helpcore-plugin-home-assistant#setup"
}
```

### `tier`

| Value | Meaning |
|---|---|
| `wasm` | Tier 1 — WASM module, runs in-process |
| `bridge` | Tier 2 — external service, user deploys separately |

### `source`

| Field | Required for | Notes |
|---|---|---|
| `type` | both | `github` or `url` |
| `repo` | `github` | `owner/repo` |
| `ref` | `github` | release tag (e.g. `1.1.1`) or branch name (e.g. `main`) |
| `wasm_asset` | `wasm` tier | filename of the WASM binary |
| `url` | `url` type | direct download URL for the WASM binary |

Bridge plugins have no WASM to download. The `source` field records where
the bridge code lives so the user can deploy it. The core prompts for the
bridge endpoint URL during install.

### `package`

Installation requires `package.url` and `package.sha256`; `size` is optional.
Legacy entries without package metadata remain browsable but are not
installable. The package is a bounded root-only `.tar.gz` containing
`manifest.toml`, `skill.md`, and `tools.json`, plus `plugin.wasm` for the WASM
tier. Archives containing symlinks, nested paths, unexpected files, oversized
content, checksum mismatches, or manifest mismatches are rejected.

### `permissions`

Declared permissions the plugin will request from the user at install time.
The core enforces these — a plugin cannot exceed what it declared here.

| Value | Meaning |
|---|---|
| `outbound_http` | Make outbound HTTP requests (to declared domains) |
| `user_data_read` | Read the installing user's data |
| `user_data_write` | Write to the installing user's data |
| `conversations_read` | Read the user's conversation history |

### `setup_guide`

Optional URL to deployment/configuration documentation. Shown to the user
during installation of bridge plugins, where manual steps are required.

---

## Install flow per tier

### Tier 1 — WASM

```
User: "install weather plugin"
  → core fetches registry entry
  → downloads and verifies package.url against package.sha256
  → presents permissions to user for approval
  → stores it in {data_dir}/users/{user_id}/plugins/{id}/versions/{version}
  → writes plugin_installs row for this user
```

### Tier 2 — Bridge

```
User: "install home-assistant plugin"
  → core fetches registry entry
  → shows setup_guide URL
  → prompts: "What is your Home Assistant bridge URL?"
  → user provides URL (e.g. http://192.168.1.50:8765)
  → presents permissions to user for approval
  → installs disabled in the user's versioned directory
  → validates the declared endpoint host and health endpoint before enablement
```

---

## Submitting a plugin (community)

Anyone can submit a plugin by opening a pull request that adds an entry to
`registry/plugins.json`.

Requirements for a PR to be accepted:

- `id` is unique, lowercase, hyphen-separated
- `homepage` links to a public repository with source code
- `permissions` accurately lists everything the plugin requests — do not
  under-declare
- A versioned `.tar.gz` package and matching SHA-256 must be published
- For `bridge` tier: a `setup_guide` URL with clear deployment instructions

---

## Repository layout

```
registry/
  plugins.json        ← the registry (one entry per plugin)
```

---

## CI validation

Every PR that touches `registry/plugins.json` should run:

- Duplicate `id` check
- `description` and `name` present and non-empty

Merging a PR is the only publish mechanism — there is no separate publish step.

---

## Versioning

Plugin versions are managed by the plugin author in their own repository.
Registry releases must use immutable, versioned package URLs. The `source`
metadata can still identify the source repository and tag, but the core only
installs the archive declared in `package`.

### Update flow

When a plugin author ships a new release:

1. Publish a new bounded `.tar.gz` package for version `1.2.0`
2. Compute its SHA-256 digest
3. Open a PR updating `version`, `package.url`, and `package.sha256`
4. CI validates the archive and digest before the registry points to it

The core compares each user's active version with the registry and exposes an
explicit update action. The previous version is retained for rollback.
