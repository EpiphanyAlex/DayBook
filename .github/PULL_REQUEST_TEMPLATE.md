## 这个 PR 做了什么

<!-- 一句话。 -->

## 对应的 sub-PRD

<!-- 链接 docs/prd/NN-xxx.md；纯文档 PR 写「无」。 -->

- 规格：`docs/prd/`
- 本 PR 覆盖该 sub-PRD 的哪一部分：

## 类型

- [ ] `feat` — 新功能
- [ ] `fix` — 缺陷修复
- [ ] `refactor` — 重构（行为不变）
- [ ] `docs` — 文档 / ADR / sub-PRD
- [ ] `test` — 测试
- [ ] `chore` — 构建、依赖、工具链

## 改动

<!-- 改了什么、为什么。 -->

-

## Constraint check

<!--
Daybook 的 17 条不可协商约束（仓库根 CLAUDE.md）。
勾选本 PR 触及的，其余标 N/A。**逐条看过再勾**——这一节是本模板存在的理由。
-->

**平台与隐私**

- [ ] **1 平台** — 仅 Tauri v2 + React/TS + Rust；无 Electron、无内嵌 Node 服务、无 `localhost` HTTP API（否则需新 ADR，见 [ADR-0001](../docs/adr/0001-local-first-desktop-platform.md)）
- [ ] **2 数据不出本机** — 无云服务、后端 API、账号体系、遥测、崩溃上报、第三方分析；新增依赖已确认不在运行时发请求

**AI 边界（[ADR-0002](../docs/adr/0002-ai-never-writes-directly.md)）**

- [ ] **3 AI 只写草稿** — agent 无任何可达的事实表写入路径；`domain::confirm` 不被 MCP 工具调用
- [ ] **4 证据链** — 草稿的 `source_id` + 原文片段非空；审核界面原文与解析结果并排
- [ ] **5 总额交叉校验** — 不符时报警并阻止批量入库；**无 force / ignore 类旁路**
- [ ] **9 工具权限由代码强制** — 无通用「执行任意 SQL」/ 任意文件写入 / 任意命令执行类工具
- [ ] **10 单 agent + 多工具** — 未按业务领域拆 agent；子 agent 只用于上下文隔离
- [ ] **15 控制流由代码决定** — 状态机 / 确认点 / 重试是确定性代码，LLM 不做最终业务决策

**金额与数据（[ADR-0004](../docs/adr/0004-data-model-sqlite-integer-money.md)）**

- [ ] **6 整数金额** — 全链路整数最小货币单位；中间计算与 IPC 传输均无浮点
- [ ] **7 多币种三元组** — 原币金额 + 本位币金额 + 当时汇率三者齐全且自洽
- [ ] **8 审计 append-only** — 无 `UPDATE` / `DELETE` 针对 `audit_log`；agent 写入与人工修改都留痕

**其他**

- [ ] **11 Agent 后端可插拔** — 上层只依赖接口；**代码中无厂商凭证 / endpoint / 登录流程**
- [ ] **12 弱信号采集默认全关** — 逐项授权、可随时关闭、高敏数据在 UI 明示（v1 不做此项则标 N/A）
- [ ] **13 语音本地** — 音频不出本机；v1 用 macOS 系统听写，无 Swift 代码（[ADR-0005](../docs/adr/0005-voice-and-system-integration.md)）
- [ ] **14 记忆存规则不存对话** — 未持久化原始对话历史
- [ ] **17 v1 范围纪律** — 未引入 [`docs/PRD.md` §6](../docs/PRD.md) 的非目标；若扩范围，本 PR 已同步修改 `docs/PRD.md`
- [ ] 不需要新 ADR，**或** 本 PR 已包含新增/修订的 ADR

## 门禁（[`CLAUDE.md`](../CLAUDE.md) 约束 16：任一失败即红）

<!-- 代码 PR 必跑并勾选；纯文档 PR 只需第一项。 -->

- [ ] 文档：`node docs/prd/check-docs.mjs` 绿（改过 `docs/prd/` 时必跑）
- [ ] 前端：`npm run lint` · `npm run typecheck` · `npm test` · `npm run build`
- [ ] Rust：`cargo fmt --all -- --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test`
- [ ] 本地跑得起来：`npm run tauri dev`

## 收尾三件事（[`CLAUDE.md`](../CLAUDE.md)，缺一即视为未完成）

- [ ] **回流** — 实现相对规格的偏离/澄清/新发现已写进对应 sub-PRD 的「回流记录」，版本号 +0.1
      *（实现证伪规格时先回写文档再改代码。计划易失，决定回流。）*
- [ ] **status 同步** — sub-PRD frontmatter 的 `status` 与 [`docs/prd/INDEX.md`](../docs/prd/INDEX.md) 一致
- [ ] **feature 速查** — 功能首次落地已在 [`.claude/features/`](../.claude/features/) 建对应文件；后续改动已同步

## 验收证据

<!--
sub-PRD 的验收标准要求什么命令，就贴什么命令的输出——不是「测试通过了」，是把输出贴上来。
理由与产品哲学同源：把「你信不信 agent 说完成了」换成「跑一下就知道」。
UI 改动：贴前后截图。
-->

```
$ cargo test foundation::
...
```
