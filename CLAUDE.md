# Daybook 协作者指南

## 当前状态

**仓库处于 M0 实现 review / 修正阶段（2026-08-24）**：Tauri/React/Rust、六表地基、五工具密封 agent 链路、截图/口述导入与审核确认已经落地，`src/` 与 `src-tauri/` 均已创建。[00 地基](./docs/prd/00-foundation.md) v0.20、[01 Agent 运行时](./docs/prd/01-agent-runtime.md) v0.26、[02 导入](./docs/prd/02-ingest.md) v0.16、[03 审核](./docs/prd/03-review.md) v0.17 **四份当前都是 `review`**——01 的安装资格 / 解析就绪度修正已落地，其 §6 的 5 条人工验收于 2026-08-23 在维护者本机全部实测执行完毕（当天修掉一个「已装未登录报错码不对」的实现缺陷；另有两条未修、留 M1 的界面问题，见 [01 §7](./docs/prd/01-agent-runtime.md)）。维护者人工 review 与 [`docs/PRD.md` §9.4](./docs/PRD.md) 的真实样本 go/no-go 尚未完成，所以**不得称 M0 已 done**。当前前端是功能基线，不是设计定稿。**M1 开工前的技术 / 产品决定已于 2026-08-24 全部关闭**：[`design.md`](./design.md) **v0.5 定稿**并接进 [`.claude/rules/frontend.md`](./.claude/rules/frontend.md) §10–§11；原图区域高亮 spike 证明 agent bbox 会误指相邻行，因此**不加坐标 schema**，保留完整原件 + `evidence_text`（[`docs/spikes/2026-08-24-r1-evidence-region.md`](./docs/spikes/2026-08-24-r1-evidence-region.md)）；前端状态选定 **TanStack Query v5 + screen reducer / 局部 state**，不引入 Zustand（[03 §3.8](./docs/prd/03-review.md)）。[`docs/design/`](./docs/design/README.md) 的九屏参考稿已评审，八条偏差逐条回流（三条改变了已写定的产品决定，见该文件），但**设计稿本身尚未按新规格重画**。设计文档仍是产品与架构的事实源——版本号以各文件 frontmatter 与 [`docs/prd/INDEX.md`](./docs/prd/INDEX.md) 为准。

> ✅ **阻塞 M0 的 R6 spike 已于 2026-08-12 做完并关闭，其结论已在 M0 实现中验证。** **MCP server 跑在独立 helper 二进制里**，由 agent CLI 自己 `fork/exec`，helper 经 Unix domain socket 连回 Tauri 主进程、**自己不碰数据库**（[`docs/prd/01-agent-runtime.md` §3.1](./docs/prd/01-agent-runtime.md)）。密封启动配置的具体 flag 组合与已验证的 CLI 版本号在 [`docs/spikes/2026-08-12-r6-agent-runtime.md`](./docs/spikes/2026-08-12-r6-agent-runtime.md)——**动 agent 运行时之前先读那一份**，里面三个反直觉的坑会直接决定实现对不对。

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
- `node scripts/check-spec-invariants.mjs` — **现行章节里不得残留已被推翻的结论**（禁用表见脚本内，每条都注明它防的是哪一次真实回退）

四条都由 [`.github/workflows/docs.yml`](./.github/workflows/docs.yml) 在 push 到 `main` 与全部 PR 上跑。**约束 16 的七条代码门禁由 [`.github/workflows/ci.yml`](./.github/workflows/ci.yml) 跑**（一条 `node scripts/verify-m0.mjs --skip-live`，不在 CI 里再抄一份清单）——**但 CI 绿不等于 M0 通过**：真实 CLI 那两步需要已登录的 agent CLI 且消耗额度，只能在本机跑。

> **第四条为什么存在**（2026-08-10 建立）：前三条查的是格式、链接可达性与提交关系。**一轮把「口述合计恒为空」改成「通常为空、说了就对账」的改动之后，它们全是绿的，而七处文档仍在说旧结论**——其中一处还写在 agent 提示词里，会直接把实现者引回错误的做法。**跨文档一致性（[`docs/prd/CLAUDE.md`](./docs/prd/CLAUDE.md) 硬规则 5）此前完全靠人记得 grep**，而这正是最容易忘的一步。 <!-- legacy -->
>
> 它不理解语义，只把几条最容易复发的旧措辞列成禁用表。**「变更记录」与「回流记录」整段跳过**（那两节的职责就是引用旧结论）；现行正文里确需引用旧措辞时，**在行尾加 `<!-- legacy -->` 显式标注**——不提供隐式启发式，那会让检查器在最该报警的地方闭嘴。

