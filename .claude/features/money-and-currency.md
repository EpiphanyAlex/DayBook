# 金额与币种

> 规格：[00 地基](../../docs/prd/00-foundation.md) · 最后更新：2026-08-13

## 一句话

金额与汇率从 SQLite、Rust 到 Tauri IPC 都以受限整数表示，前端只在安全整数范围内做展示与编辑。

## 数据流

```text
agent 十进制整数字符串
  → src-tauri/src/domain/draft.rs（解析与范围校验）
  → src-tauri/src/money.rs（ISO exponent、换算、half-even、自洽检查）
  → SQLite INTEGER
  → DecimalI64 序列化为十进制字符串
  → src/lib/bridge.ts（再次校验后转成 TS 安全整数）
  → src/App.tsx（仅格式化，不做汇总）
```

## 关键文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/money.rs` | `DecimalI64`、`10^15` 范围、ISO 4217 exponent、跨精度换算、银行家舍入 |
| `src-tauri/migrations/0001_m0.sql` | INTEGER 列、三元组全有或全无、事实表三元组必填 |
| `src/lib/bridge.ts` | IPC 整数字段集中序列化/解析；组件不得另写解析器 |
| `src/App.tsx` | 按币种 exponent 格式化与编辑显示 |

## 业务规则

- 金额绝对值与 `rate_ppm` 不超过 `10^15`；IPC JSON 中必须是字符串，不能是 JSON number。
- 汇率以 `1_000_000` 为 1.0；换算同时考虑原币与本位币 exponent，并用 i128 中间值做 half-even 舍入。
- 草稿允许三元组全空；确认入事实表前必须补齐并自洽。
- 本位币保存在数据目录的 `preferences.json`。首次解析前必须由用户明确选择，不能按地区猜测。
- 切换本位币只影响之后的解析；历史事实行逐笔冻结，汇总通过 `Database::transaction_rollup_by_base_currency` 按本位币分组。

## 已知边界与坑

- TS 的 `number` 本身是浮点类型；安全来自 IPC 两端的整数与范围校验，不是类型名。
- 前端没有金额累加逻辑。需要汇总时必须在 Rust / SQLite 侧完成，并按 `base_currency` 分组。
- 币种表是显式白名单；未知代码返回 `data.unsupported_currency`，不回退到两位小数。

## 相关

- [金额与数据规则](../rules/money-and-data.md)
- [ADR-0004](../../docs/adr/0004-data-model-sqlite-integer-money.md)
- [审核与确认](./review-and-confirm.md)
