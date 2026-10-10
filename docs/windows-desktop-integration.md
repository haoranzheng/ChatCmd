# Windows Desktop Integration v0.1

This optional bridge connects ChatCMD (MCP server) to a separately installed CursorTouch/Windows-MCP (MCP server). ChatCMD is the upstream client, not a remote desktop process.

## Local setup

> Upstream installation note (checked October 2026): the current Windows-MCP
> `pyproject.toml` requires Python 3.14 or newer even though its README may
> still mention 3.13. Prefer `uvx windows-mcp`, which resolves its own Python
> environment. Do not bind the HTTP service to `0.0.0.0`.
1. Install Windows-MCP independently, following https://github.com/CursorTouch/Windows-MCP.
2. Start Windows-MCP as a loopback Streamable HTTP service:
   `uvx windows-mcp serve --transport streamable-http --host 127.0.0.1 --port 8000 --tools "Snapshot,Screenshot,Click,Type,Scroll,Shortcut,App"`
3. In the authenticated ChatCMD management UI, open **Settings → Security → Windows Desktop Integration v0.1**, set local port 8000, enable and save, then **Test Windows-MCP connection**.
4. In **Plugin list**, explicitly add the two new `desktop_observe` and/or `desktop_control` tools to the chosen access profile; neither is added to the Safe local tools preset automatically.
5. In ChatGPT, reconnect ChatCMD to refresh the tool catalog and choose a task. Each desktop call must be locally approved.

The bridge never accepts an arbitrary upstream URL: it always connects to `http://127.0.0.1:<port>/mcp`, with redirects and proxies disabled. Disable the integration to immediately reject new calls.

## v0.1 exposed MCP tools
* `desktop_observe` with optional `kind` = `snapshot` (UI tree without image) or `screenshot` (image). Screenshots may include credentials and other sensitive screen content.
* `desktop_control` with `action` = `click`, `type`, `scroll`, `shortcut`, `switch_window`. Its arguments are strictly mapped onto upstream `Click`, `Type`, `Scroll`, `Shortcut`, or `App(mode="switch")` respectively. It never forwards arbitrary tool names or arbitrary argument dictionaries.

Each call requires individual approval even when ChatCMD is in Allow all mode. Tool allowlist, individual approvals and Windows-MCP's own desktop input ownership controls are separate mechanisms. A GUI can still launch apps or make external changes via shortcuts and UI interactions; this is not an OS sandbox and *does not* guarantee filesystem isolation. Inspect requests carefully.

This build has no managed install/start/upgrade process for Windows-MCP, no public remote endpoint support, no STDIO/SSE transport, and no unattended desktop access. Screenshot and Snapshot results are converted from upstream text/image content to MCP content blocks so a supporting ChatGPT client can render images. Image data is bounded in size; unusually large captures may be rejected. Client-side rendering still depends on the ChatGPT MCP interface. Tested CI does not replace a live Windows GUI manual acceptance test.

The management endpoints `/api/local/desktop/config` (GET/PUT) and `/api/local/desktop/status` (GET) are authenticated GUI-only routes and are not exposed to the ChatGPT extension or public MCP clients.

## Windows manual acceptance gate (preview build)

Use a **separate preview directory** for the Actions artifact. Back up the
existing ChatCMD executable, local configuration, and database/state storage
before testing. Never unpack over the running installation. If the preview
shares a persisted state directory, work from a copied state or stop and
restore the backup before returning to the installed version.

1. **Startup:** run the preview from its own directory and verify that the
   local management login works; check that existing user data is not silently
   reset, and that the version shown matches the preview package.
2. **Multiple same-drive folders:** create two saved projects with separate
   folders on the same drive, bind both to one task, and confirm the primary
   folder remains the relative-path root. Grant read-write only after checking
   both displayed paths.
3. **Scope:** read a file in each authorized folder; attempt a managed write
   inside each folder and verify individual approval. Attempt writes into
   the drive root, a sibling directory, and a `..` traversal path: they must
   be denied. Confirm a task with Restricted access cannot retrieve
   `project_context` or `workspace_roots`.
4. **Revocation:** change the task from Read-write to Read-only/Restricted and
   verify that previously available managed writes are denied, including
   after reconnecting ChatGPT. Verify changes to a saved project path require
   a fresh binding.
5. **Windows-MCP:** with the upstream process running, enable the bridge
   from local Security settings, test the connection, then request a
   `desktop_observe` Snapshot. Confirm there is an explicit local approval.
   Test Screenshot only on a screen without passwords or private content.
   Request a harmless `desktop_control` action and deny the approval first;
   verify the denied action has no effect. Approve a second harmless action
   and verify it completes.
6. **Shutdown and rollback:** disable the bridge, confirm further desktop
   calls are rejected, stop the preview, and launch the existing ChatCMD
   installation. Restore the backed-up state if the preview changed it.

Record the preview SHA256 (from `SHA256SUMS.txt`), pass/fail for each
checkpoint, and any ChatCMD/Windows-MCP logs. **Do not merge or use as a
replacement production build until the CI and this human acceptance pass.**
