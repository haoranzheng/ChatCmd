# ChatCMD 简体中文

<p align="center">
  <img src="assets/icons/logo-transparent-master-1254.png" alt="ChatCMD logo" title="ChatCMD" width="100">
</p>

<p align="center">
  通过 Model Context Protocol（MCP），让网页端 AI 在你的电脑上以受控方式执行本地工作。
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="README_vi.md">Tiếng Việt</a> ·
  <strong>简体中文</strong>
</p>

> [!CAUTION]
> ChatCMD 可以把终端、文件、Git 仓库和本地进程能力暴露给 AI 客户端。首次使用时建议只开放必要工具，并保持审批模式开启。不要公开包含 Token 的 MCP URL。

ChatCMD 是一个本地优先的 MCP 桥接程序，用于连接支持 MCP 的 AI 客户端与个人电脑。项目由 Rust 服务端、带权限边界的本地运行时、SQLite 持久化、React 管理界面，以及可选的 Chromium ChatGPT 浏览器扩展组成。

核心程序运行在你的电脑上，不依赖 ChatCMD 账号、订阅、支付、额度或托管认证服务。部分可选功能仍会主动访问外部网络，例如 ChatGPT、用于安装 Skill 的 Git 仓库、Google Fonts，或你自行配置的 Tunnel 地址。

当前汉化分支支持 **English / Tiếng Việt / 简体中文**。中文浏览器环境（如 `zh-CN`、`zh-SG`、`zh-Hans`）会自动选择简体中文，也可以在 **设置 → 显示 → 语言** 中手动切换。

## 下载最新版本

官方预编译包由上游项目提供：

<p>
  <a href="https://github.com/int04/ChatCmd/releases/latest/download/ChatCMD-windows-x64.zip"><img alt="下载 ChatCMD Windows 64 位版" src="https://img.shields.io/badge/Download-Windows%2064--bit-0078d4?style=for-the-badge&amp;logo=windows11&amp;logoColor=white"></a>
  <a href="https://github.com/int04/ChatCmd/releases/latest/download/ChatCMD-windows-x86.zip"><img alt="下载 ChatCMD Windows 32 位版" src="https://img.shields.io/badge/Download-Windows%2032--bit-0078d4?style=for-the-badge&amp;logo=windows11&amp;logoColor=white"></a>
</p>
<p>
  <a href="https://github.com/int04/ChatCmd/releases/latest/download/ChatCMD-macos-apple-silicon.zip"><img alt="下载 ChatCMD macOS Apple Silicon 版" src="https://img.shields.io/badge/Download-macOS%20Apple%20Silicon-000000?style=for-the-badge&amp;logo=apple&amp;logoColor=white"></a>
  <a href="https://github.com/int04/ChatCmd/releases/latest/download/ChatCMD-macos-intel.zip"><img alt="下载 ChatCMD macOS Intel 版" src="https://img.shields.io/badge/Download-macOS%20Intel-000000?style=for-the-badge&amp;logo=apple&amp;logoColor=white"></a>
</p>

如果需要当前汉化分支，请从源码构建，或基于此 fork 自行制作 Release。

## 为什么使用 ChatCMD

- **本地优先**：服务端、管理界面、任务历史、设置和 SQLite 数据库都保存在本机。
- **MCP 访问配置**：可以为不同 AI 客户端创建独立配置，分别授予工具权限、暂停访问和轮换密钥。
- **真实本地工具**：支持持久终端、受限文件操作、Git 操作、进程检查、任务产物与 Skill。
- **实时监督**：可以查看 Agent 进度、工具调用、文件变化、终端输出、子 Agent、审批请求和最终回复。
- **ChatGPT 网页桥接**：可选 Chromium 扩展，可通过已登录的 ChatGPT 网页会话发送、继续、排队或停止任务。
- **跨平台**：支持 Windows、macOS 和 Linux 的开发/运行环境。
- **协议开放**：使用 MCP Streamable HTTP 和本地 API，不依赖专有云控制平面。

## 主要功能

### MCP 与权限

