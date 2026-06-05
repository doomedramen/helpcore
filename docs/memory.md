# Memory and personality

helpcore maintains a set of markdown files per user that are injected into the AI context on every message. This gives the AI persistent knowledge about who you are, how you like to communicate, and anything else you want it to remember.

---

## The three personality files

These are special named files. Each shapes a different aspect of the AI's behaviour.

### Soul

**What it's for:** Tone, values, communication style. This is the most fundamental — it influences every response.

```bash
hc soul                          # print current content
hc soul --edit                   # open in $EDITOR
hc soul --set "Be direct and concise. Skip preambles. Use plain language."
```

Example soul:
```markdown
Be direct and concise. Skip pleasantries and filler phrases like "certainly!" or "great question!".
Use plain language. When you're uncertain, say so.
Prefer bullet points over prose for lists. Use code blocks for code.
```

### Identity

**What it's for:** Who the assistant *is* — its name, role, and its relationship to you. Lets you give it a persona.

```bash
hc identity
hc identity --edit
hc identity --set "You are Aria, my personal assistant. You help me stay organised."
```

Example identity:
```markdown
You are Aria, my personal AI assistant.
Your job is to help me stay organised, think through problems, and remember things.
You've worked with me for years and know my preferences well.
```

### Me (user profile)

**What it's for:** Facts about *you* that the AI should always know — background, preferences, context.

```bash
hc me
hc me --edit
hc me --set "Software engineer. Working on helpcore (Rust). macOS. Prefer Vim."
```

Example user profile:
```markdown
I'm a software engineer based in Melbourne, Australia.
Currently working on helpcore — a self-hosted AI assistant server written in Rust.
I use macOS, prefer the terminal, and write mostly Rust and Python.
My home server has 4 GB RAM.
```

---

## Memory files

Memory files are arbitrary markdown files you create under any path. They're loaded automatically based on relevance to your message (full-text search), plus the most recently updated files are always included.

### Creating and editing

```bash
# Write directly
hc memory set notes/work.md --set "Working on voice plugin. Whisper + KittenTTS."

# Open in $EDITOR (creates if it doesn't exist)
hc memory set projects/helpcore.md --edit

# Read from stdin
echo "# Server specs\n\n4 GB RAM, qwen2.5:3b" | hc memory set home/server.md
```

### Listing and reading

```bash
hc memory ls
# → notes/work.md         (1.2 KB)
# → projects/helpcore.md  (2.1 KB)
# → home/server.md        (0.3 KB)

hc memory get notes/work.md
```

### Deleting

```bash
hc memory rm notes/old-project.md
```

### Path conventions

Paths are relative, use forward slashes, and support subdirectories:

```
notes/work.md
notes/personal.md
projects/helpcore.md
home/devices.md
home/network.md
recipes/favourites.md
```

There's no enforced structure — organise however makes sense to you.

---

## How context injection works

On every chat message, helpcore assembles the AI context in this order:

1. **Core instructions** — base system prompt (not user-editable)
2. **Soul** — your tone/style file (if set)
3. **Identity** — assistant persona (if set)
4. **User profile** — your `me` file (if set)
5. **Memory files** — files relevant to the current message (FTS search + most recent)
6. **Plugin skills** — skill fragments from enabled plugins
7. **Current date**
8. **Conversation history**
9. **Your message**

The AI sees all of this as its context window. Memory files are selected automatically — you don't need to manually reference them.

---

## Tips

**Keep memory files focused.** One topic per file is better than one giant file. The search selects which files to include based on relevance — a single massive file always gets included in full.

**Update regularly.** When a project wraps up, update the file or delete it. Stale context can confuse the model.

**The soul file has the most influence.** If the AI's responses feel off, this is the first thing to tune.

**Memory files persist across conversations.** Unlike conversation history (which can be compacted), memory files are permanent until you delete them.

---

## Auto-compaction

When a conversation fills up the model's context window (above 90%), helpcore automatically summarises the oldest third of the messages and replaces them with a summary message. This happens transparently before the next model call.

You can also trigger compaction manually:

```bash
hc compact --conversation <id>
# → ✓ Compacted 18 messages into a 1024-character summary.
```

Memory files and personality files are **not** affected by compaction — they are always included fresh from disk.
