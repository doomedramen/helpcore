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
https://raw.githubusercontent.com/martinsmith/helpcore/main/registry/plugins.json
```

Operators can point `[registry] url` at their own JSON file to run a private
registry (self-hosted plugins, corporate use, etc.).

---

## Plugin entry format

```json
{
  "id": "home-assistant",
  "name": "Home Assistant",
  "description": "Control your Home Assistant instance through the AI.",
  "version": "1.0.0",
  "tier": "bridge",
  "author": "martinsmith",
  "homepage": "https://github.com/martinsmith/helpcore-plugin-home-assistant",
  "permissions": ["outbound_http"],
  "source": {
    "type": "github",
    "repo": "martinsmith/helpcore-plugin-home-assistant",
    "ref": "1.1.1",
    "wasm_asset": "plugin.wasm"
  },
  "setup_guide": "https://github.com/martinsmith/helpcore-plugin-home-assistant#setup"
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
| `type` | both | `github`, `url`, or `local` (dev only) |
| `repo` | `github` | `owner/repo` |
| `ref` | `github` | release tag (e.g. `1.1.1`) or branch name (e.g. `main`) |
| `wasm_asset` | `wasm` tier | filename of the WASM binary |
| `url` | `url` type | direct download URL for the WASM binary |

Bridge plugins have no WASM to download. The `source` field records where
the bridge code lives so the user can deploy it. The core prompts for the
bridge endpoint URL during install.

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
  → resolves WASM download URL from source.repo + source.wasm_asset
  → downloads plugin.wasm, verifies checksum against manifest
  → presents permissions to user for approval
  → stores WASM in {data_dir}/plugins/wasm/{id}/{version}/plugin.wasm
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
  → writes plugin_installs row with bridge URL
  → generates scoped token for this user+plugin
```

---

## Submitting a plugin (community)

Anyone can submit a plugin by opening a pull request that adds an entry to
`registry/plugins.json`.

Requirements for a PR to be accepted:

- Entry passes JSON schema validation (CI enforces this)
- `id` is unique, lowercase, hyphen-separated
- `homepage` links to a public repository with source code
- `permissions` accurately lists everything the plugin requests — do not
  under-declare
- For `wasm` tier: a published GitHub Release with the named WASM asset must
  exist at the time of submission
- For `bridge` tier: a `setup_guide` URL with clear deployment instructions

---

## Repository layout

```
registry/
  plugins.json        ← the registry (one entry per plugin)
  schema.json         ← JSON Schema for entry validation
  CONTRIBUTING.md     ← plugin submission guidelines
```

---

## CI validation

Every PR that touches `registry/plugins.json` runs:

- JSON schema validation against `registry/schema.json`
- Duplicate `id` check
- For `wasm` tier entries: HTTP HEAD request to verify the WASM asset URL resolves
- Lint: `description` and `name` present and non-empty

Merging a PR is the only publish mechanism — there is no separate publish step.

---

## Versioning

Plugin versions are managed by the plugin author in their own repository.
The `source.ref` field accepts either a **release tag** or a **branch name**.

### Release tag (stable)

```json
"source": { "type": "github", "repo": "owner/plugin", "ref": "1.1.1", "wasm_asset": "plugin.wasm" }
```

Core downloads the WASM from:
```
https://github.com/{repo}/releases/download/{ref}/{wasm_asset}
```

Pinned to an immutable release. Preferred for plugins in the public registry.

### Branch name (rolling)

```json
"source": { "type": "github", "repo": "owner/plugin", "ref": "main", "wasm_asset": "plugin.wasm" }
```

Core downloads the WASM from:
```
https://raw.githubusercontent.com/{repo}/{ref}/{wasm_asset}
```

Tracks the tip of the branch. Useful during development or for plugins that
commit their WASM binary to the repo. Less stable — the binary can change
without a version bump.

### Update flow

When a plugin author ships a new release:

1. Cut a new GitHub Release (tag `1.2.0`) with the new WASM binary as an asset
2. Open a PR updating `version` and `source.ref` in `registry/plugins.json`
3. CI verifies the asset URL resolves
4. PR merged → registry points to the new version

The core stores the installed `ref` in `plugin_installs.version`. On startup
(or on demand), the core compares installed refs against the registry and
notifies the user via chat when an update is available.
