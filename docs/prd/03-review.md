---
title: 03 审核与草稿区 — 草稿区、证据链、总额校验与审核界面
status: review
owner: "@maintainer"
date: 2026-09-06
version: v0.22
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

**范围**：`draft_*` 表与事实表的隔离 · 确认动作（单条 / 批量） · 证据链呈现（原文并排） · 总额交叉校验 · 异常前置排序 · 键盘流 · 行内编辑 · 虚拟滚动 · 每次纠正的审计与记忆投递 · M2 分类体系操作的影响预览与确认。

**M0 的界面是功能基线，不是设计定稿。** 它只锁定信息架构、可信闸门、状态反馈、可访问性与可操作性；当前 `src/styles.css` 的局部变量不是已定案 token 的实现。M1 的设计输入已经确定：token 事实源是 [`design.md`](../../design.md) v0.5，当前页面布局参考是 [`docs/design/desktop-v9.dc.html`](../design/desktop-v9.dc.html)，且规格与 token 冲突时不得照抄参考稿。维护者于 2026-09-06 只开放 §3.9 的有限 M1 并行切片；完整键盘流、虚拟滚动、运行事件、异常排序与「40 笔 30 秒」仍属于 M1 整体范围。这个分期不允许任何切片省掉原件同屏、报警可见、禁用态和键盘可达等既有功能要求。

**非目标**：

- **草稿的产生**——属 [01 Agent 运行时](./01-agent-runtime.md)（工具）与 [02 导入](./02-ingest.md)（编排）
- **交易的字段语义与回顾视图**——属 [04 交易](./04-transactions.md)
- **记忆规则的存储与匹配**——属 [06 记忆](./06-memory.md)；本模块只负责审核并投递**纠正事件或用户明确发起的规则提案**
- **让语音执行最终确认**——[ADR-0005 §3](../adr/0005-voice-and-system-integration.md) 永久排除：系统听写可以形成事项修改指令，但目标匹配、字段 diff 与「第 7 条金额改成 168」这类精确审核仍须视觉界面确认

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
| 批量确认时 `confirmation_policy` 不为 `single_only`（§3.3） | `review.total_mismatch` / `review.total_unavailable` |

三元组那条对应 [00 地基 §3.6](./00-foundation.md)「草稿可空、确认必填」与 [04 交易 §3.2](./04-transactions.md)「缺汇率不入库」。

M0 首屏同时呈现当前本位币选择；修改只影响之后的新解析。已有草稿的三元组逐条保留，不能因切换偏好被后台改写（[00 地基 §3.4](./00-foundation.md) v0.13）。

### 3.2 证据链呈现

#### 证据是原件，`evidence_text` 是抽取声明（2026-08-10 收紧，[ADR-0002 闸门 2](../adr/0002-ai-never-writes-directly.md)）

本节 v0.1–v0.4 把 `evidence_text` 称作「原文」，与「来源原件」混用。**两者的可信性不同级，混用会让闸门 2 在 M0 变成一个空动作**：

| | 是什么 | 谁产出 | 能不能当核对基准 |
|---|---|---|---|
| **来源原件** | 截图字节 / `utterance` 的转写文本 | 导入时逐位落盘，之后谁也不改 | **能。这是证据** |
| **`evidence_text`** | 「我读的是这一段」 | **和金额同一次模型输出** | **不能。它是抽取声明** |

**模型把 168 读成 1680 时，它同样可能把 `evidence_text` 写成「1680」——两者自洽，却一起错。** 于是「原文与解析结果并排」如果并排的是 `evidence_text`，用户核对的是模型和它自己，闸门 2 什么也没挡住。

**因此 `evidence_text` 的定位是「原件上的哪个位置」，不是「原件上写的是什么」。**

#### 硬要求

- 每条草稿必带 `source_id` + `evidence_text`（数据层非空，见 [00 地基 §3.6](./00-foundation.md)）
- **审核界面必须让来源原件本身可见**：截图渲染出来、`utterance` 的转写文本原样显示。**不是「点开看大图」，是默认可见**
- **M0 就要有原件**（2026-08-10 改）：此前 [`docs/PRD.md` §9.2](../PRD.md) 把「证据图面板」整体推到 M1，M0 只渲染 `evidence_text` 那一列——那样 M0 验的是「模型抄得像不像」，验不出「模型读得对不对」，而后者正是 M0 存在的理由。**M0 的最小形态是「原图缩略图 + 点击看原图 + `evidence_text` 同屏」**，一个 `<img>` 的成本。M0 当时只把区域高亮留给 R1 spike，**不是推迟原件本身**；该 spike 的最终结论见下一条
- **截图不做伪精确的区域高亮**（2026-08-24，§5 R1 已关闭）：实测 agent 坐标会漂到相邻行，错误高亮比没有高亮更危险。点任一条时保持来源原件与该条 `evidence_text`（抽取声明）并列；`utterance` 仍用可逐字验证的 code-point span 高亮转写原文
- **无证据的草稿不得入库**——确认动作在服务端再校验一次，不依赖 UI

### 3.3 总额交叉校验

