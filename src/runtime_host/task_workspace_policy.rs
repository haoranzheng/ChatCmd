//! Independent scope gate for mutating filesystem/git operations.
//! Neither absolute tool arguments nor a user-message path can grant write authority.
use chatcmd_runtime::{OperationContext, RuntimeError, RuntimeResult};
use serde_json::Value;
use sqlx::Row as _;
use std::path::{Component, Path, PathBuf};

use super::RuntimeHost;

pub(in crate::runtime_host) fn is_workspace_write_tool(tool: &str) -> bool {
    matches!(
        tool,
        "fs_write_text"
            | "fs_write_raw"
            | "fs_replace_text"
            | "fs_apply_edits"
            | "fs_create_directory"
            | "fs_copy"
            | "fs_move"
            | "fs_delete"
            | "fs_restore_quarantine"
            | "fs_quarantine_gc"
            | "workspace_index_rebuild"
            | "git_commit"
    )
}

impl RuntimeHost {
    /// Extra roots are snapshot-bound to the exact directories approved by the local UI.
    /// A moved, removed, or reparse-point directory must fail closed.
    pub(crate) async fn additional_workspace_roots(
        &self,
        task_id: Option<&str>,
    ) -> RuntimeResult<Vec<PathBuf>> {
        let Some(task_id) = task_id else {
            return Ok(Vec::new());
        };
        let mode = sqlx::query_scalar::<_, String>(
            "SELECT access_mode FROM task_workspace_access WHERE task_id=?",
        )
        .bind(task_id)
        .fetch_optional(self.repository.pool())
        .await
        .map_err(|_| RuntimeError::new("storage_error", "workspace access lookup failed"))?;
        if !matches!(mode.as_deref(), Some("readOnly" | "readWrite")) {
            return Ok(Vec::new());
        }
        let rows = sqlx::query("SELECT r.authorized_root,p.path FROM task_workspace_extra_roots r LEFT JOIN workspace_projects p ON p.id=r.project_id WHERE r.task_id=? ORDER BY r.project_id")
            .bind(task_id).fetch_all(self.repository.pool()).await
            .map_err(|_| RuntimeError::new("storage_error", "additional workspace lookup failed"))?;
        let mut roots = Vec::with_capacity(rows.len());
        for row in rows {
            let path = row.get::<Option<String>, _>("path").ok_or_else(|| {
                RuntimeError::new(
                    "approval_scope_invalid",
                    "An additional workspace project was removed.",
                )
            })?;
            let expected = row.get::<String, _>("authorized_root");
            let root =
                crate::api::task_workspace::validate_workspace_root(&path).map_err(|_| {
                    RuntimeError::new(
                        "approval_scope_invalid",
                        "An additional workspace path is missing or unsafe.",
                    )
                })?;
            if root != Path::new(&expected) {
                return Err(RuntimeError::new(
                    "approval_scope_invalid",
                    "An additional project root changed; reauthorize it locally.",
                ));
            }
            roots.push(root);
        }
        Ok(roots)
    }

    pub(crate) async fn require_workspace_access(
        &self,
        context: &OperationContext,
        tool: &str,
        arguments: &Value,
    ) -> RuntimeResult<()> {
        let task_id = context.task_id.as_deref().ok_or_else(|| {
            RuntimeError::new(
                "project_folder_required",
                "A task identity is required to access a workspace.",
            )
        })?;
        let row = sqlx::query(
            "SELECT a.access_mode,a.project_id,t.project_folder,p.path AS project_path FROM task_workspace_access a JOIN tasks t ON t.id=a.task_id LEFT JOIN workspace_projects p ON p.id=a.project_id WHERE a.task_id=?"
        ).bind(task_id).fetch_optional(self.repository.pool()).await
            .map_err(|_| RuntimeError::new("storage_error", "workspace authorization lookup failed"))?;

        // Existing unbound read-only tasks retain the legacy explicit-message scope.
        // Bound tasks, however, are confined to the chosen project for reads AND writes.
        let write = is_workspace_write_tool(tool);
        let Some(row) = row else {
            return if write {
                Err(RuntimeError::new(
                    "permission_change_requires_user",
                    "Writing requires a workspace selected and authorized in the authenticated local ChatCMD UI.",
                ))
            } else {
                Ok(())
            };
        };
        let access_mode = row.get::<&str, _>("access_mode");
        if access_mode == "restricted" {
            return Err(RuntimeError::new(
                "policy_denied",
                "The local user has restricted this task's workspace access.",
            ));
        }
        if write && access_mode != "readWrite" {
            return Err(RuntimeError::new(
                "permission_change_requires_user",
                "This task has no local read-write workspace authorization. Bind a project and approve read-write access in ChatCMD.",
            ));
        }
        let project_path = row
            .get::<Option<String>, _>("project_path")
            .ok_or_else(|| {
                RuntimeError::new(
                    "approval_scope_invalid",
                    "The authorized workspace project has been deleted.",
                )
            })?;
        let root = std::fs::canonicalize(&project_path).map_err(|_| {
            RuntimeError::new(
                "approval_scope_invalid",
                "The authorized workspace directory is unavailable.",
            )
        })?;
        if !root.is_dir() || root.parent().is_none() {
            return Err(RuntimeError::new(
                "approval_scope_invalid",
                "The approved workspace root is invalid.",
            ));
        }
        let bound = row
            .get::<Option<String>, _>("project_folder")
            .ok_or_else(|| {
                RuntimeError::new(
                    "project_folder_required",
                    "Task workspace must be bound locally.",
                )
            })?;
        if root.as_path() != Path::new(&bound)
            || std::fs::canonicalize(&bound).ok().as_deref() != Some(root.as_path())
        {
            return Err(RuntimeError::new(
                "approval_scope_invalid",
                "The task project changed since authorization. Rebind it in the local UI.",
            ));
        }
        let mut allowed_roots = vec![root.clone()];
        allowed_roots.extend(self.additional_workspace_roots(Some(task_id)).await?);
        let targets = mutation_paths(tool, arguments, &root)?;
        if targets.is_empty() {
            return Err(RuntimeError::new(
                "invalid_arguments",
                "Filesystem operations require an explicit scoped path.",
            ));
        }
        for target in targets {
            if !allowed_roots
                .iter()
                .any(|allowed| validate_target_in_root(allowed, &target).is_ok())
            {
                return Err(outside_scope());
            }
        }
        Ok(())
    }
}

