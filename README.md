# Daybook（日簿）

**中文** · [English](./README.en.md)

**一个本地优先、不用逐条填表的 AI 个人事务助理，把零散的钱和事整理成账目与安排。**

**生活可以先发生，记录可以后来补。** 把截图交给 Daybook，或者直接说一段话；它会把其中的交易与事项拆成待确认记录，你只需审核。

「个人事务」在 v1 只指交易与事项两个实体。Daybook 不是完整日历或通用秘书；它的核心承诺是省掉记账与事项安排里反复出现的逐条表单操作。

> `daybook` 是会计术语「日记账」：按时间顺序记录原始凭证的簿子；字面又是「每日之书」。一个词同时覆盖钱和时间两个模块，也正好描述本项目的数据模型——一条时间轴 + 按时间排列的带证据原始记录。

---

## 当前状态

**M0 实现处于 review 阶段（2026-08-23）。** Tauri / React / Rust 已落地：六表地基、五工具密封 agent 链路、截图与口述导入、审核确认与总额交叉校验能端到端跑通，`src/` 与 `src-tauri/` 均已创建。M0 的定义是**端到端点亮**——拖一张截图 → agent 读 → 经 MCP **写草稿** → **人确认** → 写事实表 → 列表显示。里程碑表见 [`docs/PRD.md` §9](./docs/PRD.md)。

**但还不能称 M0 已完成。** [00 地基](./docs/prd/00-foundation.md)、[01 Agent 运行时](./docs/prd/01-agent-runtime.md)、[02 导入](./docs/prd/02-ingest.md)、[03 审核与草稿区](./docs/prd/03-review.md) 四份当前都是 `review`——01 的安装资格 / 解析就绪度规格曾被证伪并重写，修正实现与 §6 的 5 条本机人工验收已于 2026-08-23 全部执行完毕。维护者人工 review 与 [`docs/PRD.md` §9.4](./docs/PRD.md) 的真实样本 go / no-go 都尚未完成。**当前界面是功能基线，不是设计定稿**——M1 开工前先确定设计稿与 token design system。状态总览见 [`docs/prd/INDEX.md`](./docs/prd/INDEX.md)。

**阻塞 M0 的 spike 已于 2026-08-12 做完**：MCP server 跑在独立 helper 二进制里，经 Unix domain socket 连回主进程（[`docs/prd/01-agent-runtime.md` §3.1](./docs/prd/01-agent-runtime.md)，实测记录见 [`docs/spikes/`](./docs/spikes/)）。

---

## 跑起来

**这是一个 macOS 桌面应用**（[ADR-0001](./docs/adr/0001-local-first-desktop-platform.md)）。前置条件：

| 需要 | 说明 |
|---|---|
| macOS + Xcode Command Line Tools | Tauri 构建需要 |
| Node.js 20.19+ / 22.12+ | Vite 7 的要求 |
| Rust 1.85+ | 见 [`src-tauri/Cargo.toml`](./src-tauri/Cargo.toml) 的 `rust-version` |
| **你自己的 agent CLI** | 目前实现的是 Claude Code：`claude` 已安装**且已登录**。Daybook 不打包任何厂商凭证，也不提供第三方登录——用的是你自己那份订阅 |

```bash
npm install
npm run tauri dev
```

**首次使用要先在左栏选本位币**，否则解析会返回 `data.base_currency_required`——Daybook 不按地区替你猜。之后把截图拖进左栏，或者直接把记得的事情说成一段话。

数据（账本、证据原件、日志）都在应用数据目录里，界面上「在访达中显示」可以直接打开。

### 门禁

七条并列，任一失败即红（[`CLAUDE.md`](./CLAUDE.md) 约束 16 与「常用命令」）：

