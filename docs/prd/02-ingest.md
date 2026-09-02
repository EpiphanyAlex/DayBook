---
title: 02 导入 Ingest — 截图导入、来源落库与解析编排
status: review
owner: "@maintainer"
date: 2026-08-30
version: v0.17
---

# 02 · 导入 Ingest

> 把用户手里的痕迹（截图、文件）变成系统里的**来源**（`source`），并编排 agent 对它的解析。
> 这是「考古学家」拿到铲子的地方——[03 审核与草稿区](./03-review.md) 之前的一切。

## 1. 问题

用户的真实动作是：过去一两周里，手机上攒了一堆截图（网银流水、信用卡账单、各类第三方支付账单），现在坐下来一次性处理。**来源格式是任意的**——产品不为任何特定银行或支付平台写解析器（[`docs/PRD.md` §3.1](../PRD.md)）。

系统要解决三件事：

1. **把文件变成有身份的来源**——同一张截图导入两次不应产生两条记录，否则审核界面会出现整批重复。
2. **保住证据原件**——[ADR-0002](../adr/0002-ai-never-writes-directly.md) 的证据链要求审核时能看到原图；原图丢了，草稿就失去了可核对性。
3. **编排解析**——什么时候叫 agent、叫几次、失败了怎么办。**这是确定性代码的活，不是 agent 自己判断的**（[ADR-0003 §5](../adr/0003-agent-runtime-and-pluggable-backend.md)）。

## 2. 范围与非目标

**范围**：拖拽导入 · 文件落进证据目录 · `sources` 落库与幂等 · 解析任务编排与状态机 · 跨图去重 · 批量导入 · 失败与重试。

**非目标**：

- **视觉解析本身**——由 agent 做，工具面属 [01 Agent 运行时](./01-agent-runtime.md)
- **草稿的展示与确认**——属 [03 审核与草稿区](./03-review.md)
- **照片库读取（PhotoKit）**——v1 不做，用户拖拽；v1.1 由 sidecar 提供（[ADR-0005](../adr/0005-voice-and-system-integration.md)）
- **弱信号采集**（日历/git/浏览器历史/屏幕使用时间）——[`docs/PRD.md` §6](../PRD.md) 明确非目标
- **历史数据导入**（从别的记账软件迁移）——[`docs/PRD.md` §6](../PRD.md) 明确非目标

## 3. 决定与依据

### 3.1 v1 的导入方式：拖拽

- 拖文件进应用窗口，支持一次拖多个
- **不做**「自动监听某个文件夹」——那会引入「用户不知道应用在读什么」的信任问题，与产品的隐私姿态相悖
- **不做**照片库直读（[ADR-0005](../adr/0005-voice-and-system-integration.md) v1 零 Swift）

**支持格式按里程碑分层**（2026-08-07 M0 开工评审细化）：

| 格式 | 里程碑 | 说明 |
|---|---|---|
| PNG · JPEG | **M0** | iPhone / macOS 截图的默认格式，覆盖 M0 的全部验证场景 |
| HEIC | M2 | 相机照片格式；是否需 Rust 侧解码取决于后端 CLI 支持，见 §5 R3 |
| PDF | M2 | 多页切分策略未定，见 §5 R1——**策略未定就支持等于让实现者自己发明** |

格式不在当前里程碑支持集内 → 返回 `ingest.unsupported_format`，UI 明确告知，**不静默忽略**。

#### 第二种来源：`utterance`（2026-08-08 设计评审，M0）

除拖拽文件外，**一段口述或文字也是来源**（`sources.kind = utterance`，见 [00 地基 §3.6](./00-foundation.md)「来源不等于文件」）。依据 [`docs/PRD.md` §1.1](../PRD.md)：录入摩擦是痛点的另一半，而我们的解法是「说一段话 → agent 拆成多笔 → 一次批量确认」，不是「更快的表单」。

- 输入是一个多行文本框。**语音由 macOS 系统听写完成**（用户在框内连按两下 `Fn`），应用零代码、音频不出本机（[ADR-0005 §1](../adr/0005-voice-and-system-integration.md)）
- **转写文本落盘成 `.txt`**，与截图同等对待 → `evidence_relpath` 非空，闸门 2 的实现路径对两种来源完全一致
- 每条草稿的 `evidence_text` 是**这段话里对应的那个片段**（「今天吃饭 180」这半句），不是整段
- **幂等靠一次提交一个令牌，不靠内容哈希**（2026-08-10 改定，见 §3.2）：隔天再说同样一句话是**新的一笔**，不是重复
- **`reported_total_*` 通常为空** → 对账结果 **`not_applicable`**。只有口述中恰有一条覆盖整段全部适用交易、且 amount/currency/kind 三元组不与 invalid decoy 重复的来源级 claim 时才照常对账，结果为 `passed` / `failed`；月度外部范围、按日、单笔或子组合计不得报告，合计词只是候选（[00 地基 §3.6](./00-foundation.md)「M0 单 claim 的范围资格」）。**两种情况下确认策略都是 `user_attested_batch`**——闸门 3 之外另有「整段原文 + 全部拆分结果并排 + 一次人工确认」那道（[03 审核 §3.3](./03-review.md)）；`03` 仍会把这类草稿在异常前置里单独提一档
- **M0 只记交易**：`draft_items` 是 M3 的表，所以 agent 遇到事项类内容（「明天交房租」）**必须明确回一句「这条我现在还记不了」**，不得静默丢弃（[`docs/PRD.md` §9.2](../PRD.md)）

### 3.2 来源身份与幂等

**两种来源，两把幂等键**（2026-08-10 改定，[00 地基 §3.6](./00-foundation.md)「口述的幂等键不是内容哈希」）：