**这是唯一能在无人工介入下捕获错误的机制，优先级高于解析准确率本身**（[ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 3）。

#### M0 先判 claim 范围，再谈等式（2026-08-30 no-go 回流）

`total_check` 的等式只有在 `report_source_total` 报的是**当前不可变来源全部适用交易**时才有意义。来源可以是任意 viewport 截图；边界就是导入后不再改变的那些字节，不是截图背后的整页、整月或其他分页。`expense_total` 必须覆盖来源内全部支出，`income_total` 覆盖全部收入，`net_change` 覆盖全部收入与支出。

覆盖 viewport 外交易的月度 / 周期合计、分页合计、按日 / 分类 / 语义上单笔 / 任意子组合计均为 **scope-invalid**，不得进入 `parse_attempts.reported_total_*`。多个局部 claim 不能任选一条；若有效来源级 claim 与 invalid decoy 恰有相同 amount/currency/kind，现有四列无法审计身份，M0 也不得报告。合计词只证明「这里可能有个数字」，不能证明 scope；未满足资格时保持未报告，`file` 为 `unavailable + single_only`，`utterance` 为 `not_applicable + user_attested_batch`。

**M0 不新增生产 scope 字段或多 claim 表。** domain 没有独立 OCR，无法从图像字节证明 agent 选中的 claim 是否全覆盖；因此生产 `total_check` 继续在单 claim 前提下执行下面的确定性等式，而范围误报被定义为解析缺陷，由 [01 Agent 运行时 §3.2](./01-agent-runtime.md) 的提示词契约收窄，并由 [07 评测 §3.4](./07-eval.md) 的范围真值与「scope-invalid 成功报告数必须为 0」正式硬契约检验。不能把生产侧无法独立证明写成「已验证 scope」。

#### 校验的对象是「解析完整性」，不是「这一批要确认什么」（2026-08-10 修正）

**这是一个会在 M0 第一次逐条确认时就复现的缺陷。** 本节 v0.2–v0.4 写的求和范围是「该来源下每条**未消费**草稿」。而 `failed` 时用户仍可逐条确认（见下方结果表），**确认第一条之后它就变成已消费、退出求和范围**——剩下的和永远小于声明合计，该来源从此**再也回不到 `passed`**。全部确认完后求和为 0，校验结果仍是 `failed`。

**病根是把两件正交的事捆在了一起**：

| 问题 | 求和范围 | 什么时候变 |
|---|---|---|
| **解析完整性**——agent 这一次从来源读出来的东西，合起来对不对得上它这一次报告的合计 | **全部未作废**草稿，**不论是否已消费**（范围还要再按尝试收窄，见下一小节） | 只随解析与用户编辑变，**不随确认变** |
| **这一批能不能确认** | 用户当前勾选的那些 | 每次勾选都变 |

**决定：总额校验只回答第一个问题，求和范围是「该次尝试全部 `voided_at` 为空的草稿」**（[00 地基 §3.6](./00-foundation.md)）。它是**那次解析尝试的一个属性**，确认动作不改变它。批量确认的闸门是「读一下这个属性」，不是「拿这一批去算一遍」。

#### 入参是 `attempt_id`，不是 `source_id`（2026-08-10 修正）

上一段的初稿写的是「该**来源**全部未作废草稿」。**那还不够**——它修好了「确认会改变校验结果」，没修好「**重试会把两次尝试的输出混在一起**」：

- 一个来源被成功解析两次（用户重新解析了一遍），两次的草稿都没被作废 ⇒ 求和把 24 条当成 12 条来源的输出，必然 `failed`
- 合计原本存在 `sources` 上，第一次尝试超时后草稿按 `attempt_id` 作废、**而合计留了下来**，于是拿第一次的合计去校验第二次的草稿

**病根同上一条：把某次尝试的输出记在了来源上。** 合计已随 [00 地基 §3.6](./00-foundation.md) 移到 `parse_attempts.reported_total_*`，校验的入参因此也是 `attempt_id`：

```
total_check(attempt_id) → { reconciliation_status, confirmation_policy, … }
```

**`sources.latest_attempt_id` 决定审核界面当前看的是哪一次的输出。** 旧尝试的草稿与合计原样留在库里——它们是 [07 评测](./07-eval.md) 最想要的失败样本，不删。

#### 校验式

**校验在「本次尝试报告的合计所用的那个币种」（`reported_total_currency`）上做**，且**按 `reported_total_kind` 选等式**（2026-08-10 新增，[00 地基 §3.6](./00-foundation.md)「合计必须带类型」）。

第一步，对该次尝试下每条未作废草稿，取它在该币种下的金额：

| 情形 | 取值 |
|---|---|
| 草稿原币 == `reported_total_currency` | `amount_minor` |
| 草稿原币 != `reported_total_currency` | `base_amount_minor`，**且要求 `base_currency == reported_total_currency`** |

> **为什么这样取**：账单的合计必然以账单自身的币种印刷，而账单上的外币消费**同时印着折算后的本币金额**——这正是 [04 交易 §3.2](./04-transactions.md) 汇率路径 1「从截图反推」的适用场景。取 `base_amount_minor` 才能让外币行参与合计，取原币则永远对不上。

第二步，按 `reported_total_kind` 求和：

| `reported_total_kind` | 等式 |
|---|---|
| `expense_total` | Σ（`direction = expense` 的条目）== `reported_total_minor` |
| `income_total` | Σ（`direction = income` 的条目）== `reported_total_minor` |
| `net_change` | Σ(`income`) − Σ(`expense`) == `reported_total_minor` |

**整数精确相等，无容差**（这是禁用浮点的直接理由，见 [00 地基 §3.4](./00-foundation.md)）。

> **`direction = transfer` 的条目怎么办**：**它让结果变成 `unavailable`**，不是被忽略也不是被当成支出。转账在单账户视角下的方向取决于它是转出还是转入，而 `direction` 只有一个 `transfer` 值、没有符号——**这个信息 schema 里就没有，硬算等于编一个**。真实账单里转账行常见，所以这不是边缘情况；它并入 [00 地基 §5](./00-foundation.md) R7（一来源多条 claim）在 M2 一起处理。
>
> **为什么不是「无差别求和」**：一张同时含消费与退款的账单，底部印着「本期消费合计」，无差别求和必然对不上——而它会以 `failed` 报出来，**看起来像 agent 读错了**。用错误的等式产生的报警，比不报警更糟：它训练用户忽略报警。

#### 两个维度，不是一个枚举（2026-08-10 拆开）

`total_check` 返回**两个字段**，因为它回答的本来就是两个问题：

```
reconciliation_status:  passed | failed | unavailable | not_applicable   ← 能不能做总额校验
confirmation_policy:    reconciled_batch | user_attested_batch | single_only   ← 能不能批量确认
```

**为什么必须拆**：初稿让 `not_applicable` 自己就意味着「允许批量」，那是把「校验不适用」和「换一道人工闸门就放行」焊死在一个值上。三个已经看得见的场景会立刻把它撑破： <!-- legacy -->

- 用户在口述里明确说出一条**覆盖整段全部适用交易**的来源级合计 —— 这时对账可做（`report_source_total` 照常调，[00 地基 §3.6](./00-foundation.md)「M0 单 claim 的范围资格」），结果是 `passed` / `failed`，而确认策略仍是用户背书那一档。**白拿一道校验，且不改变放行方式**；只覆盖单笔或子组的「总共」不属于此例
- 一张**只有一笔、没印合计**的截图 —— 直觉上「对账不适用」，但它是 `file`，**既不该因此获得批量放行，M0 也不该判它 `not_applicable`**。理由见下方专门一段
- M3 的一段口述**同时产生交易与事项** —— 两类条目的确认路径不同，一个枚举表达不了

#### `file` 为什么永远不判 `not_applicable`（2026-08-10 补上真正的理由）

上一版拿「单笔截图没有合计」当例子时，隐含了一句「它理论上确实对账不适用」。**那个说法把问题的位置放错了。** 真正的理由与「这张图该不该有合计」无关，而是——

**我们手上唯一的信号是 `reported_total_* IS NULL`，而这一个信号对应三种完全不同的现实：**

1. 原件**结构性没有**合计（真的只印了一笔）
2. agent **漏读了**那一行合计
3. 用户截图时**把底部裁掉了**

**从「没读到」反推「本来就没有」，是把第 2、3 种伪装成第 1 种。** 而第 2 种正是闸门 3 存在的全部理由，第 3 种是用户能自己修好的问题——两者都必须被看见。

**所以 M0 保守判：`file` + 没报合计 ⇒ `unavailable` + `single_only`。** 代价是那张真的只有一笔的截图也要逐条确认——**一条而已，而误判的代价是一整类漏读变得不可见。**

**将来真要支持 `file` 的 `not_applicable`，需要一个明确的适用性信号**（来源画像、或 agent 显式声明「这张图的版式里不存在合计行」并附证据），**不能从字段为空推出来**。这与「余额不当合计用」「类型判不出就不填」是同一条纪律：**缺少信息时不许猜一个语义出来。** 登记为 §5 R7。

**M0 不把两个结果或 claim scope 落成数据库字段**：结果仍由 `total_check` 当场算出并一起返回，`reported_total_*` 仍是单 claim 四列。多 claim / scope schema 是 M2 的事。**但概念现在就分开**，否则 UI 与 domain 会围着一个混合语义的枚举写一遍，将来两次返工。

**`reconciliation_status` 的四个值：**

| 值 | 条件 | 行为 |
|---|---|---|
| `passed` | 按上式精确相等 | — |
| `failed` | 不等 | **显式报警**（`review.total_mismatch`） |
| `unavailable` | ① 来源**本该有**合计而本次尝试没拿到（`reported_total_*` 为空且 `kind = file`）；**或** ② 存在草稿取不到该币种下的金额（缺三元组、或 `base_currency` 与合计币种不匹配）；**或** ③ 存在 `direction = transfer` 的条目 | UI 明确标注**无法校验**并**列出是哪几条导致的**。**不伪装成通过，也不谎报 failed**——「算不出来」和「算出来不对」是两回事 |
| `not_applicable` | 当前来源没有 scope-valid 合计：`kind = utterance` 且本次尝试没报合计（[00 地基 §3.6](./00-foundation.md)）。**只有口述里存在 current-source 全覆盖且三元组在候选中唯一的合计时才不落在这里**；月度 viewport 外、单笔或子组「总共」仍在这里 | — |

**`confirmation_policy` 的三个值：**

| 值 | 何时 | 批量确认 |
|---|---|---|
| `reconciled_batch` | `reconciliation_status == passed` | **可以**，理由是机器对上了账 |
| `user_attested_batch` | `kind = utterance` 且下方三条 UI 硬要求全部满足。**与对账结果无关**——口述说了合计、对上了或没对上，策略都是这一档 | **可以**，理由是**人对着整段原文背书了** |
| `single_only` | 其余一切 | **不可以**，只能逐条（`review.total_mismatch` / `review.total_unavailable`） |

- **`kind = file` 永远拿不到 `user_attested_batch`**（[ADR-0002 闸门 3](../adr/0002-ai-never-writes-directly.md)）。一张账单取不到合计是一个**信号**——要么 agent 漏读了、要么图不完整——它必须落到 `single_only` 里被看见
- **真正放行批量的是 `confirmation_policy`，不是 `not_applicable`。** 这正是拆开的意义：口述里说了合计时，`reconciliation_status` 可以是 `passed` 或 `failed`，而策略仍按口述那一档走
- **不提供 `--force` / 「忽略警告继续」类旁路**（[ADR-0002](../adr/0002-ai-never-writes-directly.md)「对实现的硬性要求」第 3 条）。**`user_attested_batch` 不是旁路**：它没有放松任何东西，只是把校验动作从「机器比对合计」换成了「人比对整段原文」

#### `user_attested_batch` 换的是另一道闸门（2026-08-10 新增，产品决定）

**背景**：[`docs/PRD.md` §1.1](../PRD.md) 把「说一段话 → agent 拆成多笔 → **一次批量确认**」作为相对竞品语音速记的唯一差异。而本节 v0.2–v0.4 让 `utterance` 恒为 `unavailable`、批量确认必被拒——**产品的核心差异被自己的闸门挡死了**，`docs/PRD.md` §9.3 第 9 步甚至把「批量确认被拒」写成了 M0 验收。两边不能都留。

**决定：给 `utterance` 独立的信任策略。** 理由是这两种来源缺合计的**原因不同**：

- 一张账单**本该**印着合计，取不到 ⇒ 出问题了 ⇒ 该挡
- 一段口述**从构造上**就没有合计 ⇒ 永远挡 ⇒ 这道闸门对它不产生任何信息，只产生摩擦

**但闸门不能凭空少一道，所以换一道人工的。** `user_attested_batch` 允许批量确认，代价是三条 UI 硬要求（**缺一则确认策略退回 `single_only`**）：

1. **整段转写原文全文可见**——不是片段、不是折叠、不是「点开看」。用户按下批量确认前，看到的是他说过的**整段话**
2. **全部拆分结果与原文并排**，条数显式呈现（「这段话拆出了 5 条」）
3. **每条的 `evidence_text` 在原文中的位置可对照**——哪半句变成了哪一条

**这道人工闸门比合计校验更强还是更弱，如实说**：更弱。合计校验能捕获「逐笔读错但合计读对」，人眼扫一遍不能保证。但**口述来源的原文是用户自己刚说出口的**，他对内容有第一手记忆——这是截图来源不具备的条件（截图是别人排版的、几周前的）。**这个差别是本决定成立的全部依据**，它不迁移到任何其他来源类型。

**M0 就要验这条**：[`docs/PRD.md` §9.3](../PRD.md) 第 9 步相应改写。

#### 基准值本身必须可核对

**这道闸门有一个结构性边界，规格必须写明。** 校验的两边——逐笔草稿与声明合计——**都由同一个 agent 在同一次会话里产生**（[01 Agent 运行时 §3.2](./01-agent-runtime.md)）。它能捕获「逐笔读错但合计读对」，**捕获不了「逐笔和合计一起读错」**。

对策是让基准值也接受人的检查：

- `report_source_total` 强制携带 `reported_total_evidence_text`（合计在来源上的原文片段），数据层有 all-or-nothing CHECK（[00 地基 §3.6](./00-foundation.md)）
- **审核界面把声明合计与它的原文片段并排显示在批量确认按钮附近**——用户按下批量确认前，最后看到的一眼就是「账单上印的合计是 1847.20，系统读到的也是 1847.20」
- 来源上没有 current-source 全覆盖合计时 agent 不得自己算一个，也不得用月度 viewport 外 / 分页 / 按日 / 分类 / 单笔 / 子组合计顶替（[01 Agent 运行时 §3.2](./01-agent-runtime.md)）；结果按来源 kind 如实为 `unavailable` 或 `not_applicable`

**一句话**：闸门 3 把「逐条核对 40 笔」压缩成「核对 1 个合计」，但压缩不到零。

### 3.4 异常前置

排序优先级（高 → 低）：

1. 总额校验 `failed` 的来源下的全部条目
2. **`kind = utterance` 来源的条目**（2026-08-08 新增，2026-08-10 改述，2026-08-13 补硬要求）——它们的确认策略恒为 `user_attested_batch`，靠的是「整段原文并排 + 一次人工确认」那道闸门，而**不是**机器对账。UI 文案随对账结果分两种：**没报合计**（`not_applicable`）→「语音来源，无合计可校验，请对着原文过一遍」；**报了合计**（`passed` / `failed`）→ 照常显示对账结果，**并在批量确认按钮旁同屏显示「确认前请对着原文过一遍」**。**两种都不让它们看起来和已通过合计校验的截图草稿一样安全**

   > **`failed` 那一档是硬要求，不是文案偏好**（2026-08-13 实施回流）：它是全产品**唯一**一条「机器已经判定对不上，却仍然允许批量确认」的路径。差额本身必须与确认按钮同屏可见，用户要能看出自己正在替机器背书。**放行而不告知，等于两道闸门都没有**——机器那道判了 `failed` 被忽略，人那道因为不知情而没有真的发生。
3. 跨图疑似重复（[02 导入 §3.6](./02-ingest.md)）
4. agent 标注的低置信条目
5. 与记忆规则冲突的条目（例：这家商户历史上一直归「买菜」，这次被起草成「日用」）。**这一档要求 domain 侧也能读 `memory_rules`**——但那是**标记冲突**，不是**覆盖分类**，不违反 [`CLAUDE.md`](../../CLAUDE.md) 约束 15（见 [06 记忆 §3.4](./06-memory.md)）
6. 其余，按交易日期

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
- **分类按里程碑迁移**：M0/M1 继续编辑当前 `category TEXT`；M2 改为 [04 交易 §3.3](./04-transactions.md) 的有效 `category_id` 选择。`direction` 改变时，界面必须把不兼容分类作为同一次人工修改**明确清空**并在 before/after 中可见，可保存为未分类；不得保留旧 scope，也不得静默改成「其他」
- **停用 / 合并墓碑不可新选**（M2）：旧事实只改备注等其他字段时可保留停用分类；活跃草稿若仍指向停用分类，必须改为有效分类或未分类后才能确认

**本位币金额是导出值，不是独立输入**（2026-08-13 实施回流）。行内编辑接受 `amount_minor` / `currency` / `base_currency` / `rate_ppm`，**`base_amount_minor` 由这四项按 [00 地基 §3.4](./00-foundation.md)「币种精度」的换算公式导出**，不接受直接编辑。

**理由不是「少一个输入框」，是三元组自洽必须由构造保证。** `rate_ppm` 是来源上印着的那个数（[`money-and-data.md §2`](../../.claude/rules/money-and-data.md)），用户纠正金额时它不变；把本位币金额也当成独立输入，就存在一个「三者互相矛盾」的可表达状态，而它唯一的守卫是一次事后校验。**本节 v0.1–v0.11 的写法就落进了这个状态**：只改金额时校验必然失败，返回 `data.money_inconsistent`——而那正是本模块 §1 举的头号例子（把 AI 读错的 1680 改回 168）。

原币与本位币相同时 `rate_ppm = 1_000_000`，走同一条换算路径，**不设特例分支**。四项都不给、且草稿本就没有三元组时，导出结果仍是空——确认时照旧 `review.incomplete_triple`。反过来，**用户补齐 `base_currency` 与 `rate_ppm` 即可让缺三元组的草稿变得可确认**，界面必须提供这两个输入（见本节末「三元组补全」）。

**行内编辑不得覆盖 `drafted_json`**（2026-08-10 新增，[00 地基 §3.6](./00-foundation.md)、[ADR-0002](../adr/0002-ai-never-writes-directly.md) 硬性要求 7）：编辑改的是草稿行的业务列，那列不可变快照原样保留。**否则「AI 当初起草成什么样」在用户改完的一瞬间就没了**——[07 评测 §3.2](./07-eval.md) 的整套真值机制建立在它上面，而 §3.1 承诺的「审计能回答当初起草成什么样」也只兑现了一半（`source_draft_id` 只指得到行，指不到行的原始内容）。

**人工丢弃写 `discarded_at`，不写 `voided_at`。** 前者是人的审核决定，后者是超时、取消、协议失败后的系统补偿；两者都保留草稿与 `drafted_json`。总额校验仍包含已丢弃但未作废的草稿，因为它校验的是本次解析是否完整，而不是用户最后选择入库哪些条目。

**三元组补全（M0 即需要）**（2026-08-13 新增）。缺三元组的草稿确认时返回 `review.incomplete_triple`（§3.1），界面必须**同时**给出本位币与汇率两个输入框，让用户当场补齐。**只提示不给入口是死路**：M0 曾出现「确认前需要补全本位币与汇率」的提示，而界面上没有任何地方能补，用户只能丢弃草稿重新解析整个来源。**一条点不动的警告比没有警告更伤信任**，而这一屏的全部意义就是信任（§1）。

### 3.6 每次纠正都留痕；后续切片共用确认闸门

- 用户对草稿的任何修改 → 写 `audit_log`（`actor = "human"`，含 before/after，[ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 4）
- **必须支持「改实体类型」**（2026-08-08 新增，M3 起）：把一条交易草稿转成事项草稿，或反之。
  理由：一句口述里的交易与事项由 agent 判断归属（[05 事项 §3.1](./05-items.md) 的「钱是否已流动」规则），**分类错是高可见、低损害的错误**——用户一眼就看出「这条不该在这」。但草稿分两张表，改类型物理上是「删一条 + 建一条」。若 UI 不提供一键转换，用户只能丢弃后重说一遍，**批处理省下的时间会在这一下被吃掉**。
  转换写两条审计（原表 `discard` + 新表 `create`），并保留 `source_id` / `evidence_text` / `attempt_id` 不变——证据链不因用户改分类而断。
  **`draft_items` 是 M3 的表**（[00 地基 §3.6](./00-foundation.md)），所以本项 **M0 与 M1 都无从实现，验收在 M3**（2026-08-10 修正：§6 此前把 `review/entity-type-switch` 放在「M1 必过」，而 M1 时目标表还不存在——那条验收在 M1 必然挂）

#### 事项的新建与更新共用审核闸门（M3）

依据 [05 事项 §3.4](./05-items.md)：`draft_items` 通过 `operation = create | update` 同时承载新建和对已有事项的修改。

- `resolution_state = ready` 的 update 必须有 `target_item_id`，并把目标事项的 before 与 agent 起草的 tri-state patch（未提及 / 清空 / 新值）并排显示；解析规则是 set 覆盖、clear 清空且抑制计划回退、unspecified 才回退计划。
- `resolution_state = needs_target` 的 update 目标为空，持久化 0–8 个由 [01 `find_item_candidates`](./01-agent-runtime.md) 返回的候选 id；它计入该次完成条数但不可确认。界面只提供「选择候选 / 改为新建 / 忽略」，不得把匹配失败静默转成 create。
- 用户选择候选或改为新建时，domain 修改草稿的当前 operation/target/resolution_state 并写人工审计；`drafted_json` 仍保留 agent 原始 operation、候选与 patch，不再次请求 agent。
- 一段 `utterance` 可以混合交易草稿、事项 create、ready update 与 needs_target update；批量确认只提交可确认且被选中的行，仍要求整段原文、全部结果、条数与每条 `evidence_text` 的对应关系同屏。
- 确认 update 后才写 `items`，并写一条 `actor = human` 的 before/after 审计；agent 的原始草稿与来源证据原样保留。
- 用户在周视图直接拖拽/拉伸不是 agent 草稿：它是确定性人工修改，可直接写事实 + 审计并提供撤销；撤销再写反向审计。

同时投递一个**纠正事件**给 [06 记忆](./06-memory.md)：`(来源商户文本, 原分类, 改后分类)` 等。**记忆写入不阻塞确认**——记忆失败不能挡住事实更新。

#### 分类体系操作的确认闸门（M2）

依据 [04 交易 §3.3](./04-transactions.md) 的分类生命周期与 [01 Agent 运行时 §3.2](./01-agent-runtime.md) 的权限边界。分类管理的主入口可以是对话，但**对话下指令不等于聊天框内直接执行**：

```
自然语言 → agent 生成结构化待确认操作 → domain 重算影响范围
         → UI 显示明细 → 人确认 → domain 执行并审计
```

- **确认前零写入**：agent 的提案不得直接改 `categories`、`transactions`、草稿当前值或 `memory_rules`；取消确认时四者逐位不变
- **影响范围由代码算**：M2 UI 展示分类、事实交易与活跃交易草稿的数量和明细；M3 记忆启用后再纳入商户规则。不能信任 agent 自报的计数，高影响操作在确认按钮同屏显示当前里程碑已存在对象的数量
- **已使用分类的重命名先问意图**：必须选择「只是修改名称」或「分类含义变化」。前者保持 ID、历史统一显示新名；后者转新建 / 拆分，agent 只可提示疑似语义变化，不能替用户裁决
- **合并**：只允许同 scope，确认卡明确写「全部历史都会改到目标分类」；逐项审计共享 `batch_id`，源分类成为 `merged_into_id` 墓碑。M3 记忆启用后，规则重定向属于**同一张合并确认卡**并展示影响，不再逐条另问。若用户只选未来，操作应改为停用而非伪装成合并
- **拆分**：区分「拆出一部分」（原分类继续启用）与「完全拆成多个」（原分类停用）；用户另选仅未来 / 同时整理历史。历史建议按商户分组并允许展开逐笔查看，无法确定项留原分类；只移动整笔交易，**不提供按金额拆单**
- **拆分 / 历史重分类的商户规则另行确认**：历史分类迁移不等于「以后也这样」。M3 确认卡为每组规则单独表达是否迁移 / 新建；该要求不适用于合并。分类体系批量审计不得被 [06 记忆](./06-memory.md) 当作纠正事件
- **有条件整批撤销**：domain 先验证交易、活跃草稿与（M3 起）规则未被再次修改，且参与分类未发生后续生命周期操作。合并撤销清除本批写入的源墓碑并恢复原引用 / 启停状态；部分和完全拆分撤销恢复所有参与实体的批次前状态，仅由本批新建且恢复引用后仍无引用的目标可删除。任一冲突整批拒绝、不做半撤销；撤销追加反向审计，不删除原批次

结构化待确认操作的持久化与工具形状仍是 [00 地基 §5](./00-foundation.md) R12 / [01 Agent 运行时 §5](./01-agent-runtime.md) R8；本节只固定审核契约，不从 UI 反推一张尚未批准的表。

#### 明确商户规则指令的确认（M3）

用户明确说「以后 X 归 Y」时，UI 展示商户模式、目标分类和「仅影响未来」后，一次人工确认即可交给 [06 记忆 §3.3](./06-memory.md) 写规则；不要求先积累两次被动纠正。用户同时要求「过去也改」时，规则提案与历史批量改类必须作为两个可区分的待确认操作展示，不能用一张规则卡静默回写历史。

### 3.7 性能

- 单次审核可能有数百条 → **虚拟滚动**
- 证据图按需加载，不一次性把整批原图读进内存

### 3.8 前端状态管理（2026-08-24 定案）

**决定：M1 引入 TanStack Query v5 管理 Tauri IPC 返回的权威快照；React `useReducer` / 局部 `useState` 管理审核屏的瞬时交互。不引入 Zustand。** [`docs/architecture.md` §8](../architecture.md) A4 与本文 §5 R3 同步关闭。

依据：TanStack Query 的[定向失效](https://tanstack.com/query/latest/docs/framework/react/guides/query-invalidation)与[从 mutation 触发失效](https://tanstack.com/query/latest/docs/framework/react/guides/invalidations-from-mutations)正好表达「前端缓存可丢、Rust 真值重取」；React reducer 适合组织屏幕交互状态，但 Context 的 Provider value 改变会更新全部消费者（[React `useContext`](https://react.dev/reference/react/useContext)），不拿它承载数百行权威快照。Zustand 的 selector 适合细粒度 UI 订阅，却不提供 query key、陈旧响应隔离与失效语义，因此当前没有引入它的收益。

状态边界按「谁是真值」划，不按「哪个组件要用」划：

| 状态 | 归属 | 例子 |
|---|---|---|
| Rust / SQLite 的可重取投影 | **TanStack Query cache** | 来源列表、按 `source_id` 的草稿与原件、按 `attempt_id` 的总额校验、agent 状态与日志 |
| 用户尚未提交的界面意图 | **screen reducer / 局部 state** | 当前来源、焦点行、编辑模式与输入缓冲、键盘流、用户明确取消选择的草稿 ID |
| 业务判定 | **仍只在 Rust domain** | 能否批量确认、总额状态、来源状态机、三元组自洽 |

Query cache 是**可失效、可重取的只读投影缓存**，不是第二份业务状态。确认、丢弃、编辑等 mutation 成功后只按 query key 定向失效并从 Rust 重取；不在前端乐观编造 `consumed` / `reviewed` / 对账结果。M1 的最小 query key：

```text
['review-sources']
['review-drafts', sourceId]
['review-evidence', sourceId]
['review-total', sourceId, attemptId]
['agent-status']
['agent-logs']
```

本地 IPC 不是网络请求，默认项必须显式覆盖：`staleTime: Infinity`、`retry: false`、`refetchOnWindowFocus: false`、`refetchOnReconnect: false`；只有 mutation、Tauri event 或明确用户动作触发失效。Tauri `invoke` 目前不能用 `AbortSignal` 终止 Rust command，但 query key 能保证来源 A 的迟到结果留在 A 的缓存里，**不能覆盖已经切到的来源 B**。

**默认全选保存为「排除集合」，不能每次重取都重新全选。** 当前 M0 `refreshSelected()` 每次刷新都用全部草稿重建选择集；用户取消一条后只要改了另一条，刷新就会把被排除项重新选中。M1 reducer 按 `(source_id, attempt_id)` 保存用户明确排除的 ID；首次加载与之后新增草稿默认选中，重取不得抹掉人的排除意图。

**M1 运行事件的显示边界（2026-09-05，未实施）**：依据 [01 Agent 运行时 §3.4/§6.2](./01-agent-runtime.md) 的 pi 设计回流，运行事件只驱动进度展示或 query 定向失效，不直接改写来源状态、草稿事实、对账结果与确认策略。来源事件按 `(source_id, attempt_id, agent_session_id)` 隔离；旧 attempt 的迟到事件不得覆盖当前尝试，重复进度不重复计数。面板卸载/消费失败不触发取消或重试；重新订阅或发现事件缺口时从 Rust 重取，快照与事件的衔接协议由 [01 §5 R9](./01-agent-runtime.md) 在运行事件切片开工前定案。请求被接受、草稿生成、解析完成、人工确认入账须分别显示，不以 CLI「完成」文本或最后一帧进度推断入账；高频进度可合并发布，截断须明示，终态不能被旧进度回退。此处不新增前端业务 store；该运行事件切片不属于 §3.9 的有限并行范围。

否决另外三种组合：

- 只用 `useState` / `useReducer` + Context 管全部状态：仍要自己写异步去重、迟到响应隔离与 mutation 失效，且 Context 没有行级 selector；
- 用 Zustand 管 IPC 数据：它擅长细粒度 UI 订阅，但不提供 query key、陈旧响应治理与失效语义，会诱导把 Rust 真值复制成前端业务 store；
- TanStack Query + Zustand 同时引入：当前 screen reducer 足以承载选择、焦点与编辑，M1 没有证据支持两套第三方状态模型。只有 profiler 证明 reducer 传播造成可见行无关重渲染时，才另行评估一个**只存 UI 状态、无 persist、无业务 mutation** 的 Zustand store。

### 3.9 有限 M1 并行切片（2026-09-06 维护者决定）

依据 [`docs/PRD.md`「有限 M1 并行开发边界」](../PRD.md)，独立新样本正式复测不再阻止本节的三个既定边界开始实现，但仍阻止 M1 整体进入 `review`。本次只回流规格与准备计划，尚未开始实现；frontmatter `review` 仍指已经启动并验收过的 M0 切片。后续真正写代码时，本文按 [`docs/prd/CLAUDE.md`](./CLAUDE.md) 的跨里程碑状态规则转为有限 M1 `in-progress`。

| 可实施边界 | 必须保持的契约 | 不得顺带带入 |
|---|---|---|
| **design token 落地** | 组件只引用 [`design.md`](../../design.md) v0.5 semantic token；禁用态、草稿中性色、最小字号与三类输入继续遵守 [前端规则 §10–§11](../../.claude/rules/frontend.md) | 不实现参考稿 04–07 的账目、统计、事项或设置能力；不把 v9 的两个原色取值覆盖到 token 事实源 |
| **Query + reducer 状态边界** | 落实 §3.8 的 query key、IPC 默认项、定向失效、来源迟到隔离和按 attempt 保存的排除集合；业务真值仍在 Rust | 不做 [01 Agent 运行时 §3.4/§6.2](./01-agent-runtime.md) 的实时事件、有界输出或收尾；不引入 Zustand |
| **完整原件证据** | 当前来源的完整截图或整段口述默认可见，当前草稿的 `evidence_text` 与其并列；截图安全退路固定为完整原件 + 抽取声明，口述仍只高亮可验证 span | 不新增 bbox、坐标迁移、OCR 或按 ordinal 猜等高区域；不改变无证据不得确认与 `confirmation_policy` |

[`docs/design/desktop-v9.dc.html`](../design/desktop-v9.dc.html) 的 01–03b 只提供当前 M0 主路径的布局与层级参考，不是产品范围清单：02 里的事项草稿属于 M3，03 的动态进度属于未开放的运行事件切片，均不能因为画在参考稿里就提前实现。页面参考与本节冲突时，以本节、§3.1–§3.8 和 [`design.md`](../../design.md) 为准。

这个有限切片只能运行合成 fixture、mock Tauri IPC 与其他零额度门禁。真实 agent、无参数 live、正式 `--m0-go-no-go` 与 diagnosis 仍须维护者另行明确授权。它不改变 `parse_attempts.reported_total_*` 四列、M0 五工具、三条总额等式、确认策略或无 force 旁路。

## 4. 否决的替代方案

| 方案 | 否决原因 |
|---|---|
| 单表 + `is_draft` 状态字段 | 状态字段是软约束，下游查询漏一处过滤就把 AI 的猜测当成了事实（[ADR-0002](../adr/0002-ai-never-writes-directly.md)「理由」、[00 地基 §4](./00-foundation.md)） |
| 证据只在点击后弹出大图 | 审第 7 条时要自己在图上找到第 7 行——审核成本随条目数线性增长，40 笔 30 秒做不到（[ADR-0002](../adr/0002-ai-never-writes-directly.md)「理由」） |
| 总额校验只警告不阻止 | 那就等于没有闸门。用户在批量确认时不会停下来读警告 |
| 总额校验带容差（如 ±1 分） | 容差会掩盖真实的解析错误；整数存储的全部意义就是让精确相等成立（[00 地基 §3.4](./00-foundation.md)） |
| 默认不选，让用户逐条勾 | 多数条目是对的，逐条勾是把 O(正确项) 的成本强加给用户；默认全选让成本只落在 O(错误项) |
| 用聊天框完成最终审核（「把第 7 条改成 168」后直接入库） | 交互形态明确是「对话下指令 + 界面做审核」（[`docs/PRD.md` §5.3](../PRD.md)）；自然语言可以发起分类体系操作，但金额、目标与影响范围仍须结构化界面精确确认，不能在聊天里直接执行 |
| 让语音直接执行审核确认 | 系统听写可形成事项修改指令，但目标匹配与字段差异仍须视觉审核；语音不能确认或绕过草稿（[ADR-0005 §3](../adr/0005-voice-and-system-integration.md)） |
| 确认后删除草稿 | 审计无法回答「入库的这条当初 AI 起草成什么样」 |

## 5. 待决与风险

| # | 事项 | 影响 | 谁来决 / 何时 |
|---|---|---|---|
| ~~R1~~ **已关闭（2026-08-24）** | 证据区域定位（在原图上高亮对应行）技术上能否稳定做到 | 本文 §3.2 | **结论：当前不可稳定定位，不进生产 schema。** 产品密封链路 156 个 bbox 中出现 1 个危险误定位、9 个相邻行侵入，vertical IoU 中位数 0.704，三轮 y 中心跨度 p95 达 0.54 个行高；同一 CLI 的 Sonnet 对照正确行中心命中仅 69.53%。M1 不加坐标列，不按 ordinal 猜等高行，继续显示完整原件 + `evidence_text`。实测与复现见 [spike 记录](../spikes/2026-08-24-r1-evidence-region.md) |
| R2 | 舍入规则若与来源自身不一致，总额校验会系统性失败（[00 地基 §5](./00-foundation.md) R3） | 本文 §3.3 | M2 实测真实对账数据 |
| ~~R3~~ **已关闭（2026-08-24）** | 前端状态管理选型（[`docs/architecture.md` §8](../architecture.md) A4 同步关闭） | 本文全部 UI | **结论：TanStack Query v5 管 Rust/Tauri 权威快照，screen reducer / 局部 state 管瞬时 UI；不引入 Zustand。** 边界、默认项与 query key 见 §3.8 |
| ~~R4~~ | ~~「40 笔 30 秒」如何客观测量~~ **已关闭（2026-08-24）**：协议写进 §6「40 笔 30 秒怎么测」——应用自埋计时、夹具固定数据、脚本固定操作、`pointerdown` 计数判「不碰鼠标」、7 轮丢首轮取中位数，通过判据含 **IQR ≤ 中位数 20%** 这条给协议自身的自检 | 本文 §6 人工验收 | ~~M1 开工前~~ **已定** |
| R5 | 低置信标注依赖 agent 自评，而模型的自评校准度未知 | 本文 §3.4 排序第 4 档 | M1 实测；不可靠则降权或去掉该维度。字段 `draft_transactions.confidence` 已在 [00 地基 §3.6](./00-foundation.md) 留好且可空，**不阻塞 M0** |
| R7（**新增 2026-08-10**） | **`file` 来源的「适用性」信号**——§3.3 现在保守判：`file` + 没报合计 ⇒ `unavailable`，因为 `reported_total_* IS NULL` 分不清「结构性没有」「agent 漏读」「截图裁掉了」。真要支持 `file` 的 `not_applicable`，需要一个**独立于字段为空**的适用性信号（来源画像，或 agent 显式声明「这版式里不存在合计行」并附证据） | 本文 §3.3、[00 地基 §3.6](./00-foundation.md) | M2 拿到真实单笔小票样本后决。**M0/M1 保守判**——误判的代价是一整类漏读变得不可见 |
| ~~R8~~（新增 2026-08-13） | ~~**M1 设计稿与 token design system 的具体形态**~~ **已关闭（2026-08-24；2026-09-06 重申当前页面参考）**：事实源是 [`design.md`](../../design.md)（v0.5 定稿，三层 primitive → semantic → component，直接映射 CSS custom properties），维护者重新指定的当前页面参考归档在 [`docs/design/`](../design/README.md)；两者已接进 [`.claude/rules/frontend.md`](../../.claude/rules/frontend.md) §10–§11，**因此现在是 code review 判据**。M0 的局部 CSS 变量仍不得倒推为已批准设计系统，参考稿也不得覆盖 token 与规格 | 本文 §2、§3.9、[`.claude/rules/frontend.md`](../../.claude/rules/frontend.md) | **已定** |
| R6（**新增 2026-08-07**） | §3.3 的舍入敏感性——逐笔 `base_amount_minor` 各自舍入后求和，与账单印刷的合计可能差几分（[00 地基 §5](./00-foundation.md) R3 的具体失败模式） | 本文 §3.3 校验式 | M2 实测真实外币账单；若系统性偏差成立，可能需要在**外币行参与合计**这条路径上另立规则，**结果回流本文与 [00 地基](./00-foundation.md)** |

## 6. 验收标准

本模块横跨 M0（最朴素的确认列表）、M1（交易审核界面做深）、M2（分类体系操作）与 M3（事项 create/update 草稿和商户规则），验收**按里程碑分层**——M0 只需闸门与确认逻辑成立，键盘流与排序属 M1，分类影响预览与批量审计属 M2，事项目标消歧、update diff 与明确商户规则提案属 M3。

#### M0 必过

- [ ] `cargo fmt --all -- --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test` 全绿
- [ ] `npm run lint` · `npm run typecheck` · `npm test` · `npm run build` 全绿
- [ ] `rg -n 'confirm' src-tauri/src/mcp` 无命中——`mcp/` 不引用确认动作（**替换原验收** `review::confirm_not_reachable_from_mcp`：`cargo test` 做不了静态调用图分析，那条写法无法实现）
- [ ] `cargo test review::confirm_rejects_draft_without_evidence` 通过——服务端二次校验返回 `review.missing_evidence`
- [ ] `cargo test review::confirm_rejects_incomplete_triple` 通过——三元组不齐时返回 `review.incomplete_triple`（§3.1 完整性校验）
- [ ] `cargo test review::total_check_exact_equality` 通过——差 1 分即 `failed`
- [ ] `cargo test review::total_check_uses_reported_currency` 通过——原币 == 报告币种时取 `amount_minor`、不等时取 `base_amount_minor`，混合币种来源能正确求和（§3.3 校验式）
- [ ] `cargo test review::total_check_unavailable_when_not_reported` 通过——`kind = file` 且本次尝试 `reported_total_*` 为空时 `reconciliation_status == unavailable`、`confirmation_policy == single_only`
- [ ] `cargo test review::total_check_survives_partial_confirm` 通过（**§3.3 修正的回归**）——`failed` 的尝试逐条确认掉一条后**再次校验结果不变**；全部确认完后结果仍不变。**把求和范围改回「未消费草稿」时该用例必须变红**
- [ ] `cargo test review::total_check_is_scoped_to_attempt` 通过——同一来源成功解析**两次**、两次草稿都未作废时，各自的校验只算自己那次的草稿；**按 `source_id` 求和的实现必须变红**（§3.3「入参是 `attempt_id`」）
- [ ] `cargo test review::retry_uses_new_reported_total` 通过——第一次尝试超时（草稿作废）后重试，校验用的是**第二次**报告的合计，不是第一次残留的那个
- [ ] `cargo test review::total_check_excludes_voided_drafts` 通过——被作废的草稿（`voided_at` 非空）不进求和
- [ ] `cargo test review::total_check_uses_reported_kind` 通过——同一批草稿（含 `expense` 与 `income`）在 `expense_total` / `income_total` / `net_change` 三种 `kind` 下分别命中三条不同等式（§3.3）
- [ ] `cargo test review::total_check_unavailable_on_transfer` 通过——含 `direction = transfer` 的条目时 `reconciliation_status == unavailable`，**不是把它按支出算进去**
- [ ] `cargo test review::utterance_yields_user_attested_batch` 通过——`kind = utterance` 且未报合计时 `reconciliation_status == not_applicable` 且 `confirmation_policy == user_attested_batch`，**批量确认可用**
- [ ] `cargo test review::utterance_with_stated_total_reconciles` 通过——口述里报了一条覆盖整段全部适用交易、且三元组在候选中唯一的 scope-valid 合计时，`reconciliation_status` 为 `passed` / `failed`（不是 `not_applicable`），而 `confirmation_policy` 仍是 `user_attested_batch`（§3.3「两个维度」）
- [x] `cargo test agent::total_markers_are_candidates_not_completion_gate` 通过——月度 viewport 外、分页、按日、单笔或子组「总共 / 合计」不报告也能正常完成，不能被关键词闸门逼进 `reported_total_*`（[01 §3.2](./01-agent-runtime.md)）
- [x] `cargo test eval::formal_scope_invalid_total_reports_must_be_zero` 通过——正式判定集合中真值标为 scope-invalid 的 case 成功报告任意合计都会触发硬 no-go（control 仍只记录），即使该数字碰巧与草稿和相等；生产侧没有 scope 字段不能让 formal 契约静默消失
- [ ] `cargo test review::file_source_never_user_attested` 通过——`kind = file` 在任何输入下都拿不到 `user_attested_batch`，也拿不到 `not_applicable`
- [ ] `cargo test review::file_without_total_is_unavailable_not_na` 通过——**只有一条草稿**且没报合计的 `file` 来源，结果是 `unavailable` + `single_only`（**不是 `not_applicable`**）——§3.3「`file` 为什么永远不判 `not_applicable`」
- [ ] `cargo test review::batch_gate_reads_policy_not_status` 通过——批量确认的准入只看 `confirmation_policy`；构造一个 `reconciliation_status == passed` 但策略为 `single_only` 的输入，批量仍被拒（§3.3「两个维度」）
- [ ] `npm test -- review/utterance-batch-gate` 通过——`user_attested_batch` 的批量确认按钮，在「整段原文全文可见 + 拆分结果并排 + 条数显式」三者任一缺失时不可点（§3.3 的三条 UI 硬要求）
- [ ] `npm test -- review/utterance-attested-warning` 通过——`kind = utterance` 且本次尝试报了合计时（`reconciliationStatus` 为 `passed` 或 `failed`），批量确认按钮旁同屏出现「确认前请对着原文过一遍」；`failed` 一档还须同屏给出差额（§3.4 第 2 档）。**这是唯一一条「机器判定不符仍允许批量」的路径**
- [ ] `npm test -- review/completed-with-gaps-banner` 通过——`parse_attempts.outcome == "completed_with_gaps"` 时审核界面显眼呈现 `unparsed_note`，与普通 `completed` 视觉上可区分（[01 §3.2](./01-agent-runtime.md)）
- [ ] `cargo test review::total_check_unavailable_when_amount_unobtainable` 通过——存在缺三元组或 `base_currency` 不匹配的草稿时结果为 `unavailable`（**不是 `failed`**），且返回体列出是哪几条
- [ ] `cargo test review::inline_edit_amount_recomputes_base` 通过——**只给 `amountMinor`**（不给任何本位币字段）改一条带完整三元组的草稿，返回 `Ok`，且 `base_amount_minor` 按原 `rate_ppm` 重算、三元组仍自洽（§3.5「本位币金额是导出值」）
- [ ] `cargo test review::inline_edit_preserves_drafted_json` 通过——**同一次改金额**之后，`drafted_json` 逐字节不变（§3.5、[ADR-0002](../adr/0002-ai-never-writes-directly.md) 硬性要求 7）。**把它改回「只改 `merchant`」时本条必须失去意义**——那正是 v0.11 让「只改金额必失败」漏出门禁的写法
- [ ] `cargo test review::inline_edit_rejects_half_triple` 通过——只给 `ratePpm` 不给 `baseCurrency` 时返回 `review.incomplete_triple`，且草稿未被修改
- [ ] `cargo test review::inline_edit_completes_missing_triple` 通过——给缺三元组的草稿补 `baseCurrency` + `ratePpm` 后，`base_amount_minor` 被导出且该草稿随即可确认（§3.5「三元组补全」）
- [ ] `cargo test review::batch_confirm_blocked_when_total_failed` 通过——返回 `review.total_mismatch`
- [ ] `cargo test review::batch_confirm_blocked_when_total_unavailable` 通过——返回 `review.total_unavailable`
- [ ] `cargo test review::no_force_bypass_exists` 通过——确认相关命令的参数里不存在 force/ignore 类旁路
- [ ] `cargo test review::single_confirm_allowed_when_total_failed` 通过——逐条确认仍可用
- [ ] `cargo test review::every_edit_writes_audit` 通过——每次修改后 `audit_log` 多一条且含 before/after
- [ ] `cargo test review::confirmed_draft_is_marked_not_deleted` 通过——`consumed_at` 置非空，行仍在
- [ ] `cargo test review::discarded_draft_is_marked_not_voided` 通过——人工丢弃只置 `discarded_at`，`voided_at` 仍为空，草稿与 `drafted_json` 均保留
- [ ] `cargo test review::total_check_survives_discard` 通过——丢弃一条草稿前后，本次尝试的总额校验结果不变
- [ ] `node scripts/verify-m0.mjs`（检查项定义见 [`docs/PRD.md` §9.3](../PRD.md)）退出码 0
- [ ] `cargo test m0::cargo_manifest_declares_default_run` 通过——`src-tauri` 两个 bin 并存时声明了 `default-run`
- [ ] `cargo test m0::app_icon_decodes_to_eight_bit_rgba` 通过——`icons/icon.png` 解码后正好 4 字节/像素

  > **这两条是「桌面应用真的起得来」的下限，放在本节是因为下面的人工验收是全仓库唯一需要启动应用的验收**（2026-08-13 人工验收当天新增）。**M0 的全部门禁都只测库、确定性链路与外部 MCP 链路，没有一条会启动桌面壳**——当天 `npm run tauri dev` 连续两次起不来（缺 `default-run`；图标是 16 位/通道，tauri 判定无效后在 `did_finish_launching` 里 abort），而 `node scripts/verify-m0.mjs` 不带 `--skip-live` 全绿。**两条都不是本模块的规格问题，但两条都会让本节的人工验收无从做起。**

**M0 人工验收**（**2026-08-13 首次实测执行完毕**，结论见 [§7 回流记录](#7-回流记录)；执行方式：真实 Claude Code CLI 解析两张截图与一段口述，逐条对照下列判据）：

- [ ] **来源原件本身在审核屏上可见**——截图能看到图、`utterance` 能看到整段转写文本，不是只有 `evidence_text` 那一列（§3.2，2026-08-10 新增）
- [ ] 每条草稿的 `evidence_text` 与解析结果在同一屏可见，无需额外点击
- [ ] 声明合计与它的原文片段显示在批量确认按钮附近（§3.3「基准值本身必须可核对」）
- [ ] `kind = file` 总额不符或无法校验时，提示可见、`confirmation_policy = single_only` 且批量确认按钮不可点；`utterance` 的 `user_attested_batch` 另按 §3.4 显示背书提示与差额
- [ ] 一段口述拆出的多条草稿，**能一次批量确认**，且确认前整段原文与全部条目同屏（§3.3 `not_applicable`）
- [ ] **缺本位币三元组的草稿，能在审核界面当场补齐并确认**——本位币与汇率两个输入框可见可填，不需要丢弃后重新解析整个来源（§3.5「三元组补全」，2026-08-13 新增）

#### M1 必过（在 M0 全部通过之上）

##### 有限并行切片必过（零额度；允许先实现）

- [ ] `npm run lint` · `npm run typecheck` · `npm test` · `npm run build` 全绿
- [ ] `npm test -- review/design-tokens` 通过——当前 M0 主路径组件只消费 semantic token；禁用态不另造底色、草稿不获得事实色、信息文字不低于 11px，且三类输入职责不串用（§2、§3.9）
- [ ] `npm test -- review/query-race` 通过——来源 A 的 IPC 晚于来源 B 返回时，A 的结果只进入 A 的 query cache，不覆盖当前来源 B
- [ ] `npm test -- review/selection-intent` 通过——用户取消选择一条后，编辑另一条触发失效重取；被取消项仍不选中，新出现草稿默认选中（§3.8「排除集合」）
- [ ] `npm test -- review/evidence-original` 通过——`file` 显示完整来源图且 `utterance` 显示整段转写；当前草稿的 `evidence_text` 同屏，但截图不渲染 bbox / 推断区域，口述只按有效 code-point span 高亮
- [ ] **人工视觉验收（不调用 agent）**：用合成 fixture / mock IPC 在 1440 × 900 依次打开首次输入、解析状态、普通审核与对账异常四态；布局层级以 [`desktop-v9.dc.html`](../design/desktop-v9.dc.html) 01–03b 为参考，色值、字号、禁用态、草稿状态与输入职责逐项以 [`design.md`](../../design.md) v0.5 为准。完整原件、当前 `evidence_text`、合计证据 / 差额与确认按钮满足 §3.2–§3.3 的同屏要求；页面中不得出现 04–07 的未来能力
- [ ] `node scripts/verify-m0.mjs --skip-live` 退出码 0；该结果只证明零额度回归，不构成 M0 或 M1 整体通过

##### M1 整体追加必过（有限切片通过仍不等于整体验收）

- [ ] 独立新样本 formal final 按 [`docs/PRD.md` §9.4](../PRD.md) 冻结规则得到退出码 0，且 `verdict = go | conditional_go`；`conditional_go` 的未达标项与对策已登记。该真实复测须另获维护者明确授权
- [ ] **运行事件与快照验收（零额度，测试命令随该切片实现补回）**：按 [01 §6.2](./01-agent-runtime.md) 注入旧 attempt 迟到/重复事件、重新订阅与高频输出；当前来源和尝试不串写，草稿数不重复累计，终态不回退，截断有标识。关闭面板不取消解析，重新订阅后的显示与 Rust 快照一致，用户排除集合不丢失；仅收到请求接收或 CLI 完成事件时不得显示已入账
- [ ] `npm test -- review/sorting` 通过——异常前置的六级排序按 §3.4 优先级
- [ ] `npm test -- review/sorting-utterance` 通过——`utterance` 来源的条目排在总额 `failed` 之后、跨图重复之前（§3.4 第 2 档）
- [ ] `npm test -- review/keyboard` 通过——§3.5 全部快捷键有对应处理，且默认全选
- [ ] **40 笔真实草稿，从打开审核界面到全部入库，不碰鼠标，≤ 30 秒**——**测量协议见下方「40 笔 30 秒怎么测」**（2026-08-24 定，R4 关闭）

##### 40 笔 30 秒怎么测

**R4 提的问题是真的**：秒表人测的方差可能大于优化幅度，那样这条判据就只是个感觉。协议的每一条都在砍一个方差来源。

**① 不用秒表，用应用自己的计时。** 起点是审核界面 mount 完成、40 条草稿全部就位、首个焦点已落定的那一刻；终点是**最后一条草稿的确认 IPC resolve**。两端都用 `performance.now()`，差值写进 `debug` 级日志（[ADR-0007](../adr/0007-local-observability-and-log-tiers.md)；`debug` 在开发构建默认开）。**人的反应时间不进区间，界面的响应时间全进区间。**

**② 数据固定，用夹具重放。** 同一份 40 条草稿的夹具（[07 评测 §3.6](./07-eval.md)），每轮重放同一批数据。**否则测的是「这批账单好不好读」，不是界面。** 夹具里必须含至少：3 条需要改金额、2 条需要补三元组、1 条对账 `failed`、1 条 `completed_with_gaps`——**异常项的处理成本是这一屏的真实成本**，全是干净数据的 40 条测不出东西。

**③ 操作脚本固定。** 预先写定按键序列（哪几条改、改成什么、哪几条丢弃），执行者照读执行。**这样测的是界面的操作成本，不是执行者的判断速度**——判断速度因人因日而异，正是 R4 担心的那个方差。

**④ 「不碰鼠标」由代码判定，不靠自觉。** 计时窗口内统计 `pointerdown` / `mousedown` 事件数，**> 0 则该轮作废**。这是唯一一条能自动查的，不查就等于没有。

**⑤ 跑 7 轮，丢第 1 轮，取剩 6 轮的中位数。** 第 1 轮含学习效应；取**中位数不取平均**（抗离群）。

**通过判据两条，缺一不算过：**

- **中位数 ≤ 30.0 s**
- **四分位距（IQR）≤ 中位数的 20%**。第二条是给协议本身的自检——**IQR 过大说明测量还没稳到能支撑「优化有没有效」这个判断**，此时先修协议，不许拿一个方差比效应还大的数字宣布通过

**记录**：每次跑测把 7 轮原始值、中位数、IQR、夹具哈希与被测 commit 写进本文 §7 回流记录。**只报中位数不报原始值的结论不予采纳**——那正好把 R4 担心的东西藏起来了。

#### M2 分类切片必过

- [ ] `cargo test review::direction_change_clears_incompatible_category` 通过——expense / income 互换或转为 transfer 时，不兼容分类在同一次人工 before/after 中清空；可确认成未分类，不改成「其他」
- [ ] `cargo test review::disabled_category_draft_requires_reselection` 通过——旧事实只改其他字段可保留停用分类，活跃草稿则必须改为有效分类或未分类后才可确认
- [ ] `npm test -- review/category-operation-impact` 通过——已使用分类重命名、合并、拆分均展示分类、事实交易与活跃交易草稿的数量和明细；取消后 `categories`、`transactions`、`draft_transactions` 当前值均不变
- [ ] `cargo test review::category_batch_undo_is_all_or_nothing` 通过——无后续修改时，合并清除本批源墓碑并恢复引用，部分 / 完全拆分恢复参与分类及对象的批次前状态；仅本批新建且无引用的目标被删除。任一对象冲突时全批拒绝且不产生部分回滚
- [ ] `npm test -- review/category-rename-intent` 通过——已使用分类改名时必须选「只是修改名称 / 分类含义变化」，后者转新建或拆分

#### M3 必过

- [ ] `npm test -- review/entity-type-switch` 通过——一条交易草稿转成事项草稿后，`source_id` / `evidence_text` / `attempt_id` 不变，且写了两条审计（§3.6）。**2026-08-10 由「M1 必过」移到此处**：目标表 `draft_items` 在 M3 才建（[00 地基 §3.6](./00-foundation.md)），放在 M1 那条验收必然挂
- [ ] `cargo test review::item_update_requires_confirmed_target` 通过——needs_target update 持久化并计入完成条数但不可确认，且 `items` 不变
- [ ] `cargo test review::item_target_resolution_preserves_drafted_json` 通过——人选候选或改为新建后当前草稿可确认，而 agent 原始 operation、候选与 patch 逐字节不变
- [ ] `cargo test review::item_update_preserves_drafted_patch` 通过——确认后事实表按 patch 更新，而 `drafted_json` 不变
- [ ] `npm test -- review/item-update-diff` 通过——before / patch / after 与对应原文同屏，set、clear、unspecified 三态可区分，clear 不回显计划值
- [ ] `npm test -- review/item-mixed-create-update-batch` 通过——同一口述可混合 create/update，取消其中一条不影响其余所选变更
- [ ] `npm test -- review/item-target-resolution` 通过——目标缺失/歧义时只有选候选、改为新建、忽略三条路，不存在静默 create
- [ ] `npm test -- review/explicit-merchant-rule-proposal` 通过——明确「以后 X 归 Y」一次确认即可生效，默认只影响未来；同时要求改历史时显示两项独立操作
- [ ] `npm test -- review/category-operation-rule-impact` 通过——M3 启用后，合并在同一确认卡展示并重定向全部规则、不逐条另问；拆分 / 历史重分类为每组规则另问「以后也这样吗」
- [ ] `cargo test review::category_batch_audit_not_memory_correction` 通过——分类体系批量操作逐项审计并共享 `batch_id`，但 M3 的记忆模块不把它们写入 `memory_rule_corrections`

## 7. 回流记录

| 日期 | 回流内容 | 依据 |
|---|---|---|
| 2026-09-06（有限 M1 并行决定） | 维护者将独立新样本正式复测从 M1 开工门槛改为整体验收门槛，只开放 design token、TanStack Query + reducer 状态边界、完整原件 + `evidence_text` 安全退路；当前页面参考重新指定为归档 v9。实时事件、排序、键盘流、虚拟滚动与 40 笔跑测仍关闭；本次只改文档，`review` 继续指 M0 已验收切片 | [`docs/PRD.md`「有限 M1 并行开发边界」](../PRD.md)；本文 §3.9/§6；[`docs/design/README.md`](../design/README.md) |
| 2026-09-05（pi 设计回流） | M1 运行事件只驱动显示或 query 失效；补旧 attempt 迟到隔离、重复进度、重新订阅、截断和请求/入账区分的 UI 边界与零额度验收。状态方案不变，`review` 仍指 M0，M1 未实施 | [01 Agent 运行时 §3.4/§6.2](./01-agent-runtime.md) 的固定版本 pi 源码参考与运行契约；本文 §3.8 |
| 2026-09-02（no-go 修正验收） | **本文由 `in-progress → review`。** 生产 `total_check` 的三条等式、对账四态、确认策略三态与无 force 旁路均未修改；关键词非强制完成和 formal `scopeInvalidTotalReports == 0` 两条新验收通过，范围资格仍由提示词 + formal 真值负责。完整零额度门禁通过，M1 界面切片未开始 | 本文 §6；[01 Agent 运行时 §3.2](./01-agent-runtime.md)；[07 评测 §3.4](./07-eval.md) |
| 2026-09-02（no-go 修正开工） | PR #27 的 claim 范围前置与既有对账 / 确认策略保持不变规格已独立 review 通过，本文先由 `draft → ready`；维护者随后批准分阶段实施计划，正式开始测试先行实现，故由 `ready → in-progress`。本轮不改三条等式、确认策略、无旁路约束或 M1 界面切片 | [PR #27](https://github.com/EpiphanyAlex/DayBook/pull/27)；[`docs/PRD.md` §9.4](../PRD.md) 防滥用流程第 4 步 |
| 2026-08-30（第一次 M0 正式 no-go） | **本文由 `review → draft`。** `6/7` 对账假警报证明「任何来源上的合计都可送进现有等式」这个隐含前提错误：月度 viewport 外、分页、按日、单笔 / 子组合计的等式语义与 current source 不同。新增「先判 claim 范围」：M0 只支持当前不可变来源全部适用交易的一条 claim；任意 viewport 仍接受，同三元组 decoy 因现有四列无法审计身份而保守不报；生产 schema 与确认策略不改，范围资格由提示词 + formal candidate/expected claim 真值检验。指标 4 分母 / 阈值、现有 total_check 等式、四硬字段与无 force 旁路均不改 | [`docs/PRD.md` §9.4](../PRD.md) 第一次正式结果；[00 地基 §3.6](./00-foundation.md) |
| 2026-08-24（M1 前置） | **R1 与 R3 关闭。** R1 的产品密封链路探针在受控合成图上仍出现危险误定位与相邻行侵入，且同一 CLI 的 Sonnet 对照大幅退化；错误高亮属于证据闸门风险，因此 M1 不加坐标列，安全退回「完整原件 + `evidence_text`」。R3 定为 **TanStack Query v5 + screen reducer / local state**：Query 只缓存 Rust 可重取投影，用户选择/焦点/编辑留在 reducer；明确修复当前迟到响应覆盖与刷新后重新全选两类风险，不引入 Zustand | [R1 spike](../spikes/2026-08-24-r1-evidence-region.md)；[`docs/architecture.md` §8](../architecture.md) A4；当前 `src/App.tsx::refreshSelected` 实现审查 |
| 2026-08-17（跨文档同步） | **[05 事项](./05-items.md) v0.8 将自然语言回溯从「只新建事项」扩为「可修改已有事项」**，因此本文 §3.6 增加 M3 create/update 共用审核闸门、目标消歧、mixed batch 与字段差异契约。该扩展只影响尚未实现的 M3；M0/M1 已验收的交易闸门、状态与 `status: review` 均未被证伪 | [`docs/PRD.md` §5.2](../PRD.md) v0.21；[05 事项 §3.4](./05-items.md) |
| 2026-08-13（人工验收） | **§6 的 M0 人工验收六条全部实测通过**，`status` 由 `in-progress` 回到 `review`。做法：本机真实 Claude Code CLI，两张截图 + 一段口述。① 原件同屏——截图渲染成图、口述显示整段转写；② `evidence_text` 与解析结果同屏无需点击；③ 声明合计与原文片段（「TOTAL SPENT 147.65」）与批量确认按钮**始终同屏**——`.reconciliation` 在 `.draft-stack` 这个滚动容器之外，条目再多也不会把它滚走；④ 一张无声明合计的截图落在 `unavailable` + `single_only`，提示可见、批量按钮禁用；⑤ 一段口述拆出 4 条、一次批量确认入库，整段原文与全部条目及条数同屏；⑥ **缺三元组的草稿当场补齐并确认**——补 `AUD` + `0.21` 后 `base_amount_minor` 由 3800 CNY 导出为 798 AUD（§3.5「本位币金额是导出值」），随即单条确认入库，`drafted_json` 里 `baseAmountMinor` 仍为 `null` | 人工验收实测（2026-08-13）：`transactions` 9 行、`audit_log` 28 行，含 `human/update` 与 `human/confirm` 各自的 before/after |
| 2026-08-13（人工验收） | **桌面应用当天根本起不来，而 `node scripts/verify-m0.mjs`（不带 `--skip-live`）全绿。** 两个独立缺陷：① `src-tauri` 自 MCP helper 拆出第二个 bin 后一直缺 `default-run`，`npm run tauri dev` / `tauri build` 停在「could not determine which binary to run」；② `icons/icon.png` 是 **16 位/通道** RGBA，`tauri-build` 按 8 字节/像素编进二进制，运行时 tauri 按 4 字节/像素反推像素数，判定图标无效后在 `did_finish_launching` 里 abort。**根因是同一条：M0 的十一条门禁只测库、确定性链路与外部 MCP 链路，没有任何一条会启动桌面壳。** 两条都已修复并各加一条 `cargo test` 断言（见 §6）。**本节记它是因为 §6 的人工验收是全仓库唯一需要启动应用的验收，被它挡住的是这一节** | 人工验收实测（2026-08-13）：`npm run tauri dev` 两次 abort 的真实输出 |
| 2026-08-13（人工验收） | **`parse_attempts` 的中断补偿在真机上按规格生效**（顺带实测，非本次目标）：解析进行中应用被重启 → `outcome = interrupted`、来源 `failed` + `agent.interrupted`、该次尝试的三条草稿 `voided_at` 置非空且**行与 `drafted_json` 均保留**；界面显示「解析失败」与重试入口，重试后走新的 `attempt_id` | 人工验收实测（2026-08-13）；[01 §3.4](./01-agent-runtime.md) |
| 2026-08-13（实现验收） | **§3.5 的行内编辑把本位币金额当成了独立可编辑字段，导致「只改金额」这一条主路径必然返回 `data.money_inconsistent`**——而它正是 §1 举的头号场景（把 AI 读错的 1680 改回 168）。改定：`base_amount_minor` 由 `amount_minor` + `currency` + `base_currency` + `rate_ppm` **导出**，不接受直接编辑，三元组自洽由构造保证而非事后校验。同步 [`money-and-data.md §3`](../../.claude/rules/money-and-data.md)。**§6 原验收写的是「行内改一条草稿的金额后」，而实现的测试改的是 `merchant`**——按验收原文写它会红；验收由 1 条拆成 4 条，其中一条专门断言「只给金额」返回 `Ok`。`status` 由 `review` 退回 `in-progress`：进入 `review` 的前提（验收标准全部跑过）当时并不成立 | M0 实施验收（2026-08-13）实测：`edit_draft` 只传 `amountMinor` 返回 `data.money_inconsistent` |
| 2026-08-13（实现验收） | **§3.4 第 2 档「报了合计时仍提示『确认前请对着原文过一遍』」在实现里落空**：`ReconciliationCard` 只给 `not_applicable` 那一档配了背书文案，`passed` / `failed` 两档没有，而 `failed` 档同时显示差额报警与一个可点的批量确认按钮。补成硬要求并加一条前端验收——**这是全产品唯一一条「机器判定不符仍允许批量」的路径，放行而不告知等于两道闸门都没有**。同步根 [`CLAUDE.md`](../../CLAUDE.md) 约束 5（见下一行） | M0 实施验收（2026-08-13）代码审查 `src/App.tsx` `ReconciliationCard` |
| 2026-08-13（实现验收） | **根 [`CLAUDE.md`](../../CLAUDE.md) 约束 5 与本文 §3.3 正面冲突**：约束 5 无条件要求「不符时阻止批量入库」，而 §3.3 自 2026-08-10 起规定 `kind = utterance` 的确认策略与对账结果无关。实现跟的是本文，四条门禁全绿而顶层文件一直在说相反的话。改定：**约束 5 补 `kind` 限定**（`file` 一律阻止；`utterance` 走 `user_attested_batch` 的人工闸门，代价是三条 UI 硬要求 + `failed` 时差额必须与按钮同屏）。**本文 §3.3 不变**——[`docs/PRD.md` §1.1](../PRD.md) 把「口述一次批量确认」当作对竞品的唯一差异，反向改会把 2026-08-10 那次产品决定解开的死结重新焊上 | 维护者裁定（2026-08-13）；[`docs/PRD.md` §1.1](../PRD.md)、本文 §3.3「`user_attested_batch` 换的是另一道闸门」 |
| 2026-08-13（实现验收） | **§3.1 的 `review.incomplete_triple` 在界面上是死路**：M0 会提示「确认前需要补全本位币与汇率」，而界面没有任何地方能补，用户只能丢弃草稿重新解析整个来源。§3.5 新增「三元组补全」——本位币与汇率两个输入框 M0 即需要，并加 1 条人工验收 | M0 实施验收（2026-08-13）：`src/App.tsx` 有该提示、无对应输入 |
| 2026-08-13 | **M0 前端明确为功能基线，不是设计定稿。** M0 锁信息架构、闸门与可用性；设计稿和语义 token design system 推到 M1 开工前确定，当前 CSS 变量不获得事实源地位 | 里程碑目标：M0 验链路，M1 做深审核界面；产品确认（2026-08-13） |
| 2026-08-13 | M0 首屏补当前本位币选择；切换不回写已有草稿或事实行 | [00 地基 §3.4](./00-foundation.md) v0.13 实施回流 |
| 2026-08-13 | 人工丢弃获得独立的 `discarded_at` 语义；明确丢弃不改变解析完整性的总额校验。把完整异常排序中的 `sorting-utterance` 从 M0 移回本节已经定义的 M1 范围，并把 `declared_currency` 测试名同步为 `reported_currency` | M0 实施计划审查；[00 地基 §3.6](./00-foundation.md) |
| 2026-08-10（五轮） | **§3.3 拿「单笔截图没有合计」当论据时，隐含了「它理论上确实对账不适用」——那把问题的位置放错了。** 真正的理由是：`reported_total_* IS NULL` **这一个信号对应三种现实**（结构性没有 / agent 漏读 / 截图裁掉了），**从「没读到」反推「本来就没有」是把后两种伪装成第一种**，而第二种正是闸门 3 存在的全部理由。M0 保守判 `unavailable` + `single_only`；适用性信号登记为 R7。同步 [ADR-0002 闸门 3](../adr/0002-ai-never-writes-directly.md) | 文档审查（五轮） |
| 2026-08-10（五轮） | §3.4 异常前置第 2 档的 UI 文案随「口述也可能报合计」分成两种；§6 新增 2 条验收 | [00 地基 §3.6](./00-foundation.md) v0.9 |
| 2026-08-10（三轮） | **§3.3 拿「口述里说了总共 100」当拆两个维度的论据，而 [00 地基 §3.6](./00-foundation.md) 同时规定 `utterance` 的 `reported_total_*` 恒为空**——同一轮改动里留下的自相矛盾。改定：**口述明说合计时照常对账**（`passed` / `failed`），**确认策略不变**（恒 `user_attested_batch`）。这正是两个维度各自独立的实例 | 文档审查（三轮） |
| 2026-08-10（二轮） | **同一轮里改成的「该来源全部未作废草稿」仍不够，还得按尝试收窄。** 它修好了「确认改变校验结果」，没修好「**重试把两次尝试的输出混在一起**」：一个来源被成功解析两次时求和会把 24 条当成 12 条的输出；而合计原本存在 `sources` 上，第一次超时后草稿按 `attempt_id` 作废、合计却留了下来。**校验入参改为 `attempt_id`**，`sources.latest_attempt_id` 决定当前受审的是哪次输出 | 文档审查（二轮）；[00 地基 §3.6](./00-foundation.md) v0.7 把合计移入 `parse_attempts` |
| 2026-08-10（二轮） | **一个 `not_applicable` 同时表达了两个维度**——「能不能对账」与「能不能批量确认」被焊在一个值上。三个场景会撑破它：口述里明确说了「总共 100」（对账可做，但策略仍是用户背书）· 一张只有一笔、没有合计的单笔截图（对账不适用，但**不该因此获得批量放行**）· M3 一段口述同时产生交易与事项。拆成 **`reconciliation_status`**（4 值）+ **`confirmation_policy`**（3 值），**真正放行批量的是后者**。M0 不落成数据库字段，两者由 `total_check` 一起返回 | 文档审查（二轮） |
| 2026-08-10 | **§3.3 的求和范围「未消费草稿」有逻辑缺陷，M0 第一次逐条确认就复现。** `failed` 时允许逐条确认，而确认一条即退出求和范围 ⇒ 剩余和恒小于声明合计 ⇒ 该来源**再也回不到 `passed`**；全部确认完后和为 0 仍报 `failed`。病根是把「解析完整性」与「这一批要确认什么」捆在一起。改为：求和范围是该来源**全部未作废**草稿，校验是**来源的属性**，确认动作不改变它 | 文档审查；[00 地基 §3.6](./00-foundation.md) `voided_at` |
| 2026-08-10 | **§3.3 的校验式没有方向，把不同概念的合计当成同一个数。** 补 `declared_total_kind` 的三条等式（`expense_total` / `income_total` / `net_change`），并规定 `direction = transfer` 的条目让结果变 `unavailable`——转账的方向信息 schema 里就没有，硬算等于编。**用错误的等式产生的报警比不报警更糟：它训练用户忽略报警。** 同步 [ADR-0002 闸门 3](../adr/0002-ai-never-writes-directly.md)、[00 地基 §3.6](./00-foundation.md)、[01 §3.2](./01-agent-runtime.md) | 文档审查 |
| 2026-08-10 | **新增第四态 `not_applicable`，解开语音批量确认与闸门 3 的死结。** [`docs/PRD.md` §1.1](../PRD.md) 把「一次批量确认」作为对竞品的唯一差异，而本文 v0.2–v0.4 让 `utterance` 恒为 `unavailable`、批量确认必被拒——产品的核心差异被自己的闸门挡死。区分「结构性没有合计」（口述）与「本该有却取不到」（截图），前者允许批量确认，**换成三条 UI 硬要求构成的人工闸门**，并如实写明这道闸门更弱、以及它为什么只对口述成立。`kind = file` 永不判为 `not_applicable`。同步 [ADR-0002 闸门 3](../adr/0002-ai-never-writes-directly.md)、[`docs/PRD.md` §9.3](../PRD.md)、[00 地基 §3.6](./00-foundation.md)、[02 导入 §3.1](./02-ingest.md) | 产品决定（2026-08-10）：给 utterance 独立信任策略 |
| 2026-08-10 | **§3.2 把 `evidence_text` 当「原文」，闸门 2 在 M0 因此形同虚设。** `evidence_text` 与被核对的金额**出自同一次模型输出**——模型把 168 读成 1680 时也会把它写成「1680」，两者自洽却一起错。区分「证据 = 不可变原件」与「`evidence_text` = 抽取声明」，并把「M0 不做证据面板」收窄为「M0 不做区域高亮」——**原件本身 M0 就要可见**，否则 M0 验的是「模型抄得像不像」。同步 [ADR-0002 闸门 2](../adr/0002-ai-never-writes-directly.md)、[`docs/PRD.md` §9.2](../PRD.md) | 文档审查 |
| 2026-08-10 | **§3.5 的行内编辑会就地改写草稿，抹掉 AI 的原始起草值。** 补「不得覆盖 `drafted_json`」。§3.6 的「改实体类型」验收**由 M1 移到 M3**——目标表 `draft_items` 在 M3 才建，放在 M1 那条必然挂 | 文档审查；[00 地基 §3.6](./00-foundation.md) `drafted_json` |

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.22 | 2026-09-06 | **开放有限 M1 并行切片，`status` 仍为 M0 `review`。** 只允许 design token、Query + reducer 状态边界与完整原件证据退路，新增零额度切片验收并把 formal 独立新样本复测写成 M1 整体追加门槛；页面参考为归档 v9，实时事件与其余 M1 能力仍未开放，本次未实现 |
| v0.21 | 2026-09-05 | 同步 M1 运行事件与 Rust 权威快照边界、任务身份隔离及 UI 故障验收；保持 Query cache + screen reducer，当前 M0 `review` 与确认策略不变 |
| v0.20 | 2026-09-02 | **第一次 no-go 修正验收，`status: in-progress → review`。** 关键词非强制与 formal scope-invalid=0 回归通过；既有 total-check 等式、确认策略、无 force 旁路与 M1 UI 均不变 |
| v0.19 | 2026-09-02 | **第一次 no-go 修正开工，`status: draft → ready → in-progress`。** PR #27 的规格已独立 review 通过，维护者批准分阶段实施；新增 formal scope 回归但保持现有 total-check 等式、确认策略与无 force 旁路。M1 不开始 |
| v0.18 | 2026-08-30 | **第一次 M0 正式 no-go 回流，`status: review → draft`。** 总额校验新增 claim 范围前置：只认 current immutable source 全部适用交易；月度 viewport 外、分页、按日 / 分类 / 单笔语义 / 子组合计不得进入等式；同三元组 decoy 也因身份不可审计而拒报。生产 schema、对账四态、确认策略三态、无 force、现有三条等式均不改；新增 formal scope-invalid=0 的验收，替换口述关键词强制验收 |
| v0.17 | 2026-08-24 | **关闭 M1 前置 R1 / R3，`status` 仍为 `review`。** R1 实测结论为「agent 截图坐标不够稳定」：不加 bbox schema/迁移，M1 保留完整原件 + `evidence_text` 的安全退路；R3 选定 TanStack Query v5 管 IPC 权威快照、screen reducer / 局部 state 管瞬时交互，不引入 Zustand。§3.8 固定 query key、桌面 IPC 默认项、mutation 失效边界与「排除集合」选择语义；§6 新增迟到响应与选择意图两条 M1 验收；规格不变式新增「不得把 M0 推迟区域高亮写成 M1 必做」防回退规则 |
| v0.16 | 2026-08-24 | **关闭 R4 与 R8，`status` 仍为 `review`。** ① §6 新增「**40 笔 30 秒怎么测**」——R4 提的方差问题是真的，所以协议的每一条都在砍一个方差来源：应用自埋 `performance.now()` 计时（人的反应时间不进区间、界面响应时间全进）、夹具固定数据且**必须含异常项**（异常项的处理成本才是这一屏的真实成本）、按键脚本固定操作以剔除执行者判断速度、`pointerdown` 计数**由代码判**「不碰鼠标」、7 轮丢首轮取中位数。通过判据两条，第二条 **IQR ≤ 中位数 20%** 是给协议自身的自检——方差比效应还大时不许宣布通过。② R8 关闭：设计事实源定为 [`design.md`](../../design.md) v0.5，已接进 [`.claude/rules/frontend.md`](../../.claude/rules/frontend.md) §10–§11。③ §3.4 的记忆冲突例子随 [04 §3.3](./04-transactions.md) 两字分类改名（食品杂货 / 日用家居 → 买菜 / 日用） |
| v0.15 | 2026-08-23 | **新增 M2 分类体系操作审核契约，`status` 仍为 `review`。** M0/M1 自由文本编辑保持不变；M2 方向变化明确清空不兼容分类、停用目标阻止新确认。对话只生成待确认操作，影响数量由代码重算；M2 覆盖分类 / 事实 / 活跃交易草稿，M3 再纳入规则。合并规则随同一确认卡重定向，拆分 / 历史重分类才逐组另问；有条件整批撤销恢复完整批次前状态，任一冲突零写入。M3 明确商户规则指令一次确认即可生效，默认不回写历史 |
| v0.14 | 2026-08-17 | **新增 M3 事项 update 审核契约，`status` 仍为 `review`。** `draft_items` 以 operation 与 resolution_state 区分 create、ready update、needs_target update；非确认态持久化有界候选并计入完成条数，目标由人选择后才可确认。tri-state patch 明确 set/clear/unspecified，确认界面显示 before/patch/after；同一口述可 mixed batch。直接拖拽属于人的确定性事实修改，写审计并可撤销。M0/M1 交易审核契约与已通过验收不变 |
| v0.13 | 2026-08-13 | **M0 人工验收六条全部实测通过，`status` 由 `in-progress` 回到 `review`。** 真实 CLI 跑两张截图 + 一段口述，逐条走完原件同屏、合计与按钮同屏、`unavailable` 禁用批量、口述一次批量确认、**缺三元组当场补齐并确认**（v0.12 新增的那条）。同批修掉两个让桌面应用**根本起不来**的缺陷——缺 `default-run`、图标 16 位/通道；**根因是十一条 M0 门禁没有一条会启动桌面壳**，两条各补一条 `cargo test` 断言进 §6。**§3 决定与依据一字未改**——本次没有证伪任何规格 |
| v0.12 | 2026-08-13 | **M0 实现验收回流四处，`status` 由 `review` 退回 `in-progress`。** ① §3.5 **本位币金额改为导出值**，修好「只改金额必失败」；②§3.4 第 2 档口述报了合计时的背书提示补成硬要求（`failed` 时差额须与按钮同屏）；③ 根 [`CLAUDE.md`](../../CLAUDE.md) 约束 5 补 `kind` 限定，解开它与 §3.3 的正面冲突（本文 §3.3 不变）；④ §3.5 新增「三元组补全」——`review.incomplete_triple` 此前在界面上是死路。§6 行内编辑验收由 1 条拆成 4 条、新增 1 条前端验收与 1 条人工验收，堵掉「测试内容与验收原文不符仍能过门禁」的口子 |
| v0.11 | 2026-08-13 | **M0 实现验收进入 `review`。** 功能基线完成并通过确定性、外部 MCP 与真实 CLI 链路；明确当前界面非设计定稿，M1 开工前确定设计稿与 token design system |
| v0.10 | 2026-08-13 | **实现回流：**首屏呈现本位币选择，并明确切换不改已有草稿/事实 |
| v0.9 | 2026-08-13 | **M0 开始实施，`status` 进入 `in-progress`。** 补人工丢弃契约与验收；修正两个验收层级/命名漂移 |
| v0.8 | 2026-08-10 | **文档审查第五轮回流两处。** ① **`file` 永不判 `not_applicable` 补上真正的理由**——不是「单笔截图理论上不适用」，而是 `reported_total_* IS NULL` 分不清「结构性没有 / 漏读 / 截图裁掉」三种现实，从「没读到」反推「本来就没有」会把漏读伪装成正常；适用性信号登记为 R7。② §3.4 的 UI 文案随「口述也可能报合计」分两种。§6 新增 2 条验收 |
| v0.7 | 2026-08-10 | **文档审查第三轮回流**：口述里明说合计时**允许对账**（`passed` / `failed`），确认策略仍恒为 `user_attested_batch`——解开 §3.3 的论据与 [00](./00-foundation.md)「恒为空」的自相矛盾。§3.1 完整性校验那行改按 `confirmation_policy` 判；§6 新增 1 条验收 |
| v0.6 | 2026-08-10 | **文档审查第二轮回流两处。** ① **总额校验的入参由 `source_id` 改为 `attempt_id`**——按来源求和会把重试后两次尝试的草稿混在一起，且合计与草稿的生命周期脱钩（合计已随 [00 地基](./00-foundation.md) v0.7 移入 `parse_attempts.reported_total_*`）。② **`not_applicable` 拆成两个维度**：`reconciliation_status`（能不能对账）+ `confirmation_policy`（能不能批量确认），**放行批量的是后者**；`kind = file` 永远拿不到 `user_attested_batch`。§6 验收新增 5 条、改写 6 条 |
| v0.5 | 2026-08-10 | **文档审查回流五处。** ① §3.3 **求和范围由「未消费草稿」改为「全部未作废草稿」**——逐条确认一条后该来源永远回不到 `passed`。② §3.3 **校验式按 `declared_total_kind` 分三条等式**，`transfer` 条目让结果变 `unavailable`。③ §3.3 **新增第四态 `not_applicable`**：口述来源允许一次批量确认，换成三条 UI 硬要求构成的人工闸门；`kind = file` 永不适用。④ §3.2 区分**证据（原件）与 `evidence_text`（抽取声明）**，原件 M0 就要可见。⑤ §3.5 行内编辑不得覆盖 `drafted_json`；§3.6 实体类型切换验收由 M1 移到 **M3**。§6 验收新增 9 条、改写 2 条，人工验收新增 2 条 |
| v0.1 | 2026-08-06 | 初版：两条写入路径的物理隔离、证据链呈现要求、总额校验三态（含 `unavailable` 不伪装成通过、无 force 旁路）、异常前置五级排序、键盘流键位表与默认全选、纠正留痕与记忆投递、虚拟滚动；否决方案八条；待决 R1–R5；验收标准 14 条可执行 + 3 条人工 |
| v0.4 | 2026-08-08 | 公开仓库去个人化：§5 R3 与本表的决策署名改为「产品决定」，去掉工具与会话指代；`owner` 改为 `@maintainer`。**决定与验收标准未变** |
| v0.3 | 2026-08-08 | **设计评审回流。** ① §3.4 异常前置**由五级扩为六级**，新增第 2 档「`kind = utterance` 来源」——闸门 3 对语音天然失效（无声明合计），UI 必须明示「无合计可校验」，**不让它们看起来和已校验的截图草稿一样安全**；第 5 档补注「domain 读 `memory_rules` 是为标记冲突、不是覆盖分类」（[06 §3.4](./06-memory.md) C′）。② §3.6 新增**「改实体类型」为硬要求**（M3 起）：一句口述里交易与事项的归属由 agent 判断，分类错是**高可见低损害**的错误，但草稿分两表、改类型物理上是删+建；无一键转换则用户只能重说一遍，**批处理省下的时间会在这一下被吃掉**。转换写两条审计且证据链字段不变。③ §6 新增 4 条验收 |
| v0.2 | 2026-08-07 | **M0 开工评审 → `status: ready`。** ① §3.3 **校验式定死**——原文「`SUM(该来源的草稿金额)`」未说原币还是本位币，外币账单必错；现明确在 `declared_total_currency` 维度求和，原币同币种取 `amount_minor`、异币种取 `base_amount_minor`。② §3.3 `unavailable` 扩为两种触发条件（未声明合计 **或** 存在取不到该币种金额的草稿），并要求列出是哪几条——「算不出来」与「算出来不对」不可混为一谈。③ §3.3 新增**「基准值本身必须可核对」**：如实写明闸门 3 的结构性边界（校验两边同源，挡不住逐笔与合计一起读错），对策是强制 `declared_total_evidence_text` 并在批量确认按钮附近与合计并排显示。④ §3.1 补**溯源字段 `source_draft_id`**（原文承诺审计能回答「当初起草成什么样」，schema 无落点）与确认时的三项完整性校验及对应错误码。⑤ §6 验收**按 M0/M1 分层**，把无法实现的 `confirm_not_reachable_from_mcp` 改为 `rg` 检查，新增 5 条校验式相关用例。⑥ §5 新增 R6（外币行参与合计的舍入敏感性） |
