# Scoped workspace write trust (opt-in)

A verified local administrator can grant a single conversation a bounded text-edit
authorization to avoid per-operation confirmations for routine code edits.

**Default is OFF.** Open task **Access permissions → Trusted workspace edits**.
Requires local authenticated UI, a bound real conversation scope, a saved project
with `readWrite` file access, and a non-child task. Duration 15 or 60 minutes;
maximum 128 auto-approved operations. The trust is task-, agent-,
conversation-, workspace-root- and workspace-revision-bound. Changes to the
workspace binding or execution mode invalidate it; explicit revocation is immediate.

Only `fs_replace_text`, `fs_apply_edits` (with expectedVersion),
`fs_write_text` (unversioned overwrite remains approval-required), and
`fs_create_directory` may consume it. Hidden or sensitive paths such as
`.git`, `.env`, `.ssh`, keys and certificates remain approval-required.
The normal canonical workspace scope gate and tool allowlist run before trust
consumption. `Deny` always wins. No other task or subagent inherits trust.
File content is not logged; consumption records a local audit event.

High-impact operations including delete, move, copy, binary writes, shell,
Git commits, process control and desktop control retain separate approval rules.
Local trust is not an OS sandbox. A trusted text edit can still change executable
source code; only enable it for a conversation you trust.

The authenticated local API:
`GET/PUT/DELETE /api/local/tasks/{id}/write-trust`.
PUT input `{"durationMinutes":15|60}`. Extension and MCP clients cannot call
these local-management routes.