| `kind` | 幂等键 | 重复的含义 |
|---|---|---|
| `file` | **内容 SHA-256**（`content_hash`，部分唯一索引） | 同一张图导两次确实是同一份证据 |
| `utterance` | **一次提交一个令牌**（`idempotency_key`，前端生成） | 同一次提交的重试 / 双击 / 崩溃重放 |

- 文件导入时计算内容的 **SHA-256**，存 `sources.content_hash`
- **文件的幂等以内容为准，不以文件名为准**——用户从不同 app 导出的同一张图文件名会不同
- **口述不能用内容哈希去重。** 本节 v0.4–v0.5 曾把「同一段话说两次判为重复」写成刻意设计，理由是「用户重说一遍通常意味着他以为上次没记上」。**那个理由只在几分钟的尺度上成立**：连续两天各说一句「今天咖啡 5 元」文本逐字相同 → 第二笔被判重复 → **一笔真实交易静默消失**，而用户看到的提示是「这段已经导入过」，他不会去追。**丢一笔真实交易，比多一批可以一键丢弃的草稿严重得多**
- **文本重复只提示，不阻止**：新来源与既有 `utterance` 文本相同时，UI 提示「你之前也说过同样的话」并列出那一条，**由用户决定**（判断留给人，[ADR-0003 §5](../adr/0003-agent-runtime-and-pluggable-backend.md)）

**重复导入的返回契约**（2026-08-07 M0 开工评审补定——原文只说「返回已存在的 `source_id` 并在 UI 告知」，没说是成功还是错误，前后端会各按各的理解写）：

- **成功返回**，不是错误。命中重复不是失败，用户拖了一张已有的图是完全正常的操作
- 返回体带 `{ source_id, deduplicated: true }`；新建时 `deduplicated: false`
- **不新建记录、不重复触发解析、不重复落盘**
- UI 依 `deduplicated` 字段提示「这张已经导入过」

> `ingest.duplicate_source`（[00 地基 §3.7](./00-foundation.md)）**保留但不用于此路径**——它只在需要以错误形式表达重复的场景使用（如未来的严格导入模式）。M0 的拖拽导入走上面的成功返回。

### 3.3 证据文件落盘

- 原件复制进 `<数据目录>/evidence/<yyyy>/<mm>/<source_id>.<ext>`（布局见 [00 地基 §3.2](./00-foundation.md)）
- **先落盘、后写库**：文件写成功才插 `sources` 行。反过来会产生「库里有记录但证据不存在」的悬空引用，那会直接破坏 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 的证据链
- **不修改原件**（不压缩、不转格式、不去 EXIF）——证据必须与用户手里的那张逐位一致
- 用户原始文件**不删除、不移动**

### 3.4 来源状态机

```
imported ──▶ parsing ──▶ parsed ──▶ reviewed
                │           │          ▲
                │           └──────────┘（重新解析：parsed 也能回到 parsing）
                └──▶ failed ───────────┘（重试后可回到 parsing）
```

| 状态 | 含义 |
|---|---|
| `imported` | 文件已落盘、已入库，尚未解析 |
| `parsing` | agent 子进程正在处理 |
| `parsed` | **agent 调过 `complete_source` 且进程正常退出**，草稿已生成，等待人工审核 |
| `failed` | 解析失败 / 超时 / 取消 / 中断 / **协议失败**；**该次尝试的草稿已全部作废**（[01 Agent 运行时 §3.4](./01-agent-runtime.md) 的补偿性作废，置 `voided_at` 不删行）。失败原因记在 `sources.parse_error_code` |
| `reviewed` | 该来源当前尝试的全部未作废草稿都已被确认（`consumed_at`）或由人丢弃（`discarded_at`） |

> **`parsed` 的判据由「退出码 0」改为「调过 `complete_source`」**（2026-08-10，[01 Agent 运行时 §3.2](./01-agent-runtime.md)）。旧判据下，agent 读了 12 笔里的 9 笔然后正常收工，退出码同样是 0——**静默漏读被判为解析成功**。现在这种情况走 `failed` + `agent.protocol_violation`：我们不知道它是读完了还是走了一半，而**「不知道」不能算通过**。

**取值集与字段**：本表即 `sources.state` 的权威取值集，字段定义在 [00 地基 §3.6](./00-foundation.md)。

**状态转移由 Rust 侧代码执行，agent 无法改状态**——工具面里没有改状态的工具，且 agent 对 `sources` 无任何写入权限，对 `parse_attempts` 也只收窄到 `reported_total_*` 等六列（[00 地基 §3.6](./00-foundation.md)「列级写入权限」、[01 Agent 运行时 §3.2](./01-agent-runtime.md)）。非法转移返回 `ingest.invalid_state_transition`。

**转移表只有一份，且必须只有一处代码在写 `sources.state`**（2026-08-13 实施回流）。M0 曾同时存在两份：本节对应的那份有穷举 25 种转移的测试**却没有任何生产代码调用**，而真正生效的是 `agent/runtime.rs` 里内联的一个 `matches!`——**两份对 `parsed → parsing` 的答案还相反**。测试全绿，因为它测的是没人用的那份。判据写成可执行的：**除状态机自身所在模块外，生产代码里不得出现第二处 `UPDATE sources SET state`**（§6 验收）。

两条补进转移表的边：

- **`parsed → parsing`**（重新解析）：[03 审核 §6](./03-review.md) 的 `total_check_is_scoped_to_attempt` 要求「同一来源成功解析两次、两次草稿都未作废」，所以它本来就必须合法；旧表漏了这一条，而内联那份允许它——**规格与实现分歧时是规格错**。
- **`failed → failed`**（幂等重入）：作废是补偿动作，会被触发两次——工具面先 `fail_protocol` 一次，runtime 收到协议失败后还会兜底再作废一次（[01 §3.4](./01-agent-runtime.md)）。把它判为非法转移会让第二次作废报错。

`reviewed` 是终态，不回到 `parsing`：那批草稿已经入库或被丢弃，再解析一次只会产生与事实行竞争的第二批草稿。

