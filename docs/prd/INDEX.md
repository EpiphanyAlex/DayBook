---
title: sub-PRD 索引与状态总览
status: ready
owner: "@maintainer"
date: 2026-08-13
version: v0.26
---

# sub-PRD 索引

> 一个能力一份，扁平文件。**本项目不用 ticket**——人写「要什么和为什么」（sub-PRD），agent 出「怎么做」（plan 阶段），人审计划。
> 写作纪律见 [`CLAUDE.md`](./CLAUDE.md)（本目录）；产品范围与里程碑见 [`docs/PRD.md`](../PRD.md)。
> **改任何 sub-PRD 的 `status` 时必须同步本文的状态总览表**——两处不一致即缺陷。

## 状态总览

| # | sub-PRD | 覆盖 | status | version |
|---|---|---|---|---|
| 00 | [地基 Foundation](./00-foundation.md) | 数据层、SQLite schema、迁移、错误契约、金额类型与 IPC 表示 | **`review`** | v0.15 |
| 01 | [Agent 运行时](./01-agent-runtime.md) | MCP server（`rmcp`）、agent 启动器、密封启动配置、完成协议、可插拔后端接口 | **`review`** | v0.20 |
| 02 | [导入 Ingest](./02-ingest.md) | 截图与口述导入、`sources` 落库、解析编排、降级与失败态矩阵 | **`review`** | v0.13 |
| 03 | [审核与草稿区](./03-review.md) | 草稿区、证据链、按尝试对账、确认策略、审核界面 | **`review`** | v0.13 |
| 04 | [交易 Transactions](./04-transactions.md) | 交易实体、多币种三元组、账户与渠道、分类、回顾 | `draft` | v0.7 |
| 05 | [事项 Items](./05-items.md) | 事项实体（backlog → 排期 → 完成时长） | `draft` | v0.7 |
| 06 | [记忆 Memory](./06-memory.md) | 记忆规则（商户映射、纠正、语境词表） | `draft` | v0.6 |
| 07 | [评测 Eval](./07-eval.md) | 评测集、评分器、回归门槛、夹具与重放 | `draft` | v0.8 |

**M0 四份已于 2026-08-13 完成实现，且全部处在 `review`**：[00 地基](./00-foundation.md) v0.15、[01 Agent 运行时](./01-agent-runtime.md) v0.20、[02 导入](./02-ingest.md) v0.13、[03 审核与草稿区](./03-review.md) v0.13。确定性链路、外部进程 → stdio MCP → UDS → SQLite、真实 Claude Code 五工具能力探测及截图/口述 happy path 均通过。

> **03 当天先退回 `in-progress`、再回到 `review`**（2026-08-13）。**退回的原因**：「行内改一条草稿的金额」这条主路径必然失败（`edit_draft` 把本位币金额当独立输入，只改金额时三元组校验必然拒绝），而 §6 对应的验收测试改的是 `merchant` 而非金额，所以门禁全绿；进入 `review` 的判据「验收标准全部跑过」当时并不成立。同批改定：根 [`CLAUDE.md`](../../CLAUDE.md) 约束 5 补 `kind` 限定（它与 [03 §3.3](./03-review.md) 的口述确认策略正面冲突）；`review.incomplete_triple` 在界面上此前是死路，M0 补本位币与汇率输入。**回到 `review` 的依据**：[03 §6](./03-review.md) 的 M0 人工验收六条当天由真实 CLI 逐条实测通过（含「缺三元组当场补齐并确认」），逐条结论见 [03 §7](./03-review.md) v0.13。
>
> **同一次人工验收暴露了一个门禁盲区，值得单独记一笔**：桌面应用当天**根本起不来**（`src-tauri` 两个 bin 缺 `default-run`；`icons/icon.png` 是 16 位/通道，tauri 判定图标无效后 abort），而 `node scripts/verify-m0.mjs` 不带 `--skip-live` 全绿。**M0 的十一条门禁只测库、确定性链路与外部 MCP 链路，没有一条会启动桌面壳。** 两条已修复并各补一条 `cargo test` 断言（[03 §6](./03-review.md)）。

