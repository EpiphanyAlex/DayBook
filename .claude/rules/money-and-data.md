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
// ✅ 正确 —— 分支类型让「最小单位」和「主单位」不可混用
type MinorUnits = number & { readonly __brand: 'MinorUnits' }

function formatMoney(amount: MinorUnits, currency: string): string {
  // 除法只允许出现在这里，且除数由币种决定
  const exp = currencyExponent(currency)          // JPY→0 · AUD→2 · KWD→3
  return `${(amount / 10 ** exp).toFixed(exp)} ${currency}`
}

// ❌ 错误
const total = items.reduce((s, i) => s + i.amount / 100, 0)  // 浮点累加 + 写死 100
const amount = parseFloat(input)                              // 浮点入口
```

**为什么是「禁止」而不是「不推荐」**：[总额交叉校验](../../docs/adr/0002-ai-never-writes-directly.md)是唯一能在无人工介入下捕获错误的机制，它**依赖精确相等**。浮点让这个唯一的自动纠错机制失效。

### 1.1 「最小货币单位」不等于「分」

**不是所有货币都是两位小数**（[ADR-0004 §2](../../docs/adr/0004-data-model-sqlite-integer-money.md)、[`docs/prd/00-foundation.md` §3.4](../../docs/prd/00-foundation.md)「币种精度」）：ISO 4217 的 minor unit exponent 多数是 2，但 **JPY / KRW 是 0**、**KWD / BHD / JOD 是 3**。

```rust
// ✅ 正确 —— exponent 由币种决定，不入库；未知币种是错误，不是待猜的值
fn currency_exponent(code: &str) -> Result<u32, AppError> { /* ISO 4217 常量表；表外 → data.unsupported_currency */ }

// ❌ 错误 —— 写死两位小数
const MINOR_PER_MAJOR: i64 = 100;
format!("{}.{:02}", amount_minor / 100, amount_minor % 100)

// ❌ 也错 —— 未知币种回退到 2 并只记一条告警 <!-- legacy -->
fn currency_exponent(code: &str) -> u32 { TABLE.get(code).copied().unwrap_or(2) }
//    告警落在日志里没人看，而那条金额已经带着错误的 exponent 进了草稿、过了总额校验、
//    被人一眼扫过去确认入库 ——「有记录但没拦住」和「没记录」对用户是同一件事
```

**`amount_minor` 单看没有意义，必须和 `currency` 一起读。** 写死 `/ 100` 不只是显示难看——**在 JPY 上会把 9700 日元显示成 97.00 日元**。

**唯一允许除法的地方**：显示层的格式化函数，且除数是 `10^exponent`。**全仓库不应存在写死的 `/ 100`。**

### 1.2 金额过 IPC 是十进制字符串

```rust
// ✅ 正确 —— i64 序列化成字符串，超范围在序列化前就拒绝
#[derive(Serialize)]
struct DraftDto {
    #[serde(serialize_with = "money_as_string")]   // "168" / "-4500"
    amount_minor: i64,
    // …
}

// ❌ 错误 —— JSON 数字，JSON.parse 会把超 2^53 的值静默舍入
struct DraftDto { amount_minor: i64 }   // serde 默认序列化成数字
```

```typescript
// ✅ 正确 —— 解析与校验都只在 call<T> 里做一次
const v = Number(raw)
if (!Number.isSafeInteger(v) || Math.abs(v) > 1e15) throw appError('data.amount_out_of_range')

// ❌ 错误 —— 组件里第二次解析，或干脆不校验
const amount = Number(dto.amountMinor)
```

**适用 `amount_minor` · `base_amount_minor` · `rate_ppm` · `reported_total_minor`**，同一条规则不留例外。范围不变式 `|v| ≤ 10^15`，**IPC 两侧各校验一次**。

**为什么不是全链路 `bigint`**：JS 的 `number` 对 `≤ 2^53` 的整数是精确的，而前端本来就不做金额累加（§6 的汇总一律由 Rust 给出）。字符串边界要挡的是**唯一一条静默路径**——`JSON.parse` 把 agent 读错成 20 位的数字悄悄舍入。完整论证与迁移后路见 [`docs/prd/00-foundation.md` §3.4](../../docs/prd/00-foundation.md)「金额怎么过 IPC」。

## 2. 汇率：定点整数

**`rate_ppm` 是「1 主单位原币 = 多少主单位本位币」× 1_000_000**——就是账单上印的、用户会手填的那个数。因此**换算公式必须带两边的 exponent**，不能直接乘。

```rust
// ✅ 正确
const RATE_SCALE: i128 = 1_000_000;

fn to_base(amount_minor: i64, rate_ppm: i64, from: &str, to: &str) -> i64 {
    // 先乘后除，i128 中间量防溢出，banker's rounding
    let num = (amount_minor as i128) * (rate_ppm as i128) * 10_i128.pow(currency_exponent(to));
    let den = RATE_SCALE * 10_i128.pow(currency_exponent(from));
    round_half_even_i128(num, den)
}