#### 启动时的崩溃恢复扫描（2026-08-08 设计评审新增）

**缺口**：解析任务此前只活在内存里。应用在解析途中崩溃或被 Ctrl-C，`sources.state` 停在 `parsing`，而**重启后没有任何东西会去碰它**——那条来源永久卡死，UI 上既不是待解析也不是失败。**M0 就会遇到，Ctrl-C 一次即复现。**

**原则：任务恢复不依赖内存中的 Promise**——应用重启后，未完成的任务必须能从持久化状态里重新认出来。完整做法是持久化任务队列；v1 规模下不需要那么重，一次启动扫描就够：

- 启动时扫描 `state = parsing` 的全部来源，**以及 `ended_at` 为空的全部 `parse_attempts`**（2026-08-10 补：两处都要扫，只扫来源会漏掉「尝试行已插入但来源状态还没推进」的那一瞬）
- **v1 同时只跑一个 agent 子进程**（§3.5），所以启动那一刻**不可能有活着的解析** —— 这些必然是上次崩溃的残留
- 来源全部转 `failed`，`parse_error_code = agent.interrupted`；对应的 `parse_attempts` 回填 `ended_at` 与 `outcome = interrupted`
- 按 [01 Agent 运行时 §3.4](./01-agent-runtime.md) 的补偿逻辑作废其草稿（置 `voided_at`），并写 `actor = "system"` / `action = "void"` 的审计
- 扫描发生在**任何窗口出现之前**，用户看到的第一屏就已经是干净状态

### 3.5 解析编排

- 首次解析前必须已有用户明确选择的本位币；没有则返回 `data.base_currency_required`，来源保留在 `imported`，不创建尝试、不消耗 agent 额度（[00 地基 §3.4](./00-foundation.md)）
- 一次导入 N 个文件 → 生成 N 个解析任务，**串行执行**（v1 同时只跑一个 agent 子进程，见 [01 Agent 运行时 §3.4](./01-agent-runtime.md)）
- **不预先注入记忆规则**（2026-08-08 改定，[06 记忆 §3.4](./06-memory.md) R1 关闭 · `architecture` A3 关闭）：agent 解析出商户后**自己调 `query_memory` 批量查**。代码侧不做上下文装配，也不在起草后改写分类
- 解析后由代码触发**总额交叉校验**（[03 审核与草稿区](./03-review.md) 的职责），**入参是本次的 `attempt_id`**——合计与草稿都属于产出它们的那次尝试（2026-08-10 改，此前写「结果落在 `sources` 上」）
- **失败不静默**：`failed` 的来源在 UI 上显式列出，附失败原因（`parse_error_code`），可一键重试
- **v1 不做自动重试**（2026-08-07 评审，[01 Agent 运行时 §5](./01-agent-runtime.md) R2 关闭）：重试由用户在 UI 上显式触发。自动重试会在用户不知情时二次消耗 AI 额度，而额度是真实约束（[`docs/PRD.md` §12](../PRD.md)）

#### 什么时候允许自动开始解析（**M1**，2026-08-24 产品决定）

这一节存在的理由是本文里有两条看起来矛盾的决定：**自动重试被否决、文件夹监听被否决，而「丢进来就自动整理」被批准。** 三条用的是同一条判据，不是三次拍脑袋：

> **消耗 agent 额度必须由一个明确的、用户当场知道自己做了的动作触发。**

| | 有没有那个动作 | 结论 |
|---|---|---|
| 拖入一个来源 → 立即解析 | **有**——拖入这个动作本身就是「请把它整理掉」，且用户此刻正看着屏幕 | **允许**，默认开 |
| 解析失败后自动重试 | **没有**——失败发生在用户已经走开之后，第二次消耗他不知道 | 否决（上一条） |
| 监听文件夹自动导入 | **没有**——用户根本没做动作，也无法预期什么时候会烧额度 | 否决（§4） |

具体规则：

- **默认开启。** 设置项「丢进来就自动整理」，注明「会用掉你已登录的 agent CLI 的额度」
- **一次拖入 1 个来源 → 直接开始解析**，零点击。这是主路径
- **一次拖入 ≥ 2 个来源 → 不自动开始**：先把它们列出来（含每个来源的缩略图 / 转写首行），由用户点「开始整理（N 个来源）」。**理由是判据本身**——一个拖放动作要花掉 N 次额度时，用户在动作发生的瞬间并不知道 N 是多少，「知道自己做了什么」这一半不再成立
- 关掉该开关后，**单个与批量都需要显式点一下**；解析编排的其余部分（串行、记忆不预注入、总额校验、失败不静默）不因这个开关而变
- 本位币未设置时仍按上面第一条走：返回 `data.base_currency_required`，**不创建尝试、不消耗额度**，开关为开也不例外

**这个开关不改变任何闸门。** 自动开始解析改的只是「谁点了那一下」，agent 产出的仍然只是草稿，仍然要人确认（[ADR-0002](../adr/0002-ai-never-writes-directly.md)）。

> **M0 现状：没有这个开关，解析一律显式触发。** 本小节是 M1 范围，不构成对已通过的 M0 验收的追认。

### 3.5.1 降级与失败态矩阵（2026-08-10 新增）

**这张表存在的理由**：这些状态此前**每一条都有归属，但没有一处能一起看到**——散在 [01 §3.4](./01-agent-runtime.md)、[01 §3.5](./01-agent-runtime.md)、本文 §3.2/§3.4/§3.5、[03 审核 §3.3](./03-review.md) 六个地方。实现时最容易漏的不是某一条，是**没意识到还有第七条**。本表是索引，判据以各自出处为准。

