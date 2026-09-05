---
title: 00 地基 Foundation — 数据层、SQLite schema、迁移与错误契约
status: review
owner: "@maintainer"
date: 2026-09-02
version: v0.23
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
├── preferences.json   ← 本机偏好（M0：当前本位币），不属于账本事实
└── evidence/           ← 证据原件（截图），普通目录，用户可自行翻看
    └── <yyyy>/<mm>/<source_id>.<ext>
```

- 位置必须是**用户看得见、能备份**的地方
- **M0 落点（2026-08-13 实施回流）**：macOS 使用 Tauri `data_dir()/Daybook`，即 `~/Library/Application Support/Daybook/`；首次告知页显示完整路径，并提供「在访达中显示」，从而让默认位置无需用户配置也能被看见和备份
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
| 金额 | **整数最小货币单位**。SQLite `INTEGER`，Rust `i64`，**IPC 上是十进制字符串**，TS 侧解析成安全整数 `number`——见下方「金额怎么过 IPC」。**「最小单位」是币种的属性，不恒等于「分」**——见「币种精度」 |
| 币种 | ISO 4217 三字母大写码，`TEXT`（`AUD` / `CNY` / …） |
| **本位币** | **可切换**，逐笔存 `base_currency`（同为 ISO 4217 码）——见下方「本位币切换语义」 |
| 汇率 | **定点整数**：存 `rate_ppm`，即「1 **主**单位原币 = 多少 **主**单位本位币」× 1_000_000，`INTEGER` |
| 舍入 | `round_half_even`，**在写入前完成一次、结果落库**，不在读取时重算。公式见下方「换算公式」 |
| 浮点 | **任何位置禁止**——包括中间计算、IPC 传输、测试夹具 |

#### 币种精度（2026-08-10 补，依据 [ADR-0004 §2](../adr/0004-data-model-sqlite-integer-money.md)）

**不是所有货币都是两位小数**：ISO 4217 的 minor unit exponent 多数为 2，但 **JPY / KRW 为 0**、**KWD / BHD / JOD 为 3**。此前本节把「最小货币单位」直接写作「分 / cent」，跨 exponent 币种会**整整差 100 倍**——这与 [`docs/PRD.md` §3.1](../PRD.md)「解析能力与国家/币种无关」直接冲突：在 schema 层写死两位小数，就是又一次把地域假设塞回底座。

- **exponent 不入库。** 它是币种的全局属性，由 Rust 侧一张 ISO 4217 常量表给出：`fn currency_exponent(code: &str) -> Result<u32, AppError>`
- **表里没有的币种是非法数据，不是待猜的数据**：返回 `data.unsupported_currency`，**拒绝这一条的写入**（2026-08-10 改，见下）
- **格式化除以 `10^exponent`，不是除以 100。** 全仓库不得出现写死的 `/ 100`（[`.claude/rules/money-and-data.md` §1](../../.claude/rules/money-and-data.md)）
- 逐行存 exponent 是**被否决的**：同币种的 exponent 是常量，逐行存等于允许两行 `CNY` 精度不同——一个只会被写错、不会被用到的自由度

> **未知币种为什么必须报错而不是回退到 2**（2026-08-10 改，此前本节写「按 2 处理并写一条 `trace` 告警」）：币种字段的取值域**已经被定义为 ISO 4217**（§3.4 第一张表）。落在域外的值只有三种来源——**agent 把币种符号读错、拼错、或读到一个我们的表还没收的新代码**，三种都是「这条数据不可信」，没有一种是「按 2 猜一下也行」。
>
> 而回退 + 告警的实际后果是**一条带告警但已经入账的错误金额**：告警落在日志里没人看，金额进了草稿、过了总额校验（同一个错误 exponent 两边都用，还可能恰好自洽）、被人一眼扫过去确认入库。**这正是本项目反复在消灭的那一类缺陷**——「有记录但没拦住」和「没记录」对用户是同一件事。
>
> 落点：`draft_transaction` / `report_source_total` 收到未知币种 → `agent.tool_rejected`（detail 带该币种码），**不写库**；已入库数据不受影响。用户看到的是「这条读到的币种 `XBT` 不是有效的 ISO 4217 代码」，可在审核界面改——**比一个悄悄差 100 倍的数字好得多**。

#### 金额怎么过 IPC（2026-08-10 定死，产品决定）

**问题**：本节 v0.1–v0.6 写「TS `number`」，而 [`CLAUDE.md`](../../CLAUDE.md) 约束 6 写「**任何位置**禁止用浮点数表示金额，**包括 IPC 传输**」。**JS 的 `number` 就是 IEEE-754 双精度，这两句话直接打架。** 分支类型（branded type）挡得住「分」和「元」混用，挡不住数值精度。

**真实的失败路径只有一条，但它是静默的**：Rust 的 `i64` 序列化成 **JSON 数字**，`JSON.parse` 把超过 `2^53 − 1` 的值**悄悄舍入**——`9007199254740993` 变成 `9007199254740992`，没有异常、没有告警。而这个值不是凭空来的：**agent 把截图上的数字读错成 20 位就会产生它**，正好是本产品最要防的那类错误。

**决定：IPC 上金额与汇率一律是十进制字符串；TS 在边界处显式解析并校验范围。**

| 层 | 表示 |
|---|---|
| SQLite · Rust | `i64` |
| **IPC（两个方向）** | **十进制字符串**（`"168"` / `"-4500"` / `"1000000"`） |
| TS 内部 | `number`，且**保证是安全整数** |

适用字段：`amount_minor` · `base_amount_minor` · `rate_ppm` · `reported_total_minor`，以及将来任何 `i64` 金额类字段。**同一条规则，不留例外**。

**范围不变式**：`|v| ≤ 10^15`。

- **Rust 侧**在序列化前校验，超出返回 `data.amount_out_of_range`（§3.7），**不序列化出去**
- **TS 侧**在 `call<T>` 的解析处再校验一次，超出抛同一个 `code`
- 上限取 `10^15` 而不是 `2^53 − 1`（≈ 9.007 × 10^15）：**留一个数量级的余量**，且它远超任何真实个人账目（按两位小数计是 10 万亿主单位）。**触到这个上限的值必然是解析错误，不是大额交易**

**为什么不是全链路 `bigint`**：那是唯一能让约束 6 字面成立的选项，但 JSON 不原生支持 `bigint`、每个渲染点都要转换、测试夹具变重，而**它多挡住的只有「TS 内部算术溢出安全整数」这一种情况——而前端根本不做金额累加**（汇总一律由 Rust 侧给出，[`.claude/rules/frontend.md` §3](../../.claude/rules/frontend.md)）。**字符串边界把静默舍入变成了一次响亮的错误**，这是那条失败路径的全部。

> **约束 6 的措辞随之改准**：不是「TS 里不存在浮点类型」（做不到），而是——**金额在任何位置都以整数表示与传输；`number` 仅作为安全整数范围内的整数载体使用，范围由 IPC 两侧强制**。[`CLAUDE.md`](../../CLAUDE.md) 约束 6 已同步。**如果哪天范围不变式挡不住了，路径是全链路 `bigint`**，登记为 §5 R9。

#### 换算公式

```
base_amount_minor = round_half_even(
    amount_minor × rate_ppm × 10^exp(base_currency)
    ÷ (1_000_000 × 10^exp(currency))
)
```

- **中间量用 `i128`**，先乘后除，不得先除
- 两边 exponent 相同时退化为 `amount_minor × rate_ppm / 1_000_000`——**绝大多数情况下与 2026-08-10 之前的写法逐位相同**，补的是漏掉的那一项，不是改决定

**自洽约束**：写入交易时按上式校验 `base_amount_minor`，不满足则拒绝写入并返回 `data.money_inconsistent`。原币与本位币相同时 `rate_ppm = 1_000_000` 且两边 exponent 必然相等，**走同一条路径，不设特例分支**。

#### 本位币切换语义（2026-08-07 拍板，[`docs/PRD.md` §13](../PRD.md) P2 已关闭）

本位币**可切换**——固定单一本位币等于在 schema 层重新引入地域假设，与 [`docs/PRD.md` §3.1](../PRD.md) 矛盾。三条规则，均由 [ADR-0004](../adr/0004-data-model-sqlite-integer-money.md)「折算冻结在交易发生那一刻」直接推导：

1. **`base_currency` 逐笔存储在交易行上，确认时冻结**——不是只存一个全局设置。全局设置只决定「新交易默认用哪个本位币」。
2. **切换本位币不改动任何历史行。** 追溯换算需要历史汇率，而历史汇率没有可靠来源（[`docs/PRD.md` §13](../PRD.md) P1）；即便有，改写已确认的事实数据也违反 [ADR-0002](../adr/0002-ai-never-writes-directly.md)（事实表只由人工确认动作写入）。
3. **跨越切换点的汇总按 `base_currency` 分组呈现，不静默相加。** 把两种本位币的金额加在一起会得到一个无意义的数字——与总额校验的 `unavailable` 不伪装成通过是同一条原则。

**M0 设置落点（2026-08-13 实施回流）**：当前本位币不新增第七张业务表，写入数据目录下的 `preferences.json`。首次解析前必须由用户明确选择 ISO 4217 币种；未选择时解析返回 `data.base_currency_required`，不得从系统地区或来源币种静默猜测。任务下达把这个值作为代码侧上下文明确告诉 agent；同币种交易按 `base_amount_minor = amount_minor`、`base_currency = currency`、`rate_ppm = 1000000` 填全。切换偏好只影响之后的新解析，已确认交易逐行冻结不变。

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
sources · parse_attempts · draft_transactions · transactions · draft_items · items
accounts · categories · memory_rules · memory_rule_corrections · draft_memory_hits · audit_log
```

**M0 建其中六张**：`sources` · `parse_attempts` · `draft_transactions` · `transactions` · **`accounts`（骨架）** · `audit_log`。其余随对应 sub-PRD 在后续里程碑建：`categories` 属 [04 交易 §3.3](./04-transactions.md)，M2；`draft_items` / `items` 属 [05 事项](./05-items.md)，M3；`memory_rules` / `memory_rule_corrections` / `draft_memory_hits` 属 [06 记忆](./06-memory.md)，M3。

> **分类列按里程碑分两阶段，不能倒写当前 M0 实况**（产品决定，2026-08-23）：M0/M1 已落地且当前受审的 `draft_transactions.category TEXT` / `transactions.category TEXT` 保持原契约；M2 新增 `categories` 并把当前业务列迁为 `category_id → categories(id)`。因此下方两张 M0 表继续列出 `category TEXT`，它们描述的是**当前已实现切片**，不是 v1 终态。M2 目标、迁移边界与验收见本节「`categories` — 分类（M2）」；[01 Agent 运行时](./01-agent-runtime.md) 的 M0 `draft_transaction` 参数也在 M2 才切换，不能让未来分类实体改写当前五工具验收。

> **`parse_attempts` 为什么在 M0**（2026-08-10 加入，产品决定）：M0 的**全部目的**是度量「视觉模型读真实账单准不准」与「一段口述能不能可靠拆多笔」（[`docs/PRD.md` §9.1](../PRD.md)）。而 `sources.agent_session_id` 只存**最近一次**解析——重试一次就把上一次覆盖掉，失败历史、换过哪个模型、提示词改没改全部丢失，[07 评测 §3.5](./07-eval.md)「区分模型退步与提示词变更导致的回归」这条直接落空。**一张表换掉一次不可解释的 M0 基线，值。**
>
> **`accounts` 骨架为什么也在 M0**（2026-08-10 修正）：`transactions.account_id` 声明为 `REFERENCES accounts(id)`，而 M0 不建 `accounts`——**这在 SQLite 上直接跑不起来**。开了 `PRAGMA foreign_keys = ON` 之后，**即使插入的 `account_id` 是 `NULL`**，只要父表不存在就报：
>
> ```
> no such table: main.accounts
> ```
>
> （sqlite 3.45.3 实测；SQLite 在**写子表时**才解析 FK 的父表，`CREATE TABLE` 阶段不报错，所以问题会推迟到 M0 的第一条 `INSERT INTO transactions` 才炸。）
>
> 两条路：**① M0 建一张最小 `accounts` 骨架**，UI 与业务逻辑仍推 M2；**② M0 不声明 FK，M2 再重建 `transactions` 加上**。**选 ①**——选 ② 就等于承认「现在留列是为了避免返工」那条理由不成立：`transactions` 照样要在 M2 被重建一次，而那正是留列想省掉的事。骨架表的成本是四列和一条迁移语句。

