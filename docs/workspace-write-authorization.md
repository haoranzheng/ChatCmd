# Task workspace binding and file authorization / 任务工作区绑定与授权

## 中文：本地管理操作步骤

本功能只通过 **ChatCMD 本地管理界面**授予权限。MCP 连接令牌、ChatGPT 消息中的绝对路径或工具参数均不能直接授予文件写入权限。

1. 启动 ChatCMD，在本机打开 `http://127.0.0.1:8080`，完成本地管理登录。不要把带 MCP 令牌的链接粘贴到管理界面。
2. 在左侧 **项目 / Projects** 点击 `+`，填写项目名称，通过 **选择文件夹** 挑选一个现有目录，例如 `E:\kaifa\SKSE\projects\skyrim-community-shaders`，保存。
3. 从 ChatCMD 的项目列表新建 ChatGPT 对话时，项目路径将传给新的桥接任务；从 ChatGPT Plugin 直接创建的 MCP 任务可能仍处于**未绑定**状态，必须在本地管理界面为该任务选择项目。不会静默猜测项目。
4. 在左侧对话列表选中对应任务，展开右侧 **任务信息**，找到 **工作区范围与文件权限**。在下拉列表里选择**主工作区**，然后在“附加授权目录”中勾选其它已保存的项目（最多 15 个）；也可点击“添加本地目录”选取文件夹并保存为项目。选定 **只读**、**读写** 或 **禁止访问**，该权限会应用于全部已选目录。
5. 保存之前确认项目目录。选择读写时必须再次确认；保存将作废旧的待处理操作审批和范围授权，且把该任务的执行模式改为 **Approval**。工具仍要在 Plugin 工具白名单内，读写操作仍需逐次审批。
6. 回到 ChatGPT 任务继续操作；失败提示 `permission_change_requires_user` 时回本地管理界面修复，不要修改 SQLite。撤销写权限时改为 **只读** 或 **禁止访问** 并保存。

如果目录被移动或删除、含 junction/symlink/reparse point，绑定会拒绝。编辑已经授权的 Workspace Project 的路径将使原任务的权限变成 **禁止访问**，必须重新审批。

### 权限含义

| 选项 | 访问语义 |
| --- | --- |
| 只读 / Read-only | 不授予项目内的受控文件写入；已本地绑定的任务只允许读取所选项目内文件；未本地绑定的历史任务仍由旧的显式读取路径规则控制 |
| 读写 / Read-write | 仅向当前任务授予所选项目目录内的受控文件修改资格，仍要求工具白名单与逐次操作审批 |
| 禁止访问 / Restricted | 拒绝当前任务经 ChatCMD 受控文件工具访问该项目 |

**多目录行为：** 主工作区继续作为相对路径和命令默认工作目录；访问附加目录必须使用明确的绝对路径（例如 `E:\\Work\\A` 与 `E:\\Assets\\B`）。同一盘符下多个目录可同时授权，但不会获得整个盘符、父目录或其它兄弟目录的权限。选择或取消勾选附加目录均需点击“保存工作区授权”；修改已授权目录的真实路径或删除项目，将撤销该目录的旧权限及关联审批。\n\n仅凭目录绑定**不能执行任何操作**：还必须通过 MCP 工具白名单和相应审批。操作路径必须位于该目录内。对另一工作区、父目录、兄弟目录或 `..` 穿越的受控文件写入会被拒绝。

**限制：** `command_run`、`shell_create` 等命令执行器并不是操作系统文件沙箱；经单次批准的 shell 进程仍拥有 ChatCMD 进程本身的 OS 权限。不要把本功能描述为可以限制任意 shell 脚本对其他目录的访问。需要严格隔离的命令应在低权限账户、容器或操作系统沙箱中运行。本功能不自动打开 Allow everything，也不授权整盘写入。

## English: local management recovery

1. Sign in to the local ChatCMD management page at `http://127.0.0.1:8080`.
2. Under **Projects**, add a saved Workspace Project using the native folder picker.
3. Select the existing ChatGPT/MCP conversation. In the right **Task information** sidebar, find **Workspace scope and file access**.
4. Select the primary saved project, then check up to 15 additional saved folders (or use **Add local folder** to create one). Choose **Read-only**, **Read-write**, or **Restricted** for every selected folder and click **Save workspace access**. Confirm each displayed path.
5. Return to the task. Write operations require an enabled MCP tool and an individual operation approval. Do not use `Allow everything`.
6. Revoke authorization by switching to Read-only or Restricted. Relative paths resolve against the primary folder; additional folders require explicit absolute paths. A changed project root invalidates prior write authority. A missing or unsafe path must be repaired by editing the saved project folder.

This is a managed filesystem authorization boundary, **not** a process sandbox. Commands executed under the ChatCMD OS account can have broader filesystem effects. Keep execution approvals enabled and inspect every command.

### Error recovery

| Code | Meaning | Local recovery |
| --- | --- | --- |
| `project_folder_required` | Task lacks a project folder for a relative operation | Select and bind a saved project |
| `permission_change_requires_user` | No locally approved read-write capability | In Task information, select a project and approve read-write |
| `path_outside_allowed_scope` | A write target is outside the authorized project | Correct the path or explicitly choose a different project locally |
| `approval_scope_invalid` | Project path or approved binding is no longer valid | Repair the project path, rebind, and approve again |
| `approval_timeout` | The individual operation approval expired | Request the operation again and approve it locally |

Neither public MCP traffic nor ChatGPT extension requests may invoke `PUT /api/local/tasks/{id}/workspace`. The route is protected by the existing local management session authentication and the extension route allowlist. The audit timeline records only task/project IDs and permission decisions, not file contents or credentials.
