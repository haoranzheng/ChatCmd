# Isolated, incremental x64 Windows package. Never installs or starts ChatCMD.
param([switch]$RunTests)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = Split-Path -Parent $PSScriptRoot
$branchRequired = 'feature/workspace-binding-write-scope-recovery'
$target = 'x86_64-pc-windows-msvc'

function Invoke-Checked([string]$Label, [scriptblock]$Action) {
    Write-Host "[local-build] $Label" -ForegroundColor Cyan
    & $Action
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed (exit $LASTEXITCODE)"
    }
}

if ($env:OS -ne 'Windows_NT') { throw 'Windows is required' }
foreach ($exe in @('git','node','npm','cargo','rustc','rustup')) {
    if (-not (Get-Command $exe -ErrorAction SilentlyContinue)) {
        throw "Missing command: $exe"
    }
}
foreach ($relative in @('Cargo.toml','web/package-lock.json','chatgpt-extension')) {
    if (-not (Test-Path (Join-Path $root $relative))) {
        throw "Not a full source checkout: missing $relative"
    }
}
Push-Location $root
try {
    Invoke-Checked 'Verify Git working tree' { git rev-parse --is-inside-work-tree }
    $actualBranch = (& git branch --show-current).Trim()
    if ($LASTEXITCODE -ne 0 -or $actualBranch -ne $branchRequired) {
        throw "Checkout must be on $branchRequired. Actual: $actualBranch"
    }
    $status = @(git status --porcelain --untracked-files=normal)
    if ($LASTEXITCODE -ne 0 -or $status.Count -gt 0) {
        throw 'Dirty source tree: use a separate clean clone; do not reset local changes.'
    }
    $sha = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $sha -notmatch '^[0-9a-f]{40}$') {
        throw 'Cannot determine source commit SHA'
    }
    $hostTriple = @(& rustc -vV | Where-Object { $_ -match '^host:' })
    if ($LASTEXITCODE -ne 0 -or $hostTriple.Count -ne 1 -or $hostTriple[0] -notmatch 'windows-msvc$') {
        throw 'A Rust Windows MSVC toolchain and Visual Studio C++ Build Tools are required.'
    }
    $installed = @(rustup target list --installed)
    if ($LASTEXITCODE -ne 0 -or $target -notin $installed) {
        throw "Missing target. Run: rustup target add $target"
    }
    $nodeVersion = (& node --version).Trim()
    if ($LASTEXITCODE -ne 0 -or [int](($nodeVersion -replace '^v','').Split('.')[0]) -lt 22) {
        throw "Node 22 or newer is required. Current: $nodeVersion"
    }
    $utc = (Get-Date).ToUniversalTime().ToString('yy.MM.dd.HHmmss')
    $short = $sha.Substring(0,8)
    $version = "$utc-local-$short"
    $env:CHATCMD_BUILD_VERSION = $version
    $outputDir = Join-Path $root "release/local-preview/$version"
    $stage = Join-Path $outputDir 'stage'
    $zipPath = Join-Path $outputDir "ChatCMD-windows-x64-$version.zip"
    if (Test-Path $outputDir) { throw "Output already exists: $outputDir" }

    if ($RunTests) {
        Invoke-Checked 'Rust formatting' { cargo fmt --all -- --check }
        Invoke-Checked 'Rust workspace tests' { cargo test --workspace --no-fail-fast }
    } else {
        Write-Warning 'Build only: matching SHA must pass GitHub CI before deployment.'
    }
    $web = Join-Path $root 'web'
    $marker = Join-Path $web 'node_modules/.package-lock.json'
    $lock = Join-Path $web 'package-lock.json'
    $packageJson = Join-Path $web 'package.json'
    $needsNpmInstall = -not (Test-Path $marker)
    if (-not $needsNpmInstall) {
        $markerUtc = (Get-Item $marker).LastWriteTimeUtc
        $needsNpmInstall = ((Get-Item $lock).LastWriteTimeUtc -gt $markerUtc) -or
            ((Get-Item $packageJson).LastWriteTimeUtc -gt $markerUtc)
    }
    if ($needsNpmInstall) {
        Invoke-Checked 'Install pinned frontend dependencies' {
            npm ci --prefix web --prefer-offline --no-audit --no-fund
        }
    }
    if ($RunTests) {
        Invoke-Checked 'Frontend lint' { npm run lint --prefix web }
        Invoke-Checked 'Frontend tests' { npm test --prefix web -- --run }
    }
    Invoke-Checked 'Frontend build' { npm run build --prefix web }
    $sourceMaps = @(Get-ChildItem (Join-Path $web 'dist') -Filter '*.map' -Recurse -File)
    if ($sourceMaps.Count -ne 0) { throw 'Unexpected source map in packaged frontend' }

    Invoke-Checked 'Incremental Rust MSVC x64 Release build' {
        cargo build --release --features embedded-web --target $target --bin chat-cmd-client
    }
    $exePath = Join-Path $root "target/$target/release/chat-cmd-client.exe"
    if (-not (Test-Path $exePath)) { throw "Missing compiled executable: $exePath" }

    New-Item -ItemType Directory -Path $stage -Force | Out-Null
    Copy-Item -LiteralPath $exePath -Destination (Join-Path $stage 'ChatCMD.exe')
    Copy-Item -LiteralPath (Join-Path $root 'chatgpt-extension') -Destination (Join-Path $stage 'chatgpt-extension') -Recurse
    Set-Content -LiteralPath (Join-Path $stage 'chatcmd-version.txt') -Value $version -Encoding Ascii
    $exeHash = (Get-FileHash (Join-Path $stage 'ChatCMD.exe') -Algorithm SHA256).Hash
    Set-Content -LiteralPath (Join-Path $stage 'SHA256SUMS.txt') -Value "$exeHash  ChatCMD.exe" -Encoding Ascii

    [ordered]@{
        repository = 'haoranzheng/ChatCmd'
        branch = $branchRequired
        commit = $sha
        version = $version
        architecture = $target
        builtUtc = (Get-Date).ToUniversalTime().ToString('o')
        ranLocalTests = [bool]$RunTests
        note = 'Isolated preview; not installed; live Windows-MCP acceptance still required.'
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $stage 'build-manifest.json') -Encoding UTF8

    Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zipPath -CompressionLevel Optimal
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
    try {
        $names = @($archive.Entries | ForEach-Object { $_.FullName })
        foreach ($name in @('ChatCMD.exe','chatcmd-version.txt','SHA256SUMS.txt','build-manifest.json')) {
            if ($name -notin $names) { throw "Broken archive: missing $name" }
        }
    } finally {
        $archive.Dispose()
    }
    $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash
    Set-Content -LiteralPath "$zipPath.sha256" -Value "$zipHash  $(Split-Path $zipPath -Leaf)" -Encoding Ascii
    Write-Host 'Local preview packaged, existing installed ChatCMD untouched.' -ForegroundColor Green
    Write-Host "PACKAGE=$zipPath"
    Write-Host "SOURCE_SHA=$sha"
    Write-Host "ZIP_SHA256=$zipHash"
} finally {
    Pop-Location
}
