# 金额与数据规则

> 依据 [ADR-0004 数据模型](../../docs/adr/0004-data-model-sqlite-integer-money.md) 与 [ADR-0002 AI 永不直接写入](../../docs/adr/0002-ai-never-writes-directly.md)。
> 对应仓库根 [`CLAUDE.md`](../../CLAUDE.md) 约束 3、4、5、6、7、8。
> **这一份的规则违反了就是缺陷，不是风格偏好。**

## 1. 金额：整数最小货币单位，禁止浮点

**任何位置禁止用浮点数表示金额——包括中间计算、IPC 传输、测试夹具。**

```rust
// ✅ 正确
struct Transaction {
    amount_minor: i64,        // 分 / cent
    currency: String,         // 原币，ISO 4217："AUD" / "CNY"
    base_amount_minor: i64,
    base_currency: String,    // 本位币可切换 ⇒ 逐笔冻结，不能只靠全局设置
    rate_ppm: i64,            // 汇率 × 1_000_000
}

// ❌ 错误
struct Transaction {
    amount: f64,              // 0.1 + 0.2 != 0.3，总额校验会永远差一分
    amount_str: String,       // 字符串金额要反复解析，且没有类型保护
}
```

```typescript
// ✅ 正确 —— 分支类型让「分」和「元」不可混用
type MinorUnits = number & { readonly __brand: 'MinorUnits' }

function formatMoney(amount: MinorUnits, currency: string): string {
  // 除法只允许出现在这里
  return `${(amount / 100).toFixed(2)} ${currency}`
}

// ❌ 错误
const total = items.reduce((s, i) => s + i.amount / 100, 0)  // 浮点累加
const amount = parseFloat(input)                              // 浮点入口
```

**为什么是「禁止」而不是「不推荐」**：[总额交叉校验](../../docs/adr/0002-ai-never-writes-directly.md)是唯一能在无人工介入下捕获错误的机制，它**依赖精确相等**。浮点让这个唯一的自动纠错机制失效。

**唯一允许除法的地方**：显示层的格式化函数。它应该是全仓库唯一一处 `/ 100`。

## 2. 汇率：定点整数

```rust
// ✅ 正确
const RATE_SCALE: i64 = 1_000_000;

fn to_base(amount_minor: i64, rate_ppm: i64) -> i64 {
    // 先乘后除，i128 中间量防溢出，banker's rounding
    round_half_even_i128((amount_minor as i128) * (rate_ppm as i128), RATE_SCALE as i128)
}

// ❌ 错误
fn to_base(amount: f64, rate: f64) -> f64 { amount * rate }   // 浮点
fn to_base(a: i64, rate_ppm: i64) -> i64 { a * rate_ppm / 1_000_000 }  // 截断而非舍入，且 i64 可能溢出
```

**原币与本位币相同时 `rate_ppm = 1_000_000`，走同一条代码路径，不设特例分支。**

```rust
// ❌ 错误 —— 特例分支迟早会漏
if tx.currency == tx.base_currency {
    tx.base_amount_minor = tx.amount_minor;   // 绕过了自洽校验
} else {
    tx.base_amount_minor = to_base(tx.amount_minor, tx.rate_ppm);
}

// ✅ 正确 —— 一条路
tx.base_amount_minor = to_base(tx.amount_minor, tx.rate_ppm);  // rate_ppm 为 1_000_000 时结果相同
```

## 2.1 本位币可切换 ⇒ 汇总必须分组

本位币**逐笔冻结**（[`docs/prd/00-foundation.md` §3.4](../../docs/prd/00-foundation.md)「本位币切换语义」）。用户切换本位币后，库里会同时存在两种 `base_currency` 的历史行。

```rust
// ❌ 错误 —— 把两种本位币的金额加在一起，得到一个无意义的数字
"SELECT SUM(base_amount_minor) FROM transactions WHERE occurred_on BETWEEN ?1 AND ?2"

// ✅ 正确 —— 按本位币分组，由上层决定怎么呈现
"SELECT base_currency, SUM(base_amount_minor) FROM transactions
   WHERE occurred_on BETWEEN ?1 AND ?2 GROUP BY base_currency"
```

**任何 `SUM(base_amount_minor)` 都必须伴随 `GROUP BY base_currency`，或伴随「结果集只含一种本位币」的显式断言。** 切换本位币**不改动任何历史行**——追溯换算需要历史汇率（无可靠来源），且改写已确认的事实数据违反 [ADR-0002](../../docs/adr/0002-ai-never-writes-directly.md)。

## 3. 三元组必须自洽

写入前校验，不满足则返回 `data.money_inconsistent`。**不得只存换算后的结果**——那样就回不到「截图上写的到底是多少」，证据链断裂。

