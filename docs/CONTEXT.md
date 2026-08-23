---
title: Daybook 术语表
status: ready
owner: "@maintainer"
date: 2026-08-23
version: v0.12
---

# 术语表

> 让「没读过那场对话的人」和「无记忆的 agent」在读任何一份文档时，对同一个词的理解一致。
>
> **本文只给定义 + 权威出处，不解释理由、不复制规则。** 想知道「为什么这么定」去 ADR，想知道「怎么写」去 [`.claude/rules/`](../.claude/rules/)，想知道「规格是什么」去 [`docs/prd/`](./prd/)。词条与权威出处冲突时**以出处为准**——本文是索引，不是第二个事实源。
>
> **词义变了先改本文，再改用到它的地方。**

---

## 产品层

| 词 | 定义 | 出处 |
|---|---|---|
| **Daybook / 日簿** | 本产品。`daybook` 是会计术语「日记账」——按时间顺序记录原始凭证的簿子 | [`README.md`](../README.md) |
| **个人事务助理** | 本产品的品类定位：**不用逐条填表，把零散的钱和事整理成账目与安排**。「个人事务」指交易与事项两个实体，不等于完整日历或通用秘书 | [`docs/PRD.md` §1](./PRD.md) |
| **回溯优先** | 本产品的设计原则：**生活可以先发生，记录可以后来补**。为「事后补记」设计、把核心动作放在重建而非录入；不再用「回溯记录器」作为品类名称 | [`docs/PRD.md` §1](./PRD.md) |
| **考古学家** | AI 在本产品中的角色比喻：从**痕迹**里把过去还原成待确认草稿。对立面是「输入框」——把 AI 当成更快的打字方式 | [`docs/PRD.md` §1](./PRD.md) |
| **集中整理** | 典型使用场景：一两周一次，坐下来把过去这段时间的钱和事一次整理完。产品气质由此定为**低频、重型、有仪式感** | [`docs/PRD.md` §1](./PRD.md) |

## 数据实体

