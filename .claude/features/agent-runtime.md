# Agent 运行时

> 规格：[01 Agent 运行时](../../docs/prd/01-agent-runtime.md) · 最后更新：2026-08-23

## 一句话

Tauri 主进程密封启动用户本机的 Claude Code；CLI 只看见五个 Daybook MCP 工具，helper 经带会话令牌的 Unix socket 把调用转回主进程。

## 数据流

```text
AgentRuntime::status（agent_status / probe_agent 两个 command 的唯一出口）
  → ClaudeCodeBackend::status → qualify()（一次，OnceCell 缓存）
      候选枚举 → canonicalize 跟符号链接 → 普通文件 + 执行位 → claude --version 限时非空
  → 与 AgentRuntime 持有的 readiness 合成 available / availabilityReason / authenticated / ready / errorCode

AgentRuntime::probe
  → claude -p（密封参数、独立短会话）
  → 结构化 init / hook 事件
  → capability manifest 必须等于五工具注册表

AgentRuntime::parse_source
  → 先写 parse_attempts
  → AgentSession 绑定 /tmp/dbk-*/m.sock
  → claude -p fork/exec target/.../daybook-mcp
  → rmcp stdio 工具调用
  → helper 以 DAYBOOK_MCP_SOCKET + TOKEN 连回主进程
  → DraftStore 按 assignment 执行
  → complete_source + 正常退出后收束状态
```

## 关键文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/agent/runtime.rs` | 探测缓存、**readiness cell 与状态合成**、串行 gate、attempt 生命周期、同步取消、日志与 14 天清理 |
| `src-tauri/src/agent/claude.rs` | CLI 候选枚举与**安装资格鉴定**、密封参数、能力清单解析、进程组超时/取消、错误分类 |
| `src-tauri/src/agent/backend.rs` | `AgentBackend` trait、`BackendStatus`（含 `ready` / `availability_reason`）、`AvailabilityReason` |
| `src/agent/presentation.ts` | 把 `BackendStatus` 翻成用户看的话；**`ready` 只认 `status.ready`** |
| `src-tauri/src/agent/session.rs` | 短路径 UDS、0700 目录、随机令牌、会话内 DraftStore |
| `src-tauri/src/agent/registry.rs` | 五工具 canonical 注册表、读写范围、`tool_surface_version` 与期望 hash |
| `src-tauri/src/mcp/mod.rs` | `rmcp` 五工具壳；只转发到 `domain::draft` |
| `src-tauri/src/bin/daybook-mcp.rs` | CLI 启动的 stdio MCP helper；自己不打开数据库 |
| `src-tauri/prompts/m0-parse.md` | 人维护的解析协议与提示注入边界 |

## 业务规则

- M0 有且只有 `list_pending_sources`、`read_source`、`draft_transaction`、`report_source_total`、`complete_source`。
- `probe` 成功前不创建 attempt、不下发解析。缓存键含 backend 版本；应用重启不复用。
- **安装资格与解析就绪度是两层，分别由两处持有**（[01 §3.5](../../docs/prd/01-agent-runtime.md)）：
  - 安装资格在 `ClaudeCodeBackend`：候选**只做枚举不做筛选**，合格判据是「`canonicalize` 后是普通文件 + 有执行位 + `claude --version` 在 5 秒内以 0 退出且输出非空」。多个候选各自失败时按 `version_unreadable > not_executable > not_found` 取「走得最远的那次」当 `availability_reason`；整批鉴定有 10 秒总预算，防版本管理器的几十个目录拖慢启动。
  - 解析就绪度在 `AgentRuntime` 的 readiness cell（`NotProbed / Probing / Failed(AppError) / Ready`），**只由 `probe()` 写**。任务期失败（额度、超时、网络）不写它——那些不改写安装事实。
  - `AgentRuntime::status()` 把两者合成 `BackendStatus`；**前端不再拼装、不再反推 ready**。