## 产品事实

- Daybook 是一个 **macOS 本地优先、不用逐条填表的 AI 个人事务助理**，把用户零散的钱和事整理成账目与安排。**「个人事务」在 v1 只指交易与事项两个实体**，不等于完整日历或通用秘书；范围仍以 [`docs/PRD.md` §5–§6](./docs/PRD.md) 为准。
- **生活可以先发生，记录可以后来补。** 「回溯优先」是设计原则，不再作为对外品类名称。产品仍为「事后补记」而设计，但核心承诺是省掉记账与事项安排里反复出现的逐条表单操作。
- 目标用户是**想把钱和事长期整理清楚、但总被逐条填表劝退的人**——放弃的原因通常不是没有整理意愿，而是当场记录与事后补记的操作成本太高。「事后补」是使用常态，不是用户细分。
- **多币种 / 多渠道 / 任意版式是能力，不是定位。** 它们来自「用户自带额度 → 可对任意来源重解析」这一条，天然与银行、币种、国家无关（解析不认识具体格式）。多账户 + 多渠道 + 双币种的组合是 **压力测试场景，不是市场边界**——不要把产品叙事窄化到任何特定国家或币种。
- **AI 在此处的角色是考古学家，不是输入框**：从截图等痕迹里把过去还原成待确认草稿，人审核后才入库。核心动作是「重建」，不是「录入」。
- **无 Daybook 账号、无远程服务端、不托管用户数据**：账本、证据与日志全部存在本机，无云同步、无遥测。AI 能力由**用户自己已安装并登录的 agent CLI** 提供（Claude Code / Codex），解析内容由该 CLI 发往其模型服务商，应用不代理、不转发、不记录、不打包任何厂商凭证。**对外别写裸的「无服务器/无后端/无账号」**——本产品自己有本机 MCP server 和「agent 后端」两个同名概念，措辞纪律见 [`docs/PRD.md` §3.3](./docs/PRD.md)。
- 两个实体、一条时间轴：**「交易」**（金额/币种/汇率/商户/证据截图）与**「事项」**（同一条记录的**计划与结果**两端，状态为 `backlog / scheduled / done / archived`；计划与结果可按用户原话记录日期、时间点、时间块或粗粒度日期范围，截止约束与计划独立）。UI 分两个视图，底层共用时间轴与记忆。

## 实施方法

非平凡改动遵守以下四条；明显的一行修正无需额外仪式，但现有门禁与同步义务不豁免。

- **先查证，再决定**：动手前先从本文、对应 PRD / ADR 与现有实现核实前提。未决歧义若会实质改变目标、契约、范围或难逆决策，必须显式说明假设与权衡并提请确认，不得静默选择。
- **简单优先**：只实现当前 `status: ready` 规格要求的最小设计，不增加未被要求的功能、抽象、可配置性或备用路径。
- **最小且完整**：每一处改动都必须能追溯到当前目标，或由该目标触发的仓库一致性义务；不顺手重构相邻代码，但强制的 PRD 回流、状态 / INDEX 同步、README 中英同步、规则与 feature 速查更新都属于改动范围，不得以「最小改动」为由省略。
- **验证驱动**：实现前先把成功判据写成测试、可执行命令，或带步骤与阈值的人工验收；多步任务的计划要为每一步写明验证点，实现后循环到全部通过。

## 实施约束

1. 桌面壳使用 **Tauri v2**；界面使用 **React + TypeScript**；系统能力、本地存储与进程管理由 **Rust/Tauri command** 提供。不创建 Electron、内嵌 Node.js 本地服务或 `localhost` HTTP API，除非先通过 ADR 修改平台决策（见 [ADR-0001](./docs/adr/0001-local-first-desktop-platform.md)）。
2. **数据不出本机**：不引入任何云服务、后端 API、账号体系、遥测、崩溃上报或第三方分析。唯一允许的出站流量是用户自己的 agent CLI 与其模型服务商之间的通信（由该 CLI 自行发起，应用不代理、不转发、不记录）。
3. **AI 永不直接写入账本**（[ADR-0002](./docs/adr/0002-ai-never-writes-directly.md)）：agent 只能产出**待确认草稿**（`draft_*` 表），经人工确认后才由确认动作写入事实表。任何绕过草稿区的写入路径都是缺陷。
4. **证据链强制**：每一条草稿必须挂载来源——`source_id`（**哪张截图、哪个文件，或哪段口述文本**；口述走 `kind = utterance`，转写文本落盘成 `.txt` 与截图同等对待）+ `evidence_text`。审核界面必须把**来源原件**与解析结果并排呈现。无证据的草稿不得入库，**全部 `draft_*` 表一视同仁，没有例外**。
   > **`evidence_text` 是 agent 的抽取声明，不是独立证据**——它和被核对的金额出自同一次模型输出，模型读错时它跟着错，两者自洽却一起错。**证据是不可变原件**，`evidence_text` 只是「原件上的哪个位置」的指针；并排的必须是原件（[ADR-0002 闸门 2](./docs/adr/0002-ai-never-writes-directly.md)）。**agent 的原始起草值（`drafted_json`）不可变**，人的编辑不得覆盖它。