| 词 | 定义 | 出处 |
|---|---|---|
| **交易 / transaction** | 一笔钱的流动。v1 的「深」模块 | [`04-transactions`](./prd/04-transactions.md) |
| **分类 / category** | 用户管理的扁平账目归类；支出分类描述主要用途，收入分类描述主要来源。分类有稳定身份与不可变的支出 / 收入作用域，改名不改变身份 | [`04-transactions` §3.3](./prd/04-transactions.md) |
| **未分类 / uncategorized** | 一笔支出或收入尚未判断分类，或用户主动清空分类；它是「没有分类」，不是名为“未分类”的分类实体，也不等于“其他” | [`04-transactions` §3.3](./prd/04-transactions.md) |
| **其他分类** | 「其他支出」「其他收入」两条真实分类，表示方向已判断但没有更合适分类；在回顾中与未分类分开 | [`04-transactions` §3.3](./prd/04-transactions.md) |
| **转账 / transfer** | 用户自有账户之间的资金移动，不是「使用银行转账付款」；向外付款仍是支出，外部收款按收入语义判断。转账不使用账目分类 | [`04-transactions` §3.1/§3.3](./prd/04-transactions.md) |
| **分类体系操作** | 对分类目录本身的新增、改名、停用、删除、合并或拆分；与修改某一笔交易的分类、修改商户分类规则是三个不同动作 | [`04-transactions` §3.3](./prd/04-transactions.md) |
| **事项 / item** | 一件准备做或已经做过的事，**同一实体包含计划与结果两端**；状态为 `backlog / scheduled / done / archived`。计划与结果分层，结果只来自用户明确陈述、手工修正或经确认的默认值，不从计划或点击时刻自动编造。v1 的「薄」模块 | [ADR-0004 §4](./adr/0004-data-model-sqlite-integer-money.md) · [`05-items`](./prd/05-items.md) |
| **backlog / 未安排** | 已记下但没有计划日期、时间或日期范围的事项集合。可以有截止约束；**只有截止不等于已安排**，也不表示优先级低 | [`05-items` §3.1](./prd/05-items.md) |
| **计划时间** | 用户准备何时做事项。v1 可为单日不定时、时间点、最长 24 小时时间块或粗粒度日期范围；使用浮动本地民用时间 | [`05-items` §3.2](./prd/05-items.md) |
| **结果时间** | `done` 事项中用户明确回溯的实际日期、开始时间、用时或日期范围。允许只覆盖计划中的部分字段；未报告字段显示时可回退计划值，但不复制进结果事实 | [`05-items` §3.2](./prd/05-items.md) |
| **截止约束** | 最晚完成的日期与可选时间，和计划正交。截止不生成计划、不占用时间格、不触发 v1 提醒 | [`05-items` §3.3](./prd/05-items.md) |
| **不定时** | 已安排到某一天，但没有具体开始时间。不是占满整天的「全天事项」 | [`05-items` §3.2](./prd/05-items.md) |
| **日期范围** | 用户只声明事项计划或结果落在起止日期之间的粗粒度表达；不推断每天都发生、每日内容或每日用时 | [`05-items` §3.2](./prd/05-items.md) |
| **事项清单** | 每个事项最多所属的一个用户自定义分组，用于周视图着色、筛选和浏览；未选择时为「未分组」，不等于多标签或优先级 | [`05-items` §3.5](./prd/05-items.md) |
| **来源 / source** | 一份**痕迹**，不一定是文件。两种 `kind`：**`file`**（截图 / PDF）与 **`utterance`**（一段口述或文字的转写结果，转写文本也落盘成 `.txt`）。它是证据链的锚点 | [`00-foundation` §3.6](./prd/00-foundation.md)「来源不等于文件」 |
| **证据 / evidence** | **不可变的来源原件**——截图字节 / `utterance` 的转写文本。导入时逐位落盘，之后谁也不改。**审核界面必须让它本身可见** | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 2 |
| **位置声明 / `source_ordinal`** | agent 声称「这条是原件上的第几条」（1 起，同尝试内唯一，允许跳号）。**必填**——`file` 来源没有 OCR 也没有坐标，位置**只能由 agent 报**，[07 评测](./prd/07-eval.md) 的条目对齐用的就是它。`utterance` 另带 **Unicode code point 区间**（零起、左闭右开，对未 normalize 的落盘文本计；Rust `.chars()` / TS `Array.from`——**不是字节偏移、不是 UTF-16 索引**） | [`01-agent-runtime` §3.2](./prd/01-agent-runtime.md) · [`07-eval` §3.2](./prd/07-eval.md) |
| **抽取声明 / `evidence_text`** | agent 声称「我读的是这一段」。**不是证据**——它和被核对的金额出自同一次模型输出，模型读错时它也会跟着错，两者自洽却一起错。作用是**指出原件上的位置**，不是充当核对基准。草稿表这两个字段（`source_id` + `evidence_text`）都非空，由数据层约束保证 | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 2 · [`03-review` §3.2](./prd/03-review.md) |
| **起草快照 / `drafted_json`** | agent 首次写入草稿时的完整字段快照，**写入后永不更新**。人在审核界面的行内编辑改业务列、不动它——它是「AI 当初写的是什么」的唯一答案，也是 eval **被评分的那一侧**（**不是真值**——真值是 `expected.json`） | [`00-foundation` §3.6](./prd/00-foundation.md) · [`07-eval` §3.2](./prd/07-eval.md) |
| **解析尝试 / parse_attempt** | 一次**解析任务** spawn 一行，无论成败（工具集探测那次不算）。记后端、模型、提示词哈希、期望与**实测**的工具面指纹、结果、自报条目数，以及**本次报告的合计**。**重试不覆盖上一次**——`sources.latest_attempt_id` 只指向当前受审的那一次 | [`00-foundation` §3.6](./prd/00-foundation.md) |
| **草稿 / draft** | AI 产出的**待确认**记录，存在 `draft_*` 表。**AI 只能写这里** | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 1 |
| **事实表** | 存放已确认数据的表（`transactions`、`items`…）。**只能由人工确认动作写入**，agent 无任何可达写入路径 | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 1 |
| **审计日志 / audit_log** | append-only 的变更记录：「谁 / 何时 / 把什么改成了什么」。只追加，不更新、不删除 | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 4 |

## 金额与币种

