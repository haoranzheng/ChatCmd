//! Windows-MCP Streamable HTTP client. Only 127.0.0.1:<port>/mcp is reachable.
//! No arbitrary upstream endpoint, remote host, or unrestricted tools/call passthrough.
use chatcmd_runtime::{RuntimeError, RuntimeResult};
use serde_json::{Value, json};
use sqlx::Row as _;
use std::time::Duration;

fn err(code: &str, message: &str) -> RuntimeError {
    RuntimeError::new(code, message)
}
const MAX_BYTES: usize = 6 * 1024 * 1024;

pub(crate) async fn config(pool: &sqlx::SqlitePool) -> RuntimeResult<(bool, u16)> {
    let rows = sqlx::query(
        "SELECT key,value_json FROM settings WHERE key IN ('desktop_enabled','desktop_port')",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| err("desktop_storage_error", "Cannot read desktop settings"))?;
    let (mut enabled, mut port) = (false, 8000);
    for row in rows {
        let json: String = row.get("value_json");
        match row.get::<&str, _>("key") {
            "desktop_enabled" => enabled = serde_json::from_str::<bool>(&json).unwrap_or(false),
            "desktop_port" => port = serde_json::from_str::<u16>(&json).unwrap_or(8000),
            _ => {}
        }
    }
    Ok((enabled, port))
}
pub(crate) fn allowed_port(port: u16, chatcmd_port: Option<u16>) -> bool {
    port > 0 && Some(port) != chatcmd_port
}

fn decode(bytes: &[u8]) -> RuntimeResult<Value> {
    if let Ok(value) = serde_json::from_slice(bytes) {
        return Ok(value);
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| err("desktop_protocol_error", "Invalid upstream text encoding"))?;
    for line in text.lines() {
        if let Some(data) = line.trim_end_matches('\r').strip_prefix("data:") {
            if let Ok(value) = serde_json::from_str::<Value>(data.trim()) {
                if value.get("jsonrpc").is_some()
                    && (value.get("result").is_some() || value.get("error").is_some())
                {
                    return Ok(value);
                }
            }
        }
    }
    Err(err(
        "desktop_protocol_error",
        "No JSON-RPC result from Windows-MCP",
    ))
}

async fn post(
    client: &reqwest::Client,
    port: u16,
    session: Option<&str>,
    payload: Value,
) -> RuntimeResult<(Value, Option<String>)> {
    let url = format!("http://127.0.0.1:{port}/mcp");
    let mut request = client
        .post(url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json")
        .header("MCP-Protocol-Version", "2025-03-26")
        .json(&payload);
    if let Some(id) = session {
        if id.len() > 128 || !id.is_ascii() || id.bytes().any(|x| x.is_ascii_control()) {
            return Err(err("desktop_protocol_error", "Unsafe MCP session id"));
        }
        request = request.header("Mcp-Session-Id", id);
    }
    let mut response = request.send().await.map_err(|_| {
        err(
            "desktop_unavailable",
            "Windows-MCP unreachable at 127.0.0.1",
        )
    })?;
    if !response.status().is_success() {
        return Err(err("desktop_upstream_http", "Windows-MCP rejected request"));
    }
    let next_session = response
        .headers()
        .get("mcp-session-id")
        .and_then(|x| x.to_str().ok())
        .map(str::to_owned);
    // Streamable HTTP may keep an SSE response open after sending the result.
    // Complete the request as soon as its matching JSON-RPC message arrives.
    if payload.get("method").and_then(Value::as_str) == Some("notifications/initialized") {
        return Ok((Value::Null, next_session));
    }
    let expected_id = payload.get("id").cloned().ok_or_else(|| {
        err(
            "desktop_protocol_error",
            "A desktop request must have a JSON-RPC id",
        )
    })?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| err("desktop_transport_error", "Incomplete MCP response"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
            return Err(err(
                "desktop_response_too_large",
                "Windows-MCP returned oversized response",
            ));
        }
        bytes.extend_from_slice(&chunk);
        // A JSON response is complete once it parses. For SSE, wait for a
        // terminated event, not for the upstream to close the HTTP stream.
        let json_value = serde_json::from_slice::<Value>(&bytes).ok();
        let sse_ready = bytes.windows(2).any(|w| w == b"\n\n")
            || bytes.windows(4).any(|w| w == b"\r\n\r\n");
        let parsed = json_value.or_else(|| sse_ready.then(|| decode(&bytes).ok()).flatten());
        if let Some(value) = parsed {
            if value.get("id") != Some(&expected_id) {
                return Err(err(
                    "desktop_protocol_error",
                    "Unexpected JSON-RPC response id",
                ));
            }
            if value.get("error").is_some() {
                return Err(err(
                    "desktop_upstream_error",
                    "Windows-MCP rejected JSON-RPC call",
                ));
            }
            if value.get("result").is_none() {
                return Err(err("desktop_protocol_error", "Missing JSON-RPC result"));
            }
            return Ok((value, next_session));
        }
    }
    let value = decode(&bytes)?;
    if value.get("id") != Some(&expected_id) || value.get("result").is_none() {
        return Err(err(
            "desktop_protocol_error",
            "Invalid JSON-RPC response identity",
        ));
    }
    Ok((value, next_session))
}

