# 总额交叉校验

> 规格：[03 审核与草稿区](../../docs/prd/03-review.md) · 最后更新：2026-08-13

## 一句话

Rust 按一次解析尝试的全部未作废草稿重算支出、收入或净变动，并把对账状态与批量确认策略分别返回。

## 数据流

```text
agent report_source_total
  → parse_attempts.reported_total_*
  → domain::confirm::total_check(attempt_id)
  → reconciliation_status + confirmation_policy
  → src/App.tsx ReconciliationCard / 批量按钮 gate
```

## 关键文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/domain/confirm.rs::total_check` | 尝试范围求和、币种选择、合计类型等式、两维结果 |
| `src-tauri/src/domain/confirm.rs::confirm_batch` | 服务端重新校验策略，不信任 UI 按钮状态 |
| `src/review/policy.ts` | 前端只读取 `confirmationPolicy` 与口述三项展示 attestation |
| `src/App.tsx::ReconciliationCard` | 显示声明值、计算值、来源合计原文与报警 |

## 业务规则

- 求和键是 `attempt_id`，范围包含该尝试全部未作废草稿；已确认或人工丢弃仍参与，避免结果随审核动作漂移。
- 报告币种等于原币时用 `amount_minor`；否则只在完整三元组且 `base_currency` 匹配时用 `base_amount_minor`。
- `expense_total` 只加支出，`income_total` 只加收入，`net_change = income - expense`。transfer 或无法取得金额时结果为 `unavailable`。
- 文件没声明合计：`unavailable + single_only`。口述没声明合计：`not_applicable + user_attested_batch`。
- 口述说出合计时仍正常得到 `passed/failed`，但确认策略保持 `user_attested_batch`。
- 放行批量的只有 `confirmation_policy`：`reconciled_batch` 与 `user_attested_batch` 放行，`single_only` 拒绝。**`kind = utterance` 恒为 `user_attested_batch`，所以对账 `failed` 时批量仍然放行**——挡住 `failed` 的只有 `kind = file`（它拿不到 `user_attested_batch`）。逐条确认在任何状态下都可用；没有 force/ignore 旁路。

## 已知边界与坑

- `reconciliation_status` 和 `confirmation_policy` 是两个维度；用前者直接放行按钮是缺陷。
- 合计在 `parse_attempts`，不在 `sources`；重试不能覆盖上次合计。
- 余额不是声明合计，单独出现余额时必须判无法校验。

## 相关

- [审核与确认](./review-and-confirm.md)
- [金额与币种](./money-and-currency.md)
- [ADR-0002](../../docs/adr/0002-ai-never-writes-directly.md)