| 词 | 定义 | 出处 |
|---|---|---|
| **最小货币单位** | 一单位货币的最小可分割额。**金额一律以整数存储它**，任何位置禁止浮点。**不恒等于「分」**——由 ISO 4217 的 minor unit exponent 决定：多数币种 2 位，JPY/KRW 是 0，KWD/BHD/JOD 是 3。因此 `amount_minor` 必须与 `currency` 一起读，格式化除以 `10^exponent` 而非写死 100。**表里没有的币种是非法数据**（`data.unsupported_currency`），**不回退到 2**——带告警入账的错误金额比拒绝更糟 | [ADR-0004 §2](./adr/0004-data-model-sqlite-integer-money.md) · [`00-foundation` §3.4](./prd/00-foundation.md) · [`money-and-data.md` §1](../.claude/rules/money-and-data.md) |
| **原币 / 原币金额** | 交易实际发生时用的货币与金额，**与截图上写的完全一致** | [ADR-0004](./adr/0004-data-model-sqlite-integer-money.md) |
| **本位币** | 用户记账的基准货币。**可切换**，且 `base_currency` **逐笔存储、确认时冻结**——切换只影响新交易，不改历史行；跨切换点的汇总按本位币**分组呈现，不静默相加** | [`00-foundation` §3.4](./prd/00-foundation.md)「本位币切换语义」 |
| **三元组** | 每笔交易同时存的三个值：**原币金额 + 本位币金额 + 当时汇率**。单币种走同一套，汇率为 1，不设特例 | [ADR-0004 §3](./adr/0004-data-model-sqlite-integer-money.md) |
| **渠道 / channel** | **支付方式类别**（`bank_debit` / `bank_credit` / `wallet` / `cash`）。**与币种是两个维度**（同一张卡可能有多币种账户），**与账户也是两个维度**。集合由用户自己维护，产品不预设任何国家的银行清单 | [`04-transactions` §3.4](./prd/04-transactions.md) |
| **账户 / account** | **具体的那张卡 / 那个账户**，与渠道正交：同一账户可有多种支付方式，同一支付方式跨多个账户。**业务在 M2**，但列与 `accounts` **骨架表在 M0 就建**（M0/M1 恒空）——外键指向不存在的父表时 SQLite 连插入 `NULL` 都会失败 | [`04-transactions` §3.4](./prd/04-transactions.md) · [`00-foundation` §3.6](./prd/00-foundation.md) |

## 校验与审核

| 词 | 定义 | 出处 |
|---|---|---|
| **声明合计** | 来源上**原本印着**的那个合计（账单底部的 Total 那一行）。**库里存的是 agent 某一次解析对它的报告**——`parse_attempts.reported_total_*` 四列（金额、币种、**类型**、原文片段），**不在 `sources` 上**：它是那次尝试的输出，和草稿同生共死。**不是账户余额，也不是 agent 把逐笔加起来的结果**。类型判不出来时不许填 | [`00-foundation` §3.6](./prd/00-foundation.md)「声明合计归尝试，不归来源」 |
| **声明合计的类型** | `expense_total` / `income_total` / `net_change`。三者对应**三条不同的等式**——把收入行算进「消费合计」，校验会稳定地错 | [`00-foundation` §3.6](./prd/00-foundation.md)「合计必须带类型」 |
| **总额交叉校验** | **入参是 `attempt_id`**：对该次尝试**全部未作废**草稿（不论是否已消费）按合计的类型求和，须与该次报告的合计精确相等。**它是那次尝试的属性，确认动作不改变它**；`sources.latest_attempt_id` 决定当前审的是哪次输出。**不伪装成通过，也不谎报 failed** | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 3 · [`03-review` §3.3](./prd/03-review.md)「校验式」 |
| **`reconciliation_status`** | 对账结果四态：`passed` / `failed` / `unavailable`（本该有却取不到）/ `not_applicable`（结构性没有）。**它只回答「能不能对账」** | [`03-review` §3.3](./prd/03-review.md) |
| **`confirmation_policy`** | 确认策略三态：`reconciled_batch`（机器对上账）/ `user_attested_batch`（**人对着整段原文背书**）/ `single_only`（只能逐条）。**真正放行批量确认的是它，不是 `not_applicable`**——两者是两个维度，口述里说了合计时对账可做而策略仍走背书那一档。`kind = file` 永远拿不到 `user_attested_batch` | [`03-review` §3.3](./prd/03-review.md) · [`docs/PRD.md` §1.1](./PRD.md) |
| **审核界面** | 产品的**胜负手**——省下的时间全兑现在这一屏。判定标准 **40 笔 30 秒** | [`03-review`](./prd/03-review.md) |
| **异常前置** | 审核界面的排序规则：校验不过的、置信度低的、与历史规则冲突的排最前。注意力花在可疑项上，而不是均匀分给 40 条 | [`03-review` §3.4](./prd/03-review.md) |

