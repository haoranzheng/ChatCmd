//! Integration tests use ephemeral SQLite and filesystem roots only.
use axum::{
    Json,
    extract::{Path as AxumPath, State},
};
use chatcmd_runtime::OperationContext;
use serde_json::json;
use std::{path::Path, sync::Arc};

use super::{TaskWorkspaceChange, set_task_workspace, task_workspace};
use crate::{
    runtime_host::{RuntimeHost, user_message_tests::test_host},
    websocket::AppState,
};

async fn fixture(
    project: &Path,
) -> (
    RuntimeHost,
    Arc<AppState>,
    String,
    String,
    tempfile::TempDir,
) {
    let (host, agent, temp) = test_host().await;
    let state = Arc::new(host.test_app_state(temp.path().join("chatcmd.db").display().to_string()));
    let task = "workspace-authorized-test-task".to_owned();
    let project_id = "workspace-authorized-test-project".to_owned();
    let canonical = project
        .canonicalize()
        .expect("canonical temporary project")
        .display()
        .to_string();
    sqlx::query("INSERT INTO workspace_projects(id,name,path,canonical_path,created_at_ms,updated_at_ms) VALUES(?,?,?, ?,0,0)")
        .bind(&project_id).bind("Ephemeral project").bind(&canonical).bind(&canonical)
        .execute(state.repository.pool()).await.expect("insert project");
    sqlx::query("INSERT INTO tasks(id,agent_id,device_id,title,source,status,generation,created_at_ms,updated_at_ms) VALUES(?,?,?,'Workspace test','mcp','running',1,0,0)")
        .bind(&task).bind(&agent).bind(state.device.id.as_str())
        .execute(state.repository.pool()).await.expect("insert task");
    (host, state, task, project_id, temp)
}

async fn bind(state: Arc<AppState>, task: &str, project: &str, mode: &str) {
    let _ = set_task_workspace(
        State(state),
        AxumPath(task.to_owned()),
        Json(TaskWorkspaceChange {
            project_id: project.to_owned(),
            access_mode: mode.to_owned(),
        }),
    )
    .await
    .expect("bind through local UI handler");
}

fn context(task: &str, agent: &str, tool: &str, id: &str) -> OperationContext {
    let mut result = OperationContext::new(id, agent, tool);
    result.task_id = Some(task.to_owned());
    result.turn_id = Some("workspace-test-turn".to_owned());
    result
}

#[tokio::test]
async fn locally_bound_project_resolves_roots_and_survives_database_reconnect() {
    let project = tempfile::tempdir().expect("project");
    let (host, state, task, project_id, _db) = fixture(project.path()).await;
    let before = task_workspace(State(state.clone()), AxumPath(task.clone()))
        .await
        .expect("lookup");
    assert_eq!(before.0["projectFolder"], json!(null));
    assert_eq!(before.0["locallyAuthorized"], false);
    bind(state.clone(), &task, &project_id, "readOnly").await;
    let stored = task_workspace(State(state.clone()), AxumPath(task.clone()))
        .await
        .expect("bound lookup");
    assert_eq!(stored.0["projectId"], project_id);
    assert_eq!(stored.0["locallyAuthorized"], true);
    let roots = host
        .dispatch(
            "workspace_roots",
            context(
                &task,
                "workspace-test-agent",
                "workspace_roots",
                "roots-check",
            ),
            json!({}),
        )
        .await
        .expect("roots");
    assert_eq!(roots, json!([project.path().canonicalize().expect("root")]));
    let db_path = _db.path().join("chatcmd.db");
    let connection = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rw", db_path.display()))
        .await
        .expect("reconnect database");
    let persisted: String =
        sqlx::query_scalar("SELECT access_mode FROM task_workspace_access WHERE task_id=?")
            .bind(&task)
            .fetch_one(&connection)
            .await
            .expect("persisted scope");
    assert_eq!(persisted, "readOnly");
    connection.close().await;
}

