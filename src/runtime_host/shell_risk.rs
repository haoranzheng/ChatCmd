//! Advisory, fail-conservative screening for obvious risky shell requests.
//! This is *not* a parser, shell sandbox, or guarantee that safe-looking commands
//! have no side effects. Approval digests still bind the full exact arguments.
use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct Inspection {
    pub high_risk: bool,
    pub reason: &'static str,
    pub preview: String,
    pub preview_truncated: bool,
    pub preview_redacted: bool,
}

fn basename(executable: &str) -> String {
    executable
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(executable)
        .to_ascii_lowercase()
}

fn shell_name(name: &str) -> bool {
    matches!(
        name,
        "cmd.exe"
            | "cmd"
            | "powershell.exe"
            | "powershell"
            | "pwsh.exe"
            | "pwsh"
            | "bash.exe"
            | "bash"
            | "sh"
            | "sh.exe"
            | "zsh"
            | "zsh.exe"
    )
}

fn is_opaque_script(name: &str, args: &[&str]) -> bool {
    let extensions = [".ps1", ".bat", ".cmd", ".sh", ".py", ".js", ".vbs"];
    if extensions.iter().any(|ext| name.ends_with(ext)) {
        return true;
    }
    if args.iter().any(|arg| {
        let arg = arg.to_ascii_lowercase();
        matches!(
            arg.as_str(),
            "-encodedcommand" | "-enc" | "-ecommand" | "-file" | "-f"
        ) || (matches!(name, "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe") && arg == "-e")
    }) {
        return true;
    }
    // Loading code from a local file cannot be inspected from argv alone.
    if matches!(
        name,
        "python" | "python.exe" | "node" | "node.exe" | "ruby" | "ruby.exe"
    ) && (args.first().is_some_and(|arg| !arg.starts_with('-'))
        || args
            .iter()
            .any(|arg| matches!(*arg, "-c" | "-e" | "--eval")))
    {
        return true;
    }
    false
}

fn danger_reason(command: &str) -> Option<&'static str> {
    let normalized = command
        .to_ascii_lowercase()
        .replace(['\r', '\n', '\t'], " ");
    let words = normalized.split_whitespace().collect::<Vec<_>>();
    let includes = |needle: &str| normalized.contains(needle);
    if [
        "remove-item",
        "remove-variable",
        "clear-content",
        "clear-item",
        "delete-item",
        "rmdir /s",
        "rd /s",
        "del /f",
        "erase /f",
        "rm -rf",
        "rm -fr",
        "rm -r ",
        "shred ",
        "git clean -f",
        "git commit",
        "git push",
        "move-item",
        "copy-item",
        "set-content",
        "out-file",
        "move /y",
        "xcopy /y",
        "cp -f",
        "mv -f",
        "sed -i",
        "truncate ",
        "git reset --hard",
        "git push --force",
        "git push -f",
        "git push --delete",
        "robocopy /mir",
        "robocopy /purge",
        "reg delete",
        "diskpart",
        "format.com",
        "format.exe",
        "format ",
        "stop-process -force",
        "taskkill /f",
        "shutdown /s",
        "shutdown /r",
        "bcdedit /delete",
        "cipher /w",
    ]
    .iter()
    .any(|pattern| includes(pattern))
    {
        return Some("Recognized potentially destructive command or overwrite");
    }
    if words.iter().any(|w| {
        matches!(
            *w,
            "rm" | "rmdir"
                | "del"
                | "erase"
                | "format"
                | "diskpart"
                | "mkfs"
                | "dd"
                | "shred"
                | "clear-content"
                | "remove-item"
        )
    }) || includes("invoke-expression")
        || includes("iex(")
        || includes("iex ")
        || includes("downloadstring")
        || includes("encodedcommand")
    {
        return Some("Shell deletion, dynamic execution, or disk operation detected");
    }
    None
}

fn looks_sensitive(args: &[&str]) -> bool {
    const FLAGS: &[&str] = &[
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "authorization",
        "bearer ",
        "private_key",
        "client_secret",
        "access_key",
        "credential",
        "sk-proj-",
        "ghp_",
        "github_pat_",
    ];
    args.iter().any(|arg| {
        let value = arg.to_ascii_lowercase();
        FLAGS.iter().any(|flag| value.contains(flag))
    })
}

