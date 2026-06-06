# helpcore — Observability

Privacy is a first-class concern. No conversation content, no message text,
no AI interaction metadata, and no user data ever appears in any log output.
Logs answer: *what is the server doing* and *who did what administratively* —
never *what did the user say or do with the AI*.

---

## Application logs

**Purpose:** admin-facing operational visibility — warnings, errors, startup
events, provider failures. Enough to diagnose problems and retrieve the
first-run setup token.

**Format:** structured JSON, one object per line (JSONL).

**Destination:** stdout only. Docker and systemd capture it automatically.
Pipe to any log aggregator (Graylog, Loki, ELK, Datadog) via standard JSON
log shippers — no helpcore-specific integration needed.

**Log levels:** `error`, `warn`, `info`, `debug` (configurable; default `info`).

### What is logged

| Event | Level | Notes |
|---|---|---|
| Server start / shutdown | info | Version, config path, data dir |
| Setup token generated | info | Token printed to stdout; not stored in logs |
| DB migration applied | info | Migration version only |
| Provider loaded / failed to load | info / error | Provider name, no credentials |
| Plugin WASM loaded / failed | info / error | Plugin ID only |
| Request errors (5xx) | error | Path, status — no request body or headers |
| Provider call failed | warn | Provider name, error type — no prompt content |
| Config reloaded (future) | info | — |

### What is never logged

- Conversation content or any AI response
- Message text or tool call arguments
- User data of any kind
- API keys, tokens, or credentials (not even truncated)
- Request bodies
- Provider prompts or completions
- Any personally identifiable information

---

## Audit log

An audit log is not yet implemented. Event auditing (login attempts, admin
actions, plugin operations) is a planned future addition.

---

## Future: external log shipping

Application logs (stdout JSONL) integrate with any standard log shipper:

- **Graylog** — GELF input or Fluentd/Logstash pipeline
- **Loki** — Promtail scrapes Docker/systemd logs
- **ELK** — Filebeat or Logstash
- **Datadog** — Datadog Agent log collection

No helpcore-specific integration required — any shipper that reads
stdout/stderr JSON works.
