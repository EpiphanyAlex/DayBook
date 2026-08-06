# Daybook 交接文档

> **这是一份临时交接文件。** 它把「立项讨论」的全部结论压缩在一处，供新会话接手搭建脚手架。
> 脚手架搭完后，本文件的内容应已完全吸收进 `CLAUDE.md` / `docs/PRD.md` / `docs/adr/` / `docs/prd/`，届时**删除本文件**。
>
> 立项讨论完成于 2026-08-06，结论已由产品负责人逐条确认。

---

## 0. 一句话

**Daybook（日簿）——一个跑在本地的 AI 桌面助手，帮你把「过去这段时间的钱和事」补记回来。**

它不是记账 app，不是待办 app，是**回溯记录器**：服务于「明知该记但当场记不了、只能事后补」的人。

**名字由来**：`daybook` 是会计术语「日记账」——按时间顺序记录原始凭证的簿子；字面又是「每日之书」。一个词同时覆盖钱和时间两个模块，且正好描述本项目的数据模型（一条时间轴 + 按时间排列的带证据原始记录）。中文名「日簿」。

---

## 1. 核心洞察（整场讨论的转折点）

痛点**不是**「记一笔账要点五下」，而是——

> **隔了 7 到 10 天，我得回过头去把这段时间发生的事一件件挖出来。**

所以 AI 的角色是**考古学家**，不是输入框：从痕迹里把过去还原出来，摆到用户面前，用户只负责审。核心动作是**重建**，不是**录入**。

**推论**：产品的正确气质是**低频、重型、有仪式感**（一两周一次的「财务体检」），不是日常高频工具。它救不了「懒得动」，只救「动了之后很痛苦」。

---

## 2. 三个支点

1. **用户自带 AI 额度 → 边际成本为零 → 可以暴力啃任意格式截图。**
   传统记账 app 必须给每家银行写专用解析器（银行改格式就挂、长尾永远覆盖不到）；按 API 计费的竞品算完账也会退回去写解析器。**只有本项目能对着任意截图直接说「读吧」。**
2. **接 Claude Code / Codex → 白捡一个 agent 内核。**
   用户要的是「下达任务、它自己编排」，这两个 CLI 本身就是成熟 agent runtime。这也是**必须桌面端**的理由：要调本地登录态、本地进程、本地文件。
3. **本地优先 + 无账号 + 无后端 + 语音也本地。**
   账目和日程极私密。数据不出本机 → 隐私天然过关、无各国合规问题、零服务器成本——这是一个人能扛得动的唯一形态。

---

## 3. 生死线：AI 永不直接写入

视觉模型**真的会把 168 读成 1680**，账本错一个数字用户信任就永久归零。

- AI 只产出**待确认草稿**，人确认才入库（两张表：`draft_*` 与事实表）
- **每条草稿挂证据**：来自哪张截图、图上哪一行原文 → 把「信任 AI」换成「扫一眼原文」
- **总额交叉校验**：拆出的 N 笔加起来必须对上截图里的合计/余额，对不上自己报警。**这是唯一能无人工抓错的机制，比解析准确率本身更重要**
- **append-only 审计日志**：每次 AI 写入、每次人工修改都留痕

---

## 4. 数据形状

- **实体「交易」**：金额、币种、汇率、商户、分类、渠道、来源截图。多币种存**原币 + 本位币 + 当时汇率**三元组，不逼用户手算。**金额一律整数存最小货币单位，禁止浮点。**
- **实体「事项」**：一条记录走完整生命周期 `backlog（无日期）→ 排到某天 → 完成（带实际时长）→ 未完成退回 backlog`。「待办」和「时间日志」是它的两端，**不是两个功能**。
- **时间轴是共同骨架，但 UI 分两个视图**——不在同一张日历里混着显示钱和时间。
- **存储：SQLite 为唯一事实源**，截图证据存旁边的普通目录，另提供纯文本导出。数据库文件放在用户看得见能备份的位置，**不放 iCloud Drive**（会损坏）。

**终局价值**：待办写着「这周交房租 2000」，账单里出现一笔 2000 转账 → 待办自动打勾、预算自动核销。**意图与事实互相印证**——这是分成两个 app 永远做不到的事，也是「一个 app」的真正理由。但这是 **v2 的奖励，不是 v1 的门票**。

