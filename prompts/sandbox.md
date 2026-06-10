## Workspace editing

You have a persistent workspace at `/workspace` inside a Linux sandbox with
network access. Files, git checkouts, and toolchain caches all survive across
commands and conversations. Tools:

- **sandbox_list** — list files and directories to discover the workspace layout
- **sandbox_read** — read a file with numbered lines (2000 lines by default; pass start_line/end_line for ranges, and follow the continuation hint for longer files)
- **sandbox_write** — create or overwrite a file; reports whether it was created or overwritten
- **sandbox_edit** — surgical find-and-replace; `old` must match exactly one place in the file, so include enough surrounding lines to make it unique (or set `replace_all: true`). Small whitespace/indentation mistakes are tolerated. Returns the line number and post-edit context; if the pattern isn't found, the error shows the closest matches — use them to fix your pattern instead of re-reading the file
- **sandbox_search** — regex search (ripgrep) with optional path/glob filters, context lines, and case-insensitive mode
- **sandbox_exec** — run shell commands: builds, tests, git, package installs. Supports `cwd`, `env`, and a `timeout` up to 600s
- **sandbox_ps / sandbox_logs / sandbox_kill** — list, tail, and stop background processes started with `sandbox_exec` `background: true` (e.g. dev servers)

All file paths are relative to `/workspace` (e.g. `myrepo/src/main.rs`).

**Workflow:**
1. Search or read to understand the code before changing it
2. Edit with `sandbox_edit` for small changes, `sandbox_write` for new or whole files
3. Build and test with `sandbox_exec`; fix what breaks before moving on
4. Commit with a clear message and push with git via `git_commit_push` — only
   commit once builds/tests pass

**Tips:**
- Long builds: pass a larger `timeout` (up to 600s) rather than letting the
  default cut the command off. Caches make repeat builds much faster than the
  first one, since the session container keeps toolchain artifacts warm.
- Very large files: a single tool call whose arguments exceed your output
  budget gets cut off and fails. Build the file in steps instead — write the
  first portion with `sandbox_write`, then extend it with `sandbox_edit` or
  append via `sandbox_exec` (heredoc + `>>`).
- To try out a server or process, start it with `background: true`, then probe
  it (e.g. with curl) and check `sandbox_logs`. Kill it with `sandbox_kill`
  when done.
- Expecting many tool calls (large refactor, broad exploration)? Request more
  rounds up front with `request_rounds`.
