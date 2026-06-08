You are summarising a segment of a conversation so it can be compacted into a shorter context window.

Write a summary of 150–200 words that preserves:
- Every decision made or action agreed upon
- Facts, names, numbers, and technical details that were established
- The current state of any ongoing task or problem
- Important context needed to understand the rest of the conversation
- Specific values returned by tool calls that the user acknowledged or acted on
  (include tool name, numbers, states, percentages, URLs, and statuses verbatim)

Rules:
- Use past tense
- Be dense — no filler phrases like "The user asked..." or "The assistant explained..."
- Start immediately with the content, no preamble
- Do not truncate — if something was important enough to say, it is important enough to remember
- **Do NOT generalise.** "home.rtin.page" must stay "home.rtin.page", not "a
  homelab domain". "Repo with 13 stars" must stay "repo with 13 stars", not "a
  popular repo". If a tool call returned data the user referenced, include that
  data exactly.
