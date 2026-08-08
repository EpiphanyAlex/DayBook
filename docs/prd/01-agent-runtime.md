---
title: 01 Agent 运行时 — MCP server、agent 启动器与可插拔后端
status: ready
owner: "@maintainer"
date: 2026-08-08
version: v0.5
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

**这份清单就是 agent 能做的全部事情。** 按里程碑分层——**M0 只注册 M0 那一组**，因为 `draft_items` / `memory_rules` 两张表在 M0 尚未建（[00 地基 §3.6](./00-foundation.md)），注册无表可写的工具会让验收无法通过：

| 工具 | 里程碑 | 能力 | 写入目标表集合 | **读取范围** |
|---|---|---|---|---|
| `list_pending_sources` | **M0** | 列出待解析的来源 | ∅ | **仅本次任务指派的来源**（M0 恒为 1 个）；不得遍历 `sources` 全表 |
| `read_source` | **M0** | 读一个来源的元数据与证据文件 | ∅ | **仅本次任务指派的来源**；`source_id` 不在指派集合内 → `agent.tool_rejected` |
| `draft_transaction` | **M0** | 起草一笔交易 | `{draft_transactions}` | ∅ |
| `report_source_total` | **M0** | 回报来源自身印着的合计 | `{sources.declared_total_*}`（列级） | ∅ |
| `query_memory` | M3 | 查记忆规则 | ∅ | **仅显式传入的键**（商户名等）；**不提供「列出全部规则」** |
| `draft_item` | M3 | 起草一个事项 | `{draft_items}` | ∅ |

> **两列都是工具注册时必须声明的元数据，不是文档里的说明文字。** 验收 `agent::tools_cannot_write_fact_tables` 与 `agent::tools_declare_read_scope` 遍历的正是这两份声明——没有它们，测试无法实现。

#### 只读 ≠ 无限读（2026-08-08 设计评审新增）

此前本表只声明**写入**目标表，只读工具一律标 `∅`。**但「只读」不等于「无限读」**，依据 [ADR-0006](../adr/0006-smart-agent-dumb-tools.md)「附带决定：读取范围也要收窄」，其原则是**最小暴露**——AI 只读取任务需要的内容。

两个具体后果：

- **`query_memory` 若能列举全部规则**，agent 就能把用户的**个人语境词表**整个拉进上下文（「我妈 = 家庭支出」这类）。它对解析一张超市小票毫无用处，却会随请求发往模型服务商。因此本工具**只按键回答**：`query_memory(merchants: [...])` 返回这些商户的规则，**没有「全部列出」这个能力**。
- **`read_source` / `list_pending_sources` 收窄到「本次任务指派的来源」**。M0 的编排是代码侧串行下发（[02 导入 §3.5](./02-ingest.md)），agent 从不自己挑要解析什么，所以这个收窄不损失任何能力。M2 批量时一次任务可能指派多个来源，工具形态不变。

**硬性禁令**（违反即缺陷，[ADR-0003 §3](../adr/0003-agent-runtime-and-pluggable-backend.md)）：

1. **不提供通用「执行任意 SQL」类工具**
2. **不提供通用「任意文件写入 / 任意命令执行」类工具**
3. **工具集里不存在任何能触及事实表（`transactions` / `items`）的工具**
4. **`domain::confirm`（确认动作）不被任何 MCP 工具调用**——它只能由 Tauri command 触发。**实现手段是模块边界**：`mcp/` 只依赖 `domain::draft`，拿不到 `domain::confirm`（见 [`.claude/rules/rust-tauri.md` §4](../../.claude/rules/rust-tauri.md)），越权在编译期不可表达

**`draft_transaction` 的参数强制**：`source_id` 与 `evidence_text` 是必填参数，缺任一 → 返回 `agent.tool_rejected`，不写库。**这是证据链在工具层的第一道闸**（数据层还有第二道，见 [00 地基 §3.6](./00-foundation.md)）。

#### `report_source_total` 的可信性要求（2026-08-07 M0 开工评审新增）

**问题**：总额交叉校验是 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 3——「唯一能在无人工介入下捕获错误的机制」。但校验的两边（逐笔草稿、声明合计）**都由同一个 agent 在同一次会话里产生**。若 agent 把逐笔读错后又用自己那批数的和当作「声明合计」，**校验永远通过，闸门完全失效**。

**因此本工具的语义被收窄为**：

