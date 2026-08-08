# Daybook（日簿）

**一个跑在本地的 AI 桌面助手，帮你把「过去这段时间的钱和事」补记回来。**

它不是记账 app，不是待办 app，是**回溯记录器**——为「事后补记」而设计，不是为「当场记录」。

> `daybook` 是会计术语「日记账」：按时间顺序记录原始凭证的簿子；字面又是「每日之书」。一个词同时覆盖钱和时间两个模块，也正好描述本项目的数据模型——一条时间轴 + 按时间排列的带证据原始记录。

---

## 当前状态

**骨架阶段（2026-08-06 建立）。** 约束与文档已就位，`src/` 与 `src-tauri/` **尚未创建**。
第一个里程碑是 **M0 端到端点亮**：拖一张截图 → agent 读 → 经 MCP 写入 SQLite → 列表显示。里程碑表见 [`docs/PRD.md` §9](./docs/PRD.md)。

现在唯一可跑的命令是文档门禁（CI 对所有 PR 强制，见 [`.github/workflows/docs.yml`](./.github/workflows/docs.yml)）：

```bash
node docs/prd/check-docs.mjs   # docs/prd/ 的 frontmatter + 链接
node scripts/check-links.mjs   # 全仓库 Markdown 链接
```

前端与 Rust 的命令要等 [`docs/prd/00-foundation.md`](./docs/prd/00-foundation.md) 落地后才存在，清单见 [`CLAUDE.md`](./CLAUDE.md)「常用命令」。

---

## 解决的问题

记账工具的设计前提是「交易发生时你会记一笔」——而这个前提在实际使用中几乎不成立。记录被推迟，直到某天你坐下来面对一堆模糊的痕迹，试图把过去两周重建出来。

**现有工具在这一步把用户丢掉了**：它们持续为「当场记录」优化，却把事后重建原样留给你手工完成——翻流水、对截图、逐条敲、算汇率。补记一次的成本远高于当场记十次，于是人就放弃了。

Daybook 的前提相反：**默认你是事后补的。**

## 产品原理

1. **用户自带 AI 额度 → 边际成本为零 → 可以对任意来源做重解析。**
   传统记账 app 必须为每家银行写专用解析器：格式改版就挂、长尾永远覆盖不到、每进入一个新国家都要重做一遍。按 API 计费的竞品算完 token 账也会退回去写解析器。
   **这条能力天然与银行、币种、国家无关——因为它根本不认识具体格式。** 多币种、多渠道、任意版式都是它的自然结果，不需要单独设计。
2. **接用户已装的 agent CLI（Claude Code / Codex）→ 直接获得成熟的 agent runtime。**
   这也是**必须桌面端**的理由：要调本地登录态、本地进程、本地文件。
3. **本地优先 + 无账号 + 无后端。**
   账目和日程极私密。数据不出本机 → 隐私天然成立、无各国合规问题、零服务器成本。

痛感随**账户数 → 支付渠道数 → 币种数**递增，三项越多价值越明显。验证样本取**多账户 + 多渠道 + 双币种**的组合，因为它对解析能力的压力最大——**是压力测试场景，不是市场边界**。

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

[MIT](./LICENSE)。

选 MIT 是为了让 [ADR-0001](./docs/adr/0001-local-first-desktop-platform.md)「后果」里那条**「贡献者只需 Node + Rust 工具链，不需要 Xcode」**真的成立——工具链门槛降下来了，许可门槛就不该再立一道。决策记录见 [`docs/PRD.md` §13](./docs/PRD.md) P4。
