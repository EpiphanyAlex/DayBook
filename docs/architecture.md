---
title: Daybook 系统架构基线
status: ready
owner: "@alex"
date: 2026-08-06
version: v0.1
---

# 系统架构基线

> 本文描述 Daybook 的**结构**：有哪些组件、各自负责什么、数据怎么流动、边界画在哪。
> **不可逆的决定在 [`docs/adr/`](./adr/)，本文不重新论证，只标依据。** 具体表结构、字段名、命令签名属于 [`docs/prd/`](./prd/) 各 sub-PRD 的范围。
> **当前状态：骨架阶段，`src/` 与 `src-tauri/` 尚未创建**——本文描述的是**目标结构**，落地由 [`docs/prd/00-foundation.md`](./prd/00-foundation.md) 起步。

---

## 1. 全局结构

```
┌─ React UI (TypeScript + Vite) ─────────────────────┐
│    审核界面 · 交易视图 · 事项视图 · 回顾            │
│                    ↕ Tauri command（IPC，唯一通道）  │
├─ Tauri v2 主进程 (Rust) ───────────────────────────┤
│    ├─ command 层      前端能调的全部能力            │
│    ├─ domain 层       校验、状态机、确认动作         │
│    ├─ MCP server      stdio · rmcp · 进程内         │  ← agent 通过它读写
│    ├─ agent launcher  spawn `claude -p` / `codex exec`
│    └─ store 层        rusqlite · 迁移 · 证据文件     │
└─ SQLite（单一事实源）+ 证据目录（截图原件）─────────┘
                    ↕
          用户自己的 agent CLI ──→ 其模型服务商
          （该 CLI 自行发起，应用不代理/不转发/不记录）
```

**唯一的出站流量**是 agent CLI 与模型服务商之间的通信。应用本身没有任何网络代码（[ADR-0001](./adr/0001-local-first-desktop-platform.md)）。

## 2. 组件职责

| 组件 | 负责 | **不**负责 |
|---|---|---|
| **React UI** | 渲染、交互、键盘流、表单校验（体验层） | 业务规则、SQL、进程管理 |
| **command 层** | 前端可调能力的全部入口；参数校验；统一错误契约 | 业务逻辑本身 |
| **domain 层** | 总额校验、确认动作、事项状态机、记忆规则应用 | 存储细节、UI 状态 |
| **MCP server** | 把「起草能力」暴露给 agent；**权限边界即工具签名** | 决定是否入库 |
| **agent launcher** | spawn / 监控 / 回收 agent CLI 子进程；后端可插拔 | 解析 agent 的业务输出（那由 MCP 工具承接） |
| **store 层** | SQLite 访问、迁移、证据文件读写 | 业务校验 |

## 3. 两条写入路径（架构上最重要的一张图）

[ADR-0002](./adr/0002-ai-never-writes-directly.md) 在结构上的落地：**写入草稿和写入事实，是两条物理隔离的路径。**

```
路径 A：agent 起草（AI 可达）
  agent CLI ──MCP 工具──▶ domain::draft ──▶ store ──▶ draft_* 表
                                                      + audit_log

路径 B：人工确认（AI 不可达）
  React UI ──Tauri command──▶ domain::confirm ──▶ store ──▶ 事实表
                                  │                          + audit_log
                                  └─ 总额交叉校验不过 ⇒ 拒绝，不提供旁路
```

**结构性保证**（违反即缺陷）：

1. **MCP 工具集里不存在能触及事实表的工具。** 不是「有但要求别用」，是根本没有。
2. **`domain::confirm` 不被任何 MCP 工具调用。** 确认动作只能由 Tauri command 触发。
3. **不存在通用「执行任意 SQL」或「任意文件写入」类工具**（[ADR-0003](./adr/0003-agent-runtime-and-pluggable-backend.md) §3）。
4. **两条路径都写 `audit_log`**，且该表 append-only。

## 4. 一次导入的完整数据流（M0 的骨架）