1. 它回报的必须是**来源上原本印着的那个数字**（账单底部的 Total、余额行），**不是 agent 把逐笔加起来的结果**。
2. 参数为 `(amount_minor, currency, evidence_text)` **三者齐全**，缺任一 → `agent.tool_rejected`。`evidence_text` 是该合计在来源上的原文片段。
3. **来源上没有印合计时，agent 必须不调用本工具**——留空即 `unavailable`（[03 审核 §3.3](./03-review.md)），**不许自己算一个填进去**。
4. `declared_total_*` 三列在数据层有 all-or-nothing CHECK（[00 地基 §3.6](./00-foundation.md)），漏填一项写不进去。

**诚实说明这道闸门的边界**：它能捕获「逐笔读错但合计读对」，捕获不了「逐笔和合计一起读错」。后者只能靠**人扫一眼合计的原文**——这就是第 2 条强制 `evidence_text` 的理由：审核界面把声明合计与它的原文并排显示，让基准本身也可核对（[03 审核 §3.3](./03-review.md)）。**闸门 3 不是万能的，规格必须如实写明它挡不住什么。**

### 3.3 每次工具写入都记审计

`draft_transaction` / `draft_item` / `report_source_total` 每次成功调用写一条 `audit_log`，`actor = "agent"`，附后端标识与本次 agent 会话 ID。依据 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 4。

### 3.4 agent launcher

- 通过 `std::process::Command` spawn 子进程，把 MCP server 的 stdio 端接上
- **v1 后端**：Claude Code（`claude -p`）
- **并发**：v1 **同时只跑一个 agent 子进程**。排队，不并发
- **日志**：**落盘，分两级**——见下方「日志分级」。此前本条写「不落盘」，已由 [ADR-0007](../adr/0007-local-observability-and-log-tiers.md) 推翻

**超时与失败的作废语义**（2026-08-07 M0 开工评审修正）：

- 单次任务有硬超时（M0 默认值见 §5 R1），超时后 kill 子进程，把该来源置为 `failed` 并写 `parse_error_code = agent.timeout`
- **不留半截草稿**：该来源下本次会话产生的草稿全部作废

> **修正**：本节原文写「已写入的草稿随失败一并作废（**同一事务**）」，**这在物理上做不到**——§3.3 要求每次工具写入**各自**记一条审计，N 次独立的 MCP 调用不可能事后收进同一个事务。
>
> **正确语义是补偿动作**：作废是一次独立的删除，按 `(source_id, agent_session_id)` 定位本次会话的草稿，在**它自己的**事务里删除，并写一条 `audit_log`（`actor = "system"`、`action = "void"`、`entity_type = "source"`）。agent 此前那 N 条 `actor = "agent"` 的审计记录**保持不变**——`audit_log` 是 append-only，不回溯抹除。审计因此如实呈现「起草了 N 条 → 超时 → 系统作废」的完整过程。

#### 日志分级（2026-08-08，依据 [ADR-0007](../adr/0007-local-observability-and-log-tiers.md)）

**本条推翻了 v0.1–v0.3 的「不落盘」。** 原因：评审要求的「查日志 → 复现 bug → 变成回归测试」链条，前提就是日志落盘——进程一退内存缓冲就没了。

| 级别 | 默认 | 内容 |
|---|---|---|
| `trace` | **开** | 工具调用的**名称与参数形状**、耗时、退出码、重试次数、状态机转移、`agent_session_id`、`backend_id`、usage 元数据。**不含金额、不含原文、不含 prompt** |
| `debug` | **关** | `trace` 全部，外加完整提示词、agent 原始输出、**完整的 MCP 工具调用参数** |

- 位置 `<数据目录>/logs/`，与 SQLite 和 `evidence/` 同级——**用户看得见、能自己删**
- 一次会话一个 JSONL 文件，文件名含 `agent_session_id`
- **默认保留期后自动清除**（具体天数实现时定，回流本文）
- **绝不上传、绝不上报**。[ADR-0001](../adr/0001-local-first-desktop-platform.md) 禁的是「数据离开本机」，不是「写进本机磁盘」
- **自用阶段 `debug` 默认开** —— 夹具导出依赖它（[07 评测 §3.6](./07-eval.md)），关着等于没有飞轮
- `debug` 开关**必须在 UI 上可见并注明「会记录完整账目细节」**，不是只能改配置文件的隐藏项