#[tokio::test]
async fn authorized_write_is_scoped_and_revocation_blocks_subsequent_writes() {
    let project = tempfile::tempdir().expect("project A");
    let outside = tempfile::tempdir().expect("project B");
    let file = project.path().join("allowed.txt");
    std::fs::write(&file, "before").expect("test file");
    let external = outside.path().join("outside.txt");
    std::fs::write(&external, "untouched").expect("outside file");
    let (host, state, task, project_id, _db) = fixture(project.path()).await;
    let agent: String = sqlx::query_scalar("SELECT agent_id FROM tasks WHERE id=?")
        .bind(&task)
        .fetch_one(state.repository.pool())
        .await
        .expect("agent");
    bind(state.clone(), &task, &project_id, "readWrite").await;
    let denied_read = host
        .require_workspace_access(
            &context(&task, &agent, "fs_read_text", "read-outside"),
            "fs_read_text",
            &json!({"path":external}),
        )
        .await
        .expect_err("read outside bound workspace");
    assert_eq!(denied_read.code, "path_outside_allowed_scope");
    let approved_mode: String =
        sqlx::query_scalar("SELECT mode FROM task_execution_modes WHERE task_id=?")
            .bind(&task)
            .fetch_one(state.repository.pool())
            .await
            .expect("approval policy");
    assert_eq!(approved_mode, "approval");
    host.dispatch(
        "fs_replace_text",
        context(&task, &agent, "fs_replace_text", "in-scope"),
        json!({"path":file,"oldText":"before","newText":"after"}),
    )
    .await
    .expect("write in A");
    assert_eq!(std::fs::read_to_string(&file).expect("read A"), "after");
    let denied = host
        .dispatch(
            "fs_replace_text",
            context(&task, &agent, "fs_replace_text", "outside-scope"),
            json!({"path":external,"oldText":"untouched","newText":"illegal"}),
        )
        .await
        .expect_err("reject B");
    assert_eq!(denied.code, "path_outside_allowed_scope");
    assert_eq!(
        std::fs::read_to_string(&external).expect("read B"),
        "untouched"
    );
    bind(state.clone(), &task, &project_id, "restricted").await;
    let denied = host
        .dispatch(
            "fs_replace_text",
            context(&task, &agent, "fs_replace_text", "after-revoke"),
            json!({"path":file,"oldText":"after","newText":"illegal"}),
        )
        .await
        .expect_err("revoke");
    assert_eq!(denied.code, "policy_denied");
    assert_eq!(
        std::fs::read_to_string(&file).expect("read after revoke"),
        "after"
    );
}

#[tokio::test]
async fn read_only_cannot_write_and_rebinding_invalidates_old_scope() {
    let first = tempfile::tempdir().expect("first project");
    let second = tempfile::tempdir().expect("second project");
    let (host, state, task, project_id, _db) = fixture(first.path()).await;
    let agent: String = sqlx::query_scalar("SELECT agent_id FROM tasks WHERE id=?")
        .bind(&task)
        .fetch_one(state.repository.pool())
        .await
        .expect("agent");
    bind(state.clone(), &task, &project_id, "readOnly").await;
    let first_file = first.path().join("first.txt");
    std::fs::write(&first_file, "one").expect("first file");
    let err = host
        .require_workspace_access(
            &context(&task, &agent, "fs_replace_text", "read-only"),
            "fs_replace_text",
            &json!({"path":first_file}),
        )
        .await
        .expect_err("read-only");
    assert_eq!(err.code, "permission_change_requires_user");
    let second_id = "workspace-second";
    let canonical = second
        .path()
        .canonicalize()
        .expect("second path")
        .display()
        .to_string();
    sqlx::query("INSERT INTO workspace_projects(id,name,path,canonical_path,created_at_ms,updated_at_ms) VALUES(?,?,?, ?,0,0)")
        .bind(second_id).bind("Second").bind(&canonical).bind(&canonical)
        .execute(state.repository.pool()).await.expect("second project");
    bind(state.clone(), &task, second_id, "readWrite").await;
    let err = host
        .require_workspace_access(
            &context(&task, &agent, "fs_replace_text", "after-rebind"),
            "fs_replace_text",
            &json!({"path":first_file}),
        )
        .await
        .expect_err("old scope revoked");
    assert_eq!(err.code, "path_outside_allowed_scope");
}

