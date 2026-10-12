//! Authenticated task-specific Skills routing observations.
use super::{Problem, db_problem};
use crate::websocket::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use sqlx::Row as _;
use std::sync::Arc;

pub(super) async fn task_skill_routing(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Problem> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE id=?")
        .bind(&id)
        .fetch_one(state.repository.pool())
        .await
        .map_err(db_problem)?;
    if exists == 0 {
        return Err(Problem::new(
            StatusCode::NOT_FOUND,
            "Task not found",
            "Select a conversation task.",
        ));
    }
    let rows = sqlx::query(
        "SELECT turn_id,payload_json,created_at_ms FROM timeline_events
         WHERE task_id=? AND kind='status'
           AND json_extract(payload_json,'$.source')='skill_router'
         ORDER BY created_at_ms DESC,event_id DESC LIMIT 60",
    )
    .bind(&id)
    .fetch_all(state.repository.pool())
    .await
    .map_err(db_problem)?;
    let mut turn: Option<String> = None;
    let mut suggestions = Vec::<Value>::new();
    let mut history = Vec::<Value>::new();
    let mut read_ids = Vec::<String>::new();
    for row in &rows {
        let payload: Value =
            serde_json::from_str(row.get::<&str, _>("payload_json")).unwrap_or(Value::Null);
        if payload["eventType"] == "suggested" {
            turn = row.get::<Option<String>, _>("turn_id");
            suggestions = payload["suggestions"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            break;
        }
    }
    for row in &rows {
        let payload: Value =
            serde_json::from_str(row.get::<&str, _>("payload_json")).unwrap_or(Value::Null);
        let row_turn = row.get::<Option<String>, _>("turn_id");
        if payload["eventType"] == "read" && row_turn == turn {
            if let Some(skill) = payload["skillId"].as_str() {
                if !read_ids.iter().any(|s| s == skill) {
                    read_ids.push(skill.to_owned());
                }
            }
        }
        if history.len() < 18 {
            history.push(json!({"turnId":row_turn,"eventType":payload["eventType"],
                "skillId":payload["skillId"],"truncated":payload["truncated"],
                "atMs":row.get::<i64,_>("created_at_ms")}));
        }
    }
    Ok(Json(
        json!({"taskId":id,"turnId":turn,"suggestions":suggestions,
        "readIds":read_ids,"history":history,"adherence":"notVerified"}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::chatgpt_router_tests::{extension_request, fixture};
    #[tokio::test]
    async fn extension_client_cannot_read_task_skill_history() {
        let (_state, app, _dir) = fixture("running").await;
        let response = extension_request(
            &app,
            "GET",
            "/api/local/tasks/task-a/skill-routing",
            json!({}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    #[tokio::test]
    async fn skill_panel_shows_only_observed_reads_in_current_task_turn() {
        let (state, _app, _dir) = fixture("running").await;
        for (event, turn, kind, payload) in [
            (
                "skill-a",
                "turn-1",
                "suggested",
                json!({"source":"skill_router","eventType":"suggested","suggestions":[{"skillId":"blueprint-flow","title":"Blueprint","reason":"蓝图","score":8}]}),
            ),
            (
                "skill-b",
                "turn-1",
                "read",
                json!({"source":"skill_router","eventType":"read","skillId":"blueprint-flow","truncated":false}),
            ),
            (
                "skill-c",
                "turn-2",
                "suggested",
                json!({"source":"skill_router","eventType":"suggested","suggestions":[{"skillId":"frontend-skill","title":"Frontend","reason":"前端","score":9}]}),
            ),
        ] {
            sqlx::query("INSERT INTO timeline_events(event_id,task_id,turn_id,actor,kind,idempotency_key,payload_json,created_at_ms) VALUES(?,'task-a',?,'system','status',?,?,?)")
                .bind(event).bind(turn).bind(event).bind(payload.to_string())
                .bind(match kind {"suggested" if turn=="turn-2"=>20,_=>10})
                .execute(state.repository.pool()).await.expect("insert event");
        }
        let Json(data) = task_skill_routing(State(state.clone()), Path("task-a".into()))
            .await
            .expect("routing");
        assert_eq!(data["turnId"], "turn-2");
        assert_eq!(data["suggestions"][0]["skillId"], "frontend-skill");
        assert_eq!(data["readIds"], json!([])); // Old turn read must not count as current.
        assert_eq!(data["adherence"], "notVerified");
        let Json(empty) = task_skill_routing(State(state), Path("task-b".into()))
            .await
            .expect("empty");
        assert_eq!(empty["suggestions"], json!([])); // Task isolation
        assert_eq!(empty["history"], json!([]));
    }
}