fn mutation_paths(tool: &str, args: &Value, root: &Path) -> RuntimeResult<Vec<PathBuf>> {
    // Workspace metadata and project rule reads must respect restricted tasks.
    // ProjectContextService separately validates every requested target against root.
    if matches!(tool, "workspace_roots" | "project_context") {
        return Ok(vec![root.to_path_buf()]);
    }
    if tool.starts_with("git_") {
        return Ok(vec![resolve_target(
            root,
            args.get("cwd").and_then(Value::as_str).unwrap_or("."),
        )?]);
    }
    let mut result = Vec::new();
    for field in ["path", "source", "destination", "quarantinePath"] {
        if let Some(raw) = args.get(field).and_then(Value::as_str) {
            result.push(resolve_target(root, raw)?);
        }
    }
    if let Some(paths) = args.get("paths").and_then(Value::as_array) {
        for path in paths {
            let raw = path.as_str().ok_or_else(|| {
                RuntimeError::new("invalid_arguments", "Expected a string workspace path.")
            })?;
            result.push(resolve_target(root, raw)?);
        }
    }
    if let Some(requests) = args.get("requests").and_then(Value::as_array) {
        for req in requests {
            if let Some(raw) = req.get("path").and_then(Value::as_str) {
                result.push(resolve_target(root, raw)?);
            }
        }
    }
    if result.is_empty() && tool == "workspace_index_status" {
        result.push(root.to_path_buf());
    }
    Ok(result)
}

fn resolve_target(root: &Path, raw: &str) -> RuntimeResult<PathBuf> {
    let path = Path::new(raw);
    if raw.is_empty() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(RuntimeError::new(
            "path_outside_allowed_scope",
            "Relative path traversal is forbidden for workspace writes.",
        ));
    }
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    })
}

fn validate_target_in_root(root: &Path, target: &Path) -> RuntimeResult<()> {
    let mut ancestor = target.to_path_buf();
    loop {
        // Path::exists follows symlinks and returns false for a dangling link.
        // That would incorrectly treat a symlink pointing to a not-yet-created
        // file outside the workspace as an ordinary missing destination.
        match std::fs::symlink_metadata(&ancestor) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(outside_scope());
                }
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !ancestor.pop() {
                    return Err(outside_scope());
                }
            }
            Err(_) => return Err(outside_scope()),
        }
    }
    let resolved = ancestor.canonicalize().map_err(|_| outside_scope())?;
    if resolved != root && !resolved.starts_with(root) {
        return Err(outside_scope());
    }
    // For existing files this compares the full resolved file location.
    // For newly created entries this checks the closest existing ancestor.
    Ok(())
}

fn outside_scope() -> RuntimeError {
    RuntimeError::new(
        "path_outside_allowed_scope",
        "Write target is outside the locally approved workspace project.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outside_and_parent_traversal_are_rejected() {
        let a = tempfile::tempdir().expect("A");
        let b = tempfile::tempdir().expect("B");
        let root = a.path().canonicalize().expect("A root");
        assert!(validate_target_in_root(&root, &root.join("child.txt")).is_ok());
        assert_eq!(
            validate_target_in_root(&root, &b.path().join("other.txt"))
                .unwrap_err()
                .code,
            "path_outside_allowed_scope"
        );
        assert_eq!(
            mutation_paths("fs_replace_text", &json!({"path":"../outside"}), &root)
                .unwrap_err()
                .code,
            "path_outside_allowed_scope"
        );
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_pointing_outside_root_is_rejected() {
        let a = tempfile::tempdir().expect("A");
        let b = tempfile::tempdir().expect("B");
        let destination = b.path().join("not-created-yet.txt");
        let link = a.path().join("external-file");
        std::os::unix::fs::symlink(&destination, &link).expect("symlink");
        assert!(
            !link.exists(),
            "a dangling symlink must not appear to exist"
        );
        let root = a.path().canonicalize().expect("root");
        assert_eq!(
            validate_target_in_root(&root, &link).unwrap_err().code,
            "path_outside_allowed_scope"
        );
    }

    #[cfg(unix)]
    #[test]
    fn dangling_directory_symlink_pointing_outside_root_is_rejected() {
        let a = tempfile::tempdir().expect("A");
        let b = tempfile::tempdir().expect("B");
        let directory = b.path().join("not-created-yet");
        let link = a.path().join("external-dir");
        std::os::unix::fs::symlink(&directory, &link).expect("symlink");
        let root = a.path().canonicalize().expect("root");
        assert_eq!(
            validate_target_in_root(&root, &link.join("new.txt"))
                .unwrap_err()
                .code,
            "path_outside_allowed_scope"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_outside_root_is_rejected() {
        let a = tempfile::tempdir().expect("A");
        let b = tempfile::tempdir().expect("B");
        let link = a.path().join("external");
        std::os::unix::fs::symlink(b.path(), &link).expect("link");
        let root = a.path().canonicalize().expect("root");
        assert_eq!(
            validate_target_in_root(&root, &link.join("other.txt"))
                .unwrap_err()
                .code,
            "path_outside_allowed_scope"
        );
    }
}
