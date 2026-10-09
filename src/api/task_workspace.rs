//! Authenticated, local-UI-only task workspace binding.
//! An MCP caller cannot mint or broaden this authority.
use std::path::{Path as FsPath, PathBuf};

use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;
use std::sync::Arc;

use crate::websocket::{AppEvent, AppState};
use super::{Problem, db_problem, now_ms};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TaskWorkspaceChange {
    project_id: String,
    access_mode: String,
}

pub(super) async fn task_workspace(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let task = sqlx::query("SELECT project_folder FROM tasks WHERE id=?")
        .bind(&id).fetch_optional(state.repository.pool()).await.map_err(db_problem)?
        .ok_or_else(missing_task)?;
    let binding = sqlx::query(
        "SELECT project_id,access_mode FROM task_workspace_access WHERE task_id=?"
    ).bind(&id).fetch_optional(state.repository.pool()).await.map_err(db_problem)?;
    Ok(Json(json!({
        "taskId": id,
        "projectFolder": task.get::<Option<String>, _>("project_folder"),
        "projectId": binding.as_ref().map(|row| row.get::<String, _>("project_id")),
        "accessMode": binding.as_ref().map_or("readOnly", |row| row.get::<&str, _>("access_mode")),
        "locallyAuthorized": binding.is_some()
    })))
}

pub(super) async fn set_task_workspace(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(change): Json<TaskWorkspaceChange>,
) -> Result<Json<Value>, Problem> {
    if !matches!(change.access_mode.as_str(), "restricted" | "readOnly" | "readWrite") {
        return Err(Problem::new(StatusCode::BAD_REQUEST, "Invalid workspace access mode",
            "Choose restricted, readOnly, or readWrite."));
    }
    let row = sqlx::query("SELECT path FROM workspace_projects WHERE id=?")
        .bind(&change.project_id).fetch_optional(state.repository.pool()).await.map_err(db_problem)?
        .ok_or_else(|| Problem::new(StatusCode::NOT_FOUND, "Workspace project not found",
            "Create or select a saved workspace project in the local management UI."))?;
    let canonical = validate_workspace_root(row.get::<&str, _>("path"))?;
    let folder = canonical.to_string_lossy().into_owned();
    let now = now_ms();
    let audit_id = Uuid::new_v4().to_string();
    let mut tx = state.repository.pool().begin().await.map_err(db_problem)?;
    // This endpoint is not exposed to MCP/extension callers. Never resolve project IDs from
    // tool arguments or a message's absolute-path text.
    let updated = sqlx::query("UPDATE tasks SET project_folder=?,updated_at_ms=? WHERE id=?")
        .bind(&folder).bind(now).bind(&id)
        .execute(&mut *tx).await.map_err(db_problem)?.rows_affected();
    if updated != 1 { return Err(missing_task()); }
    sqlx::query("INSERT INTO task_workspace_access(task_id,project_id,access_mode,updated_at_ms) VALUES(?,?,?,?) ON CONFLICT(task_id) DO UPDATE SET project_id=excluded.project_id,access_mode=excluded.access_mode,updated_at_ms=excluded.updated_at_ms")
        .bind(&id).bind(&change.project_id).bind(&change.access_mode).bind(now)
        .execute(&mut *tx).await.map_err(db_problem)?;
    // Binding changes invalidate earlier approvals and inherited read grants.
    sqlx::query("WITH RECURSIVE tree(id) AS (SELECT ? UNION SELECT child_task_id FROM subagent_runs JOIN tree ON parent_task_id=tree.id WHERE child_task_id IS NOT NULL) UPDATE approvals SET state='cancelled',decision_json=json_object('reason','workspace binding changed'),resolved_at_ms=? WHERE task_id IN (SELECT id FROM tree) AND state='pending'")
        .bind(&id).bind(now).execute(&mut *tx).await.map_err(db_problem)?;
    sqlx::query("WITH RECURSIVE tree(id) AS (SELECT ? UNION SELECT child_task_id FROM subagent_runs JOIN tree ON parent_task_id=tree.id WHERE child_task_id IS NOT NULL) UPDATE approval_grants SET state='revoked',updated_at_ms=? WHERE task_id IN (SELECT id FROM tree) AND state='active'")
        .bind(&id).bind(now).execute(&mut *tx).await.map_err(db_problem)?;
    // Always retain per-operation approval, even if the previous task policy was allowAll.
    sqlx::query("INSERT INTO task_execution_modes(task_id,mode,updated_at_ms) VALUES(?,'approval',?) ON CONFLICT(task_id) DO UPDATE SET mode='approval',updated_at_ms=excluded.updated_at_ms")
        .bind(&id).bind(now).execute(&mut *tx).await.map_err(db_problem)?;
    let payload = json!({"source":"authenticatedLocalUi","change":"taskWorkspaceAccess",
        "projectId":change.project_id,"accessMode":change.access_mode,"auditId":audit_id});
    sqlx::query("INSERT INTO timeline_events(event_id,task_id,turn_id,session_id,actor,kind,idempotency_key,payload_json,metadata_json,created_at_ms) VALUES(?,?,NULL,NULL,'user','status',?,?,NULL,?)")
        .bind(&audit_id).bind(&id).bind(&audit_id).bind(payload.to_string()).bind(now)
        .execute(&mut *tx).await.map_err(db_problem)?;
    tx.commit().await.map_err(db_problem)?;
    let mut event = AppEvent::new("workspace.access_changed", payload);
    event.task_id = Some(id.clone());
    state.publish(event);
    Ok(Json(json!({"taskId":id,"projectId":change.project_id,"projectFolder":folder,
        "accessMode":change.access_mode,"locallyAuthorized":true})))
}

