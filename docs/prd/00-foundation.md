---
title: 00 地基 Foundation — 数据层、SQLite schema、迁移与错误契约
status: ready
owner: "@alex"
date: 2026-08-08
version: v0.4
---

# 00 · 地基 Foundation

> 定义所有模块共用的底座，并**把跨模块的共享决定一次性拍死**：SQLite 接入与迁移、标识/时间/金额约定、表结构、Tauri 命令契约与错误形状、TS 桥。
> 依据：[ADR-0004 数据模型](../adr/0004-data-model-sqlite-integer-money.md)、[ADR-0001 本地优先桌面平台](../adr/0001-local-first-desktop-platform.md)、[`docs/architecture.md`](../architecture.md)。

## 1. 问题

地基的「用户」是其余六份 sub-PRD 的执行者——一批**无记忆的 agent**。它们会在两处相撞：

1. **文件层**：`Cargo.toml`、迁移文件、命令注册点、共享 TS 类型——谁都要碰。
2. **语义层**：ID 是什么类型、时间怎么存、金额怎么表示、错误长什么形状、`draft_*` 与事实表的边界画在哪。**这些问题如果本文保持沉默，每次实施都会用自己的假设去填，且各填各的。** 文件不冲突，语义照样冲突。

因此本文的标准是**零沉默**（见 [`CLAUDE.md`](./CLAUDE.md)）：任何两份 sub-PRD 必须达成一致的东西，在这里要么被决定（标依据），要么显式挂起（标谁来决，§5）。单个模块内部的实现自由（函数组织、内部命名、文件怎么分）**不在此规格化**。

**当前仓库状态（2026-08-06 实测）**：`src/` 与 `src-tauri/` 尚不存在，无任何代码。**一切从零决定，没有既成事实需要迁就。**

## 2. 范围与非目标

**范围**：SQLite 接入与迁移运行器 · 标识/时间/金额/汇率约定 · 全部表结构 · Tauri 命令契约与统一错误形状 · TS 类型桥 · 证据文件目录布局 · 应用数据目录位置。

**非目标**：

- **业务逻辑**——总额校验属 [03 审核与草稿区](./03-review.md)，解析编排属 [02 导入](./02-ingest.md)
- **MCP server 与 agent 启动**——属 [01 Agent 运行时](./01-agent-runtime.md)
- **UI 组件与设计令牌**——M1 审核界面开工前不需要，届时由 [03 审核与草稿区](./03-review.md) 定
- **同步、导出格式细节**——v1 只保证「纯文本导出」这条兜底存在，格式在 M4 定

## 3. 决定与依据

### 3.1 存储引擎：SQLite，`rusqlite`（bundled）

依据 [ADR-0004 §1](../adr/0004-data-model-sqlite-integer-money.md)。

- **不用 ORM**（`diesel` 的抽象层厚、`sqlx` 引入不必要的 async 复杂度）
- **不用 `tauri-plugin-sql`**——它让前端能直接发 SQL，破坏 [`docs/architecture.md` §6](../architecture.md) 的前后端边界
- `bundled` feature：不依赖系统 SQLite 版本，打包可预测
- 开库时设 `PRAGMA journal_mode=WAL`、`PRAGMA foreign_keys=ON`

### 3.2 应用数据目录

```
<用户数据目录>/Daybook/
├── daybook.db          ← SQLite，唯一事实源
├── daybook.db-wal
├── daybook.db-shm
└── evidence/           ← 证据原件（截图），普通目录，用户可自行翻看
    └── <yyyy>/<mm>/<source_id>.<ext>
```

- 位置必须是**用户看得见、能备份**的地方
- **不得放 iCloud Drive**——按需下载与文件替换会损坏 SQLite（WAL 与主文件不同步）。若检测到路径在 iCloud 容器内，启动时报错并拒绝打开，不静默继续
- 数据库只存证据文件的**相对路径**，不存绝对路径（用户移动整个目录后仍可用）

### 3.3 标识与时间约定（全模块强制）

