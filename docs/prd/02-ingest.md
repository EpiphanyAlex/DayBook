---
title: 02 导入 Ingest — 截图导入、来源落库与解析编排
status: ready
owner: "@alex"
date: 2026-08-07
version: v0.3
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

### 3.2 来源身份与幂等

- 导入时计算文件内容的 **SHA-256**，存 `sources.content_hash`，该列**唯一**
- **幂等以内容为准，不以文件名为准**——用户从不同 app 导出的同一张图文件名会不同

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
                │                      ▲
                └──▶ failed ───────────┘（重试后可回到 parsing）
```

| 状态 | 含义 |
|---|---|
| `imported` | 文件已落盘、已入库，尚未解析 |
| `parsing` | agent 子进程正在处理 |
| `parsed` | 解析完成，草稿已生成，等待人工审核 |
| `failed` | 解析失败或超时；**该会话的草稿已全部作废**（[01 Agent 运行时 §3.4](./01-agent-runtime.md) 的补偿性作废）。失败原因记在 `sources.parse_error_code` |
| `reviewed` | 该来源的全部草稿都已被确认或丢弃 |

**取值集与字段**：本表即 `sources.state` 的权威取值集，字段定义在 [00 地基 §3.6](./00-foundation.md)。

**状态转移由 Rust 侧代码执行，agent 无法改状态**——工具面里没有改状态的工具，且 agent 对 `sources` 的写入权限在数据层收窄到 `declared_total_*` 三列（[00 地基 §3.6](./00-foundation.md)「列级写入权限」、[01 Agent 运行时 §3.2](./01-agent-runtime.md)）。非法转移返回 `ingest.invalid_state_transition`。

### 3.5 解析编排

- 一次导入 N 个文件 → 生成 N 个解析任务，**串行执行**（v1 同时只跑一个 agent 子进程，见 [01 Agent 运行时 §3.4](./01-agent-runtime.md)）
- 解析前**注入记忆规则**到任务上下文（商户→分类映射等），来源是 [06 记忆](./06-memory.md)；注入点的具体形态见 [`docs/architecture.md` §8](../architecture.md) 未决 A3
- 解析后由代码触发**总额交叉校验**（[03 审核与草稿区](./03-review.md) 的职责），结果落在 `sources` 上
- **失败不静默**：`failed` 的来源在 UI 上显式列出，附失败原因（`parse_error_code`），可一键重试
- **v1 不做自动重试**（2026-08-07 评审，[01 Agent 运行时 §5](./01-agent-runtime.md) R2 关闭）：重试由用户在 UI 上显式触发。自动重试会在用户不知情时二次消耗 AI 额度，而额度是真实约束（[`docs/PRD.md` §12](../PRD.md)）

### 3.6 跨图去重（M2）

跨图重复的典型来源：信用卡账单截图与银行流水截图记录了同一笔消费。

- **候选判定**：同金额 + 同币种 + 日期相差 ≤ 2 天 → 标为疑似重复
- **绝不自动合并**——只在审核界面把疑似重复项**并排前置**，由用户判定（与 [ADR-0002](../adr/0002-ai-never-writes-directly.md) 「AI 不做最终业务决策」同源）
- 用户的判定结果进 [06 记忆](./06-memory.md)（例：「银行流水与信用卡账单总是重复，优先留信用卡那条」）

### 3.7 批量导入（M2）

- 一次可拖入的文件数无硬上限，UI 显示队列与逐个进度
- 任一文件失败不中断队列
- **队列可中断**：用户点停止后，已完成的来源保留，未开始的丢弃，进行中的按超时逻辑作废其草稿

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
- [ ] `cargo test ingest::failed_source_has_no_drafts` 通过——解析失败后该来源关联草稿数为 0，且 `parse_error_code` 非空
- [ ] `cargo test ingest::agent_cannot_write_source_columns` 通过——agent 经工具面只能写 `declared_total_*`，改 `state` / `evidence_relpath` 等列无路径可走
- [ ] `cargo test ingest::batch_continues_after_failure` 通过——队列中一个文件失败，其余照常完成
- [ ] `cargo test ingest::cross_image_dedup_candidates`（**M2**）通过——构造同金额同币种相差 1 天的两条草稿，被标为疑似重复且**未自动合并**
- [ ] `node scripts/verify-m0.mjs`（**待建**）退出码 0

**人工验收**：

- [ ] 拖入一张真实的网银流水截图，能走完 `imported → parsing → parsed`，草稿数与图上条目数一致（dogfooding 样本：CBA 网银）
- [ ] 拖入一张已导入过的图，UI 明确提示「已导入过」而不是静默无反应

## 7. 回流记录

*（尚无——本 sub-PRD 未开工。实现证伪规格时先回写这里，再改代码。）*

---

### 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| v0.1 | 2026-08-06 | 初版：v1 拖拽导入、SHA-256 内容幂等、证据先落盘后写库、来源五态状态机、解析编排（串行/记忆注入/失败不静默）、跨图去重候选判定（不自动合并）、批量导入；否决方案七条；待决 R1–R5；验收标准 11 条可执行 + 2 条人工 |
| v0.3 | 2026-08-07 | **M0 开工评审 → `status: ready`。** ① §3.1 支持格式**按里程碑分层**——M0 只收 PNG/JPEG，HEIC 与 PDF 推到 M2；原文把 PDF 列进支持集，而多页切分策略是 R1 待决，「策略未定就支持」等于让实现者自己发明。② §3.2 补定**重复导入的返回契约**——成功返回 + `deduplicated` 标志，原文只说「返回 source_id 并提示」，未说成功还是错误，前后端会各写各的。③ §3.4 补 `state` 取值集的权威归属、`parse_error_code`、非法转移错误码，并对齐 [01](./01-agent-runtime.md) 修正后的「按会话作废」措辞。④ §3.5 明确 **v1 不做自动重试**（[01](./01-agent-runtime.md) R2 关闭的落点）——自动重试会在用户不知情时二次消耗额度。⑤ §5 **R3 改期至 M2**：HEIC 是相机照片格式，iPhone/macOS 截图默认 PNG，M0 用不到。⑥ §6 新增 4 条验收 |
| v0.2 | 2026-08-07 | 随 [`docs/PRD.md` v0.2](../PRD.md) 定位修正同步：§1 问题陈述去掉具名银行/支付平台（CBA、微信、支付宝）改为泛化来源，并显式声明「不为任何特定银行或支付平台写解析器」；§3.6 与人工验收中的具名样本降为 dogfooding 样本标注 |
