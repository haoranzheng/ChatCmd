# Windows x64 local preview build

This feature branch packages Windows x64 locally by default. Normal GitHub
pull-request checks still run Windows and Linux Rust tests, strict Clippy and
frontend checks. GitHub release packaging is opt-in via workflow_dispatch input
package_windows_preview=true.

Prepare a clean separate source checkout, not your installed ChatCMD directory:

    git clone --branch feature/workspace-binding-write-scope-recovery --single-branch https://github.com/haoranzheng/ChatCmd.git ChatCmd-preview-src
    cd ChatCmd-preview-src
    git rev-parse HEAD
    git status --short

Requirements: Windows x64, MSVC C++ build tools and Windows SDK, rustup with a
Windows MSVC toolchain and x86_64-pc-windows-msvc target, Git, Node.js 22+,
npm, and PowerShell.

Package without repeating the full CI (faster):

    powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-windows-local-preview.ps1

Optionally execute the local Rust and web tests:

    powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-windows-local-preview.ps1 -RunTests

Result is under release/local-preview/<version>/ and includes a zipped executable,
extension, build manifest and checksums. Check that the source SHA exactly
matches the green CI run before deployment. Extract into a fresh test directory;
do not replace the running binary or reuse the production SQLite database.
Carry out the Windows-MCP and workspace approval checks in
docs/windows-desktop-integration.md.

Repeat builds can use git pull --ff-only only in the clean preview source clone.
The script refuses dirty checkouts, performs no git reset/clean, does not install
or start ChatCMD and does not overwrite the existing installation.
