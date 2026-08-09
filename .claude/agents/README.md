# Daybook 开发 agent 团队

> 这些是**开发 Daybook 仓库时**用的 Claude Code subagent（`.claude/agents/*.md`），只影响开发工作流，**不进入产品运行时**。

## 先分清一件事：两个「agent」不是同一层

仓库根 [`CLAUDE.md`](../../CLAUDE.md) 约束 10 写着「**单 agent + 多工具**，不按业务领域拆 agent」——**那条管的是产品运行时**：Daybook 跑起来之后，用户机器上只有一个 agent 拿着一组工具在解析截图、起草条目，产品不引入多 agent 自主编排（[ADR-0006](../../docs/adr/0006-smart-agent-dumb-tools.md)）。

**本目录是开发期的**：维护者与 Claude Code 一起写这个仓库时的分工。按角色拆开发期 subagent **不与约束 10 冲突**——两者只是同名。讨论时务必先说清指的是哪一层，否则会被当成违反约束而白讨论一轮。

| | 产品运行时 agent | 开发期 subagent（本目录） |
|---|---|---|
| 跑在哪 | 用户机器上，Daybook 启动的 CLI 子进程 | 贡献者本机，Claude Code 会话内 |
| 规格在 | [`docs/prd/01-agent-runtime.md`](../../docs/prd/01-agent-runtime.md) · [ADR-0003](../../docs/adr/0003-agent-runtime-and-pluggable-backend.md) | 本目录 |
| 架构 | 单 agent + 多工具，不按业务拆 | 按角色拆 |
| 能写什么 | 只有 `draft_*` 表（[ADR-0002](../../docs/adr/0002-ai-never-writes-directly.md)） | 仓库文件（各 agent 有各自边界） |

## 角色与模型

| Agent | 角色 | 推理引擎 | 读/写 |
|---|---|---|---|
| [`researcher`](./researcher.md) | 收集开发资料、比较方案、查依赖许可证 | **codex headless**（`codex exec`）+ web | 只读 |
| [`reviewer`](./reviewer.md) | 按 17 条约束的 rubric 审代码/diff | **codex headless**（`codex exec review`） | 只读 |
| [`architect`](./architect.md) | 架构设计 / ADR / 模块分解 | **codex headless**（`codex exec`） | 写文档 |
| [`backend`](./backend.md) | Rust / Tauri：`commands` `domain` `mcp` `agent` | **Opus** | 读写代码 |
| [`frontend`](./frontend.md) | React + TS：审核界面、两个视图、IPC 桥 | **Opus** | 读写代码 |
| [`data-model`](./data-model.md) | SQLite schema、迁移、`store/`、金额原语 | **Opus** | 读写代码 |
| [`tester`](./tester.md) | 验收标准可执行化、夹具与重放、eval、门禁 | **Opus** | 写测试与夹具 |
| [`prd-keeper`](./prd-keeper.md) | 回流、status 同步、feature 速查、文档门禁 | **Opus** | 写文档 |
| [`coordinator`](./coordinator.md) | GitHub：PR（套模板）、issue、CI | **Sonnet** + gh CLI | PR/Issue |

**边界划分**（避免两个 agent 改同一片地）：

```
src-tauri/src/commands|domain|mcp|agent   → backend
src-tauri/src/store + schema + 迁移        → data-model
src/                                       → frontend
tests/ + fixtures/ + scripts/eval*.mjs     → tester（生产代码只读）
docs/ + .claude/features/ + README*.md     → prd-keeper（架构决定与 ADR 归 architect）
.github/                                   → coordinator
```

**单元测试跟着实现走**，由 `backend` / `frontend` / `data-model` 在同一个 PR 里交（约束 16 的门禁本来就要求）。`tester` 负责的是**验收层**——把验收标准变成能跑的命令、造可复现的夹具、跑 eval 与回归、独立复核「跑过了」这句话。这样分是为了避免实现方等测试、测试等实现方的乒乓。

## 关键事实（非显而易见）