pub(crate) fn inspect(tool: &str, arguments: &Value) -> Option<Inspection> {
    if !matches!(tool, "command_run" | "shell_create" | "shell_write") {
        return None;
    }
    let executable = if tool == "shell_write" {
        "<active terminal>"
    } else {
        arguments
            .get("executable")
            .and_then(Value::as_str)
            .unwrap_or("<default shell>")
    };
    let argv = arguments
        .get("arguments")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    let name = basename(executable);
    let joined = if tool == "shell_write" {
        arguments
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    } else {
        argv.join(" ")
    };
    let dynamic = danger_reason(&format!("{name} {joined}"));
    let pasted_script = tool == "shell_write"
        && arguments.get("inputKind").and_then(Value::as_str) == Some("paste")
        && joined.lines().count() > 1;
    let opaque = (tool != "shell_write" && is_opaque_script(&name, &argv)) || pasted_script;
    // Interactive shell sessions can accept *later* commands. Explain this
    // explicitly instead of asserting that the initial argv is safe.
    let interactive = tool == "shell_create" && shell_name(&name) && argv.is_empty();
    let high_risk = dynamic.is_some() || opaque || interactive;
    let reason = if let Some(reason) = dynamic {
        reason
    } else if opaque {
        "Script or encoded command contents cannot be inspected from arguments"
    } else if interactive {
        "Interactive shell can execute commands that are not yet visible"
    } else {
        "No recognized destructive pattern; command effects are not guaranteed safe"
    };
    let preview_redacted = arguments.get("sensitive").and_then(Value::as_bool) == Some(true)
        || looks_sensitive(&argv)
        || looks_sensitive(&[joined.as_str()]);
    let text = if preview_redacted {
        format!("{executable} [arguments hidden: potentially sensitive values]")
    } else {
        format!("{executable} {joined}")
    };
    // Avoid multiline control characters and cap the amount stored in history.
    let normalized = text
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>();
    let preview_truncated = normalized.chars().count() > 480;
    let preview = normalized.chars().take(480).collect::<String>();
    Some(Inspection {
        high_risk,
        reason,
        preview,
        preview_truncated,
        preview_redacted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn check(exe: &str, args: &[&str]) -> Inspection {
        inspect("command_run", &json!({"executable":exe,"arguments":args})).unwrap()
    }
    #[test]
    fn flags_obvious_destructive_commands_and_opaque_scripts() {
        for (exe, args) in [
            (
                "powershell.exe",
                vec!["-NoProfile", "-Command", "Remove-Item -Recurse C:\\work"],
            ),
            ("cmd.exe", vec!["/c", "rmdir /s /q C:\\work"]),
            ("bash", vec!["-c", "rm -rf ./dist"]),
            ("git", vec!["clean", "-fdx"]),
            ("git", vec!["reset", "--hard"]),
            ("git", vec!["commit", "-m", "change"]),
            ("powershell.exe", vec!["-Command", "Move-Item a b"]),
            ("python.exe", vec!["-c", "print('x')"]),
            ("robocopy", vec!["source", "dest", "/mir"]),
            ("powershell.exe", vec!["-EncodedCommand", "AAE="]),
            ("pwsh.exe", vec!["-File", "script.ps1"]),
        ] {
            // "git clean -fdx" intentionally triggers as "git clean -f..." prefix.
            assert!(check(exe, &args).high_risk, "{exe} {args:?}");
        }
    }
    #[test]
    fn safe_looking_command_is_not_claimed_guaranteed_safe() {
        let result = check("git", &["status", "--short"]);
        assert!(!result.high_risk);
        assert!(result.reason.contains("not guaranteed"));
    }
    #[test]
    fn secrets_are_not_persisted_in_previews() {
        let result = check(
            "powershell.exe",
            &["-Command", "Remove-Item ./x; --token super-secret"],
        );
        assert!(result.high_risk);
        assert!(result.preview_redacted);
        assert!(!result.preview.contains("super-secret"));
    }
    #[test]
    fn command_preview_is_bounded_and_control_free() {
        let result = check("cmd.exe", &["/c", &"x".repeat(1000)]);
        assert!(result.preview_truncated);
        assert_eq!(result.preview.chars().count(), 480);
    }
    #[test]
    fn terminal_input_is_screened_for_destructive_commands_and_paste() {
        let dangerous = inspect("shell_write", &json!({"text":"Remove-Item C:\\Temp\\x"})).unwrap();
        assert!(dangerous.high_risk);
        let paste = inspect(
            "shell_write",
            &json!({"text":"echo first\necho second","inputKind":"paste"}),
        )
        .unwrap();
        assert!(paste.high_risk);
        let benign = inspect("shell_write", &json!({"text":"echo hello"})).unwrap();
        assert!(!benign.high_risk);
        let sensitive = inspect(
            "shell_write",
            &json!({"text":"private-password","sensitive":true}),
        )
        .unwrap();
        assert!(sensitive.preview_redacted);
        assert!(!sensitive.preview.contains("private-password"));
    }
    #[test]
    fn interactive_shell_launch_requires_review() {
        let result = inspect(
            "shell_create",
            &json!({"executable":"powershell.exe","arguments":[]}),
        )
        .unwrap();
        assert!(result.high_risk);
    }
}