```bash
npm run lint && npm run typecheck && npm test && npm run build
cd src-tauri && cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

外加四条文档门禁（[`.github/workflows/docs.yml`](./.github/workflows/docs.yml)）：

```bash
node docs/prd/check-docs.mjs        # docs/prd/ 的 frontmatter + 链接
node scripts/check-links.mjs        # 全仓库 Markdown 链接
node scripts/check-readme-sync.mjs  # README.en.md 不落后于 README.md
node scripts/check-spec-invariants.mjs  # 现行章节不得残留已被推翻的结论
```

一条命令把上面十一条全部跑一遍，外加真实 CLI 的能力探测与截图/口述 happy path：

```bash
node scripts/verify-m0.mjs              # 含真实 CLI 两步，会消耗你自己的额度
node scripts/verify-m0.mjs --skip-live  # 跳过真实 CLI；这不是完整 M0 通过
```

CI 在所有 PR 上跑：文档门禁走 [`docs.yml`](./.github/workflows/docs.yml)，代码门禁走 [`ci.yml`](./.github/workflows/ci.yml)（即上面那条 `--skip-live`）。**CI 绿不等于 M0 通过**——真实 CLI 那两步需要一个已登录的 agent CLI，只能在本机跑。

---

## 解决的问题

记账工具的设计前提是「交易发生时你会记一笔」——而这个前提在实际使用中几乎不成立。记录被推迟，直到某天你坐下来面对一堆模糊的痕迹，试图把过去两周重建出来。

**现有工具在这一步把用户丢掉了**：它们持续为「当场记录」优化，却把事后重建原样留给你手工完成——翻流水、对截图、逐条敲、算汇率。补记一次的成本远高于当场记十次，于是人就放弃了。

同样的摩擦也出现在事项安排里：日程与待办工具要求你逐件创建条目，反复填写标题、日期、状态和时长。钱和事被分在两个 app 里，但你面对的是同一份负担——**不断把已经发生或准备去做的生活翻译成一个个表单。**

Daybook 的前提相反：**主路径不要求逐条填表；生活可以先发生，记录可以后来补。** 你交出截图、口述或文字，agent 把其中的交易与事项整理成待确认记录。

## 产品原理

1. **用户自带 AI 额度 → 产品侧边际成本为零 → 可以对任意来源做重解析。**
   传统记账 app 必须为每家银行写专用解析器：格式改版就挂、长尾永远覆盖不到、每进入一个新国家都要重做一遍。按 API 计费的竞品算完 token 账也会退回去写解析器。
   **这条能力天然与银行、币种、国家无关——因为它根本不认识具体格式。** 多币种、多渠道、任意版式都是它的自然结果，不需要单独设计。
   *（「边际成本为零」指的是本项目这一侧：token 记在你自己的订阅或 API 账户上。你那边不是零——额度有限，超出可能产生费用。）*
2. **接用户已装的 agent CLI（Claude Code / Codex）→ 直接获得成熟的 agent runtime。**
   这也是**必须桌面端**的理由：要调本地登录态、本地进程、本地文件。
3. **本地优先：无 Daybook 账号、无远程服务端、不托管你的数据。**
   账目和日程极私密。你不需要为 Daybook 注册任何账号，本项目不运行任何远程服务，**因此没有任何由我们运营的服务端会留存你的数据**。（你选的模型服务商那边留不留、留多久，取决于它自己的政策——那部分我们既不经手也管不了，见下。）

### 数据到底去了哪：如实说明

这里分两件事——**东西存在哪**，和**解析时什么会离开这台机器**。混在一起说就会出现「证据文件只在本机」和「截图会发给模型服务商」看起来打架的错觉。

**存在哪（持久化）**

| | 位置 |
|---|---|
| 账本、证据文件、日志 | **只在你的机器上**——应用数据目录，你看得见、能自己删 |
| Daybook 账号 / 远程服务端 / 云同步 / 遥测 / 崩溃上报 | **不存在**，本项目不运行任何远程服务 |

**什么会离开这台机器（传输）**

解析要靠你自己安装并登录的 agent CLI，而 `claude -p` / `codex exec` 的推理跑在它们各自的模型服务商那边。**解析一张截图时，那张截图和相关文本会由那个 CLI 发往它对应的服务商**，用的是**你自己**在那家服务商的订阅与登录态。这条链路由 CLI 自己发起——Daybook 不代理、不转发、不记录——但它确实存在。**它不产生任何 Daybook 侧的存储**：发出去的内容不经过我们的服务器，因为没有我们的服务器。

**解析内容会被发到哪，取决于你选的后端**：换成本地模型进程（[ADR-0003](./docs/adr/0003-agent-runtime-and-pluggable-backend.md) 的可插拔接口之一），解析内容就**不需要发给任何远程模型服务商**。注意这说的是解析内容——**那个本地进程自己会不会联网**（检查更新、上报使用情况）由它自己决定，Daybook 管不着也不替它担保。**这是你所选后端的属性，不是本产品的默认承诺。**

> **术语提醒**：技术栈里的「Rust MCP server」和「agent 后端」都是本机内部的东西——前者是暴露给 agent CLI 的本地工具面，后者是「用哪个本地进程做推理」的抽象接口。**两者都不是远程服务端**，别和上面说的「无远程服务端」搞混。

痛感随**账户数 → 支付渠道数 → 币种数**递增，三项越多价值越明显。验证样本取**多账户 + 多渠道 + 双币种**的组合，因为它对解析能力的压力最大——**是压力测试场景，不是市场边界**。

---

## 生死线：AI 永不直接写入账本

视觉模型**真的会把 168 读成 1680**，账本错一个数字用户信任就永久归零。四道闸门（完整论证见 [ADR-0002](./docs/adr/0002-ai-never-writes-directly.md)）：

| 闸门 | 作用 |
|---|---|
| **草稿区** | AI 只写 `draft_*` 表，人确认才进事实表。子进程以密封配置启动，**它实际拿得到的工具**在下发任务前实测 |
| **证据链** | 每条草稿挂「来自哪张截图（或哪段口述）、模型说它读的是哪一段」——审核时**并排给你看的是原件**，把「信任 AI」换成「扫一眼原件」 |
| **总额交叉校验** | 拆出的 N 笔加起来必须对上来源自己印着的合计，对不上自己报警。来源本来就没有合计时（比如你说的一段话），换成「整段原文并排 + 你按一次确认」那道闸门 |
| **append-only 审计日志** | 每次 AI 写入、每次人工修改都留痕；**AI 最初写的那一版永远保留** |

---

## 技术栈

| 层 | 选择 | 何时写 |
|---|---|---|
| UI | React + TypeScript + Vite（主版本在 [`00-foundation`](./docs/prd/00-foundation.md) 初始化时锁定为当时的最新稳定版） | v1 |
| 桌面壳 | Tauri 2 | v1 |
| 核心 | Rust —— `rusqlite` + 进程管理 + 文件监听 | v1 |
| Agent 工具面 | Rust MCP server（`rmcp` 官方 SDK） | v1 |
| Agent 后端 | 可插拔接口，**后端只能是你已配置好的外部进程**：`claude -p` / `codex exec` / 本地模型进程 | v1 建接口，v1 只实现 Claude Code |
| 照片库读取 | Swift sidecar（PhotoKit，无 UI 独立二进制） | v1.1 |
| 语音 | v1 用 macOS 系统听写（零代码）→ v1.1 换 Swift sidecar | v1 + v1.1 |

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
| 应用图标与加载动画（色板、几何、动效规格） | [`assets/brand/README.md`](./assets/brand/README.md) |
| 界面 token 体系（色阶、字号、间距、组件规格）**待评审** | [`design.md`](./design.md) |

要提 PR 的话，流程与模板见 [`CLAUDE.md`](./CLAUDE.md)。

**本项目不用 ticket。** 人写「要什么和为什么」（sub-PRD），agent 出「怎么做」（plan 阶段），人审计划。理由与工作流见 [`CLAUDE.md`](./CLAUDE.md)「PRD 体系与工作流」。

---

## 成功标准

- **一个月**：用它完成 3 次钱与事项的集中整理，**且没有一次因为不放心账目而回去翻原始截图核对**。一旦开始偷偷复核，它就已经失败了。
- **终点**：不再打开原来的记账软件和日程软件。

---

## 许可

**代码**：[MIT](./LICENSE)。随便用——fork、改、闭源商用、打包卖都行，不需要经过谁同意；署名欢迎，但不强制。

**你的数据不在这份许可的管辖范围内。** 账本、证据截图、转写文本和日志从来没有离开过你的机器，也从来没有进过任何由本项目运营的服务——所以它们不需要任何人授权，我们也没有任何可以授予或撤销的权利。

**agent CLI 不是 Daybook 的一部分。** Claude Code / Codex 由各自厂商发布，遵守它们自己的许可与服务条款；你用的是你自己的订阅与登录态。**用它们跑 Daybook 的解析是否符合各自条款，我们尚未完成核实**（[`docs/PRD.md` §12](./docs/PRD.md)、[`docs/prd/01-agent-runtime.md` §5](./docs/prd/01-agent-runtime.md) R4）——这条在 M4 打包发布前必须有结论。在那之前，请自己确认你的使用方式符合你所用 CLI 的条款。