`debug` 必须包含**完整**工具调用参数，因为 agent 是非确定性的：复现一个 bug 不能靠「重跑一次 agent」，只能靠**重放录下来的调用序列**。

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
- **提示词模板是「程序记忆」，只能由应用版本或人工编辑更新，不得被模型修改。**「程序记忆」指的是**规定 agent 怎么做事的那部分**（提示词、模板、流程），它与 agent 记住的事实（[06 记忆](./06-memory.md)）分属两类：后者随使用积累，前者只能由人改。事实上工具面里没有写文件的工具，所以 agent 现在改不了——**但那是巧合，不是设计**，因此在此明写。任何未来新增的工具都不得让 agent 触及提示词目录
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
| R1 | agent 单次任务的硬超时默认值——太短会砍掉正常的长截图解析，太长会让失败态卡住 UI | 本文 §3.4 | **M0 取 180 秒为初值**（2026-08-07 评审给定，避免「无值可用」阻塞开工）。M0 实测一张真实网银流水截图的耗时后校准，**结果回流本文** |
| ~~R2~~ **已关闭（2026-08-07）** | 解析失败/超时的重试策略放在 launcher 还是 domain（[`docs/architecture.md` §8](../architecture.md) 未决 A2） | 本文 §3.4、[02 导入 §3.5](./02-ingest.md) | **结论：domain。** launcher 只负责「起进程、看着它、超时就杀」，不知道失败是否值得重试——那要看来源状态与用户意图。**v1 不做自动重试**：`failed` 的来源显式列在 UI 上由用户一键重试（[02 导入 §3.5](./02-ingest.md)），符合「控制流由代码决定、且不偷偷烧用户额度」。[`docs/architecture.md` §8](../architecture.md) A2 同步关闭 |
| R3 | 长截图的子 agent 上下文隔离怎么切——按图切还是按解析结果条数切（[`docs/architecture.md` §8](../architecture.md) 未决 A1） | 本文 §3.2、[02 导入](./02-ingest.md) | M2 批量解析时实测决定，**不阻塞 M0**（M0 单张截图） |
| R4 | Anthropic 订阅额度政策若再变（[`docs/PRD.md` §12](../PRD.md)），Claude Code 后端可能失效 | 全产品 | 对策已定（可插拔接口），**不需要额外决策**；登记以免被当成新问题重新讨论 |
| ~~R5~~ **已关闭（2026-08-07）** | agent 会话 ID 的粒度——一次导入一个会话，还是一个来源一个会话 | 本文 §3.3、[00 地基 §3.6](./00-foundation.md) schema | **结论：一个来源一个会话。** 理由是 §3.4 的作废语义按 `(source_id, agent_session_id)` 定位本次会话的草稿——若一次导入共用一个会话，批量导入时某一张超时会波及同批其他来源的草稿。已落进 [00 地基 §3.6](./00-foundation.md) 的 `sources.agent_session_id` 与 `draft_transactions.agent_session_id` 两列 |

## 6. 验收标准

