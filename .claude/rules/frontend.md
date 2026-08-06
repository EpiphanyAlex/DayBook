# 前端规则（React + TypeScript）

> 依据 [ADR-0001 本地优先桌面平台](../../docs/adr/0001-local-first-desktop-platform.md) 与 [`docs/prd/03-review.md`](../../docs/prd/03-review.md)。
> 对应仓库根 [`CLAUDE.md`](../../CLAUDE.md) 约束 1、2、6、16。
> 金额的处理规则在 [`money-and-data.md`](./money-and-data.md)，不在本文重复。

## 1. 唯一通道是 Tauri command

```typescript
// ✅ 正确 —— 统一经一层 call 包装
import { call } from '@/lib/bridge'
const drafts = await call<Draft[]>('list_drafts', { sourceId })

// ❌ 错误
await fetch('http://localhost:3000/drafts')   // 没有 localhost API，也不许有
await invoke('list_drafts', ...)              // 裸 invoke：错误形状不统一
import Database from 'tauri-plugin-sql-api'   // 前端直连 SQLite，破坏分层
```

`call<T>` 负责把 Tauri 的错误规整成 `AppError` 形状。

## 2. 按 `code` 分支，不解析文案

```typescript
// ✅ 正确
try {
  await call('confirm_batch', { sourceId, ids })
} catch (e) {
  const err = e as AppError
  if (err.code === 'review.total_mismatch') showTotalWarning(err.detail)
  else showGenericError(err.message)
}

// ❌ 错误
if (err.message.includes('总额')) { ... }   // 文案改了就挂
```

## 3. 业务规则不在前端

总额校验、状态机、确认条件、三元组自洽——**全在 Rust 侧**。前端只做体验层校验（输入框格式、必填提示）。

```typescript
// ❌ 错误 —— 在前端判断能不能入库
if (sum(drafts) === source.declaredTotal) { await call('confirm_batch', ...) }

// ✅ 正确 —— 服务端是唯一判据；前端只是提前给用户看结果
const check = await call<TotalCheck>('total_check', { sourceId })
setBatchEnabled(check.status === 'passed')
// 但即使前端算错了，服务端仍会拒绝
```

## 4. 金额

```typescript
// ✅ 正确
type MinorUnits = number & { readonly __brand: 'MinorUnits' }
<td>{formatMoney(tx.amountMinor, tx.currency)}</td>

// ❌ 错误
<td>{(tx.amount / 100).toFixed(2)}</td>   // 除法散落在组件里
const total = drafts.reduce((s, d) => s + d.amount / 100, 0)  // 浮点累加
```

**IPC 传的是整数**——不是格式化字符串、不是浮点。`/ 100` 只允许出现在格式化函数里。

## 5. 审核界面：键盘优先

这一屏是产品的胜负手（[`docs/prd/03-review.md`](../../docs/prd/03-review.md)），**40 笔 30 秒**是判定标准。

```typescript
// ✅ 正确 —— 键位表集中定义，行为可测
const KEYMAP = {
  ArrowUp: 'focus-prev', ArrowDown: 'focus-next',
  ' ': 'toggle-select', Enter: 'edit', Escape: 'cancel',
  'cmd+Enter': 'confirm-selected', 'cmd+a': 'select-all', d: 'discard',
} as const

// ❌ 错误
<div onClick={...}>   // 只能点，不能键盘走
```

三条硬要求：

1. **默认全选**——多数条目是对的，用户的动作应该是「取消掉不对的」
2. **行内编辑**，不弹模态框
3. **原文与解析结果并排**，默认可见，不是「点开看大图」

## 6. 性能

```typescript
// ✅ 正确 —— 数百条起用虚拟滚动，证据图按需加载
<VirtualList items={drafts} rowHeight={44} renderRow={...} />
<img src={evidenceUrl} loading="lazy" />

// ❌ 错误
{drafts.map(d => <Row key={d.id} {...d} />)}   // 500 条全渲染
{drafts.map(d => <img src={d.evidenceUrl} />)} // 一次性读进全部原图
```

## 7. 无网络、无遥测

```typescript
// ❌ 以下任何一种都直接违反 CLAUDE.md 约束 2
fetch('https://...')
new WebSocket('wss://...')
import posthog from 'posthog-js'
import * as Sentry from '@sentry/react'
<script src="https://cdn.../analytics.js">
```

**依赖也要看**：引入任何新 npm 包前，确认它不在运行时发请求、不上报使用数据。

## 8. 命名与类型

```typescript
// ✅ 正确
interface Draft { id: string; amountMinor: MinorUnits; evidenceText: string }
const isLoading = true
const canConfirm = check.status === 'passed'
const TOTAL_STATUS = { PASSED: 'passed', FAILED: 'failed', UNAVAILABLE: 'unavailable' } as const

// ❌ 错误
const data: any = await call('list_drafts')      // any 让 IPC 契约失效
if (status === 'passed') { ... }                  // 魔法字符串
const loading = true                              // 布尔名不像布尔
```

- **不用 `any`**——IPC 返回类型必须显式声明，那是前后端契约的唯一体现
- 状态字符串定义为常量对象 + `as const`，不散写字面量
- 组件文件 `PascalCase.tsx`，其余 `camelCase.ts`

## 9. 两个视图不混显

**时间轴是共同骨架，但 UI 分两个视图**——**不在同一张日历里混着显示钱和时间**（[ADR-0004 §4](../../docs/adr/0004-data-model-sqlite-integer-money.md)）。交易组件与事项组件不共用同一个列表容器。

## 10. 门禁

改完前端代码，四条都要绿（[`CLAUDE.md`](../../CLAUDE.md) 约束 16）：

```bash
npm run lint
npm run typecheck
npm test
npm run build
```
