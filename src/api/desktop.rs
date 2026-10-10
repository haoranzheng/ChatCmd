//! Local GUI-authenticated Windows desktop bridge configuration.
use super::{Problem, now_ms, storage_problem};
use crate::websocket::AppState;
use axum::{Json, extract::State, http::StatusCode};
use chatcmd_core::{Setting, SettingsStore as _};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DesktopChange {
    enabled: bool,
    port: u16,
}

pub(super) async fn desktop_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, Problem> {
    let (enabled, port) = crate::desktop_bridge::config(state.repository.pool())
        .await
        .map_err(|_| {
            Problem::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Desktop settings unavailable",
                "Unable to load desktop settings",
            )
        })?;
    Ok(Json(
        json!({"enabled":enabled,"port":port,"endpoint":format!("http://127.0.0.1:{port}/mcp")}),
    ))
}
pub(super) async fn save_desktop_config(
    State(state): State<Arc<AppState>>,
    Json(c): Json<DesktopChange>,
) -> Result<Json<Value>, Problem> {
    if !crate::desktop_bridge::allowed_port(c.port, Some(state.port)) {
        return Err(Problem::new(
            StatusCode::BAD_REQUEST,
            "Invalid port",
            "Port must be 1..65535 and different from the ChatCMD port",
        ));
    }
    for (key, value) in [
        ("desktop_enabled", json!(c.enabled)),
        ("desktop_port", json!(c.port)),
    ] {
        state
            .repository
            .set_setting(&Setting {
                key: key.to_owned(),
                value_json: value.to_string(),
                updated_at_ms: now_ms(),
            })
            .await
            .map_err(storage_problem)?;
    }
    desktop_config(State(state)).await
}
pub(super) async fn desktop_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, Problem> {
    let (enabled, port) = crate::desktop_bridge::config(state.repository.pool())
        .await
        .map_err(|_| {
            Problem::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Desktop settings unavailable",
                "Unable to load desktop settings",
            )
        })?;
    if !enabled {
        return Ok(Json(json!({"enabled":false,"connected":false,"tools":[]})));
    }
    Ok(Json(match crate::desktop_bridge::probe(port).await {
        Ok(tools) => json!({"enabled":true,"connected":true,"tools":tools}),
        Err(e) => json!({"enabled":true,"connected":false,"tools":[],"error":e.code}),
    }))
}
