CREATE TABLE IF NOT EXISTS interactions (
    id                   TEXT NOT NULL PRIMARY KEY,
    conversation_id      TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    user_id              TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    assistant_message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    tool_call_id         TEXT NOT NULL,
    tool_name            TEXT NOT NULL,
    payload              TEXT NOT NULL,
    status               TEXT NOT NULL DEFAULT 'pending',
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS interactions_one_pending_per_conversation
ON interactions(conversation_id)
WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS interactions_user_status
ON interactions(user_id, status);