---

## 5. Agent 架构

**一个 agent + 两套工具 + 两张表。** 不按业务领域拆——用户一句话经常跨域（「今天吃饭 180，明天交房租，上周那 400 是给我妈买茶叶」），拆开就要分派再合并，凭空多出错误面还闭不了环。

**隔离靠工具权限，不靠 agent 自觉**：`记账` 工具在代码层只能写交易表，`改历史` 带硬规则。**不得提供通用的「执行任意 SQL」类工具。**

唯一该拆子 agent 的场景是**上下文隔离**（解析 60 笔的长截图），按上下文拆不按业务拆。

**关键架构判断：本 app 本质上是一个本地 MCP server。**
不要让 agent 输出 JSON 再解析，而是用 MCP 暴露工具。三个理由：① 工具层强制隔离直接落地 ② Claude Code 和 Codex 都支持 MCP，可插拔后端白拿 ③ 多轮编排天然支持（agent 可以查完历史再决定怎么记）。

```
┌─ React UI ──────────────────────┐   审核界面 / 两个视图 / 回顾
├─ Tauri (Rust) ──────────────────┤   进程管理、文件、SQLite
│    ├─ MCP server (stdio, rmcp)  │   ← agent 通过它读写数据
│    └─ agent launcher            │   ← spawn `claude -p` / `codex exec`
└─ SQLite + 截图目录 ─────────────┘
```

---

## 6. 交互形态

**对话下指令 + 界面做审核 + 界面看回顾。** 不是聊天框包打天下。

- **语音/文字**下达任务级指令，agent 自己编排
- **审核界面是胜负手**：整个产品省下的时间全兑现在这一屏。密集表格、原文并排、异常项排最前、默认全选、键盘走完。**做成 30 秒是成功，做成 20 分钟这产品就没意义。**
- 语音**只用于输入**，绝不用于审核（没法用嘴说「第 7 条金额改成 168」）

---

## 7. 记忆系统记什么

不是存对话历史，是存**规则**：商户→分类映射、用户的每次纠正、个人语境（「我妈」=家庭支出）、表达习惯、语音专有名词表。

**这是唯一的复利**：第一个月 40 条改 8 条，半年后 40 条改 1 条。

---

## 8. 技术栈（已定稿）

| 层 | 选择 | 何时写 |
|---|---|---|
| UI | React 18 + TypeScript + Vite | v1 |
| 桌面壳 | Tauri 2 | v1 |
| 核心 | Rust —— `rusqlite` + 进程管理 + 文件监听 | v1 |
| Agent 工具面 | Rust MCP server（`rmcp` 官方 SDK，production-ready） | v1 |
| Agent 后端 | 可插拔接口：`claude -p` / `codex exec` / API key / 本地模型 | v1 建接口，v1 只实现 Claude Code |
| 照片库读取 | Swift sidecar（PhotoKit，无 UI 的独立二进制） | **v1.1** |
| 语音 | v1 用 macOS 系统听写（连按两下 `Fn`，零代码）→ v1.1 换 Swift sidecar（SpeechAnalyzer） | v1.1 |

### 为什么不是原生 SwiftUI

给 SwiftUI 的公道话：PhotoKit / SpeechAnalyzer / 菜单栏 / 通知全是一等公民，二进制小，冷启动快，而 v1 本来就只做 Apple 生态。

**但它在两件命门上吃亏**：① **审核界面**（密集表格 + 行内编辑 + 原文并排 + 键盘流 + 虚拟滚动）是 web 的绝对主场，SwiftUI Table 做这个要一路顶着 API 打 ② **回顾图表**灵活度差一个量级。再加**开源可参与性**（不必装 Xcode）和 **Windows 后路**（改配置 vs 重写）。

**Swift 的 AI coding 问题真正卡在验证循环**（`.xcodeproj` 是巨大 XML、`xcodebuild` 反馈慢、SwiftUI 运行时问题要肉眼看模拟器）——而我们的 Swift 面积恰好完全避开：单个 `.swift` 文件、`swiftc` 一行编译、命令行验证、无 UI、各约 150 行。**v1 更是零 Swift。**

### 为什么不是 Electron

