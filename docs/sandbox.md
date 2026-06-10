# Execution Sandbox

The sandbox gives the assistant a persistent Linux environment for working on
code: reading and editing files, running builds and tests, using git, and
managing long-running processes. It is implemented in
`packages/server/src/sandbox/mod.rs` (container lifecycle) and exposed as
built-in tools in `packages/server/src/plugins/runtime.rs`.

## Architecture

Commands run inside a single long-lived **session container** via the Docker
exec API — not a fresh container per command. This keeps per-command latency
in the tens of milliseconds and lets toolchain caches stay warm.

- **Session container** — named `helpcore-sandbox-session`, runs
  `sleep infinity` under an init process (`init: true`, so zombies are
  reaped). Created lazily on first use; removed after 30 minutes idle; torn
  down and recreated when the image or resource limits change. A leftover
  container from a previous server run is replaced on startup thanks to the
  fixed name.
- **Workspace volume** — the named volume `helpcore-sandbox-workspace` is
  mounted at `/workspace`. Files, git checkouts, and caches survive container
  recreation and server restarts. Cargo/pip/npm caches are redirected under
  `/workspace/.cache` via container environment.
- **Image** — `helpcore-sandbox:latest` by default; build it with
  `make sandbox-image`. Any image works as long as it provides `/bin/sh` and
  coreutils `timeout`. The bundled image (`docker/sandbox.Dockerfile`) adds
  git, rust (clippy/rustfmt), python, node, ripgrep, sqlite, and common build
  tools.
- **Session init** — when a fresh container starts, an init script applies
  the configured git identity and credential helper and clones any
  `sandbox.repos` that aren't already present in `/workspace`. Init failures
  are logged but never block the sandbox.

## Tools

The `sandbox_*` tools and the accompanying system-prompt guidance
(`prompts/sandbox.md`) are only presented to the model when the sandbox is
enabled and Docker is reachable; a disabled sandbox is invisible rather than
a source of failing calls.

| Tool | Purpose |
| --- | --- |
| `sandbox_list` | Recursive directory listing (depth-limited, skips `.git`) |
| `sandbox_read` | Read a file with numbered lines; line ranges, 2000-line default window with continuation hint, long lines clipped, did-you-mean on missing paths |
| `sandbox_write` | Create/overwrite a file (content travels via exec stdin, so size is not limited by kernel argument limits) |
| `sandbox_edit` | Find-and-replace computed server-side; ambiguous patterns are rejected with match locations; "not found" errors include closest near-matches |
| `sandbox_search` | Regex search via ripgrep (grep fallback) with path/glob filters |
| `sandbox_exec` | Run a shell command; supports `timeout`, `cwd`, `env`, and `background` |
| `sandbox_ps` / `sandbox_logs` / `sandbox_kill` | List, tail, and stop background processes |

`sandbox_edit` matches `old` with a strict-to-forgiving strategy pipeline —
exact, indentation-flexible (relative indent preserved), line-trimmed, then
whitespace-normalized — so slightly mis-quoted whitespace still finds the
right block, and the response reports which strategy matched. A pattern that
matches more than one location is an error (listing the line numbers) unless
`replace_all` is set: the tool never guesses between occurrences.

Background processes are started with `setsid` in their own session and
process group; their output is captured to `/tmp/helpcore-proc/<id>.log`
inside the container (this state intentionally lives and dies with the
container, unlike `/workspace`).

## Configuration

```toml
[sandbox]
enabled   = true
image     = "helpcore-sandbox:latest"
timeout   = 120        # default per-command seconds (commands may request up to 600)
memory_mb = 4096
cpus      = 2.0
# host    = "tcp://192.168.1.10:2375"   # remote Docker daemon
# repos   = ["https://github.com/you/project.git"]

[sandbox.git]
# user_name  = "My Assistant"
# user_email = "assistant@example.com"
# token      = "github_pat_..."        # HTTPS git credential, kept in container env
# token_user = "x-access-token"
```

`image`, `timeout`, and `memory_mb` are also editable at runtime from the
admin UI; the session container is recreated on the next command after a
change.

## Execution semantics

- **Timeouts** are enforced in-container with `timeout(1)`, plus a host-side
  backstop that recreates the container if it stops responding entirely.
- **Output** is capped per stream (100 kB by default), keeping the first 40%
  and last 60% so the end of build/test output — where the errors are —
  survives truncation.
- **Network access is enabled** (bridge networking): cloning, pushing, and
  package installs are part of the intended workflow.

## Security posture

- The container is unprivileged with hard resource caps (memory, CPU,
  pids_limit 512) and no host bind mounts — only the named workspace volume.
- Commands run as root *inside* the container; isolation comes from the
  container boundary, not in-container privileges.
- The git token is injected as container environment and read by a credential
  helper; it is never written to a file inside the container. Scope the token
  to the repositories the assistant should touch.
- Anything inside `/workspace` is writable by any sandboxed command — treat
  the volume as assistant-owned state.
- When mounting the Docker socket into a containerized server, consider a
  socket proxy (e.g. `tecnativa/docker-socket-proxy`) and point `sandbox.host`
  at it instead of exposing the raw socket.

## Testing

Unit tests live alongside the code. Live end-to-end tests (require Docker and
the sandbox image) are ignored by default:

```sh
cargo test -p helpcore-server --test sandbox_live -- --ignored --test-threads=1
```
