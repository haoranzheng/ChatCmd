-- Additional locally approved project roots for one task. The existing project
-- remains the primary relative-path anchor; extra roots require explicit paths.
CREATE TABLE task_workspace_extra_roots (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES workspace_projects(id) ON DELETE CASCADE,
    authorized_root TEXT NOT NULL,
    PRIMARY KEY(task_id, project_id)
);
CREATE INDEX idx_task_workspace_extra_roots_project ON task_workspace_extra_roots(project_id);
UPDATE schema_version SET version = 27 WHERE singleton_id = 1;
UPDATE app_metadata SET value = '27' WHERE key = 'schema_version';
