---
title: 07 评测 Eval — 解析质量的评测集、评分器、回归门槛与夹具
status: draft
owner: "@alex"
date: 2026-08-08
version: v0.2
---

# 07 · 评测 Eval

> 回答一个 `docs/PRD.md` 认定为生死攸关、却此前无人负责的问题：**agent 读得准不准，怎么知道？**
> 依据：[ADR-0002 AI 永不直接写入](../adr/0002-ai-never-writes-directly.md)、[ADR-0007 本地可观测性与日志分级](../adr/0007-local-observability-and-log-tiers.md)、[`docs/PRD.md` §9.1](../PRD.md)。

## 1. 问题

[`docs/PRD.md` §9.1](../PRD.md) 说 M0 优先，因为它验掉产品最大的未知数——**视觉模型读真实账单准不准**（M0 起还加上第二个：一段口述能不能可靠拆成多笔）。

但在本文建立之前，**没有任何文档负责这个问题**。七份 sub-PRD 里最靠近生死线的那件事，连规格都没有。后果是：

1. 「读得准」没有定义。金额差一分算不算错？商户名少个门店号算不算错？没有判据，就没有争论的基础
2. 模型或提示词一改，**无从判断是变好还是变坏**
3. 闸门有没有真的在拦——`verify-m0.mjs`（[`docs/PRD.md` §9.3](../PRD.md)）验的是「一次跑通」，不是「持续没坏」

本文要把这三件事变成可以跑的命令。

## 2. 范围与非目标

**范围**：eval 走哪条路径 · 评分器定义（什么算读对）· eval 集从哪来与怎么增长 · 回归判定方式 · 夹具导出与重放 · 本机数据与 CI 数据的分离。

**非目标**：

- **提示词优化本身**——本文提供度量，不规定怎么改提示词
- **模型选择**——后端可插拔属 [01 Agent 运行时](./01-agent-runtime.md)
- **UI**：eval 结果不做进产品界面。用户关心审核界面快不快，不关心准确率数字
- **通用 eval 框架**——见 §4 否决方案
- **人工标注流程**：我们的标注是审核界面的副产品（§3.2），不需要独立标注工序

## 3. 决定与依据

### 3.1 eval 走生产同一条路径

**决定：eval 调用的是我们自己的 Rust 侧**——起 MCP server、spawn `claude -p`、落进临时数据目录、然后查表打分。**不直接调 Anthropic API。**

理由：直接调 API 测的是**另一个系统**——没有我们的工具面、没有提示词模板、没有闸门。跑绿了不能说明产品是对的。而 `PRD §9.1` 真正要验的不是「模型能不能认出数字」，是**整条链路能不能产出可信的草稿**。

代价诚实登记：**每跑一轮 eval 就烧一轮订阅额度**，与做一次真实导入同价。20 个用例 ≈ 20 次导入。额度是 [`docs/PRD.md` §12](../PRD.md) 登记的真实约束，所以 eval **不进 CI、不自动触发**，只在改提示词、换后端、发版前手动跑。

### 3.2 eval 集是现有三张表的一个视图，不新建数据

**审核界面里用户的每一次纠正，天然就是一条标注好的样本。** 三要素已经全在库里：

| eval 三要素 | 出处 |
|---|---|
| 输入 | `sources.evidence_relpath`（截图原件）或 `sources` 上的转写文本（`kind = utterance`） |
| agent 的输出 | `draft_transactions` —— 确认后**标记已消费而非删除**（[03 审核 §3.1](./03-review.md)），原始起草值原样保留 |
| 正确答案 | `transactions` —— 人确认过的那一行，经 `source_draft_id` 指回原草稿 |

三者一 join，差异就是 agent 犯的错。**eval 集不需要新建任何数据结构。**

`draft_transactions.backend_id` / `model_id`（[00 地基 §3.6](./00-foundation.md)）是这套机制的必要条件：不记的话，模型一升级就无法解释基线变化是模型变了还是提示词变了。

**纠正数据一份三用**：记忆规则（[06 记忆](./06-memory.md)）、审计留痕（[ADR-0002](../adr/0002-ai-never-writes-directly.md) 闸门 4）、eval 样本。三者读同一批数据。

