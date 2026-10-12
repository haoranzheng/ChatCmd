//! Local GUI-authenticated Windows desktop bridge configuration.
use super::{Problem, db_problem, now_ms};
use crate::websocket::AppState;
use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{env, path::Path, sync::Arc, time::Duration};

#[cfg(windows)]
struct OwnedWindowsMcp {
    port: u16,
    child: tokio::process::Child,
}

#[cfg(windows)]
fn owned_server() -> &'static tokio::sync::Mutex<Option<OwnedWindowsMcp>> {
    static OWNED: std::sync::OnceLock<tokio::sync::Mutex<Option<OwnedWindowsMcp>>> =
        std::sync::OnceLock::new();
    OWNED.get_or_init(|| tokio::sync::Mutex::new(None))
}

#[cfg(windows)]
fn windows_mcp_executable() -> Option<std::path::PathBuf> {
    let name = "windows-mcp.exe";
    let mut candidates = Vec::new();
    if let Some(home) = env::var_os("USERPROFILE") {
        candidates.push(
            std::path::PathBuf::from(home)
                .join(".local")
                .join("bin")
                .join(name),
        );
    }
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path).map(|p| p.join(name)));
    }
    candidates.into_iter().find(|path| path.is_file())
}

#[cfg(any(windows, test))]
fn limited_start_args(port: u16) -> Vec<String> {
    [
        "serve",
        "--transport",
        "streamable-http",
        "--host",
        "127.0.0.1",
        "--port",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([
        port.to_string(),
        "--tools".to_owned(),
        "Snapshot,Screenshot,Click,Type,Scroll,Shortcut,App".to_owned(),
    ])
    .collect()
}

#[cfg(windows)]
async fn managed_server_active() -> bool {
    let mut guard = owned_server().lock().await;
    if let Some(owned) = guard.as_mut() {
        if matches!(owned.child.try_wait(), Ok(None)) {
            return true;
        }
    }
    *guard = None;
    false
}

#[cfg(not(windows))]
async fn managed_server_active() -> bool {
    false
}

/// Explicit authenticated local UI request. Never starts a service on boot or
/// exposes Windows-MCP beyond the Windows user session's loopback interface.
pub(super) async fn start_desktop_server(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, Problem> {
    #[cfg(not(windows))]
    {
        let _ = state;
        Err(Problem::new(
            StatusCode::NOT_IMPLEMENTED,
            "Windows only",
            "Windows-MCP can only be launched on Windows.",
        ))
    }
    #[cfg(windows)]
    {
        let (enabled, port) = crate::desktop_bridge::config(state.repository.pool())
            .await
            .map_err(|_| {
                Problem::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Desktop configuration unavailable",
                    "Unable to inspect desktop configuration.",
                )
            })?;
        if !enabled {
            return Err(Problem::new(
                StatusCode::CONFLICT,
                "Desktop integration disabled",
                "Enable and save desktop integration before starting Windows-MCP.",
            ));
        }
        if !crate::desktop_bridge::allowed_port(port, Some(state.port)) {
            return Err(Problem::new(
                StatusCode::BAD_REQUEST,
                "Invalid desktop port",
                "Configured port conflicts with ChatCMD.",
            ));
        }
        let mut guard = owned_server().lock().await;
        if let Some(owned) = guard.as_mut() {
            if matches!(owned.child.try_wait(), Ok(None)) {
                if owned.port == port {
                    return Ok(Json(
                        json!({"started":true,"managedByChatCmd":true,"alreadyRunning":true,"port":port}),
                    ));
                }
                return Err(Problem::new(
                    StatusCode::CONFLICT,
                    "Managed server already running",
                    "Stop the managed server before changing its port.",
                ));
            }
        }
        *guard = None;
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return Err(Problem::new(
                StatusCode::CONFLICT,
                "Desktop port occupied",
                "Another process owns the configured port. ChatCMD will not take control of it.",
            ));
        }
        let executable = windows_mcp_executable().ok_or_else(|| {
            Problem::new(
                StatusCode::NOT_FOUND,
                "Windows-MCP launcher missing",
                "Install windows-mcp.exe or add the installed launcher to PATH.",
            )
        })?;
        let mut command = tokio::process::Command::new(executable);
        command.args(limited_start_args(port));
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        let mut child = command.spawn().map_err(|_| {
            Problem::new(
                StatusCode::BAD_GATEWAY,
                "Windows-MCP startup failed",
                "The user-session launcher could not start.",
            )
        })?;
        let mut healthy = false;
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(350)).await;
            if !matches!(child.try_wait(), Ok(None)) {
                break;
            }
            if let Ok(Ok(tools)) =
                tokio::time::timeout(Duration::from_secs(2), crate::desktop_bridge::probe(port))
                    .await
            {
                if [
                    "Snapshot",
                    "Screenshot",
                    "Click",
                    "Type",
                    "Scroll",
                    "Shortcut",
                    "App",
                ]
                .iter()
                .all(|expected| tools.iter().any(|tool| tool == expected))
                {
                    healthy = true;
                    break;
                }
            }
        }
        if !healthy {
            let _ = child.kill().await;
            return Err(Problem::new(
                StatusCode::BAD_GATEWAY,
                "Windows-MCP failed health check",
                "The new process did not expose all seven UI tools on 127.0.0.1. It was stopped.",
            ));
        }
        *guard = Some(OwnedWindowsMcp { port, child });
        Ok(Json(
            json!({"started":true,"managedByChatCmd":true,"alreadyRunning":false,"port":port}),
        ))
    }
}

