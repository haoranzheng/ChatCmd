//! Explicit, revocable local-UI consent for bounded in-workspace text edits.
//! Deliberately separate from general execution allow-all and safe-read grants.
use super::{Problem, db_problem, now_ms};
use crate::websocket::{AppEvent, AppState};
use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WriteTrustChange {
    duration_minutes: u32,
}

fn disabled(id: &str) -> Value {
    json!({"taskId":id,"active":false,"expiresAtMs":null,"usedCalls":0,"maxCalls":0})
}

pub(super) async fn task_write_trust(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tasks WHERE id=?")
        .bind(&id).fetch_one(state.repository.pool()).await.map_err(db_problem)?;
    if exists == 0 {
        return Err(Problem::new(StatusCode::NOT_FOUND, "Task not found", "Select a conversation task."));
    }
    let row = sqlx::query(
        "SELECT w.expires_at_ms,w.max_calls,w.used_calls FROM task_write_trust w
         JOIN tasks t ON t.id=w.task_id AND t.agent_id=w.agent_id AND t.conversation_scope_hash=w.conversation_scope
         JOIN task_workspace_access a ON a.task_id=w.task_id AND a.access_mode='readWrite'
             AND a.updated_at_ms=w.workspace_revision
         WHERE w.task_id=? AND w.root=t.project_folder
             AND w.expires_at_ms>? AND w.used_calls<w.max_calls",
    ).bind(&id).bind(now_ms()).fetch_optional(state.repository.pool()).await.map_err(db_problem)?;
    Ok(Json(match row {
        Some(row) => json!({"taskId":id,"active":true,
            "expiresAtMs":row.get::<i64,_>("expires_at_ms"),
            "maxCalls":row.get::<i64,_>("max_calls"),
            "usedCalls":row.get::<i64,_>("used_calls")}),
        None => disabled(&id),
    }))
}

pub(super) async fn set_task_write_trust(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<WriteTrustChange>,
) -> Result<Json<Value>, Problem> {
    if !matches!(input.duration_minutes, 15 | 60) {
        return Err(Problem::new(StatusCode::BAD_REQUEST,"Invalid write trust",
            "Only 15 or 60 minutes are supported."));
    }
    let row=sqlx::query(
        "SELECT t.agent_id,t.conversation_scope_hash,t.project_folder,
                a.access_mode,a.updated_at_ms
         FROM tasks t LEFT JOIN task_workspace_access a ON a.task_id=t.id
         WHERE t.id=?"
    ).bind(&id).fetch_optional(state.repository.pool()).await.map_err(db_problem)?
        .ok_or_else(||Problem::new(StatusCode::NOT_FOUND,"Task not found","Select a conversation task."))?;
    let mode=row.get::<Option<String>,_>("access_mode");
    let root=row.get::<Option<String>,_>("project_folder");
    let scope=row.get::<Option<String>,_>("conversation_scope_hash");
    let revision=row.get::<Option<i64>,_>("updated_at_ms");
    if mode.as_deref()!=Some("readWrite") || root.as_deref().is_none_or(str::is_empty)
        || scope.as_deref().is_none_or(str::is_empty) || revision.is_none() {
        return Err(Problem::new(StatusCode::CONFLICT,"Workspace write access required",
            "Bind this exact conversation to an existing local project in Read-write mode first."));
    }
    // No child task may receive or create a parent-like unattended grant.
    let child: i64=sqlx::query_scalar(
        "SELECT COUNT(*) FROM subagent_runs WHERE child_task_id=?"
    ).bind(&id).fetch_one(state.repository.pool()).await.map_err(db_problem)?;
    if child != 0 {
        return Err(Problem::new(StatusCode::FORBIDDEN,"Child task cannot be trusted",
            "Only the owner conversation may enable scoped write trust."));
    }
    let now=now_ms();
    let expires=now.saturating_add(i64::from(input.duration_minutes)*60_000);
    let mut tx=state.repository.pool().begin().await.map_err(db_problem)?;
    sqlx::query(
        "INSERT INTO task_write_trust(task_id,agent_id,conversation_scope,root,workspace_revision,
             expires_at_ms,max_calls,used_calls,updated_at_ms)
         VALUES(?,?,?,?,?,?,128,0,?)
         ON CONFLICT(task_id) DO UPDATE SET
           agent_id=excluded.agent_id,conversation_scope=excluded.conversation_scope,
           root=excluded.root,workspace_revision=excluded.workspace_revision,
           expires_at_ms=excluded.expires_at_ms,max_calls=128,used_calls=0,updated_at_ms=excluded.updated_at_ms"
    ).bind(&id).bind(row.get::<String,_>("agent_id"))
      .bind(scope.unwrap_or_default()).bind(root.unwrap_or_default())
      .bind(revision.unwrap_or_default()).bind(expires).bind(now)
      .execute(&mut *tx).await.map_err(db_problem)?;
    let event_id=Uuid::new_v4().to_string();
    let payload=json!({"source":"authenticatedLocalUi","change":"writeTrustGranted",
        "scope":"safeTextEdits","maxCalls":128,"expiresAtMs":expires});
    sqlx::query("INSERT INTO timeline_events(event_id,task_id,turn_id,session_id,actor,kind,idempotency_key,payload_json,metadata_json,created_at_ms)
        VALUES(?,?,NULL,NULL,'user','status',?,?,NULL,?)")
        .bind(&event_id).bind(&id).bind(&event_id).bind(payload.to_string()).bind(now)
        .execute(&mut *tx).await.map_err(db_problem)?;
    tx.commit().await.map_err(db_problem)?;
    let mut e=AppEvent::new("write.trust_changed",payload); e.task_id=Some(id.clone());state.publish(e);
    Ok(Json(json!({"taskId":id,"active":true,"expiresAtMs":expires,"maxCalls":128,"usedCalls":0})))
}

