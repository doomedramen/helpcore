# helpcore — Interface Contract

This document defines what the core API must expose so any interface can be
built against it, and what each interface is expected to support.

---

## Design principle

**The AI is the interface to the core.**

Plugin store management, configuration, and discovery are not UI widgets —
they are built-in AI capabilities. The core ships with a built-in skill that
gives the AI knowledge of the store and tools to act on it. "Install the
home-assistant plugin" works identically in every interface because it is just
a chat message. Rich UIs (e.g. a web browse-and-click store) are progressive
enhancements, not requirements.

---

## Built-in core skills (not plugins)

These are always available to the AI regardless of which plugins a user has
installed. They are part of the core's own system prompt and tool set.

| Skill | Tools |
|---|---|
| Plugin store | `search_plugins`, `install_plugin`, `uninstall_plugin`, `enable_for_user`, `disable_for_user`, `list_installed`, `configure_plugin` |
| User & account | `get_profile`, `update_settings`, `list_sessions`, `revoke_session` |
| Conversation | `list_conversations`, `get_conversation`, `delete_conversation` |
| Provider | `list_providers`, `set_active_provider` |

---

## Universal features

Every interface — regardless of platform — must support these:

| Feature | Notes |
|---|---|
| Auth | Login, logout, session management |
| Chat | Send a message, stream the response (SSE) |
| Slash command invocation | Discover registered commands from core, invoke via `/command` |
| Plugin output rendering | Typed structured data from tool calls; plain-text fallback is mandatory |
| Conversation history | List and resume past conversations |

Everything else (plugin store, configuration, etc.) is accessible via natural
language through chat. No interface needs special UI for it.

---

## Plugin output rendering

Plugins return typed structured output: `{ "type": "calendar_event", "data": { ... } }`.

Each interface renders types it understands. Plain-text fallback is mandatory
for all interfaces — plugins must always provide a `text` fallback. The core
passes output through without rendering it.

```
Plugin → structured output → Core (passes through) → Interface (renders)
```

---

## Interface capability matrix

| Feature | Web | CLI | WhatsApp / Telegram | Smart speaker | iOS (planned) |
|---|---|---|---|---|---|
| Auth | ✓ | ✓ | via bridge plugin | via voice profile | ✓ |
| Chat + streaming | ✓ | ✓ | ✓ (text only) | ✓ (voice) | ✓ |
| Slash commands | ✓ | ✓ | `/command` prefix | voice trigger | ✓ |
| Plain-text output | ✓ | ✓ | ✓ | ✓ | ✓ |
| Rich card output | ✓ | — | — | — | ✓ |
| Markdown rendering | ✓ | partial | — | — | ✓ |
| Code highlighting | ✓ | ✓ | — | — | — |
| File upload | ✓ | ✓ | limited | — | ✓ |
| Store browse UI | ✓ | — | — | — | ✓ |
| Raw JSON mode | — | ✓ | — | — | — |
| Pipe / scriptable | — | ✓ | — | — | — |
| Multi-user detection | — | — | — | per voice profile | — |
| Native notifications | — | — | ✓ (platform) | — | ✓ |

---

## Interface notes

### Web (Next.js)
Primary rich interface. Implements the full feature set including a visual
plugin store, rich card rendering, and markdown. Built against the REST + SSE
API directly.

### CLI
Primary power-user interface and the first interface built alongside the core.
Pipe-friendly (`--json` flag for raw output), scriptable, supports streaming.
Runs natively on macOS in development; connects to the server over HTTP.

### WhatsApp / Telegram
Delivered as Tier 2 bridge plugins — separate services that receive messages
from the platform and POST them to the core as regular chat requests. The
platform's own formatting constraints apply. Plugin store and configuration
are done via natural language chat.

### Smart speaker (future)
Requires voice I/O layer and per-voice user profile detection (analogous to
Alexa/Siri household profiles). Multi-user routing happens at the bridge level
before the request reaches the core.

### iOS (planned)
Folder and architecture placeholder. Will be a Swift client consuming the
same REST + SSE API as the web app. Native notifications, Siri shortcuts,
and rich card rendering.

---

## API endpoints the core must expose for interfaces

| Endpoint | Method | Purpose |
|---|---|---|
| `/api/auth/login` | POST | Obtain a session token |
| `/api/auth/logout` | POST | Revoke session |
| `/api/chat` | POST | Send message, stream response (SSE) |
| `/api/providers` | GET | List active chat providers available to the authenticated user |
| `/api/conversations` | GET | List conversations |
| `/api/conversations/:id/messages` | GET | Get persisted conversation history and message states |
| `/api/conversations/:id` | DELETE | Delete conversation |
| `/api/conversations/:id/messages/:message_id/retry` | POST | Retry a failed/interrupted assistant turn |
| `/api/slash-commands` | GET | Discover all registered slash commands |
| `/api/plugins` | GET | List installed plugins for the user |
| `/api/plugins/store` | GET | Browse the configured store |
| `/api/plugins/:id/install` | POST | Install disabled for the current user |
| `/api/plugins/:id/update` | POST | Verify and activate an explicit update |
| `/api/plugins/:id/rollback` | POST | Swap to the retained prior version |
| `/api/plugins/:id/config` | PUT | Save per-user bridge settings and encrypted credentials |
| `/api/plugins/:id/enable` | PUT | Enable or disable the user's install |
| `/api/plugins/:id` | DELETE | Remove only the user's install and versions |
| `/api/admin/config` | GET, PUT | Admin-only global config, registry, and blacklist policy |
| `/api/health` | GET | Server health check |