- Token 化的 Streamable HTTP 端点，例如 `http://127.0.0.1:8080/mcp/<token>`。
- 可为不同 AI 客户端或不同工作创建独立访问配置。
- 支持按工具白名单、权限分组和权限预设。
- 可以启用、禁用、编辑、删除和轮换访问配置。
- 支持用户自行管理公网域名、反向代理、IP 地址和 Tunnel，并在保存前进行连通性测试。

### 本地工具

| 分组 | 能力 |
| --- | --- |
| 设备 | 列出并检查本地执行设备。 |
| 终端 | 创建、写入、等待、读取、发送信号、调整尺寸、列出、检查和关闭持久 PTY 会话。 |
| 文件与工作区 | 查找根目录；列出、搜索、读取、创建、替换、写入、检查、复制、移动和删除文件/目录。 |
| Git | 查看状态、Diff、日志、分支、Revision，并通过受控接口创建 Commit。 |
| 进程 | 列出、检查并终止本地进程或进程树。 |
| Skills | 从 `.agents` 和 `.codex` 中发现并读取项目级或用户级 Skill。 |
| 任务与编排 | 跟踪用户回合、进度、执行模式、产物、规划问题、子 Agent、等待与完成状态。 |

完整 MCP 方法参考见 [docs/mcp_method.md](docs/mcp_method.md)。

### 子 Agent

ChatCMD 可以把较大的任务拆分给多个子 Agent。每个子 Agent 都有独立子任务，但仍关联到父任务和根回合，因此可以在一个工作流中统一监督。

主要机制包括：

1. 父 Agent 创建或复用子任务，并可限制允许的文件、操作、依赖和验收条件。
2. 子 Agent 与父 Agent 共用全局并发预算，避免无限并发。
3. 子 Agent 不会自动获得无限权限；正常工具授权和审批规则仍然有效。
4. 支持父 → 子 → 孙级委派，并避免因并发槽位耗尽导致永久等待。
5. ChatCMD 会跟踪心跳、租约、超时与异常退出。
6. 子 Agent 的最终报告持久化到 SQLite，由父 Agent 负责整合和验证。

更多细节见 [docs/subagent-reports.md](docs/subagent-reports.md) 与 [docs/subagent-approval-grants.md](docs/subagent-approval-grants.md)。

### ChatGPT 消息队列

当 ChatGPT 仍在处理当前回复时，可以提前准备下一条指令：

- **加入队列**：等当前对话真正空闲后自动发送。
- **立即发送**：允许 AI 在同一对话的下一次 MCP 调用中尽快收到消息；如果当前回合先结束，则自动退回普通队列。
- 队列消息支持调整顺序、编辑、删除、提升为立即发送或恢复为普通排队。

### 压缩上下文并继续

长对话接近上下文限制时，可以使用 **“立即压缩上下文并继续”**：

1. 先确认是否需要在新对话中自动继续工作。
2. 在安全边界冻结当前任务。
3. 让当前 ChatGPT 对话生成结构化交接内容并持久化到 SQLite。
4. 打开新的 ChatGPT 对话并发送恢复上下文。
5. 保留原 ChatCMD 任务 ID、项目、权限、时间线、草稿和消息队列。
6. 如果用户选择继续，则在新对话绑定完成后发送后续工作请求。
7. 只有在能确认原始标签页身份时才会尝试关闭旧标签页。

## 管理界面

管理界面用于：

- 查看运行时健康状态与最近活动；
- 创建和管理 MCP/Plugin 访问配置；
- 设置公网地址与 Tunnel；
- 查看任务、终端和会话；
- 安装与管理 Skills；
- 配置执行审批、并发、工作区根目录、主题、字体、语言与声音；
- 查看数据库和诊断日志；
- 管理 ChatGPT 浏览器桥接。

### 简体中文

本汉化分支新增：

- `zh-CN` 应用语言；
- 1011 条完整简体中文 UI 文案；
- 中文浏览器语言自动识别；
- 设置页“简体中文”选项；
- `zh-CN` 数字/日期 Locale；
- `Noto Sans SC` 字体预设；
- Windows/macOS 中文系统字体回退；
- ChatGPT Bridge 扩展名称与说明的简体中文本地化。

## ChatGPT 浏览器桥接