async fn establish(port: u16) -> RuntimeResult<(reqwest::Client, Option<String>, Vec<String>)> {
    if !allowed_port(port, None) {
        return Err(err("desktop_invalid_port", "Invalid desktop MCP port"));
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(35))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|_| err("desktop_transport_error", "Failed to create MCP client"))?;
    let (initialized,session)=post(&client,port,None,json!({
        "jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"ChatCMD","version":"0.1"}}
    })).await?;
    if initialized
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str)
        .is_none()
    {
        return Err(err(
            "desktop_protocol_error",
            "Windows-MCP initialize failed",
        ));
    }
    post(
        &client,
        port,
        session.as_deref(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await?;
    let (catalog, _) = post(
        &client,
        port,
        session.as_deref(),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    )
    .await?;
    let tools = catalog
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            err(
                "desktop_protocol_error",
                "Missing Windows-MCP tools/list result",
            )
        })?
        .iter()
        .filter_map(|x| x.get("name").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    Ok((client, session, tools))
}

fn field_text(args: &Value, key: &str, max: usize) -> RuntimeResult<String> {
    let s = args.get(key).and_then(Value::as_str).unwrap_or("");
    if s.is_empty() || s.len() > max || s.contains('\0') {
        return Err(err(
            "desktop_invalid_arguments",
            "Required desktop string is empty or too long",
        ));
    }
    Ok(s.to_owned())
}
fn point(args: &Value, required: bool) -> RuntimeResult<serde_json::Map<String, Value>> {
    let mut out = serde_json::Map::new();
    if let Some(label) = args.get("label") {
        let label = label
            .as_u64()
            .filter(|x| *x <= 100000)
            .ok_or_else(|| err("desktop_invalid_arguments", "Invalid UI label"))?;
        out.insert("label".into(), json!(label));
    }
    if let Some(loc) = args.get("loc") {
        let valid = loc.as_array().is_some_and(|a| {
            a.len() == 2
                && a.iter()
                    .all(|x| x.as_i64().is_some_and(|n| (-32768..=32768).contains(&n)))
        });
        if !valid {
            return Err(err("desktop_invalid_arguments", "loc must be two integers"));
        }
        out.insert("loc".into(), loc.clone());
    }
    if out.len() > 1 || (required && out.is_empty()) {
        return Err(err(
            "desktop_invalid_arguments",
            "Provide exactly one of loc or label",
        ));
    }
    Ok(out)
}
pub(crate) fn observe(args: &Value) -> RuntimeResult<(&'static str, Value)> {
    match args
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("snapshot")
    {
        "snapshot" => Ok((
            "Snapshot",
            json!({"use_vision":false,"use_ui_tree":true,"use_annotation":false}),
        )),
        "screenshot" => Ok(("Screenshot", json!({"use_annotation":false}))),
        _ => Err(err(
            "desktop_denied",
            "Only Screenshot or Snapshot may be called",
        )),
    }
}
pub(crate) fn control(args: &Value) -> RuntimeResult<(&'static str, Value)> {
    let action = args.get("action").and_then(Value::as_str).unwrap_or("");
    let mut out = match action {
        "click" | "type" => point(args, true)?,
        "scroll" => point(args, false)?,
        _ => serde_json::Map::new(),
    };
    let name = match action {
        "click" => {
            let button = args.get("button").and_then(Value::as_str).unwrap_or("left");
            let clicks = args.get("clicks").and_then(Value::as_u64).unwrap_or(1);
            if !matches!(button, "left" | "middle" | "right") || !(1..=2).contains(&clicks) {
                return Err(err("desktop_invalid_arguments", "Invalid button or clicks"));
            }
            out.insert("button".into(), json!(button));
            out.insert("clicks".into(), json!(clicks));
            "Click"
        }
        "type" => {
            out.insert("text".into(), json!(field_text(args, "text", 2048)?));
            out.insert(
                "clear".into(),
                json!(args.get("clear").and_then(Value::as_bool).unwrap_or(false)),
            );
            out.insert("press_enter".into(), json!(false));
            "Type"
        }
        "scroll" => {
            let dir = args
                .get("direction")
                .and_then(Value::as_str)
                .unwrap_or("down");
            let wheel = args.get("wheelTimes").and_then(Value::as_u64).unwrap_or(1);
            if !matches!(dir, "up" | "down") || !(1..=10).contains(&wheel) {
                return Err(err("desktop_invalid_arguments", "Invalid vertical scroll"));
            }
            out.insert("type".into(), json!("vertical"));
            out.insert("direction".into(), json!(dir));
            out.insert("wheel_times".into(), json!(wheel));
            "Scroll"
        }
        "shortcut" => {
            let s = field_text(args, "shortcut", 60)?;
            // Explicit safe-navigation list prevents shortcuts that launch commands
            // or system utilities (e.g. Win+R) from bypassing process controls.
            if !matches!(
                s.to_ascii_lowercase().as_str(),
                "ctrl+a"
                    | "ctrl+c"
                    | "ctrl+v"
                    | "ctrl+z"
                    | "ctrl+f"
                    | "ctrl+s"
                    | "alt+tab"
                    | "tab"
                    | "enter"
                    | "esc"
                    | "escape"
                    | "shift+tab"
                    | "ctrl+tab"
                    | "ctrl+shift+tab"
            ) {
                return Err(err(
                    "desktop_denied",
                    "Shortcut not in desktop v0.1 allowlist",
                ));
            }
            out.insert("shortcut".into(), json!(s));
            "Shortcut"
        }
        "switch_window" => {
            out.insert("mode".into(), json!("switch"));
            out.insert("name".into(), json!(field_text(args, "name", 160)?));
            "App"
        }
        _ => return Err(err("desktop_denied", "Unapproved desktop tool or action")),
    };
    Ok((name, Value::Object(out)))
}
pub(crate) async fn probe(port: u16) -> RuntimeResult<Vec<String>> {
    let (_, _, all) = establish(port).await?;
    Ok(all
        .into_iter()
        .filter(|x| {
            matches!(
                x.as_str(),
                "Snapshot" | "Screenshot" | "Click" | "Type" | "Scroll" | "Shortcut" | "App"
            )
        })
        .collect())
}
pub(crate) async fn invoke(port: u16, tool: &str, args: &Value) -> RuntimeResult<Value> {
    let (name, params) = match tool {
        "desktop_observe" => observe(args)?,
        "desktop_control" => control(args)?,
        _ => return Err(err("desktop_denied", "Unsupported desktop tool")),
    };
    let (client, session, catalog) = establish(port).await?;
    if !catalog.iter().any(|x| x == name) {
        return Err(err(
            "desktop_tool_unavailable",
            "Approved Windows-MCP tool not present",
        ));
    }
    let (response, _) = post(
        &client,
        port,
        session.as_deref(),
        json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":name,"arguments":params}
        }),
    )
    .await?;
    let result = response
        .get("result")
        .ok_or_else(|| err("desktop_protocol_error", "Missing tool result"))?;
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(err(
            "desktop_upstream_error",
            "Windows-MCP tool returned an error",
        ));
    }
    Ok(json!({"tool":name,"result":result}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn denies_injected_desktop_methods() {
        for action in [
            "launch",
            "process",
            "powershell",
            "registry",
            "filesystem",
            "launch_executable",
        ] {
            assert!(control(&json!({"action":action,"name":"calc.exe"})).is_err());
        }
        let (tool,p)=control(&json!({"action":"switch_window","name":"Notepad","executable":"cmd.exe","cwd":"C:\\\\Windows"})).unwrap();
        assert_eq!(tool, "App");
        assert_eq!(p, json!({"mode":"switch","name":"Notepad"}));
        assert!(control(&json!({"action":"click","loc":[1,2],"label":1})).is_err());
        assert!(control(&json!({"action":"shortcut","shortcut":"win+r"})).is_err());
        assert_eq!(
            control(&json!({"action":"shortcut","shortcut":"ctrl+s"}))
                .unwrap()
                .0,
            "Shortcut"
        );
        assert!(control(&json!({"action":"type","label":1,"text":"X".repeat(2049)})).is_err());
    }
    #[test]
    fn rejects_incomplete_sse_and_accepts_complete_events() {
        let json = json!({"jsonrpc":"2.0","id":7,"result":{"ok":true}});
        assert!(decode(format!("event: message\ndata: {json}").as_bytes()).is_ok());
        assert!(decode(b"event: message\ndata: {\"jsonrpc\":").is_err());
        assert_eq!(
            decode(format!("event: message\ndata: {json}\n\n").as_bytes()).unwrap(),
            json
        );
    }
    #[test]
    fn sse_and_json_parsing() {
        let j = json!({"jsonrpc":"2.0","id":1,"result":{"ok":true}});
        assert_eq!(decode(j.to_string().as_bytes()).unwrap(), j);
        assert_eq!(
            decode(format!("event: message\ndata: {j}\n\n").as_bytes()).unwrap(),
            j
        );
        assert!(decode(b"data: malformed").is_err());
    }
}