#### `accounts` — 账户骨架（M0 建表，M2 才用）

| 列 | 类型 | 空 | 说明 |
|---|---|---|---|
| `id` | TEXT PK | 非空 | |
| `name` | TEXT | 非空 | 用户自己起的名字（「主力储蓄卡」）。**M0/M1 不产生任何行** |
| `created_at` | TEXT | 非空 | |
| `archived_at` | TEXT | 可空 | 软归档：销户的卡不删，历史交易还挂着它 |

**M0 只建表、不写行、UI 不呈现。** 语义（账户与渠道是两个维度）见 [04 交易 §3.4](./04-transactions.md)。**别在 M0 给它加字段**——真实需要的列（机构、币种、卡号后四位、初始余额）要等 M2 拿到真实账单再定，现在猜等于白猜一遍。

#### `categories` — 分类（M2 建表并迁移）

分类产品语义、默认清单与生命周期的权威出处是 [04 交易 §3.3](./04-transactions.md)；本节只固定跨模块必须共用的 M2 数据边界。

| 列 | 类型 | 空 | 说明 |
|---|---|---|---|
| `id` | TEXT PK | 非空 | 稳定 UUID；默认分类与用户分类同一种实体 |
| `name` | TEXT | 非空 | 展示名；默认值统一**两个汉字**，用户自定义不受此限制。**同名只在同一 `scope` 内唯一**——「礼金」「其他」两侧各有一条（[04 §3.3](./04-transactions.md)「跨 scope 允许同名」） |
| `normalized_name` | TEXT | 非空 | 同 scope 唯一键；具体 Unicode / 空白算法是 [04 §5](./04-transactions.md) R6，M2 实施计划前定 |
| `scope` | TEXT | 非空 | `expense` / `income`，创建后不可修改 |
| `sort_order` | INTEGER | 非空 | 同 scope 的显示顺序；默认分类按 [04 §3.3](./04-transactions.md) 固定顺序种入，用户分类追加在后 |
| `created_at` | TEXT | 非空 | |
| `disabled_at` | TEXT | 可空 | 停用不改历史 |
| `merged_into_id` | TEXT → `categories(id)` | 可空 | 合并墓碑；非空期间不可选择、手工恢复或再次参与分类操作，只有成功撤销写入它的原合并批次才可清空 |

**约束**：`UNIQUE(scope, normalized_name)`；`merged_into_id` 不得等于自身，且源 / 目标 scope 必须相同（跨行约束由触发器或等强度的 domain 事务校验保证）。`draft_transactions.category_id` / `transactions.category_id` 均可空；`transfer` 必须为空，`expense` / `income` 引用的分类 scope 必须匹配。`category_id IS NULL` 是「未分类」，不是一行系统分类，也不能成为合并目标。

**M2 只前进迁移边界**：

1. 在任何建表、重建表或写 `user_version` 之前，对旧 schema 做一次**只读预检**：计算每行的目标 scope / 规范化名称 / 候选分类 ID，并把所有冲突与无法映射行完整写入诊断结果
2. `transfer` 上存在旧分类文本、名称规范化冲突或其他无法映射情况时，预检必须在**零 schema 写入**状态停止；诊断保留稳定行 ID、原 direction 与原分类文本，不静默清空、丢弃或改 direction。用户可执行的修复入口仍由 §5 R13 决定，不能等应用进入半迁移或无法启动后才要求在应用内修改
3. 预检无问题后，新建 `categories` 并按 [04 §3.3](./04-transactions.md) 固定顺序种入默认分类；之后应用启动或升级不得重复补种
4. 非空历史 `category TEXT` 按 `(direction 对应 scope, normalized_name)` 建立或复用分类实体，再回填 `category_id`；空字符串按空值处理
5. 同名文本同时用于支出与收入时生成两个不同 ID；不因名字相同跨 scope 合并
6. `drafted_json` 保留 agent 当时输出的原始分类文本，不回写成 ID；迁移只改当前业务列，证据与审计快照原样保留
7. 重建外键并完成一致性检查后才移除旧 `category TEXT`；迁移事务任一步失败都整体回滚，不带半迁移状态启动

分类合并、拆分与撤销会逐项写 `audit_log` 并共享 `batch_id`；`audit_log.batch_id` 在 M2 增加为可空列，普通单项操作留空。撤销先验证本批受影响对象均未发生后续修改或分类操作，再恢复完整批次前状态：合并撤销可清除本批写入的源 `merged_into_id`；部分 / 完全拆分撤销恢复源与目标原状态，仅由本批新建且恢复引用后仍无引用的目标分类可删除；任一冲突整批拒绝。仅凭审计 before/after 是否足以承载分类操作待确认态，尚未获产品批准；结构化待确认操作的表与 MCP 工具形状必须在 M2 开工前由 [01 Agent 运行时](./01-agent-runtime.md) 与 [03 审核与草稿区](./03-review.md) 共同定案，本节不自行发明新表。

> **2026-08-07 M0 开工评审**：以下 M0 表的字段**逐列定死**。此前本节只写「关键约束」并注明「详细字段由对应 sub-PRD 在开工前补进」——现在正是开工前，不补齐则每次实施各写各的（**零沉默原则**）。**2026-08-10 由四张增至六张**（新增 `parse_attempts` 与 `accounts` 骨架，理由见上）。
> 类型省略处按 §3.3 约定：ID 为 `TEXT`（UUID v4）、时间为 `TEXT`（RFC 3339 UTC）、业务日期为 `TEXT`（`YYYY-MM-DD`）。

#### `sources` — 一份被导入的原始材料

| 列 | 类型 | 空 | 说明 |
|---|---|---|---|
| `id` | TEXT PK | 非空 | |
| `kind` | TEXT | 非空 | **`file`**（截图/PDF）\| **`utterance`**（一段口述或文字的转写结果）。见下方「来源不等于文件」 |
| `content_hash` | TEXT | 非空 | SHA-256 十六进制。**`kind = file` 的导入幂等以此为准**（[02 导入 §3.2](./02-ingest.md)）；`utterance` 也算并存，但**不以它去重**——见下方「口述的幂等键不是内容哈希」 |
| `idempotency_key` | TEXT | 可空 | **一次提交一个令牌**，由前端生成。`kind = utterance` 时非空，`file` 时为空 |
| `original_filename` | TEXT | **可空** | 用户那份文件的原名，仅供显示。`kind = utterance` 时为空 |
| `ext` | TEXT | 非空 | 规范化小写、不带点。`utterance` 恒为 `txt` |
| `byte_size` | INTEGER | 非空 | |
| `evidence_relpath` | TEXT | 非空 | **相对数据目录**的路径（§3.2）。`utterance` 的转写文本**也落盘成 `.txt`**，与截图同等对待 |
| `imported_at` | TEXT | 非空 | |
| `state` | TEXT | 非空 | 取值集 `imported` / `parsing` / `parsed` / `failed` / `reviewed`，语义与转移规则由 [02 导入 §3.4](./02-ingest.md) 定义 |
| `parse_error_code` | TEXT | 可空 | `state = failed` 时非空，取 §3.7 的 `agent.*` / `ingest.*` 码 |
| `latest_attempt_id` | TEXT → `parse_attempts(id)` | 可空 | **当前受审的是哪次尝试的输出**。历史尝试在 `parse_attempts` 里 |

**唯一性约束**（2026-08-10 改）：

```sql
CREATE UNIQUE INDEX sources_file_hash ON sources(content_hash) WHERE kind = 'file';
CREATE UNIQUE INDEX sources_idem_key  ON sources(idempotency_key) WHERE idempotency_key IS NOT NULL;
```

**列级写入权限**：**agent 对 `sources` 没有任何写入权限**（2026-08-10 改，此前是「只能写 `declared_total_*` 四列」——那四列已移到 `parse_attempts`）。本表全部列只由 Rust 侧代码写。 <!-- legacy -->

##### 声明合计归尝试，不归来源（2026-08-10 改定）

本节 v0.3–v0.6 把 `declared_total_*` 放在 `sources` 上。**放错地方了**：那四个值不是原件自身的属性，**是某一次 agent 解析的输出**——和草稿完全同源、同样可能读错。放在来源上，它的生命周期就和产出它的那次尝试脱钩，三种坏情况： <!-- legacy -->

1. 第一次尝试读出合计写进 `sources` → 随后超时 → **草稿按 `attempt_id` 全部作废，而合计留了下来**
2. 用户重试 → 新尝试调 `report_source_total` → 撞上「一个来源只接受一次成功调用」被拒，或者默默沿用上一次那个**可能本来就读错**的值
3. 一个来源被解析过两次且都成功时，「该来源全部未作废草稿」会**混进两次尝试的输出**，总额校验对着一堆重复条目求和

**改为**：