// ❌ 错误
fn to_base(amount: f64, rate: f64) -> f64 { amount * rate }             // 浮点
fn to_base(a: i64, rate_ppm: i64) -> i64 { a * rate_ppm / 1_000_000 }   // 截断而非舍入，i64 可能溢出，
                                                                        // 且漏了 exponent：AUD→JPY 结果大 100 倍
```

**两边 exponent 相同时（绝大多数情况）退化成 `amount_minor × rate_ppm / 1_000_000`**——所以带上 exponent 项**不会改变现有的任何一个正确结果**，只会修好 JPY / KWD 那些错的。

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
    let expected = to_base(tx.amount_minor, tx.rate_ppm, &tx.currency, &tx.base_currency);
    if tx.base_amount_minor != expected {
        return Err(AppError::money_inconsistent(expected, tx.base_amount_minor));
    }
    Ok(())
}
```

### 3.1 本位币金额是导出值，不是独立输入

**编辑路径上，自洽要由构造保证，不能靠一次事后校验挡住**（2026-08-13，[`docs/prd/03-review.md` §3.5](../../docs/prd/03-review.md)「本位币金额是导出值」）。

```rust
// ❌ 错误 —— 三者都当独立输入，于是「互相矛盾」是一个可表达的状态
let amount = patch.amount_minor.unwrap_or(before.amount_minor);
let base   = patch.base_amount_minor.or(before.base_amount_minor);   // 用户只改了 amount
validate_triple(amount, &currency, base, &base_currency, rate)?;     // 必然 money_inconsistent

// ✅ 正确 —— rate_ppm 是来源上印的事实，base 只能算出来
let base = match (base_currency, rate_ppm) {
    (Some(currency), Some(rate)) => Some(to_base(amount, rate, &original_currency, currency)?),
    (None, None)                 => None,
    _ => return Err(AppError::new("review.incomplete_triple", "本位币金额、币种与汇率必须全填或全空")),
};
```

**`rate_ppm` 是账单上印着、用户会手填的那个数**（§2）——用户纠正金额时它不变，所以本位币金额只能跟着重算。**这不是「少一个输入框」的取舍**：M0 曾把它当独立字段，于是「把 AI 读错的 1680 改回 168」这条审核界面的主路径**必然**返回 `data.money_inconsistent`，而当时的验收测试改的是 `merchant`，门禁全绿。

**判据**：能不能构造出一个「三元组不自洽」的入参？能，说明本位币金额还是输入而不是导出值。

**推论**：补齐 `base_currency` + `rate_ppm` 就能让缺三元组的草稿变得可确认——所以**界面必须提供这两个输入**，否则 `review.incomplete_triple` 是一条用户点不动的死路。

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

> ⚠️ **这个判据只覆盖了一半**（2026-08-10 补）：它问的是**我们注册的**工具，而 agent CLI 自带内置工具（执行命令、读写文件），一条 `sqlite3 daybook.db "INSERT INTO transactions …"` 就绕过全部四道闸门，**而上面这份清单照样是干净的**。另一半在 [`rust-tauri.md` §5.1](./rust-tauri.md)「密封启动配置」与 [`docs/prd/01-agent-runtime.md` §3.7](../../docs/prd/01-agent-runtime.md)：**子进程必须密封启动，且有效工具集要实测**。两半缺一，另一半就是装饰。

### 4.1 agent 的原始起草值不可变

```rust
// ✅ 正确 —— 起草时由 Rust 侧写一次快照，此后永不更新
let drafted_json = serde_json::to_string(&args)?;
store.insert_draft(args, drafted_json)?;

// ❌ 错误 —— 人的编辑覆盖掉「AI 当初写的是什么」
"UPDATE draft_transactions SET amount_minor = ?1, drafted_json = ?2 WHERE id = ?3"
```

**审核界面的行内编辑改业务列，不改 `drafted_json`。** 它同时是审计的「当初起草成什么样」与 [`docs/prd/07-eval.md` §3.2](../../docs/prd/07-eval.md) 的 eval 真值——**用户把 1680 改回 168 之后，如果草稿行被就地改写，错误率的度量恒为零**。约束强度与 `audit_log` 的 append-only 同级（[ADR-0002](../../docs/adr/0002-ai-never-writes-directly.md) 硬性要求 7）。

## 5. 证据链：草稿必带来源

```rust
// ✅ 正确 —— 工具层与数据层各一道
struct DraftTransactionArgs {
    source_id: Uuid,        // 必填，不是 Option
    evidence_text: String,  // 必填，不是 Option
    // …
}
```

**`evidence_text` 是抽取声明，不是独立证据**（2026-08-10，[ADR-0002 闸门 2](../../docs/adr/0002-ai-never-writes-directly.md)）。它和被核对的金额**出自同一次模型输出**——模型把 168 读成 1680 时，也会把 `evidence_text` 写成「1680」，两者自洽却一起错。

