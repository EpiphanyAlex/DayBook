---
title: sub-PRD 索引与状态总览
status: ready
owner: "@maintainer"
date: 2026-08-08
version: v0.7
---

# sub-PRD 索引

> 一个能力一份，扁平文件。**本项目不用 ticket**——人写「要什么和为什么」（sub-PRD），agent 出「怎么做」（plan 阶段），人审计划。
> 写作纪律见 [`CLAUDE.md`](./CLAUDE.md)（本目录）；产品范围与里程碑见 [`docs/PRD.md`](../PRD.md)。
> **改任何 sub-PRD 的 `status` 时必须同步本文的状态总览表**——两处不一致即缺陷。

## 状态总览

| # | sub-PRD | 覆盖 | status | version |
|---|---|---|---|---|
| 00 | [地基 Foundation](./00-foundation.md) | 数据层、SQLite schema、迁移、错误契约、金额类型 | **`ready`** | v0.5 |
| 01 | [Agent 运行时](./01-agent-runtime.md) | MCP server（`rmcp`）、agent 启动器、可插拔后端接口 | **`ready`** | v0.5 |
| 02 | [导入 Ingest](./02-ingest.md) | 截图与口述导入、`sources` 落库、解析编排 | **`ready`** | v0.5 |
| 03 | [审核与草稿区](./03-review.md) | 草稿区、证据链、总额校验、审核界面 | **`ready`** | v0.4 |
| 04 | [交易 Transactions](./04-transactions.md) | 交易实体、多币种三元组、分类、回顾 | `draft` | v0.5 |
| 05 | [事项 Items](./05-items.md) | 事项实体（backlog → 排期 → 完成时长） | `draft` | v0.3 |
| 06 | [记忆 Memory](./06-memory.md) | 记忆规则（商户映射、纠正、语境词表） | `draft` | v0.3 |
| 07 | [评测 Eval](./07-eval.md) | 评测集、评分器、回归门槛、夹具与重放 | `draft` | v0.3 |

**M0 四份已于 2026-08-07 完成开工评审，进入 `ready`**：[00 地基](./00-foundation.md)、[01 Agent 运行时](./01-agent-runtime.md)、[02 导入](./02-ingest.md)、[03 审核与草稿区](./03-review.md)。评审收敛了 5 处跨文档冲突与 8 处缺口，关闭待决 3 项、改期 2 项，各份变更记录有逐条说明。

**下一步**（依据 [`CLAUDE.md`](../../CLAUDE.md)「PRD 体系与工作流」步骤 2）：agent 读这四份 → 进 plan mode → 出 M0 实施计划 → 人审 → 批准后 `status: in-progress`。

[04 交易](./04-transactions.md)、[05 事项](./05-items.md)、[06 记忆](./06-memory.md) 仍为 `draft`，各自在 M2/M3 开工前评审。

## 里程碑 × sub-PRD

**两个正交维度：sub-PRD 按能力切，里程碑按时间切。** 里程碑的判定标准与 M0 各份取哪一片，见 [`docs/PRD.md` §9](../PRD.md)。

| 里程碑 | 涉及 sub-PRD |
|---|---|
| **M0** 端到端点亮 | [00](./00-foundation.md) + [01](./01-agent-runtime.md) + [02](./02-ingest.md) + [03](./03-review.md) 各取最小切片 |
| **M1** 审核界面 | [03](./03-review.md) 做深 |
| **M2** 批量与多币种 | [02](./02-ingest.md) + [04](./04-transactions.md) |
| **M3** 事项与记忆 | [05](./05-items.md) + [06](./06-memory.md) |
| **M4** 可插拔与打包 | [01](./01-agent-runtime.md) 补全 |

[07 评测](./07-eval.md) **不绑定单一里程碑**：M0 起就要有 20 用例的基线（`PRD §9.1` 的两个未知数都靠它度量），之后随每次改提示词/换后端跑。夹具重放（`07 §3.6`）是唯一进 CI 的部分。

## 依赖关系

```
00 地基 ──┬── 01 Agent 运行时 ──┐
          │                      ├── 02 导入 ── 03 审核与草稿区 ── 04 交易
          └──────────────────────┘                    │
                                                       └── 06 记忆
          └── 05 事项
```

- **[00 地基](./00-foundation.md) 是所有模块的前置**：schema、错误契约、金额类型定下来之前，其余六份的实现会各自用自己的假设填空（**零沉默原则**，见 [`CLAUDE.md`](./CLAUDE.md)）。
- **[03 审核与草稿区](./03-review.md) 是 [04 交易](./04-transactions.md) 与 [05 事项](./05-items.md) 的共同入口**：两个实体走同一套「草稿 → 确认 → 事实表」流程。
- **[06 记忆](./06-memory.md) 的输入来自 [03](./03-review.md) 的每一次人工纠正**；输出**不注入**解析流程，而是由 agent 经 `query_memory` 主动查（[06 §3.4](./06-memory.md)，2026-08-08 改定）。

## 跨 sub-PRD 的共享决定在哪

避免在多份文档里各写一遍（**跨文档一致性**，见 [`CLAUDE.md`](./CLAUDE.md) 硬规则 5）：