5. **总额交叉校验**：从一个来源拆出的条目，其合计必须与该来源自身印着的**声明合计**核对；不符时必须显式报警，**且 `kind = file` 一律阻止批量入库**。这是唯一能在无人工介入下捕获错误的机制，优先级高于解析准确率本身。**基准是声明合计，不是账户余额**——余额要当基准需要期初/期末/方向三项，schema 只存一个数字，只印余额时正确结果是 `unavailable`（[ADR-0002 闸门 3](./docs/adr/0002-ai-never-writes-directly.md)）。
   > 四条容易写错的细节（[03 审核 §3.3](./docs/prd/03-review.md)）：**① 入参是 `attempt_id`，求和范围是该次尝试全部未作废的草稿**——按「未消费」求和会让逐条确认一条后该来源永远回不到 `passed`，按 `source_id` 求和会把重试后两次尝试的草稿混在一起；**② 合计存在 `parse_attempts.reported_total_*`，不在 `sources` 上**——它是那次解析的输出，和草稿同生共死；**③ 合计带类型**（`expense_total` / `income_total` / `net_change`），三者是三条不同的等式，判不出类型就是 `unavailable`；**④ 结果是两个字段**——`reconciliation_status`（能不能对账，四态）与 `confirmation_policy`（能不能批量确认，三态），**放行批量的是后者**；`kind = file` 永远拿不到 `user_attested_batch`。**`kind = utterance` 的确认策略与对账结果无关**——口述里说了合计、对上了或没对上，策略都是 `user_attested_batch`（[03 审核 §3.3](./docs/prd/03-review.md)）。它换的不是「免检」，是把机器对账换成「人对着整段原文背书」这道人工闸门，**代价是三条 UI 硬要求缺一不可，且对账 `failed` 时差额必须与确认按钮同屏**——那是全产品唯一一条「机器判定不符仍允许批量」的路径，**放行而不告知等于两道闸门都没有**。
6. **金额一律以整数存储与传输最小货币单位**。任何位置禁止用**小数/浮点**表示金额，包括中间计算与 IPC 传输。三条细则（[00 地基 §3.4](./docs/prd/00-foundation.md)）：**① 「最小单位」不恒等于「分」**——由 ISO 4217 的 exponent 决定（JPY/KRW 0 位、KWD/BHD/JOD 3 位），格式化除以 `10^exponent` 而非写死 100，汇率换算公式同样要带两边的 exponent；**② IPC 上金额与汇率是十进制字符串**——JSON 数字会让 `JSON.parse` 把超 `2^53` 的值静默舍入，而那正是 agent 读错数字时产生的值；**③ TS 的 `number` 只作为安全整数载体**，范围不变式 `|v| ≤ 10^15` 由 IPC 两侧各校验一次，超出返回 `data.amount_out_of_range`。**「TS 里没有浮点类型」做不到，所以这一条靠的是表示 + 范围，不是类型。**
7. **多币种三元组**：每笔交易同时存**原币金额 + 本位币金额 + 当时汇率**。不得只存换算后的结果，也不得在录入时要求用户手算。
8. **append-only 审计日志**：每一次 agent 写入、每一次人工修改都留一条「谁 / 何时 / 把什么改成了什么」。审计表只追加，不更新、不删除。
9. **工具权限由代码强制，不靠 agent 自觉**：MCP 工具的写入范围在实现层面锁死（记账工具只能写**交易草稿表** `draft_transactions`，事项工具只能写**事项草稿表** `draft_items`，修正工具带硬规则）。**工具集里不存在任何能触及事实表的工具**（与约束 3 同一件事，不是两条）。不得提供通用的「执行任意 SQL」类工具。
   > **这条只管我们注册的工具，另一半在启动参数里**（[01 §3.7](./docs/prd/01-agent-runtime.md)）：后端是通用编码 agent，自带执行命令与文件读写工具，一条 `sqlite3 daybook.db "INSERT INTO transactions …"` 就绕过全部四道闸门，**而遍历自己工具注册表的测试照样全绿**。因此 **agent 子进程必须以密封配置启动，且有效工具集在下发任务前实测**，不相等即 `agent.tool_surface_unsealed` 拒绝运行、不降级。**两半缺一，另一半就是装饰。**
