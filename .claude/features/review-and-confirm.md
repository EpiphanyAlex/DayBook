# 审核与确认

> 规格：[03 审核与草稿区](../../docs/prd/03-review.md) · 最后更新：2026-08-24

## 一句话

审核台把不可变来源原件、agent 的证据片段、草稿字段与合计报警放在同一屏；只有人触发的 Tauri command 能把草稿写入事实表。

## 数据流

```text
list_review_sources / list_active_drafts / read_evidence / check_source_total
  → src/App.tsx 三栏审核台
  → update_draft / discard_draft / confirm_draft(s)
  → src-tauri/src/domain/confirm.rs
  → transactions + draft consumed_at/discarded_at + human audit
```

## 关键文件

| 文件 | 职责 |
|---|---|
| `src/App.tsx` | 来源夹、原件灯箱、草稿卡、行内编辑、单条/批量确认与提示 |
| `src/styles.css` | evidence-first 三栏桌面布局与异常状态 |
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

- 原件整体可见，但没有截图区域高亮。2026-08-24 的 R1 产品链路 spike 证明 agent bbox 会误指相邻行，已决定**不**在 M1 增加伪精确高亮；当前完整原件 + `evidence_text` 并列就是截图来源的安全退路（[实测](../../docs/spikes/2026-08-24-r1-evidence-region.md)）。虚拟滚动与完整键盘流仍未实现。
- 三栏界面是 M0 功能基线，尚未按已定稿的 [`design.md`](../../design.md) v0.5 重做；`src/styles.css` 的 `:root` 变量不是 token design system。
- `evidence_text` 是 agent 的抽取声明，不是独立证据，所以截图或口述全文默认可见。
- 当前界面是证据检查台，不是传统记账表单；金额汇总不在 React 内计算。
- **改金额时不要再把本位币金额当独立输入**：v0.11 的 `edit_draft` 那样写，于是「把 AI 读错的 1680 改回 168」必然返回 `data.money_inconsistent`，而当时的验收测试改的是 `merchant`，门禁全绿。见 [03 §3.5](../../docs/prd/03-review.md)「本位币金额是导出值」。
- 卡片上的汇率输入是主单位小数（`1 USD = 1.538462 AUD`），`parseRateInput` 转成 `rate_ppm` 整数后过 IPC；**除法只出现在 `formatRate` 里**。
- 当前 `src/App.tsx::refreshSelected` 每次重取都会用全部草稿重建 `selectedDrafts`：用户取消一条后，编辑另一条触发刷新会把前者重新选中。当前又把所有来源的异步结果写进同一组 `drafts/evidence/check` state，快速切来源存在迟到响应覆盖风险；两条都是现实现边界，M1 的状态方案与验收见 [03 §3.8/§6](../../docs/prd/03-review.md)。

## 相关

- [总额交叉校验](./total-cross-check.md)
- [导入截图与口述](./ingest-screenshot.md)
- [前端规则](../rules/frontend.md)
