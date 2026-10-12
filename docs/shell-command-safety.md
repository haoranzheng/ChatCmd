# Shell command approval hardening (candidate)

This version performs bounded, *advisory* screening of `command_run`, `shell_create` and `shell_write` before dispatch. It is **not** an OS sandbox, a complete shell parser or a promise that unflagged code is safe. Arbitrary interpreters, shell quoting, aliases, downloaded code and scripts can conceal side effects.

## What is different

- Under task execution mode **Allow**, a recognizable destructive or mutating shell command cannot bypass the approval queue. Common patterns include `Remove-Item`, recursive `rm`/`del`/`rmdir`, `git clean`/`reset --hard`/`commit`/`push`, file move/copy/write operations, `robocopy /mir`, disk tools, forceful process shutdown and interpreter script/eval execution. Interactive shell launches and multi-line terminal paste are also marked risky.
- For those requests, the server stores `highRisk: true` in the *one-use approval request* together with the full-argument operation digest. The local GUI displays a warning, command preview and risk explanation. The approval API requires explicit `highRiskAcknowledged=true`; 'Allow similar' is rejected even if manually submitted.
- Preview is capped at 480 characters, normalized to one line and hides *all* arguments when suspicious credential strings or `sensitive:true` are detected. Only the command preview, not full shell input, goes into the approval summary. Review the original request when a preview is abbreviated.
- Benign-looking commands retain the existing execution-mode behavior and do not create extra prompts solely due to this classifier. An unflagged command is **not verified safe**. Existing workspace/file-write/desktop boundaries remain independent and win over task mode.

## Known limitations and next security work

- PowerShell, cmd, Bash, Python and Node can construct or download destructive commands dynamically. No regex/substring classifier can close that gap. A strong isolation boundary requires an OS-level low-privilege process/container, explicit filesystem allowlists and denied network/registry access as appropriate.
- Inspecting shell text is not a substitute for inspecting the contents of a referenced script file; encoded and file-based scripts are conservatively flagged instead.
- Path and file count in the approval UI are advisory only; command scripts can affect resources outside the originally displayed working directory unless sandboxed.
- Tokens without recognizable credential indicators may appear in the preview. The local UI must be treated as potentially sensitive.

The candidate must pass Rust/Windows/Linux CI and isolated GUI testing before replacing a running installation. No system service or auto-start is enabled by these changes.