Tauri 二进制小一个数量级，Rust 侧做 SQLite / 进程管理 / 文件监听更扎实，且 `rmcp` 让 MCP server 可以在同一进程内起。

---

## 9. 里程碑

| | 内容 | 判定标准 |
|---|---|---|
| **M0** | 拖一张截图 → agent 读 → 经 MCP 写入 SQLite → 列表显示 | 端到端跑通，丑无所谓 |
| **M1** | 审核界面：表格、原文并排、键盘流、总额校验、异常前置 | 40 笔 30 秒审完 |
| **M2** | 多图批量、跨图去重、多币种 + 历史汇率 | 一次处理真实的 10 天 |
| **M3** | 事项薄层 + 记忆规则 | backlog → 拖到某天 → 记时长 |
| **M4** | 可插拔 agent 后端补全 + 打包 + README | 别人能跑起来 |

**M0 优先，因为它能验掉最大的未知数**：视觉模型读澳洲银行截图到底准不准。

---

## 10. v1 边界

**深：「交易」** —— 截图导入 → 解析重建 → 证据链 + 总额校验 → 审核 → 入库 → 基础回顾。做到能真正替代手工流程。

**薄：「事项」** —— backlog 列表、一句话批量丢入、拖到某天、记完成时段、能问「这周时间花哪了 / 什么没做完」。

**明确不做**：提醒、重复任务、子任务、优先级算法、番茄钟；弱信号采集（git/日历/浏览器/屏幕使用时间——未来做，但必须逐项授权、默认全关）；意图↔事实闭环；历史数据导入；手机端；同步、账号、后端；变现。

---

## 11. 需要清醒接受的三件事

1. **它救不了「懒得动」，只救「动了之后很痛苦」。** 用户依然要花 3-5 分钟在手机上截图。它把之后的一小时压成十几分钟。
2. **时间日志做不到「几点到几点」。** 钱有客观痕迹，时间没有。只能粗粒度——好在**时间容忍模糊，钱不容忍**。
3. **入门级会员的用量限制是真实约束。** 核心操作（多图 + 长上下文 + 多轮推理）恰好最烧额度。

---

## 12. 成功标准

- **一个月**：用它补过 3 次账，**且没有一次因为不放心而回去翻原始截图核对**。（一旦开始偷偷复核，它就已经失败了）
- **终点**：不再打开原来的记账软件和日程软件。

---

## 13. 竞品调研结论（2026-08-06）

