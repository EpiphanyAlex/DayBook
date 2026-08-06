---
title: 05 事项 Items — backlog、排期与完成时长
status: draft
owner: "@alex"
date: 2026-08-06
version: v0.1
---

# 05 · 事项 Items

> v1 的「薄」模块。**做得薄是决定，不是妥协**——[`docs/PRD.md` §5](../PRD.md) 的范围纪律是「交易做深、事项做薄」。
> 依据：[ADR-0004 §4](../adr/0004-data-model-sqlite-integer-money.md)（两个实体、一条时间轴）。

## 1. 问题

用户同样需要事后补记「这段时间我都干了什么、什么没干完」。但事项与交易有一个根本差异：

> **钱有客观痕迹，时间没有。**

截图能证明「8 月 3 日在 Coles 花了 47.30」，但没有任何痕迹能证明「8 月 3 日下午写了两小时文档」。所以：

- **时间日志做不到「几点到几点」**，只能粗粒度
- **好在时间容忍模糊，钱不容忍**（[`docs/PRD.md` §12](../PRD.md)）

第二个洞察决定了数据模型：**「待办」和「时间日志」是同一个实体的两端，不是两个功能。** 一条记录走完整生命周期。如果拆成两张表，「这周计划做的事」和「这周实际花的时间」就永远对不上——而那正是产品终局价值（意图与事实互相印证）的基础。

## 2. 范围与非目标

**范围**：事项实体与生命周期状态机 · backlog 列表 · 一句话批量丢入 · 排到某天 · 记完成时段与实际时长 · 两个基础问答（「这周时间花哪了」「什么没做完」）。

**非目标**（[`docs/PRD.md` §6](../PRD.md) 明确不做）：

- 提醒、通知
- 重复任务
- 子任务、任务依赖
- 优先级算法
- 番茄钟、计时器
- 弱信号采集自动生成事项（未来做，前置条件见 [ADR-0005 §4](../adr/0005-voice-and-system-integration.md)）
- **意图↔事实闭环**（待办「交房租 2000」被账单里的转账核销）——**v2**（[`docs/PRD.md` §11](../PRD.md)）

## 3. 决定与依据

### 3.1 生命周期状态机

```
        ┌──────────────────────────────────┐
        ▼                                  │
    backlog ──排到某天──▶ scheduled ──▶ done
    （无日期）                 │
                               └──未完成──▶ backlog
```

| 状态 | 含义 | 关键字段 |
|---|---|---|
| `backlog` | 已记下，尚未安排日期 | `scheduled_on` 为空 |
| `scheduled` | 排到了某一天 | `scheduled_on` 非空 |
| `done` | 完成 | `done_on` 非空；`actual_minutes` 可空 |

- **`backlog` 不是「优先级低」**，只是「还没安排日期」
- **未完成退回 backlog**：过了 `scheduled_on` 仍未 `done` 的事项，**不自动退回**——由用户在回顾时决定退回还是改期。自动退回会让用户失去「这周什么没做完」的信息
- **`archived`（放弃）** 是第四个终态，与 `done` 区分——「不做了」和「做完了」在回顾里含义不同

### 3.2 时间粒度：只记时长，不记时段

- `actual_minutes`（整数分钟）**可空**——用户可以只标记完成而不记时长
- **不记「几点到几点」**：没有客观痕迹支撑，逼用户填只会得到编造的数据
- 不提供计时器（番茄钟是非目标）

### 3.3 一句话批量丢入

用户的真实动作是「一口气想起五件事」。

- 输入框接受多行文本，**一行一个事项**，直接进 `backlog`
- **也支持经 agent 起草**：用户说一段话，agent 拆成多条 `draft_items`，走 [03 审核与草稿区](./03-review.md) 的同一套确认流程
- **语音输入**：v1 用 macOS 系统听写，用户在输入框内自行触发（连按两下 `Fn`）；应用零代码（[ADR-0005 §1](../adr/0005-voice-and-system-integration.md)）

### 3.4 草稿区同样适用

