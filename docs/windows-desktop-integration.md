# Windows Desktop Integration v0.1

This optional bridge connects ChatCMD (MCP server) to a separately installed CursorTouch/Windows-MCP (MCP server). ChatCMD is the upstream client, not a remote desktop process.

## Local setup
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

This build has no managed install/start/upgrade process for Windows-MCP, no public remote endpoint support, no STDIO/SSE transport, and no unattended desktop access. For screenshots, the current bridge returns MCP tool content including image data as structured JSON; dedicated image rendering in the ChatGPT UI is a later enhancement. Tested CI does not replace a live Windows GUI manual acceptance test.

The management endpoints `/api/local/desktop/config` (GET/PUT) and `/api/local/desktop/status` (GET) are authenticated GUI-only routes and are not exposed to the ChatGPT extension or public MCP clients.
