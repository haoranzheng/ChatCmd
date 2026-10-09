-- Locally approved task/project filesystem authority. Never created by MCP calls.
CREATE TABLE task_workspace_access (
    task_id TEXT PRIMARY KEY NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    project_id TEXT REFERENCES workspace_projects(id) ON DELETE SET NULL,
    access_mode TEXT NOT NULL CHECK(access_mode IN ('restricted','readOnly','readWrite')),
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX idx_task_workspace_access_project ON task_workspace_access(project_id);
UPDATE schema_version SET version = 26 WHERE singleton_id = 1;