| 约定 | 值 |
|---|---|
| 主键类型 | **UUID v4**，以 `TEXT` 存小写带连字符形式 |
| 时间 | **RFC 3339 UTC**，以 `TEXT` 存（如 `2026-08-06T04:12:00Z`） |
| 「交易发生日」这类业务日期 | 单独字段，`TEXT` 存 `YYYY-MM-DD`，**不带时区**——用户说「8 月 3 号那笔」指的是本地日历日 |
| 布尔 | `INTEGER` 0/1 |
| 枚举 | `TEXT`，取值集在本文各处显式列出 |

**为什么 UUID 而不是自增整数**：改 ID 类型是全表迁移，成本远高于一开始就用 UUID（[ADR-0004 §5](../adr/0004-data-model-sqlite-integer-money.md)）。

### 3.4 金额与汇率表示（全模块强制）

依据 [ADR-0004 §2/§3](../adr/0004-data-model-sqlite-integer-money.md)，❌/✅ 代码对照见 [`.claude/rules/money-and-data.md`](../../.claude/rules/money-and-data.md)。

| 项 | 决定 |
|---|---|
| 金额 | **整数最小货币单位**（分 / cent）。SQLite `INTEGER`，Rust `i64`，TS `number` |
| 币种 | ISO 4217 三字母大写码，`TEXT`（`AUD` / `CNY` / …） |
| **本位币** | **可切换**，逐笔存 `base_currency`（同为 ISO 4217 码）——见下方「本位币切换语义」 |
| 汇率 | **定点整数**：存 `rate_ppm`，即「1 单位原币 = 多少本位币」× 1_000_000，`INTEGER` |
| 舍入 | 本位币金额 = `round_half_even(原币金额 × rate_ppm / 1_000_000)`。**舍入在写入前完成一次，结果落库**，不在读取时重算 |
| 浮点 | **任何位置禁止**——包括中间计算、IPC 传输、测试夹具 |

**自洽约束**：写入交易时校验 `本位币金额 == round_half_even(原币金额 × rate_ppm / 1_000_000)`，不满足则拒绝写入并返回 `data.money_inconsistent`。原币与本位币相同时 `rate_ppm = 1_000_000`，**不设特例分支**。

#### 本位币切换语义（2026-08-07 拍板，[`docs/PRD.md` §13](../PRD.md) P2 已关闭）

本位币**可切换**——固定单一本位币等于在 schema 层重新引入地域假设，与 [`docs/PRD.md` §3.1](../PRD.md) 矛盾。三条规则，均由 [ADR-0004](../adr/0004-data-model-sqlite-integer-money.md)「折算冻结在交易发生那一刻」直接推导：

1. **`base_currency` 逐笔存储在交易行上，确认时冻结**——不是只存一个全局设置。全局设置只决定「新交易默认用哪个本位币」。
2. **切换本位币不改动任何历史行。** 追溯换算需要历史汇率，而历史汇率没有可靠来源（[`docs/PRD.md` §13](../PRD.md) P1）；即便有，改写已确认的事实数据也违反 [ADR-0002](../adr/0002-ai-never-writes-directly.md)（事实表只由人工确认动作写入）。
3. **跨越切换点的汇总按 `base_currency` 分组呈现，不静默相加。** 把两种本位币的金额加在一起会得到一个无意义的数字——与总额校验的 `unavailable` 不伪装成通过是同一条原则。

> **给实现者**：不要写「假设全库只有一个本位币」的查询。任何汇总类 SQL 必须 `GROUP BY base_currency` 或显式断言结果集只含一种本位币。

### 3.5 迁移

- 编号 SQL 文件：`src-tauri/migrations/0001_core.sql`、`0002_*.sql`…（**待建**）
- 用 SQLite 的 `PRAGMA user_version` 记录已应用到第几号，启动时顺序补齐
- **幂等**：重复启动不重复应用
- **只前进不回滚**：不实现 down migration。开发期改 schema 就改迁移文件并重建本地库；v1 发布后只加新编号
- 磁盘 `user_version` **超前**于代码已知的最大编号（用户降级了应用）⇒ 报 `data.migration_drift` 并拒绝打开，**不静默继续**

### 3.6 表结构

**完整表清单**（v1 终态）：

