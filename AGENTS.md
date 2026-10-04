# AGENTS.md — Daybook

> 给 **Codex / 其他非 Claude agent CLI** 的入口。本项目的 agent 后端可插拔（[ADR-0003](./docs/adr/0003-agent-runtime-and-pluggable-backend.md)），开发侧同样不绑定单一 CLI。

## 先读这一句

**[`CLAUDE.md`](./CLAUDE.md) 是唯一事实源，动手前必须完整读一遍。** 本文**不复述**它的 17 条实施约束——摘要会漂移：上一版这里抄的「数据不出本机」就漏掉了 `CLAUDE.md` 约束 2 里「用户自己的 agent CLI 与其模型服务商之间的通信是唯一允许的出站流量」这条例外，摘要读者因此得到了一条比原文更严格、也更错的规则。

Daybook 是一个 **macOS 本地优先、不用逐条填表的 AI 个人事务助理**，把用户零散的钱和事整理成账目与安排。**「个人事务」在 v1 只指交易与事项两个实体**；「回溯优先」是设计原则，不是品类名称。AI 的角色是**考古学家，不是输入框**。**当前状态**：第一次 M0 正式结果仍为 `no_go` / exit 3；修正实现与零额度门禁已完成，但独立新样本正式复测尚未授权。[00 地基](./docs/prd/00-foundation.md) v0.25、[01 Agent 运行时](./docs/prd/01-agent-runtime.md) v0.37、[03 审核](./docs/prd/03-review.md) v0.24、[07 评测](./docs/prd/07-eval.md) v0.23 与 [02 导入](./docs/prd/02-ingest.md) v0.19 中 03 为有限 M1 `done`，00/01/02/07 为零额度后端切片 `in-progress`。已落地关键词降级、current-source scope、formal v2 完整 fixture-set 指纹、scope 硬失败、bounded evidence、四硬字段两侧值、口述 span ordinal 与纯合成 CI 回归；旧报告与 `fixtures/local/m0-2026-08-24` 永久保留。真实 agent 与新 formal 仍需再次明确授权，授权前只跑零额度门禁。2026-09-06 的 M1 授权只开放 [03 审核 §3.9](./docs/prd/03-review.md) 的有限 M1 并行范围：已定案 design token、TanStack Query + reducer、完整原件 + `evidence_text` 安全退路；有限 M1 三项已完成零额度验收。独立复测由该有限切片的开工门槛改为 M1 整体验收门槛；实时事件与其余 M1、M2–M4 均未开放。

**近期范围（零额度切片实施中）**：先做 Codex 适配与 Claude Code / Codex 的统一模型选择，Gemini、Grok、DeepSeek 等后延。依据见[总 PRD](./docs/PRD.md#codex-model-priority)与 [01 Agent 运行时 §3.5 / R10](./docs/prd/01-agent-runtime.md#codex-model-selection)。运行时边界见 [01](./docs/prd/01-agent-runtime.md#runtime-harness)，截图专项协议见 [07](./docs/prd/07-eval.md#backend-screenshot-comparison)。[零额度可行性记录](./docs/spikes/2026-10-02-codex-app-server-feasibility.md)已确认部分协议能力，但 Codex 0.154.0 的完整有效能力面尚不可证，真实解析保持阻塞。统一选择、Codex 只读发现、共用有界进程/RPC 与评测归因已按审核计划实施；真实解析和截图专项仍未执行。真实调用/正式复测授权边界不变。

## 干活的顺序

1. **[`CLAUDE.md`](./CLAUDE.md)** —— 17 条约束、文档层级、PRD 工作流、收尾三件事。它用 `@` 引了三份 `.claude/rules/`（金额与数据 / Rust · Tauri / 前端），按需读。
2. **[`docs/PRD.md`](./docs/PRD.md)** —— 范围、非目标、里程碑。
3. **对应的 sub-PRD**（索引：[`docs/prd/INDEX.md`](./docs/prd/INDEX.md)）。**规格先行**——`status: ready` 才能开工，写 `docs/prd/` 时还要遵守 [`docs/prd/CLAUDE.md`](./docs/prd/CLAUDE.md) 的写作纪律。
4. 想知道「某功能现在怎么实现」读 [`.claude/features/`](./.claude/features/)。

**门禁、回流、`status` 同步、feature 速查、README 中英同步、PR 模板** —— 判据全在 [`CLAUDE.md`](./CLAUDE.md)「PRD 体系与工作流」与 [`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md)，照那两份做，别照记忆做。

## 两处例外

文档一律中文 Markdown，**两处例外**（仓库公开，它们是外部读者的第一接触面）：[`README.en.md`](./README.en.md) 与 [`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md) 的**骨架**（PR 正文中英皆可）。