- **Claude Code subagent 的 `model:` 只能是 Claude 模型**（opus / sonnet / haiku / fable），没法把大脑设成 GPT/Codex。所以 codex 三人组（`researcher` / `reviewer` / `architect`）是**「精简 Claude driver（sonnet）→ 运行 `codex exec` headless」**的模式：真正的推理由 codex 侧的模型完成，Claude 层只负责组织 prompt、跑 CLI、**核验**并转述。
- **不把具体模型名写进本目录**：codex 用的是**本机 `~/.codex/config.toml` 的默认模型**，各人不同，写死会误导贡献者，也等于把某台机器的私人配置固化进公开仓库。某次任务确需钉死某个模型时，在那次调用里加 `-m <model>`。
- **核验这一步不能省**：Codex 看不到你的会话，它给出的文件路径、表名、字段名、错误码必须回仓库查证，对不上的直接丢弃。三个 codex agent 的提示词里都写死了这条。
- 这套模式与产品里的 agent 后端同构——[ADR-0003](../../docs/adr/0003-agent-runtime-and-pluggable-backend.md) 的可插拔后端第一天就包含 `codex exec`，开发期用它就是 dogfooding。但**参数不同**：开发期用 `--sandbox read-only`，沿用本机既有的 codex 配置；产品运行时那套隔离参数是给用户账目数据用的，别混。
- driver 用 `sonnet` 省成本；想让某个 codex agent 的编排层更强，把它的 `model:` 改成 `opus` 即可。
- **两个 agent 不碰版本控制**（`architect`、`prd-keeper`）：它们只留工作区改动，提交/推送/开 PR 归 `coordinator` 与编排者。这样「谁动了 git」永远只有一个答案。
- **`tester` 不改被测的生产代码**。理由同上：**能改被测代码的测试者，会用放宽断言来让红变绿**。测试挂了只有两种正确反应——改测试，或把缺陷报给实现 agent；「把断言调松一点」是这个角色存在意义的反面。
- **`tester` 跑 eval 前必须先说成本、等人批准**：eval 走生产同一条路径（真 spawn agent CLI），20 个用例 ≈ 20 次真实导入的订阅额度消耗（[`docs/prd/07-eval.md`](../../docs/prd/07-eval.md) §3.1）。它**不进 CI、不自动触发**；进 CI 的是零额度、确定性的夹具重放。
- 新增/改动 agent 后当前会话不一定热加载；必要时新开会话或用 `/agents` 刷新。

## 用法

- 显式调度：Agent 工具传 `subagent_type: "backend"`（或在 FleetView 里点名）。
- 多 agent 编排：用 Workflow 工具把这些 agent 编进 pipeline/parallel。
- **典型一轮**：`architect` 出设计并写进 sub-PRD → `data-model` / `backend` / `frontend` 实现（各自带单元测试）→ `tester` 跑验收标准与门禁、贴真实输出 → `reviewer` 按 17 条约束审 → `prd-keeper` 做收尾三件事 → `coordinator` 套 [PR 模板](../../.github/PULL_REQUEST_TEMPLATE.md) 开 PR。
- **`reviewer` 与 `tester` 不重复**：前者读代码找「写得对不对」，后者跑命令看「跑起来对不对」。两者都只读生产代码、都不自己动手改——发现的问题回到实现 agent。
- 本项目**不用 ticket**：人写「要什么和为什么」（sub-PRD），agent 出「怎么做」（plan 阶段），人审计划——见 [`CLAUDE.md`](../../CLAUDE.md)「PRD 体系与工作流」。

## 维护约定

**agent 出问题时，定位根因并更新对应 agent 的提示词**——把修复写回 `.claude/agents/<name>.md`，而不是只手动纠正当次输出。

1. 定位根因：缺指令 / 指令歧义 / codex 参数错 / 少了某条约束 / 工具范围不对 / 模型选错？
2. 对症改该 agent 的系统提示正文，必要时改 frontmatter（`tools` / `model` / `description`）。
3. 改动**最小且可泛化**（写通用规则，不堆当次事故细节，避免提示词膨胀）；只把**会复发**的失败模式写进去。
4. 说明改了什么、为什么，可能时复测确认。

约束本身变了（[`CLAUDE.md`](../../CLAUDE.md) 的 17 条、[`.claude/rules/`](../rules/) 的细则、新的 ADR），**同一个提交里把受影响的 agent 提示词一起改**——提示词里抄着过期约束，比没抄更糟。

### 提示词厚度分两档，别写成一样厚

**判据是「这条规则在别处有没有事实源」，不是「重不重要」。**

| 档 | 谁 | 怎么写 | 约 |
|---|---|---|---|
| **薄** | `backend` `frontend` `data-model` | 角色 + 必读的 rules 链接 + 地盘边界 + 三条底线 + 门禁 + 收尾 | 30 行 |
| **厚** | `reviewer` `tester` `architect` `prd-keeper` | 完整的 rubric / 方法 / 判据——**agent 文件就是唯一事实源** | 60–110 行 |

实现类之所以薄，是因为 [`CLAUDE.md`](../../CLAUDE.md) 用 `@.claude/rules/…` 把三份细则 import 进了上下文——**在 agent 提示词里再抄一遍就是造第二个事实源**，改了 rules 忘了改提示词就会漂移。这与 [`docs/prd/CLAUDE.md`](../../docs/prd/CLAUDE.md) 硬规则 5（跨文档一致性）是同一条纪律。

判断类之所以厚，是因为 `reviewer` 的 17 条约束 rubric、`tester` 的 eval 与回归的成本区分、`architect` 的模块分解规则、`prd-keeper` 的收尾操作形态，在 `rules/` 里**不存在**——薄了就没内容了。

**新增 agent 时先问：我要写的东西，`.claude/rules/` 或 `docs/` 里已经有吗？** 有就链过去，没有才写进提示词。
