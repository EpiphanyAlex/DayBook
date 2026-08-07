---
title: 03 审核与草稿区 — 草稿区、证据链、总额校验与审核界面
status: ready
owner: "@alex"
date: 2026-08-07
version: v0.2
---

# 03 · 审核与草稿区

> **产品的胜负手。** 整个产品省下的时间全兑现在这一屏。
> 依据：[ADR-0002 AI 永不直接写入](../adr/0002-ai-never-writes-directly.md)（本模块是它四道闸门的主要落地处）。

## 1. 问题

**视觉模型真的会把 168 读成 1680**，而账本错一个数字用户信任就永久归零——**归零不可逆**：一旦用户开始「偷偷回去核对原始截图」，产品声称节省的时间就全部吐了回去（[`docs/PRD.md` §7](../PRD.md) 成功标准）。

所以本模块要同时解决两个互相拉扯的目标：

1. **可信**——用户必须能确认每一条，而不是被迫信任 AI；
2. **快**——**40 笔 30 秒审完**。**做成 30 秒是成功，做成 20 分钟这产品就没意义。**

把这两条同时做到的唯一办法，是**把用户的心智负担从「判断 AI 对不对」换成「扫一眼原文，对得上就过」**，再用总额交叉校验把「必须逐条细看」的范围压缩到少数异常项。

## 2. 范围与非目标

**范围**：`draft_*` 表与事实表的隔离 · 确认动作（单条 / 批量） · 证据链呈现（原文并排） · 总额交叉校验 · 异常前置排序 · 键盘流 · 行内编辑 · 虚拟滚动 · 每次纠正的审计与记忆投递。

**非目标**：

- **草稿的产生**——属 [01 Agent 运行时](./01-agent-runtime.md)（工具）与 [02 导入](./02-ingest.md)（编排）
- **交易的字段语义与回顾视图**——属 [04 交易](./04-transactions.md)
- **记忆规则的存储与匹配**——属 [06 记忆](./06-memory.md)；本模块只负责**投递纠正事件**
- **语音审核**——[ADR-0005 §3](../adr/0005-voice-and-system-integration.md) 永久排除：没法用嘴说「第 7 条金额改成 168」

## 3. 决定与依据

### 3.1 两条写入路径的物理隔离

依据 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 1、[`docs/architecture.md` §3](../architecture.md)。

```
路径 A（AI 可达）：MCP 工具 → domain::draft → draft_* 表
路径 B（AI 不可达）：Tauri command → domain::confirm → 事实表
```

**`domain::confirm` 不被任何 MCP 工具调用。** 确认动作只能由前端的 Tauri command 触发。实现手段是**模块边界**——`mcp/` 只依赖 `domain::draft`，拿不到 `domain::confirm`，越权在编译期不可表达（[`.claude/rules/rust-tauri.md` §4](../../.claude/rules/rust-tauri.md)）。

草稿被确认后**标记为已消费而非删除**（`draft_transactions.consumed_at` 置非空，[00 地基 §3.6](./00-foundation.md)）。

**溯源在 schema 层的落点**（2026-08-07 M0 开工评审补齐）：确认动作写出的 `transactions` 行带 **`source_draft_id`** 指回原草稿。此前本节承诺「审计能回答入库的这条当初 AI 起草成什么样」，但 schema 里没有任何字段兑现它——只靠 `audit_log` 的 `before_json` 做不到高效反查。

**确认时的完整性校验**（服务端，不依赖 UI）：

| 检查 | 不过时返回 |
|---|---|
| 草稿有 `source_id` 且 `evidence_text` 非空 | `review.missing_evidence` |
| 草稿三元组齐全（`base_amount_minor` / `base_currency` / `rate_ppm` 均非空） | `review.incomplete_triple` |
| 批量确认时该来源总额校验为 `passed` | `review.total_mismatch` / `review.total_unavailable` |

三元组那条对应 [00 地基 §3.6](./00-foundation.md)「草稿可空、确认必填」与 [04 交易 §3.2](./04-transactions.md)「缺汇率不入库」。

### 3.2 证据链呈现

- 每条草稿必带 `source_id` + `evidence_text`（数据层非空，见 [00 地基 §3.6](./00-foundation.md)）
- **审核界面必须把原文与解析结果并排呈现**——不是「点开看大图」，是默认可见
- 点任一条 → 证据面板高亮该条对应的原图区域；**若无法定位到区域，至少显示原文片段**（区域定位是加分项，原文片段是底线）
- **无证据的草稿不得入库**——确认动作在服务端再校验一次，不依赖 UI

