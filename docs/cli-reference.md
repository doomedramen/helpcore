# CLI reference

The `hc` command (binary: `helpcore`) is a pure HTTP client that talks to a running helpcore server.

```
hc <command> [options]
hc --help
hc <command> --help
```

Credentials are stored at `~/.helpcore/credentials` after `hc login`. The access token is automatically refreshed when it expires.

---

## Auth

### `hc login`

Log in to a helpcore server and save credentials.

```bash
hc login
hc login --server http://your-server:3000
hc login --server http://your-server:3000 --email you@example.com
```

Options:
- `--server <url>` — server base URL (overrides stored value)
- `--email <email>` — skip the email prompt

### `hc logout`

Log out and clear stored credentials.

```bash
hc logout
```

### `hc completions <shell>`

Print a shell completion script for `bash`, `zsh`, `fish`, `elvish`, or `powershell`. The script is keyed to whichever binary name you invoke it as (`hc` or `helpcore`).

```bash
# zsh — write to a directory on your $fpath
hc completions zsh > ~/.zfunc/_hc

# bash
hc completions bash > /etc/bash_completion.d/hc

# fish
hc completions fish > ~/.config/fish/completions/hc.fish
```

### `hc setup`

First-admin wizard. Run once after a fresh server install to create the admin account.

```bash
hc setup --server http://your-server:3000
hc setup --url "http://your-server:3000/setup?token=abc123"
```

Options:
- `--server <url>` — server base URL (will prompt for the setup token)
- `--url <full-url>` — the full setup URL printed by the server on first start

### `hc status`

Show the configured server URL and verify the session against the server (refreshing the access token if needed). Prints the signed-in user and role on success, or why the connection failed.

```bash
hc status
# → Server: http://your-server:3000
# → Status: connected
# → User:   Martin (martin@example.com)
# → Role:   admin
```

---

## Chat

### `hc ask`

One-shot chat. Streams the response to stdout and exits.

```bash
hc ask "what is the capital of France?"
hc ask what is the capital of France   # quotes optional for multi-word queries
```

Options:
- `-c, --conversation <id>` — continue an existing conversation
- `-p, --provider <id>` — use a specific provider (overrides server default)
- `-m, --model <name>` — use a specific model
- `--server <url>` — server URL (overrides stored credentials)

The conversation ID is printed at the end of each response. Use it with `-c` to continue.

```bash
hc ask "let's plan a project"
# → ... response ...
# → conversation: 01jx5a2b3c4d...

hc ask -c 01jx5a2b3c4d "add a testing phase"
```

---

## Conversation management

### `hc conversation ls`

List your conversations, most recently updated first — useful for finding the ID of a conversation you want to continue or compact.

```bash
hc conversation ls
# → 01jx5a2b3c4d    8     msgs  2026-06-08T10:32:01Z  Plan the testing phase
```

### `hc conversation show <id>`

Print the messages in a conversation as a plain-text transcript.

```bash
hc conversation show 01jx5a2b3c4d
```

### `hc compact`

Summarise the oldest messages in a conversation to free up context space. Run this when a long conversation starts getting slow or the model seems to forget earlier context.

```bash
hc compact --conversation 01jx5a2b3c4d
# → ✓ Compacted 12 messages into a 847-character summary.
```

Options:
- `-c, --conversation <id>` — conversation to compact (required)
- `--server <url>` — server URL

---

## Personality

These commands manage the markdown files injected into the AI context on every message.

### `hc soul`

The assistant's tone, values, and communication style. Applies to all conversations.

```bash
hc soul                    # print current content
hc soul --edit             # open in $EDITOR
hc soul --set "Be concise and direct. No filler phrases."
```

### `hc identity`

Who the assistant is — its name, role, and relationship to you.

```bash
hc identity
hc identity --edit
hc identity --set "You are Kira, my personal assistant."
```

### `hc me`

Facts about you that the AI should always know.

```bash
hc me
hc me --edit
hc me --set "I'm a software engineer. I use macOS and prefer Rust."
```

All three commands accept:
- `--set <text>` — write directly without opening an editor
- `--edit` — open `$EDITOR` with the current content
- `--server <url>` — server URL

---

## Memory files

Arbitrary markdown files stored per-user on the server and injected into context.

### `hc memory ls`

List all memory files.

```bash
hc memory ls
# → notes/work.md         (1.2 KB)
# → home/devices.md       (0.4 KB)
# → projects/helpcore.md  (2.1 KB)
```

### `hc memory get <path>`

Print a memory file.

```bash
hc memory get notes/work.md
```

### `hc memory set <path>`

Create or overwrite a memory file.

```bash
hc memory set notes/work.md --set "Working on helpcore. Next: voice plugin."
hc memory set notes/work.md --edit     # open in $EDITOR
hc memory set notes/work.md            # read from stdin
```

Options:
- `--set <text>` — write directly
- `--edit` — open `$EDITOR`
- If neither is given, reads from stdin

### `hc memory rm <path>`

Delete a memory file.

```bash
hc memory rm notes/work.md
```

All memory commands accept `--server <url>`.

---

## Plugins

### `hc plugin ls`

List all plugins registered on the server for your account.

```bash
hc plugin ls
# → my-bridge-plugin  My Bridge  v1.0.0  bridge  enabled
```

### `hc plugin token <plugin-id>`

Generate a scoped bearer token for a bridge plugin. Tokens are `hcp_`-prefixed and can only call the APIs the plugin was granted access to.

```bash
hc plugin token my-bridge-plugin
# → hcp_abc123...
```

Options:
- `--permissions <list>` — comma-separated permission list (defaults to the plugin's declared permissions)
- `--server <url>` — server URL

Copy the token into the bridge service's `HELPCORE_TOKEN` environment variable.

### `hc plugin enable <plugin-id>`

Enable a plugin for your account.

```bash
hc plugin enable my-bridge-plugin
```

### `hc plugin disable <plugin-id>`

Disable a plugin without uninstalling it.

```bash
hc plugin disable my-bridge-plugin
```

---

## Global flags

Most commands accept:

| Flag | Description |
|---|---|
| `--server <url>` | Override the stored server URL for this invocation |
| `--help` | Show help for the command |

---

## Credential file

Stored at `~/.helpcore/credentials` (mode 600, never committed):

```toml
server_url    = "http://your-server:3000"
access_token  = "..."
refresh_token = "..."
```

Override the server URL for a single command without editing this file using `--server`. Override it permanently with `hc login --server <url>`.

The `HELPCORE_CREDENTIALS` environment variable can point to an alternative credentials file (used in testing).