```
sources · draft_transactions · transactions · draft_items · items
memory_rules · audit_log
```

**M0 建其中四张**：`sources` · `draft_transactions` · `transactions` · `audit_log`。其余三张随对应 sub-PRD 在后续里程碑建（`draft_items` / `items` 属 [05 事项](./05-items.md)，`memory_rules` 属 [06 记忆](./06-memory.md)）。

> **2026-08-07 M0 开工评审**：以下四张表的字段**逐列定死**。此前本节只写「关键约束」并注明「详细字段由对应 sub-PRD 在开工前补进」——现在正是开工前，不补齐则每次实施各写各的（**零沉默原则**）。
> 类型省略处按 §3.3 约定：ID 为 `TEXT`（UUID v4）、时间为 `TEXT`（RFC 3339 UTC）、业务日期为 `TEXT`（`YYYY-MM-DD`）。

#### `sources` — 一份被导入的原始材料

| 列 | 类型 | 空 | 说明 |
|---|---|---|---|
| `id` | TEXT PK | 非空 | |
| `kind` | TEXT | 非空 | **`file`**（截图/PDF）\| **`utterance`**（一段口述或文字的转写结果）。见下方「来源不等于文件」 |
| `content_hash` | TEXT **UNIQUE** | 非空 | SHA-256 十六进制；**导入幂等以此为准**（[02 导入 §3.2](./02-ingest.md)）。`utterance` 取转写文本的哈希 |
| `original_filename` | TEXT | **可空** | 用户那份文件的原名，仅供显示。`kind = utterance` 时为空 |
| `ext` | TEXT | 非空 | 规范化小写、不带点。`utterance` 恒为 `txt` |
| `byte_size` | INTEGER | 非空 | |
| `evidence_relpath` | TEXT | 非空 | **相对数据目录**的路径（§3.2）。`utterance` 的转写文本**也落盘成 `.txt`**，与截图同等对待 |
| `imported_at` | TEXT | 非空 | |
| `state` | TEXT | 非空 | 取值集 `imported` / `parsing` / `parsed` / `failed` / `reviewed`，语义与转移规则由 [02 导入 §3.4](./02-ingest.md) 定义 |
| `declared_total_minor` | INTEGER | 可空 | **来源自身印着的合计**，供总额校验 |
| `declared_total_currency` | TEXT | 可空 | 该合计的币种——**没有币种的金额无法校验** |
| `declared_total_evidence_text` | TEXT | 可空 | 合计在来源上的原文片段——**校验基准本身也必须可核对**，见 [03 审核 §3.3](./03-review.md) |
| `parse_error_code` | TEXT | 可空 | `state = failed` 时非空，取 §3.7 的 `agent.*` / `ingest.*` 码 |
| `agent_session_id` | TEXT | 可空 | 最近一次解析的会话（§3.3 会话粒度） |

**CHECK 约束**：`declared_total_minor` / `declared_total_currency` / `declared_total_evidence_text` **三者要么全空、要么全非空**。缺任一即视为「未声明合计」，总额校验结果为 `unavailable`。

**列级写入权限**：agent 经 MCP 工具**只能写** `declared_total_*` 三列（[01 Agent 运行时 §3.2](./01-agent-runtime.md) 的 `report_source_total`）。`state` 等其余列只由 Rust 侧代码写。

##### 来源不等于文件（2026-08-08 设计评审）

`kind` 这一列存在的理由：**闸门 2 要的不是「文件」，是「痕迹 + 原文」**。一段口述就是痕迹，转写文本就是原文。

此前 `sources` 隐含假设来源必然是文件，导致一个硬墙——`draft_transactions.source_id` 非空，而口述没有文件，于是**「今天吃饭 180」这句话在结构上无法起草**。而 [ADR-0003 §2](../adr/0003-agent-runtime-and-pluggable-backend.md) 论证「不按业务领域拆 agent」时举的例子正是一句跨实体的口述，ADR 自己的论据落不了地。

三条随之确定：