fn missing_task() -> Problem {
    Problem::new(StatusCode::NOT_FOUND, "Task not found", "The task no longer exists.")
}

pub(super) fn validate_workspace_root(value: &str) -> Result<PathBuf, Problem> {
    let requested = FsPath::new(value);
    if !requested.is_absolute() {
        return Err(Problem::new(StatusCode::BAD_REQUEST, "Invalid workspace path",
            "The project path must be an existing absolute directory."));
    }
    // Reject junctions/reparse components, including an intermediate parent.
    let mut component_path = PathBuf::new();
    for part in requested.components() {
        component_path.push(part.as_os_str());
        let meta = std::fs::symlink_metadata(&component_path).map_err(|_| {
            Problem::new(StatusCode::BAD_REQUEST, "Workspace path unavailable",
                "The selected project directory does not exist or is inaccessible.")
        })?;
        if meta.file_type().is_symlink() || is_windows_reparse_point(&meta) {
            return Err(Problem::new(StatusCode::BAD_REQUEST, "Workspace reparse point rejected",
                "Select a directory without symlinks, junctions or reparse-point components."));
        }
    }
    let canonical = std::fs::canonicalize(requested).map_err(|_| {
        Problem::new(StatusCode::BAD_REQUEST, "Workspace path unavailable",
            "The selected directory could not be resolved.")
    })?;
    if !canonical.is_dir() || canonical.parent().is_none() {
        return Err(Problem::new(StatusCode::BAD_REQUEST, "Invalid workspace root",
            "Select an existing project directory, not a filesystem root."));
    }
    Ok(canonical)
}

#[cfg(windows)]
fn is_windows_reparse_point(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_windows_reparse_point(_: &std::fs::Metadata) -> bool { false }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_existing_folder_but_rejects_missing_and_relative() {
        let temp = tempfile::tempdir().expect("temp workspace");
        assert!(validate_workspace_root(temp.path().to_str().expect("utf8")).is_ok());
        assert!(validate_workspace_root("not-a-workspace").is_err());
        assert!(validate_workspace_root(temp.path().join("missing").to_str().expect("utf8")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_project_root() {
        let temp = tempfile::tempdir().expect("workspace");
        let link = temp.path().join("linked");
        std::os::unix::fs::symlink(temp.path(), &link).expect("symlink");
        assert!(validate_workspace_root(link.to_str().expect("utf8")).is_err());
    }
}
