---
title: Daybook 术语表
status: ready
owner: "@maintainer"
date: 2026-08-09
version: v0.3
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
| **回溯记录器** | 本产品的品类定位：**为「事后补记」设计，不是为「当场记录」**。不是记账 app，不是待办 app。核心动作是**重建**，不是**录入** | [`docs/PRD.md` §1](./PRD.md) |
| **考古学家** | AI 在本产品中的角色比喻：从**痕迹**里把过去还原成待确认草稿。对立面是「输入框」——把 AI 当成更快的打字方式 | [`docs/PRD.md` §1](./PRD.md) |
| **财务体检** | 典型使用场景：一两周一次，坐下来把过去这段时间的钱和事一次补完。产品气质由此定为**低频、重型、有仪式感** | [`docs/PRD.md` §2](./PRD.md) |

## 数据实体

| 词 | 定义 | 出处 |
|---|---|---|
| **交易 / transaction** | 一笔钱的流动。v1 的「深」模块 | [`04-transactions`](./prd/04-transactions.md) |
| **事项 / item** | 一件要做或已做的事，**一条记录走完整生命周期**：`backlog（无日期）→ 排到某天 → 完成（带实际时长）`。「待办」与「时间日志」是这个实体的两端，不是两个功能。v1 的「薄」模块 | [ADR-0004 §4](./adr/0004-data-model-sqlite-integer-money.md) · [`05-items`](./prd/05-items.md) |
| **backlog** | 事项生命周期的起点：**已记下但尚未排到某一天**的事项集合。不是「优先级低的事」 | [`05-items` §3.1](./prd/05-items.md) |
| **来源 / source** | 一份**痕迹**，不一定是文件。两种 `kind`：**`file`**（截图 / PDF）与 **`utterance`**（一段口述或文字的转写结果，转写文本也落盘成 `.txt`）。它是证据链的锚点 | [`00-foundation` §3.6](./prd/00-foundation.md)「来源不等于文件」 |
| **证据 / evidence** | 草稿与来源之间的具体连接：`source_id` + **原文片段**。**全部 `draft_*` 表的这两个字段都非空**，由数据层约束保证，不靠 UI 自觉 | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 2 |
| **草稿 / draft** | AI 产出的**待确认**记录，存在 `draft_*` 表。**AI 只能写这里** | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 1 |
| **事实表** | 存放已确认数据的表（`transactions`、`items`…）。**只能由人工确认动作写入**，agent 无任何可达写入路径 | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 1 |
| **审计日志 / audit_log** | append-only 的变更记录：「谁 / 何时 / 把什么改成了什么」。只追加，不更新、不删除 | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 4 |

## 金额与币种

| 词 | 定义 | 出处 |
|---|---|---|
| **最小货币单位** | 分 / cent。**金额一律以整数存储最小货币单位**，任何位置禁止浮点 | [ADR-0004 §2](./adr/0004-data-model-sqlite-integer-money.md) · [`money-and-data.md` §1](../.claude/rules/money-and-data.md) |
| **原币 / 原币金额** | 交易实际发生时用的货币与金额，**与截图上写的完全一致** | [ADR-0004](./adr/0004-data-model-sqlite-integer-money.md) |
| **本位币** | 用户记账的基准货币。**可切换**，且 `base_currency` **逐笔存储、确认时冻结**——切换只影响新交易，不改历史行；跨切换点的汇总按本位币**分组呈现，不静默相加** | [`00-foundation` §3.4](./prd/00-foundation.md)「本位币切换语义」 |
| **三元组** | 每笔交易同时存的三个值：**原币金额 + 本位币金额 + 当时汇率**。单币种走同一套，汇率为 1，不设特例 | [ADR-0004 §3](./adr/0004-data-model-sqlite-integer-money.md) |
| **渠道 / channel** | 这笔钱经过的支付通道。**渠道与币种是两个维度**（同一张卡可能有多币种账户）。集合由用户自己维护，产品不预设任何国家的银行清单 | [`04-transactions`](./prd/04-transactions.md) |

## 校验与审核

| 词 | 定义 | 出处 |
|---|---|---|
| **声明合计** | 来源上**原本印着**的那个合计（账单底部的 Total 那一行），存在 `sources.declared_total_*` 三列。**不是账户余额，也不是 agent 把逐笔加起来的结果** | [`00-foundation` §3.6](./prd/00-foundation.md) · [`01-agent-runtime` §3.2](./prd/01-agent-runtime.md) |
| **总额交叉校验** | 从一个来源拆出的条目合计，必须与该来源的**声明合计**精确相等；不符时报警并阻止批量入库。来源没印合计时结果是 `unavailable`，**不伪装成通过** | [ADR-0002](./adr/0002-ai-never-writes-directly.md) 闸门 3 · [`03-review` §3.3](./prd/03-review.md)「校验式」 |
| **审核界面** | 产品的**胜负手**——省下的时间全兑现在这一屏。判定标准 **40 笔 30 秒** | [`03-review`](./prd/03-review.md) |
| **异常前置** | 审核界面的排序规则：校验不过的、置信度低的、与历史规则冲突的排最前。注意力花在可疑项上，而不是均匀分给 40 条 | [`03-review` §3.4](./prd/03-review.md) |

## Agent 与运行时