### 3.3 总额交叉校验

**这是唯一能在无人工介入下捕获错误的机制，优先级高于解析准确率本身**（[ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 3）。

#### 校验式（2026-08-07 M0 开工评审定死）

原文只写「`SUM(该来源的草稿金额) == sources.declared_total_minor`」，**「草稿金额」是原币还是本位币没说**——外币账单的声明合计是外币，而逐笔折算后求和还会引入舍入误差。不定死则 M0/M1 必错。

**校验在「来源声明合计的那个币种」（`declared_total_currency`）上做。** 对该来源下每条**未消费**草稿，取它在该币种下的金额：

| 情形 | 取值 |
|---|---|
| 草稿原币 == `declared_total_currency` | `amount_minor` |
| 草稿原币 != `declared_total_currency` | `base_amount_minor`，**且要求 `base_currency == declared_total_currency`** |

求和后与 `declared_total_minor` **整数精确相等**，无容差（这是禁用浮点的直接理由，见 [00 地基 §3.4](./00-foundation.md)）。

> **为什么这样取**：账单的合计必然以账单自身的币种印刷，而账单上的外币消费**同时印着折算后的本币金额**——这正是 [04 交易 §3.2](./04-transactions.md) 汇率路径 1「从截图反推」的适用场景。取 `base_amount_minor` 才能让外币行参与合计，取原币则永远对不上。

#### 三种结果

| 结果 | 条件 | 行为 |
|---|---|---|
| `passed` | 合计精确相等 | 允许批量确认 |
| `failed` | 合计不等 | **显式报警并阻止批量入库**（`review.total_mismatch`）；用户仍可逐条确认（逐条是用户自己核对过的） |
| `unavailable` | ① 来源未声明合计（`declared_total_*` 为空）；**或** ② 存在草稿取不到该币种下的金额（缺三元组、或 `base_currency` 与声明币种不匹配） | UI 明确标注**无法校验**并**列出是哪几条导致的**；批量确认被拒（`review.total_unavailable`）。**不伪装成通过，也不谎报 failed**——「算不出来」和「算出来不对」是两回事 |

- **不提供 `--force` / 「忽略警告继续」类旁路**（[ADR-0002](../adr/0002-ai-never-writes-directly.md)「对实现的硬性要求」第 3 条）

#### 基准值本身必须可核对

**这道闸门有一个结构性边界，规格必须写明。** 校验的两边——逐笔草稿与声明合计——**都由同一个 agent 在同一次会话里产生**（[01 Agent 运行时 §3.2](./01-agent-runtime.md)）。它能捕获「逐笔读错但合计读对」，**捕获不了「逐笔和合计一起读错」**。

对策是让基准值也接受人的检查：

- `report_source_total` 强制携带 `declared_total_evidence_text`（合计在来源上的原文片段），数据层有 all-or-nothing CHECK（[00 地基 §3.6](./00-foundation.md)）
- **审核界面把声明合计与它的原文片段并排显示在批量确认按钮附近**——用户按下批量确认前，最后看到的一眼就是「账单上印的合计是 1847.20，系统读到的也是 1847.20」
- 来源上没印合计时 agent 不得自己算一个（[01 Agent 运行时 §3.2](./01-agent-runtime.md)），结果如实为 `unavailable`

**一句话**：闸门 3 把「逐条核对 40 笔」压缩成「核对 1 个合计」，但压缩不到零。

### 3.4 异常前置

排序优先级（高 → 低）：

1. 总额校验 `failed` 的来源下的全部条目
2. 跨图疑似重复（[02 导入 §3.6](./02-ingest.md)）
3. agent 标注的低置信条目
4. 与记忆规则冲突的条目（例：这家商户历史上一直归「餐饮」，这次被起草成「购物」）
5. 其余，按交易日期

**理由**：用户的注意力应该花在可疑项上，而不是均匀分给 40 条。这是「40 笔 30 秒」在信息架构上的实现方式。

### 3.5 键盘流（M1 核心）

**目标：全流程不碰鼠标。**

| 键 | 动作 |
|---|---|
| `↑` / `↓` | 上下移动焦点 |
| `Space` | 切换选中 |
| `Enter` | 进入行内编辑 |
| `Esc` | 退出编辑 / 取消 |
| `Cmd+Enter` | 确认已选条目入库 |
| `Cmd+A` | 全选 |
| `D` | 丢弃当前条目 |

- **默认全选**——多数条目是对的，用户的动作应该是「取消掉不对的」而不是「勾选对的」
- 行内编辑直接改金额/商户/分类/日期，不弹模态框

### 3.6 每次纠正都留痕并投递记忆

- 用户对草稿的任何修改 → 写 `audit_log`（`actor = "human"`，含 before/after，[ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 4）
- 同时投递一个**纠正事件**给 [06 记忆](./06-memory.md)：`(来源商户文本, 原分类, 改后分类)` 等
- **记忆的写入不阻塞确认**——记忆失败不能挡住入库

### 3.7 性能

- 单次审核可能有数百条 → **虚拟滚动**
- 证据图按需加载，不一次性把整批原图读进内存

## 4. 否决的替代方案

| 方案 | 否决原因 |
|---|---|
| 单表 + `is_draft` 状态字段 | 状态字段是软约束，下游查询漏一处过滤就把 AI 的猜测当成了事实（[ADR-0002](../adr/0002-ai-never-writes-directly.md)「理由」、[00 地基 §4](./00-foundation.md)） |
| 证据只在点击后弹出大图 | 审第 7 条时要自己在图上找到第 7 行——审核成本随条目数线性增长，40 笔 30 秒做不到（[ADR-0002](../adr/0002-ai-never-writes-directly.md)「理由」） |
| 总额校验只警告不阻止 | 那就等于没有闸门。用户在批量确认时不会停下来读警告 |
| 总额校验带容差（如 ±1 分） | 容差会掩盖真实的解析错误；整数存储的全部意义就是让精确相等成立（[00 地基 §3.4](./00-foundation.md)） |
| 默认不选，让用户逐条勾 | 多数条目是对的，逐条勾是把 O(正确项) 的成本强加给用户；默认全选让成本只落在 O(错误项) |
| 用聊天框审核（「把第 7 条改成 168」） | 交互形态明确是「对话下指令 + 界面做审核」（[`docs/PRD.md` §5.3](../PRD.md)）；自然语言定位行号比直接点它慢一个量级 |
| 语音审核 | [ADR-0005 §3](../adr/0005-voice-and-system-integration.md) 永久排除 |
| 确认后删除草稿 | 审计无法回答「入库的这条当初 AI 起草成什么样」 |

## 5. 待决与风险

| # | 事项 | 影响 | 谁来决 / 何时 |
|---|---|---|---|
| R1 | 证据区域定位（在原图上高亮对应行）技术上能否稳定做到——取决于 agent 能否可靠回报坐标 | 本文 §3.2 | M1 开工前 spike；做不到就退到「只显示原文片段」，**结果回流本文** |
| R2 | 舍入规则若与来源自身不一致，总额校验会系统性失败（[00 地基 §5](./00-foundation.md) R3） | 本文 §3.3 | M2 实测真实对账数据 |
| R3 | 前端状态管理选型（[`docs/architecture.md` §8](../architecture.md) 未决 A4） | 本文全部 UI | M1 开工前，@alex 决 |
| R4 | 「40 笔 30 秒」如何客观测量——秒表人测的方差可能大于优化幅度 | 本文 §6 人工验收 | M1 开工前定测量协议，**写进本文 §6** |
| R5 | 低置信标注依赖 agent 自评，而模型的自评校准度未知 | 本文 §3.4 排序第 3 优先级 | M1 实测；不可靠则降权或去掉该维度。字段 `draft_transactions.confidence` 已在 [00 地基 §3.6](./00-foundation.md) 留好且可空，**不阻塞 M0** |
| R6（**新增 2026-08-07**） | §3.3 的舍入敏感性——逐笔 `base_amount_minor` 各自舍入后求和，与账单印刷的合计可能差几分（[00 地基 §5](./00-foundation.md) R3 的具体失败模式） | 本文 §3.3 校验式 | M2 实测真实外币账单；若系统性偏差成立，可能需要在**外币行参与合计**这条路径上另立规则，**结果回流本文与 [00 地基](./00-foundation.md)** |

## 6. 验收标准

本模块横跨 M0（最朴素的确认列表）与 M1（审核界面做深），验收**按里程碑分层**——M0 只需闸门与确认逻辑成立，键盘流与排序属 M1。

#### M0 必过

- [ ] `cargo fmt --all -- --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test` 全绿
- [ ] `npm run lint` · `npm run typecheck` · `npm test` · `npm run build` 全绿
- [ ] `rg -n 'confirm' src-tauri/src/mcp` 无命中——`mcp/` 不引用确认动作（**替换原验收** `review::confirm_not_reachable_from_mcp`：`cargo test` 做不了静态调用图分析，那条写法无法实现）
- [ ] `cargo test review::confirm_rejects_draft_without_evidence` 通过——服务端二次校验返回 `review.missing_evidence`
- [ ] `cargo test review::confirm_rejects_incomplete_triple` 通过——三元组不齐时返回 `review.incomplete_triple`（§3.1 完整性校验）
- [ ] `cargo test review::total_check_exact_equality` 通过——差 1 分即 `failed`
- [ ] `cargo test review::total_check_uses_declared_currency` 通过——原币 == 声明币种时取 `amount_minor`、不等时取 `base_amount_minor`，混合币种来源能正确求和（§3.3 校验式）
- [ ] `cargo test review::total_check_unavailable_when_not_declared` 通过——`declared_total_*` 为空时结果为 `unavailable`，批量确认路径不把它当 `passed`
- [ ] `cargo test review::total_check_unavailable_when_amount_unobtainable` 通过——存在缺三元组或 `base_currency` 不匹配的草稿时结果为 `unavailable`（**不是 `failed`**），且返回体列出是哪几条
- [ ] `cargo test review::batch_confirm_blocked_when_total_failed` 通过——返回 `review.total_mismatch`
- [ ] `cargo test review::batch_confirm_blocked_when_total_unavailable` 通过——返回 `review.total_unavailable`
- [ ] `cargo test review::no_force_bypass_exists` 通过——确认相关命令的参数里不存在 force/ignore 类旁路
- [ ] `cargo test review::single_confirm_allowed_when_total_failed` 通过——逐条确认仍可用
- [ ] `cargo test review::every_edit_writes_audit` 通过——每次修改后 `audit_log` 多一条且含 before/after
- [ ] `cargo test review::confirmed_draft_is_marked_not_deleted` 通过——`consumed_at` 置非空，行仍在
- [ ] `node scripts/verify-m0.mjs`（**待建**，检查项定义见 [`docs/PRD.md` §9.3](../PRD.md)）退出码 0

**M0 人工验收**：

- [ ] 每条草稿的原文片段与解析结果在同一屏可见，无需额外点击
- [ ] 声明合计与它的原文片段显示在批量确认按钮附近（§3.3「基准值本身必须可核对」）
- [ ] 总额不符或无法校验时，提示可见且批量确认按钮不可点

#### M1 必过（在 M0 全部通过之上）

- [ ] `npm test -- review/sorting` 通过——异常前置的五级排序按 §3.4 优先级
- [ ] `npm test -- review/keyboard` 通过——§3.5 全部快捷键有对应处理，且默认全选
- [ ] **40 笔真实草稿，从打开审核界面到全部入库，不碰鼠标，≤ 30 秒**（测量协议见 §5 待决 R4，**M1 开工前必须先把协议写进本节**）

## 7. 回流记录

*（尚无——本 sub-PRD 未开工。实现证伪规格时先回写这里，再改代码。）*

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.1 | 2026-08-06 | 初版：两条写入路径的物理隔离、证据链呈现要求、总额校验三态（含 `unavailable` 不伪装成通过、无 force 旁路）、异常前置五级排序、键盘流键位表与默认全选、纠正留痕与记忆投递、虚拟滚动；否决方案八条；待决 R1–R5；验收标准 14 条可执行 + 3 条人工 |
| v0.2 | 2026-08-07 | **M0 开工评审 → `status: ready`。** ① §3.3 **校验式定死**——原文「`SUM(该来源的草稿金额)`」未说原币还是本位币，外币账单必错；现明确在 `declared_total_currency` 维度求和，原币同币种取 `amount_minor`、异币种取 `base_amount_minor`。② §3.3 `unavailable` 扩为两种触发条件（未声明合计 **或** 存在取不到该币种金额的草稿），并要求列出是哪几条——「算不出来」与「算出来不对」不可混为一谈。③ §3.3 新增**「基准值本身必须可核对」**：如实写明闸门 3 的结构性边界（校验两边同源，挡不住逐笔与合计一起读错），对策是强制 `declared_total_evidence_text` 并在批量确认按钮附近与合计并排显示。④ §3.1 补**溯源字段 `source_draft_id`**（原文承诺审计能回答「当初起草成什么样」，schema 无落点）与确认时的三项完整性校验及对应错误码。⑤ §6 验收**按 M0/M1 分层**，把无法实现的 `confirm_not_reachable_from_mcp` 改为 `rg` 检查，新增 5 条校验式相关用例。⑥ §5 新增 R6（外币行参与合计的舍入敏感性） |
