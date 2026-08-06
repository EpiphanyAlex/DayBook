---
title: 00 地基 Foundation — 数据层、SQLite schema、迁移与错误契约
status: draft
owner: "@alex"
date: 2026-08-06
version: v0.1
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
| 汇率 | **定点整数**：存 `rate_ppm`，即「1 单位原币 = 多少本位币」× 1_000_000，`INTEGER` |
| 舍入 | 本位币金额 = `round_half_even(原币金额 × rate_ppm / 1_000_000)`。**舍入在写入前完成一次，结果落库**，不在读取时重算 |
| 浮点 | **任何位置禁止**——包括中间计算、IPC 传输、测试夹具 |

**自洽约束**：写入交易时校验 `本位币金额 == round_half_even(原币金额 × rate_ppm / 1_000_000)`，不满足则拒绝写入并返回 `data.money_inconsistent`。单币种交易 `rate_ppm = 1_000_000`，**不设特例分支**。

### 3.5 迁移

- 编号 SQL 文件：`src-tauri/migrations/0001_core.sql`、`0002_*.sql`…（**待建**）
- 用 SQLite 的 `PRAGMA user_version` 记录已应用到第几号，启动时顺序补齐
- **幂等**：重复启动不重复应用
- **只前进不回滚**：不实现 down migration。开发期改 schema 就改迁移文件并重建本地库；v1 发布后只加新编号
- 磁盘 `user_version` **超前**于代码已知的最大编号（用户降级了应用）⇒ 报 `data.migration_drift` 并拒绝打开，**不静默继续**

### 3.6 表结构

**M0 需要的四张**（其余在后续里程碑随对应 sub-PRD 建）：

| 表 | 用途 | 关键约束 |
|---|---|---|
| `sources` | 一份被导入的原始材料 | `content_hash` 唯一（导入幂等，见 [02 导入](./02-ingest.md)）；`declared_total_minor` 可空（来源自己声明的合计，供总额校验） |
| `draft_transactions` | AI 起草的待确认交易 | **`source_id` 非空**、**`evidence_text` 非空**（[ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 2，数据层强制） |
| `transactions` | 已确认交易（事实表） | 三元组三字段均非空；`confirmed_at` 非空 |
| `audit_log` | append-only 变更记录 | 字段：`actor`（`agent` / `human`）、`at`、`entity_type`、`entity_id`、`action`、`before_json`、`after_json` |

**完整表清单**（v1 终态，各表的详细字段由对应 sub-PRD 在开工前补进本文 §3.6）：

```
sources · draft_transactions · transactions · draft_items · items
memory_rules · audit_log
```

**硬性结构保证**（违反即缺陷，见 [`docs/architecture.md` §3](../architecture.md)）：

1. `draft_*` 与事实表**结构分离**，不共用同一张表加状态字段
2. 草稿表的 `source_id` 与 `evidence_text` 为 `NOT NULL`
3. 代码中**不存在**针对 `audit_log` 的 `UPDATE` / `DELETE` 语句

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
- 地基自身的 `code` 集：`data.storage_failure` · `data.migration_drift` · `data.money_inconsistent` · `data.not_found` · `data.invalid_argument` · `data.icloud_path_rejected`

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
| R1 | 本位币是固定单一（AUD）还是可切换——影响 `transactions` 是否需要 `base_currency` 字段（[`docs/PRD.md` §13](../PRD.md) 开放问题 P2） | 本文 §3.6、[04 交易](./04-transactions.md) | @alex，**M2 批量与多币种开工前必须决**；M0 期间按「固定 AUD 但字段留着」实现 |
| R2 | Rust↔TS 类型漂移；是否引入 `tauri-specta` 之类 codegen | 本文 §3.8 | 漂移造成第一个真实 bug 时开 spike，@alex 决 |
| R3 | 舍入规则选 half-even 尚未经真实对账验证——若某来源自身用 half-up，总额校验会系统性差几分 | 本文 §3.4、[03 审核与草稿区](./03-review.md) 的总额校验 | M2 处理真实 10 天数据时实测，**结果必须回流本文** |
| R4 | 证据目录长期累积到 GB 级后的清理策略（[`docs/PRD.md` §13](../PRD.md) 开放问题 P3） | 本文 §3.2、[02 导入](./02-ingest.md) | 真实使用出现容量问题时 |
| R5 | 数据库 at-rest 加密——v1 不做（数据不出本机 + macOS FileVault 已提供一层）。若未来需要，`rusqlite` 的 `bundled-sqlcipher` 是路径 | 全产品安全姿态 | v1 明确不做，登记以免被沉默填掉 |

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
- [ ] `cargo test foundation::draft_requires_evidence` 通过——`source_id` 或 `evidence_text` 为空时插入 `draft_transactions` 失败
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