```rust
// ✅ 正确
fn validate(tx: &Transaction) -> Result<(), AppError> {
    let expected = to_base(tx.amount_minor, tx.rate_ppm);
    if tx.base_amount_minor != expected {
        return Err(AppError::money_inconsistent(expected, tx.base_amount_minor));
    }
    Ok(())
}
```

## 4. AI 只写草稿表

```rust
// ✅ 正确 —— MCP 工具只能触及 draft_*
#[tool]
async fn draft_transaction(&self, args: DraftTransactionArgs) -> Result<DraftId> {
    self.store.insert_draft_transaction(args)   // 只有这一个目的地
}

// ❌ 错误 —— 任何一种都是缺陷
#[tool]
async fn create_transaction(...)          // 直接写事实表
#[tool]
async fn confirm_draft(...)               // 形式合规、实质绕过：确认动作只能由人触发
#[tool]
async fn execute_sql(query: String)       // 通用 SQL 工具，绝对禁止
#[tool]
async fn write_file(path: String, ...)    // 通用文件写入，绝对禁止
```

**判据**：把工具清单列出来，问「这个工具能不能让一条未经人确认的数据出现在 `transactions` 或 `items` 里？」——能，就是缺陷。

## 5. 证据链：草稿必带来源

```rust
// ✅ 正确 —— 工具层与数据层各一道
struct DraftTransactionArgs {
    source_id: Uuid,        // 必填，不是 Option
    evidence_text: String,  // 必填，不是 Option
    // …
}
```

```sql
-- ✅ 正确 —— 数据层非空约束，不依赖应用层自觉
CREATE TABLE draft_transactions (
    id            TEXT PRIMARY KEY,
    source_id     TEXT NOT NULL REFERENCES sources(id),
    evidence_text TEXT NOT NULL,
    ...
);
```

**没有例外。** `draft_items` 与 `draft_transactions` 用同一套约束：两个字段都非空。用户口述的事项也有来源——那段话本身就是一条 `kind = utterance` 的 `sources`（转写文本落盘成 `.txt`，[`docs/prd/00-foundation.md` §3.6](../../docs/prd/00-foundation.md)「来源不等于文件」）。

> `docs/prd/05-items.md` v0.1–v0.3 曾写「`draft_items.source_id` 可空」，**已于 2026-08-09 删除**（[05 §3.4](../../docs/prd/05-items.md)）——它与 [ADR-0002](../../docs/adr/0002-ai-never-writes-directly.md) 硬性要求 2 冲突，且成文于 `utterance` 引入之前。看到旧措辞按本文为准。

## 6. 总额交叉校验不留旁路

```rust
// ✅ 正确
fn confirm_batch(&self, source_id: Uuid, ids: &[Uuid]) -> Result<(), AppError> {
    match self.total_check(source_id)? {
        TotalCheck::Passed => self.do_confirm(ids),
        TotalCheck::Failed { .. } => Err(AppError::total_mismatch()),
        TotalCheck::Unavailable => Err(AppError::total_unavailable()),  // 不伪装成通过
    }
}

// ❌ 错误
fn confirm_batch(&self, ids: &[Uuid], force: bool) -> Result<(), AppError> {
    if !force && self.total_check(...)? != Passed { /* … */ }   // force 就是旁路
}
```

## 7. 审计日志 append-only

```rust
// ✅ 正确 —— 只有 insert
impl AuditLog {
    fn record(&self, entry: AuditEntry) -> Result<()> { /* INSERT */ }
}

// ❌ 错误
"UPDATE audit_log SET ..."     // 可更新的审计日志不是审计日志
"DELETE FROM audit_log ..."
```

**两条写入路径都要记**：agent 经 MCP 工具写草稿（`actor = "agent"`），人工确认与修改（`actor = "human"`）。

## 8. 标识与时间

```rust
// ✅ 正确
id: Uuid,                          // UUID v4，TEXT 存小写带连字符
created_at: String,                // RFC 3339 UTC："2026-08-06T04:12:00Z"
occurred_on: String,               // "YYYY-MM-DD"，本地日历日，不带时区

// ❌ 错误
id: i64,                           // 自增整数，改类型是全表迁移
created_at: i64,                   // Unix 时间戳，人读不了、时区语义丢失
occurred_on: DateTime<Utc>,        // 业务日期不该带时区：「8 月 3 号那笔」是本地日历日
```

## 9. 快速自查

改完涉及金额或数据层的代码，跑：

```bash
rg -n 'f32|f64' src-tauri/src                                    # 金额模块应无命中
rg -n 'SUM\(base_amount_minor\)' src-tauri/src                   # 每处都应带 GROUP BY base_currency
rg -n 'UPDATE\s+audit_log|DELETE\s+FROM\s+audit_log' src-tauri/src   # 应无命中
rg -n 'execute_sql|raw_query|write_file' src-tauri/src/mcp        # 应无命中
rg -n '/ 100|\* 100' src/                                         # 前端只应命中格式化函数
```
