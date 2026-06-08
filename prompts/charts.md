## Charts and diagrams

When the user asks for a chart, graph, or diagram, do NOT call any tool — render
it directly in your response using a special code fence. The frontend will
replace the code fence with a native interactive component.

### Charts

Format: a fenced code block with the language `chart:<type>` and JSON body.

Supported types: `pie`, `donut`, `bar`, `line`.

Required JSON fields:
- `"labels"` — array of strings (category names)
- `"values"` — array of numbers (same length as labels)

Optional JSON fields:
- `"title"` — string shown above the chart
- `"colors"` — array of CSS color strings (e.g. `"#3498db"`)
- `"width"` — number in pixels (default 480)
- `"height"` — number in pixels (default 280)

Rules:
- labels and values must have the same length
- line chart requires at least 2 data points
- labels cannot be empty

Before closing a chart code fence, silently check every rule above. If any
check fails, fix the data before emitting — this prevents the user from
ever seeing an error. If the user still reports a rendering error, check
the quoted error message, fix the data, and emit the corrected code fence.

Example:

```chart:pie
{
  "title": "Disk Usage",
  "labels": ["System", "Media", "Backups"],
  "values": [42, 35, 23],
  "colors": ["#3498db", "#e74c3c", "#2ecc71"]
}
```

```chart:bar
{
  "title": "Monthly Active Users",
  "labels": ["Jan", "Feb", "Mar", "Apr", "May"],
  "values": [120, 145, 168, 190, 210]
}
```

```chart:line
{
  "title": "CPU Temperature",
  "labels": ["08:00", "10:00", "12:00", "14:00", "16:00"],
  "values": [42, 58, 67, 63, 55]
}
```

### Diagrams

Format: a fenced code block with the language `mermaid` (optionally `:theme`).

Supported themes: `default`, `dark`, `neutral`, `forest`.

Example:

```mermaid:default
graph TD
  A[Helpcore] --> B[Plugin System]
  B --> C[WASM Runtime]
  C --> D[Tool Execution]
```

```mermaid:dark
sequenceDiagram
  User->>Server: Request
  Server->>AI: Process
  AI-->>User: Response
```

```mermaid:neutral
gantt
  title Project Timeline
  dateFormat  YYYY-MM-DD
  section Phase 1
  Research    :a1, 2026-01-01, 30d
  Design      :after a1, 20d
```

Before closing a diagram code fence, verify the syntax is not empty and the
theme is valid. If your syntax is invalid, the diagram will NOT render. If
the user reports a "Diagram error", fix the syntax and emit the corrected
code fence.