- **证据是不可变原件**（截图字节 / 转写文本），`evidence_text` 只是「原件上的哪个位置」的指针
- **审核界面必须让原件本身可见**，不能只渲染 `evidence_text` 那一列——否则用户核对的是模型和它自己（[`frontend.md` §5](./frontend.md)）
- 因此**不要在代码或注释里把 `evidence_text` 叫「原文」**，那个词属于原件

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
// ✅ 正确 —— 放行看的是确认策略，不是对账状态
fn confirm_batch(&self, attempt_id: Uuid, ids: &[Uuid]) -> Result<(), AppError> {
    let check = self.total_check(attempt_id)?;
    match check.confirmation_policy {
        Policy::ReconciledBatch | Policy::UserAttestedBatch => self.do_confirm(ids),
        Policy::SingleOnly => Err(match check.reconciliation_status {
            Status::Failed { .. } => AppError::total_mismatch(),
            _ => AppError::total_unavailable(),          // 不伪装成通过
        }),
    }
}

// ❌ 错误
fn confirm_batch(&self, ids: &[Uuid], force: bool) -> Result<(), AppError> {
    if !force && self.total_check(...)? != Passed { /* … */ }   // force 就是旁路
}
fn confirm_batch(...) { if status == NotApplicable { self.do_confirm(ids) } }
// ↑ 也是错的：把「能不能对账」当成了「能不能批量确认」，两者是两个维度
```

**等式之前先守 claim scope。** M0 的 `report_source_total` 只允许当前不可变来源全部适用交易的一条 claim；任意 viewport 截图仍接受，但月度 viewport 外、分页、按日 / 分类 / 单笔语义 / 子组合计不得报告。合计词只是候选，不能强制调用；同三元组 invalid decoy 因现有四列无法审计身份也不得报告。生产保持单 claim 四列，formal 用 bounded `candidateClaims` 保证三元组唯一、再以 expected amount/currency/kind 锁定 eligible claim，scope-invalid=0（含错报同源 decoy）由真值契约守（[00 地基 §3.6](../../docs/prd/00-foundation.md)、[07 评测 §3.4](../../docs/prd/07-eval.md)）。

**`NotApplicable` 与 `UserAttestedBatch` 只属于 `kind = utterance`**（[`docs/prd/03-review.md` §3.3](../../docs/prd/03-review.md)）。`kind = file` 取不到合计是**信号**，必须落在 `Unavailable` + `SingleOnly`——把它也放行等于闸门 3 白做。

### 6.1 校验的入参是 `attempt_id`，求和范围是「该次尝试的全部未作废草稿」

```rust
// ❌ 错误 —— 逐条确认掉一条后，剩余和永远小于合计，该来源再也回不到 passed
"SELECT ... FROM draft_transactions WHERE source_id = ?1 AND consumed_at IS NULL"

// ❌ 也错 —— 按来源求和，重试后两次尝试的草稿会混在一起
"SELECT ... FROM draft_transactions WHERE source_id = ?1 AND voided_at IS NULL"

// ✅ 正确 —— 校验是「这次尝试的解析完整性」，确认动作不改变它
"SELECT ... FROM draft_transactions WHERE attempt_id = ?1 AND voided_at IS NULL"
```

**总额校验回答的是「agent 这一次从来源读出来的东西，对不对得上它这一次报告的合计」**，与「这一批要确认哪些」正交。合计存在 `parse_attempts.reported_total_*`——**它和草稿一样是那次尝试的输出，不是来源的属性**（[`docs/prd/00-foundation.md` §3.6](../../docs/prd/00-foundation.md)）。

**求和还要按 `reported_total_kind` 选等式**（`expense_total` / `income_total` / `net_change`），并在存在 `direction = transfer` 的条目时返回 `Unavailable`——转账在 schema 里没有符号，硬算等于编一个。

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
rg -n 'UPDATE[^;]*drafted_json' src-tauri/src                    # 应无命中（§4.1 起草值不可变）
rg -n 'execute_sql|raw_query|write_file' src-tauri/src/mcp        # 应无命中
rg -n '/ 100|\* 100|100\.0' src-tauri/src src                     # 只应命中 exponent 表实现（§1.1）
rg -n 'consumed_at IS NULL' src-tauri/src/domain                  # 总额校验路径上应无命中（§6.1）
rg -n 'declared_total' src-tauri/src                             # 应无命中——已改名 reported_total_* 并移到 parse_attempts
rg -n 'unwrap_or\(2\)|unwrap_or_default' src-tauri/src            # exponent 查表处不应命中（未知币种要报错）
rg -n 'amount_minor.*: i64' src-tauri/src/commands                # DTO 上应带字符串序列化标注（§1.2）
```