## Agent 与运行时

| 词 | 定义 | 出处 |
|---|---|---|
| **agent CLI** | 用户**自己已安装并登录**的命令行 agent（`claude -p` / `codex exec`）。应用不打包厂商凭证、不提供第三方登录、不代理鉴权 | [ADR-0003 §4](./adr/0003-agent-runtime-and-pluggable-backend.md) |
| **MCP server** | 本应用暴露给 agent 的工具面，走 stdio、用 `rmcp` 实现。**本 app 本质上是一个本地 MCP server**——不让 agent 输出 JSON 再解析。它跑在一个**独立的 MCP helper 二进制**里（2026-08-12 由 R6 定案），由 agent CLI 拉起，经 Unix domain socket 连回主进程；**helper 不碰数据库** | [ADR-0003 §1](./adr/0003-agent-runtime-and-pluggable-backend.md)、[`01-agent-runtime` §3.1](./prd/01-agent-runtime.md) |
| **agent launcher** | Rust 侧负责 spawn / 监控 / 回收 agent CLI 子进程的组件 | [`01-agent-runtime` §3.4](./prd/01-agent-runtime.md) |
| **工具权限** | MCP 工具的写入范围**在实现层面锁死**，**权限边界就是工具签名**。不得提供通用的「执行任意 SQL」类工具 | [ADR-0003 §3](./adr/0003-agent-runtime-and-pluggable-backend.md) · [`01-agent-runtime` §3.2](./prd/01-agent-runtime.md) |
| **有效工具集** | agent **实际拿得到**的那一套，**不等于我们注册的那一套**——通用 CLI 自带执行命令与文件读写工具，足以绕过全部四道闸门。**闸门 1 的前提是子进程密封启动 + 有效工具集实测**，不相等即拒绝下发任务 | [ADR-0003 §3](./adr/0003-agent-runtime-and-pluggable-backend.md) · [`01-agent-runtime` §3.7](./prd/01-agent-runtime.md) |
| **密封启动配置** | 起 agent 子进程时关掉内置工具、外部配置来源、其他 MCP server 与权限绕过模式，只留本应用注入的那一组。**具体 flag 不写进规格**（CLI 会变），规格规定的是目标状态与验证方式 | [`01-agent-runtime` §3.7](./prd/01-agent-runtime.md) |
| **完成协议 / `complete_source`** | agent 必须显式声明「这个来源我读完了」，附条目数与未解析区域。**没调即协议失败，来源不判为 `parsed`**——退出码 0 证明不了它没读一半就走 | [`01-agent-runtime` §3.2](./prd/01-agent-runtime.md) |
| **smart agent, dumb tools** | 工具设计原则：**推理、编排、判断留给 agent；工具只做执行。** 但 **dumb ≠ gullible**——拒绝畸形输入（缺原文片段、三元组不自洽）不是智能，是类型 | [ADR-0006](./adr/0006-smart-agent-dumb-tools.md) |
| **子 agent** | **只用于上下文隔离**（如解析超长截图），**不用于业务分工**。产品运行时不引入多 agent 自主编排。**与开发期 subagent 同名但不同层** | [ADR-0003 §2](./adr/0003-agent-runtime-and-pluggable-backend.md) · [`.claude/agents/README.md`](../.claude/agents/README.md) |
| **可插拔后端** | agent 后端的抽象接口。**后端只能是用户已配置好的外部进程**：`claude -p` / `codex exec` / 本地模型进程——应用不存凭证、不发出站请求。v1 只实现 Claude Code，但接口从第一天存在。**它是本机概念，不是远程服务端** | [ADR-0003 §4](./adr/0003-agent-runtime-and-pluggable-backend.md) |
| **日志分级** | 落盘，两级：`trace`（只记形状，无金额/原文/prompt）与 `debug`（含完整 prompt 与工具调用参数，供夹具重放） | [ADR-0007](./adr/0007-local-observability-and-log-tiers.md) |

