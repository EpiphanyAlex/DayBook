# 审核与确认

> 规格：[03 审核与草稿区](../../docs/prd/03-review.md) · 最后更新：2026-09-06

## 一句话

审核台把不可变来源原件、agent 的抽取声明、草稿字段与合计报警放在同一屏；只有人触发的 Tauri command 能把草稿写入事实表。

## 数据流

```text
list_review_sources / list_active_drafts / read_evidence / check_source_total
  → src/review/queries.ts（Query 缓存 Rust 投影）
  → src/App.tsx + screenReducer.ts（选择、焦点、编辑意图）
  → EvidencePane / DraftCard / ReconciliationCard 三栏审核台
  → update_draft / discard_draft / confirm_draft(s)
  → src-tauri/src/domain/confirm.rs
  → transactions + draft consumed_at/discarded_at + human audit
```

## 关键文件

| 文件 | 职责 |
|---|---|
| `src/App.tsx` | 来源夹、导入/解析动作与审核屏组装 |
| `src/review/queries.ts`、`src/lib/queryClient.ts` | IPC query key、请求身份封装、取消旧观察、定向重读、一次启动探测 |
| `src/review/screenReducer.ts` | 来源与焦点、按 attempt 的排除集合、带 revision 的编辑缓冲 |
| `src/review/mutations.ts` | 捕获来源/attempt/草稿 ID、提交互斥、写成功与重读失败分离 |
| `src/review/EvidencePane.tsx`、`src/review/evidenceSpan.ts` | 默认完整原件、当前声明、严格 code-point span、读取/解码失败 |
| `src/review/DraftCard.tsx`、`src/review/ReconciliationCard.tsx` | 行内编辑、补齐三元组、固定对账与确认区 |
| `src/styles/tokens.css`、`src/styles.css` | design.md 三层 token、cloud 的 rail/paper 区域、首次 composer 与三栏审核 |
| `src-tauri/src/domain/confirm.rs` | 查询 DTO、编辑、丢弃、单条/批量确认、事实表写入、人工审计 |
| `src-tauri/src/lib.rs` | 审核相关 Tauri commands；不被 MCP 模块引用 |
| `src/review/policy.ts` | 口述批量确认的三项 UI 展示 attestation |
| `src/review/AttestationHint.tsx` | `user_attested_batch` 的背书提示，与确认按钮同屏；`failed` 时同屏给出差额 |

## 数据结构

- 草稿原始值在 `drafted_json`，永不可更新；行内编辑只改结构化列并写 before/after 审计。
- **`DraftPatch` 没有 `base_amount_minor`**：本位币金额由 `amount_minor` + `currency` + `base_currency` + `rate_ppm` 经 `convert_minor` 导出（`domain/confirm.rs::edit_draft`）。三元组自洽由构造保证，不靠事后校验。
- 人工丢弃写 `discarded_at`，解析失败作废写 `voided_at`，确认写 `consumed_at`；三者互斥且都保留行。
- 事实交易通过 `source_draft_id` 回指草稿，并复制 `source_id` 与 `evidence_text`。

## 业务规则

- 确认服务端重新检查证据非空、三元组完整且自洽；UI 通过不能绕过。
- 单条确认不受合计失败阻挡；批量确认必须经过 `confirmation_policy`。
- 口述批量确认还必须由前端证明全文可见、结果并排、条数显式；服务端要求三项 attestation 都为真。
- **口述报了合计且对账 `failed` 时，批量确认仍然放行**（策略恒为 `user_attested_batch`），所以 `AttestationHint` 必须把差额与「由你背书」放在按钮旁 —— 这是全产品唯一一条「机器判定不符仍允许批量」的路径。
- **缺三元组的草稿能在卡片上当场补齐**：填本位币 + 汇率 → `update_draft` 导出 `base_amount_minor` → 该草稿随即可确认，不必丢弃后重解析整个来源。
- 批量中证据或三元组不完整的草稿逐条列为 rejected，其余可确认；策略级失败则整批拒绝。
- 所有活动草稿被确认或丢弃后，来源转 `reviewed`。

## 已知边界与坑

- **有限 M1 三项已实现并通过零额度验收**：[03 §3.9/§6](../../docs/prd/03-review.md) 的 token、Query + reducer 与完整原件证据；第一次 M0 `no_go` 与其余未开放范围保持不变。
- **M1 运行事件投影未实施，且不在有限三项内**：[03 §3.8/§6](../../docs/prd/03-review.md) 与 [01 §3.4/§6.2](../../docs/prd/01-agent-runtime.md) 已规定旧 attempt 事件隔离、重复进度不累计、重新订阅重取快照、展示截断明示；事件只驱动显示或 query 失效，不决定来源状态、确认策略与入账。面板关闭不取消解析，不抹掉用户的排除集合。
- 原件整体可见，但没有截图区域高亮。2026-08-24 的 R1 产品链路 spike 证明 agent bbox 会误指相邻行，已决定**不**在 M1 增加伪精确高亮；当前完整原件 + `evidence_text` 并列就是截图来源的安全退路（[实测](../../docs/spikes/2026-08-24-r1-evidence-region.md)）。虚拟滚动与完整键盘流仍未实现。
- 三栏迁移以 [`design.md`](../../design.md) v0.5 为 token 事实源；[v9 页面参考](../../docs/design/README.md) 只提供 01–03b 层级，未来导航与能力未带入。
- `evidence_text` 是 agent 的抽取声明，不是独立证据，所以截图或口述全文默认可见。
- 当前界面是证据检查台，不是传统记账表单；金额汇总不在 React 内计算。
- **改金额时不要再把本位币金额当独立输入**：v0.11 的 `edit_draft` 那样写，于是「把 AI 读错的 1680 改回 168」必然返回 `data.money_inconsistent`，而当时的验收测试改的是 `merchant`，门禁全绿。见 [03 §3.5](../../docs/prd/03-review.md)「本位币金额是导出值」。
- 卡片上的汇率输入是主单位小数字符串（`1 USD = 1.538462 AUD`），`parseRateInput` 转成 `rate_ppm` 整数后过 IPC；显示同样使用字符串切片。
- 来源 A 的迟到响应只留在 A 的 query key；草稿响应封装请求 source/attempt（空数组也不例外），每条 DTO 校验身份。换 attempt 时取消旧观察并重读，Query 取消不意味着 Rust command 停止。
- 选择从当前草稿减去按 `(sourceId, attemptId)` 保存的排除集合导出；切回来源和重读不抹除意图，新草稿默认选中，不做磁盘持久化。
- mutation 不自动重试，不抢回后来的来源选择；编辑完成只清除匹配 revision 的缓冲。失败保留「未保存」与重试/恢复入口，未保存时暂停确认。
- 写成功但投影重读失败显示「操作已完成，读取失败」；按钮只重读，不再发写命令。成功写入只刷新来源、草稿、该尝试合计，不重读不可变原件。
- 口述 span 只接受合法 code-point 区间与逐字匹配；失败保留全文及声明，不搜索替代。任一当前声明无法对应时暂停批量背书，仍可对照全文单条确认。截图读取或解码未就绪时暂停确认。
- 前端差额只对 Rust 返回的两侧整数做 BigInt 展示格式化，不重算草稿和或判定 policy。

## 相关

- [总额交叉校验](./total-cross-check.md)
- [导入截图与口述](./ingest-screenshot.md)
- [前端规则](../rules/frontend.md)
