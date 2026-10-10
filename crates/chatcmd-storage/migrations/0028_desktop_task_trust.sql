-- Explicit local-UI-only desktop trust. It applies to exactly one task
-- and the agent owning that task; never inherited by subagents.
CREATE TABLE desktop_task_trust (
    task_id TEXT PRIMARY KEY NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    scope TEXT NOT NULL CHECK(scope IN ('observe','control')),
    port INTEGER NOT NULL CHECK(port BETWEEN 1 AND 65535),
    expires_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_desktop_task_trust_expiry ON desktop_task_trust(expires_at_ms);
UPDATE schema_version SET version = 28 WHERE singleton_id = 1;
UPDATE app_metadata SET value = '28' WHERE key = 'schema_version';
