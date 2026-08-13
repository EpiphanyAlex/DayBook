# Agent 运行时

> 规格：[01 Agent 运行时](../../docs/prd/01-agent-runtime.md) · 最后更新：2026-08-13

## 一句话

Tauri 主进程密封启动用户本机的 Claude Code；CLI 只看见五个 Daybook MCP 工具，helper 经带会话令牌的 Unix socket 把调用转回主进程。

## 数据流

```text
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
| `src-tauri/src/agent/runtime.rs` | 探测缓存、串行 gate、attempt 生命周期、同步取消、日志与 14 天清理 |
| `src-tauri/src/agent/claude.rs` | CLI 发现、密封参数、能力清单解析、进程组超时/取消、错误分类 |
| `src-tauri/src/agent/session.rs` | 短路径 UDS、0700 目录、随机令牌、会话内 DraftStore |
| `src-tauri/src/agent/registry.rs` | 五工具 canonical 注册表、读写范围、`tool_surface_version` 与期望 hash |
| `src-tauri/src/mcp/mod.rs` | `rmcp` 五工具壳；只转发到 `domain::draft` |
| `src-tauri/src/bin/daybook-mcp.rs` | CLI 启动的 stdio MCP helper；自己不打开数据库 |
| `src-tauri/prompts/m0-parse.md` | 人维护的解析协议与提示注入边界 |

## 业务规则

- M0 有且只有 `list_pending_sources`、`read_source`、`draft_transaction`、`report_source_total`、`complete_source`。
- `probe` 成功前不创建 attempt、不下发解析。缓存键含 backend 版本；应用重启不复用。
- 密封配置清空 built-in tools、settings、skills、用户 MCP、auto-memory 与可调用 subagent；实测 manifest 多一项就返回 `agent.tool_surface_unsealed`。
- CLI 仍会声明不可调用的 `general-purpose` agent。只有在 `Agent` / `Task` 工具存在时 agent 定义才算有效能力；hook 不享受这个例外。
- attempt 在 spawn 前插入。失败、超时、取消、协议失败按 attempt 作废草稿；作废是置 `voided_at` 并写 system 审计，不删行。
- `cancel` 先向独立进程组发 SIGTERM，2 秒后仍未退出才 SIGKILL；等 attempt、草稿与 stdout/stderr 持久化收束后才返回。应用退出走同一条 shutdown，防止 helper 成为孤儿。
- `trace` 常开且不含金额、原文或 prompt；`debug` 可见开关，含完整调用与原始流。两类日志启动时清除 14 天前文件。
- 口述原文命中「总共 / 一共 / 合计 / 总计 / 独立 TOTAL」但未报告合计时，代码拒绝 `complete_source` 并保持会话可补救；这只是防已知漏报的保守词集，不是通用语义解析。

## 已知边界与坑

- `--safe-mode` 会把显式 `--mcp-config` 一起屏蔽，不能用于生产密封启动。
- Claude Code 2.1.229 在有 `structuredContent` 时不把第二个 text content block 交给模型；口述正文因此同时放在 `structuredContent.text`。
- 探测会真实调用一次无副作用工具来逼出 hook 事件，会消耗少量 CLI 额度。
- managed policy 不保证在 init 中声明，是规格登记的残余风险。

## 相关

- [ADR-0003](../../docs/adr/0003-agent-runtime-and-pluggable-backend.md)
- [R6 spike](../../docs/spikes/2026-08-12-r6-agent-runtime.md)
- [导入截图与口述](./ingest-screenshot.md)
