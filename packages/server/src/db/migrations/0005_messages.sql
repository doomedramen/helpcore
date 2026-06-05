CREATE TABLE IF NOT EXISTS messages (
    id              TEXT    NOT NULL PRIMARY KEY,
    conversation_id TEXT    NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    role            TEXT    NOT NULL,
    content         TEXT    NOT NULL DEFAULT '',
    tool_call_id    TEXT,
    tool_calls      TEXT,
    provider_id     TEXT,
    model           TEXT,
    sequence        INTEGER NOT NULL,
    compacted       INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS messages_conversation ON messages(conversation_id, sequence);
CREATE INDEX IF NOT EXISTS messages_active ON messages(conversation_id, sequence) WHERE compacted = 0;
