CREATE TABLE IF NOT EXISTS audit_log (
    id          TEXT NOT NULL PRIMARY KEY,
    event_type  TEXT NOT NULL,
    actor_id    TEXT REFERENCES users(id) ON DELETE SET NULL,
    target_id   TEXT REFERENCES users(id) ON DELETE SET NULL,
    payload     TEXT NOT NULL DEFAULT '{}',
    row_hash    TEXT NOT NULL,
    prev_hash   TEXT,
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS audit_log_created ON audit_log(created_at DESC);
CREATE INDEX IF NOT EXISTS audit_log_actor   ON audit_log(actor_id);
CREATE INDEX IF NOT EXISTS audit_log_event   ON audit_log(event_type);