- **国内移动端是红海**：账明、一木记账、咔皮记账、AI记账本、百事AA记账——「AI 截图记账 + 语音记账 + 对话分析」全部有人做且免费。**「AI 记账」本身不是差异化。**
- **海外开源集体回避 AI**：Actual Budget（本地优先、有 AI 查询）、Firefly III（**官方立场就是「不要黑盒机器学习」**）、ezBookkeeping、CurioPay。这反证了「AI 不敢写账本」是真实的坎——本项目的证据链 + 总额校验正是绕过它的路。
- **时间追踪**：[ActivityWatch](https://github.com/activitywatch/activitywatch)（开源、本地优先、跨平台、可写自定义 watcher）是弱信号采集的现成基础设施，**未来应当接它而不是自己造**；它只做「自动采集」，不做「人工补录」，也不碰钱——中间那层「把机器活动翻译成人类事项」正是本项目该加的价值。
- **结构性护城河（不是包装，是对手做不到）**：① 竞品商业模式依赖云，「数据不出本机」做了就没饭吃 ② 竞品自己付 token 钱，做不起重解析 ③ 竞品是记账公司，不做钱与时间同轴。
- **最锋利的切口**：**跨境双币种、多支付渠道**——国内 app 看不懂 CBA 对账单，海外 app 不知道微信支付宝是什么，「人民币换澳元按当时汇率入账」两边都不管。

### 平台风险（已知，不写进产品叙事）

Anthropic 对第三方 agent 使用订阅额度的政策反复过：2026-04-04 封禁 → 05 月改为 Agent SDK credits（按 API 价计费）→ **06-15 暂停该改动**（当前 `claude -p` 与第三方 app 仍走订阅额度）。方向明确但未落地。**对策就是可插拔后端接口，不需要在 README 或定位里反复强调。**

---

## 14. 文档与协作规范（照抄来源）

规范骨架抄 **MeritAI**（`~/MeritAI`），从 **JobPin AI**（`~/JobpinAI/Jobpin-AI`）挑三样。

### 从 MeritAI 抄（参考文件路径）

| 要抄的东西 | 参考文件 |
|---|---|
| `CLAUDE.md` 六段式（当前状态/产品事实/实施约束/文档层级/完成后必做） | `~/MeritAI/CLAUDE.md` |
| **目录级 `docs/prd/CLAUDE.md` 写作纪律**（最强的一招） | `~/MeritAI/docs/prd/CLAUDE.md` |
| `check-docs.mjs` 机械门禁（frontmatter 必填 + 相对链接可达，62 行，CI-ready） | `~/MeritAI/docs/prd/check-docs.mjs` |
| PR 模板含 **Constraint check**（把 CLAUDE.md 约束做成 checkbox） | `~/MeritAI/.github/PULL_REQUEST_TEMPLATE.md` |
| sub-PRD 深度标准（15 节）——**只抄结构，不抄 ticket 体系**（见 §17） | `~/MeritAI/docs/prd/00-foundation/README.md` |
| `.claude/settings.json` 只 allow 只读/门禁类命令 | `~/MeritAI/.claude/settings.json` |
| agent 花名册 + **维护约定**（出问题改 prompt 不改单次输出） | `~/MeritAI/.claude/agents/README.md` |

**写作纪律的核心六条**（务必落进 `docs/prd/CLAUDE.md`）：指称自包含 · 编号首现展开 · 路径真实 · frontmatter 必填 · 跨文档一致性 · 结论带出处。

两条结构性原则：
- **零沉默原则**：任何两份 sub-PRD / 两次实施必须达成一致的东西（schema、契约、状态语义），要么**被决定**（标依据），要么**显式挂起**（标谁来决）。**唯一不允许的状态是沉默**——沉默会被每次实施用自己的假设填掉，且各填各的。
- **深度花在边界上，不花在内部**：sub-PRD 只规格化模块边界（接口、schema、状态语义），内部实现自由度（函数组织、内部命名）不规格化——那是 agent planning 的空间。

> **⚠️ 本项目不用 ticket。** MeritAI 的 `tickets/` 体系（`size`/`agent`/`blocked_by`/file-scope 不相交/波次容量核对）是为 3 人并行设计的，本项目单人 + agent 执行，整套砍掉。**替代方案见 §17**——只保留它真正有价值的三样：规格先行、可验证的完成定义、回流义务。

### 从 JobPin AI 抄

1. **`.claude/features/`** —— 功能领域速查（每个功能「现在是怎么实现的」：数据流、schema、关键文件路径、业务规则），让 agent 接手时免去逆向读代码。参考 `~/JobpinAI/Jobpin-AI/.claude/features/credits-system.md`。
2. **`@path` 引用语法** —— `CLAUDE.md` 保持短，细则拆到 `.claude/rules/`，用 `@.claude/rules/xxx.md` 引用。参考 `~/JobpinAI/Jobpin-AI/CLAUDE.md` 的 Quick Navigation 表。
3. **规则配 ❌/✅ 代码对照** —— 比抽象描述有效一个数量级。参考 `~/JobpinAI/Jobpin-AI/.claude/rules/typescript-standards.md`。
4. **Chorus 的 status 生命周期**（`draft → ready → in-progress → review → done → blocked → archived`）与「AI 边做边勾 checkbox、立刻更新不批量」的强制规则。参考 `~/JobpinAI/Jobpin-AI/.claude/rules/prd-management.md`。

### Subagent 策略

**M0 阶段不建 subagent**——只有一张端到端的票，建 agent 是纯开销。等 M1（审核界面）与 M2（批量解析）需要并行时再拆 `frontend` / `backend` 两个。

已知的关键事实（来自 MeritAI）：**Claude Code subagent 的 `model:` 只能是 Claude 模型**，无法把大脑设成 GPT/Codex；要用 Codex 就得走「精简 Claude driver（sonnet）→ 运行 `codex exec` headless」的模式。

---

## 15. 骨架现状与待建清单

### 已完成

- `~/Daybook/` 目录结构 + `git init`（分支 `main`，尚无 commit）
- **`CLAUDE.md`** —— 六段式已写完，含 **17 条实施约束**

### 待建

| 文件 | 说明 |
|---|---|
| `README.md` | 导航与摘要 |
| `.gitignore` | Node / Rust / macOS |
| `AGENTS.md` | 给 Codex 的精简入口（因为要可插拔后端） |
| `.claude/settings.json` | `permissions.allow` 只放只读/门禁命令 |
| `.claude/rules/*.md` | 至少三份：Rust/Tauri、前端、金额与数据（配 ❌/✅ 对照） |
| `.claude/features/README.md` | 目录说明 + 何时补 |
| `.github/PULL_REQUEST_TEMPLATE.md` | 含 Constraint check（对应 CLAUDE.md 的 17 条） |
| `docs/PRD.md` | 范围、验收、非目标 |
| `docs/CONTEXT.md` | 术语表（交易/事项/草稿/证据/来源/本位币…） |
| `docs/architecture.md` | 三层架构 + MCP server + agent launcher |
| `docs/adr/0001-local-first-desktop-platform.md` | Tauri v2 + React/TS + Rust；为什么不 SwiftUI / 不 Electron |
| `docs/adr/0002-ai-never-writes-directly.md` | 草稿区 + 证据链 + 总额校验 + 审计日志 |
| `docs/adr/0003-agent-runtime-and-pluggable-backend.md` | 自带 CLI + 本地 MCP server + 可插拔后端接口 |
| `docs/adr/0004-data-model-sqlite-integer-money.md` | SQLite 单一事实源、整数金额、多币种三元组、两实体 |
| `docs/adr/0005-voice-and-system-integration.md` | **提议中** —— v1 零 Swift；sidecar 推 v1.1 |
| `docs/prd/CLAUDE.md` | 目录级写作纪律（照 MeritAI 那份改写） |
| `docs/prd/check-docs.mjs` | 机械门禁 |
| `docs/prd/INDEX.md` | sub-PRD 索引 + 状态总览 |
| `docs/prd/00-foundation.md` … `06-memory.md` | 七份 sub-PRD（清单见 §17） |

**注意：不建 `tickets/` 目录，不写 ticket。** 理由与替代方案见 §17。

---

## 16. 新会话的第一步

1. 读 `CLAUDE.md`（已存在）与本文件
2. 按 §15 待建清单逐个补齐，顺序建议：`.gitignore` → `README.md` → `docs/PRD.md` → 五份 ADR → `docs/CONTEXT.md` → `docs/architecture.md` → `docs/prd/CLAUDE.md` → `check-docs.mjs` → `docs/prd/INDEX.md` → 七份 sub-PRD → `.claude/rules` + PR 模板
3. 跑 `node docs/prd/check-docs.mjs` 确认绿
4. 首个 commit（骨架）
5. 把 M0 涉及的四份 sub-PRD 写到 `status: ready`，然后进 plan mode 开 M0
6. 骨架完成、内容全部吸收后**删除本文件**

---

## 17. PRD 体系（ADLC 形态，**不用 ticket**）

**决定（2026-08-06，产品负责人拍板）：不拆传统 Jira 式 ticket。**

### 为什么

ticket 的元数据（`size`、`agent`、`blocked_by/blocks`、file-scope 不相交、工时估算、波次容量核对）**存在的唯一理由是「把工作切成人类可承接的单元并在多人间协调」**。执行者变成 agent、团队规模变成 1 之后，这些字段全部失去意义。MeritAI 那套 85 张票是为 3 人并行设计的，照抄过来就是纯仪式。

**更根本的一点**：把 sub-PRD 拆成执行单元，**正是 agent 在 plan 阶段该做的事**。人预先拆完，等于替 agent 做了它更擅长的工作。

### 但三样东西必须保留（在 ADLC 里价值反而更高）

1. **规格先行** —— agent 不写规格就会自由发挥，而且发挥得很自信
2. **可验证的完成定义** —— **agent 会宣称完成**；这是 ADLC 特有的失败模式
3. **回流义务** —— agent 比人更容易在实现中偏离规格且不声张

### 结构

```
docs/
├── PRD.md                  ← 总 PRD：产品范围、用户、成功标准、非目标、里程碑地图
└── prd/
    ├── CLAUDE.md           ← 写作纪律
    ├── check-docs.mjs      ← 机械门禁
    ├── INDEX.md            ← sub-PRD 索引 + 状态总览
    ├── 00-foundation.md    ← 数据层、SQLite schema、迁移、错误契约
    ├── 01-agent-runtime.md ← MCP server（rmcp）、agent 启动器、可插拔后端接口
    ├── 02-ingest.md        ← 截图导入、sources 落库、解析编排
    ├── 03-review.md        ← 草稿区、证据链、总额校验、审核界面
    ├── 04-transactions.md  ← 交易实体、多币种三元组、分类、回顾
    ├── 05-items.md         ← 事项实体（backlog → 排期 → 完成时长）
    └── 06-memory.md        ← 记忆规则（商户映射、纠正、语境词表）
```

**扁平文件，不建文件夹**——文件夹原本是为了装 `tickets/`。某份 sub-PRD 长出附属材料（schema 草案、调研）时再改成文件夹。

### sub-PRD 的结构

照 MeritAI 的深度标准，但**删掉「工作拆分」那一节**（那是 agent planning 的产出）：

```yaml
---
title:
status: draft | ready | in-progress | review | done
owner: "@alex"
date: 2026-08-06
version: v0.1
---

## 1. 问题            这个模块解决什么
## 2. 范围与非目标
## 3. 决定与依据      架构选择，每条标依据（零沉默原则保留）
## 4. 否决的替代方案  为什么不那样做
## 5. 待决与风险      标谁来决、何时决
## 6. 验收标准        ⭐ 尽量写成可执行的命令，不是散文
## 7. 回流记录        实现证伪规格时回写这里
## 变更记录
```

**`check-docs.mjs` 的 frontmatter 必填字段据此设为：`title` · `status` · `owner` · `date` · `version`**（不再有 ticket 类的第二套字段）。

### 验收标准要写成「给 agent 跑」，不是「给人勾」

这是 ADLC 相对 SDLC 最有价值的一处升级，**务必落实**：

```markdown
❌ 给人勾（传统 ticket 写法）
- [ ] 幂等：并发双导入同一张截图只产生一条 source 记录

✅ 给 agent 跑
- [ ] `cargo test ingest::idempotent_source` 通过
- [ ] `node scripts/verify-m0.mjs` 退出码 0
```

理由和整个产品的哲学同源：**把「你信不信 agent 说完成了」换成「跑一下就知道」**——正如产品里把「信任 AI 的解析」换成「扫一眼原文 + 总额对账」。

### 工作流

```
1. 人 + agent 一起把 sub-PRD 写到 status: ready
2. agent 读 sub-PRD → 进 plan mode → 出实施计划 → 人审
3. 批准后开发，status: in-progress
4. 收尾：跑验收标准 → 回流 → status: done
```

**一条规则钉死：计划易失，决定回流。**
agent 的实施计划是一次性的，不进 git。但 planning 或实现过程中**证伪了规格**（例：「sub-PRD 里的 schema 行不通，因为 X」），必须**先改 sub-PRD 再写代码**，在「回流记录」留一行、版本号 +0.1。**这是防文档腐烂的唯一机制。**

### 里程碑与 sub-PRD 的关系

两个正交维度：**sub-PRD 按能力切，里程碑按时间切。** 里程碑表放在 `docs/PRD.md`：

| 里程碑 | 涉及 sub-PRD | 判定标准 |
|---|---|---|
| **M0** 端到端点亮 | `00` + `01` + `02` + `03` 各取最小切片 | 拖一张截图 → agent 读 → 经 MCP 入库 → 列表显示 |
| **M1** 审核界面 | `03` 做深 | 40 笔 30 秒审完 |
| **M2** 批量与多币种 | `02` + `04` | 一次处理真实的 10 天 |
| **M3** 事项与记忆 | `05` + `06` | backlog → 排期 → 记时长 |
| **M4** 可插拔与打包 | `01` 补全 | 别人能跑起来 |

M0 天生横跨多份 sub-PRD（walking skeleton 就是这样），在 `docs/PRD.md` 里写清楚每份取哪一片即可。