```
1. 用户拖入截图
      ↓
2. store：文件落进证据目录 + 写 sources 表（原件、哈希、导入时间）
      ↓
3. agent launcher：spawn agent CLI，告知「有一个新 source 待解析」
      ↓
4. agent：经 MCP 工具读取该 source →（视觉解析）→ 逐条调用起草工具
      ↓
5. MCP 工具 → domain::draft → draft_transactions
      每条强制带 source_id + 原文片段，否则工具直接拒绝
      ↓
6. domain：对该 source 做总额交叉校验，结果落在 source 上
      ↓
7. UI：审核界面拉取草稿 + 原文并排显示，异常项前置
      ↓
8. 用户逐条/批量确认 → Tauri command → domain::confirm
      校验不过 ⇒ 拒绝批量入库
      ↓
9. store：写 transactions + audit_log；草稿标记已消费
      ↓
10. 用户的每次修改 → 记忆规则（商户→分类映射等）
```

**控制流由代码决定**：第 6、8、9 步的判断全是确定性代码。**LLM 只在第 4 步出现**，做抽取、解析、分类与起草，不做最终业务决策（[ADR-0003](./adr/0003-agent-runtime-and-pluggable-backend.md) §5）。

## 5. 存储布局

| 位置 | 内容 | 说明 |
|---|---|---|
| SQLite 数据库文件 | 全部结构化数据（唯一事实源） | 放在用户看得见、能备份的位置；**不放 iCloud Drive**（会损坏） |
| 证据目录（数据库旁） | 截图原件 | 普通目录，用户能自己翻看；数据库只存路径与元数据 |
| 纯文本导出 | 兜底 | 让用户随时能带着数据走 |

金额一律整数存最小货币单位；多币种存三元组。完整规则见 [ADR-0004](./adr/0004-data-model-sqlite-integer-money.md) 与 [`.claude/rules/money-and-data.md`](../.claude/rules/money-and-data.md)。

## 6. 前后端边界

- **唯一通道是 Tauri command**。不创建 Electron、内嵌 Node.js 本地服务或 `localhost` HTTP API（[ADR-0001](./adr/0001-local-first-desktop-platform.md)）。
- **IPC 上传的金额是整数**，不是格式化字符串、不是浮点。
- 错误走**统一错误契约**（形状与错误码集由 [`docs/prd/00-foundation.md`](./prd/00-foundation.md) 定义），前端不解析错误文案做分支。
- 前端不含业务规则——总额校验、状态机、确认条件全在 Rust 侧。

## 7. Agent 运行时边界

- MCP server 走 **stdio**、用 **`rmcp`**、**在 Tauri 主进程内起**：不额外拉进程、不开端口（[ADR-0003](./adr/0003-agent-runtime-and-pluggable-backend.md) §1）。
- **单 agent + 多工具**，不按业务领域拆。子 agent 只用于**上下文隔离**（如解析超长截图），不用于业务分工。
- **后端可插拔**：agent launcher 通过接口访问后端，v1 只实现 Claude Code，接口从第一天存在。
- **应用不打包任何厂商凭证、不提供第三方登录、不代理厂商鉴权。**

## 8. 尚未决定的结构问题

这些属于「已知会影响结构、但现在决定为时过早」，登记在此以免被沉默填掉（**零沉默原则**，见 [`docs/CONTEXT.md`](./CONTEXT.md)）：

| # | 问题 | 影响 | 谁来决 / 何时 |
|---|---|---|---|
| A1 | 长截图的子 agent 上下文隔离怎么切——按图切还是按解析结果条数切 | [`docs/prd/02-ingest.md`](./prd/02-ingest.md) | M2 批量解析时，实测决定 |
| ~~A2~~ **已关闭（2026-08-07）** | agent 解析失败/超时的重试策略放在 launcher 还是 domain | [`docs/prd/01-agent-runtime.md`](./prd/01-agent-runtime.md) | **结论：domain，且 v1 不做自动重试**——launcher 只管「起进程、看着它、超时就杀」，不知道失败是否值得重试；自动重试会在用户不知情时二次消耗 AI 额度。`failed` 的来源显式列在 UI 上由用户一键重试。见 [01 §5](./prd/01-agent-runtime.md) R2 |
| A3 | 记忆规则在解析前注入 agent 上下文，还是在起草后由 domain 应用 | [`docs/prd/06-memory.md`](./prd/06-memory.md) | M3 前，两种都要能被审计日志覆盖 |
| A4 | 前端状态管理选型（是否引入状态库） | 全部 UI 模块 | M1 审核界面开工前 |

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.1 | 2026-08-06 | 初版：全局结构图、组件职责表、两条写入路径的结构性保证、一次导入的完整数据流、存储布局、前后端与 agent 运行时边界、未决结构问题 A1–A4 |