| 词 | 定义 | 出处 |
|---|---|---|
| **agent CLI** | 用户**自己已安装并登录**的命令行 agent（`claude -p` / `codex exec`）。应用不打包厂商凭证、不提供第三方登录、不代理鉴权 | [ADR-0003 §4](./adr/0003-agent-runtime-and-pluggable-backend.md) |
| **MCP server** | 本应用暴露给 agent 的工具面，走 stdio、用 `rmcp` 实现。**本 app 本质上是一个本地 MCP server**——不让 agent 输出 JSON 再解析。⚠️ **它跑在哪个进程里尚未定**（[`01-agent-runtime` §5](./prd/01-agent-runtime.md) R6） | [ADR-0003 §1](./adr/0003-agent-runtime-and-pluggable-backend.md) |
| **agent launcher** | Rust 侧负责 spawn / 监控 / 回收 agent CLI 子进程的组件 | [`01-agent-runtime` §3.4](./prd/01-agent-runtime.md) |
| **工具权限** | MCP 工具的写入范围**在实现层面锁死**，**权限边界就是工具签名**。不得提供通用的「执行任意 SQL」类工具 | [ADR-0003 §3](./adr/0003-agent-runtime-and-pluggable-backend.md) · [`01-agent-runtime` §3.2](./prd/01-agent-runtime.md) |
| **smart agent, dumb tools** | 工具设计原则：**推理、编排、判断留给 agent；工具只做执行。** 但 **dumb ≠ gullible**——拒绝畸形输入（缺原文片段、三元组不自洽）不是智能，是类型 | [ADR-0006](./adr/0006-smart-agent-dumb-tools.md) |
| **子 agent** | **只用于上下文隔离**（如解析超长截图），**不用于业务分工**。产品运行时不引入多 agent 自主编排。**与开发期 subagent 同名但不同层** | [ADR-0003 §2](./adr/0003-agent-runtime-and-pluggable-backend.md) · [`.claude/agents/README.md`](../.claude/agents/README.md) |
| **可插拔后端** | agent 后端的抽象接口。**后端只能是用户已配置好的外部进程**：`claude -p` / `codex exec` / 本地模型进程——应用不存凭证、不发出站请求。v1 只实现 Claude Code，但接口从第一天存在。**它是本机概念，不是远程服务端** | [ADR-0003 §4](./adr/0003-agent-runtime-and-pluggable-backend.md) |
| **日志分级** | 落盘，两级：`trace`（只记形状，无金额/原文/prompt）与 `debug`（含完整 prompt 与工具调用参数，供夹具重放） | [ADR-0007](./adr/0007-local-observability-and-log-tiers.md) |

## 记忆与评测

| 词 | 定义 | 出处 |
|---|---|---|
| **记忆 / memory** | **存规则，不存对话**：商户→分类映射、用户的每次纠正、个人语境词表、语音专有名词表。由 agent 主动调 `query_memory` 按键查，**不预先注入上下文**；学出来的规则是派生物，**永不成为事实源**，改规则不追溯重算 | [`06-memory` §3.4](./prd/06-memory.md) |
| **纠正 / correction** | 用户在审核时对草稿做的修改。**一份数据三处用**：记忆规则的输入、`actor = "human"` 的审计留痕、eval 样本 | [`06-memory`](./prd/06-memory.md) · [`07-eval` §3.2](./prd/07-eval.md) |
| **eval 集 / 评测集** | 度量「agent 读得准不准」的样本集合，从「草稿 ←`source_draft_id`→ 交易」这条 join 里挑——**不需要独立标注工序**。真调模型、烧订阅额度、**不进 CI** | [`07-eval` §3.2/§3.3](./prd/07-eval.md) |
| **夹具 / fixture** | 一次真实运行的**录像**：输入 + 那次的完整工具调用序列 + 人确认后的正确结果。**重放时跳过 agent**，所以测的不是模型，是「agent 读错时代码有没有拦住」——零额度、确定性、**可进 CI**。本机夹具含真实金额，不进 git | [`07-eval` §3.6](./prd/07-eval.md) |

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
| v0.3 | 2026-08-09 | **收回术语表本分。** 本文此前把架构、记忆机制、评测方法与流程决定的**论证**整段抄了进来（约 195 行），形成第二个事实源——改了 ADR 就得记得改这里，而没人会记得。现改为**定义 + 权威出处**的表格，理由一律留在出处。同步修正三处已漂移的措辞：总额校验的基准由「合计/余额」改为**声明合计**（[ADR-0002](./adr/0002-ai-never-writes-directly.md) 同日修订）；证据链去掉「每条**交易**草稿」的窄化，改为全部 `draft_*` 表（[05 §3.4](./prd/05-items.md) 同日删除事项例外）；MCP server 条目标注进程归属未定（[`01-agent-runtime` §5](./prd/01-agent-runtime.md) R6）。新增 `声明合计` 与 `日志分级` 两个词条。**可插拔后端**词条删掉「用户自备 API key」并注明它是本机概念、不是远程服务端（[ADR-0003 §4](./adr/0003-agent-runtime-and-pluggable-backend.md) 同日修订）。**词义未变** |
| v0.2 | 2026-08-08 | 公开仓库去个人化：`owner` 改为 `@maintainer`。**术语与定义未变** |
| v0.1 | 2026-08-06 | 初版术语表 |
