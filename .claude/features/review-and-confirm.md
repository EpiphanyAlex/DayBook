# 审核与确认

> 规格：[03 审核与草稿区](../../docs/prd/03-review.md) · 最后更新：2026-08-13

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

## 数据结构

- 草稿原始值在 `drafted_json`，永不可更新；行内编辑只改结构化列并写 before/after 审计。
- 人工丢弃写 `discarded_at`，解析失败作废写 `voided_at`，确认写 `consumed_at`；三者互斥且都保留行。
- 事实交易通过 `source_draft_id` 回指草稿，并复制 `source_id` 与 `evidence_text`。

## 业务规则

- 确认服务端重新检查证据非空、三元组完整且自洽；UI 通过不能绕过。
- 单条确认不受合计失败阻挡；批量确认必须经过 `confirmation_policy`。
- 口述批量确认还必须由前端证明全文可见、结果并排、条数显式；服务端要求三项 attestation 都为真。
- 批量中证据或三元组不完整的草稿逐条列为 rejected，其余可确认；策略级失败则整批拒绝。
- 所有活动草稿被确认或丢弃后，来源转 `reviewed`。

## 已知边界与坑

- M0 原件整体可见，但不做截图区域高亮、虚拟滚动与完整键盘流；这些属于后续里程碑。
- M0 三栏界面是功能基线，不是批准后的设计稿；`src/styles.css` 里的 `:root` 变量不是 token design system。M1 开工前先确定设计稿与语义 token，再做视觉精修。
- `evidence_text` 是 agent 的抽取声明，不是独立证据，所以截图或口述全文默认可见。
- 当前界面是证据检查台，不是传统记账表单；金额汇总不在 React 内计算。

## 相关

- [总额交叉校验](./total-cross-check.md)
- [导入截图与口述](./ingest-screenshot.md)
- [前端规则](../rules/frontend.md)