10. **单 agent + 多工具**，不按业务领域拆 agent。子 agent 只用于**上下文隔离**（如解析超长截图），不用于业务分工。产品运行时不引入多 agent 自主编排。
11. **Agent 后端可插拔，但后端只能是「用户已配置好的外部进程」**：`claude -p` / `codex exec` / 本地模型进程，接口从第一天存在。应用**不打包任何厂商凭证、不存储用户的 API key、不提供第三方登录、不代理厂商鉴权、不自己发出站请求**；用户使用的是自己已安装并登录的 CLI。**「应用直连模型 API」不是可插拔接口的一个选项**——它会同时破掉本条与约束 2（唯一出站流量归 CLI），要做必须先写新 ADR 重定出站与凭证边界。
12. **弱信号采集默认全关**（日历、git、浏览器历史、屏幕使用时间等）：逐项授权、可随时关闭、采集结果只留本机。窗口标题等高敏数据必须在 UI 上明示其敏感性。
13. **语音转写在本地完成**，音频不出本机。v1 使用 macOS 系统听写（用户在输入框内自行触发），不写任何 Swift 代码；本地转写 sidecar 推迟到 v1.1（见 [ADR-0005](./docs/adr/0005-voice-and-system-integration.md)）。
14. **记忆系统存规则，不存对话**：商户→分类映射、用户的每次纠正、个人语境词表、语音专有名词表。不得把原始对话历史当作记忆持久化。
15. **控制流由代码决定**：状态机、确认点、重试策略是确定性的；LLM 只做抽取、解析、分类与起草，不做最终业务决策。
16. **测试门禁**：前端 `npm run lint` · `npm run typecheck` · `npm test` · `npm run build` 与 Rust `cargo fmt --all -- --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test` **并列**，任一失败即红。**Rust 三条的参数照抄，不用简写**——`cargo clippy` 不带 `--all-targets` 查不到测试与 example，等于门禁开了个口子。
17. **v1 范围纪律**：「交易」做深、「事项」做薄。明确不做——提醒、重复任务、子任务、优先级算法、番茄钟 / `in_progress` / 计时器、日/月视图、完整日历集成、多个不连续精确时间片段、每周实际用时汇总、弱信号采集、意图↔交易事实闭环、历史数据导入、手机端、任何形式的数据同步 / Daybook 账号 / **Daybook 自建的远程服务端** / 变现。范围变更需先改 [docs/PRD.md](./docs/PRD.md)。（**这里说的不是「agent 后端」**——那是本机的可插拔推理进程，见约束 11。）

## 文档层级

1. [`docs/PRD.md`](./docs/PRD.md)：产品范围、验收与非目标。
2. [`docs/adr/`](./docs/adr/)：已接受的难逆决策 —— `0001` 本地优先桌面平台、`0002` AI 永不直接写入与证据链、`0003` Agent 运行时与可插拔后端、`0004` 数据模型、`0006` smart agent dumb tools、`0007` 本地可观测性与日志分级；提议中 —— `0005` 语音与系统集成。
3. [`docs/architecture.md`](./docs/architecture.md)：系统架构基线。
4. [`docs/CONTEXT.md`](./docs/CONTEXT.md)：当前术语。
5. [`docs/spikes/`](./docs/spikes/)：**带日期的实测记录**（`YYYY-MM-DD-slug.md`）。装的是 sub-PRD 不该装的**易腐内容**——别人家 CLI 的 flag 组合、已验证的版本号、踩过的坑。**它会过期，过期了就重跑而不是修补**；因此每份必须在顶部写明被测版本。结论回流对应 sub-PRD，flag 留在这里。
6. [`README.md`](./README.md)：导航与摘要；[`README.en.md`](./README.en.md) 是它的英文镜像。

