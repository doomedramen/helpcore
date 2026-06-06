# helpcore — Providers & Context Assembly

Reference implementation: `references/zeroclaw/crates/zeroclaw-providers/`
Personality templates: `references/zeroclaw/crates/zeroclaw-runtime/src/agent/personality_templates/`

---

## What a provider is

A provider is a raw LLM connection. It has a type (which API it speaks), one or
more roles (what it is used for), and credentials. Providers are persisted in
the server config file and can be managed there or through the admin-only web
interface. Credentials are never returned by the API and are never stored in
the database; a new credential crosses the authenticated admin API only when it
is submitted.

Admin controls which providers each user can access. Users are granted access
per-provider by the admin.

---

## Provider roles

A provider is assigned one or more roles. When the core or a plugin needs a
capability, it routes to the first available provider with the matching role for
that user.

| Role | Purpose |
|---|---|
| `chat` | General conversation — the default |
| `code` | Coding assistance |
| `image_gen` | Image generation |
| `video_gen` | Video generation |
| `embeddings` | Vector embeddings for memory / RAG |

A provider can hold multiple roles (e.g. OpenAI covers `chat` + `image_gen`).

---

## Server config

Providers are declared in `~/.helpcore/config.toml` (or the path selected by
`HELPCORE_CONFIG`). Credentials are stored here only.

```toml
[[providers]]
id          = "anthropic-main"
name        = "Anthropic Claude"
type        = "anthropic"
roles       = ["chat", "code"]
api_key     = "sk-ant-..."
default_model = "claude-sonnet-4-6"

[[providers]]
id          = "openai-main"
name        = "OpenAI"
type        = "openai"
roles       = ["chat", "image_gen"]
api_key     = "sk-..."
default_model = "gpt-4o"

[[providers]]
id          = "local-ollama"
name        = "Local Ollama"
type        = "ollama"
roles       = ["chat"]
url         = "http://localhost:11434"
default_model = "llama3.2"
```

Supported types at launch: `anthropic`, `openai`, `ollama`, `openai_compatible`
(covers any provider that speaks the OpenAI chat completions API).

---

## Per-user access control

Admin grants users access to specific providers. A user only sees and can use
providers they have been granted.

When a user's request needs a role (e.g. `chat`), the core picks the first
provider with that role that the user has been granted. If the user has set a
preferred provider for a role, that is tried first. On failure, the next
available provider with the same role is tried (fallback chain).

---

## Reliability

Every provider is wrapped in a reliability layer (adapted from zeroclaw's
`ReliableModelProvider`):

- **Retry with exponential backoff** — transient errors (5xx, timeouts, 429)
  are retried up to a configured limit
- **Non-retryable detection** — 4xx errors, auth failures, and model-not-found
  errors abort immediately without burning retry budget
- **Provider fallback** — if all retries are exhausted, the next provider with
  the same role is tried
- **Context truncation** — if context window is exceeded, oldest non-system
  messages are dropped and the request is retried
- **Empty completion re-roll** — blank responses are retried rather than
  returned to the user

---

## Context assembly

Every request, regardless of provider or role, receives the same assembled
context. This is what makes providers interchangeable — switching from Anthropic
to Ollama is seamless because the context is identical.

```
1. Core system instructions    (hardcoded — security rules, tool dispatch)
2. SOUL.md                     (per-user — personality, values, communication style)
3. IDENTITY.md                 (per-user — AI name, vibe)
4. USER.md                     (per-user — facts about this user)
5. Memories                    (retrieved — relevant entries for this conversation)
6. Plugin skill fragments      (per-user installed plugins — prompt fragments)
7. Tool definitions            (per-user installed plugins — JSON Schema)
──────────────────────────────────────────────────────────
8. Conversation history
9. Current message
```

---

## Soul / personality system

Each user has a set of editable markdown personality files. These are created
from default templates when the account is created and can be edited at any time
via chat or the web UI.

| File | Purpose |
|---|---|
| `SOUL.md` | Core values, behaviour, communication style — who the AI is |
| `IDENTITY.md` | AI name, vibe, persona |
| `USER.md` | Facts about the user that the AI should always know |

Stored in the DB (`user_personality` table) as markdown text. Included verbatim
in every system prompt, for every provider.

The default templates are shipped with the core and rendered with per-user
placeholders at account creation time:

| Placeholder | Value |
|---|---|
| `{agent}` | AI name the user chose (e.g. "Nova") |
| `{user}` | User's display name |
| `{tz}` | User's timezone |
| `{comm_style}` | Communication style preference set during onboarding |

---

## Database schema additions

### `user_providers`
Per-user provider access grants.

| Column | Type | Notes |
|---|---|---|
| user_id | UUID | FK → users.id |
| provider_id | TEXT | Matches `id` in config.toml |
| enabled | BOOL | Admin can revoke without deleting |
| preferred_for_roles | JSON | e.g. `["chat"]` — user preference |
| granted_by | UUID | FK → users.id (the admin who granted) |
| granted_at | TIMESTAMP | |

### `user_personality`
Per-user soul / personality files.

| Column | Type | Notes |
|---|---|---|
| user_id | UUID | FK → users.id |
| filename | TEXT | `SOUL.md`, `IDENTITY.md`, `USER.md` |
| content | TEXT | Markdown |
| updated_at | TIMESTAMP | |

Primary key: `(user_id, filename)`.