### 3.3 评分：outcome 与 transcript 两个维度

依据 [Anthropic《Demystifying evals for AI agents》](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents) 的建议——同时评「终态对不对」与「它是怎么走到那儿的」。

**维度一：outcome（草稿字段对不对）**

| 字段 | 判据 | 类型 |
|---|---|---|
| `amount_minor` | **精确相等**，无容差 | 代码型 |
| `currency` | 精确相等 | 代码型 |
| `occurred_on` | 精确相等 | 代码型 |
| `direction` | 精确相等 | 代码型 |
| 条目数 | 与人确认后的条数相等（多读、漏读都算错） | 代码型 |
| `merchant` | **待决**，见 §5 R1 | ？ |
| `category` | 不计入 M0/M1 的分数——它由记忆规则演进，不是解析能力 | — |

**维度二：transcript（agent 有没有守规矩）**

| 检查 | 判据 | 类型 |
|---|---|---|
| 每条草稿都有 `evidence_text` | 非空，且是输入中真实出现过的子串 | 代码型 |
| `report_source_total` 是**抄的**不是**算的** | 声明合计 ≠ 逐笔之和时，说明它抄了账单（这是好事）；恒等于逐笔之和是**可疑信号**（见 [01 §3.2](./01-agent-runtime.md) 可信性要求） | 代码型 |
| 没有越权工具调用 | `audit_log` 里 `actor = "agent"` 的记录只触及草稿表与 `declared_total_*` | 代码型 |
| 事项内容被明确拒绝（M0） | `kind = utterance` 且含事项类内容时，agent 明说「记不了」而非静默丢弃（[`docs/PRD.md` §9.2](../PRD.md)） | 代码型 |

**几乎全是代码型评分器。** 金额是整数、精确相等；闸门合规是查审计表。**不用 LLM-judge，因此不需要校准 judge**——省掉了 eval 里最麻烦的一层。这是「金额一律整数」（[ADR-0004](../adr/0004-data-model-sqlite-integer-money.md)）的一个未预期红利。

### 3.4 20 个用例起步

Anthropic 的建议是「20–50 个来自真实失败的用例是很好的起点」，且早期 agent 用小集合就够——每次改动的影响很明显。

**决定：20 个起步。** 增长方式是被动的：dogfooding 期间每次在审核界面改一条，就多一条候选样本。定期从 §3.2 的 join 里挑进 eval 集，**优先挑改动幅度大的和被改过两次以上的**。

**不追求覆盖率指标。** 20 个用例覆盖不了长尾，也不该假装能。它的作用是**回归探针**，不是质量证书。

### 3.5 回归判定：逐条对比，不用百分比门槛

**N = 20 时，单条用例 = 5 个百分点。** 设「准确率不得低于 85%」这类门槛在这个规模下是噪声。

**决定：逐条对比上一轮结果。任何一条从「过」变「不过」都必须人看一眼**，不自动放行也不自动拦截。eval 脚本输出的是一张 diff 表（哪条变了、变成什么），不是一个分数。

同时输出模型标识与后端标识——否则无法区分「模型退步了」和「我改坏了提示词」。

### 3.6 夹具与重放：把 eval 与回归拆成两种成本

**agent 是非确定性的**，所以「复现一个 bug」不能是「重新跑一次 agent」。必须重放那次录下来的工具调用序列（依赖 `debug` 级日志，见 [ADR-0007](../adr/0007-local-observability-and-log-tiers.md)）。

**夹具导出器**：`node scripts/export-fixture.mjs <agent_session_id>`（**待建**）把散落三处的数据打包成一个自包含目录：

```
fixtures/local/<date>-<slug>/
├── input.png | input.txt   ← 截图原件，或 utterance 的转写文本
├── tool-calls.json         ← agent 那次调了哪些工具、每次的完整参数
└── expected.json           ← 人确认后的正确结果
```

**导出器一律写进 `fixtures/local/`**，因为它导出的是真实数据。要进 CI 得先脱敏并移入 `fixtures/ci/`——目录划分见 §3.7。

**重放时跳过 `claude -p`**，直接把 `tool-calls.json` 喂进系统。因此它测的**不是模型**，是——

> **当 agent 读错时，我们的代码有没有拦住。**