文档采用中文 Markdown，**两处例外，都因为仓库公开、它们是外部读者的第一接触面**：

1. [`README.en.md`](./README.en.md) —— 英文读者的唯一入口，[`README.md`](./README.md) 的镜像。
2. [`.github/PULL_REQUEST_TEMPLATE.md`](./.github/PULL_REQUEST_TEMPLATE.md) —— **模板骨架**（章节名、勾选项、注释）用英文，因为提 PR 是外部贡献者第一件要填的事。**PR 正文本身中英皆可。** 骨架里引用的约束仍以中文 [`CLAUDE.md`](./CLAUDE.md) 为准，英文只是转述。

新增 ADR 使用 `docs/adr/NNNN-slug.md`，至少包含日期、状态、背景、决策、理由和后果。

**动了 [`README.md`](./README.md)，[`README.en.md`](./README.en.md) 就必须在合并前跟上——任何改动都算，包括改一个错别字。** 中文是事实源，英文是镜像，两份冲突时以中文为准。**不同步即缺陷**——腐烂的英文版比没有英文版更糟：它会用过时的措辞冒充事实源，而唯一会读它的人恰好没有第二份可对照。

由 `node scripts/check-readme-sync.mjs` 强制，它按顺序查两件事：

1. **工作区**——`git diff HEAD` 里 `README.md` 变了而 `README.en.md` 没变，直接红。这一条在你还没提交时就拦住你。**CI 的工作区是干净的，所以那边恒不触发**，纯粹是本地的护栏。
2. **HEAD 的祖先关系**——不存在「动过 `README.md` 而 `README.en.md` 没见过」的提交。**这一条管的是 HEAD 这个状态，不是单个提交**：先提中文、再提英文，第二次提交后同样是绿的。

两条实务：

- **推荐同一个提交改两份**，不是因为门禁要求，而是因为中间那次提交是红的（bisect、CI 跑到它、别人在那个点检出，都会看见红）。
- **脚本不比对译文内容**，两份说的不是一回事它照样绿。看到 `✓` 不等于同步完了，自己过一眼对应段落。

**`.claude/rules/`** 是按主题拆分的实现细则，供 agent 按需加载。本文保持短，细则按需引用：

| 你要动的东西 | 读 |
|---|---|
| 金额、汇率、草稿区、审计（❌/✅ 代码对照） | @.claude/rules/money-and-data.md |
| Rust / Tauri：分层、错误契约、MCP 工具面、SQLite | @.claude/rules/rust-tauri.md |
| React / TypeScript：IPC 桥、审核界面键盘流、性能 | @.claude/rules/frontend.md |
| 界面视觉：色阶、字号、间距、组件规格、三类输入 | [`design.md`](./design.md)（v0.5 **定稿，是判据**）+ @.claude/rules/frontend.md |
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
4. 验收标准全部跑过 → `status: review`
5. 收尾三件事做完（见下）→ `status: done`

**完整状态机（含 `blocked` / `archived` 与各条转移的判据）只有一个事实源：[`docs/prd/CLAUDE.md`](./docs/prd/CLAUDE.md)「status 生命周期」。** 本文这五行是它的索引，不是第二份定义；对不上时以那份为准。

**验收标准必须尽量写成可执行的命令，不是散文**——`cargo test x::y 通过`、`node scripts/verify-m0.mjs 退出码 0`，而不是「幂等性正确」。理由与产品哲学同源：把「你信不信 agent 说完成了」换成「跑一下就知道」。

### 收尾三件事（缺一即视为未完成）

1. **回流**：把实现相对规格的偏离、澄清、新发现回写到对应 sub-PRD 的「回流记录」，版本号 +0.1（写作纪律见 [`docs/prd/CLAUDE.md`](./docs/prd/CLAUDE.md)）。**`+0.1` 是序号递增，不是小数运算——v0.9 的下一版是 v0.10。** **实现证伪规格时先回写文档再改代码。** `docs/prd/` 落后于实现即缺陷。
   > **计划易失，决定回流**——agent 的实施计划不进 git，但计划/实现中做出的**决定**必须落回 sub-PRD。
2. **更新 status**：sub-PRD frontmatter 的 `status` 与 [`docs/prd/INDEX.md`](./docs/prd/INDEX.md) 同步。
3. **补 feature 速查**：功能首次落地时在 `.claude/features/` 建对应文件（数据流、关键文件路径、业务规则）；后续改动同步更新。

回写完跑 `node docs/prd/check-docs.mjs` 确认绿再收尾。
