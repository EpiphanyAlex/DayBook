# Daybook（日簿）

**一个跑在本地的 AI 桌面助手，帮你把「过去这段时间的钱和事」补记回来。**

它不是记账 app，不是待办 app，是**回溯记录器**——服务于「明知该记但当场记不了、只能事后补」的人。

> `daybook` 是会计术语「日记账」：按时间顺序记录原始凭证的簿子；字面又是「每日之书」。一个词同时覆盖钱和时间两个模块，也正好描述本项目的数据模型——一条时间轴 + 按时间排列的带证据原始记录。

---

## 当前状态

**骨架阶段（2026-08-06 建立）。** 约束与文档已就位，`src/` 与 `src-tauri/` **尚未创建**。
第一个里程碑是 **M0 端到端点亮**：拖一张截图 → agent 读 → 经 MCP 写入 SQLite → 列表显示。里程碑表见 [`docs/PRD.md` §9](./docs/PRD.md)。

现在唯一可跑的命令是文档门禁：

```bash
node docs/prd/check-docs.mjs
```

前端与 Rust 的命令要等 [`docs/prd/00-foundation.md`](./docs/prd/00-foundation.md) 落地后才存在，清单见 [`CLAUDE.md`](./CLAUDE.md)「常用命令」。

---

## 它凭什么和别的记账工具不一样

1. **用户自带 AI 额度 → 边际成本为零 → 可以暴力啃任意格式截图。**
   传统记账 app 必须给每家银行写专用解析器（银行改格式就挂、长尾永远覆盖不到）；按 API 计费的竞品算完账也会退回去写解析器。只有本项目能对着任意截图直接说「读吧」。
2. **接用户已装的 agent CLI（Claude Code / Codex）→ 白捡一个 agent 内核。**
   这也是**必须桌面端**的理由：要调本地登录态、本地进程、本地文件。
3. **本地优先 + 无账号 + 无后端。**
   账目和日程极私密。数据不出本机 → 隐私天然过关、无各国合规问题、零服务器成本。

最锋利的切口是**跨境双币种、多支付渠道**（澳洲银行卡 + 信用卡 + 微信 + 支付宝并行）——国内 app 看不懂 CBA 对账单，海外 app 不知道微信支付宝是什么，「人民币换澳元按当时汇率入账」两边都不管。

---

## 生死线：AI 永不直接写入账本

视觉模型**真的会把 168 读成 1680**，账本错一个数字用户信任就永久归零。四道闸门（完整论证见 [ADR-0002](./docs/adr/0002-ai-never-writes-directly.md)）：

| 闸门 | 作用 |
|---|---|
| **草稿区** | AI 只写 `draft_*` 表，人确认才进事实表 |
| **证据链** | 每条草稿挂「来自哪张截图、图上哪一行原文」——把「信任 AI」换成「扫一眼原文」 |
| **总额交叉校验** | 拆出的 N 笔加起来必须对上来源自己声明的合计/余额，对不上自己报警 |
| **append-only 审计日志** | 每次 AI 写入、每次人工修改都留痕 |

---

## 技术栈

| 层 | 选择 | 何时写 |
|---|---|---|
| UI | React 18 + TypeScript + Vite | v1 |
| 桌面壳 | Tauri 2 | v1 |
| 核心 | Rust —— `rusqlite` + 进程管理 + 文件监听 | v1 |
| Agent 工具面 | Rust MCP server（`rmcp` 官方 SDK） | v1 |
| Agent 后端 | 可插拔接口：`claude -p` / `codex exec` / API key / 本地模型 | v1 建接口，v1 只实现 Claude Code |
| 照片库读取 | Swift sidecar（PhotoKit，无 UI 独立二进制） | v1.1 |
| 语音 | v1 用 macOS 系统听写（零代码）→ v1.1 换 Swift sidecar | v1.1 |

选型理由见 [ADR-0001](./docs/adr/0001-local-first-desktop-platform.md)（为什么不 SwiftUI、为什么不 Electron）与 [ADR-0003](./docs/adr/0003-agent-runtime-and-pluggable-backend.md)（为什么是本地 MCP server）。

---

## 文档导航

| 你要找的 | 去读 |
|---|---|
| 协作规则与 17 条实施约束 | [`CLAUDE.md`](./CLAUDE.md) |
| 给 Codex 的精简入口 | [`AGENTS.md`](./AGENTS.md) |
| 产品范围、成功标准、非目标、里程碑 | [`docs/PRD.md`](./docs/PRD.md) |
| 难以逆转的决定 | [`docs/adr/`](./docs/adr/) |
| 系统架构基线 | [`docs/architecture.md`](./docs/architecture.md) |
| 术语（交易/事项/草稿/证据/来源/本位币…） | [`docs/CONTEXT.md`](./docs/CONTEXT.md) |
| 各能力的规格 | [`docs/prd/INDEX.md`](./docs/prd/INDEX.md) |
| 按主题拆分的实现细则 | [`.claude/rules/`](./.claude/rules/) |
| 「这个功能现在是怎么实现的」 | [`.claude/features/`](./.claude/features/) |

**本项目不用 ticket。** 人写「要什么和为什么」（sub-PRD），agent 出「怎么做」（plan 阶段），人审计划。理由与工作流见 [`CLAUDE.md`](./CLAUDE.md)「PRD 体系与工作流」。

---

## 成功标准

- **一个月**：用它补过 3 次账，**且没有一次因为不放心而回去翻原始截图核对**。一旦开始偷偷复核，它就已经失败了。
- **终点**：不再打开原来的记账软件和日程软件。

---

## 许可

尚未选定。开源意图见 [ADR-0001](./docs/adr/0001-local-first-desktop-platform.md)「后果」一节（可参与性是选择 web 技术栈的理由之一）。
