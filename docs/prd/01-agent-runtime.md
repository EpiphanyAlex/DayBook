---
title: 01 Agent 运行时 — MCP server、agent 启动器与可插拔后端
status: draft
owner: "@alex"
date: 2026-08-07
version: v0.2
---

# 01 · Agent 运行时

> 把「起草能力」以 MCP 工具的形式暴露给用户自己的 agent CLI，并管理这些 CLI 子进程的生命周期。
> 依据：[ADR-0003 Agent 运行时与可插拔后端](../adr/0003-agent-runtime-and-pluggable-backend.md)、[ADR-0002 AI 永不直接写入](../adr/0002-ai-never-writes-directly.md)。

## 1. 问题

产品的成本模型建立在「**用户自带 AI 额度 → 边际成本为零 → 可以暴力啃任意格式截图**」上（[`docs/PRD.md` §3](../PRD.md) 支点 1）。这要求应用能：

1. 把数据库读写能力交给用户本机已登录的 agent CLI；
2. **在结构上保证** agent 无法绕过草稿区（[ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 1）；
3. 不被单一厂商绑死——Anthropic 对第三方 agent 使用订阅额度的政策已反复三次（[`docs/PRD.md` §12](../PRD.md)）。

**本模块是 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 1 的物理实现处。** 如果这里的工具面开错一个口子，上层所有校验都是装饰。

## 2. 范围与非目标

**范围**：MCP server（stdio · `rmcp` · 进程内）· 工具面定义与权限边界 · agent launcher（spawn / 监控 / 回收 / 超时）· 可插拔后端接口 · agent 侧提示词与任务下达 · 子进程日志采集。

**非目标**：

- **解析编排的业务流程**（什么时候该起草、起草几条）——属 [02 导入](./02-ingest.md)
- **草稿的校验与确认**——属 [03 审核与草稿区](./03-review.md)
- **多 agent 自主编排**——[ADR-0003 §2](../adr/0003-agent-runtime-and-pluggable-backend.md) 明确不做
- **代理厂商鉴权、打包厂商凭证、第三方登录**——[ADR-0003 §4](../adr/0003-agent-runtime-and-pluggable-backend.md) 明确禁止
- **v1 实现 Claude Code 以外的后端**——接口存在即可，M4 再补

## 3. 决定与依据

### 3.1 MCP server：stdio · `rmcp` · 主进程内

依据 [ADR-0003 §1](../adr/0003-agent-runtime-and-pluggable-backend.md)。

- 传输用 **stdio**：没有端口、没有本机其他程序可见的攻击面，生命周期天然绑定子进程
- 实现用 **`rmcp`**（官方 Rust MCP SDK）
- **在 Tauri 主进程内起**，不额外拉进程、不开 HTTP 端口（与 [ADR-0001](../adr/0001-local-first-desktop-platform.md) 的「不开 localhost API」一致）

### 3.2 工具面（权限边界即工具签名）

**v1 工具清单**——**这份清单就是 agent 能做的全部事情**：

| 工具 | 能力 | 写入范围 |
|---|---|---|
| `list_pending_sources` | 列出待解析的来源 | 只读 |
| `read_source` | 读一个来源的元数据与文件路径 | 只读 |
| `draft_transaction` | 起草一笔交易 | **只能写 `draft_transactions`** |
| `draft_item` | 起草一个事项 | **只能写 `draft_items`** |
| `query_memory` | 查记忆规则（商户→分类映射等） | 只读 |
| `report_source_total` | 回报「来源自己声明的合计」，供总额校验 | 只能写 `sources.declared_total_minor` |

**硬性禁令**（违反即缺陷，[ADR-0003 §3](../adr/0003-agent-runtime-and-pluggable-backend.md)）：

1. **不提供通用「执行任意 SQL」类工具**
2. **不提供通用「任意文件写入 / 任意命令执行」类工具**
3. **工具集里不存在任何能触及事实表（`transactions` / `items`）的工具**
4. **`domain::confirm`（确认动作）不被任何 MCP 工具调用**——它只能由 Tauri command 触发

**`draft_transaction` 的参数强制**：`source_id` 与 `evidence_text` 是必填参数，缺任一 → 工具直接返回错误，不写库。**这是证据链在工具层的第一道闸**（数据层还有第二道，见 [00 地基 §3.6](./00-foundation.md)）。

### 3.3 每次工具写入都记审计

`draft_transaction` / `draft_item` / `report_source_total` 每次成功调用写一条 `audit_log`，`actor = "agent"`，附后端标识与本次 agent 会话 ID。依据 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 4。

### 3.4 agent launcher

- 通过 `std::process::Command` spawn 子进程，把 MCP server 的 stdio 端接上
- **v1 后端**：Claude Code（`claude -p`）
- **超时**：单次任务有硬超时（默认值待实测定，见 §5 风险 R1），超时后 kill 子进程并把该来源标记为解析失败，**不留半截草稿**——已写入的草稿随失败一并作废（同一事务）
- **并发**：v1 **同时只跑一个 agent 子进程**。排队，不并发
- **日志**：子进程 stdout/stderr 采集到内存环形缓冲，供 UI 排障显示；**不落盘**（可能含截图内容片段）

### 3.5 可插拔后端接口

```rust
// 形状示意，非最终签名——内部实现自由度不在本文规格化
trait AgentBackend {
    fn id(&self) -> &'static str;              // "claude-code" / "codex" / ...
    fn probe(&self) -> Result<BackendStatus>;  // CLI 是否已安装/已登录
    fn spawn(&self, task: &AgentTask) -> Result<AgentHandle>;
}
```

**约束**：

- v1 的 Claude Code 实现**不得成为其他代码的直接依赖**——上层只见 `dyn AgentBackend`
- `probe()` 只做「CLI 存在且可执行」的检测，**不代用户登录、不读取用户的凭证文件**
- 后端不可用时应用**仍可启动**，UI 如实显示「未检测到可用的 agent CLI」并给出安装指引

### 3.6 任务下达

- 应用给 agent 的是**任务级指令**（「有一个新来源 `<id>` 待解析，用工具读它，逐笔起草，最后回报合计」），不是「填这个 JSON」
- 提示词模板存为**独立文件**，不硬编码在 Rust 字符串里——便于调整与 diff
- **控制流由代码决定**（[ADR-0003 §5](../adr/0003-agent-runtime-and-pluggable-backend.md)）：是否入库、是否重试、总额是否通过，全由 Rust 侧判断，不问 agent

## 4. 否决的替代方案

| 方案 | 否决原因 |
|---|---|
| agent 输出 JSON，应用解析 | ① 权限边界要在解析后再补一层校验，而 MCP 的边界就是工具签名 ② 做不到多轮编排（agent 无法「先查历史上这个商户归哪一类，再决定怎么起草」） ③ 失去「Claude Code 和 Codex 都支持 MCP」这条可插拔红利（[ADR-0003](../adr/0003-agent-runtime-and-pluggable-backend.md)「理由」） |
| HTTP 传输的 MCP server | 要开端口 → 本机其他程序可见的攻击面，与「数据不出本机」的姿态相悖；且生命周期不再天然绑定子进程 |
| MCP server 做成独立二进制 | `rmcp` 允许进程内起，独立二进制凭空多一层进程管理与版本同步 |
| 按业务领域拆「记账 agent」+「事项 agent」 | 用户一句话经常跨域（「今天吃饭 180，明天交房租，上周那 400 是给我妈买茶叶」），拆开要先分派再合并，凭空多出错误面且闭不了环（[ADR-0003 §2](../adr/0003-agent-runtime-and-pluggable-backend.md)） |
| 应用内置 API key / 提供厂商登录 | 直接违反 [ADR-0003 §4](../adr/0003-agent-runtime-and-pluggable-backend.md)；且一旦代理鉴权，「数据不出本机」就不再成立 |
| v1 就实现多个后端 | 第二个后端的价值在厂商政策变化时才兑现；接口存在即可保住架构，实现推到 M4 |

## 5. 待决与风险

| # | 事项 | 影响 | 谁来决 / 何时 |
|---|---|---|---|
| R1 | agent 单次任务的硬超时默认值——太短会砍掉正常的长截图解析，太长会让失败态卡住 UI | 本文 §3.4 | M0 实测一张真实银行流水截图的耗时后定（dogfooding 样本：CBA 网银），**结果回流本文** |
| R2 | 解析失败/超时的重试策略放在 launcher 还是 domain（[`docs/architecture.md` §8](../architecture.md) 未决 A2） | 本文 §3.4、[02 导入](./02-ingest.md) | M0 实现时就近决定并回流两处 |
| R3 | 长截图的子 agent 上下文隔离怎么切——按图切还是按解析结果条数切（[`docs/architecture.md` §8](../architecture.md) 未决 A1） | 本文 §3.2、[02 导入](./02-ingest.md) | M2 批量解析时实测决定 |
| R4 | Anthropic 订阅额度政策若再变（[`docs/PRD.md` §12](../PRD.md)），Claude Code 后端可能失效 | 全产品 | 对策已定（可插拔接口），**不需要额外决策**；登记以免被当成新问题重新讨论 |
| R5 | agent 会话 ID 与审计日志的关联粒度——一次导入一个会话，还是一个来源一个会话 | 本文 §3.3、[03 审核与草稿区](./03-review.md) 的溯源 UI | M0 实现时定并回流 |

## 6. 验收标准

- [ ] `cargo fmt --all -- --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test` 全绿
- [ ] `cargo test agent::tool_surface_has_no_sql_tool` 通过——遍历已注册工具，断言无通用 SQL / 通用文件写入 / 通用命令执行类工具
- [ ] `cargo test agent::tools_cannot_write_fact_tables` 通过——断言全部工具的写入目标表集合与 `{transactions, items}` 交集为空
- [ ] `cargo test agent::draft_requires_evidence_args` 通过——`draft_transaction` 缺 `source_id` 或 `evidence_text` 时返回错误且未写库
- [ ] `cargo test agent::every_write_tool_writes_audit` 通过——每个写入类工具调用后 `audit_log` 恰好多一条且 `actor = "agent"`
- [ ] `cargo test agent::timeout_leaves_no_partial_drafts` 通过——注入超时后，该来源关联的草稿数为 0
- [ ] `cargo test agent::backend_absent_app_still_starts` 通过——`probe()` 失败时应用初始化仍成功，状态如实为不可用
- [ ] `cargo test agent::single_concurrent_process` 通过——连续下达两个任务时第二个排队，同时存活的子进程数恒为 1
- [ ] `rg -n 'sk-|api[_-]?key|Authorization' src-tauri/src` 无命中（不打包厂商凭证）
- [ ] `node scripts/verify-m0.mjs`（**待建**，M0 端到端脚本）退出码 0

**人工验收**：

- [ ] 未安装 Claude Code 的干净机器上启动应用，不崩溃，UI 给出可操作的安装指引
- [ ] 一次真实解析中，UI 能看到子进程日志（用于排障）

## 7. 回流记录

*（尚无——本 sub-PRD 未开工。实现证伪规格时先回写这里，再改代码。）*

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.1 | 2026-08-06 | 初版：MCP server 形态（stdio/`rmcp`/进程内）、六个工具的权限边界与四条硬性禁令、审计写入、launcher（超时/并发/日志）、可插拔后端接口形状、任务下达方式；否决方案六条；待决 R1–R5；验收标准 10 条可执行 + 2 条人工 |
| v0.2 | 2026-08-07 | 随 [`docs/PRD.md` v0.2](../PRD.md) 定位修正同步：待决 R1 的实测样本描述从「真实澳洲银行截图」改为「真实银行流水截图」，具名组合降为 dogfooding 样本标注。决定与验收标准未变 |
