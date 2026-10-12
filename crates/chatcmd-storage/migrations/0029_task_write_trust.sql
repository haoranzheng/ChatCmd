-- Local authenticated, task-bound file-edit trust. No API/MCP client can
-- grant it; any workspace or task policy change deletes this row.
CREATE TABLE task_write_trust (
    task_id TEXT PRIMARY KEY NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    conversation_scope TEXT NOT NULL,
    root TEXT NOT NULL,
    workspace_revision INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    max_calls INTEGER NOT NULL CHECK(max_calls BETWEEN 1 AND 256),
    used_calls INTEGER NOT NULL DEFAULT 0 CHECK(used_calls >= 0),
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_task_write_trust_expiry ON task_write_trust(expires_at_ms);
UPDATE schema_version SET version=29 WHERE singleton_id=1;
UPDATE app_metadata SET value='29' WHERE key='schema_version';
