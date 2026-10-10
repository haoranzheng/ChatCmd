//! A local authenticated administrator may trust desktop tools for one task.
//! No MCP or extension call may create or broaden this authority.
use super::{Problem, db_problem, now_ms};
use crate::websocket::{AppEvent, AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row as _;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DesktopTrustChange {
    scope: String,
    duration_minutes: u32,
}

async fn task_agent(state: &AppState, id: &str) -> Result<String, Problem> {
    let row = sqlx::query("SELECT agent_id FROM tasks WHERE id=?")
        .bind(id)
        .fetch_optional(state.repository.pool())
        .await
        .map_err(db_problem)?
        .ok_or_else(|| {
            Problem::new(
                StatusCode::NOT_FOUND,
                "Task not found",
                "Select a real conversation task.",
            )
        })?;
    Ok(row.get("agent_id"))
}

pub(super) async fn task_desktop_trust(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    task_agent(&state, &id).await?;
    let now = now_ms();
    let row = sqlx::query("SELECT scope,port,expires_at_ms FROM desktop_task_trust WHERE task_id=? AND expires_at_ms>?")
        .bind(&id).bind(now).fetch_optional(state.repository.pool()).await.map_err(db_problem)?;
    Ok(Json(match row {
        Some(row) => json!({"taskId":id,"active":true,
            "scope":row.get::<&str,_>("scope"),
            "port":row.get::<i64,_>("port"),
            "expiresAtMs":row.get::<i64,_>("expires_at_ms")}),
        None => json!({"taskId":id,"active":false,"scope":"none","expiresAtMs":null}),
    }))
}

pub(super) async fn set_task_desktop_trust(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(change): Json<DesktopTrustChange>,
) -> Result<Json<Value>, Problem> {
    if !matches!(change.scope.as_str(), "observe" | "control")
        || !matches!(change.duration_minutes, 15 | 60 | 480)
    {
        return Err(Problem::new(
            StatusCode::BAD_REQUEST,
            "Invalid desktop trust",
            "Choose observe/control and a 15, 60 or 480 minute duration.",
        ));
    }
    let agent_id = task_agent(&state, &id).await?;
    let (enabled, port) = crate::desktop_bridge::config(state.repository.pool())
        .await
        .map_err(|_| {
            Problem::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Desktop settings unavailable",
                "Check local desktop settings.",
            )
        })?;
    if !enabled {
        return Err(Problem::new(
            StatusCode::CONFLICT,
            "Desktop integration disabled",
            "Enable Windows-MCP in Security settings before trusting the conversation.",
        ));
    }
    let identity: Option<String> = sqlx::query_scalar(
        "SELECT conversation_scope_hash FROM tasks WHERE id=?",
    )
    .bind(&id)
    .fetch_one(state.repository.pool())
    .await
    .map_err(db_problem)?;
    if !identity.as_deref().is_some_and(|scope| !scope.trim().is_empty()) {
        return Err(Problem::new(
            StatusCode::CONFLICT,
            "Conversation identity unbound",
            "Send a new ChatGPT message from this conversation to bind its identity before enabling desktop trust.",
        ));
    }
    // Do not create a dormant permission that will silently activate later:
    // the upstream must be healthy at the time of this explicit grant.
    let tools = crate::desktop_bridge::probe(port).await.map_err(|_| {
        Problem::new(
            StatusCode::CONFLICT,
            "Windows-MCP not connected",
            "Check Windows-MCP diagnostics and start the loopback Streamable HTTP service first.",
        )
    })?;
    let suitable = if change.scope == "observe" {
        tools.iter().any(|t| t == "Snapshot" || t == "Screenshot")
    } else {
        [
            "Snapshot",
            "Screenshot",
            "Click",
            "Type",
            "Scroll",
            "Shortcut",
            "App",
        ]
        .iter()
        .all(|needed| tools.iter().any(|t| t == needed))
    };
    if !suitable {
        return Err(Problem::new(
            StatusCode::CONFLICT,
            "Windows-MCP tools missing",
            "Start Windows-MCP with the approved UI tool allowlist.",
        ));
    }
    let now = now_ms();
    let expires = now.saturating_add(i64::from(change.duration_minutes) * 60_000);
    let audit_id = Uuid::new_v4().to_string();
    let mut tx = state.repository.pool().begin().await.map_err(db_problem)?;
    sqlx::query(
        "INSERT INTO desktop_task_trust(task_id,agent_id,scope,port,expires_at_ms,updated_at_ms)
        VALUES(?,?,?,?,?,?) ON CONFLICT(task_id) DO UPDATE SET
        agent_id=excluded.agent_id,scope=excluded.scope,port=excluded.port,
        expires_at_ms=excluded.expires_at_ms,updated_at_ms=excluded.updated_at_ms",
    )
    .bind(&id)
    .bind(&agent_id)
    .bind(&change.scope)
    .bind(i64::from(port))
    .bind(expires)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(db_problem)?;
    let payload = json!({"source":"authenticatedLocalUi","change":"desktopTrustGranted",
        "scope":change.scope,"expiresAtMs":expires});
    sqlx::query("INSERT INTO timeline_events(event_id,task_id,turn_id,session_id,actor,kind,idempotency_key,payload_json,metadata_json,created_at_ms)
        VALUES(?,?,NULL,NULL,'user','status',?,?,NULL,?)")
        .bind(&audit_id).bind(&id).bind(&audit_id).bind(payload.to_string())
        .bind(now).execute(&mut *tx).await.map_err(db_problem)?;
    tx.commit().await.map_err(db_problem)?;
    let mut event = AppEvent::new("desktop.trust_changed", payload);
    event.task_id = Some(id.clone());
    state.publish(event);
    Ok(Json(json!({"taskId":id,"active":true,"scope":change.scope,
        "port":port,"expiresAtMs":expires})))
}

pub(super) async fn revoke_task_desktop_trust(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    task_agent(&state, &id).await?;
    let now = now_ms();
    let mut tx = state.repository.pool().begin().await.map_err(db_problem)?;
    sqlx::query("DELETE FROM desktop_task_trust WHERE task_id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(db_problem)?;
    let audit_id = Uuid::new_v4().to_string();
    let payload = json!({"source":"authenticatedLocalUi","change":"desktopTrustRevoked"});
    sqlx::query("INSERT INTO timeline_events(event_id,task_id,turn_id,session_id,actor,kind,idempotency_key,payload_json,metadata_json,created_at_ms)
        VALUES(?,?,NULL,NULL,'user','status',?,?,NULL,?)")
        .bind(&audit_id).bind(&id).bind(&audit_id).bind(payload.to_string())
        .bind(now).execute(&mut *tx).await.map_err(db_problem)?;
    tx.commit().await.map_err(db_problem)?;
    let mut event = AppEvent::new("desktop.trust_changed", payload);
    event.task_id = Some(id.clone());
    state.publish(event);
    Ok(Json(
        json!({"taskId":id,"active":false,"scope":"none","expiresAtMs":null}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::chatgpt_router_tests::{extension_request, fixture};

    #[tokio::test]
    async fn extension_cannot_change_or_read_desktop_trust() {
        let (_state, app, _directory) = fixture("running").await;
        for (method, body) in [
            ("GET", json!({})),
            ("PUT", json!({"scope":"control","durationMinutes":480})),
            ("DELETE", json!({})),
        ] {
            let response =
                extension_request(&app, method, "/api/local/tasks/task-a/desktop-trust", body)
                    .await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
    }

    #[tokio::test]
    async fn local_trust_requires_enabled_desktop_and_real_task() {
        let (state, _app, _directory) = fixture("running").await;
        let read = task_desktop_trust(State(state.clone()), Path("task-a".into()))
            .await
            .unwrap();
        assert_eq!(read.0["active"], false);
        let result = set_task_desktop_trust(
            State(state.clone()),
            Path("task-a".into()),
            Json(DesktopTrustChange {
                scope: "control".into(),
                duration_minutes: 60,
            }),
        )
        .await;
        assert_eq!(result.unwrap_err().status, StatusCode::CONFLICT);
        assert!(
            task_desktop_trust(State(state), Path("not-a-task".into()))
                .await
                .is_err()
        );
    }
}