/// Stop *only* a server that this ChatCMD process explicitly launched.
/// Never kill an unrelated process found on the configured port.
pub(super) async fn stop_desktop_server(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, Problem> {
    #[cfg(not(windows))]
    {
        let _ = state;
        Err(Problem::new(
            StatusCode::NOT_IMPLEMENTED,
            "Windows only",
            "No managed Windows-MCP server on this platform.",
        ))
    }
    #[cfg(windows)]
    {
        let mut guard = owned_server().lock().await;
        let Some(mut owned) = guard.take() else {
            return Err(Problem::new(
                StatusCode::CONFLICT,
                "No managed server",
                "ChatCMD can stop only Windows-MCP processes it started in this session.",
            ));
        };
        if matches!(owned.child.try_wait(), Ok(None)) {
            owned.child.kill().await.map_err(|_| {
                Problem::new(
                    StatusCode::BAD_GATEWAY,
                    "Cannot stop Windows-MCP",
                    "The managed process could not be stopped.",
                )
            })?;
        }
        // Stopping the integrated service revokes unattended desktop control.
        sqlx::query("DELETE FROM desktop_task_trust")
            .execute(state.repository.pool())
            .await
            .map_err(db_problem)?;
        Ok(Json(
            json!({"stopped":true,"managedByChatCmd":false,"port":owned.port}),
        ))
    }
}

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
    let old = crate::desktop_bridge::config(state.repository.pool())
        .await
        .map_err(|_| {
            Problem::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Desktop settings unavailable",
                "Unable to load desktop settings",
            )
        })?;
    let mut transaction = state.repository.pool().begin().await.map_err(db_problem)?;
    let now = now_ms();
    for (key, value) in [
        ("desktop_enabled", json!(c.enabled)),
        ("desktop_port", json!(c.port)),
    ] {
        sqlx::query("INSERT INTO settings(key,value_json,updated_at_ms) VALUES(?,?,?)
            ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at_ms=excluded.updated_at_ms")
            .bind(key).bind(value.to_string()).bind(now)
            .execute(&mut *transaction).await.map_err(db_problem)?;
    }
    if old != (c.enabled, c.port) {
        // A disabled integration or different local listener invalidates all
        // previously granted unattended desktop permissions.
        sqlx::query("DELETE FROM desktop_task_trust")
            .execute(&mut *transaction)
            .await
            .map_err(db_problem)?;
    }
    transaction.commit().await.map_err(db_problem)?;
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
    let launcher_found = launcher_found();
    let managed_by_chatcmd = managed_server_active().await;
    let checked_at_ms = now_ms();
    if !enabled {
        return Ok(Json(json!({
            "enabled":false,"connected":false,"tools":[],"managedByChatCmd":managed_by_chatcmd,"checkedAtMs":checked_at_ms,
            "checks":{"launcherFound":launcher_found,"portListening":null},
            "error":"desktop_disabled",
            "repairHints":["Enable Windows desktop integration in local Security settings; it is disabled by default."]
        })));
    }
    let expected = [
        "Snapshot",
        "Screenshot",
        "Click",
        "Type",
        "Scroll",
        "Shortcut",
        "App",
    ];
    Ok(Json(match crate::desktop_bridge::probe(port).await {
        Ok(tools) => {
            let missing: Vec<&str> = expected
                .into_iter()
                .filter(|name| !tools.iter().any(|t| t == name))
                .collect();
            if missing.is_empty() {
                json!({"enabled":true,"connected":true,"tools":tools,"missingTools":[],"managedByChatCmd":managed_by_chatcmd,
                    "checkedAtMs":checked_at_ms,
                    "checks":{"launcherFound":launcher_found,"portListening":true},"repairHints":[]})
            } else {
                json!({"enabled":true,"connected":true,"tools":tools,"missingTools":missing,"managedByChatCmd":managed_by_chatcmd,
                    "checkedAtMs":checked_at_ms, "error":"desktop_tools_missing",
                    "checks":{"launcherFound":launcher_found,"portListening":true},
                    "repairHints":["Restart Windows-MCP with all seven UI tools enabled: Snapshot,Screenshot,Click,Type,Scroll,Shortcut,App."]})
            }
        }
        Err(error) => {
            // Diagnose without running programs, mutating settings, or opening
            // firewall/network access. Restrict the probe to the fixed loopback port.
            let listening = tokio::time::timeout(
                Duration::from_secs(2),
                tokio::net::TcpStream::connect(("127.0.0.1", port)),
            )
            .await
            .is_ok_and(|value| value.is_ok());
            json!({"enabled":true,"connected":false,"tools":[],"managedByChatCmd":managed_by_chatcmd,"checkedAtMs":checked_at_ms,
                "error":error.code,
                "checks":{"launcherFound":launcher_found,"portListening":listening},
                "repairHints":repair_hints(&error.code, listening, launcher_found)})
        }
    }))
}

