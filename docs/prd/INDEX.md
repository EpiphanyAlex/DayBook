---
title: sub-PRD 索引与状态总览
status: ready
owner: "@alex"
date: 2026-08-06
version: v0.1
---

# sub-PRD 索引

> 一个能力一份，扁平文件。**本项目不用 ticket**——人写「要什么和为什么」（sub-PRD），agent 出「怎么做」（plan 阶段），人审计划。
> 写作纪律见 [`CLAUDE.md`](./CLAUDE.md)（本目录）；产品范围与里程碑见 [`docs/PRD.md`](../PRD.md)。
> **改任何 sub-PRD 的 `status` 时必须同步本文的状态总览表**——两处不一致即缺陷。

## 状态总览

| # | sub-PRD | 覆盖 | status | version |
|---|---|---|---|---|
| 00 | [地基 Foundation](./00-foundation.md) | 数据层、SQLite schema、迁移、错误契约、金额类型 | `draft` | v0.1 |
| 01 | [Agent 运行时](./01-agent-runtime.md) | MCP server（`rmcp`）、agent 启动器、可插拔后端接口 | `draft` | v0.1 |
| 02 | [导入 Ingest](./02-ingest.md) | 截图导入、`sources` 落库、解析编排 | `draft` | v0.1 |
| 03 | [审核与草稿区](./03-review.md) | 草稿区、证据链、总额校验、审核界面 | `draft` | v0.1 |
| 04 | [交易 Transactions](./04-transactions.md) | 交易实体、多币种三元组、分类、回顾 | `draft` | v0.1 |
| 05 | [事项 Items](./05-items.md) | 事项实体（backlog → 排期 → 完成时长） | `draft` | v0.1 |
| 06 | [记忆 Memory](./06-memory.md) | 记忆规则（商户映射、纠正、语境词表） | `draft` | v0.1 |

**当前全部为 `draft`**：七份规格已按 2026-08-06 立项讨论结论成形，但尚未经人工评审。
**下一步**（依据 [`CLAUDE.md`](../../CLAUDE.md)「PRD 体系与工作流」步骤 1）：人 + agent 一起把 M0 涉及的四份——[00 地基](./00-foundation.md)、[01 Agent 运行时](./01-agent-runtime.md)、[02 导入](./02-ingest.md)、[03 审核与草稿区](./03-review.md)——审到 `status: ready`，然后进 plan mode 开 M0。

## 里程碑 × sub-PRD

**两个正交维度：sub-PRD 按能力切，里程碑按时间切。** 里程碑的判定标准与 M0 各份取哪一片，见 [`docs/PRD.md` §9](../PRD.md)。

| 里程碑 | 涉及 sub-PRD |
|---|---|
| **M0** 端到端点亮 | [00](./00-foundation.md) + [01](./01-agent-runtime.md) + [02](./02-ingest.md) + [03](./03-review.md) 各取最小切片 |
| **M1** 审核界面 | [03](./03-review.md) 做深 |
| **M2** 批量与多币种 | [02](./02-ingest.md) + [04](./04-transactions.md) |
| **M3** 事项与记忆 | [05](./05-items.md) + [06](./06-memory.md) |
| **M4** 可插拔与打包 | [01](./01-agent-runtime.md) 补全 |

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
- **[06 记忆](./06-memory.md) 的输入来自 [03](./03-review.md) 的每一次人工纠正**，输出注入 [02](./02-ingest.md) 的解析编排。

## 跨 sub-PRD 的共享决定在哪

避免在多份文档里各写一遍（**跨文档一致性**，见 [`CLAUDE.md`](./CLAUDE.md) 硬规则 5）：

| 你想知道 | 权威出处 |
|---|---|
| 金额怎么存、汇率怎么表示 | [ADR-0004](../adr/0004-data-model-sqlite-integer-money.md) + [00 地基](./00-foundation.md) |
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