pub(super) async fn revoke_task_write_trust(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let exists: i64=sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE id=?")
        .bind(&id).fetch_one(state.repository.pool()).await.map_err(db_problem)?;
    if exists==0 { return Err(Problem::new(StatusCode::NOT_FOUND,"Task not found","Select a conversation task.")); }
    let now=now_ms();
    let mut tx=state.repository.pool().begin().await.map_err(db_problem)?;
    sqlx::query("DELETE FROM task_write_trust WHERE task_id=?").bind(&id)
        .execute(&mut *tx).await.map_err(db_problem)?;
    let event_id=Uuid::new_v4().to_string();
    let payload=json!({"source":"authenticatedLocalUi","change":"writeTrustRevoked"});
    sqlx::query("INSERT INTO timeline_events(event_id,task_id,turn_id,session_id,actor,kind,idempotency_key,payload_json,metadata_json,created_at_ms)
        VALUES(?,?,NULL,NULL,'user','status',?,?,NULL,?)")
        .bind(&event_id).bind(&id).bind(&event_id).bind(payload.to_string()).bind(now)
        .execute(&mut *tx).await.map_err(db_problem)?;
    tx.commit().await.map_err(db_problem)?;
    let mut e=AppEvent::new("write.trust_changed",payload);e.task_id=Some(id.clone());state.publish(e);
    Ok(Json(disabled(&id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::chatgpt_router_tests::{extension_request, fixture};
    #[tokio::test]
    async fn extension_cannot_manage_write_trust() {
        let (_state, app, _dir)=fixture("running").await;
        for (method,body) in [
            ("GET",json!({})),("PUT",json!({"durationMinutes":60})),("DELETE",json!({}))
        ] {
            let response=extension_request(&app,method,"/api/local/tasks/task-a/write-trust",body).await;
            assert_eq!(response.status(),StatusCode::FORBIDDEN);
        }
    }
    #[tokio::test]
    async fn unbound_or_read_only_task_cannot_enable_write_trust() {
        let (state,_app,_dir)=fixture("running").await;
        let error=set_task_write_trust(State(state),Path("task-a".into()),
            Json(WriteTrustChange{duration_minutes:60})).await.unwrap_err();
        assert_eq!(error.status,StatusCode::CONFLICT);
    }
}