- **`parse_source` 是 fail-closed 闸门**：readiness 非 `Ready` 直接返回——`Failed` 返回那次探测自己的码（`agent.not_authenticated` / `agent.tool_surface_unsealed` / …），`NotProbed` 与 `Probing` 返回 `agent.not_ready`。**它不再隐式补一次 probe**；探测由启动时的 `probe_agent` 或用户显式重试触发。本位币检查仍在最前面（`data.base_currency_required`，不属于 readiness）。
- **子进程失败原因要同时读 stdout 与 stderr**（`classify_process_failure`）：CLI **未登录时退出码 1、stderr 是 0 字节**，原因只在 stdout 的 stream-json 里——一条 `"error":"authentication_failed"` 事件加一条 `"is_error":true` / `"result":"Not logged in · Please run /login"` 的终结事件。两处抽出来与 stderr 合成一段信号，交**同一张词表**判定。**别按 `subtype` 判**：那条终结事件的 `subtype` 仍写着 `"success"`。也**别把整个 stdout 灌进词表**——正常解析的输出里本来就有账目文本。
- 密封配置清空 built-in tools、settings、skills、用户 MCP、auto-memory 与可调用 subagent；实测 manifest 多一项就返回 `agent.tool_surface_unsealed`。
- CLI 仍会声明不可调用的 `general-purpose` agent。只有在 `Agent` / `Task` 工具存在时 agent 定义才算有效能力；hook 不享受这个例外。
- attempt 在 spawn 前插入。失败、超时、取消、协议失败按 attempt 作废草稿；作废是置 `voided_at` 并写 system 审计，不删行。
- `cancel` 先向独立进程组发 SIGTERM，2 秒后仍未退出才 SIGKILL；等 attempt、草稿与 stdout/stderr 持久化收束后才返回。应用退出走同一条 shutdown，防止 helper 成为孤儿。
- `trace` 常开且不含金额、原文或 prompt；`debug` 可见开关，含完整调用与原始流。两类日志启动时清除 14 天前文件。
- 口述原文命中「总共 / 一共 / 合计 / 总计 / 独立 TOTAL」但未报告合计时，代码拒绝 `complete_source` 并保持会话可补救；这只是防已知漏报的保守词集，不是通用语义解析。
- **该闸门有两条出口、且有界**：回报合计，或在 `unparsed_note` 里说明为什么没有合计（产出 `completed_with_gaps`）。词表认不出「一共去了三个地方」这类非金额用法，只留一条出口会让这种口述根本无法完成解析。同一次尝试内第二次仍未满足即 `agent.protocol_violation`；计数器与条目数不符那条各自独立（`total_marker_rejections` / `completion_rejections`）。
- **密封指纹从真命令读回来，不是手抄的清单**：`seal()` 是 `sealed_command` 与 `sealed_config_contract` 共用的那一处，指纹取后者的 argv + env（socket/token/prompt 用固定占位串，helper 只取文件名），所以「加一个 flag 而指纹不变」在构造上不可能，也不随安装位置漂移。
- **CLI 发现只查静态路径，不 spawn 登录 shell**（枚举出的是**候选**，合格与否见上一条）：`PATH` + `~/.local/bin` + `~/.claude/local` + npm-global / volta / bun / yarn / pnpm + nvm / fnm / n 的带版本号目录（较新版本优先）。从 Finder 启动的 `.app` 只继承 `/usr/bin:/bin:/usr/sbin:/sbin`，`PATH` 那一路基本必然落空；这条只在打包后暴露，`cargo test` 里 `PATH` 是全的。发现只在构造 `AgentRuntime` 时做一次，装完 CLI 需重启应用。

## 已知边界与坑

- **`AgentBackend::status()` 是 `async` 的**，因为安装资格里含一次 `--version` 子进程。写测试 fake 时别忘了这个 `async`，也别在 fake 里自己编 `ready`——`AgentRuntime` 会覆盖它。
- **测试里解析之前要先 `runtime.probe(...)`**（`runtime.rs` 的 `probed()` 助手）。修正前 `parse_source` 自己顺手探一次，所以老用例不用管就绪度；现在不探就是 `agent.not_ready`。
- 三种安装失败原因在 UI 上是**三句不同的话**（`src/agent/presentation.ts`）：没找到 → 去装，不可执行 → `chmod`，版本读不出 → 修这个安装。都说成「未安装」等于没指引。
- `--safe-mode` 会把显式 `--mcp-config` 一起屏蔽，不能用于生产密封启动。
- **「已装未登录」这一档只有真机能验**：`cargo test` 与 `verify-m0.mjs` 都测不到它，2026-08-23 人工验收才发现界面报的是 `agent.spawn_failed`。复现方式：把 `HOME` 指到一个空目录再起应用（CLI 的凭证在 `$HOME/.claude.json`，换 `HOME` 即未登录），`PATH` 里放一个指向真 CLI 的同名符号链接。同一套受控 `HOME` / `PATH` 也能造出 `not_found` / `not_executable` / `version_unreadable` 三态，以及「`--version` 立即返回、真实会话先 `sleep` 再 exec」的延迟 probe。
- Claude Code 2.1.229 在有 `structuredContent` 时不把第二个 text content block 交给模型；口述正文因此同时放在 `structuredContent.text`。
- 探测会真实调用一次无副作用工具来逼出 hook 事件，会消耗少量 CLI 额度。
- managed policy 不保证在 init 中声明，是规格登记的残余风险。

## 相关

- [ADR-0003](../../docs/adr/0003-agent-runtime-and-pluggable-backend.md)
- [R6 spike](../../docs/spikes/2026-08-12-r6-agent-runtime.md)
- [导入截图与口述](./ingest-screenshot.md)