一条「把 168 读成 1680」的夹具，断言是「总额交叉校验必须报警、批量确认必须被拒」。谁把闸门改坏了，这条夹具立刻变红。

于是两件事分开了：

| | 测什么 | 怎么跑 | 成本 | 进 CI |
|---|---|---|---|---|
| **eval**（§3.1–3.5） | 模型读得准不准 | 真调 `claude -p` | 烧额度 | ❌ |
| **回归**（本节） | 代码改了会不会挂 | 重放夹具 | 零额度、确定性 | ✅ |

**写测试这一步不自动化**（见 §4）。导出器的产物是可重放的夹具；基于夹具写 `cargo test` 交给 agent 做。

### 3.7 本机数据与 CI 数据必须分离

夹具与 eval 集里是**真实截图和真实金额**。

- **本机集**：随自用积累而增长，**不进 git**
- **CI 集**：手工**脱敏或合成**的一小撮，进仓库，只用于重放回归
- **两套不得混用。** 任何让真实账目进入 git 的路径都是缺陷

**分离靠目录，不靠自觉**——两套数据放同一个目录、指望提交时挑，迟早会漏。`.gitignore` 里的对应物：

```gitignore
/fixtures/local/     ← 本机集，绝不进 git
!/fixtures/ci/       ← CI 集，显式反挡，必须能提交
```

`/samples/` 同理已挡。**判据**：`git status` 里永远不该出现 `fixtures/local/` 下的任何文件；出现了就是 `.gitignore` 被改坏了。

## 4. 否决的替代方案

