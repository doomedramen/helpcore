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

**Purpose:** security and administrative accountability. Records who did what
to the system, for intrusion detection and access review.

**Storage:** DB table (`audit_log`) — queryable, exposable via `GET /admin/audit`.

**Tamper evidence:** each row includes a hash of its content + previous row's
hash (chain). Breaks in the chain are detectable.

**Retention:** configurable; default 90 days.

### Events recorded

**Auth & sessions**

| Event | Fields recorded |
|---|---|
| Login success | user_id, ip, user_agent, timestamp |
| Login failure | email (attempted), ip, user_agent, timestamp |
| Logout | user_id, session_id, timestamp |
| Session revoked | user_id, session_id, revoked_by, timestamp |
| Password changed | user_id, timestamp |
| Force-password-change cleared | user_id, timestamp |

**API keys**

| Event | Fields recorded |
|---|---|
| API key created | user_id, key name (not the key), timestamp |
| API key revoked | user_id, key name, timestamp |

**Admin actions**

| Event | Fields recorded |
|---|---|
| User created | new_user_id, created_by (admin), timestamp |
| User deactivated / reactivated | user_id, by (admin), timestamp |
| User deleted | user_id, by (admin), timestamp |
| Password reset | user_id, reset_by (admin), timestamp |
| Provider access granted | user_id, provider_id, granted_by, timestamp |
| Provider access revoked | user_id, provider_id, revoked_by, timestamp |

**Plugins**

| Event | Fields recorded |
|---|---|
| Plugin installed | user_id, plugin_id, version, timestamp |
| Plugin uninstalled | user_id, plugin_id, timestamp |
| Permissions approved | user_id, plugin_id, permissions (list), timestamp |

**Security**

| Event | Fields recorded |
|---|---|
| Setup token generated | timestamp |
| Setup token used | ip, user_agent, timestamp |
| Invalid / expired token attempt | ip, user_agent, timestamp (no token value) |
| Plugin permission denied | user_id, plugin_id, operation, timestamp |

### What is never in the audit log

- Conversations — not even that one occurred
- Messages — not even metadata
- Provider calls — not even which model was used
- Tool executions — nothing
- Any AI interaction whatsoever

---

## Database schema

### `audit_log`

| Column | Type | Notes |
|---|---|---|
| id | UUID | Primary key |
| event_type | TEXT | e.g. `auth.login_success`, `admin.user_created` |
| actor_id | UUID | FK → users.id; null for unauthenticated events |
| target_id | UUID | FK → users.id; null when not user-targeted |
| payload | JSON | Event-specific fields (see tables above) |
| row_hash | TEXT | SHA-256 of this row's content + prev_hash |
| prev_hash | TEXT | Hash of preceding row (chain integrity) |
| created_at | TIMESTAMP | |

---

## Prometheus metrics (day 2)

A `GET /metrics` endpoint exposing standard Prometheus metrics. Not built on
day one — added once the core is stable.

Planned metrics:

| Metric | Type | Description |
|---|---|---|
| `helpcore_requests_total` | counter | HTTP requests by method, path, status |
| `helpcore_request_duration_seconds` | histogram | Request latency |
| `helpcore_provider_calls_total` | counter | Provider calls by provider_id, role, outcome |
| `helpcore_provider_duration_seconds` | histogram | Provider call latency by provider_id |
| `helpcore_active_sessions` | gauge | Currently active sessions |
| `helpcore_plugin_calls_total` | counter | Plugin tool dispatches by plugin_id, outcome |

No user data, conversation counts, or message volumes are exposed in metrics.

---

## Future: external log shipping

Application logs (stdout JSONL) integrate with any standard log shipper:

- **Graylog** — GELF input or Fluentd/Logstash pipeline
- **Loki** — Promtail scrapes Docker/systemd logs
- **ELK** — Filebeat or Logstash
- **Datadog** — Datadog Agent log collection

No helpcore-specific integration required — any shipper that reads
stdout/stderr JSON works.
