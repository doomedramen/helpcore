# Configuration reference

helpcore is configured via `config.toml`. Create it from the example:

```bash
cp config.toml.example config.toml
```

The server reads the path set by `HELPCORE_CONFIG`, falling back to
`~/.helpcore/config.toml`.

> **Docker users:** see the note at the bottom about bind mounts.

---

## `[server]`

```toml
[server]
name = "My helpcore"
url  = "http://localhost:3000"
port = 3000
```

| Key | Default | Description |
|---|---|---|
| `name` | `"helpcore"` | Display name shown in the setup wizard |
| `url` | `"http://localhost:3000"` | Public base URL of this server; used when printing the first-run setup URL |
| `port` | `3000` | Port to listen on; must match the Docker `ports` mapping |

---

## `[data]`

```toml
[data]
# dir = "/data"
```

| Key | Default | Description |
|---|---|---|
| `dir` | `~/.helpcore/data` | Directory for the SQLite database and uploaded files. Docker sets this to `/data` via the `HELPCORE_DATA` env var. Leave unset for the default on native installs. |

---

## `[logging]`

```toml
[logging]
level = "info"
```

| Key | Default | Description |
|---|---|---|
| `level` | `"info"` | Log verbosity: `trace`, `debug`, `info`, `warn`, `error` |

---

## `[plugins]`

```toml
[plugins]
blacklist = []
```

| Key | Default | Description |
|---|---|---|
| `blacklist` | `[]` | Plugin IDs that can never be installed on this server, regardless of who requests them |

### Local plugins

Each local plugin needs a `[[plugins.local]]` block pointing to a directory containing `manifest.toml`:

```toml
[[plugins.local]]
id      = "voice-kittentts"
path    = "plugins/voice-kittentts"   # relative to the server working directory
enabled = true
```

| Key | Description |
|---|---|
| `id` | Must match the `id` in `manifest.toml` |
| `path` | Path to the plugin directory |
| `enabled` | Whether to activate it for all users at startup |

The plugin is registered for every existing user when helpcore starts. The `skill.md` (if present) is injected into every chat context when the plugin is enabled.

---

## `[registry]`

```toml
[registry]
url = "https://raw.githubusercontent.com/doomedramen/helpcore/main/registry/plugins.json"
```

| Key | Default | Description |
|---|---|---|
| `url` | Public helpcore registry | Direct URL to a registry JSON file. It may also be a `file://` URL for local development. |

The admin web UI reads this catalog to show available, installed, and
server-blocked plugins.

---

## `[[providers]]`

At least one provider with `roles = ["chat"]` is required. Multiple providers can be listed.

```toml
[[providers]]
id            = "ollama"
name          = "Ollama"
type          = "ollama"
default_model = "qwen2.5:3b"
roles         = ["chat"]
url           = "http://localhost:11434"
num_ctx       = 4096
# num_predict = 2048
```

### Common fields

| Key | Required | Description |
|---|---|---|
| `id` | yes | Unique identifier; clients use this to select a provider |
| `name` | yes | Human-readable display name |
| `type` | yes | Provider type: `ollama`, `anthropic`, `openai`, `openai_compatible` |
| `default_model` | yes | Model used when the client doesn't specify one |
| `roles` | yes | List of roles this provider serves; at least one must be `"chat"` |
| `num_ctx` | no | Context window size in tokens (default: 8192). Set to match the model's actual window. |
| `num_predict` | no | Max tokens to generate per response (default: 2048) |

### Ollama

```toml
[[providers]]
id            = "ollama"
name          = "Ollama"
type          = "ollama"
default_model = "qwen2.5:3b"
roles         = ["chat"]
url           = "http://localhost:11434"
num_ctx       = 4096
```

| Key | Description |
|---|---|
| `url` | Ollama server base URL |

**Common URL values:**

| Setup | URL |
|---|---|
| Ollama running natively, helpcore native | `http://localhost:11434` |
| Both in Docker Compose | `http://ollama:11434` |
| helpcore in Docker, Ollama on Docker Desktop host | `http://host.docker.internal:11434` |
| helpcore in Docker, Ollama on a separate server | `http://192.168.1.x:11434` |

### Anthropic

```toml
[[providers]]
id            = "anthropic"
name          = "Anthropic Claude"
type          = "anthropic"
api_key       = "sk-ant-..."
default_model = "claude-opus-4-5"
roles         = ["chat"]
```

| Key | Description |
|---|---|
| `api_key` | Anthropic API key |

### OpenAI

```toml
[[providers]]
id            = "openai"
name          = "OpenAI"
type          = "openai"
api_key       = "sk-..."
default_model = "gpt-4o"
roles         = ["chat"]
```

### OpenAI-compatible (LM Studio, vLLM, etc.)

```toml
[[providers]]
id            = "lm-studio"
name          = "LM Studio"
type          = "openai_compatible"
url           = "http://localhost:1234/v1"
default_model = "local-model"
roles         = ["chat"]
```

---

## Web administration

Admin users can open **Server settings** in the web sidebar to manage the
server identity, logging level, registry URL, plugin blacklist, and providers.
The server validates the submitted configuration and atomically replaces the
configured TOML file.

Provider API keys are never returned by the API. Leaving a key field blank
preserves the saved value; the UI provides an explicit option to remove it.
New keys are sent only in the authenticated admin update request and remain in
the config file rather than the database.

Configuration changes are persisted immediately but server, logging, registry,
blacklist, and provider changes take effect after restarting helpcore. Per-user
plugin enable/disable changes from the Plugins tab take effect immediately.

---

## Model RAM guide (Ollama)

Rough RAM requirements for the model itself (helpcore + OS overhead is ~200 MB additional):

| Model | RAM | Quality |
|---|---|---|
| `gemma2:2b` | ~1.6 GB | Good baseline |
| `qwen2.5:3b` | ~2 GB | Recommended all-rounder |
| `phi3.5:mini` | ~2.2 GB | Strong reasoning |
| `llama3.2:3b` | ~2 GB | General purpose |
| `llama3.1:8b` | ~6 GB | Much better quality |
| `mistral:7b` | ~5 GB | Solid instruction following |

---

## Environment variables

Any config value can be overridden with environment variables using the pattern `HELPCORE_<SECTION>_<KEY>` (uppercase, underscores). Examples:

| Env var | config.toml equivalent |
|---|---|
| `HELPCORE_DATA=/data` | `[data] dir = "/data"` |
| `HELPCORE_SERVER_PORT=3000` | `[server] port = 3000` |
| `HELPCORE_LOGGING_LEVEL=debug` | `[logging] level = "debug"` |

The Docker image sets `HELPCORE_DATA=/data` automatically.

---

## Docker config

The repository development stack and production stack both use a writable
directory mount at `./config`:

```bash
make config
# creates config/config.toml
```

`docker-compose.prod.yml` is intended for Dockge and other source-less servers.
It mounts `./config` as a directory. On first start, the image copies its
bundled default to `./config/config.toml`; edit that file and restart helpcore.

Older releases used a bind mount; Docker could create an empty `config.toml`
directory when the source file was missing.

The container entrypoint repairs ownership and modes on the config directory
and file before dropping to UID 100. The admin API reports whether atomic
same-directory replacement is possible; the web UI disables Save with the
deployment error when the mount is intentionally read-only. Saves use a unique
temporary file, `fsync`, atomic rename, and mode `0600`. Provider/server changes
still require an external restart.
