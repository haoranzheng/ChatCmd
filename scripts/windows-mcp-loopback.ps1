<#
Windows-MCP least-privilege loopback launcher for ChatCMD.
Status is read-only; Start runs only the 7 GUI tools; persistent login
startup requires the explicit InstallAtLogin action, never implicit.
#>
param(
    [ValidateSet('Status','Start','InstallAtLogin','RemoveAtLogin')]
    [string]$Action = 'Status'
)
$ErrorActionPreference = 'Stop'
$taskName = 'ChatCMD-Windows-MCP-Limited'
$port = 8000
$arguments = 'serve --transport streamable-http --host 127.0.0.1 --port 8000 --tools Snapshot,Screenshot,Click,Type,Scroll,Shortcut,App'
$tool = Get-Command windows-mcp.exe -ErrorAction SilentlyContinue
if (-not $tool) { throw 'windows-mcp.exe is not installed or missing from PATH.' }
$program = $tool.Source
function Test-Loopback {
    $listener = Get-NetTCPConnection -LocalAddress '127.0.0.1' -LocalPort $port -State Listen -ErrorAction SilentlyContinue
    if (-not $listener) {
        Write-Host 'Windows-MCP listener: OFFLINE (127.0.0.1:8000)'
        return $false
    }
    $body='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"chatcmd-health","version":"1"}}}'
    try {
        $result=Invoke-WebRequest -Uri 'http://127.0.0.1:8000/mcp' -Method Post -Headers @{'Accept'='application/json, text/event-stream';'MCP-Protocol-Version'='2025-06-18'} -ContentType 'application/json' -Body $body -UseBasicParsing -TimeoutSec 6
        $ok=$result.StatusCode -eq 200 -and $result.Content.Contains('"name":"windows-mcp"')
        Write-Host ('Windows-MCP initialize: ' + $(if($ok){'OK'}else{'INVALID RESPONSE'}))
        return $ok
    } catch {
        Write-Warning ('Listener responded but MCP initialization failed: ' + $_.Exception.Message)
        return $false
    }
}
switch ($Action) {
    'Status' {
        $ok=Test-Loopback
        if (-not $ok) { exit 1 }
    }
    'Start' {
        if (Test-Loopback) { Write-Host 'Already running. No duplicate launched.'; break }
        # Use the installed launcher with an explicit tool allowlist.
        $proc=Start-Process -FilePath $program -ArgumentList $arguments -PassThru -WindowStyle Hidden
        Start-Sleep -Seconds 2
        if (-not (Test-Loopback)) { throw "Windows-MCP did not start. Process ID=$($proc.Id). Check its console error logs." }
        Write-Host "Started limited Windows-MCP PID=$($proc.Id)"
    }
    'InstallAtLogin' {
        if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) {
            throw "Scheduled task $taskName already exists; refusing to overwrite it."
        }
        $user=[System.Security.Principal.WindowsIdentity]::GetCurrent().Name
        $act=New-ScheduledTaskAction -Execute $program -Argument $arguments
        $trigger=New-ScheduledTaskTrigger -AtLogOn -User $user
        $principal=New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
        $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
        Register-ScheduledTask -TaskName $taskName -Action $act -Trigger $trigger -Principal $principal -Settings $settings | Out-Null
        Write-Host "Installed interactive-user login task: $taskName (limited to seven UI tools and loopback)."
        Write-Host 'No system-wide access or TCP 0.0.0.0 binding was configured.'
    }
    'RemoveAtLogin' {
        $task=Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
        if ($task) { Unregister-ScheduledTask -TaskName $taskName -Confirm:$false; Write-Host 'Login task removed.' }
        else { Write-Host 'No ChatCMD limited login task installed.' }
    }
}