| 你想知道 | 权威出处 |
|---|---|
| 金额怎么存、汇率怎么表示 | [ADR-0004](../adr/0004-data-model-sqlite-integer-money.md) + [00 地基](./00-foundation.md) |
| **本位币可切换后的语义**（逐笔冻结 / 汇总分组） | [00 地基 §3.4](./00-foundation.md)「本位币切换语义」 |
| **M0 四张表的逐列字段** | [00 地基 §3.6](./00-foundation.md) |
| **权威错误码集**（18 条，新增码先改这里） | [00 地基 §3.7](./00-foundation.md) |
| **总额校验比哪个金额**（原币 vs 本位币） | [03 审核 §3.3](./03-review.md)「校验式」 |
| **闸门 3 挡不住什么** | [03 审核 §3.3](./03-review.md)「基准值本身必须可核对」+ [01 §3.2](./01-agent-runtime.md)「`report_source_total` 可信性要求」 |
| **M0 端到端脚本验什么** | [`docs/PRD.md` §9.3](../PRD.md) |
| **工具的智能边界**（什么归 agent、什么归工具） | [ADR-0006](../adr/0006-smart-agent-dumb-tools.md) |
| **日志落什么、不落什么** | [ADR-0007](../adr/0007-local-observability-and-log-tiers.md) + [01 §3.4](./01-agent-runtime.md) |
| **口述来源（`kind = utterance`）的语义** | [00 地基 §3.6](./00-foundation.md)「来源不等于文件」+ [02 §3.1](./02-ingest.md) |
| **交易与事项的边界** | [05 事项 §3.1](./05-items.md)「钱是否已经流动」 |
| **记忆规则怎么应用** | [06 记忆 §3.4](./06-memory.md) |
| **什么算「读对」、回归怎么判** | [07 评测 §3.3/§3.5](./07-eval.md) |
| 草稿与事实表怎么隔离、证据怎么挂 | [ADR-0002](../adr/0002-ai-never-writes-directly.md) + [03 审核与草稿区](./03-review.md) |
| MCP 工具的权限边界 | [ADR-0003](../adr/0003-agent-runtime-and-pluggable-backend.md) + [01 Agent 运行时](./01-agent-runtime.md) |
| 错误码集与错误形状 | [00 地基](./00-foundation.md) |
| 术语（交易/事项/来源/证据/本位币…） | [`docs/CONTEXT.md`](../CONTEXT.md) |
| 组件职责与数据流 | [`docs/architecture.md`](../architecture.md) |

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.1 | 2026-08-06 | 初版：七份 sub-PRD 索引、状态总览、里程碑映射、依赖关系图、共享决定出处表 |
| v0.2 | 2026-08-07 | 随 [`docs/PRD.md` v0.2](../PRD.md) 定位修正同步 01/02/04 的版本号至 v0.2（去窄化：多币种与多渠道是能力而非定位，具名银行/支付平台降为 dogfooding 样本） |
| v0.3 | 2026-08-07 | 随**本位币可切换**拍板（[`docs/PRD.md` §13](../PRD.md) P2 关闭）同步：00 → v0.2、04 → v0.3；共享决定出处表新增「本位币切换语义」一行，指向 [00 地基 §3.4](./00-foundation.md) |
| v0.4 | 2026-08-07 | **M0 四份完成开工评审并进入 `ready`**：00 → v0.3、01 → v0.3、02 → v0.3、03 → v0.2。状态总览与「下一步」改写为进 plan mode 开 M0；共享决定出处表新增 5 行（M0 逐列字段、权威错误码集、总额校验比哪个金额、闸门 3 的边界、M0 端到端脚本） |
| v0.5 | 2026-08-08 | **设计评审同步。** 新增 **[07 评测](./07-eval.md)**（此前没有任何 sub-PRD 负责「agent 读得准不准」，而 `PRD §9.1` 认定它是生死线）；版本同步 00→v0.4 · 01→v0.4 · 02→v0.4 · 03→v0.3 · 04→v0.4 · 05→v0.2 · 06→v0.2；里程碑映射补一段说明 07 不绑定单一里程碑；共享决定出处表新增 6 行（工具的智能边界、日志分级、口述来源语义、交易/事项边界、记忆应用、读对判据） |
| v0.6 | 2026-08-08 | 随文档门禁进 CI 同步：07 → v0.2（夹具目录拆为 `fixtures/local/` 与 `fixtures/ci/` 两支）。文档门禁自此为两条——`docs/prd/check-docs.mjs` 与 `scripts/check-links.mjs`，由 `.github/workflows/docs.yml` 对所有 PR 强制 |
| v0.7 | 2026-08-08 | **公开仓库去个人化同步**：八份 sub-PRD 版本 +0.1（00→v0.5 · 01→v0.5 · 02→v0.5 · 03→v0.4 · 04→v0.5 · 05→v0.3 · 06→v0.3 · 07→v0.3）；全目录 `owner` 改为 `@maintainer`；本表去掉工具与会话指代。**状态、依赖关系与共享决定出处表未变** |