| 用户遇到的情况 | 判定处 | 错误码 / 状态 | 应用行为 |
|---|---|---|---|
| 未发现合格 agent CLI（未找到候选、不可执行或版本不可读取） | 安装资格检查（[01 §3.5](./01-agent-runtime.md)） | `agent.backend_unavailable` + 稳定的 `availability_reason` | **应用照常启动**，`ready = false`；按原因给安装或修复指引，手工录入可用（[04 §3.5](./04-transactions.md)） |
| CLI 合格但 readiness probe 尚未完成 | 主动就绪探测（[01 §3.5](./01-agent-runtime.md)） | 状态本身非错误；`ready = false`。**用户在这个窗口里显式点解析，命令层返回 `agent.not_ready`** | 显示「正在检查」，**不创建尝试、不下发解析** |
| CLI 装了但没登录 | readiness probe | `agent.not_authenticated` | 应用照常启动，但指引是**去登录**而不是去安装；不下发解析 |
| capability manifest 无法证明与预期严格相等 | readiness probe 的密封比较（[01 §3.7](./01-agent-runtime.md)） | `agent.tool_surface_unsealed` | `ready = false`，**拒绝下发任务**，不降级运行 |
| readiness probe 的其他失败（helper 无法启动、探测超时、额度或网络暂不可用） | readiness probe（[01 §3.5](./01-agent-runtime.md)） | 对应 `agent.spawn_failed` / `agent.timeout` / `agent.quota_exhausted` 等 | 保留安装事实，`ready = false`；**不创建尝试、不改变来源状态、不下发解析**，UI 给对应动作并允许用户显式重探测 |
| 解析任务下发后额度耗尽 | 后端报告（[01 §3.4](./01-agent-runtime.md)） | `agent.quota_exhausted` | 来源转 `failed`，**不自动重试** |
| 解析任务超时 | 硬超时（[01 §5](./01-agent-runtime.md) R1） | `agent.timeout` | 来源转 `failed`，草稿作废，UI 可一键重试 |
| 用户点停止 | `cancel`（[01 §3.4](./01-agent-runtime.md)） | `agent.cancelled` | 同步 kill，草稿作废 |
| 应用崩溃 / 强杀 | 启动扫描（§3.4） | `agent.interrupted` | 下次启动时清理，第一屏就是干净的 |
| **读了一半就收工** | 未调 `complete_source`（[01 §3.2](./01-agent-runtime.md)） | `agent.protocol_violation` | 来源转 `failed`，**不判为 `parsed`** |
| **自报条目数对不上** | `complete_source`（[01 §3.2](./01-agent-runtime.md)） | `agent.completion_mismatch` | **可补救**：会话不封闭，agent 补齐后重调；再次不符才判协议失败 |
| **agent 说有一块没读** | `unparsed_note` 非空 | `outcome = completed_with_gaps` | 来源仍转 `parsed`，但审核界面**显眼提示**，不与普通成功同貌 |
| 记忆键没查全（**M3**） | `complete_source`（[06 §3.4](./06-memory.md)） | `agent.memory_lookup_incomplete` | 同上，可补救：返回缺的键，agent 补查后重调 |
| 部分成功（批量中某张失败） | 队列（§3.7） | 该来源 `failed`，其余照常 | 队列不中断 |
| 重复来源（文件） | 内容哈希（§3.2） | 非错误，`deduplicated: true` | 成功返回，UI 提示「已导入过」 |
| 重复文本（口述） | 文本比对（§3.2） | **非错误，也不去重** | 提示「你之前也说过同样的话」，**由用户决定** |
| 格式不支持 | 格式集（§3.1） | `ingest.unsupported_format` | 不落盘不写库，UI 明说 |
| 文件来源没有 scope-valid 合计 / 合计取不到 | 总额校验（[03 §3.3](./03-review.md)） | `unavailable` + `single_only` | 批量确认被拒，逐条可用 |
| 口述来源（没有 scope-valid 来源级合计） | 总额校验 | `not_applicable` + **`user_attested_batch`** | **批量确认可用**（三条 UI 前提，[03 §3.3](./03-review.md)） |
| 缺汇率 | 确认时校验（[04 §3.2](./04-transactions.md)） | `review.incomplete_triple` | 该条不入库，其余可入 |
| 文件来源合计对不上 | 总额校验 | `failed` + `single_only` → `review.total_mismatch` | 批量被拒，逐条可用 |
| 口述来源的 scope-valid 合计对不上 | 总额校验 | `failed` + **`user_attested_batch`** → `review.total_mismatch` | 异常前置并警告；仍可在三条 UI 前提下由人对整段原文背书后批量确认 |
| 误确认了一条 | 软删除（[04 §3.5](./04-transactions.md)） | — | 删除是软删除并写审计 |

> **一条贯穿全表的原则**：**每一格都有一个用户看得懂的说明和一个可做的动作。** 没有静默失败，也没有「出错了」这种说了等于没说的提示——上一节「失败不静默」只是它的一个实例，不是全部。

### 3.6 跨图去重（M2）

跨图重复的典型来源：信用卡账单截图与银行流水截图记录了同一笔消费。

- **候选判定**：同金额 + 同币种 + 日期相差 ≤ 2 天 → 标为疑似重复
- **绝不自动合并**——只在审核界面把疑似重复项**并排前置**，由用户判定（与 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 「AI 不做最终业务决策」同源）
- 用户的判定结果进 [06 记忆](./06-memory.md)（例：「银行流水与信用卡账单总是重复，优先留信用卡那条」）

### 3.7 批量导入（M2）

- 一次可拖入的文件数无硬上限，UI 显示队列与逐个进度
- 任一文件失败不中断队列
- **队列可中断**：用户点停止后，已完成的来源保留，未开始的丢弃，进行中的按超时逻辑作废其草稿

### 3.8 「整理记录」是一等对象（**M1**，2026-08-24 产品决定）