`chatgpt-extension` 是可选的 Chromium Manifest V3 扩展。它操作当前浏览器中已经登录的 `chatgpt.com` 页面，而不是调用 OpenAI API。

它可以：

- 从 ChatCMD 本地界面创建 ChatGPT 对话；
- 继续已有对话并保持浏览器侧身份；
- 按 ChatGPT 界面中可见的模型名称切换模型；
- 排队、重排、编辑、立即发送或删除后续消息；
- 停止当前回复；
- 把 ChatGPT 回复和对话 URL 回传到对应的本地任务；
- 在 ChatGPT 页面上显示 ChatCMD 审批请求；
- 在原生子 Agent 委派不可用时提供浏览器端回退执行。

开发安装：

1. 启动 ChatCMD：`http://127.0.0.1:8080` 或 `http://localhost:8080`。
2. 打开 `chrome://extensions/`、`edge://extensions/` 或 `brave://extensions/`。
3. 开启“开发者模式”。
4. 选择“加载已解压的扩展程序”。
5. 选择仓库中的 `chatgpt-extension` 文件夹。
6. 在同一个浏览器配置中登录 ChatGPT。
7. 重新加载 ChatGPT 与本地 ChatCMD 页面。

## 架构

ChatCMD 的核心组成：

- **Rust 主服务**：HTTP、MCP、API、WebSocket、运行时管理。
- **chatcmd-core**：核心领域模型。
- **chatcmd-runtime**：终端、文件系统、Git、进程与本地执行。
- **chatcmd-storage**：SQLite 持久化。
- **chatcmd-mcp**：MCP 服务端与工具目录。
- **web**：React + Vite 管理界面。
- **chatgpt-extension**：ChatGPT 浏览器桥接扩展。

## 环境要求