- **原件属于来源**（`evidence_relpath`、`content_hash`、`kind`），**读出来的东西属于尝试**（草稿、合计、条目数、未解析说明）
- 四列改名并移到 `parse_attempts`：**`reported_total_minor` / `reported_total_currency` / `reported_total_kind` / `reported_total_evidence_text``**
- **总额校验的入参是 `attempt_id`，不是 `source_id`**——只算该次尝试的未作废草稿（[03 审核 §3.3](./03-review.md)）
- **`sources.latest_attempt_id` 决定当前审核的是哪次输出**。重试产生新尝试、新草稿、新合计，旧的一整套原样留在库里供 [07 评测](./07-eval.md) 用
- 「一次成功调用」的约束从**每来源**收窄为**每尝试**——重试本来就该能重新回报合计

> **命名为什么从 `declared_` 改成 `reported_`**：`declared` 读起来像「来源自己声明的」，而这一列存的是 **agent 报告它在来源上看到的东西**。概念上「声明合计」仍指账单底部印着的那个数（[ADR-0002 闸门 3](../adr/0002-ai-never-writes-directly.md)），但**库里存的从来是 agent 对它的一次转述**——和 `evidence_text` 是同一类东西，同一类可信度。名字应该说实话。

##### 合计必须带类型（2026-08-10 新增）

**一个裸数字表达不了「本期消费合计」「本期收入合计」「净变动」的差别**，而这三者对应三条不同的等式（逐条等式在 [03 审核 §3.3](./03-review.md)）。此前 schema 只有金额与币种，于是校验只能对该来源的全部草稿无差别求和——**一张同时含消费与退款的账单，底部印着「本期消费合计」，校验必然对不上**，而它会以 `failed` 的形式报出来，看起来像 agent 读错了。

- 取值集：`expense_total`（本期消费/支出合计）· `income_total`（本期收入合计）· `net_change`（净变动，收入减支出）
- **判不出类型时 agent 不得调 `report_source_total`**（[01 §3.2](./01-agent-runtime.md)），结果如实为 `unavailable`——与「余额不当合计用」同一条原则：**基准的语义不确定，就不是基准**（[ADR-0002 闸门 3](../adr/0002-ai-never-writes-directly.md)）
- **一次尝试只登记一条合计。** 账单同时印了消费合计与收入合计时，M0 只有在其中恰有一条满足下方「当前来源全覆盖」资格时才报告；多条局部 claim 不得挑一条硬塞进四列。「一来源多条 claim」仍登记为 §5 R7，M2 决

##### M0 单 claim 的范围资格：只认当前不可变来源全覆盖（2026-08-30 no-go 回流）

**来源边界是导入后不可变的证据字节，不是它背后的整页、整月或完整账户。** 因此 M0 继续接受任意 viewport 截图；截图裁到哪里，当前来源就到哪里。`report_source_total` 唯一支持的 claim 必须同时满足：

1. 来源上明确印出 / 说出一条 `expense_total`、`income_total` 或 `net_change`；
2. scope 精确等于当前来源中的**全部适用交易**：`expense_total` 覆盖全部支出，`income_total` 覆盖全部收入，`net_change` 覆盖全部收入与支出；M0 现有校验遇到 `transfer` 仍为 `unavailable`，不把缺失的转入 / 转出符号编出来；
3. scope 不包含当前不可变来源之外的交易，也不只覆盖当前来源内的一个局部组；
4. 该 claim 的 amount/currency/kind 三元组在来源全部合计候选中唯一。若一个有效总计与按日 / 子组 decoy 恰好三元组相同，现有四列无法审计 agent 选中了哪一条，M0 保守地不报告；这不是新增多 claim schema。

下列全部 **scope-invalid**，agent 不得调用 `report_source_total`：覆盖截图 viewport 外交易的月度 / 周期合计；分页列表的跨页合计；按日、按分类、语义上只属于单笔的金额 / 小计；任意其他子组合计。`总共` / `一共` / `合计` / `总计` / `TOTAL` 只是**候选信号**，不是范围资格证明。口述里的「一共 1500，其中 300 水电、1200 房租」若整段还有其他交易，1500 只是子组 claim，不能报告。

**不为这次修正新增生产 schema。** `parse_attempts.reported_total_*` 仍是四列 all-or-nothing、一次尝试至多一条；不新增 scope 列，不提前建立 `reconciliation_claims`。生产代码没有独立 OCR，无法从截图字节证明 agent 选中的 claim 是否全覆盖；M0 先由提示词收窄输出，再由 [07 评测 §3.4](./07-eval.md) 的人工真值列出 bounded `candidateClaims` 并标注 exact expected claim（amount/currency/kind）与「scope-invalid 成功报告数必须为 0」正式契约检验；同图有效总计旁有 decoy 小计时，错报 decoy 也必须被抓住。未满足资格时四列保持全空，按 [03 审核 §3.3](./03-review.md) 现有 `kind` 规则得到 `unavailable` 或 `not_applicable`。

##### 口述的幂等键不是内容哈希（2026-08-10 改定）

本节 v0.3–v0.5 曾写「`utterance` 的幂等键仍是内容哈希，同一段话说两次判为重复，这是刻意的」。**该理由只在几分钟的尺度上成立，跨天就变成静默数据丢失**：连续两天各说一句「今天咖啡 5 元」，文本逐字相同 → 第二天那笔被判为重复来源、**直接消失**，而用户不会发现——他刚说完，界面显示「这段已经导入过」。

**丢一笔真实交易，比多一批可以一键丢弃的重复草稿严重得多。** 因此改为：

1. **`content_hash` 的唯一约束收窄到 `kind = 'file'`。** 文件有客观内容身份，同一张图导两次确实是同一份证据
2. **`utterance` 的幂等由 `idempotency_key` 保证**——前端在**一次提交**里生成一次令牌，重试、双击、崩溃重放都带同一个令牌，因此不会重复落库；隔天再说一遍是**新的一次提交**，产生新来源
3. **文本重复只提示，不阻止**：新来源的 `content_hash` 与既有 `utterance` 相同时，UI 提示「你之前也说过同样的话」并列出那一条，由用户决定——**判断留给人**（[ADR-0003 §5](../adr/0003-agent-runtime-and-pluggable-backend.md)）

##### 来源不等于文件（2026-08-08 设计评审）

`kind` 这一列存在的理由：**闸门 2 要的不是「文件」，是「痕迹 + 原文」**。一段口述就是痕迹，转写文本就是原文。

此前 `sources` 隐含假设来源必然是文件，导致一个硬墙——`draft_transactions.source_id` 非空，而口述没有文件，于是**「今天吃饭 180」这句话在结构上无法起草**。而 [ADR-0003 §2](../adr/0003-agent-runtime-and-pluggable-backend.md) 论证「不按业务领域拆 agent」时举的例子正是一句跨实体的口述，ADR 自己的论据落不了地。

三条随之确定：

1. **转写文本落盘成 `.txt`，与截图同等对待。** 这样 `evidence_relpath` 保持非空，闸门 2 的实现路径对两种来源完全一致，不产生分支。
2. **`utterance` 来源通常没有 scope-valid 合计**（2026-08-30 收窄）—— 一段口述一般没有来源级聚合，此时 `reported_total_*` 全空、对账结果 `not_applicable`、确认策略 `user_attested_batch`（[03 审核 §3.3](./03-review.md)）。只有用户说出的合计满足上方「当前不可变来源全部适用交易」资格时，agent 才可调用 `report_source_total`，对账结果变成 `passed` / `failed`；**词面上出现「总共」不等于 scope 合格**。覆盖月度外部范围、单笔或子组的口述合计仍保持全空。
   **确认策略不随之改变**：无论对账做没做成，`utterance` 的策略恒为 `user_attested_batch`。**这正是把两个维度拆开的理由**（[03 审核 §3.3](./03-review.md)「两个维度，不是一个枚举」）。
   **闸门 3 对语音来源通常不适用**；代之以「整段原文 + 全部拆分结果并排展示 + 一次人工确认」这道闸门。只有存在 scope-valid 来源级 claim 时两道都在。
3. **`utterance` 的幂等键是提交令牌，不是内容哈希**（2026-08-10 改定）—— 见上方「口述的幂等键不是内容哈希」。原写法会让「今天咖啡 5 元」这类跨天重复的真实交易被静默吞掉。

#### `parse_attempts` — 一次解析尝试（2026-08-10 新增）

**一次解析任务 spawn = 一行**，无论成败。它是「这条草稿是谁在什么条件下产出的」的唯一答案，也是 [07 评测](./07-eval.md) 的归因依据。

> **工具集探测的那次 spawn 不算**（2026-08-10 澄清）：[01 §3.7](./01-agent-runtime.md) 的有效工具集探测是一次独立的、短命的子进程，**它不解析任何来源，因此不产生 `parse_attempts` 行**。「一次 spawn 一行」说的是**解析任务**的 spawn。探测失败时不新增行，与 [01 §6](./01-agent-runtime.md) 的 `unsealed_surface_blocks_task` 一致。

| 列 | 类型 | 空 | 说明 |
|---|---|---|---|
| `id` | TEXT PK | 非空 | |
| `source_id` | TEXT → `sources(id)` | 非空 | |
| `agent_session_id` | TEXT | 非空 | 一个来源一次尝试一个会话（[01 §5](./01-agent-runtime.md) R5） |
| `backend_id` | TEXT | 非空 | `claude-code` / `codex` / …（[01 §3.5](./01-agent-runtime.md)） |
| `backend_version` | TEXT | 可空 | 后端 CLI 自报的版本；取不到为空 |
| `model_id` | TEXT | 可空 | 后端报告的模型标识 |
| `prompt_hash` | TEXT | 非空 | 本次所用提示词模板的 SHA-256——**提示词是程序记忆**（[01 §3.6](./01-agent-runtime.md)），改了必须能看出来 |
| `tool_surface_version` | TEXT | 非空 | **我们期望的**工具面版本，由代码给出 |
| `effective_capability_hash` | TEXT | 非空 | **实测到的** capability manifest 指纹（[01 §3.7](./01-agent-runtime.md)）。**覆盖工具型与非工具型两类条目**——hook / 插件 / 权限模式没有名字、没有参数 schema，但同样是能力。**只哈希「工具名 + server + 参数 schema」不够**：那样一个改写每次调用的 `PreToolUse` hook 挂上去，指纹一个字节都不变 |
| `app_version` | TEXT | 非空 | |
| `started_at` · `ended_at` | TEXT | 起非空 / 止可空 | `ended_at` 为空 = 进行中或崩溃残留（[02 导入 §3.4](./02-ingest.md) 启动扫描） |
| `outcome` | TEXT | 可空 | `completed` / **`completed_with_gaps`** / `failed` / `timeout` / `interrupted` / `cancelled` / `protocol_violation`；进行中为空 |
| `error_code` | TEXT | 可空 | `outcome` 非 `completed*` 时取 §3.7 的 `agent.*` 码 |
| `reported_item_count` | INTEGER | 可空 | agent 经 `complete_source` 自报的条目数（[01 §3.2](./01-agent-runtime.md)） |
| `unparsed_note` | TEXT | 可空 | agent 自报的「有哪块我没读」——**空字符串与 NULL 语义不同**：前者是「它说全读了」，后者是「它没说」 |
| **`reported_total_minor`** | INTEGER | 可空 | **agent 报告它在当前不可变来源上看到、且满足本节「全部适用交易」范围资格的合计**（此前叫 `sources.declared_total_minor`，2026-08-10 移入本表） | <!-- legacy -->
| **`reported_total_currency`** | TEXT | 可空 | 该合计的币种——**没有币种的金额无法校验** |
| **`reported_total_kind`** | TEXT | 可空 | `expense_total` / `income_total` / `net_change`，见 `sources`「合计必须带类型」 |
| **`reported_total_evidence_text`** | TEXT | 可空 | 合计在来源上的原文片段——**校验基准本身也必须可核对**（[03 审核 §3.3](./03-review.md)） |

**CHECK 约束**：`reported_total_*` **四者要么全空、要么全非空**。缺任一即视为「本次尝试未取到合计」。

**列级写入权限**：agent 经 MCP 工具对**本表**只能写六列——`reported_total_*` 四列（`report_source_total`）与 `reported_item_count` / `unparsed_note`（`complete_source`），且**只能写自己那一行**；本表其余列由 Rust 侧代码写。

> **agent 在整个库里能写的地方就两处**（2026-08-10 说全）：**① `draft_*` 表**（业务实体输出，经 `draft_transaction` / `draft_item`）；**② 本表这六列**（协议与对账元数据）。**其余每一张表、每一列都不可达**，事实表尤其（[ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 1）。本行此前写「其余列与**全部其他表**由 Rust 侧代码写」，字面上把 `draft_*` 也排除了——那和闸门 1 的写法自相矛盾。

> **`outcome` 的三档「结束了」要分清**（2026-08-10）：
>
> - **`completed`** —— 调过 `complete_source`、自报条目数与实际草稿数一致、`unparsed_note` 为空字符串。**只有这一档是干净的**
> - **`completed_with_gaps`** —— 同上，但 `unparsed_note` 非空：agent 自己说了「有一块我没读」。**草稿可用，但 UI 必须显眼提示**，不能和 `completed` 长一个样。**它是这条来源在审核界面里的一个警告，不是一次成功**
> - **`protocol_violation`** —— 没调 `complete_source` 就退出，或反复调用后自报条目数仍与实际不符（[01 §3.2](./01-agent-runtime.md)）。**不判为 `parsed`**（[02 导入 §3.4](./02-ingest.md)）
>
> **为什么 `reported_item_count` 与实际草稿数都要留**：两者不等本身就是信号——agent 说「我起草了 12 条」而库里只有 9 条，说明有 3 次工具调用被拒或它在说谎。**这个信号现在会当场变成一次工具调用失败**（[01 §3.2](./01-agent-runtime.md)），两个数仍要留档供 [07 §3.3](./07-eval.md) 用。

#### `draft_transactions` — AI 起草的待确认交易

| 列 | 类型 | 空 | 说明 |
|---|---|---|---|
| `id` | TEXT PK | 非空 | |
| `source_id` | TEXT → `sources(id)` | **非空** | [ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 2，数据层强制 |
| `evidence_text` | TEXT | **非空** | 同上。**这是 agent 的抽取声明，不是独立证据**——见下方「`drafted_json`」 |
| `source_ordinal` | INTEGER | **非空** | **这条在原件上是第几条**（1 起，自上而下 / 口述中出现的先后）。见下方「位置是 agent 报的，不是我们算的」 |
| `evidence_span_start` · `evidence_span_end` | INTEGER | 可空 | `evidence_text` 在转写文本里的位置，**坐标定义见下方「span 用哪套坐标」**。**`kind = utterance` 时必填，`file` 时恒空** |
| `attempt_id` | TEXT → `parse_attempts(id)` | 非空 | 溯源到具体哪次尝试。后端、模型、提示词哈希全在那张表上，**不在本表重复一份** |
| `drafted_json` | TEXT | **非空** | **agent 首次写入时的完整字段快照，写入后永不更新**——见下方 |
| `voided_at` | TEXT | 可空 | 非空 = 本行已作废（超时/中断的补偿性作废，[01 §3.4](./01-agent-runtime.md)）。**作废不删行** |
| `discarded_at` | TEXT | 可空 | 非空 = **人主动丢弃**这条草稿。它与协议失败产生的 `voided_at` 语义不同，不能复用同一列；丢弃同样不删行 |
| `occurred_on` | TEXT | 非空 | 业务日期 |
| `amount_minor` | INTEGER | 非空 | 原币金额 |
| `currency` | TEXT | 非空 | 原币币种 |
| `base_amount_minor` | INTEGER | **可空** | 见下方「草稿阶段的三元组可空」 |
| `base_currency` | TEXT | 可空 | 同上 |
| `rate_ppm` | INTEGER | 可空 | 同上 |
| `direction` | TEXT | 非空 | `expense` / `income` / `transfer` |
| `merchant` | TEXT | 非空 | **原文**，不做归一化覆盖（[04 交易 §3.1](./04-transactions.md)） |
| `category` | TEXT | 可空 | **M0/M1 阶段列**，起草时未必判得出；M2 迁为 `category_id → categories(id)`，见本节「`categories` — 分类」 |
| `channel` | TEXT | 可空 | 同上 |
| `confidence` | INTEGER | 可空 | agent 自评 0–100，供 [03 审核 §3.4](./03-review.md) 异常前置排序 |
| `created_at` | TEXT | 非空 | |
| `consumed_at` | TEXT | 可空 | **非空 = 已被确认消费**；[03 审核 §3.1](./03-review.md)「标记为已消费而非删除」的落点 |

> **位置是 agent 报的，不是我们算的**（2026-08-10 新增）：[07 评测 §3.2](./07-eval.md) 的条目对齐要求预测侧也有位置，而它上一版写的是「草稿按 `evidence_text` 在原件上的位置排序」——**那做不到**：`file` 来源是一张 PNG，系统里**没有 OCR、没有坐标**，我们无从知道那段文字在图上哪里。这与同一份文档里「`evidence_text` 的子串断言对图像来源无法实现」是同一个事实的两面，上一版只认了一半。
>
> **因此位置由 agent 在起草时一并报告**，成为工具的必填参数（[01 §3.2](./01-agent-runtime.md)）：
>
> - **`source_ordinal` 两种来源都必填**——对齐算法因此只有一条路径，不按 `kind` 分支
> - **`utterance` 另外必填字符区间**（`evidence_span_*`），它同时兑现 [07 §3.3](./07-eval.md) 的子串断言与 [03 审核 §3.2](./03-review.md) 的原文高亮；`file` 的区域定位仍是 [03 §5](./03-review.md) R1 的 spike 对象
> - **`(attempt_id, source_ordinal)` 唯一**：两条草稿都声称自己是第 3 条，是协议错误 → `agent.tool_rejected`
> - **不要求连续**：`1, 2, 4` 合法——agent 跳过了读不动的一行，那该写进 `unparsed_note`（[01 §3.2](./01-agent-runtime.md)）。**跳号是信号，不是错误**，它进 [07 §3.3](./07-eval.md) 的 transcript 维度
>
> **这仍然是 agent 自报的数，我们无法独立核验**——但它换来三件事：eval 的对齐**写得出来了**、审核界面能稳定按来源顺序排（[03 §3.4](./03-review.md) 的排序之下仍需一个确定的原始序）、以及 ordinal 本身出错时**它是一个可观测的 transcript 错误**，而不是一个静默的对齐失败。

##### span 用哪套坐标（2026-08-10 定死）

上一版只写「字符区间」。**「字符」在这条链路上有四种互不相同的含义**，而这条链路正好横跨 Rust 与 TypeScript：

| 坐标系 | 谁的默认 | 「今天喝了☕️ 5 元」里 `5` 的起点 |
|---|---|---|
| UTF-8 字节偏移 | Rust 的 `&str[..]` 索引 | 一个数 |
| UTF-16 code unit | JS 的 `String.prototype.slice` | 另一个数（emoji 占 2） |
| Unicode code point | Rust `char` / JS `Array.from` | 又一个数 |
| grapheme cluster | 人眼看到的「一个字」 | 再一个数（☕️ 的变体选择符会合并） |

**中文夹一个 emoji 就会立刻错位**，而错位的表现是「高亮选错了半句话」——看起来像模型报错了位置，实际是两端各按自己的默认解释了同一个数字。**这类缺陷不会在写代码时暴露，会在用户第一次说带 emoji 的话时暴露。**

**决定**：

- **零起、左闭右开** `[start, end)`
- **单位是 Unicode scalar value（code point）**——不是字节、不是 UTF-16 code unit、不是 grapheme
- **计量对象是那份原样落盘、未经任何 normalize 的转写文本**（`evidence_relpath` 指向的 `.txt` 内容逐字节那一份）。**不许先做 NFC/NFKC 再算**——归一化会改变 code point 数量
- **两端的实现方式定死**：Rust 用 `.chars()`，TypeScript 用 `Array.from(text)`。**不许用各语言的原生字符串索引**（`&s[a..b]` / `s.slice(a,b)`），那正是四套坐标混进来的入口

**写入时强制校验两条**，不满足 → `agent.tool_rejected`：

```
0 ≤ start < end ≤ code_point_length(转写文本)
slice_by_code_points(转写文本, start, end) == evidence_text
```

> **第二条顺带解决了一件别的事**：它让 `utterance` 的 `evidence_text` **变成可独立核验的**——我们手里有原文，能逐字比对 agent 声称读的那一段是否真的在那儿。**这是 `file` 来源不具备的**（没有 OCR，无从比对，[03 审核 §3.2](./03-review.md)），所以口述路径的证据链实际上比截图路径强一档。**别把这条便宜当成两种来源都有。**

**若实测发现模型报不准精确数字**（这是可能的），退路是把工具参数改成 `evidence_text` + **`evidence_occurrence`**（第几次出现），由 Rust 在原文里算出 span。**那是一次工具形态变更，M0 第一轮 eval 之后才有数据支撑，现在不做**——登记为 §5 R10。

> **`drafted_json` 为什么必须存**（2026-08-10 新增）：[03 审核 §3.5](./03-review.md) 允许**行内编辑直接改草稿**，改完这一行就等于人的答案。此前 [07 评测 §3.2](./07-eval.md) 声称「草稿保留原始起草值」——**那是错的**：一旦用户把 1680 改回 168，「草稿 ← 交易」这条 join 两边一模一样，eval 看到的错误率恒为零，`audit_log` 里那条 `before_json` 是唯一残存的真相，而靠遍历审计链重建原始草稿既慢又脆。
>
> 一列不可变快照同时解决三件事：eval 的真值、审计的「AI 当初起草成什么样」（[03 §3.1](./03-review.md) 此前只由 `source_draft_id` 兑现一半）、以及 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 硬性要求 7。
>
> **写入规则**：agent 经 MCP 工具插入时由 **Rust 侧**序列化当次参数写入，**不由 agent 提供**；此后任何 `UPDATE draft_transactions` 都不得触及该列（与 `audit_log` 的 append-only 同级的硬约束）。
>
> **后端与模型标识为什么不在本表**（2026-08-10 移走）：它们此前逐条重复存在每条草稿上，同一次解析的 40 条草稿写 40 遍同样的 `backend_id`。现在归 `parse_attempts` 一行，草稿只留 `attempt_id`——**同一个事实只有一个出处**（[`CLAUDE.md`](./CLAUDE.md) 硬规则 5）。eval 的归因照旧，多一次 join。

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
| `merchant_normalized` · `category` · `channel` · `note` | TEXT | 可空 | `category` 是 **M0/M1 阶段列**；M2 迁为可空 `category_id → categories(id)` |
| `account_id` | TEXT → `accounts(id)` | 可空 | **哪张卡 / 哪个账户**（[04 交易 §3.4](./04-transactions.md)）。**M0/M1 恒为空，M2 起启用**——见下方「账户与渠道是两个维度」 |
| `confirmed_at` | TEXT | 非空 | 事实表的行必然经人确认 |
| `deleted_at` | TEXT | 可空 | **软删除**（[04 交易 §3.5](./04-transactions.md)）；非空的行不进任何汇总 |

**CHECK 约束**：`source_draft_id` 非空 ⇒ `source_id` 非空（来自草稿必有来源）；`source_id` 非空 ⇔ `evidence_text` 非空。

##### 账户与渠道是两个维度（2026-08-10 新增，产品决定）

[`docs/PRD.md` §2](../PRD.md) 说痛感随「**账户数** → 支付渠道数 → 币种数」递增、多账户用户是首要验证场景。但此前 schema 里只有 `channel` 一个维度，取值示例是 `bank_debit` / `wallet` 这类**支付方式类别**——「我这笔刷的是**哪张**卡」在结构上无法表达。后果不止是回顾少一个维度：[02 导入 §3.6](./02-ingest.md) 的跨图去重（同一笔在信用卡账单与银行流水各出现一次）与 [04 交易 §5](./04-transactions.md) R3 的转账双边，**都需要「这两条属于不同账户」这个信息才能判**。

- `channel` = **支付方式类别**（`bank_debit` / `bank_credit` / `wallet` / `cash`）
- `account_id` = **具体账户或卡**，用户自己维护。**`accounts` 骨架表在 M0 就建**（见上方「`accounts` — 账户骨架」），字段补全与 UI 在 **M2**
- **两者正交**，不是一个维度的粗细两档：同一个账户可以有多种支付方式，同一种支付方式跨多个账户
- **M0/M1 不实现业务**：列与骨架表都建好但恒为空，保证 M2 补字段时不用改 `transactions` 的既有行

> **为什么现在就留列而不是 M2 再加**：只前进迁移（§3.5）下加一列本来就不贵，但 M0/M1 期间的回顾查询、去重逻辑、导出格式**会按「没有账户维度」这个前提写**，等 M2 再回头补是把同一批代码改两遍。留列的成本是一列 NULL，不留的成本是一次返工。

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
| `batch_id` | TEXT | 可空 | **M2 新增**；分类合并 / 拆分 / 整批撤销的逐项审计共享一个 UUID，普通操作为空 |

> **`actor` 为什么必须有 `system`**：超时作废草稿（[01 Agent 运行时 §3.4](./01-agent-runtime.md)）、状态机推进、迁移——这些由代码执行，既不是 agent 也不是人。只给 `{agent, human}` 会逼实现者往里塞一个语义错误的值。

**硬性结构保证**（违反即缺陷，见 [`docs/architecture.md` §3](../architecture.md)）：

1. `draft_*` 与事实表**结构分离**，不共用同一张表加状态字段
2. 草稿表的 `source_id` 与 `evidence_text` 为 `NOT NULL`
3. 代码中**不存在**针对 `audit_log` 的 `UPDATE` / `DELETE` 语句
4. agent **对 `sources` 无写入权限**；对 `parse_attempts` 收窄到 `reported_total_*` 四列 + `reported_item_count` + `unparsed_note`，且**只能写自己那一行**（2026-08-10 改）
5. 代码中**不存在**任何会写 `drafted_json` 的 `UPDATE` 语句（2026-08-10 新增，[ADR-0002](../adr/0002-ai-never-writes-directly.md) 硬性要求 7）
6. `(attempt_id, source_ordinal)` 上有唯一索引；`kind = utterance` 的草稿 `evidence_span_*` 非空、`file` 的恒空（CHECK）
7. 金额与汇率**在 IPC 上是字符串**，两侧各有一次范围校验（2026-08-10 新增，§3.4「金额怎么过 IPC」）
8. `draft_transactions.source_id` 必须等于其 `attempt_id` 所属的 `parse_attempts.source_id`；`sources.latest_attempt_id` 非空时，该尝试也必须属于同一个来源。SQLite 的行内 CHECK 无法跨表表达，迁移用触发器（或等强度的数据层事务检查）强制
9. `voided_at` 只表达系统对失败尝试的补偿性作废，`discarded_at` 只表达人主动丢弃；两者都保留历史行，审核列表只展示二者均为空且未消费的草稿
10. **M2 分类约束**：`transfer.category_id IS NULL`，支出 / 收入的分类 scope 与 direction 相同；停用或合并墓碑不接受新引用。只有通过原批次撤销事务才可清除该批写入的墓碑并恢复原引用。合并 / 拆分 / 撤销逐项追加审计并共享 `batch_id`，不得改 `drafted_json`

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
| `data.unsupported_currency` | 00 | 币种码不在 ISO 4217 表内（§3.4「币种精度」）。**不回退到 exponent 2**——带告警入账的错误金额比拒绝更糟（2026-08-10 新增） |
| `data.amount_out_of_range` | 00 | 金额或汇率超出 `\|v\| ≤ 10^15`（§3.4「金额怎么过 IPC」）。**IPC 两侧各校验一次**（2026-08-10 新增） |
| `data.not_found` | 00 | 实体不存在 |
| `data.invalid_argument` | 00 | 参数形状错误 |
| `data.icloud_path_rejected` | 00 | 数据目录在 iCloud 容器内（§3.2） |
| `ingest.duplicate_source` | [02](./02-ingest.md) | 内容哈希命中已有来源（§3.2 幂等；**非错误语义**，见 02 §3.2 的返回契约） |
| `ingest.unsupported_format` | [02](./02-ingest.md) | 文件格式不在支持集内 |
| `ingest.evidence_write_failed` | [02](./02-ingest.md) | 证据落盘失败（此时不写 `sources` 行） |
| `ingest.invalid_state_transition` | [02](./02-ingest.md) | 非法状态转移（§3.4） |
| `agent.backend_unavailable` | [01](./01-agent-runtime.md) | 安装资格检查未发现合格 CLI：没有候选路径、候选不可执行，或版本无法在限定时间内成功读取（[01 §3.5](./01-agent-runtime.md)）。**不表示完整 readiness probe 的任意失败**——未认证与能力面不密封分别使用 `agent.not_authenticated` 与 `agent.tool_surface_unsealed` |
| `agent.not_ready` | [01](./01-agent-runtime.md) | **合格 CLI 已发现，但本次应用生命周期内的完整 readiness probe 尚未开始或仍在进行**（2026-08-22 新增，[01 §3.5](./01-agent-runtime.md)）。此时**拒绝创建 `parse_attempts`、拒绝下发解析任务**（fail closed）。状态矩阵里这一档的 `error_code` 是空、UI 显示「正在检查」——**这个码只在用户显式发起解析时由命令层返回**，不写进 `BackendStatus`。probe 已跑完但失败时不用它，用各自的码（`agent.not_authenticated` / `agent.tool_surface_unsealed` / …） |
| `agent.not_authenticated` | [01](./01-agent-runtime.md) | CLI 存在但未登录（2026-08-10 新增）——与「没装」是两种不同的用户动作，UI 要给不同指引 |
| `agent.quota_exhausted` | [01](./01-agent-runtime.md) | 后端报告用量额度耗尽（2026-08-10 新增）。**不重试**（[02 导入 §3.5](./02-ingest.md)），否则在用户不知情时接着烧 |
| `agent.timeout` | [01](./01-agent-runtime.md) | 单次任务超硬超时 |
| `agent.interrupted` | [01](./01-agent-runtime.md) | 上次运行中断的残留（应用崩溃 / 强杀），由启动扫描判定（2026-08-10 补登记；[02 导入 §3.4](./02-ingest.md) 早已在用它，此前**未登记**） |
| `agent.cancelled` | [01](./01-agent-runtime.md) | 用户主动取消本次解析（2026-08-10 新增） |
| `agent.protocol_violation` | [01](./01-agent-runtime.md) | 子进程正常退出但未调 `complete_source`，或反复调用后自报条目数仍与实际不符（2026-08-10 新增，[01 §3.2](./01-agent-runtime.md)）——**不判为解析成功** |
| `agent.unexplained_gap` | [01](./01-agent-runtime.md) | `source_ordinal` 跳号而 `unparsed_note` 为空（2026-08-10 新增，[01 §3.2](./01-agent-runtime.md)）——**跳号必须有说明**。同样是可补救的拒绝 |
| `agent.completion_mismatch` | [01](./01-agent-runtime.md) | `complete_source` 自报条目数 ≠ 实际草稿数（2026-08-10 新增）。**这是可补救的工具级拒绝**，不封闭会话——agent 补齐或修正后可再调（[01 §3.2](./01-agent-runtime.md)） |
| `agent.memory_lookup_incomplete` | [01](./01-agent-runtime.md) | `complete_source` 时发现起草出的商户有未经 `query_memory` 查过的（**M3**，[06 记忆 §3.4](./06-memory.md)）。同样可补救，返回体带缺的键 |
| `agent.tool_surface_unsealed` | [01](./01-agent-runtime.md) | 启动前的 readiness probe **无法证明完整 capability manifest 与预期严格相等**：结构化清单缺失/不可读、缺项、多项，或出现非预期 hook / 插件 / 权限模式（[01 §3.7](./01-agent-runtime.md)）——**拒绝下发任务**，不降级运行 |
| `agent.spawn_failed` | [01](./01-agent-runtime.md) | 子进程起不来 |
| `agent.tool_rejected` | [01](./01-agent-runtime.md) | 工具参数不合法（如缺 `evidence_text`） |
| `review.total_mismatch` | [03](./03-review.md) | 总额校验 `failed` 时批量确认被拒 |
| `review.total_unavailable` | [03](./03-review.md) | 总额校验 `unavailable` 时批量确认被拒 |
| `review.missing_evidence` | [03](./03-review.md) | 确认时草稿缺证据（服务端二次校验） |
| `review.incomplete_triple` | [03](./03-review.md) | 确认时草稿三元组不齐（§3.6「草稿阶段的三元组可空」） |

### 3.8 TS 类型桥

- 前端不手写 `invoke` 调用，统一走一层 `call<T>(command, args)` 包装：把 Tauri 的错误规整成 `AppError` 形状
- **`call<T>` 同时是金额的解析与校验点**（2026-08-10）：IPC 上的金额是十进制字符串（§3.4「金额怎么过 IPC」），在这里解析成 `number` 并校验 `|v| ≤ 10^15`，超出抛 `data.amount_out_of_range`。**组件里不应出现第二处解析**
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
| ~~R1~~ **已关闭（2026-08-07）** | 本位币是固定单一还是可切换 | 本文 §3.4/§3.6、[04 交易](./04-transactions.md) | **结论：可切换**（产品决定，[`docs/PRD.md` §13](../PRD.md) P2 同步关闭）。`base_currency` 逐笔存储并在确认时冻结，切换不改历史行，汇总按本位币分组——三条规则与推导见 §3.4「本位币切换语义」 |
| R2 | Rust↔TS 类型漂移；是否引入 `tauri-specta` 之类 codegen | 本文 §3.8 | 漂移造成第一个真实 bug 时开 spike，产品决定 |
| R3 | 舍入规则选 half-even 尚未经真实对账验证——若某来源自身用 half-up，总额校验会系统性差几分 | 本文 §3.4、[03 审核与草稿区](./03-review.md) 的总额校验 | M2 处理真实 10 天数据时实测，**结果必须回流本文** |
| R4 | 证据目录长期累积到 GB 级后的清理策略（[`docs/PRD.md` §13](../PRD.md) 开放问题 P3） | 本文 §3.2、[02 导入](./02-ingest.md) | 真实使用出现容量问题时 |
| R5 | 数据库 at-rest 加密——v1 不做（数据不出本机 + macOS FileVault 已提供一层）。若未来需要，`rusqlite` 的 `bundled-sqlcipher` 是路径 | 全产品安全姿态 | v1 明确不做，登记以免被沉默填掉 |
| ~~R6~~ **已关闭（2026-08-24）** | 证据区域坐标字段——[03 审核 §5](./03-review.md) R1 若结论是「能稳定定位」，`draft_transactions` 需加坐标列 | 本文 §3.6 | **结论：不加。** 产品密封链路与模型对照均未达到危险误定位 / 相邻行侵入门槛；错误高亮比无高亮更危险。M1 不创建 `0002_*` 坐标迁移、不改 `draft_transaction` 工具形状；完整原件 + `evidence_text` 继续作为安全退路。见 [spike 记录](../spikes/2026-08-24-r1-evidence-region.md) |
| R7（**2026-08-30 no-go 后重述**） | **一次尝试多条、分组或跨范围合计**——月结单、分页与按日 / 分类汇总常同时存在多条 claim；M0 四列既表达不了多个 scope，也无法证明哪条覆盖当前来源全部适用交易 | 本文 §3.6 `reported_total_*`、[03 审核 §3.3](./03-review.md) | M2 拿到独立样本后决。候选仍是 `reconciliation_claims` 子表，但 **M0 修正不提前实现**：仅允许一条 current-source 全覆盖 claim；多条局部 claim 一律不报告。第一次正式 no-go 的 `6/7` 假警报正是提前把这些 claim 塞进单字段的实证（[`docs/PRD.md` §9.4](../PRD.md)） |
| R10（**新增 2026-08-10**） | **模型报不准 span 时的退路**——§3.6「span 用哪套坐标」要求 agent 直接报 code point 区间。若实测发现它经常算错，退路是改成 `evidence_text` + `evidence_occurrence`（第几次出现），由 Rust 算 span | 本文 §3.6、[01 §3.2](./01-agent-runtime.md) 工具参数 | **M0 第一轮 eval 后决**——这是工具形态变更，需要真实数据支撑。在此之前不做 |
| R11（**新增 2026-08-10**） | **M3 的 ordinal 跨表唯一**——同一段口述会同时产出 `draft_transactions` 与 `draft_items`，两张表**各自**的 `UNIQUE(attempt_id, source_ordinal)` **保证不了跨表唯一**：交易第 2 条和事项第 2 条会同时存在，而 [07 评测](./07-eval.md) 的对齐按 ordinal 配对 | 本文 §3.6、[05 事项](./05-items.md)、[07 评测 §3.2](./07-eval.md) | **M3 开工前决**，两条候选：① domain 在同一事务里跨表检查；② 引入一张公共的位置占用表。**不阻塞 M0**（`draft_items` 是 M3 的表），登记以免被沉默填掉 |
| R9（**新增 2026-08-10**） | **范围不变式挡不住时怎么办**——§3.4 用「IPC 传字符串 + `\|v\| ≤ 10^15`」换掉了全链路 `bigint`。若将来出现合法但超范围的金额（极端通胀币种、或产品扩到机构场景），路径是**全链路 `bigint`**：IPC 已经是字符串，改动面收窄在 TS 侧 | 本文 §3.4/§3.8、[`.claude/rules/frontend.md`](../../.claude/rules/frontend.md) | 出现第一个被 `data.amount_out_of_range` 拒绝的**合法**金额时。**在此之前不做**——`bigint` 的代价是真的，而这个场景至今是假想的 |
| R8（**新增 2026-08-10**） | **跨 exponent 币种的汇率精度**——`rate_ppm` 是主单位汇率 × 1e6（§3.4）。原币 exponent 大于本位币时有效精度会掉：`1 JPY = 0.010412 AUD` 存成 `10412`，相对精度只剩 1e-4，大额换算后可能与账单印的折算金额差几分，从而让总额校验系统性 `failed` | 本文 §3.4、[03 审核 §5](./03-review.md) R6 | **M2 与 R3（舍入规则）一并实测**。候选：提高标度到 1e-9（`rate_nano`），或对该情形改存「最小单位对最小单位」的比率。**M0/M1 不动**——验证场景的两个币种 exponent 都是 2，本条不触发 |
| R12（**新增 2026-08-23**） | AI-native 分类管理的结构化待确认操作如何持久化、使用哪些有界 MCP 工具；若直接新增写分类 / 规则 / 事实的工具会破坏 [ADR-0002](../adr/0002-ai-never-writes-directly.md) | 本文 §3.6 `categories`、[01 Agent 运行时](./01-agent-runtime.md)、[03 审核](./03-review.md) | M2 开工前从当前产品规则产出边界方案并由人审；在此之前不命名表 / 工具，不改 M0 五工具 |
| R13（**新增 2026-08-23**） | 分类迁移只读预检发现旧 `transfer.category TEXT` 或名称冲突后，用户从哪里完成修复并安全重试；启动后才报错会把用户锁在无法使用应用也无法修改数据的死路里 | 本文 §3.6「M2 只前进迁移边界」、[03 审核与草稿区](./03-review.md) | **M2 进入 `ready` 前决定**可执行的修复入口（迁移前 UI、专用命令或等强度方案）及备份 / 重试流程；必须在旧 schema 未改写时可用，不扩成通用 SQL 工具 |

## 6. 验收标准

**可执行命令**（`cargo test` 的具体测试名是本 sub-PRD 对实现的**契约**，实现时须使用这些名字；改名要回流本文）：

- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` 通过
- [ ] `cargo test` 全绿
- [ ] `npm run lint` · `npm run typecheck` · `npm test` · `npm run build` 全绿
- [ ] `cargo test foundation::migration_idempotent` 通过——同一库连开两次，`user_version` 不变、无重复表
- [ ] `cargo test foundation::m0_creates_six_tables` 通过——空目录启动后 M0 六张表存在（含 `accounts` 骨架）
- [ ] `cargo test foundation::migration_drift_rejected` 通过——把 `user_version` 手工调高一号后打开，返回 `data.migration_drift` 且不写入任何数据
- [ ] `cargo test foundation::money_roundtrip_is_integer` 通过——金额经「写入 → 读出 → IPC 序列化 → 反序列化」四步后逐位相等
- [ ] `cargo test foundation::money_inconsistent_rejected` 通过——三元组不自洽时写入被拒并返回 `data.money_inconsistent`
- [ ] `cargo test foundation::currency_exponent_is_not_hardcoded_two` 通过——`currency_exponent("JPY") == 0`、`("KWD") == 3`、`("AUD") == 2`；未知币种返回 `data.unsupported_currency` 且不写库（§3.4「币种精度」）
- [ ] `cargo test foundation::convert_across_exponents` 通过——AUD（exp 2）→ JPY（exp 0）与反向各一例，结果与手算逐位相等；**去掉公式里的 `10^exp` 项时该用例必须变红**
- [ ] `rg -n '/ 100|\* 100' src-tauri/src src` 无命中，或每处命中都在 exponent 表实现内（§3.4）
- [ ] `cargo test foundation::base_currency_frozen_per_row` 通过——切换全局本位币后，已确认交易的 `base_currency` 与 `base_amount_minor` 逐行不变
- [ ] `cargo test foundation::base_currency_required_before_parse` 通过——未选择本位币时解析返回 `data.base_currency_required`，选择后写入 `preferences.json` 且可重启读回
- [ ] `cargo test foundation::rollup_groups_by_base_currency` 通过——含两种本位币的数据集，汇总按本位币分组返回，**不产生跨本位币的单一合计**
- [ ] `rg -n 'SUM\(base_amount_minor\)' src-tauri/src` 的每处命中都伴随 `GROUP BY base_currency` 或单本位币断言
- [ ] `cargo test foundation::draft_requires_evidence` 通过——`source_id` 或 `evidence_text` 为空时插入 `draft_transactions` 失败
- [ ] `cargo test foundation::reported_total_all_or_nothing` 通过——`parse_attempts.reported_total_*` 四列只填一部分时 CHECK 拒绝
- [x] `cargo test foundation::m0_total_claim_schema_stays_single` 通过——M0 no-go 修正后仍只有 `parse_attempts.reported_total_*` 四列、一次尝试至多一条；没有提前新增 scope 列或 `reconciliation_claims` 表（§3.6「M0 单 claim 的范围资格」）
- [ ] `cargo test foundation::reported_total_lives_on_attempt` 通过——`sources` 表**不存在** `declared_total_*` 列；同一来源解析两次，两行 `parse_attempts` 各自带自己的 `reported_total_*`，第一次的值不被第二次覆盖（§3.6「声明合计归尝试，不归来源」） <!-- legacy -->
- [ ] `cargo test ingest::utterance_source_roundtrip` 通过——`kind = utterance` 的来源，转写文本已落盘成 `.txt` 且 `evidence_relpath` 非空（闸门 2 对两种来源同一条实现路径）
- [ ] `cargo test review::utterance_yields_user_attested_batch` 通过——未报告合计的 `utterance` 尝试，`reported_total_*` 全空、CHECK 通过、对账结果为 `not_applicable`
- [ ] `cargo test foundation::m0_insert_transaction_with_null_account` 通过——`PRAGMA foreign_keys = ON` 下，M0 schema 里插入 `account_id IS NULL` 的交易**成功**（`accounts` 骨架表存在）；同时插入一个不存在的 `account_id` **被 FK 拒绝**
- [ ] `cargo test foundation::m2_category_text_migrates_without_loss`（**M2**）通过——非空旧文本按 direction/scope 迁到稳定 ID，同名跨 scope 不合并，`drafted_json` 原文不变
- [ ] `cargo test foundation::category_migration_preflight_is_read_only`（**M2**）通过——transfer 旧分类或名称冲突完整进入诊断，且 `categories`、旧表、`user_version` 与证据逐位不变；不静默丢弃或留下半迁移
- [ ] `cargo test foundation::category_migration_repair_path_roundtrip`（**M2**，R13 定案后）通过——用户通过获批修复入口处理全部诊断项，再次预检与迁移成功；该入口不能执行任意 SQL
- [ ] `cargo test foundation::category_reference_constraints_hold`（**M2**）通过——scope 不匹配、transfer 带分类、引用停用 / 合并墓碑均被拒；`category_id IS NULL` 可确认；仅原批次整批撤销可清除墓碑并恢复引用
- [ ] `cargo test foundation::category_batch_audit_is_append_only`（**M2**）通过——合并 / 拆分 / 撤销的逐项审计共享 `batch_id`，旧审计不删除，`drafted_json` 不更新；任一后续冲突使撤销零写入
- [ ] `cargo test foundation::money_crosses_ipc_as_string` 通过——金额与 `rate_ppm` 序列化出去是十进制字符串，允许范围上界 `10^15` 往返后逐位相等（走 JSON 数字时该用例必须变红）；`i64::MAX` 必须由下一条范围验收拒绝
- [ ] `cargo test foundation::amount_out_of_range_rejected` 通过——`|v| > 10^15` 的金额在序列化前被拒并返回 `data.amount_out_of_range`
- [ ] `npm test -- bridge/money-parse` 通过——`call<T>` 把字符串金额解析成安全整数，超范围抛 `data.amount_out_of_range`；组件里不存在第二处解析
- [ ] `cargo test ingest::utterance_idempotent_by_token` 通过——同一段文本配**不同** `idempotency_key` 提交两次产生**两条** `sources`；配**同一个**令牌提交两次只产生一条（§3.6「口述的幂等键不是内容哈希」）
- [ ] `cargo test foundation::file_hash_unique_only_for_files` 通过——两条 `kind = utterance` 且 `content_hash` 相同的行可以共存，而两条 `kind = file` 且哈希相同的不行（部分唯一索引）
- [ ] `cargo test foundation::draft_links_to_attempt` 通过——每条草稿的 `attempt_id` 指向真实的 `parse_attempts` 行，且该行 `backend_id` 非空（[07 评测 §3.2](./07-eval.md) 的必要条件）
- [ ] `cargo test foundation::draft_attempt_source_must_match` 通过——草稿的 `source_id` 与所属尝试的 `source_id` 不同时插入被拒
- [ ] `cargo test foundation::latest_attempt_must_belong_to_source` 通过——来源不能把另一个来源的尝试设为 `latest_attempt_id`
- [ ] `cargo test foundation::discard_is_distinct_from_void` 通过——人工丢弃只写 `discarded_at`，失败补偿只写 `voided_at`，二者均不删除行
- [ ] `cargo test foundation::draft_ordinal_is_unique_per_attempt` 通过——同一尝试内两条草稿用同一个 `source_ordinal` 时插入被拒（§3.6「位置是 agent 报的」）
- [ ] `cargo test foundation::utterance_draft_requires_span` 通过——`kind = utterance` 的草稿缺 `evidence_span_*` 时被拒；`file` 的草稿带 span 时被拒
- [ ] `cargo test agent::draft_span_bounds_are_checked` 通过——`start == end`、`start > end`、`end > code_point_length(text)`、负数四种越界都被拒（§3.6 第一条校验）；**只实现 substring 相等检查会漏掉全部四种**
- [ ] `cargo test agent::draft_span_must_match_text` 通过——转写文本含 emoji 与中文时，`slice_by_code_points(text, start, end) == evidence_text`；**改用字节偏移或 UTF-16 索引的实现必须让该用例变红**（§3.6「span 用哪套坐标」）
- [ ] `npm test -- bridge/span-roundtrip` 通过——同一份含 emoji 的文本与同一对 `(start, end)`，Rust 与 TS 两端切出的子串**逐字相等**
- [ ] `rg -n 'as_bytes\(\)|\.slice\(|&s\[' src-tauri/src src` 的命中都不在 span 计算路径上（§3.6：Rust 用 `.chars()`、TS 用 `Array.from`）
- [ ] `cargo test foundation::unknown_currency_rejected` 通过——币种码不在 ISO 4217 表内时返回 `data.unsupported_currency` 且**不写库**；**回退到 exponent 2 的实现必须让该用例变红**
- [ ] `cargo test foundation::drafted_json_is_immutable` 通过——草稿被行内编辑后 `drafted_json` 逐字节不变，而 `amount_minor` 等列已改（[ADR-0002](../adr/0002-ai-never-writes-directly.md) 硬性要求 7）
- [ ] `rg -n 'drafted_json' src-tauri/src` 的命中中**不存在** `UPDATE`（与 `audit_log` 同级的 append-only 检查）
- [ ] `cargo test agent::attempt_inherits_probe_hash` 通过——一次解析后 `parse_attempts` 恰好多一行，`prompt_hash` / `tool_surface_version` / `app_version` 均非空，且继承本次探测的 `effective_capability_hash`
- [ ] `cargo test agent::retry_creates_new_attempt` 通过——同一来源解析两次产生**两行** `parse_attempts`，第一行不被覆盖，`sources.latest_attempt_id` 指向第二行
- [ ] `cargo test foundation::draft_triple_all_or_nothing` 通过——草稿三元组只填部分时 CHECK 拒绝
- [ ] `cargo test review::confirm_rejects_incomplete_triple` 通过——三元组不齐的草稿确认时返回 `review.incomplete_triple`
- [ ] `cargo test review::transaction_traces_to_draft` 通过——由草稿确认而来的 `transactions` 行，`source_draft_id` 指向该草稿且该草稿 `consumed_at` 非空
- [ ] `cargo test foundation::audit_actor_accepts_system` 通过——`actor = "system"` 可写入（超时作废等代码触发的动作有合法 actor）
- [ ] `cargo test foundation::icloud_path_rejected` 通过——数据目录指向 iCloud 容器路径时返回 `data.icloud_path_rejected`
- [ ] `rg -n 'f32|f64' src-tauri/src` 在金额相关模块无命中（浮点禁令，见 [`.claude/rules/money-and-data.md`](../../.claude/rules/money-and-data.md)）
- [ ] `rg -n 'UPDATE\s+audit_log|DELETE\s+FROM\s+audit_log' src-tauri/src` 无命中（append-only）

