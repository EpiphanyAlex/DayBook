# Daybook 协作者指南

## 当前状态

**仓库处于骨架阶段（2026-08-06 建立）**：约束已就位，`src/` 与 `src-tauri/` **尚未创建**。设计文档（基线 v0.1 · 2026-08-06）是产品与架构的事实源。第一个里程碑是 **M0 端到端点亮**（见 [`docs/PRD.md`](./docs/PRD.md) 里程碑表）。

### 常用命令

**前端（仓库根目录）** — *FND-1 落地后可用*
- `npm install` — 安装依赖
- `npm run dev` — Vite 开发服务器（仅前端）
- `npm run tauri dev` — 启动完整桌面应用
- `npm run lint` · `npm run typecheck`（`tsc --noEmit`）
- `npm test`（Vitest 一次性）· `npm run test:watch`
- `npm run build`（`tsc --noEmit` + `vite build`）· `npm run tauri build`

**Rust / Tauri（`src-tauri/`）** — *FND-1 落地后可用*
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`

**文档门禁（现在就可用，CI 对所有 PR 强制）**
- `node docs/prd/check-docs.mjs` — `docs/prd/` 的 frontmatter 必填字段 + 相对链接可达
- `node scripts/check-links.mjs` — 全仓库 `.md` 相对链接可达（ADR、总 PRD、`CLAUDE.md`、`.claude/rules/` 等 `check-docs` 覆盖不到的部分），并禁止 `file://` 绝对路径
- `node scripts/check-readme-sync.mjs` — [`README.en.md`](./README.en.md) 不落后于 [`README.md`](./README.md)（判据见「文档层级」的同步规则）

三条都由 [`.github/workflows/docs.yml`](./.github/workflows/docs.yml) 在 push 到 `main` 与全部 PR 上跑。

## 产品事实

- Daybook 是一个 **macOS 本地优先的桌面应用**，帮用户把过去一段时间的钱和事补记回来。它是**回溯记录器**，不是记账 app，也不是待办 app。**为「事后补记」而设计，不是为「当场记录」。**
- 目标用户是**想记账但坚持不下来的人**——放弃的原因通常不是不想记，而是补记成本太高。「事后补」是使用常态，不是用户细分。
- **多币种 / 多渠道 / 任意版式是能力，不是定位。** 它们来自「用户自带额度 → 可对任意来源重解析」这一条，天然与银行、币种、国家无关（解析不认识具体格式）。多账户 + 多渠道 + 双币种的组合是 **压力测试场景，不是市场边界**——不要把产品叙事窄化到任何特定国家或币种。
- **AI 在此处的角色是考古学家，不是输入框**：从截图等痕迹里把过去还原成待确认草稿，人审核后才入库。核心动作是「重建」，不是「录入」。
- 数据全部留在本机：无账号、无后端、无云同步、无遥测。AI 能力由**用户自己已安装并登录的 agent CLI** 提供（Claude Code / Codex），应用不打包任何厂商凭证。
- 两个实体、一条时间轴：**「交易」**（金额/币种/汇率/商户/证据截图）与**「事项」**（一条记录走完 `backlog → 排到某天 → 完成并带实际时长` 的生命周期）。UI 分两个视图，底层共用时间轴与记忆。

## 实施约束