fn launcher_found() -> bool {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|directory| {
            ["uvx", "windows-mcp"].iter().any(|name| {
                Path::new(&directory)
                    .join(format!("{name}{suffix}"))
                    .is_file()
            })
        })
    })
}

fn repair_hints(error: &str, listening: bool, launcher: bool) -> Vec<&'static str> {
    if !listening {
        let mut hints = vec![
            "No TCP listener found on the configured 127.0.0.1 port. Check the Windows-MCP process and port.",
        ];
        if !launcher {
            hints.push("uvx/windows-mcp was not found on PATH. Install uv and Windows-MCP before retrying.");
        }
        hints.push("Run: uvx windows-mcp serve --transport streamable-http --host 127.0.0.1 --port 8000 --tools \"Snapshot,Screenshot,Click,Type,Scroll,Shortcut,App\" (change 8000 to your configured port).");
        return hints;
    }
    if error == "desktop_upstream_http" {
        vec![
            "A service is listening, but it rejected MCP requests. Check for a port conflict and verify the Streamable HTTP /mcp endpoint.",
        ]
    } else if error == "desktop_protocol_error" {
        vec![
            "The TCP port is open but MCP initialize/tools-list failed. Check Windows-MCP transport, protocol version, and service logs.",
        ]
    } else if error == "desktop_response_too_large" {
        vec![
            "Windows-MCP returned too much data. Reduce screenshot scaling and check the upstream server.",
        ]
    } else {
        vec![
            "The desktop service accepts TCP connections but MCP communication failed. Review Windows-MCP logs and test the configured port.",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::chatgpt_router_tests::{extension_request, fixture};

    #[tokio::test]
    async fn extension_cannot_enable_desktop_bridge() {
        let (_state, app, _temp) = fixture("completed").await;
        for (method, path, input) in [
            (
                "PUT",
                "/api/local/desktop/config",
                json!({"enabled":true,"port":8000}),
            ),
            ("GET", "/api/local/desktop/status", json!({})),
            ("POST", "/api/local/desktop/server/start", json!({})),
            ("POST", "/api/local/desktop/server/stop", json!({})),
        ] {
            let response = extension_request(&app, method, path, input).await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
    }

    #[tokio::test]
    async fn desktop_diagnosis_reports_disabled_without_network_probe() {
        let (state, _app, _temp) = fixture("running").await;
        let status = desktop_status(State(state)).await.unwrap().0;
        assert_eq!(status["connected"], false);
        assert_eq!(status["error"], "desktop_disabled");
        assert!(
            status["repairHints"]
                .as_array()
                .is_some_and(|hints| !hints.is_empty())
        );
    }

    #[test]
    fn launcher_is_loopback_only_and_tool_limited() {
        let args = limited_start_args(8765);
        assert_eq!(
            args,
            [
                "serve",
                "--transport",
                "streamable-http",
                "--host",
                "127.0.0.1",
                "--port",
                "8765",
                "--tools",
                "Snapshot,Screenshot,Click,Type,Scroll,Shortcut,App"
            ]
        );
    }

    #[test]
    fn diagnosis_distinguishes_listener_and_missing_launcher() {
        let no_listener = repair_hints("desktop_unavailable", false, false);
        assert_eq!(no_listener.len(), 3);
        let wrong_protocol = repair_hints("desktop_protocol_error", true, true);
        assert!(wrong_protocol[0].contains("MCP initialize"));
    }
}