- [ ] `cargo fmt --all -- --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test` 全绿
- [ ] `cargo test agent::tool_surface_has_no_sql_tool` 通过——遍历已注册工具，断言无通用 SQL / 通用文件写入 / 通用命令执行类工具
- [ ] `cargo test agent::tools_cannot_write_fact_tables` 通过——遍历每个工具**注册时声明的写入目标表集合**（§3.2），断言与 `{transactions, items}` 交集为空
- [ ] `cargo test agent::m0_tool_surface_is_exactly_four` 通过——M0 注册的工具恰为 §3.2 的四个，不含目标表尚未建立的 `draft_item` / `query_memory`
- [ ] `cargo test agent::tools_declare_read_scope` 通过——每个工具都声明了读取范围，遍历该声明可断言无「全表/全库」范围
- [ ] `cargo test agent::read_source_rejects_unassigned` 通过——`read_source` 传入未指派的 `source_id` 时返回 `agent.tool_rejected`，不返回数据
- [ ] `cargo test agent::query_memory_has_no_list_all` 通过（**M3**）——工具签名要求显式键，不存在「列出全部规则」的调用形式
- [ ] `cargo test agent::trace_log_has_no_content` 通过——`trace` 级写入路径产生的记录中不含金额字段、`evidence_text` 或 prompt 文本
- [ ] `cargo test agent::debug_log_is_replayable` 通过——`debug` 级记录的工具调用序列可被反序列化并原样重放（[07 评测 §3.6](./07-eval.md)）
- [ ] `rg -n 'prompts/' src-tauri/src/mcp` 无命中——工具面不触及提示词目录（§3.6 程序记忆）
- [ ] `rg -n 'confirm' src-tauri/src/mcp` 无命中——`mcp/` 模块不引用确认动作（禁令 4 的可执行形式；原验收写作「静态断言调用方集合」，`cargo test` 做不了调用图分析）
- [ ] `cargo test agent::draft_requires_evidence_args` 通过——`draft_transaction` 缺 `source_id` 或 `evidence_text` 时返回 `agent.tool_rejected` 且未写库
- [ ] `cargo test agent::report_total_requires_evidence_and_currency` 通过——`report_source_total` 缺 `currency` 或 `evidence_text` 时返回 `agent.tool_rejected` 且未写库（§3.2 可信性要求第 2 条）
- [ ] `cargo test agent::every_write_tool_writes_audit` 通过——每个写入类工具调用后 `audit_log` 恰好多一条且 `actor = "agent"`
- [ ] `cargo test agent::timeout_voids_only_own_session` 通过——两个来源各自解析，其一超时后，**只有该来源该会话的草稿被作废**，另一来源的草稿不受影响（§5 R5 的会话粒度结论）
- [ ] `cargo test agent::void_is_audited_as_system` 通过——作废写一条 `actor = "system"` / `action = "void"` 的审计，且 agent 此前的 `actor = "agent"` 记录**仍在**（append-only，§3.4）
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
| v0.5 | 2026-08-08 | 公开仓库去个人化：§3.2「只读 ≠ 无限读」与 §3.6「程序记忆」两处**去掉外部参考仓库出处、把结论内联**（最小暴露 = AI 只读取任务需要的内容；程序记忆 = 规定 agent 怎么做事的那部分，与事实记忆分属两类）——**两条规定本身未变**；§3.4「dogfooding 期间 `debug` 默认开」改为「自用阶段」（同 [ADR-0007](../adr/0007-local-observability-and-log-tiers.md)）；§5 R1 的具名网银样本改为「真实网银流水截图」；`owner` 改为 `@maintainer` |
| v0.4 | 2026-08-08 | **设计评审回流。** ① §3.2 工具表新增 **「读取范围」列** + 「只读 ≠ 无限读」小节（依据 [ADR-0006](../adr/0006-smart-agent-dumb-tools.md)）：此前只锁写入，导致 `query_memory` 可列举全部规则、把用户的个人语境词表整个送进模型上下文；现改为只按键回答，`read_source` / `list_pending_sources` 收窄到本次任务指派的来源。② §3.4 **「不落盘」被推翻**（[ADR-0007](../adr/0007-local-observability-and-log-tiers.md)）：改为 `trace` 常开（元数据，无金额原文）/ `debug` 默认关（含完整工具调用参数，供夹具重放），自用阶段 `debug` 默认开，开关必须在 UI 可见。③ §3.6 明写**提示词模板属程序记忆、不得被模型修改**（程序记忆与事实记忆分属两类，前者只能由人改）——原先 agent 改不了只是巧合。④ §6 新增 6 条验收 |
| v0.3 | 2026-08-07 | **M0 开工评审 → `status: ready`。** ① §3.2 工具面**按里程碑分层**——M0 只注册 4 个，`draft_item` / `query_memory` 推到 M3（其目标表 `draft_items` / `memory_rules` 在 M0 尚未建，注册即验收必挂）；工具须**注册时声明写入目标表集合**，否则 `tools_cannot_write_fact_tables` 无法实现。② §3.2 新增 **`report_source_total` 可信性要求**——修复闸门 3 的结构性失效：原规格允许 agent 自行填写总额校验的基准值，而校验两边同源等于没有闸门；现强制 `(amount_minor, currency, evidence_text)` 三者齐全、必须是来源上印着的数字、没印就不许调用，并如实写明这道闸门挡不住什么。③ §3.4 **修正「同一事务」**——N 次独立 MCP 调用各自记审计，不可能事后收进一个事务；改为按 `(source_id, agent_session_id)` 的补偿性作废 + `actor = "system"` 审计。④ §5 **R2、R5 关闭**（重试归 domain 且 v1 不自动重试；会话粒度 = 一个来源一个会话），**R1 给定 M0 初值 180 秒**避免无值阻塞。⑤ §6 验收从 10 条增至 14 条，并把无法实现的「静态断言调用图」改为 `rg` 检查 |
| v0.2 | 2026-08-07 | 随 [`docs/PRD.md` v0.2](../PRD.md) 定位修正同步：待决 R1 的实测样本描述从「真实澳洲银行截图」改为「真实银行流水截图」，具名组合降为 dogfooding 样本标注。决定与验收标准未变 |