1. 桌面壳使用 **Tauri v2**；界面使用 **React + TypeScript**；系统能力、本地存储与进程管理由 **Rust/Tauri command** 提供。不创建 Electron、内嵌 Node.js 本地服务或 `localhost` HTTP API，除非先通过 ADR 修改平台决策（见 [ADR-0001](./docs/adr/0001-local-first-desktop-platform.md)）。
2. **数据不出本机**：不引入任何云服务、后端 API、账号体系、遥测、崩溃上报或第三方分析。唯一允许的出站流量是用户自己的 agent CLI 与其模型服务商之间的通信（由该 CLI 自行发起，应用不代理、不转发、不记录）。
3. **AI 永不直接写入账本**（[ADR-0002](./docs/adr/0002-ai-never-writes-directly.md)）：agent 只能产出**待确认草稿**（`draft_*` 表），经人工确认后才由确认动作写入事实表。任何绕过草稿区的写入路径都是缺陷。
4. **证据链强制**：每一条草稿必须挂载来源——`source_id`（哪张截图/哪个文件）+ 原文片段。审核界面必须把原文与解析结果并排呈现。无证据的草稿不得入库。
5. **总额交叉校验**：从一个来源拆出的条目，其合计必须与该来源自身声明的合计/余额核对；不符时必须显式报警并阻止批量入库。这是唯一能在无人工介入下捕获错误的机制，优先级高于解析准确率本身。
6. **金额一律以整数存储最小货币单位**（分 / cent）。任何位置禁止用浮点数表示金额，包括中间计算与 IPC 传输。
7. **多币种三元组**：每笔交易同时存**原币金额 + 本位币金额 + 当时汇率**。不得只存换算后的结果，也不得在录入时要求用户手算。
8. **append-only 审计日志**：每一次 agent 写入、每一次人工修改都留一条「谁 / 何时 / 把什么改成了什么」。审计表只追加，不更新、不删除。
9. **工具权限由代码强制，不靠 agent 自觉**：MCP 工具的写入范围在实现层面锁死（记账工具只能写交易表，事项工具只能写事项表，修正工具带硬规则）。不得提供通用的「执行任意 SQL」类工具。
10. **单 agent + 多工具**，不按业务领域拆 agent。子 agent 只用于**上下文隔离**（如解析超长截图），不用于业务分工。产品运行时不引入多 agent 自主编排。
11. **Agent 后端可插拔**：`claude -p` / `codex exec` / 用户自备 API key / 本地模型，接口从第一天存在。应用**不打包任何厂商凭证、不提供第三方登录、不代理厂商鉴权**；用户使用的是自己已安装并登录的 CLI。
12. **弱信号采集默认全关**（日历、git、浏览器历史、屏幕使用时间等）：逐项授权、可随时关闭、采集结果只留本机。窗口标题等高敏数据必须在 UI 上明示其敏感性。
13. **语音转写在本地完成**，音频不出本机。v1 使用 macOS 系统听写（用户在输入框内自行触发），不写任何 Swift 代码；本地转写 sidecar 推迟到 v1.1（见 [ADR-0005](./docs/adr/0005-voice-and-system-integration.md)）。
14. **记忆系统存规则，不存对话**：商户→分类映射、用户的每次纠正、个人语境词表、语音专有名词表。不得把原始对话历史当作记忆持久化。
15. **控制流由代码决定**：状态机、确认点、重试策略是确定性的；LLM 只做抽取、解析、分类与起草，不做最终业务决策。
16. **测试门禁**：前端 `npm run lint` · `npm run typecheck` · `npm test` · `npm run build` 与 Rust `cargo fmt --check` · `cargo clippy -- -D warnings` · `cargo test` **并列**，任一失败即红。
17. **v1 范围纪律**：「交易」做深、「事项」做薄。明确不做——提醒、重复任务、子任务、优先级算法、番茄钟、弱信号采集、意图↔事实闭环、历史数据导入、手机端、任何形式的同步/账号/后端/变现。范围变更需先改 [docs/PRD.md](./docs/PRD.md)。

## 文档层级

1. [`docs/PRD.md`](./docs/PRD.md)：产品范围、验收与非目标。
2. [`docs/adr/`](./docs/adr/)：已接受的难逆决策 —— `0001` 本地优先桌面平台、`0002` AI 永不直接写入与证据链、`0003` Agent 运行时与可插拔后端、`0004` 数据模型、`0006` smart agent dumb tools、`0007` 本地可观测性与日志分级；提议中 —— `0005` 语音与系统集成。
3. [`docs/architecture.md`](./docs/architecture.md)：系统架构基线。
4. [`docs/CONTEXT.md`](./docs/CONTEXT.md)：当前术语。
5. [`README.md`](./README.md)：导航与摘要；[`README.en.md`](./README.en.md) 是它的英文镜像。

文档采用中文 Markdown，**唯一例外是 [`README.en.md`](./README.en.md)**——仓库公开，它是英文读者的唯一入口。新增 ADR 使用 `docs/adr/NNNN-slug.md`，至少包含日期、状态、背景、决策、理由和后果。