参考设计稿把左栏的「整理记录」做成能翻回去的列表（[`docs/design/README.md`](../design/README.md) 第 8 条），此前无规格出处。定义取**不需要新表**的那一种：

> **一条「整理记录」= 一个来源，加上它当前受审的那次解析尝试。**
> 数据上就是 `sources` 的一行 + `sources.latest_attempt_id` 指向的 `parse_attempts` 那一行，**不新增表、不新增分组实体**。

- 列表按 `sources.imported_at` **倒序**
- 每条显示：来源缩略图（`file`）或转写首行（`utterance`）· 导入时间 · 来源状态（§3.4 五态）· 两个计数——**「N 条待确认」**（该尝试未作废且未消费的草稿数）与**「N 条已记下」**（未作废且已消费的草稿数）
- **重试不产生第二条记录**：`latest_attempt_id` 换指向，同一条记录的计数随之变（[00 地基 §3.6](./00-foundation.md)「重试不覆盖上一次」——旧尝试的行仍在库里，只是不再是受审的那一次）

**明确不是**「一次坐下来整理的那一批」。那个读法需要一个跨来源的分组 ID，也就需要新表；而 §3.7 的批量导入本身已经是 N 个独立来源、串行解析、各自对账，没有任何规则以「这一批」为单位。**没有规则需要它，就不建它。**

#### 导航位置

整理记录列表位于**「补记」**这一屏的左栏。四个导航名定为 **补记 / 账目 / 事项 / 设置**；其中「补记」属本文，「账目」属 [04 交易](./04-transactions.md)，「事项」属 [05 事项](./05-items.md)。

> **M0 现状：三栏功能基线，没有左栏整理记录列表，也没有这四个导航名。** 本节是 M1 范围。
>
> **「设置」这一屏目前没有规格归属**，而参考设计稿的第 07 屏已经画了它（常用设定 · 类别与清单 · 解析与模型 · 数据与隐私），其内容分散在 00 / 01 / 04 / 05。本文只登记这个缺口，**不在这里替它立规格**——本文批准的「丢进来就自动整理」开关（§3.5）是设置屏上的一项，也是目前唯一一项有出处的。


## 4. 否决的替代方案

| 方案 | 否决原因 |
|---|---|
| 监听文件夹自动导入 | 「应用在后台读我的文件夹」与产品的隐私姿态相悖；且用户无法预期什么时候会烧掉 AI 额度 |
| 用文件名 + 大小做幂等键 | 同一张图从不同 app 导出文件名不同、元数据不同——内容哈希才是稳定身份 |
| 证据只存路径，不复制原件 | 用户整理相册/清空下载目录后证据链断裂，草稿变成不可核对的裸数字 |
| 导入时压缩/转格式以省空间 | 证据必须与用户手里的那张逐位一致；容量问题另有对策（[`docs/PRD.md` §13](../PRD.md) 开放问题 P3） |
| 先写库、后落盘 | 中途失败会产生「库里有记录但证据不存在」的悬空引用，直接破坏 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 证据链 |
| 自动合并跨图重复项 | 重复判定本质是业务判断（同一笔？还是真的刷了两次同样金额？），[ADR-0003 §5](../adr/0003-agent-runtime-and-pluggable-backend.md) 要求这类判断留给人 |
| 并发跑多个 agent 子进程 | v1 的额度约束下并发只会更快撞上用量上限（[`docs/PRD.md` §12](../PRD.md)）；且并发解析同一批图更难做跨图去重 |

## 5. 待决与风险

| # | 事项 | 影响 | 谁来决 / 何时 |
|---|---|---|---|
| R1 | PDF 多页账单怎么切——整份丢给 agent 还是按页切成多个来源 | 本文 §3.1/§3.5 | M2 拿到真实 PDF 账单后决，**回流本文** |
| R2 | 跨图去重的窗口（±2 天）与判定维度是否够——退款、分期、外币重复入账都可能干扰 | 本文 §3.6 | M2 实测真实 10 天数据后调，**结果回流本文** |
| R3 | HEIC 是否需要在 Rust 侧解码后再交给 agent（取决于后端 CLI 对 HEIC 的支持） | 本文 §3.1 | **改期至 M2，不阻塞 M0**（2026-08-07 评审）：HEIC 是相机照片格式，而 iPhone/macOS **截图默认是 PNG**——M0 的验证场景用不到它。M2 支持 HEIC 时实测决定 |
| R4 | 长截图（60 笔以上）的上下文隔离切法（[`docs/architecture.md` §8](../architecture.md) 未决 A1、[01 Agent 运行时 §5](./01-agent-runtime.md) R3） | 本文 §3.5 | M2，实测决定 |
| R5 | 证据目录容量增长的清理策略（[`docs/PRD.md` §13](../PRD.md) 开放问题 P3） | 本文 §3.3、[00 地基](./00-foundation.md) | 真实使用出现容量问题时 |

## 6. 验收标准

