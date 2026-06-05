# helpcore — CLI Design

The CLI is a native macOS binary (dev) and Linux binary (prod). It is a pure
HTTP client — it always talks to a running helpcore server over REST + SSE.

Binary: `helpcore` (alias: `hc`)

---

## Default behaviour

Running `helpcore` with no arguments opens the TUI chat interface, resuming
the last active conversation. Every other subcommand is a one-shot operation
that prints output and exits.

```bash
helpcore              # TUI chat (resume last conversation)
helpcore --new        # TUI chat (new conversation)
```

---

## Command reference

### Auth
```bash
helpcore login [--server https://your-server.com]
helpcore logout
helpcore setup          # first-admin wizard (calls the setup endpoint)
helpcore whoami         # current user, server URL, session info
helpcore status         # server health, version, connected user
```

### Chat (non-interactive)
```bash
helpcore ask "question"               # one-shot, streams response, exits
helpcore ask "question" --json        # raw JSON output (scriptable)
helpcore ask "question" --provider anthropic-main
```

### Conversations
```bash
helpcore conversations list
helpcore conversations delete <id>
```

### API keys
```bash
helpcore api-keys list
helpcore api-keys create --name "obsidian bridge"
helpcore api-keys revoke <id>
```

### Providers (admin)
```bash
helpcore providers list
helpcore providers grant --user <id> --provider <id>
helpcore providers revoke --user <id> --provider <id>
```

### Users (admin)
```bash
helpcore users list
helpcore users create --email <email>
helpcore users deactivate <id>
helpcore users delete <id>
helpcore users reset-password <id>
```

---

## Server URL resolution

Resolved in this order (each level overrides the previous):

1. Stored value in `~/.helpcore/credentials` (written by `helpcore login`)
2. Env var `HELPCORE_SERVER=https://...`
3. `--server https://...` flag on any command

---

## Credentials file

`~/.helpcore/credentials` — permissions `600`, never committed.

```toml
server_url    = "https://helpcore.example.com"
access_token  = "..."
refresh_token = "..."
```

Token refresh is handled transparently by the CLI on every request.

---

## TUI chat interface

Built with **ratatui** + **crossterm**. Tokio async runtime handles SSE
streaming concurrently with keyboard input.

### Layout

```
┌──────────────────────────────────────────────────────────────┐
│ helpcore  ·  Martin's Server               anthropic / claude │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  You                                                         │
│  install the home-assistant plugin                           │
│                                                              │
│  Assistant                                                   │
│  I'll install the Home Assistant plugin. It needs one        │
│  permission: outbound_http → homeassistant.local             │
│                                                              │
│  Allow? (yes / no)                                           │
│                                                              │
│  You                                                         │
│  yes                                                         │
│                                                              │
│  Assistant                                                   │
│  Done. Home Assistant is now active. Try: /ha lights on ▌    │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ > _                                              /help  esc  │
└──────────────────────────────────────────────────────────────┘
```

Three regions:
- **Header** — server name, active provider + model
- **Messages** — scrollable conversation history; streaming responses append
  character-by-character with a cursor indicator (`▌`)
- **Input** — single-line input with prompt `>`

### Keyboard shortcuts

| Key | Action |
|---|---|
| `Enter` | Send message |
| `Ctrl+C` / `Esc` | Quit |
| `Ctrl+L` | Clear message display (does not delete conversation) |
| `↑ / ↓` | Scroll message history |
| `PgUp / PgDn` | Scroll faster |
| `Ctrl+N` | New conversation |
| `Ctrl+R` | Search conversation history |
| `Tab` | Autocomplete slash commands |

### Slash commands in TUI

Type `/` to see available slash commands (discovered from the server's
`GET /api/slash-commands` endpoint). Registered by installed plugins.

Built-in TUI slash commands (not sent to server):
- `/new` — start a new conversation
- `/exit` — quit
- `/help` — show available slash commands
- `/history` — show conversation list and switch

### Streaming

SSE chunks arrive on a background tokio task and are sent to the render loop
via an `mpsc` channel. The TUI re-renders on each chunk, appending to the
current assistant message in the messages pane. A `▌` cursor is shown at the
end of the streaming message until the stream completes.

### Colours

- Header bar — bold, inverted
- `You` label — blue
- `Assistant` label — green
- Streaming cursor `▌` — dimmed
- Slash command suggestions — yellow
- Error messages — red

---

## One-shot mode (`ask`)

For scripting and piping. Does not open the TUI.

```bash
# Pipe output to another tool
helpcore ask "summarise this" < notes.txt | pbcopy

# JSON output for programmatic use
helpcore ask "list 3 tasks" --json | jq '.text'

# Use a specific provider
helpcore ask "write a Rust function" --provider local-ollama
```

Exits with code `0` on success, `1` on error.

---

## Crate location

`cli/` in the Cargo workspace. Depends on `crates/helpcore-api` for shared
request/response types. Does not depend on `core/` — the server is always
remote.