事项也走 `draft_items` → 人工确认 → `items`（[ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 1）。

**但证据链要求宽松于交易**：事项的来源常常是用户自己的一句话，没有截图。因此：

- `draft_items.source_id` **可空**（与 `draft_transactions` 不同，后者非空）
- 来源是用户口述时，`evidence_text` 存那句原话
- **理由**：证据链的目的是防止「AI 编造了一个用户没说过的数字」。事项没有数字精度问题，且用户口述本身就是原文——**时间容忍模糊，钱不容忍**

### 3.5 两个基础问答

v1 只回答两个问题，**不做通用报表**：

1. **「这周时间花哪了」**——已完成事项按 `actual_minutes` 汇总（未记时长的单独列出条数）
2. **「什么没做完」**——`scheduled_on` 已过但仍非 `done` 的事项

### 3.6 UI：独立视图

**时间轴是共同骨架，但 UI 分两个视图**——**不在同一张日历里混着显示钱和时间**（[ADR-0004 §4](../adr/0004-data-model-sqlite-integer-money.md)）。

事项视图 = backlog 列表 + 按天的排期视图。

## 4. 否决的替代方案

| 方案 | 否决原因 |
|---|---|
| 拆成「待办表」+「时间日志表」 | 「这周计划做的事」和「这周实际花的时间」永远对不上，从数据模型层面放弃产品终局价值（[ADR-0004 §4](../adr/0004-data-model-sqlite-integer-money.md)、[`docs/PRD.md` §11](../PRD.md)） |
| 记录起止时刻（`started_at` / `ended_at`） | 无客观痕迹支撑，逼用户填只会得到编造的数据；时间容忍模糊（[`docs/PRD.md` §12](../PRD.md)） |
| 过期自动退回 backlog | 用户会失去「这周什么没做完」这个信息——而那正是 §3.5 的问答之一 |
| `done` 与「放弃」合并为一个终态 | 回顾里含义不同：「做完了」是成果，「不做了」是决策 |
| 事项也强制非空 `source_id` | 事项常来自用户口述，强制会让「一句话丢五件事」这条主路径走不通；且证据链的目的（防编造数字）在事项上不成立 |
| v1 就做与交易的自动核销 | [`docs/PRD.md` §11](../PRD.md) 明确是 v2；v1 只需保证数据形状不挡路 |
| v1 引入优先级/子任务/重复任务 | [`docs/PRD.md` §6](../PRD.md) 非目标。「薄」的字面含义 |

## 5. 待决与风险

| # | 事项 | 影响 | 谁来决 / 何时 |
|---|---|---|---|
| R1 | 「薄」到什么程度算够——若实际使用中事项模块无人用，说明薄过头或方向错 | 本模块存废 | M3 用满两周后由 @alex 判断，**结论回流本文** |
| R2 | 排期视图与交易视图的时间轴是否共享一套组件——共享省代码，但两者信息密度差异大 | 本文 §3.6、[04 交易](./04-transactions.md) 回顾 | M3 开工时定 |
| R3 | 事项的分类/标签是否需要——v1 暂不做，但「这周时间花哪了」若无分组可能没有信息量 | 本文 §3.5 | M3 实测后决；**若加，需回流本文与 [00 地基](./00-foundation.md) schema** |
| R4 | agent 从一段话拆事项的粒度（「买菜做饭」是一件还是两件）——纯主观，无客观判据 | 本文 §3.3 | M3 实测；对策方向是让记忆规则学用户的习惯（[06 记忆](./06-memory.md)） |
| R5 | 未来接入弱信号采集（[ADR-0005 §4](../adr/0005-voice-and-system-integration.md)）时，自动生成的事项与手工事项如何区分 | 本文 §3.1 字段 | v1 不做，登记以免被沉默填掉；届时应接 [ActivityWatch](https://github.com/activitywatch/activitywatch) 而非自建 |

## 6. 验收标准

- [ ] `cargo fmt --all -- --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test` 全绿
- [ ] `npm run lint` · `npm run typecheck` · `npm test` · `npm run build` 全绿
- [ ] `cargo test items::lifecycle_transitions` 通过——枚举全部合法/非法转移；`backlog → done` 直接跳转合法（当天想起、当天做完），`done → scheduled` 非法
- [ ] `cargo test items::no_auto_return_to_backlog` 通过——过期未完成的事项状态不变，仍为 `scheduled`
- [ ] `cargo test items::archived_is_distinct_from_done` 通过——两个终态在查询中可区分
- [ ] `cargo test items::actual_minutes_is_optional_integer` 通过——可空且为整数分钟，无浮点
- [ ] `cargo test items::draft_item_allows_null_source` 通过——与 `draft_transactions` 的非空约束区分
- [ ] `cargo test items::draft_item_still_requires_evidence_text` 通过——口述场景下 `evidence_text` 存原话，仍非空
- [ ] `cargo test items::confirm_writes_audit` 通过
- [ ] `npm test -- items/bulk-input` 通过——多行文本一行一条进 backlog
- [ ] `npm test -- items/week-rollup` 通过——「这周时间花哪了」的汇总，未记时长的条目单独计数不混入总和
- [ ] `npm test -- items/unfinished` 通过——「什么没做完」只返回 `scheduled_on` 已过且非 `done`/`archived` 的事项

**人工验收**：

- [ ] 一次说五件事（可用 macOS 系统听写），五条进 backlog，拖两条到明天，其中一条标完成并记 90 分钟
- [ ] 事项视图与交易视图是两个独立视图，同一屏不混显钱和时间

## 7. 回流记录

*（尚无——本 sub-PRD 未开工。实现证伪规格时先回写这里，再改代码。）*

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.1 | 2026-08-06 | 初版：四态生命周期（含 `archived` 与 `done` 区分、不自动退回 backlog）、只记时长不记时段、一句话批量丢入与 v1 语音方案、事项草稿的 `source_id` 可空及其理由、两个基础问答、独立视图；否决方案七条；待决 R1–R5；验收标准 12 条可执行 + 2 条人工 |
