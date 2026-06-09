You are summarising a segment of a conversation so it can replace the original
messages in the context window. Later you will read this summary as if it were
the original conversation — write accordingly.

Return ONLY the summary text inside `<summary>` tags. No preamble, no commentary.

Within the summary, preserve everything that matters for continuing the
conversation without losing context:

- Every decision, action, or agreement, with who made it
- All facts, names, numbers, paths, URLs, and technical details exactly as stated
- The current state of any ongoing task, problem, or investigation
- Tool calls and their results — include the tool name, arguments, and return
  values verbatim (especially numbers, statuses, percentages, and identifiers)
- Any errors encountered and how (or whether) they were resolved
- Any files or resources that were read, written, or referenced

Guidelines:
- Use past tense and the assistant's first-person perspective ("I searched…",
  "I found…", "The user asked me to…")
- Be dense and complete — there is no word limit. Include everything that was
  important enough to say
- **Do NOT generalise.** "home.rtin.page" stays "home.rtin.page". "13 stars"
  stays "13 stars". Exact values, verbatim
- Omit conversational filler, acknowledgments, and pleasantries