**`review` 不等于 M0 产品 go。** 维护者人工 review 与 [`docs/PRD.md` §9.4](../PRD.md) 的 20–30 张真实截图 + 20 段口述评测尚未完成；在正式给出 go / no-go 并完成收尾前，四份都不得进入 `done`。M0 当前三栏界面是功能基线，不是设计定稿；M1 开工前先确定设计稿与 token design system。

> ✅ **[01 Agent 运行时](./01-agent-runtime.md) 已于 2026-08-12 回到 `ready`——M0 不再被阻塞。** 它曾于 2026-08-09 退回 `draft`：§3.1「MCP server 在 Tauri 主进程内」与 §3.4「Tauri spawn CLI 并把 server 的 stdio 端接上」互斥——stdio 型 MCP server 由 agent CLI 自己 `fork/exec`，没有「连到已在跑的进程」这种形态。R6 的四项检查已全部做完，实测记录见 [`docs/spikes/2026-08-12-r6-agent-runtime.md`](../spikes/2026-08-12-r6-agent-runtime.md)。
>
> **结论：进程归属取候选 ①——独立 MCP helper 二进制 + Unix domain socket。** 候选 ②（应用自身二进制加子命令）实测同样可行，但会让两个进程同时写 SQLite；候选 ③（Agent SDK 内嵌）被 [`CLAUDE.md`](../../CLAUDE.md) 约束 1 挡在实测之前。
>
> **两处规格被实现证伪并已回流**（[01 §3.7](./01-agent-runtime.md)）：① capability manifest 的 `input_schema` 字段**后端拿不出来**，已删除并写明兜底；② hook **不在 CLI 的初始化握手里**——实测一个 `PreToolUse` hook 确实执行了而握手只字未提，探测改为「跑一次短会话并主动引发一次工具调用把它逼出来」，两处盲区如实登记。
>
> **一条要长期盯着的风险**：R6 第 ③ 项核实厂商条款的结果**不是绿灯**，详见 [01 §5](./01-agent-runtime.md) R4 与 [`docs/PRD.md` §12](../PRD.md)。**不阻塞 M0，但 M4 打包发布前必须重新核实。**

> **2026-08-10 文档审查同步**（[`docs/PRD.md` v0.10](../PRD.md)）：**八份 sub-PRD 全部有实质改动**，其中三条会产生错误行为、不只是措辞——① [01 §3.7](./01-agent-runtime.md)：`agent` 的**有效工具集**远大于我们注册的工具面，一条 `sqlite3` 命令即绕过四道闸门；② [03 §3.3](./03-review.md)：总额校验对**未消费**草稿求和，逐条确认一条后该来源再也回不到 `passed`；③ [00 §3.4](./00-foundation.md)：金额与汇率写死两位小数，JPY/KWD 会差 100 倍。三条产品决定已拍：**口述来源独立信任策略**（[03 §3.3](./03-review.md) 的 `user_attested_batch`）、**M0 扩到六表五工具**（[`docs/PRD.md` §9.2](../PRD.md)）、**账户维度现在留字段 M2 实现**（[04 §3.4](./04-transactions.md)）。逐条见各份「回流记录」。

**下一步**：维护者 review M0 实现；随后按 [`docs/PRD.md` §9.4](../PRD.md) 完成真实样本度量并给出 go / no-go。若进入 M1，先确定审核主路径设计稿与 token design system，再做 40 笔 30 秒的审核界面。

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
          │                     ├── 02 导入 ── 03 审核与草稿区 ──┬── 04 交易
          └─────────────────────┘                               ├── 05 事项
                                                                └── 06 记忆