- [Rust](https://www.rust-lang.org/tools/install) **1.85 或更高版本**，包含 Cargo。
- [Node.js](https://nodejs.org/) **20.19+** 或 **22.12+**，以及 npm。
- [Git](https://git-scm.com/)。
- 本地 Shell：
  - Windows：PowerShell 或 `cmd.exe`
  - macOS/Linux：`bash` 或 `zsh`
- 平台构建工具：
  - Windows：Visual Studio Build Tools + MSVC C++ 工作负载
  - macOS：Xcode Command Line Tools
  - Linux：C/C++ 工具链，以及桌面目标依赖的 `winit` / `tray-icon` 平台包

## 从源码快速启动

使用上游仓库：

```bash
git clone https://github.com/int04/ChatCmd.git
cd ChatCmd/web
npm ci
npm run build
cd ..
cargo run
```

使用当前中文 fork 时，将仓库地址替换为：

```bash
git clone https://github.com/haoranzheng/ChatCmd.git
cd ChatCmd
git checkout feat/zh-cn-localization
cd web
npm ci
npm run build
cd ..
cargo run
```

打开 <http://127.0.0.1:8080>。首次启动会自动创建并迁移本地 SQLite 数据库。

前端热更新开发模式：

```bash
# 终端 1：仓库根目录
cargo run

# 终端 2
cd web
npm ci
npm run dev
```

然后打开 <http://127.0.0.1:5173>。Vite 会把 `/api` 和 `/ws` 代理到 8080 端口的 Rust 服务。

## 连接 MCP 客户端

1. 在 ChatCMD 中打开 **Plugin 列表**。
2. 选择 **创建新的 Plugin 连接**。
3. 为配置填写容易识别的名称。
4. 只选择该客户端确实需要的工具权限，然后保存。
5. 本地 MCP 客户端可从配置菜单选择 **创建新的访问码**，并立即保存一次性端点。
6. 将该 URL 添加为客户端的 Streamable HTTP MCP 服务器。无需 `Authorization` 请求头，密钥位于 URL 最后一段路径中。

Web 托管 AI 通过公网端点连接时，请阅读 [docs/PLUGIN_SETUP.md](docs/PLUGIN_SETUP.md)。

## 配置

| 环境变量 | 默认值 | 用途 |
| --- | --- | --- |
| `CHATCMD_BIND` | `127.0.0.1` | 监听 IP。除非明确了解暴露范围与 Origin 策略，否则建议保持回环地址。 |
| `CHATCMD_PORT` | `8080` | HTTP、MCP、API、UI 和 WebSocket 端口。 |
| `CHATCMD_DB_PATH` | 平台数据目录 | 覆盖 SQLite 数据库路径。 |
| `CHATCMD_WEB_DIST` | `web/dist` | 非嵌入式开发构建使用的前端目录。 |
| `CHATCMD_LOG_PATH` | `logs/chatcmd.log` | 覆盖追加式诊断日志路径。 |
| `CHATCMD_FINALIZATION_GRACE_SECONDS` | `120` | 自动结束等待宽限期，限制为 30–3600 秒。 |
| `CHATCMD_BUILD_VERSION` | Cargo 包版本 | 嵌入构建或 Release 包中的版本号。 |
| `RUST_LOG` | `chat_cmd_client=info,tower_http=info` | Rust tracing 日志过滤器。 |

默认数据库位置：

- Windows：`%LOCALAPPDATA%\ChatCmdClient\data\chatcmd.db`
- macOS：`~/Library/Application Support/ChatCmdClient/chatcmd.db`
- Linux：`$XDG_DATA_HOME/chatcmd-client/chatcmd.db`，或 `~/.local/share/chatcmd-client/chatcmd.db`

## 构建 Release

构建嵌入前端的独立二进制：

```bash
cd web
npm ci
npm run build
cd ..
cargo build --release --features embedded-web
```

Windows 打包：

```powershell
.\scripts\build-windows.ps1 -Version 0.1.0
```

macOS 打包：

```bash
CHATCMD_BUILD_VERSION=0.1.0 ./scripts/build-macos.sh
```

完整发布说明见 [docs/RELEASING.md](docs/RELEASING.md)。

## 验证修改

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings

cd web
npm ci
npm run lint
npm test -- --run
npm run build

cd ../chatgpt-extension
node --test content-chatgpt.test.cjs
```

贡献者工作流和更细粒度测试命令见 [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)。

## 安全与隐私

- 把每个 MCP URL 当作密码处理。任何获得完整 Token URL 的人，都可能获得对应配置的权限。
- 本地访问配置密钥以哈希形式存储。某些公网 Plugin 链接为了能够再次复制，会以可恢复明文保存在本地 SQLite 中，因此应使用操作系统账户与磁盘安全措施保护数据库。
- 远程访问优先使用回环绑定，并通过带认证的 HTTPS Tunnel 或反向代理暴露。
- 本地管理 API 使用可信调用方标记并加密 JSON Body；WebSocket 使用临时 ECDH 派生的 AES-GCM 会话。这属于纵深防御，无法保护已经被攻击者控制的浏览器或电脑。
- 浏览器扩展没有 Cookie 权限，也不会读取或写入 ChatGPT 登录 Token，但它的设计目标就是读取并操作当前已登录的 ChatGPT 页面 DOM。
- 报告漏洞前请先阅读 [SECURITY.md](SECURITY.md)。不要在公开 Issue 中放置密钥或私人数据。

## 文档

- [文档索引](docs/README.md)
- [Plugin 与 ChatGPT 配置](docs/PLUGIN_SETUP.md)
- [架构](docs/ARCHITECTURE.md)
- [开发指南](docs/DEVELOPMENT.md)
- [MCP 方法参考](docs/mcp_method.md)
- [故障排查](docs/TROUBLESHOOTING.md)
- [加密协议](docs/ENCRYPTION_PROTOCOL.md)
- [诊断日志](docs/logs.md)
- [Release 指南](docs/RELEASING.md)

目前上述技术文档仍以上游英文版本为准；主界面、浏览器扩展元数据和本 README 已完成简体中文化。

## 贡献

欢迎提交贡献。创建 Pull Request 前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)、[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) 和 [GOVERNANCE.md](GOVERNANCE.md)。

## 许可证

ChatCMD 使用 [MIT License](LICENSE)。在遵守许可证声明与免责声明的前提下，可以使用、复制、修改、分发、再许可和销售，包括集成到商业产品中。

第三方依赖、服务、商标和附带媒体仍受各自许可证和条款约束。

Copyright © 2026 Nghia Duc and ChatCMD contributors.