- [ ] `cargo fmt --all -- --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test` 全绿
- [ ] `cargo test ingest::idempotent_source` 通过——同一文件连导两次只产生一条 `sources` 记录，且第二次不触发解析
- [ ] `cargo test ingest::idempotent_returns_ok_with_flag` 通过——重复导入是**成功返回**且 `deduplicated == true`，首次导入 `deduplicated == false`（§3.2 返回契约）
- [ ] `cargo test ingest::idempotent_ignores_filename` 通过——同内容不同文件名视为同一来源
- [ ] `cargo test ingest::unsupported_format_rejected` 通过——M0 支持集外的格式返回 `ingest.unsupported_format`，不落盘不写库
- [ ] `cargo test ingest::evidence_written_before_row` 通过——注入「写库失败」后，不存在「有 `sources` 行但证据文件缺失」的状态
- [ ] `cargo test ingest::original_bytes_preserved` 通过——落盘文件与输入文件的 SHA-256 相等
- [ ] `cargo test ingest::state_machine_transitions` 通过——枚举全部合法/非法转移，非法转移被拒
- [ ] `cargo test ingest::agent_cannot_change_source_state` 通过——工具面中不存在改 `sources.state` 的工具
- [ ] `cargo test ingest::failed_source_has_no_active_drafts` 通过——解析失败后该次尝试不存在 `voided_at IS NULL` 的草稿，且 `parse_error_code` 非空；已作废历史行仍保留
- [ ] `cargo test ingest::agent_cannot_write_source_columns` 通过——agent 经工具面写不到 `sources` 的任何一列（合计已移到 `parse_attempts`），改 `state` / `evidence_relpath` 无路径可走
- [ ] `cargo test ingest::source_state_has_one_writer` 通过——生产代码里除状态机模块外不存在第二处 `UPDATE sources SET state`（§3.4「转移表只有一份」）
- [ ] `cargo test ingest::reparse_of_a_parsed_source_is_legal` 通过——`parsed → parsing` 合法、`reviewed → parsing` 返回 `ingest.invalid_state_transition`（§3.4 两条补进的边）
- [ ] `cargo test ingest::startup_scan_clears_stuck_parsing` 通过——预置一条 `state = parsing` 的来源后启动，扫描把它转 `failed` + `agent.interrupted` 并作废其草稿
- [ ] `cargo test ingest::utterance_source_roundtrip` 通过——投入一段文本 → `kind = utterance`、`ext = txt`、转写文本已落盘且 `evidence_relpath` 非空；无 scope-valid claim 与 current-source 全覆盖 claim 分别由 `cargo test review::utterance_yields_user_attested_batch` / `cargo test review::utterance_with_stated_total_reconciles` 覆盖（§3.1）
- [ ] `cargo test ingest::utterance_idempotent_by_token` 通过——同一段文本配**同一个** `idempotency_key` 投两次只产生一条 `sources`（`deduplicated == true`）；配**不同**令牌投两次产生**两条**，`deduplicated == false`（§3.2，**取代原先的 `utterance_idempotent_by_text`**）
- [ ] `cargo test agent::missing_complete_source_is_protocol_violation` 通过——agent 未调 `complete_source` 时来源转 `failed` + `agent.protocol_violation`，**不得为 `parsed`**（§3.4）
- [ ] `cargo test ingest::startup_scan_closes_open_attempts` 通过——预置一行 `ended_at` 为空的 `parse_attempts` 后启动，扫描把它回填为 `outcome = "interrupted"`（§3.4）
- [ ] `npm test -- ingest/batch-continues` 通过——前端逐文件编排中一个文件导入或解析失败，其余照常完成；失败按来源汇总提示
- [ ] `cargo test ingest::cross_image_dedup_candidates`（**M2**）通过——构造同金额同币种相差 1 天的两条草稿，被标为疑似重复且**未自动合并**
- [ ] `node scripts/verify-m0.mjs` 退出码 0

**人工验收**：

- [ ] 拖入一张真实的网银流水截图，能走完 `imported → parsing → parsed`，草稿数与图上条目数一致（样本取一份真实网银流水截图）
- [ ] 拖入一张已导入过的图，UI 明确提示「已导入过」而不是静默无反应

## 7. 回流记录