## 记忆与评测

| 词 | 定义 | 出处 |
|---|---|---|
| **记忆 / memory** | **存规则，不存对话**：商户→分类映射、用户的每次纠正、个人语境词表、语音专有名词表。由 agent 主动调 `query_memory` 按键查，**不预先注入上下文**。**规则只能影响待确认草稿；事实只由人的确认动作产生；规则变化不追溯改写事实** | [`06-memory` §3.4/§3.5](./prd/06-memory.md) |
| **商户分类规则 / `merchant_category`** | 「某个商户文本模式以后默认建议到哪个稳定分类」的用户确认规则；默认只影响未来草稿，不等于分类体系操作，也不自动改写历史 | [`06-memory` §3.2/§3.3](./prd/06-memory.md) |
| **纠正 / correction** | 用户在审核时对草稿做的修改，**就是 `audit_log` 里 `actor = "human"` 的那一行**（没有第二套事件表）。**一份数据三处用**：记忆规则的输入、审计留痕、eval 样本 | [`06-memory` §3.2](./prd/06-memory.md) · [`07-eval` §3.2](./prd/07-eval.md) |
| **eval 集 / 评测集** | 度量「agent 读得准不准」的样本集合。**真值只有一个：来源级期望条目集合**（`expected.json`，每条带位置标识）；`drafted_json` 是**被评分的输出快照**，不是真值。评分**先按 `source_ordinal` 做 full outer join、再逐字段比**（ordinal 两侧唯一，**不是序列对齐、不需要动态规划**）——拿被评字段当匹配键会让字段准确率恒为 100%。用例清单固定在 manifest 里。真调模型、烧订阅额度、**不进 CI** | [`07-eval` §3.2/§3.3](./prd/07-eval.md) |
| **夹具 / fixture** | 一次真实运行的**录像**：输入 + 那次的完整工具调用序列 + 期望结果 + **重放所需的环境**（初始 DB 状态、ID 映射、版本三元组）。**重放时跳过 agent**，所以测的不是模型，是「agent 读错时代码有没有拦住」——零额度、确定性、**可进 CI**。本机夹具含真实金额，不进 git | [`07-eval` §3.6](./prd/07-eval.md) |

## 其他

