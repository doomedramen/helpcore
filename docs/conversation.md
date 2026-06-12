# helpcore — Conversation & Context Model

---

## Conversations

Conversations are stored in SQLite. Each conversation belongs to one user.

### Database schema

#### `conversations`

| Column | Type | Notes |
|---|---|---|
| id | UUID | Primary key |
| user_id | UUID | FK → users.id |
| title | TEXT | Auto-generated from first user message (truncated to ~60 chars) |
| provider_id | TEXT | Last used provider |
| model | TEXT | Last used model |
| message_count | INTEGER | Maintained as messages are inserted |
| created_at | TIMESTAMP | |
| updated_at | TIMESTAMP | Updated on each new message |

#### `messages`

| Column | Type | Notes |
|---|---|---|
| id | UUID | Primary key |
| conversation_id | UUID | FK → conversations.id |
| role | TEXT | `user`, `assistant`, `tool`, `system`, `summary` |
| content | TEXT | Message text or tool result |
| tool_call_id | TEXT | For `tool` role messages — links to the call |
| tool_calls | JSON | For `assistant` messages that invoke tools |
| provider_id | TEXT | Which provider generated this (assistant messages) |
| model | TEXT | Which model generated this |
| status | TEXT | `pending`, `streaming`, `awaiting_input`, `complete`, `failed`, or `interrupted` |
| error | TEXT | Terminal generation error, when present |
| updated_at | TIMESTAMP | Last persisted chunk or state transition |
| sequence | INTEGER | Ordering within the conversation |
| created_at | TIMESTAMP | |

The `summary` role is a special system-inserted message that replaces a
compacted history segment (see Context overflow below).

Generation is server-owned. The user message and an assistant placeholder are
created in one transaction before the provider starts. Provider work continues
after an SSE disconnect, chunks are persisted before they are emitted, and the
UI reloads/polls message history while an assistant row is active. On startup,
unfinished rows become `interrupted` and can be retried against the original
user turn. Only one assistant generation may be active per conversation.

---

## Memory

Memory files are stored in the SQLite database (`memory_files` table) and
searched at query time via SQLite FTS5. The AI manages memory directly — it
decides what to write, how to name files, and how to organise them.

One `.md` file per person, project, or topic. Paths use `parent/key` notation
(flat by default, subdirectories when justified). The AI owns the structure.

### Built-in memory tools (core-level, not plugins)

The core exposes these tools to the AI for memory management:

| Tool | Purpose |
|---|---|
| `memory_search(query)` | FTS5 search across all memory files |
| `memory_read(path)` | Read a specific memory file |
| `memory_write(path, content)` | Write or overwrite a memory file |
| `memory_append(path, content)` | Append to an existing memory file |
| `memory_list(path?)` | List files/directories in memory root or subdir |
| `memory_move(from, to)` | Move or rename a file (ask user first) |
| `memory_delete(path)` | Delete a file (ask user first) |

All paths are relative to the user's memory root. The core enforces the
per-user boundary — the AI cannot access another user's memory directory.

### FTS5 index

An FTS5 virtual table in SQLite mirrors the memory filesystem content.
Updated whenever a file is written via `memory_write` or `memory_append`.
Used for top-N retrieval at query time.

```sql
CREATE VIRTUAL TABLE memory_fts USING fts5(
    content,
    filename,
    content='memory_files',
    content_rowid='rowid'
);

CREATE TABLE memory_files (
    id INTEGER PRIMARY KEY,
    user_id UUID NOT NULL,
    path TEXT NOT NULL,         -- relative path within user's memory dir
    content TEXT NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    UNIQUE (user_id, path)
);
```

---

## Context assembly

Every LLM request assembles the same context structure regardless of provider
or role. See [`docs/providers.md`](providers.md) for the full layer list.

### Memory retrieval

Before each request, the core performs a top-N FTS5 search against the user's
memory files. The query is derived from the current user message plus the most
recent exchange (for conversational continuity).

Default: top 10 results, configurable per user. Results are included in the
system prompt under a `## Memories` section.

### System prompt assembly

```
## Core instructions        (hardcoded)
## Soul                     (SOUL.md from user_personality)
## Identity                 (IDENTITY.md from user_personality)
## User                     (USER.md from user_personality)
## Memories                 (top-N FTS5 results from memory files)
## Skills                   (plugin skill fragments — one per enabled plugin)
## Tools                    (tool definitions JSON Schema — from enabled plugins
                             + built-in memory tools)
## Current date             (date + timezone from user profile)
```