07 评测 ── 不绑定单一里程碑，横跨全程（见上方「里程碑 × sub-PRD」）
```

- **[00 地基](./00-foundation.md) 是所有模块的前置**：schema、错误契约、金额类型定下来之前，其余七份的实现会各自用自己的假设填空（**零沉默原则**，见 [`CLAUDE.md`](./CLAUDE.md)）。
- **[03 审核与草稿区](./03-review.md) 是 [04 交易](./04-transactions.md) 与 [05 事项](./05-items.md) 的共同入口**：两个实体走同一套「草稿 → 确认 → 事实表」流程。
- **[06 记忆](./06-memory.md) 的输入来自 [03](./03-review.md) 的每一次人工纠正**；输出**不注入**解析流程，而是由 agent 经 `query_memory` 主动查（[06 §3.4](./06-memory.md)，2026-08-08 改定）。

## 跨 sub-PRD 的共享决定在哪

避免在多份文档里各写一遍（**跨文档一致性**，见 [`CLAUDE.md`](./CLAUDE.md) 硬规则 5）：

| 你想知道 | 权威出处 |
|---|---|
| 金额怎么存、汇率怎么表示 | [ADR-0004](../adr/0004-data-model-sqlite-integer-money.md) + [00 地基](./00-foundation.md) |
| **本位币可切换后的语义**（逐笔冻结 / 汇总分组） | [00 地基 §3.4](./00-foundation.md)「本位币切换语义」 |
| **M0 六张表的逐列字段** | [00 地基 §3.6](./00-foundation.md) |
| **币种精度（exponent）与换算公式** | [00 地基 §3.4](./00-foundation.md)「币种精度」 |
| **权威错误码集**（30 条，新增码先改这里） | [00 地基 §3.7](./00-foundation.md) |
| **总额校验：入参、求和范围、按什么等式、两个结果字段** | [03 审核 §3.3](./03-review.md)「校验式」 |
| **闸门 3 挡不住什么** | [03 审核 §3.3](./03-review.md)「基准值本身必须可核对」+ [01 §3.2](./01-agent-runtime.md)「`report_source_total` 可信性要求」 |
| **口述为什么能一次批量确认** | [03 审核 §3.3](./03-review.md)「`user_attested_batch` 换的是另一道闸门」+ [`docs/PRD.md` §1.1](../PRD.md) 脚注 |
| **有效工具集 / 密封启动配置** | [01 §3.7](./01-agent-runtime.md) + [ADR-0003 §3](../adr/0003-agent-runtime-and-pluggable-backend.md) |
| **「解析完成」的判据** | [01 §3.2](./01-agent-runtime.md)「`complete_source`」+ [02 导入 §3.4](./02-ingest.md) |
| **失败与降级都有哪些情况** | [02 导入 §3.5.1](./02-ingest.md)「降级与失败态矩阵」 |
| **账户与渠道的区别** | [04 交易 §3.4](./04-transactions.md) |
| **eval 的真值从哪来 / 条目怎么对齐** | [07 评测 §3.2](./07-eval.md) |
| **M0 端到端脚本验什么 / M0 的 go-no-go 阈值** | [`docs/PRD.md` §9.3](../PRD.md) · [`docs/PRD.md` §9.4](../PRD.md) |
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
| v0.26 | 2026-08-13 | **[03 审核与草稿区](./03-review.md) → v0.13，`status` 由 `in-progress` 回到 `review`——M0 四份现在全部在 `review`。** 依据是 [03 §6](./03-review.md) 的 M0 人工验收六条由真实 CLI 逐条实测通过，含 v0.12 新增的「缺三元组当场补齐并确认」。同批修掉两个让桌面应用起不来的缺陷（缺 `default-run`、图标 16 位/通道），并记下根因：**M0 门禁没有一条会启动桌面壳**。**00 / 01 / 02 状态与版本未变** |
| v0.25 | 2026-08-13 | **实现验收第二批回流：**[01 Agent 运行时](./01-agent-runtime.md) → v0.20（合计词闸门补出口与边界；CLI 发现路径补 nvm / fnm / volta / pnpm）、[02 导入](./02-ingest.md) → v0.13（状态机在实现里有两份且互相矛盾，转移表补 `parsed → parsing` 与 `failed → failed`，并要求 `sources.state` 只有一处写入点）。同批修好前端第二张币种 exponent 表与 Rust 那张的分叉（新增逐条比对测试）。**两份 status 均未变** |
| v0.24 | 2026-08-13 | **[03 审核与草稿区](./03-review.md) → v0.12，`status` 由 `review` 退回 `in-progress`。** 实现验收发现「行内改金额」主路径必然返回 `data.money_inconsistent`，而对应验收测试改的是 `merchant`，门禁看不见——进入 `review` 的判据当时不成立。同批改定：本位币金额改为导出值；根 [`CLAUDE.md`](../../CLAUDE.md) 约束 5 补 `kind` 限定（解开它与 [03 §3.3](./03-review.md) 的正面冲突）；口述报了合计时的背书提示补成硬要求；`review.incomplete_triple` 补上界面入口。**00 / 01 / 02 状态与版本未变** |
| v0.23 | 2026-08-13 | **M0 四份同步进入 `review`**：00→v0.15 · 01→v0.19 · 02→v0.12 · 03→v0.11。统一门禁、外部 MCP/UDS 与真实 Claude Code 链路通过；明确维护者 review 和 §9.4 真实样本 go/no-go 仍待完成，M0 UI 非设计定稿 |
| v0.22 | 2026-08-13 | [00 地基](./00-foundation.md) → v0.14：验收选择器对齐真实模块与测试清单，消除 `cargo test <filter>` 匹配 0 项仍返回成功的假绿；状态仍为 `in-progress` |
| v0.21 | 2026-08-13 | [02 导入](./02-ingest.md) → v0.11：批量失败后继续的验收移到真实前端队列控制点；状态仍为 `in-progress` |
| v0.16 | 2026-08-13 | **M0 四份同步进入 `in-progress`**：00→v0.12 · 01→v0.14 · 02→v0.9 · 03→v0.9；记录开工前对未知币种、工具 schema 指纹、失败草稿、人工丢弃与验收层级的回流 |
| v0.1 | 2026-08-06 | 初版：七份 sub-PRD 索引、状态总览、里程碑映射、依赖关系图、共享决定出处表 |
| v0.15 | 2026-08-12 | **R6 spike 做完，[01 Agent 运行时](./01-agent-runtime.md) 由 `draft` 回到 `ready`（v0.13）——M0 不再被阻塞。** 进程归属定为**独立 MCP helper 二进制 + Unix domain socket**（候选 ①，理由是把全部 SQLite 写入收敛到主进程一处）。**两处规格被实现证伪并已回流 01 §3.7**：capability manifest 的 `input_schema` 后端拿不出来（删字段 + 写明兜底）；hook 不在 CLI 初始化握手里，探测改为「跑一次短会话并主动引发一次工具调用」。**R4 风险上调**：厂商条款核实结果不是绿灯，可插拔后端由「接口先摆着」提为「第二个实现要真能跑」，同步 [`docs/PRD.md` §12](../PRD.md)。**新增 `docs/spikes/`**：带日期的实测记录，装 sub-PRD 不该装的易腐内容（flag 组合、已验证 CLI 版本号）。**其余七份 sub-PRD 的 status 与版本未变** |
| v0.14 | 2026-08-10 | **公开文档降噪同步**：00→v0.11 · 01→v0.12 · 05→v0.7 · 06→v0.6 · 07→v0.8 · [`docs/PRD.md`](../PRD.md)→v0.13；同时修正状态总览中 00/01/02/03/05 已落后于各文件 frontmatter 的版本号。**所有 status 未变** |
| v0.2 | 2026-08-07 | 随 [`docs/PRD.md` v0.2](../PRD.md) 定位修正同步 01/02/04 的版本号至 v0.2（去窄化：多币种与多渠道是能力而非定位，具名银行/支付平台降为 dogfooding 样本） |
| v0.3 | 2026-08-07 | 随**本位币可切换**拍板（[`docs/PRD.md` §13](../PRD.md) P2 关闭）同步：00 → v0.2、04 → v0.3；共享决定出处表新增「本位币切换语义」一行，指向 [00 地基 §3.4](./00-foundation.md) |
| v0.4 | 2026-08-07 | **M0 四份完成开工评审并进入 `ready`**：00 → v0.3、01 → v0.3、02 → v0.3、03 → v0.2。状态总览与「下一步」改写为进 plan mode 开 M0；共享决定出处表新增 5 行（M0 逐列字段、权威错误码集、总额校验比哪个金额、闸门 3 的边界、M0 端到端脚本） |
| v0.5 | 2026-08-08 | **设计评审同步。** 新增 **[07 评测](./07-eval.md)**（此前没有任何 sub-PRD 负责「agent 读得准不准」，而 `PRD §9.1` 认定它是生死线）；版本同步 00→v0.4 · 01→v0.4 · 02→v0.4 · 03→v0.3 · 04→v0.4 · 05→v0.2 · 06→v0.2；里程碑映射补一段说明 07 不绑定单一里程碑；共享决定出处表新增 6 行（工具的智能边界、日志分级、口述来源语义、交易/事项边界、记忆应用、读对判据） |
| v0.6 | 2026-08-08 | 随文档门禁进 CI 同步：07 → v0.2（夹具目录拆为 `fixtures/local/` 与 `fixtures/ci/` 两支）。文档门禁自此为两条——`docs/prd/check-docs.mjs` 与 `scripts/check-links.mjs`，由 `.github/workflows/docs.yml` 对所有 PR 强制 |
| v0.7 | 2026-08-08 | **公开仓库去个人化同步**：八份 sub-PRD 版本 +0.1（00→v0.5 · 01→v0.5 · 02→v0.5 · 03→v0.4 · 04→v0.5 · 05→v0.3 · 06→v0.3 · 07→v0.3）；全目录 `owner` 改为 `@maintainer`；本表去掉工具与会话指代。**状态、依赖关系与共享决定出处表未变** |
| v0.13 | 2026-08-10 | **第六轮文档审查同步**（00→v0.10 · 01→v0.11 · 02→v0.8 · 03→v0.8 · 05→v0.6 · 07→v0.7 · [`docs/PRD.md`](../PRD.md) v0.12）。**① 口述显式合计的改定补齐七处漏同步**（PRD §9.2、02 §3.1/§6、03 §3.4、05 §3.4、`.claude/agents/backend.md`）——最后一处会直接诱导后端把两个维度焊回一起。**② `file` 永不判 `not_applicable` 补上真正的理由**：`reported_total_* IS NULL` 分不清「结构性没有 / 漏读 / 截图裁掉」，从「没读到」反推「本来就没有」会把漏读伪装成正常（03 R7）。**③ 有效能力探测的比较对象由「工具清单」扩为 capability manifest**，hash 改名 `effective_capability_hash` 并覆盖 hook / 插件 / 权限模式——那三类没有名字也没有参数 schema，先前根本进不了比较集合。**④ 新增第四条文档门禁 `scripts/check-spec-invariants.mjs`**：前三条查格式、链接与提交关系，本轮的七处漂移全程是绿的 |
| v0.12 | 2026-08-10 | **第五轮文档审查同步**（00→v0.9 · 01→v0.10 · 07→v0.7）。四处精度问题：**① `evidence_span` 的坐标系定死**——原文「字符区间」在 Rust（UTF-8 字节）与 TS（UTF-16 code unit）之间有四种解释，**中文夹一个 emoji 即错位**；改为零起、左闭右开的 **Unicode code point**，两端实现方式指定，写入时强制 `slice_by_code_points(...) == evidence_text`。**② ordinal 跳号由口头协议变成可验证的检查**（跳号 + 空说明 ⇒ `agent.unexplained_gap`）。**③ 有效工具集探测补第二条要件：清单必须对全部工具来源具有权威性**——只返回 MCP `tools/list` 的接口形式上完全合规，却看不见 `Bash`/`Read`/`Edit`。**④ eval 的配对术语更正为 ordinal 上的 full outer join**（不需要动态规划），降级集合匹配收窄为纯诊断。新增 R10（span 报不准的退路）R11（M3 ordinal 跨表唯一）与 01 R7（结构化 `unparsed_regions`） |
| v0.11 | 2026-08-10 | **第四轮文档审查同步**（00→v0.8 · 01→v0.9 · 03→v0.7 · 07→v0.6，[`docs/PRD.md` v0.11](../PRD.md)）。三个真缺口：**① `draft_transaction` 新增必填 `source_ordinal`**——[07](./07-eval.md) 的条目对齐要预测侧的位置，而 `file` 来源没有 OCR 也没有坐标，系统算不出来；**② §3.7 的探测定死必须走结构化 introspection、不许问模型**（用模型自述验证对模型的约束是循环论证），拿不到即 R6 失败结论；**③ 未知币种由「回退 exponent 2 + 告警」改为拒绝**。另：`complete_source` 不写 `audit_log`（判据是会不会影响账目）、口述明说合计时允许对账（策略不变）、§9.4 干净来源率的口径冻结。机械漂移清理六处 |
| v0.10 | 2026-08-10 | **第三轮文档审查同步（[`docs/PRD.md` v0.10](../PRD.md)）：八份 sub-PRD 版本再全部 +0.1**（00→v0.7 · 01→v0.8 · 02→v0.7 · 03→v0.6 · 04→v0.7 · 05→v0.5 · 06→v0.5 · 07→v0.5）。**状态仍未变。** 五个阻塞项已解：**① 声明合计由 `sources` 移入 `parse_attempts.reported_total_*`**、对账入参改为 `attempt_id`（重试后两次尝试的输出此前会混在一起）；**② `accounts` 骨架进 M0**（外键指向不存在的父表会让第一条 `INSERT` 直接失败，已实测）；**③ 金额过 IPC 改为十进制字符串** + `|v| ≤ 10^15` 范围校验；**④ eval 改为按位置保序对齐**（拿被评字段当匹配键会让字段准确率恒为 100%）；**⑤ `complete_source` 条目数不符改为可补救的拒绝**。另：`not_applicable` 拆成 `reconciliation_status` + `confirmation_policy` 两个维度；记忆的查询覆盖由告警改为可补救的拒绝、`last_affirmed_at` 收窄为三种明确动作。错误码集由 24 条增至 28 条，M0 表数由五张改为六张 |
| v0.9 | 2026-08-10 | **第二轮文档审查同步（[`docs/PRD.md` v0.9](../PRD.md)）：八份 sub-PRD 版本全部 +0.1**（00→v0.6 · 01→v0.7 · 02→v0.6 · 03→v0.5 · 04→v0.6 · 05→v0.5 · 06→v0.4 · 07→v0.4）。**状态未变**——01 仍为 `draft`，且其 R6 spike 增加第 ④ 项（密封启动配置与有效工具集实测），与进程归属必须在同一次 spike 里一起测。共享决定出处表新增 8 行（币种精度、口述批量确认、有效工具集、完成判据、失败态矩阵、账户与渠道、eval 真值、go/no-go 阈值），错误码集由 18 条增至 24 条，M0 表数由四张改为五张 |
| v0.8 | 2026-08-09 | **文档审查同步。** ① **[01 Agent 运行时](./01-agent-runtime.md) 退回 `draft`**（v0.6）——MCP server 的进程归属自相矛盾，新增 R6 并要求 M0 开工前 spike；状态总览与「下一步」相应改写。② [05 事项](./05-items.md) → v0.4：删除 `draft_items.source_id` 可空这条例外——它与 [ADR-0002](../adr/0002-ai-never-writes-directly.md)「草稿表 `source_id` 非空」直接冲突，且 2026-08-08 引入 `kind = utterance` 后口述事项本来就有来源可挂。③ **依赖关系图修正**：05 事项此前在图上是断开的（挂在一条已闭合的支线下），现改为与 04、06 并列挂在 03 之后；`07 评测` 单独成行；「其余六份」改为「其余七份」（共八份 sub-PRD，00 是前置）。**状态与共享决定出处表其余部分未变** |