1. **转写文本落盘成 `.txt`，与截图同等对待。** 这样 `evidence_relpath` 保持非空，闸门 2 的实现路径对两种来源完全一致，不产生分支。
2. **`utterance` 的 `declared_total_*` 恒为空** —— 一段口述没有「账单底部印着的合计」。CHECK 约束不受影响（全空是合法的），总额校验结果恒为 `unavailable`（[03 审核 §3.3](./03-review.md)）。**闸门 3 对语音来源天然失效**，这是已知且接受的代价。
3. **`utterance` 的幂等键仍是内容哈希** —— 同一段话说两次会被判为重复。这是**刻意的**：用户重说一遍通常意味着他以为上次没记上，返回已存在的 `source_id` 比产生两批重复草稿好。

#### `draft_transactions` — AI 起草的待确认交易

| 列 | 类型 | 空 | 说明 |
|---|---|---|---|
| `id` | TEXT PK | 非空 | |
| `source_id` | TEXT → `sources(id)` | **非空** | [ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 2，数据层强制 |
| `evidence_text` | TEXT | **非空** | 同上 |
| `agent_session_id` | TEXT | 非空 | 溯源到具体哪次解析 |
| `backend_id` | TEXT | 非空 | 产出这条草稿的后端（`claude-code` / `codex` / …，见 [01 §3.5](./01-agent-runtime.md)） |
| `model_id` | TEXT | 可空 | 后端报告的模型标识，取不到时为空 |
| `occurred_on` | TEXT | 非空 | 业务日期 |
| `amount_minor` | INTEGER | 非空 | 原币金额 |
| `currency` | TEXT | 非空 | 原币币种 |
| `base_amount_minor` | INTEGER | **可空** | 见下方「草稿阶段的三元组可空」 |
| `base_currency` | TEXT | 可空 | 同上 |
| `rate_ppm` | INTEGER | 可空 | 同上 |
| `direction` | TEXT | 非空 | `expense` / `income` / `transfer` |
| `merchant` | TEXT | 非空 | **原文**，不做归一化覆盖（[04 交易 §3.1](./04-transactions.md)） |
| `category` | TEXT | 可空 | 起草时未必判得出 |
| `channel` | TEXT | 可空 | 同上 |
| `confidence` | INTEGER | 可空 | agent 自评 0–100，供 [03 审核 §3.4](./03-review.md) 异常前置排序 |
| `created_at` | TEXT | 非空 | |
| `consumed_at` | TEXT | 可空 | **非空 = 已被确认消费**；[03 审核 §3.1](./03-review.md)「标记为已消费而非删除」的落点 |

> **`backend_id` / `model_id` 为什么必须存**（2026-08-08 新增）：[07 评测 §3.2](./07-eval.md) 的 eval 集就是「草稿 ← 交易」这条 join。若不记产出草稿的后端与模型，模型一升级就**无法区分「模型退步了」和「我改坏了提示词」**——20 个用例的基线会变得不可解释。这是 eval 成立的必要条件，不是可选的元数据。

**草稿阶段的三元组可空**：`base_amount_minor` / `base_currency` / `rate_ppm` **三者要么全空、要么全非空**（CHECK）。全非空时必须满足 §3.4 的自洽约束。

允许可空的理由：起草时汇率未必可得（[04 交易 §3.2](./04-transactions.md) 的三条汇率路径都可能落空）。**但确认入库时三者必须齐全**——缺则拒绝确认，对应 04 的「缺汇率不入库」。这条是草稿表与事实表约束强度不同的唯一处，刻意为之。

#### `transactions` — 已确认交易（事实表）

| 列 | 类型 | 空 | 说明 |
|---|---|---|---|
| `id` | TEXT PK | 非空 | |
| `source_id` | TEXT → `sources(id)` | 可空 | **仅手工录入时为空**（[04 交易 §3.5](./04-transactions.md)） |
| `source_draft_id` | TEXT → `draft_transactions(id)` | 可空 | **溯源字段**：兑现 [03 审核 §3.1](./03-review.md)「审计能回答入库的这条当初 AI 起草成什么样」 |
| `evidence_text` | TEXT | 可空 | 与 `source_id` 同生共死 |
| `occurred_on` | TEXT | 非空 | |
| `amount_minor` · `currency` | | 非空 | 原币 |
| `base_amount_minor` · `base_currency` · `rate_ppm` | | **非空** | 三元组齐全且自洽（§3.4） |
| `direction` · `merchant` | TEXT | 非空 | |
| `merchant_normalized` · `category` · `channel` · `note` | TEXT | 可空 | |
| `confirmed_at` | TEXT | 非空 | 事实表的行必然经人确认 |
| `deleted_at` | TEXT | 可空 | **软删除**（[04 交易 §3.5](./04-transactions.md)）；非空的行不进任何汇总 |

**CHECK 约束**：`source_draft_id` 非空 ⇒ `source_id` 非空（来自草稿必有来源）；`source_id` 非空 ⇔ `evidence_text` 非空。

#### `audit_log` — append-only 变更记录

| 列 | 类型 | 空 | 说明 |
|---|---|---|---|
| `id` | TEXT PK | 非空 | |
| `actor` | TEXT | 非空 | **`agent` / `human` / `system`** |
| `at` | TEXT | 非空 | |
| `entity_type` | TEXT | 非空 | `source` / `draft_transaction` / `transaction` / … |
| `entity_id` | TEXT | 非空 | |
| `action` | TEXT | 非空 | `create` / `update` / `confirm` / `discard` / `void` / `delete` |
| `before_json` · `after_json` | TEXT | 可空 | `create` 无 before，`delete` 无 after |
| `agent_session_id` | TEXT | 可空 | `actor = agent` 时非空 |

> **`actor` 为什么必须有 `system`**：超时作废草稿（[01 Agent 运行时 §3.4](./01-agent-runtime.md)）、状态机推进、迁移——这些由代码执行，既不是 agent 也不是人。只给 `{agent, human}` 会逼实现者往里塞一个语义错误的值。

**硬性结构保证**（违反即缺陷，见 [`docs/architecture.md` §3](../architecture.md)）：

1. `draft_*` 与事实表**结构分离**，不共用同一张表加状态字段
2. 草稿表的 `source_id` 与 `evidence_text` 为 `NOT NULL`
3. 代码中**不存在**针对 `audit_log` 的 `UPDATE` / `DELETE` 语句
4. agent 对 `sources` 的写入权限**收窄到 `declared_total_*` 三列**

### 3.7 命令契约与错误形状

- 前端能调的一切走 **Tauri command**，无第二通道（[ADR-0001](../adr/0001-local-first-desktop-platform.md)）
- 命令统一返回 `Result<T, AppError>`；`AppError` 序列化形状：

```jsonc
{
  "code": "data.migration_drift",   // 命名空间.错误名，稳定，前端可分支
  "message": "…",                   // 面向用户的中文说明
  "detail": null                    // 可选，结构化补充；不含敏感数据
}
```

- **前端按 `code` 分支，不解析 `message` 文案**
- **命名空间**：`data.*`（存储/迁移/一致性）· `ingest.*` · `review.*` · `agent.*` · `memory.*`

**权威错误码集**——本表是全仓库唯一出处，新增码先改这里再写代码（2026-08-07 补全：此前只登记了 `data.*`，而 [`.claude/rules/money-and-data.md`](../../.claude/rules/money-and-data.md) 与 [`.claude/rules/frontend.md`](../../.claude/rules/frontend.md) 已在示例中使用未登记的 `review.*` 码）：

| 码 | 归属 | 何时返回 |
|---|---|---|
| `data.storage_failure` | 00 | SQLite 读写失败 |
| `data.migration_drift` | 00 | 磁盘 `user_version` 超前于代码（§3.5） |
| `data.money_inconsistent` | 00 | 三元组不自洽（§3.4） |
| `data.not_found` | 00 | 实体不存在 |
| `data.invalid_argument` | 00 | 参数形状错误 |
| `data.icloud_path_rejected` | 00 | 数据目录在 iCloud 容器内（§3.2） |
| `ingest.duplicate_source` | [02](./02-ingest.md) | 内容哈希命中已有来源（§3.2 幂等；**非错误语义**，见 02 §3.2 的返回契约） |
| `ingest.unsupported_format` | [02](./02-ingest.md) | 文件格式不在支持集内 |
| `ingest.evidence_write_failed` | [02](./02-ingest.md) | 证据落盘失败（此时不写 `sources` 行） |
| `ingest.invalid_state_transition` | [02](./02-ingest.md) | 非法状态转移（§3.4） |
| `agent.backend_unavailable` | [01](./01-agent-runtime.md) | `probe()` 失败或 CLI 不可执行 |
| `agent.timeout` | [01](./01-agent-runtime.md) | 单次任务超硬超时 |
| `agent.spawn_failed` | [01](./01-agent-runtime.md) | 子进程起不来 |
| `agent.tool_rejected` | [01](./01-agent-runtime.md) | 工具参数不合法（如缺 `evidence_text`） |
| `review.total_mismatch` | [03](./03-review.md) | 总额校验 `failed` 时批量确认被拒 |
| `review.total_unavailable` | [03](./03-review.md) | 总额校验 `unavailable` 时批量确认被拒 |
| `review.missing_evidence` | [03](./03-review.md) | 确认时草稿缺证据（服务端二次校验） |
| `review.incomplete_triple` | [03](./03-review.md) | 确认时草稿三元组不齐（§3.6「草稿阶段的三元组可空」） |

### 3.8 TS 类型桥

- 前端不手写 `invoke` 调用，统一走一层 `call<T>(command, args)` 包装：把 Tauri 的错误规整成 `AppError` 形状
- 金额在 TS 侧用**分支类型**（branded type）标记为「最小单位整数」，防止和「元」混用
- Rust↔TS 类型是否引入 codegen（如 `tauri-specta`）**暂不决定**，见 §5 风险 R2

## 4. 否决的替代方案

| 方案 | 否决原因 |
|---|---|
| 文件化存储（Markdown / JSON 目录），不用 SQLite | 审核界面要对几百到几千条记录做排序、过滤、按商户聚合、跨图去重，回顾要做时间聚合——这些是关系查询，文件化要么自己实现索引要么每次全量扫（[ADR-0004](../adr/0004-data-model-sqlite-integer-money.md)「理由」）。**代价（数据不能用文本编辑器直接看）用「纯文本导出」补偿** |
| 金额用 `f64` / `Decimal` 库 | `0.1 + 0.2 != 0.3` 会让[总额交叉校验](../adr/0002-ai-never-writes-directly.md)——**唯一的自动纠错机制**——失效；`Decimal` 库能解决精度但引入依赖且仍需约定标度，不如直接整数直白 |
| 单张 `transactions` 表 + `is_draft` 状态字段 | 状态字段是软约束：一旦事实表里同时存在已确认与未确认记录，所有下游查询都要记得过滤，漏一处就把 AI 的猜测当成了事实（[ADR-0002](../adr/0002-ai-never-writes-directly.md)「理由」） |
| 只存本位币金额 + 一张历史汇率表 | 历史汇率在「数据不出本机」前提下没有可靠来源（[`docs/PRD.md` §13](../PRD.md) 开放问题 P1）；三元组把折算冻结在交易发生那一刻，事后汇率表变了不影响历史 |
| `tauri-plugin-sql`（前端直连 SQLite） | 破坏前后端边界（[`docs/architecture.md` §6](../architecture.md)），且让 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 的两条写入路径隔离在前端侧失效 |
| 实现 down migration | v1 单人单机，回滚场景等于「删库重建」；维护双向迁移是纯开销 |

## 5. 待决与风险

| # | 事项 | 影响 | 谁来决 / 何时 |
|---|---|---|---|
| ~~R1~~ **已关闭（2026-08-07）** | 本位币是固定单一还是可切换 | 本文 §3.4/§3.6、[04 交易](./04-transactions.md) | **结论：可切换**（@alex 拍板，[`docs/PRD.md` §13](../PRD.md) P2 同步关闭）。`base_currency` 逐笔存储并在确认时冻结，切换不改历史行，汇总按本位币分组——三条规则与推导见 §3.4「本位币切换语义」 |
| R2 | Rust↔TS 类型漂移；是否引入 `tauri-specta` 之类 codegen | 本文 §3.8 | 漂移造成第一个真实 bug 时开 spike，@alex 决 |
| R3 | 舍入规则选 half-even 尚未经真实对账验证——若某来源自身用 half-up，总额校验会系统性差几分 | 本文 §3.4、[03 审核与草稿区](./03-review.md) 的总额校验 | M2 处理真实 10 天数据时实测，**结果必须回流本文** |
| R4 | 证据目录长期累积到 GB 级后的清理策略（[`docs/PRD.md` §13](../PRD.md) 开放问题 P3） | 本文 §3.2、[02 导入](./02-ingest.md) | 真实使用出现容量问题时 |
| R5 | 数据库 at-rest 加密——v1 不做（数据不出本机 + macOS FileVault 已提供一层）。若未来需要，`rusqlite` 的 `bundled-sqlcipher` 是路径 | 全产品安全姿态 | v1 明确不做，登记以免被沉默填掉 |
| R6（**新增 2026-08-07**） | 证据区域坐标字段——[03 审核 §5](./03-review.md) R1 若结论是「能稳定定位」，`draft_transactions` 需加坐标列 | 本文 §3.6 | M1 开工前随 03 R1 一并决；**只前进迁移，届时加 `0002_*.sql` 即可**，不阻塞 M0 |

## 6. 验收标准

**可执行命令**（`cargo test` 的具体测试名是本 sub-PRD 对实现的**契约**，实现时须使用这些名字；改名要回流本文）：

- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` 通过
- [ ] `cargo test` 全绿
- [ ] `npm run lint` · `npm run typecheck` · `npm test` · `npm run build` 全绿
- [ ] `cargo test foundation::migration_idempotent` 通过——同一库连开两次，`user_version` 不变、无重复表
- [ ] `cargo test foundation::migration_drift_rejected` 通过——把 `user_version` 手工调高一号后打开，返回 `data.migration_drift` 且不写入任何数据
- [ ] `cargo test foundation::money_roundtrip_is_integer` 通过——金额经「写入 → 读出 → IPC 序列化 → 反序列化」四步后逐位相等
- [ ] `cargo test foundation::money_inconsistent_rejected` 通过——三元组不自洽时写入被拒并返回 `data.money_inconsistent`
- [ ] `cargo test foundation::base_currency_frozen_per_row` 通过——切换全局本位币后，已确认交易的 `base_currency` 与 `base_amount_minor` 逐行不变
- [ ] `cargo test foundation::rollup_groups_by_base_currency` 通过——含两种本位币的数据集，汇总按本位币分组返回，**不产生跨本位币的单一合计**
- [ ] `rg -n 'SUM\(base_amount_minor\)' src-tauri/src` 的每处命中都伴随 `GROUP BY base_currency` 或单本位币断言
- [ ] `cargo test foundation::draft_requires_evidence` 通过——`source_id` 或 `evidence_text` 为空时插入 `draft_transactions` 失败
- [ ] `cargo test foundation::declared_total_all_or_nothing` 通过——`declared_total_*` 三列只填其一或其二时 CHECK 拒绝
- [ ] `cargo test foundation::utterance_evidence_is_a_file` 通过——`kind = utterance` 的来源，转写文本已落盘成 `.txt` 且 `evidence_relpath` 非空（闸门 2 对两种来源同一条实现路径）
- [ ] `cargo test foundation::utterance_has_no_declared_total` 通过——`kind = utterance` 的 `declared_total_*` 全空、CHECK 通过、总额校验结果为 `unavailable`
- [ ] `cargo test foundation::draft_records_backend_and_model` 通过——每条草稿的 `backend_id` 非空（[07 评测 §3.2](./07-eval.md) 的必要条件）
- [ ] `cargo test foundation::draft_triple_all_or_nothing` 通过——草稿三元组只填部分时 CHECK 拒绝
- [ ] `cargo test foundation::confirm_requires_complete_triple` 通过——三元组不齐的草稿确认时返回 `review.incomplete_triple`
- [ ] `cargo test foundation::transaction_traces_to_draft` 通过——由草稿确认而来的 `transactions` 行，`source_draft_id` 指向该草稿且该草稿 `consumed_at` 非空
- [ ] `cargo test foundation::audit_actor_accepts_system` 通过——`actor = "system"` 可写入（超时作废等代码触发的动作有合法 actor）
- [ ] `cargo test foundation::icloud_path_rejected` 通过——数据目录指向 iCloud 容器路径时返回 `data.icloud_path_rejected`
- [ ] `rg -n 'f32|f64' src-tauri/src` 在金额相关模块无命中（浮点禁令，见 [`.claude/rules/money-and-data.md`](../../.claude/rules/money-and-data.md)）
- [ ] `rg -n 'UPDATE\s+audit_log|DELETE\s+FROM\s+audit_log' src-tauri/src` 无命中（append-only）

**人工验收**：

- [ ] 应用数据目录在访达里能直接打开，`evidence/` 下的截图能双击预览

## 7. 回流记录

*（尚无——本 sub-PRD 未开工。实现证伪规格时先回写这里，再改代码。）*

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.1 | 2026-08-06 | 初版：存储引擎与数据目录、标识/时间/金额/汇率约定、迁移策略、M0 四表与 v1 全表清单、命令契约与 `AppError` 形状、TS 桥；否决方案六条；待决 R1–R5；验收标准 11 条可执行 + 1 条人工 |
| v0.4 | 2026-08-08 | **设计评审（`/grill-with-docs` 会话）回流。** ① `sources` 新增 **`kind`（`file` \| `utterance`）** 并加「来源不等于文件」小节：闸门 2 要的是「痕迹 + 原文」而非「文件」；此前的隐含假设造成硬墙——`draft_transactions.source_id` 非空而口述无文件，导致 [ADR-0003 §2](../adr/0003-agent-runtime-and-pluggable-backend.md) 自己举的跨实体口述例子**在结构上落不了地**。转写文本落盘成 `.txt` 与截图同等对待，`original_filename` 随之改为可空。② `draft_transactions` 新增 **`backend_id` / `model_id`**——[07 评测](./07-eval.md) 的 eval 集是「草稿 ← 交易」join，不记后端与模型则基线不可解释。③ §6 新增 3 条验收 |
| v0.3 | 2026-08-07 | **M0 开工评审 → `status: ready`。** ① §3.6 四张 M0 表**逐列定死**（此前只有「关键约束」，本节自己注明「详细字段开工前补」——现在补上）：`sources` 新增 `state` / `evidence_relpath` / `parse_error_code` / `agent_session_id` 与 **`declared_total_currency`+`declared_total_evidence_text`**（原设计只有金额没有币种，无法校验；且校验基准本身必须可核对）；`draft_transactions` 明确三元组**草稿可空、确认必填**；`transactions` 新增 **`source_draft_id` 溯源列**（兑现 [03 §3.1](./03-review.md) 的审计承诺，原先无落点）；`audit_log.actor` **新增 `system`**（超时作废等代码触发的动作原先无合法取值）；新增 agent 对 `sources` 的**列级写入权限**收窄。② §3.7 补全**权威错误码集** 18 条——此前只登记 `data.*`，而 `.claude/rules/` 已在示例中使用未登记的 `review.*` 码。③ §5 新增 R6（证据坐标字段，随 [03](./03-review.md) R1 决，不阻塞 M0）。④ §6 新增 6 条验收 |
| v0.2 | 2026-08-07 | **待决 R1 关闭：本位币可切换**（@alex 拍板，[`docs/PRD.md` §13](../PRD.md) P2 同步关闭）。§3.4 新增 `base_currency` 约定行与「本位币切换语义」小节（逐笔冻结 / 切换不改历史 / 汇总按本位币分组，三条均由 [ADR-0004](../adr/0004-data-model-sqlite-integer-money.md) 折算冻结原则推导）；§3.4 自洽约束的「单币种交易」改述为「原币与本位币相同时」（本位币不再固定，原措辞会有歧义）；§3.6 `transactions` 关键约束逐字段列出并加入 `base_currency`；§6 新增 3 条验收（逐行冻结、汇总分组、`SUM` 必带 `GROUP BY`） |