| 词 | 定义 | 出处 |
|---|---|---|
| **弱信号采集** | 从日历、git、浏览器历史、屏幕使用时间等处自动获取活动痕迹。**v1 明确不做** | [ADR-0005 §4](./adr/0005-voice-and-system-integration.md) |
| **sub-PRD** | 一个能力一份的规格文档，扁平存放在 [`docs/prd/`](./prd/)。**本项目不用 ticket** | [`CLAUDE.md`](../CLAUDE.md)「PRD 体系与工作流」 |
| **回流** | 把实现相对规格的偏离、澄清、新发现**回写到 sub-PRD**，版本号 +0.1。口诀：**计划易失，决定回流** | [`docs/prd/CLAUDE.md`](./prd/CLAUDE.md)「回流义务」 |

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.12 | 2026-08-23 | **补账目分类领域语言。** 新增分类、未分类、其他分类、转账、分类体系操作与商户分类规则；只给定义并链接 [04 交易](./prd/04-transactions.md) / [06 记忆](./prd/06-memory.md)，不复制默认分类清单、生命周期或数据表细节 |
| v0.11 | 2026-08-17 | **事项术语随 [`docs/PRD.md` v0.21](./PRD.md) 与 [05 事项 v0.8](./prd/05-items.md) 重写。** 「待办 + 时间日志」改为同一实体的「计划 + 结果」；更新事项与 backlog 定义，新增计划时间、结果时间、截止约束、不定时、日期范围与事项清单。术语只给定义，完整状态/时间规则仍以 [05 事项](./prd/05-items.md) 为准 |
| v0.10 | 2026-08-14 | **产品层术语随 [`docs/PRD.md` v0.18](./PRD.md) 调整。** 新增正式品类「个人事务助理」与设计原则「回溯优先」，停止把「回溯记录器」当作品类；「财务体检」改为同时覆盖交易与事项的「集中整理」。数据实体与功能范围未变 |
| v0.8 | 2026-08-10 | **随第六轮同步**：`effective_tool_surface_hash` 改名 **`effective_capability_hash`** 并覆盖非工具型能力（hook / 插件 / 权限模式）；**声明合计**与 **`confirmation_policy`** 补「口述明说合计时对账照常做、策略不变」 |
| v0.7 | 2026-08-10 | **随第五轮同步**：**位置声明**补 span 的坐标系（code point、零起左闭右开、未 normalize）；**eval 集**的配对术语改为 **ordinal 上的 full outer join** |
| v0.6 | 2026-08-10 | **随第四轮同步**：新增 **位置声明 / `source_ordinal`** 词条（`file` 无 OCR 无坐标，位置只能由 agent 报，eval 对齐用它）；**最小货币单位**补「未知币种是非法数据、不回退」；**声明合计**与 **`confirmation_policy`** 补「口述里说了合计时对账照常做、策略不变」 |
| v0.5 | 2026-08-10 | **随第三轮文档审查同步（[`docs/PRD.md` v0.10](./PRD.md)）**：**声明合计**改为「存在 `parse_attempts.reported_total_*`，不在 `sources` 上」；**总额交叉校验**入参改为 `attempt_id`；**`not_applicable`** 一条拆成 **`reconciliation_status`** 与 **`confirmation_policy`** 两条（放行批量的是后者）；**eval 集**的真值收窄为 `expected.json` 一个、匹配改为按位置保序对齐；**最小货币单位**补 IPC 上传字符串。**新增**：`accounts` 骨架在 M0 建（见「账户 / account」出处） |
| v0.4 | 2026-08-10 | **随第二轮文档审查同步词义（[`docs/PRD.md` v0.9](./PRD.md)）。** 改写四条：**证据**（收窄为不可变原件）· **最小货币单位**（不恒等于「分」，由 ISO 4217 exponent 决定）· **渠道**（收窄为支付方式类别）· **总额交叉校验**（求和范围是全部未作废草稿、按类型选等式、四态）· **记忆**（「永不成为事实源」改为准确表述）· **纠正**（就是 `audit_log` 里 `actor = "human"` 的行）· **eval 集**（真值改为 `drafted_json` + 来源级期望集合）· **夹具**（补重放环境）。新增八条：**抽取声明 / `evidence_text`** · **起草快照 / `drafted_json`** · **解析尝试 / parse_attempt** · **账户 / account** · **声明合计的类型** · **`not_applicable`** · **有效工具集** · **密封启动配置** · **完成协议 / `complete_source`** |
| v0.9 | 2026-08-12 | **MCP server 词条的「进程归属尚未定」改为定案**（[01 §3.1](./prd/01-agent-runtime.md)，2026-08-12 R6 spike）：独立 helper 二进制，由 agent CLI 拉起，经 Unix domain socket 连回主进程，**自己不碰数据库**。**词义未变**，只是补上了它住在哪 |
| v0.3 | 2026-08-09 | **收回术语表本分。** 本文此前把架构、记忆机制、评测方法与流程决定的**论证**整段抄了进来（约 195 行），形成第二个事实源——改了 ADR 就得记得改这里，而没人会记得。现改为**定义 + 权威出处**的表格，理由一律留在出处。同步修正三处已漂移的措辞：总额校验的基准由「合计/余额」改为**声明合计**（[ADR-0002](./adr/0002-ai-never-writes-directly.md) 同日修订）；证据链去掉「每条**交易**草稿」的窄化，改为全部 `draft_*` 表（[05 §3.4](./prd/05-items.md) 同日删除事项例外）；MCP server 条目标注进程归属未定（[`01-agent-runtime` §5](./prd/01-agent-runtime.md) R6）。新增 `声明合计` 与 `日志分级` 两个词条。**可插拔后端**词条删掉「用户自备 API key」并注明它是本机概念、不是远程服务端（[ADR-0003 §4](./adr/0003-agent-runtime-and-pluggable-backend.md) 同日修订）。**词义未变** |
| v0.2 | 2026-08-08 | 公开仓库去个人化：`owner` 改为 `@maintainer`。**术语与定义未变** |
| v0.1 | 2026-08-06 | 初版术语表 |
