//! Opt-in task-scoped, bounded text edit trust. The normal workspace
//! gate and catalog policy have already run before this function is called.
use chatcmd_runtime::{OperationContext, RuntimeError, RuntimeResult};
use serde_json::{Value, json};
use sqlx::{Row as _, SqlitePool};
use uuid::Uuid;

fn eligible(tool: &str, args: &Value) -> bool {
    let allowed = matches!(
        tool,
        "fs_replace_text" | "fs_apply_edits" | "fs_write_text" | "fs_create_directory"
    );
    if !allowed {
        return false;
    }
    let Some(path) = args.get("path").and_then(Value::as_str) else {
        return false;
    };
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    if lower.split('/').any(|part| {
        matches!(
            part,
            ".git" | ".ssh" | ".aws" | ".env" | ".config" | "secrets" | "credentials"
        ) || part.starts_with(".env.")
            || part.ends_with(".pem")
            || part.ends_with(".key")
            || part.ends_with(".pfx")
            || part.ends_with(".p12")
    }) {
        return false;
    }
    // An unversioned overwrite is higher-risk than an exact edit or new file.
    if tool == "fs_write_text"
        && args.get("overwrite").and_then(Value::as_bool) == Some(true)
        && args
            .get("expectedVersion")
            .and_then(Value::as_str)
            .is_none()
    {
        return false;
    }
    if tool == "fs_apply_edits"
        && args
            .get("expectedVersion")
            .and_then(Value::as_str)
            .is_none()
    {
        return false;
    }
    true
}

pub(crate) async fn consume(
    pool: &SqlitePool,
    context: &OperationContext,
    tool: &str,
    arguments: &Value,
) -> RuntimeResult<bool> {
    if !eligible(tool, arguments) {
        return Ok(false);
    }
    let Some(task) = context.task_id.as_deref() else {
        return Ok(false);
    };
    let Some(scope) = context
        .conversation_scope_id
        .as_deref()
        .filter(|s| !s.is_empty())
    else {
        return Ok(false);
    };
    let now = crate::runtime_host::now_ms();
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| RuntimeError::new("storage_error", "write trust transaction failed"))?;
    let result = sqlx::query(
        "UPDATE task_write_trust SET used_calls=used_calls+1,updated_at_ms=?
         WHERE task_id=? AND agent_id=? AND conversation_scope=?
           AND expires_at_ms>? AND used_calls<max_calls
           AND root=(SELECT project_folder FROM tasks WHERE id=? AND agent_id=?
             AND conversation_scope_hash=?)
           AND workspace_revision=(SELECT updated_at_ms FROM task_workspace_access
             WHERE task_id=? AND access_mode='readWrite')
           AND NOT EXISTS(SELECT 1 FROM subagent_runs WHERE child_task_id=?)
         RETURNING used_calls",
    )
    .bind(now)
    .bind(task)
    .bind(&context.agent_id)
    .bind(scope)
    .bind(now)
    .bind(task)
    .bind(&context.agent_id)
    .bind(scope)
    .bind(task)
    .bind(task)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| RuntimeError::new("storage_error", "write trust lookup failed"))?;
    let Some(row) = result else { return Ok(false) };
    let uses = row.get::<i64, _>("used_calls");
    // Durable audit without storing file content or secrets.
    let event_id = Uuid::new_v4().to_string();
    let payload = json!({"source":"taskWriteTrust","change":"writeAutoApproved",
        "tool":tool,"usedCalls":uses});
    sqlx::query("INSERT INTO timeline_events(event_id,task_id,turn_id,session_id,actor,kind,idempotency_key,payload_json,metadata_json,created_at_ms)
        VALUES(?,?,?,NULL,'system','status',?,?,NULL,?)")
        .bind(&event_id).bind(task).bind(context.turn_id.as_deref())
        .bind(&event_id).bind(payload.to_string()).bind(now)
        .execute(&mut *tx).await
        .map_err(|_|RuntimeError::new("storage_error","write trust audit failed"))?;
    tx.commit()
        .await
        .map_err(|_| RuntimeError::new("storage_error", "write trust commit failed"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_bounded_safe_edits_are_eligible() {
        assert!(eligible("fs_replace_text", &json!({"path":"src/main.rs"})));
        assert!(eligible(
            "fs_write_text",
            &json!({"path":"new.txt","overwrite":false})
        ));
        assert!(eligible(
            "fs_apply_edits",
            &json!({"path":"src/main.rs","expectedVersion":"v1"})
        ));
        assert!(!eligible("fs_apply_edits", &json!({"path":"src/main.rs"})));
        assert!(!eligible("fs_write_raw", &json!({"path":"a.bin"})));
        assert!(!eligible("fs_delete", &json!({"path":"a.txt"})));
        assert!(!eligible("git_commit", &json!({"path":"a.txt"})));
        assert!(!eligible("fs_write_text", &json!({"path":".git/config"})));
        assert!(!eligible(
            "fs_replace_text",
            &json!({"path":"E:\\repo\\.env"})
        ));
        assert!(!eligible(
            "fs_write_text",
            &json!({"path":"a.txt","overwrite":true})
        ));
    }
}