**人工验收**：

- [ ] 应用数据目录在访达里能直接打开，`evidence/` 下的截图能双击预览

## 7. 回流记录

| 日期 | 回流内容 | 依据 |
|---|---|---|
| 2026-09-02（no-go 修正验收） | **本文由 `in-progress → review`。** `foundation::m0_total_claim_schema_stays_single` 与完整零额度门禁通过：生产仍只有 `parse_attempts.reported_total_*` 四列、一次尝试至多一条，没有新增 scope 列或多 claim 表；第一次 v1 报告与旧本机 fixtures 未修改，M1 未开始 | 本文 §6；[07 评测 §6](./07-eval.md)；`node scripts/verify-m0.mjs --skip-live` |
| 2026-09-02（no-go 修正开工） | PR #27 的 current-source 单 claim 范围与保持现有 schema 的规格已独立 review 通过，本文先由 `draft → ready`；维护者随后批准分阶段实施计划，正式开始补测试与实现，故由 `ready → in-progress`。本轮只实现 M0 修正，不增加多 claim schema、不开始 M1 | [PR #27](https://github.com/EpiphanyAlex/DayBook/pull/27)；[`docs/PRD.md` §9.4](../PRD.md) 防滥用流程第 4 步 |
| 2026-08-30（第一次 M0 正式 no-go） | **本文由 `review → draft`。** 第一次正式结果为 `no_go` / exit 3；声明合计可获得率 `4/20`、假警报率 `6/7`，主要因为月度 viewport 外、分页、按日、单笔或子组合计被塞进单条 `reported_total_*`。被证伪的是「来源上出现一个合计即可报告」这条隐含范围，不是四列生命周期。M0 收窄为一条覆盖当前不可变来源全部适用交易的 claim；合计关键词降为候选信号；生产 schema、指标 4 分母 / 阈值与多 claim 的 M2 决策均不动。首次报告与 `fixtures/local/m0-2026-08-24` 永久保留 | [`docs/PRD.md` §9.4](../PRD.md) 第一次正式结果；[07 评测 §7](./07-eval.md) |
| 2026-08-24（M1 前置） | **R6 随 [03 审核 §5](./03-review.md) R1 一并关闭为「不加截图坐标列」。** 产品密封链路的受控合成图仍出现危险误定位与相邻行侵入；同一 CLI 的 Sonnet 对照进一步说明该能力不具备跨模型稳定性。既然代码无法独立验证模型是否指到正确行，可空 bbox 也保护不了「有值但值错」；因此不做原计划的 `0002_*` 迁移，也不改 M0 五工具 | [R1 spike](../spikes/2026-08-24-r1-evidence-region.md)；[03 审核 §3.2/§5](./03-review.md) |
| 2026-08-22（跨文档同步） | **权威错误码表新增 `agent.not_ready`。** [01 §3.5](./01-agent-runtime.md) 的状态矩阵把「已发现合格 CLI、尚未探测或探测中」定为 `error_code` 空的**非错误**态，但同一条规格又要求这一档 fail closed——用户显式发起解析时命令层必须返回一个码，而矩阵里没有。复用 `agent.backend_unavailable` 会把 v0.16 刚拆开的「安装资格 / 解析就绪度」两层重新合上，所以登记新码，只用于 probe 未开始 / 进行中 | 01 的 M0 修正实施计划（2026-08-22 获批）；[01 §3.5](./01-agent-runtime.md) 状态矩阵与 §6 `agent::readiness_blocks_attempt_and_task` |
| 2026-08-17（跨文档同步） | 权威错误码表把 `agent.backend_unavailable` 从含糊的「`probe()` 失败」收窄为安装资格失败（未找到、不可执行、版本不可读取）；完整 readiness probe 的认证、密封与其他失败继续使用各自错误码。`agent.tool_surface_unsealed` 同步改准为「无法证明完整 manifest 与预期严格相等」，不再只写“多出工具”；错误码集合不变 | [`docs/PRD.md` P5](../PRD.md) 部分关闭；[01 Agent 运行时 §3.5](./01-agent-runtime.md) v0.21 |
| 2026-08-13 | **验收选择器按真实职责对齐。** 口述落盘/幂等归 `ingest`，span 工具边界与 attempt 生命周期归 `agent`，确认完整性/溯源归 `review`；原来的 `foundation::*` 名称会让 `cargo test <filter>` 在 0 个测试时仍 exit 0，制造假绿。行为判据不变，只把选择器改到实际执行它的测试 | M0 验收清单逐条与 `cargo test -- --list` 对照 |
| 2026-08-13 | **实现发现“全局本位币决定新交易”没有任何存储与首选路径。** M0 定为本地 `preferences.json` + 首次解析前人工选择；不从地区或来源静默猜，切换不改历史行 | §3.4 已有的逐笔冻结语义；M0 真实确认链路需要完整三元组 |
| 2026-08-13 | **M0 实施开工前补齐四处数据硬约束。** ① 未知币种验收与正文统一为拒绝写入；② `draft_transactions` 新增 `discarded_at`，不再让人工丢弃复用协议失败的 `voided_at`；③ 明确草稿与尝试、来源与最新尝试的跨行归属不变式，并要求触发器或等强度的数据层检查；④ IPC 往返验收由与范围不变式冲突的 `i64::MAX` 改为允许上界 `10^15`，前者应被拒绝。实施落点另明确为 Tauri `data_dir()/Daybook`，首次告知显示路径并可在访达中揭示 | M0 实施计划与首轮 Foundation 测试；否则可产生跨来源证据链与无法区分的作废原因 |
| 2026-08-10（四轮） | **`evidence_span` 只写了「字符区间」，而「字符」在 Rust 与 TS 之间有四种互不相同的含义**（UTF-8 字节 / UTF-16 code unit / code point / grapheme）。**中文夹一个 emoji 就会立刻错位**，且表现为「高亮选错半句话」，看起来像模型报错了位置。定死：零起、左闭右开、**Unicode code point**、对象是未经 normalize 的落盘文本；Rust 用 `.chars()`、TS 用 `Array.from`，**不许用原生字符串索引**；写入时强制两条校验。退路（`evidence_occurrence`）登记为 R10 | 文档审查（四轮） |
| 2026-08-10（四轮） | 新增 R11：**M3 的 ordinal 跨表唯一**——两张草稿表各自的 `UNIQUE(attempt_id, source_ordinal)` 保证不了跨表唯一，而同一段口述会同时产出两类草稿。不阻塞 M0，登记以免被沉默填掉 | 文档审查（四轮） |
| 2026-08-10（三轮） | **eval 的条目对齐在 `file` 来源上写不出来**：[07 §3.2](./07-eval.md) 要求预测侧也有位置，而它写的是「草稿按 `evidence_text` 在原件上的位置排序」——**系统里没有 OCR 也没有坐标**，这与同一份文档承认的「子串断言对图像来源无法实现」是同一个事实，上一版只认了一半。改为**位置由 agent 起草时一并报告**：`draft_transactions` 新增 `source_ordinal`（两种来源都必填、`(attempt_id, ordinal)` 唯一、允许跳号）与 `evidence_span_*`（`utterance` 必填）。同步 [01 §3.2](./01-agent-runtime.md) 工具参数、[07 §3.2](./07-eval.md) 对齐算法、[03 §3.4](./03-review.md) 排序 | 文档审查（三轮） |
| 2026-08-10（三轮） | **未知币种回退到 exponent 2 会产出「带告警但已入账的错误金额」。** 币种的取值域已被定义为 ISO 4217，域外值只有读错/拼错/新代码三种来源，没有一种适合猜。`currency_exponent` 改为返回 `Result`，未知即 `data.unsupported_currency` 并拒绝写入 | 文档审查（三轮）；与「有记录但没拦住 = 没记录」同一条原则 |
| 2026-08-10（三轮） | 三处机械漂移：M0 表数「四→五」改为「四→六」；`accounts` 表「M2 建」改为「M0 建骨架」；**列级写入权限那句「其余列与全部其他表由 Rust 写」字面上排除了 `draft_*`**，改为说全 agent 能写的两处 | 文档审查（三轮） |
| 2026-08-10（二轮） | **`declared_total_*` 放在 `sources` 上是放错了地方。** 那四个值不是原件的属性，是**某一次 agent 解析的输出**。放在来源上导致三种坏情况：① 尝试超时后草稿按 `attempt_id` 作废而合计留了下来；② 重试时新尝试被「一来源一次成功调用」挡住，或默默沿用上次可能读错的值；③ 一个来源被成功解析两次时，「全部未作废草稿」混进两次尝试的输出。改为四列改名 `reported_total_*` 移入 `parse_attempts`，**总额校验入参改为 `attempt_id`**，`sources.latest_attempt_id` 决定当前受审的是哪次输出。agent 对 `sources` 的写入权限随之归零 | 文档审查（二轮）；生命周期必须与产出它的那次尝试闭合 |
| 2026-08-10（二轮） | **`transactions.account_id REFERENCES accounts(id)` 会让 M0 的第一条 `INSERT` 直接失败。** sqlite 3.45.3 实测：`PRAGMA foreign_keys = ON` 下父表不存在时，**即使 `account_id` 是 `NULL`** 也报 `no such table: main.accounts`（SQLite 在写子表时才解析父表，`CREATE TABLE` 阶段不报错，所以问题推迟到运行时）。**M0 加一张 `accounts` 骨架表（四列，不写行、UI 不呈现），M0 由五表改为六表** | 实测；备选「M0 不声明 FK、M2 重建 `transactions`」与「留列是为了避免返工」的理由自相矛盾 |
| 2026-08-10（二轮） | **「TS `number`」与约束 6「任何位置禁止浮点」直接打架**——JS 的 `number` 就是 IEEE-754 双精度，branded type 挡得住单位混用、挡不住精度。真实失败路径是 `JSON.parse` 把超 `2^53` 的值**静默舍入**，而这个值正是 agent 把截图数字读错成 20 位时产生的。改为 **IPC 传十进制字符串 + `\|v\| ≤ 10^15` 范围不变式 + 两侧各校验一次**，新增 `data.amount_out_of_range`。约束 6 的措辞同步改准；全链路 `bigint` 登记为 R9 的后路 | 产品决定（2026-08-10）：IPC 传字符串，TS 内部仍 number |
| 2026-08-10 | **§3.4 的金额与汇率约定在非 2 位小数币种上是错的。** 「最小货币单位」被直接写作「分 / cent」、换算公式直接乘 `rate_ppm / 1e6`——对 JPY（exp 0）/ KWD（exp 3）会差 10^\|Δexp\| 倍。补 `currency_exponent()` 与含 exponent 的换算公式；格式化不得写死 `/ 100`。**这不是改决定，是补一直漏掉的那一项**（同 exponent 时逐位相同）。同步 [ADR-0004 §2/§3](../adr/0004-data-model-sqlite-integer-money.md)、[04 交易 §3.2](./04-transactions.md)、[`.claude/rules/money-and-data.md`](../../.claude/rules/money-and-data.md)、[`.claude/rules/frontend.md`](../../.claude/rules/frontend.md) | 文档审查；与 [`docs/PRD.md` §3.1](../PRD.md)「解析能力与国家/币种无关」冲突——写死两位小数就是把地域假设塞回 schema |
| 2026-08-10 | **`utterance` 用内容哈希永久去重会静默吞掉真实交易。** 跨天各说一句「今天咖啡 5 元」文本逐字相同，第二笔被判重复直接消失。改为：`content_hash` 的唯一约束收窄到 `kind = 'file'`，`utterance` 用一次提交一个 `idempotency_key`，文本重复只提示不阻止。同步 [02 导入 §3.2](./02-ingest.md) | 文档审查；原理由（「重说一遍通常意味着以为没记上」）只在几分钟尺度成立 |
| 2026-08-10 | **新增 `parse_attempts` 表（M0）与 `draft_transactions.drafted_json`（M0）。** 前者：`sources.agent_session_id` 只存最近一次，重试即覆盖，[07 评测 §3.5](./07-eval.md) 的「模型退步 vs 提示词改坏」无从区分；`backend_id` / `model_id` 随之从草稿表移到尝试表，同一事实只留一个出处。后者：[03 审核 §3.5](./03-review.md) 的行内编辑会就地改写草稿，[07 §3.2](./07-eval.md)「草稿保留原始起草值」因此**不成立**，eval 真值恒等于人的答案。同步 [`docs/PRD.md` §9.2/§9.3](../PRD.md)、[01 §3.4](./01-agent-runtime.md)、[07 §3.2](./07-eval.md) | 文档审查 + 产品决定（2026-08-10）：接受为此扩大 M0 |
| 2026-08-10 | **`sources` 新增 `declared_total_kind`。** 单个 scalar 表达不了「消费合计 / 收入合计 / 净变动」，而三者对应三条不同等式；一张含退款的账单按旧写法必然 `failed`，看起来像 agent 读错。同步 [ADR-0002 闸门 3](../adr/0002-ai-never-writes-directly.md)、[01 §3.2](./01-agent-runtime.md)、[03 §3.3](./03-review.md) | 文档审查 |
| 2026-08-10 | **`transactions` 新增 `account_id`（M0/M1 恒空，M2 启用）+ `accounts` 进全表清单。** [`docs/PRD.md` §2](../PRD.md) 以多账户为首要验证场景，而 schema 只有 `channel`（支付方式类别），「哪张卡」无法表达，[02 §3.6](./02-ingest.md) 跨图去重与 [04 §5](./04-transactions.md) R3 转账双边因此没有落点。同步 [04 §3.1/§3.4](./04-transactions.md) | 产品决定（2026-08-10）：现在留字段，M2 实现 |
| 2026-08-10 | **§3.7 补 6 个 `agent.*` 错误码。** 其中 `agent.interrupted` 早在 [02 导入 §3.4](./02-ingest.md) 被使用却从未登记——本表自称「全仓库唯一出处」，漏登记即缺陷 | 文档审查 |

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.23 | 2026-09-02 | **第一次 no-go 修正验收，`status: in-progress → review`。** 单 claim 四列不变的回归与完整零额度门禁通过；未运行真实 agent / formal，未修改第一次报告或旧 fixtures，未开始 M1 |
| v0.22 | 2026-09-02 | **第一次 no-go 修正开工，`status: draft → ready → in-progress`。** PR #27 的规格已独立 review 通过，维护者批准分阶段实施；先补保持单 claim schema 的回归，再改实现。M1 不开始 |
| v0.21 | 2026-08-30 | **第一次 M0 正式 no-go 回流，`status: review → draft`。** 定义 M0 单 claim 只认 current immutable source 全部适用交易；任意 viewport 仍支持，月度 viewport 外、分页、按日 / 分类 / 单笔语义 / 子组合计均不得报告；有效 claim 与 invalid decoy 三元组相同时也因身份不可审计而拒报，关键词只作候选。`parse_attempts.reported_total_*` 四列与一次一条限制不变，不提前实现多 claim schema；新增保持单 claim schema 的验收。第一次 no-go、旧报告与 `fixtures/local/m0-2026-08-24` 不改 |
| v0.20 | 2026-08-24 | **关闭 R6，`status` 仍为 `review`。** [03 审核](./03-review.md) R1 spike 未达到证据高亮的安全门槛，因此不新增截图 bbox 列、不创建 `0002_*` 迁移、不改 `draft_transaction`；M0 六表五工具与已实现 schema 一字未动 |
| v0.19 | 2026-08-24 | **跟随 [04 §3.3](./04-transactions.md) v0.9，`status` 仍为 `review`（M2 表，M0 实现不受影响）。** `categories.name` 的注释由「默认值统一四个汉字」改为「两个汉字」，并注明唯一性只在同一 `scope` 内。**`UNIQUE(scope, normalized_name)` 这条约束本来就是对的、一个字没改**——变的是它从此**真的被用到**：四字命名下没有任何两条默认分类同名，`scope` 那一列在种入路径上从未被行使过；两字命名后「礼金」与「其他」两侧各一条，**种子数据本身就会去撞这个索引**。这一行存在的意义是提醒实现者别在种入时按 `name` 去查重 |
| v0.18 | 2026-08-23 | **补 M2 分类实体与迁移边界，`status` 仍为 `review`。** v1 终态表清单新增 `categories`；明确 M0/M1 已实现的 `category TEXT` 不变，M2 才迁为稳定 `category_id`。补分类表最小列、方向 / 停用 / 合并引用约束、默认只种一次、旧文本无损迁移、迁移前只读预检、`audit_log.batch_id` 与完整批次前状态撤销；结构化分类操作的表 / 工具形状及诊断修复入口登记为 M2 `ready` 前待决，不自行发明、不改变当前 M0 六表五工具 |
| v0.17 | 2026-08-22 | **§3.7 新增错误码 `agent.not_ready`，`status` 仍为 `review`。** 合格 CLI 已发现但完整 readiness probe 未开始或进行中时，用户显式发起解析由命令层返回该码并拒绝创建 `parse_attempts`；`BackendStatus` 上这一档仍是 `error_code` 空（UI 显示「正在检查」）。probe 跑完但失败仍用各自的码。错误码集合由 29 条增至 30 条，其余不变 |
| v0.16 | 2026-08-17 | **安装资格 / 解析就绪度错误语义同步，`status` 仍为 `review`。** `agent.backend_unavailable` 只表示未发现合格 CLI（未找到、不可执行、版本不可读取），不再用「任意 `probe()` 失败」把认证与密封失败混进安装状态；`agent.tool_surface_unsealed` 同步扩准为「无法证明完整 capability manifest 与预期严格相等」，覆盖清单缺失/不可读、缺项、多项及非工具副作用能力；错误码集合未变 |
| v0.15 | 2026-08-13 | **M0 实现验收进入 `review`。** 六表迁移、整数金额/IPC、证据目录、本位币偏好与访达揭示入口已落地，统一 M0 门禁通过 |
| v0.14 | 2026-08-13 | **验收审计回流：**把 10 条已漂移的 `foundation::*` 测试选择器对齐到真实的 `ingest` / `agent` / `review` 测试，消除 0-test 假绿；验收行为不变 |
| v0.13 | 2026-08-13 | **实现回流：补齐当前本位币设置。** 本地 `preferences.json` 持久化、首次解析前人工选择、未选明确拒绝；切换只影响后续解析 |
| v0.12 | 2026-08-13 | **M0 开始实施，`status` 进入 `in-progress`。** 修正未知币种与 IPC 上界的旧验收；新增 `discarded_at` 与两条跨行归属不变式；补 4 条可执行验收；确定默认数据目录与首次告知呈现 |
| v0.1 | 2026-08-06 | 初版：存储引擎与数据目录、标识/时间/金额/汇率约定、迁移策略、M0 四表与 v1 全表清单、命令契约与 `AppError` 形状、TS 桥；否决方案六条；待决 R1–R5；验收标准 11 条可执行 + 1 条人工 |
| v0.11 | 2026-08-10 | **公开文档降噪。** §3.6 对评测目标的引用去掉第一人称会话式表述，schema 与验收标准未变 |
| v0.10 | 2026-08-10 | **文档审查第五轮回流**：`effective_tool_surface_hash` **改名 `effective_capability_hash`** 并扩到非工具型能力（[01 §3.7](./01-agent-runtime.md)）；§6 补 span 的四种越界用例 |
| v0.9 | 2026-08-10 | **文档审查第四轮回流。** ① **`evidence_span` 的坐标系定死**（零起、左闭右开、Unicode code point、未 normalize 的落盘文本，两端实现方式指定）——原文「字符区间」在 Rust/TS 之间有四种解释，中文夹 emoji 即错位；写入时强制 `slice_by_code_points(...) == evidence_text`，顺带让 `utterance` 的 `evidence_text` 变成可独立核验的。② 新增错误码 `agent.unexplained_gap`。③ §5 新增 R10（span 报不准的退路）与 R11（M3 ordinal 跨表唯一）。§6 新增 4 条验收 |
| v0.8 | 2026-08-10 | **文档审查第三轮回流。** ① `draft_transactions` 新增 **`source_ordinal`**（两种来源必填）与 **`evidence_span_*`**（`utterance` 必填）——没有它 [07](./07-eval.md) 的条目对齐在 `file` 来源上根本写不出来（没有 OCR、没有坐标）。② **未知币种由「回退 2 + 告警」改为 `data.unsupported_currency` 拒绝**。③ 三处机械漂移：M0 表数、`accounts` 建表时机、列级写入权限的措辞。§6 新增 3 条验收 |
| v0.7 | 2026-08-10 | **文档审查第二轮回流，三处硬缺陷。** ① **`declared_total_*` 由 `sources` 移到 `parse_attempts` 并改名 `reported_total_*`**——合计是某次尝试的输出，不是原件的属性；旧位置让它在重试后与草稿生命周期脱钩。总额校验入参改为 `attempt_id`；agent 对 `sources` 的写入权限归零。② **M0 加 `accounts` 骨架表（五表→六表）**——`account_id` 的 FK 指向不存在的父表时，SQLite 在 `foreign_keys = ON` 下**连插入 `NULL` 都会失败**（实测）。③ **金额与汇率在 IPC 上改为十进制字符串** + `\|v\| ≤ 10^15` 范围不变式 + 两侧校验，新增 `data.amount_out_of_range`——原写法（TS `number`）与约束 6 字面冲突，且 `JSON.parse` 的静默舍入正好落在「agent 读错数字」这条路径上。§5 新增 R9；§6 新增 7 条验收、改写 3 条 |
| v0.6 | 2026-08-10 | **文档审查回流，六处。** ① §3.4 补**币种 exponent** 与含 exponent 的换算公式（JPY/KWD 会差 100 倍）。② §3.6 `sources`：`content_hash` 唯一约束收窄到 `kind = 'file'`、新增 `idempotency_key`——口述用内容哈希永久去重会**静默吞掉跨天的真实重复交易**。③ §3.6 新增 **`parse_attempts` 表**（M0 第五张）并把 `backend_id` / `model_id` 从草稿表移入；`sources.agent_session_id` 改为 `latest_attempt_id`。④ §3.6 `draft_transactions` 新增 **`drafted_json`**（不可变起草快照）与 `voided_at`——行内编辑会就地改写草稿，[07 §3.2](./07-eval.md) 的 eval 真值原本不成立。⑤ §3.6 `sources` 新增 **`declared_total_kind`**、`transactions` 新增 **`account_id`**。⑥ §3.7 补 6 个 `agent.*` 码（含早被使用却未登记的 `agent.interrupted`）。§5 新增 R7（一来源多条合计）R8（跨 exponent 汇率精度）；§6 新增 11 条验收。详见「7. 回流记录」 |
| v0.5 | 2026-08-08 | 公开仓库去个人化：§5 待决表与本表的决策署名统一为「产品决定」，去掉工具与会话指代；`owner` 改为 `@maintainer`。**决定、schema 与验收标准未变** |
| v0.4 | 2026-08-08 | **设计评审回流。** ① `sources` 新增 **`kind`（`file` \| `utterance`）** 并加「来源不等于文件」小节：闸门 2 要的是「痕迹 + 原文」而非「文件」；此前的隐含假设造成硬墙——`draft_transactions.source_id` 非空而口述无文件，导致 [ADR-0003 §2](../adr/0003-agent-runtime-and-pluggable-backend.md) 自己举的跨实体口述例子**在结构上落不了地**。转写文本落盘成 `.txt` 与截图同等对待，`original_filename` 随之改为可空。② `draft_transactions` 新增 **`backend_id` / `model_id`**——[07 评测](./07-eval.md) 的 eval 集是「草稿 ← 交易」join，不记后端与模型则基线不可解释。③ §6 新增 3 条验收 |
| v0.3 | 2026-08-07 | **M0 开工评审 → `status: ready`。** ① §3.6 四张 M0 表**逐列定死**（此前只有「关键约束」，本节自己注明「详细字段开工前补」——现在补上）：`sources` 新增 `state` / `evidence_relpath` / `parse_error_code` / `agent_session_id` 与 **`declared_total_currency`+`declared_total_evidence_text`**（原设计只有金额没有币种，无法校验；且校验基准本身必须可核对）；`draft_transactions` 明确三元组**草稿可空、确认必填**；`transactions` 新增 **`source_draft_id` 溯源列**（兑现 [03 §3.1](./03-review.md) 的审计承诺，原先无落点）；`audit_log.actor` **新增 `system`**（超时作废等代码触发的动作原先无合法取值）；新增 agent 对 `sources` 的**列级写入权限**收窄。② §3.7 补全**权威错误码集** 18 条——此前只登记 `data.*`，而 `.claude/rules/` 已在示例中使用未登记的 `review.*` 码。③ §5 新增 R6（证据坐标字段，随 [03](./03-review.md) R1 决，不阻塞 M0）。④ §6 新增 6 条验收 |
| v0.2 | 2026-08-07 | **待决 R1 关闭：本位币可切换**（产品决定，[`docs/PRD.md` §13](../PRD.md) P2 同步关闭）。§3.4 新增 `base_currency` 约定行与「本位币切换语义」小节（逐笔冻结 / 切换不改历史 / 汇总按本位币分组，三条均由 [ADR-0004](../adr/0004-data-model-sqlite-integer-money.md) 折算冻结原则推导）；§3.4 自洽约束的「单币种交易」改述为「原币与本位币相同时」（本位币不再固定，原措辞会有歧义）；§3.6 `transactions` 关键约束逐字段列出并加入 `base_currency`；§6 新增 3 条验收（逐行冻结、汇总分组、`SUM` 必带 `GROUP BY`） |