| 日期 | 回流内容 | 依据 |
|---|---|---|
| 2026-08-30（跨文档同步） | 口述来源的合计说明补 current-source 全覆盖 scope：关键词或单笔 / 子组「总共」不足以报告，同三元组 decoy 也拒报；降级矩阵按 kind 明列 `failed` 时 file 仍为 `single_only`、utterance 仍为 `user_attested_batch`，不改既有确认策略。本文导入边界与实现未被第一次 no-go 证伪，`status` 保持 `review`；只做共享语义同步 | [00 地基 §3.6](./00-foundation.md) v0.21；[`docs/PRD.md` §9.4](../PRD.md) 第一次正式结果 |
| 2026-08-22（跨文档同步） | §3.5.1「readiness probe 尚未完成」一行只写了「非错误」，没说用户在这个窗口里**主动**点解析时命令返回什么——落到实现上就是一个必须填的空。补 `agent.not_ready`（[00 §3.7](./00-foundation.md) v0.17 登记）：状态本身仍是非错误的中间态，这个码只属于显式发起的解析请求 | [01 §3.5](./01-agent-runtime.md) v0.23 的第四条实现边界 |
| 2026-08-17（跨文档同步） | §3.5.1 原把 `agent.backend_unavailable` 缩写成「CLI 没装」，漏掉不可执行与版本不可读取，也没有 probe 进行中的 fail-closed 状态。矩阵按 [01 Agent 运行时 §3.5](./01-agent-runtime.md) v0.21 拆为安装资格、探测中、未认证、密封失败与其他 probe 失败五类，并区分 probe 期与解析任务期复用同一错误码时的不同状态行为；应用仍可启动的既有降级原则不变 | [`docs/PRD.md` P5](../PRD.md) 部分关闭；[01 Agent 运行时 §3.5](./01-agent-runtime.md) v0.21 |
| 2026-08-13（实现验收） | **§3.4 的状态机在实现里存在两份，且互相矛盾。** 本节对应的 `SourceState::can_transition_to` 有穷举 25 种转移的测试却无生产调用点；真正生效的是 `agent/runtime.rs` 内联的 `matches!`，两份对 `parsed → parsing` 的答案相反——**测试全绿，因为它测的是没人用的那份**。改定：转移表补 `parsed → parsing`（重新解析，[03 §6](./03-review.md) 本来就要求它）与 `failed → failed`（作废是会被触发两次的补偿动作），并把「只有一处代码写 `sources.state`」写成可执行验收。§6 新增 2 条 | M0 实施验收（2026-08-13）：`rg 'transition_source\|can_transition_to'` 在生产代码里零命中 |
| 2026-08-13 | 完整 live 验收复现 Claude 对「总共」口述起草成功却漏调 `report_source_total`，使对账静默落成 `not_applicable`。口述明显合计词因此增加代码侧完成前闸门；图片不做假 OCR，仍由模型识别 | [01 Agent 运行时 §3.2](./01-agent-runtime.md) `report_source_total` 可信性要求第 6 条；`verify-m0.mjs` 真实 CLI happy path |
| 2026-08-13 | **批量继续的验收从 Rust 测试移到前端队列测试。** M0 的 Rust command 一次只处理一个来源，批量顺序与“单项失败后继续”由 React 编排；在 Rust 另造一个 UI 不调用的批量函数不能约束真实控制流。前端按来源隔离错误并在队列结束后汇总提示 | M0 验收审计；[`CLAUDE.md`](../../CLAUDE.md) 约束 15「控制流由代码决定」 |
| 2026-08-13 | 解析编排补本位币前置：未设置时保留 `imported`，不建尝试、不消耗额度 | [00 地基 §3.4](./00-foundation.md) v0.13 实施回流 |
| 2026-08-13 | 失败来源验收由「关联草稿数为 0」改为「没有未作废草稿」——协议失败要保留 `voided_at` 历史行；`reviewed` 判据明确读取独立的 `consumed_at` / `discarded_at` | [00 地基 §3.6](./00-foundation.md) 的 append-only 历史语义 |
| 2026-08-10（五轮） | **口述显式合计的改定漏了本文**：§3.1 与 §6 仍写「恒为空」。这是「改了共享决定但没 grep 全仓」的又一次——同一轮里还漏了 [`docs/PRD.md`](../PRD.md)、[03 §3.4](./03-review.md)、[05 §3.4](./05-items.md) 与 `.claude/agents/backend.md`。**本轮同时建立 `scripts/check-spec-invariants.mjs` 把这类漂移变成会红的检查** | 文档审查（五轮） |
| 2026-08-10（二轮） | **§3.5「总额校验结果落在 `sources` 上」跟着合计一起搬家**：校验入参改为 `attempt_id`，agent 对 `sources` 的写入权限归零。§3.5.1 矩阵补三行（自报条目数对不上、agent 说有一块没读、记忆键没查全），并把两条对账相关行改成「状态 + 确认策略」两个字段 | [00 地基 §3.6](./00-foundation.md) v0.7 · [03 审核 §3.3](./03-review.md) v0.6 · [01 §3.2](./01-agent-runtime.md) v0.8 |
| 2026-08-10 | **§3.2 的「口述用内容哈希去重」会静默吞掉真实交易。** 跨天各说一句「今天咖啡 5 元」文本逐字相同，第二笔被判重复直接消失，而用户看到的是「已导入过」，不会去追。改为两种来源两把幂等键：`file` 用内容哈希，`utterance` 用一次提交一个令牌；文本重复只提示不阻止。同步 [00 地基 §3.6](./00-foundation.md) 的部分唯一索引与 `idempotency_key` 列 | 文档审查 |
| 2026-08-10 | **§3.4 的 `parsed` 判据是「退出码 0」，静默漏读因此被判为解析成功。** 改为「调过 `complete_source` 且正常退出」，未调即 `failed` + `agent.protocol_violation`。启动扫描同时扫 `ended_at` 为空的 `parse_attempts`。同步 [01 §3.2/§3.4](./01-agent-runtime.md) | 文档审查；产品决定（2026-08-10）接受为此扩大 M0 |
| 2026-08-10 | **新增 §3.5.1 降级与失败态矩阵。** 16 种情况此前每条都有归属，但散在六份文档的六个小节里，**没有一处能一起看到**——实现时最容易漏的不是某一条，是没意识到还有第七条。本表是索引，判据以各自出处为准 | 文档审查 |
| 2026-08-10 | §3.1 口述来源的总额校验结果由 `unavailable` 改为 **`not_applicable`**（[03 审核 §3.3](./03-review.md) 新增第四态） | 产品决定（2026-08-10）：给 utterance 独立信任策略 |

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.17 | 2026-08-30 | **第一次 no-go 后跨文档同步，`status` 仍为 `review`。** 口述只有 current-source 全覆盖 claim 才报告；月度外部范围、按日、单笔 / 子组合计与关键词本身均不够，同三元组 decoy 也拒报。降级矩阵补 kind 限定，明确 file / utterance 的既有确认策略不因 `failed` 改变。导入格式、落盘、幂等、状态机与编排实现不变 |
| v0.16 | 2026-08-24 | **参考设计稿评审的两项产品决定回流，`status` 仍为 `review`（新增两节都是 **M1** 范围，不改任何 M0 结论）。** ① §3.5 新增「什么时候允许自动开始解析」——批准「丢进来就自动整理」（默认开），并把本文里三条看似矛盾的决定统一到**一条判据**：消耗 agent 额度必须由一个明确的、用户当场知道自己做了的动作触发。拖入符合，自动重试与文件夹监听不符合，**所以不是三次拍脑袋**。批量拖入（≥ 2 个来源）不自动开始，因为一个动作要花掉 N 次额度时用户在动作发生的瞬间不知道 N 是多少。② 新增 §3.8「整理记录」——取**不需要新表**的定义（一个来源 + 它当前受审的那次尝试），显式否掉「一次坐下来那一批」的读法（需要跨来源分组 ID，而没有任何规则以「这一批」为单位）；同时登记四个导航名与**「设置」屏无规格归属**这个缺口 |
| v0.15 | 2026-08-22 | **补 `agent.not_ready`，`status` 仍为 `review`。** §3.5.1「readiness probe 尚未完成」一行明确：状态是非错误的中间态，但用户在这个窗口里显式发起解析时命令层返回 `agent.not_ready`；不创建尝试、不下发解析的行为不变 |
| v0.14 | 2026-08-17 | **安装资格 / 解析就绪度跨文档同步，`status` 仍为 `review`。** §3.5.1 不再把 `agent.backend_unavailable` 一律称为「没装」，补稳定 `availability_reason`，并把 readiness probe 完成前及 probe 期其他失败与任务期失败拆开：前者 `ready = false` 且不创建尝试、不改变来源；行为权威出处仍为 [01 §3.5](./01-agent-runtime.md) |
| v0.13 | 2026-08-13 | **实现回流：**§3.4 转移表补 `parsed → parsing` 与 `failed → failed`，并要求 `sources.state` 只有一处写入点——此前规格里的状态机没有生产调用点，生效的是 `agent/runtime.rs` 里与它矛盾的内联判断。§6 新增 2 条验收；`status` 仍为 `review` |
| v0.12 | 2026-08-13 | **M0 实现验收进入 `review`。** PNG/JPEG 与口述导入、幂等、串行编排、批量容错、崩溃恢复及真实截图/口述 happy path 已通过统一门禁 |
| v0.11 | 2026-08-13 | **实现回流：**批量容错验收改到真实的前端队列控制点；单项失败不再中断后续来源，结束后汇总提示。口述合计与完成协议的验收选择器同步对齐实际 `review` / `agent` 测试，消除 0-test 假绿 |
| v0.10 | 2026-08-13 | **实现回流：**解析前要求明确本位币，未设置时不启动任务 |
| v0.9 | 2026-08-13 | **M0 开始实施，`status` 进入 `in-progress`。** 修正失败来源与人工丢弃的验收语义 |
| v0.8 | 2026-08-10 | **文档审查第五轮同步**：§3.1 口述来源的合计由「恒为空」改为「**通常**为空；用户明说「总共 100」时照常对账」——与 [00 地基 §3.6](./00-foundation.md) v0.9 的改定对齐（此前只改了 00 和 03，本文漏同步）。§6 的 `utterance_source_roundtrip` 相应加一条「含合计的文本」断言 |
| v0.7 | 2026-08-10 | **文档审查第二轮回流**：§3.5 总额校验入参改为 `attempt_id`（合计已移入 `parse_attempts`）、agent 对 `sources` 写入权限归零；§3.5.1 矩阵由 16 行增至 19 行并把对账行改为「状态 + 确认策略」两个字段；§3.1 与 §6 的 `declared_total_*` 全部改为 `reported_total_*` |
| v0.6 | 2026-08-10 | **文档审查回流四处。** ① §3.2 **口述的幂等键由内容哈希改为一次提交一个令牌**——原写法会让跨天重复的真实交易静默消失。② §3.4 **`parsed` 的判据改为「调过 `complete_source`」**，未调即协议失败；启动扫描补扫未闭合的 `parse_attempts`。③ 新增 **§3.5.1 降级与失败态矩阵**（16 行索引）。④ §3.1 口述的总额校验结果改为 `not_applicable`。§6 验收新增 2 条、改写 1 条 |
| v0.1 | 2026-08-06 | 初版：v1 拖拽导入、SHA-256 内容幂等、证据先落盘后写库、来源五态状态机、解析编排（串行/记忆注入/失败不静默）、跨图去重候选判定（不自动合并）、批量导入；否决方案七条；待决 R1–R5；验收标准 11 条可执行 + 2 条人工 |
| v0.5 | 2026-08-08 | 公开仓库去个人化：§3.4 崩溃恢复扫描**去掉外部参考仓库出处**，把「任务恢复不依赖内存中的 Promise」作为原则直接写出——**做法未变**；§6 人工验收的具名网银样本改为「真实网银流水截图」；`owner` 改为 `@maintainer` |
| v0.4 | 2026-08-08 | **设计评审回流。** ① §3.1 新增**第二种来源 `utterance`**（M0）——依据 [`docs/PRD.md` §1.1](../PRD.md)「录入摩擦是痛点的另一半」；转写文本落盘成 `.txt` 与截图同等对待、`evidence_text` 取对应片段、闸门 3 对其天然失效、M0 只记交易且遇事项内容必须明说记不了。② §3.4 新增**启动时的崩溃恢复扫描**——此前解析任务只活在内存里，崩溃后 `state = parsing` 的来源**永久卡死**且重启后无人处理（M0 Ctrl-C 一次即复现）；依据「任务恢复不依赖内存中的 Promise」。③ §6 新增 4 条验收 |
| v0.3 | 2026-08-07 | **M0 开工评审 → `status: ready`。** ① §3.1 支持格式**按里程碑分层**——M0 只收 PNG/JPEG，HEIC 与 PDF 推到 M2；原文把 PDF 列进支持集，而多页切分策略是 R1 待决，「策略未定就支持」等于让实现者自己发明。② §3.2 补定**重复导入的返回契约**——成功返回 + `deduplicated` 标志，原文只说「返回 source_id 并提示」，未说成功还是错误，前后端会各写各的。③ §3.4 补 `state` 取值集的权威归属、`parse_error_code`、非法转移错误码，并对齐 [01](./01-agent-runtime.md) 修正后的「按会话作废」措辞。④ §3.5 明确 **v1 不做自动重试**（[01](./01-agent-runtime.md) R2 关闭的落点）——自动重试会在用户不知情时二次消耗额度。⑤ §5 **R3 改期至 M2**：HEIC 是相机照片格式，iPhone/macOS 截图默认 PNG，M0 用不到。⑥ §6 新增 4 条验收 |
| v0.2 | 2026-08-07 | 随 [`docs/PRD.md` v0.2](../PRD.md) 定位修正同步：§1 问题陈述去掉具名银行/支付平台（CBA、微信、支付宝）改为泛化来源，并显式声明「不为任何特定银行或支付平台写解析器」；§3.6 与人工验收中的具名样本降为 dogfooding 样本标注 |