| 方案 | 否决原因 |
|---|---|
| 直接调 Anthropic API 做 eval | 测的是另一个系统（无工具面、无提示词模板、无闸门）。跑绿了不说明产品对——见 §3.1 |
| 引入 [promptfoo](https://www.promptfoo.dev/docs/faq/) | 它的隐私姿态确实吻合（本地跑、结果本地存、遥测可关），但在 §3.1 的路径下只能当 exec provider 的壳；我们真正要的（读 SQLite 做 join、查 `audit_log` 验闸门）它帮不上，反而多一层依赖与供应链面 |
| DeepEval | 强项是模型评分器，而我们几乎全是代码型评分器（§3.3） |
| Braintrust / Arize 等云平台 | **直接违反 [`CLAUDE.md`](../../CLAUDE.md) 约束 2**（无云服务、无遥测） |
| OpenAI Evals | 只支持 OpenAI API |
| 百分比准确率门槛 | N = 20 时单条 = 5 个百分点，门槛是噪声。逐条 diff 更有信息量（§3.5） |
| 自动生成测试代码 | 单人项目里是负资产——会得到一堆没人读过的测试。难点在「让 bug 可复现」，不在「写测试」 |
| eval 进 CI 自动跑 | 每次都烧订阅额度，且 CI 环境没有已登录的 agent CLI |
| 把 eval 面板做进产品 | 用户关心审核界面快不快，不关心准确率数字 |

## 5. 待决与风险

| # | 事项 | 影响 | 谁来决 / 何时 |
|---|---|---|---|
| R1 | **`merchant` 怎么算对**——银行流水的商户文本常带门店号与流水号（`WOOLWORTHS 1234 SYDNEY`）。精确相等太严，模糊匹配又需要定阈值 | 本文 §3.3 | 拿到真实商户文本样本后决（M1 前）。倾向：**只评「是否从原文中正确抽取」，不评归一化**——归一化归 [06 记忆](./06-memory.md) |
| R2 | 20 个用例对「一段口述拆多笔」够不够——口述的变化面（语序、省略、口语数字）可能比截图更宽 | 本文 §3.4 | M0 实测后调，**结果回流本文** |
| R3 | eval 一轮的额度成本未实测。若 20 用例一轮就吃掉半个月配额，跑的频率会被迫压低 | 本文 §3.1 | M0 跑第一轮后记录实际消耗，**回流本文** |
| R4 | 夹具重放要求 `tool-calls.json` 的格式与工具面同步演进；工具签名一改，旧夹具可能失效 | 本文 §3.6 | 首次出现失效夹具时定版本化策略 |
| R5 | 脱敏 CI 集怎么造——手工改数字容易造出不自洽的样本（合计对不上） | 本文 §3.7 | 建 CI 集时决；倾向用**合成**而非脱敏真实样本 |
| R6 | 低置信标注（`draft_transactions.confidence`）依赖 agent 自评，其校准度未知（同 [03 审核 §5](./03-review.md) R5）。若不可靠，eval 无法用它做分层分析 | 本文 §3.3 | M1 实测 |

## 6. 验收标准

- [ ] `cargo fmt --all -- --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test` 全绿
- [ ] `node scripts/eval.mjs --dry-run`（**待建**）退出码 0——在不调用 agent 的情况下校验 eval 集完整性（每个用例都有输入、agent 输出、期望结果三要素）
- [ ] `node scripts/eval.mjs`（**待建**）跑完 20 用例并输出逐条 diff 表，表内含**每条的模型标识与后端标识**
- [ ] `node scripts/eval.mjs` 在检测不到可用 agent CLI 时**非零退出**并明确报原因，**不静默降级为通过**
- [ ] `node scripts/export-fixture.mjs <session_id>`（**待建**）产出的目录自包含：不引用数据库、不引用 `evidence/`，换一台机器解压即可重放
- [ ] `cargo test eval::replay_fixture_catches_total_mismatch` 通过——重放一条「金额读错」夹具，总额校验报 `review.total_mismatch` 且 `transactions` 保持为空
- [ ] `cargo test eval::replay_does_not_invoke_agent` 通过——重放路径上没有 spawn 子进程（`AgentBackend::spawn` 调用次数为 0）
- [ ] `cargo test eval::evidence_text_is_substring_of_input` 通过——每条草稿的 `evidence_text` 必须是输入文本/OCR 结果的真实子串（transcript 维度）
- [ ] `rg -n 'fixtures/' .gitignore` 有命中——本机夹具不进 git
- [ ] `git ls-files fixtures/ | xargs -r rg -l '[0-9]{4,}'` 人工过一遍——仓库内 CI 夹具不含真实金额

**人工验收**：

- [ ] 改一次提示词，跑一轮 eval，能从 diff 表看出「哪几条变了」而不是只看到一个分数
- [ ] 随便挑一条历史 bug，用导出器导出夹具，`cargo test` 能稳定复现（连跑三次结果一致）

## 7. 回流记录

- **2026-08-08 · 夹具目录定为 `fixtures/local/` 与 `fixtures/ci/` 两支**（§3.6、§3.7）。
  起因是建 CI 门禁时要往 `.gitignore` 加夹具规则，发现 §3.7 只说了「本机集不进 git、CI 集进仓库」，**没说这两套怎么在文件系统上分开**——而一条 `.gitignore` 规则必须落到具体路径。
  沿用 §3.7 原有的「两套不得混用」，把它实现为目录划分：`/fixtures/local/` 被 ignore，`!/fixtures/ci/` 显式反挡。§3.6 的导出器输出路径随之从 `fixtures/<date>-<slug>/` 改为 `fixtures/local/<date>-<slug>/`——导出器碰的一定是真实数据，默认落点就该在不进 git 的那一支。

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.1 | 2026-08-08 | 初版，出自 2026-08-08 设计评审（`/grill-with-docs` 会话）。此前七份 sub-PRD 里**没有任何一份负责「agent 读得准不准」**，而 [`docs/PRD.md` §9.1](../PRD.md) 认定它是生死线。确立：eval 走生产同一条路径（不直接调 API）· eval 集是现有三表 join 的视图（不新建数据）· outcome + transcript 两个维度、几乎全代码型评分器 · 20 用例起步 · 逐条 diff 而非百分比门槛 · 夹具重放把 eval（烧额度、不进 CI）与回归（零额度、进 CI）拆开 · 本机真实数据与 CI 脱敏数据严格分离。否决方案九条，待决 R1–R6 |
| v0.2 | 2026-08-08 | 夹具目录落为 `fixtures/local/`（不进 git）与 `fixtures/ci/`（进仓库）两支，§3.7 补 `.gitignore` 对应写法与判据，§3.6 导出路径随之改到 `local/` 一支。起因见「7. 回流记录」——建 CI 门禁时发现 §3.7 只说了两套数据要分离、没说怎么分 |