**改了 [`README.md`](./README.md) 必须在同一个提交里同步 [`README.en.md`](./README.en.md)。** 增删章节、改结论、改链接、改表格行都算；纯中文措辞润色不影响事实时可略。中文是事实源，英文是镜像，两份冲突时以中文为准。**不同步即缺陷**——腐烂的英文版比没有英文版更糟：它会用过时的措辞冒充事实源，而唯一会读它的人恰好没有第二份可对照。

这条规则由 `node scripts/check-readme-sync.mjs` 强制，不靠自觉：判据是**祖先关系**而非提交时间——从 HEAD 出发、不在 `README.en.md` 最后一次改动的历史里、且改动了 `README.md` 的提交，数量必须为 0（时间戳只到秒且会被 `git rebase` 重写，会产生假绿）。因此**同一个提交里改两份**是唯一顺畅的路径——补一个中文错别字也要把对应措辞带到英文版。

**`.claude/rules/`** 是按主题拆分的实现细则，供 agent 按需加载。本文保持短，细则按需引用：

| 你要动的东西 | 读 |
|---|---|
| 金额、汇率、草稿区、审计（❌/✅ 代码对照） | @.claude/rules/money-and-data.md |
| Rust / Tauri：分层、错误契约、MCP 工具面、SQLite | @.claude/rules/rust-tauri.md |
| React / TypeScript：IPC 桥、审核界面键盘流、性能 | @.claude/rules/frontend.md |
| 某个功能「现在是怎么实现的」 | @.claude/features/README.md |
| 开发期 subagent 找谁干、边界在哪 | [`.claude/agents/README.md`](./.claude/agents/README.md) |

**`.claude/features/`** 是**功能领域速查**——每个功能「现在是怎么实现的」，供 agent 接手时免去逆向阅读代码。功能实现后必须补上对应文件。

**`.claude/agents/`** 是**开发期的 Claude Code subagent**（`architect` / `researcher` / `reviewer` / `backend` / `frontend` / `data-model` / `tester` / `prd-keeper` / `coordinator`）。**注意别与约束 10 混为一谈**：约束 10「单 agent + 多工具」管的是**产品运行时**的 agent 架构，本目录是**开发这个仓库时**的分工，两者只是同名。约束或 `.claude/rules/` 变了，同一个提交里把受影响的 agent 提示词一起改。

## PRD 体系与工作流

**本项目不用 ticket。** 人写「要什么和为什么」（sub-PRD），agent 出「怎么做」（plan 阶段），人审计划。把 sub-PRD 拆成执行单元是 agent 的活，不该由人预先做完。

```
docs/PRD.md          总 PRD：范围、用户、成功标准、非目标、里程碑地图
docs/prd/INDEX.md    sub-PRD 索引 + 状态总览
docs/prd/NN-*.md     sub-PRD：一个能力一份，扁平文件（00–07）
```

**工作流**：
1. 人 + agent 一起把 sub-PRD 写到 `status: ready`
2. agent 读 sub-PRD → 进 plan mode → 出实施计划 → 人审
3. 批准后开发，`status: in-progress`
4. 收尾（见下）→ `status: done`

**验收标准必须尽量写成可执行的命令，不是散文**——`cargo test x::y 通过`、`node scripts/verify-m0.mjs 退出码 0`，而不是「幂等性正确」。理由与产品哲学同源：把「你信不信 agent 说完成了」换成「跑一下就知道」。

### 收尾三件事（缺一即视为未完成）

1. **回流**：把实现相对规格的偏离、澄清、新发现回写到对应 sub-PRD 的「回流记录」，版本号 +0.1（写作纪律见 [`docs/prd/CLAUDE.md`](./docs/prd/CLAUDE.md)）。**实现证伪规格时先回写文档再改代码。** `docs/prd/` 落后于实现即缺陷。
   > **计划易失，决定回流**——agent 的实施计划不进 git，但计划/实现中做出的**决定**必须落回 sub-PRD。
2. **更新 status**：sub-PRD frontmatter 的 `status` 与 [`docs/prd/INDEX.md`](./docs/prd/INDEX.md) 同步。
3. **补 feature 速查**：功能首次落地时在 `.claude/features/` 建对应文件（数据流、关键文件路径、业务规则）；后续改动同步更新。

回写完跑 `node docs/prd/check-docs.mjs` 确认绿再收尾。