#[cfg(windows)]
#[test]
fn windows_junction_is_never_a_workspace_root() {
    let temp = tempfile::tempdir().expect("temp");
    let real = temp.path().join("real");
    let junction = temp.path().join("junction");
    std::fs::create_dir(&real).expect("real");
    let output = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(&junction)
        .arg(&real)
        .output()
        .expect("Windows junction creation");
    assert!(
        output.status.success(),
        "junction creation not available: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(super::validate_workspace_root(junction.to_str().expect("unicode path")).is_err());
}

#[tokio::test]
async fn mcp_extension_cannot_bind_or_grant_a_workspace() {
    use crate::api::chatgpt_router_tests::{extension_request, fixture};
    use axum::http::StatusCode;
    let (_state, app, _temp) = fixture("completed").await;
    let result = extension_request(
        &app,
        "PUT",
        "/api/local/tasks/task-a/workspace",
        json!({"projectId":"project-a","accessMode":"readWrite"}),
    )
    .await;
    assert_eq!(result.status(), StatusCode::FORBIDDEN);
    let result =
        extension_request(&app, "GET", "/api/local/tasks/task-a/workspace", json!({})).await;
    assert_eq!(result.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn management_session_is_required_for_workspace_mutation() {
    use crate::api::chatgpt_router_tests::fixture;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let (_state, app, _temp) = fixture("completed").await;
    let request = Request::builder()
        .method("PUT")
        .uri("/api/local/tasks/task-a/workspace")
        .header("X-ChatCmdClient", "local-ui")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"projectId":"none","accessMode":"readWrite"}).to_string(),
        ))
        .expect("local request");
    let response = app.oneshot(request).await.expect("route response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn removing_a_saved_project_keeps_a_restrictive_tombstone() {
    let project = tempfile::tempdir().expect("temporary project");
    let (host, state, task, project_id, _db) = fixture(project.path()).await;
    let agent: String = sqlx::query_scalar("SELECT agent_id FROM tasks WHERE id=?")
        .bind(&task)
        .fetch_one(state.repository.pool())
        .await
        .expect("agent");
    bind(state.clone(), &task, &project_id, "readWrite").await;

    // An unrelated project's pending approval must not be cancelled merely
    // because a different project was deleted (previous null-project query
    // revoked every existing tombstone).
    let other = tempfile::tempdir().expect("unrelated project");
    let unrelated_id = "unrelated-workspace";
    let unrelated_task = "unrelated-task";
    let other_path = other.path().canonicalize().expect("unrelated root");
    sqlx::query("INSERT INTO workspace_projects(id,name,path,canonical_path,created_at_ms,updated_at_ms) VALUES(?,?,?, ?,0,0)")
        .bind(unrelated_id).bind("Unrelated").bind(other_path.to_string_lossy().as_ref())
        .bind(other_path.to_string_lossy().as_ref())
        .execute(state.repository.pool()).await.expect("unrelated project");
    sqlx::query("INSERT INTO tasks(id,agent_id,device_id,title,source,status,generation,created_at_ms,updated_at_ms) VALUES(?,?,?,'Unrelated task','mcp','running',1,0,0)")
        .bind(unrelated_task).bind(&agent).bind(state.device.id.as_str())
        .execute(state.repository.pool()).await.expect("unrelated task");
    bind(state.clone(), unrelated_task, unrelated_id, "readWrite").await;
    sqlx::query("INSERT INTO approvals(id,task_id,session_id,state,request_json,decision_json,created_at_ms,resolved_at_ms) VALUES('unrelated-approval',?,NULL,'pending','{}',NULL,0,NULL)")
        .bind(unrelated_task).execute(state.repository.pool()).await.expect("pending unrelated approval");

    crate::api::workspaces::delete_workspace_project(State(state.clone()), AxumPath(project_id))
        .await
        .expect("delete saved project");
    let row = task_workspace(State(state.clone()), AxumPath(task.clone()))
        .await
        .expect("tombstone");
    assert_eq!(row.0["accessMode"], "restricted");
    assert!(row.0["projectId"].is_null());
    let pending: String =
        sqlx::query_scalar("SELECT state FROM approvals WHERE id='unrelated-approval'")
            .fetch_one(state.repository.pool())
            .await
            .expect("unrelated approval retained");
    assert_eq!(pending, "pending");
    let error = host
        .require_workspace_access(
            &context(&task, &agent, "fs_read_text", "tombstone-read"),
            "fs_read_text",
            &json!({"path":project.path().join("any.txt")}),
        )
        .await
        .expect_err("project deletion revokes reads");
    assert_eq!(error.code, "policy_denied");
}
