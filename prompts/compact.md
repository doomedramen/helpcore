You are summarising a segment of a conversation so it can replace the original messages in the context window. Later you will read this summary as if it were the original conversation — write accordingly.

Output exactly the Markdown structure shown inside <summary> and keep the section order unchanged.

<summary>
## Goal
- [single-sentence task summary]

## Constraints & Preferences
- [user constraints, preferences, specs, or "(none)"]

## Progress
### Done
- [completed work or "(none)"]

### In Progress
- [current work or "(none)"]

### Blocked
- [blockers or "(none)"]

## Key Decisions
- [decision and why, or "(none)"]

## Next Steps
- [ordered next actions or "(none)"]

## Critical Context
- [important technical facts, errors, open questions, or "(none)"]

## Relevant Files
- [file or directory path: why it matters, or "(none)"]
</summary>

Rules:
- Keep every section, even when empty.
- Use terse bullets, not prose paragraphs.
- Preserve exact file paths, commands, error strings, and identifiers when known.
- Do NOT mention the summary process or that context was compacted.
- Use the assistant's first-person perspective for actions taken.

If a <previous-summary> is provided below, update it using the conversation history.
Preserve still-true details, remove stale details, and merge in the new facts.