### Conversation history loading

Messages are loaded from the `messages` table ordered by `sequence`. Summary
messages appear inline as system messages where the compacted segment was.

Tool-use/tool-result pairs are always kept atomic — orphaned tool results
(whose call was dropped) are removed before sending to the provider, as they
cause API 400 errors.

Ollama tool calls are persisted as an assistant message with `tool_calls`,
followed by `tool` role result rows linked through `tool_call_id`. The provider
loop has fixed server-owned limits:

- At most eight tool rounds and 180 seconds of tool-phase work per turn.
- The third consecutive identical tool call is rejected before execution.
- Four failed calls, two all-failed rounds, or a terminal auth/config/policy
  error stop tool use early.
- Once stopped, the provider gets one tool-free response with a 30-second
  timeout. If that fails, helpcore returns a concise inability-to-verify
  fallback instead of leaving the turn resumable.

The model cannot extend these budgets. Retrying a failed or externally
interrupted response starts a fresh attempt without instructions to continue
the previous strategy.

### Durable user interactions

`request_user_input` and `request_plugin_action` must be called alone in a tool
round. The server persists the tool call and an `interactions` row, changes the
assistant placeholder to `awaiting_input`, and ends the SSE stream with
`input_required` instead of `done`.

Only one interaction may be pending per conversation. New turns and retries
remain blocked until it is answered or dismissed. The web UI and `hc ask`
restore the pending interaction through
`GET /api/conversations/{id}/interaction`; responses are submitted to
`POST /api/conversations/{id}/interactions/{interaction_id}/respond`, which
streams the resumed assistant turn.

Question interactions support single select, multi-select, and text input.
Every selectable option carries required help text. Plugin configuration
values bypass conversation and interaction storage; secrets are written
directly to encrypted plugin storage.

---

## Context overflow handling

When the assembled context (system prompt + history + current message) would
exceed the provider's context window, helpcore uses **AI summarisation** rather
than silently dropping messages.

### Overflow detection

Token count estimated at ~4 chars/token plus ~4 tokens per message for role
framing. If estimated tokens > provider context limit × 0.9 (10% headroom),
overflow handling activates.

### Summarisation flow

```
1. Identify the oldest third of non-system messages in the conversation
2. Call the provider (same provider, same model) with:
     "Summarise the following conversation segment in under 200 words,
      preserving key decisions, facts, and context:"
     [oldest-third messages]
3. Receive summary text
4. Insert a `summary` role message into the DB at the correct sequence position
5. Mark the summarised messages as compacted (soft-delete, retained for export)
6. Re-estimate tokens with summary replacing the segment
7. If still over budget: compact oversized tool result pages while preserving continuation metadata
8. If still over budget: drop oldest tool-use/result pairs atomically
   (emergency fallback — log a warning, this should be rare)
```

The first user message (conversation anchor) is never dropped or summarised
away. Losing the original framing causes silent context drift.

Compacted messages are retained in the DB (flagged, not deleted) so the full
conversation history is always available for export or review, even after
summarisation.

### Oversized tool results

Tool results whose serialized model payload exceeds 10,000 bytes are retained
in a bounded, in-memory cache for the active assistant turn. The model receives
the first page with `total_lines`, exact line/column coordinates, a
`continuation_token`, and a hint to call `tool_result_read`. Each page is capped
at 200 lines and may continue within a single long line.

The continuation cache is intentionally turn-scoped: it prevents large plugin
responses from inflating persisted conversation context while still allowing
the model to retrieve every part needed for the current task.

Image, audio, and video data URLs use a separate path. Their full payload is
persisted and rendered in the tool result UI, while model context receives only
compact asset metadata. Token estimation and conversation compaction also use
that compact representation.

---

## Conversation title generation

On the first user message of a new conversation, the core generates a title
by truncating the message to ~60 characters at a word boundary. No extra LLM
call is made for title generation — keep it cheap.

Example: "what are some good ways to structure a rust workspace for a monorepo"
→ "what are some good ways to structure a rust workspace…"

User can rename at any time.

---

## Key invariants

- Tool-use/result pairs are always dropped atomically. A tool result without
  its call causes a provider 400 error.
- Orphaned tool messages are removed on every history load from DB.
- The first user message (anchor) is never dropped from context.
- Summary messages are system messages, always at position 0 of their segment.
