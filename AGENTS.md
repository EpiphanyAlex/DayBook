# AGENTS.md — Daybook

> 给 **Codex / 其他非 Claude agent CLI** 的精简入口。
> 本项目的 agent 后端是可插拔的（[ADR-0003](./docs/adr/0003-agent-runtime-and-pluggable-backend.md)），开发侧同样不绑定单一 CLI。
> **完整版是 [`CLAUDE.md`](./CLAUDE.md)**；本文是它的摘要，两者冲突时以 `CLAUDE.md` 为准。改了 `CLAUDE.md` 的约束，必须同步本文。

## 这是什么项目

Daybook（日簿）是一个 **macOS 本地优先的桌面应用**，帮用户把「过去这段时间的钱和事」补记回来。**回溯记录器**，不是记账 app，不是待办 app。AI 在此处的角色是**考古学家，不是输入框**：从截图等痕迹里把过去还原成待确认草稿，人审核后才入库。

**当前状态**：骨架阶段，`src/` 与 `src-tauri/` 尚未创建。第一个里程碑是 M0 端到端点亮。

## 十条不可违反的约束（完整 17 条见 `CLAUDE.md`「实施约束」）

1. **Tauri v2 + React/TS + Rust**。不要 Electron、不要内嵌 Node 服务、不要 `localhost` HTTP API（要改先写 ADR，见 [ADR-0001](./docs/adr/0001-local-first-desktop-platform.md)）。
2. **数据不出本机**。不引入云服务、后端 API、账号体系、遥测、崩溃上报、第三方分析。
3. **AI 永不直接写入账本**。agent 只能写 `draft_*` 表，人确认后才由确认动作写事实表（[ADR-0002](./docs/adr/0002-ai-never-writes-directly.md)）。
4. **每条草稿必须挂证据**：`source_id` + 原文片段。无证据不得入库。
5. **总额交叉校验**：拆出的条目合计必须与来源自己声明的合计/余额核对，不符时报警并阻止批量入库。
6. **金额一律整数存最小货币单位**（分 / cent）。任何位置禁止浮点，包括中间计算与 IPC 传输。
7. **多币种三元组**：原币金额 + 本位币金额 + 当时汇率，三个都存。
8. **审计日志 append-only**：只追加，不更新、不删除。
9. **工具权限由代码强制**：MCP 工具写入范围锁死在实现层；**不得提供通用的「执行任意 SQL」类工具**。
10. **控制流由代码决定**：状态机、确认点、重试策略是确定性的；LLM 只做抽取、解析、分类与起草。

## 干活之前

1. 读 [`docs/PRD.md`](./docs/PRD.md) 确认范围与里程碑。
2. 读对应的 sub-PRD（索引：[`docs/prd/INDEX.md`](./docs/prd/INDEX.md)）。**规格先行**——sub-PRD 是 `status: ready` 才能开工。
3. 需要实现细则时按主题读 [`.claude/rules/`](./.claude/rules/)；需要知道「某功能现在怎么实现」读 [`.claude/features/`](./.claude/features/)。

## 干完之后（缺一即视为未完成）

1. **跑门禁**——前端 `npm run lint` · `npm run typecheck` · `npm test` · `npm run build`，Rust `cargo fmt --check` · `cargo clippy -- -D warnings` · `cargo test`，任一失败即红。改过 `docs/prd/` 还要跑 `node docs/prd/check-docs.mjs`；改过任何 `.md` 还要跑 `node scripts/check-links.mjs`（两条文档门禁 CI 对所有 PR 强制）。
2. **回流**——实现相对规格的偏离、澄清、新发现回写对应 sub-PRD 的「回流记录」，版本号 +0.1。**实现证伪规格时先回写文档再改代码。**
3. **更新 status**——sub-PRD frontmatter 与 [`docs/prd/INDEX.md`](./docs/prd/INDEX.md) 同步；功能首次落地时补 `.claude/features/` 速查。

**计划易失，决定回流。** 实施计划不进 git，但计划/实现中做出的**决定**必须落回 sub-PRD。

## 写作与提交

- 文档一律中文 Markdown。新增 ADR 用 `docs/adr/NNNN-slug.md`，至少含日期、状态、背景、决策、理由、后果。
- 开 PR 套用 [`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md)，其中 **Constraint check** 一节逐条对应 `CLAUDE.md` 的 17 条约束。
