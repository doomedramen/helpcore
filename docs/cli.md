# helpcore — CLI Design

The CLI is a native macOS binary (dev) and Linux binary (prod). It is a pure
HTTP client — it always talks to a running helpcore server over REST + SSE.

Binary: `helpcore` (alias: `hc`)

---

## Default behaviour

Running `helpcore` requires a subcommand. There is no default TUI mode.

```bash
helpcore ask "hello"   # one-shot chat
helpcore login         # log in to a server
helpcore status        # check connection
```

---

## Command reference

### Auth
```bash
hc login [--server https://your-server.com] [--email user@example.com]
hc logout
hc setup [--url <setup-url>] [--server <url>]
hc status               # server URL, login state, version info
```

### Chat (one-shot)
```bash
hc ask "question"               # one-shot, streams response, exits
hc ask "question" -c <conv-id>  # continue an existing conversation
hc ask "question" --provider <id> --model <name>
```

### Conversation management
```bash
hc compact -c <conversation-id>   # summarise oldest messages to free context
```

### Personality files
```bash
hc soul               # print current soul
hc soul --set "text"  # write new soul directly
hc soul --edit        # open $EDITOR with current content

hc identity           # print identity
hc identity --set ...
hc identity --edit

hc me                 # print user profile
hc me --set ...
hc me --edit
```

### Memory files
```bash
hc memory ls                    # list all memory files
hc memory get notes.md          # print a memory file
hc memory set notes.md --content "text"  # create/overwrite
hc memory set notes.md --edit   # edit in $EDITOR
hc memory rm notes.md           # delete a memory file
```

### Plugin management
```bash
hc plugin ls                      # list installed plugins
hc plugin token voice-kittentts   # generate a bridge plugin token
hc plugin enable voice-kittentts  # enable a plugin
hc plugin disable voice-kittentts # disable a plugin
```

---

## Server URL resolution

Resolved in this order (each level overrides the previous):

1. Stored value in `~/.helpcore/credentials` (written by `hc login`)
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

Token refresh is handled transparently by the CLI for the `ask` command.

---

## One-shot mode (`ask`)

For scripting and piping. Does not open a TUI.

```bash
# Pipe output to another tool
hc ask "summarise this" < notes.txt | pbcopy

# JSON output for programmatic use
hc ask "list 3 tasks" --json | jq '.text'

# Use a specific provider
hc ask "write a Rust function" --provider local-ollama
```

Exits with code `0` on success, `1` on error.

---

## Crate location

`packages/cli/` in the Cargo workspace. Depends on `packages/api/` for shared
request/response types. Does not depend on `packages/server/` — the server is
always remote.
