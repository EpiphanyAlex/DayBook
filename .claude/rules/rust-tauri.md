# Rust / Tauri 规则

> 依据 [ADR-0001 本地优先桌面平台](../../docs/adr/0001-local-first-desktop-platform.md) 与 [ADR-0003 Agent 运行时](../../docs/adr/0003-agent-runtime-and-pluggable-backend.md)。
> 对应仓库根 [`CLAUDE.md`](../../CLAUDE.md) 约束 1、2、9、10、11、15、16。
> 金额与数据层的规则在 [`money-and-data.md`](./money-and-data.md)，不在本文重复。

## 1. 分层：command → domain → store

```
src-tauri/src/
├── lib.rs          Tauri 应用装配、插件注册
├── commands/       前端能调的一切；参数校验；返回 Result<T, AppError>
├── domain/         业务规则：总额校验、确认动作、状态机
├── mcp/            MCP server 与工具面（rmcp）
├── agent/          agent launcher + 可插拔后端
└── store/          rusqlite 访问、迁移、证据文件读写
```

**依赖方向单向**：`commands → domain → store`，`mcp → domain → store`。

```rust
// ❌ 错误 —— command 直接发 SQL，业务规则会散落到入口层
#[tauri::command]
fn confirm_draft(id: String, db: State<Db>) -> Result<(), AppError> {
    db.execute("INSERT INTO transactions SELECT * FROM draft_transactions WHERE id = ?1", [id])?;
    Ok(())
}

// ✅ 正确 —— command 只做参数校验与转发
#[tauri::command]
fn confirm_draft(id: String, app: State<AppCtx>) -> Result<(), AppError> {
    let id = Uuid::parse_str(&id).map_err(|_| AppError::invalid_argument("id"))?;
    app.domain.confirm_draft(id)          // 校验、审计、写库都在 domain
}
```

## 2. 平台边界

```rust
// ❌ 以下任何一种都需要先改 ADR-0001，否则是缺陷
tokio::net::TcpListener::bind("127.0.0.1:3000")   // localhost HTTP API
std::process::Command::new("node")                 // 内嵌 Node 服务
reqwest::get("https://api.example.com/rates")      // 应用自己发出站请求
```

**唯一允许的出站流量**是用户自己的 agent CLI 与其模型服务商之间的通信，由该 CLI 自行发起。**应用不代理、不转发、不记录。**

**唯一允许 spawn 的子进程**是 agent CLI（以及 v1.1 的 Swift sidecar，[ADR-0005](../../docs/adr/0005-voice-and-system-integration.md)）。

## 3. 错误契约

所有 command 返回 `Result<T, AppError>`。序列化形状固定：

```rust
#[derive(Serialize)]
pub struct AppError {
    pub code: String,        // "data.migration_drift" —— 稳定，前端按它分支
    pub message: String,     // 面向用户的中文说明
    pub detail: Option<Value>, // 可选结构化补充；不含敏感数据
}
```

```rust
// ❌ 错误
fn cmd() -> Result<T, String>                  // 字符串错误，前端只能解析文案
fn cmd() -> Result<T, anyhow::Error>           // 不可序列化，且丢失 code
panic!("source not found")                     // panic 穿过 IPC 会杀掉整个命令
.unwrap()                                      // 同上

// ✅ 正确
fn cmd() -> Result<T, AppError>
Err(AppError::not_found("source", id))
```

**命名空间**：`data.*` · `ingest.*` · `review.*` · `agent.*` · `memory.*`。完整码集见 [`docs/prd/00-foundation.md` §3.7](../../docs/prd/00-foundation.md)。

## 4. MCP 工具面

**权限边界就是工具签名。** 详细的 ❌/✅ 见 [`money-and-data.md` §4](./money-and-data.md)。三条底线：

1. 工具集里不存在通用 SQL / 通用文件写入 / 通用命令执行工具
2. 工具集里不存在能触及 `transactions` / `items` 的工具
3. `domain::confirm` 不被任何 MCP 工具调用

```rust
// ✅ 正确 —— 工具的写入目标在类型上就是收窄的
impl DraftTools {
    async fn draft_transaction(&self, args: DraftTransactionArgs) -> Result<DraftId> {
        self.draft_store.insert(args)   // DraftStore 没有写事实表的方法
    }
}
```

**做法**：给草稿区一个独立的 store 类型（`DraftStore`），它**根本没有**写事实表的方法。这样越权在编译期就不可表达，而不是靠 review 发现。

## 5. Agent 后端可插拔

```rust
// ✅ 正确 —— 上层只见 trait
struct AgentLauncher { backend: Box<dyn AgentBackend> }

// ❌ 错误 —— Claude Code 成了其他代码的直接依赖
struct AgentLauncher { claude: ClaudeCodeBackend }
```

**绝不出现在代码里**：任何厂商 API key、endpoint、登录流程、凭证文件读取。

```rust
// ❌ 错误
const ANTHROPIC_API_KEY: &str = "sk-ant-...";
let creds = fs::read_to_string(home.join(".claude/.credentials.json"))?;  // 读用户凭证
```

## 6. 控制流由代码决定

状态机、确认点、重试策略是**确定性的 Rust 代码**。LLM 只做抽取、解析、分类与起草。

```rust
// ❌ 错误 —— 让模型决定业务动作
if agent_says("should_confirm") { self.confirm(ids)?; }

// ✅ 正确 —— 代码判断
match self.total_check(source_id)? {
    TotalCheck::Passed => self.confirm(ids)?,
    _ => return Err(AppError::total_mismatch()),
}
```

## 7. SQLite 使用

```rust
// ✅ 正确
conn.execute("PRAGMA journal_mode = WAL", [])?;
conn.execute("PRAGMA foreign_keys = ON", [])?;
conn.execute("INSERT INTO sources (id, content_hash) VALUES (?1, ?2)", params![id, hash])?;

// ❌ 错误
format!("INSERT INTO sources (id) VALUES ('{}')", id)   // 字符串拼接 SQL
```

- 迁移只前进不回滚，用 `PRAGMA user_version` 记录进度
- **多步写入必须在同一事务里**——尤其「写草稿 + 写审计」与确认动作的三步：**标记草稿已消费（`consumed_at` 置非空）+ 写事实表 + 写审计**
- **确认不删草稿。** 草稿行原样保留，只置 `consumed_at`——审计要能回答「入库的这条当初 AI 起草成什么样」，删了就答不了（[`docs/prd/03-review.md` §3.1](../../docs/prd/03-review.md)）

```rust
// ✅ 正确
let tx = conn.transaction()?;
tx.execute(INSERT_TRANSACTION, ...)?;
tx.execute(INSERT_AUDIT, ...)?;
tx.commit()?;
```

## 8. 日志与隐私

**日志落盘，分两级** —— 依据 [ADR-0007 本地可观测性与日志分级](../../docs/adr/0007-local-observability-and-log-tiers.md)。**本条推翻了旧规则「不落盘」**：不落盘则「查日志 → 复现 bug → 变成回归测试」这条链不成立（进程一退内存缓冲就没了）。

| 级别 | 内容 | 写入路径上**不得出现** |
|---|---|---|
| `trace` | 工具名与**参数形状**、耗时、退出码、重试次数、状态机转移、`agent_session_id`、`backend_id`、usage 元数据 | 金额字段、`evidence_text`、prompt 文本 |
| `debug` | `trace` 全部 + 完整提示词 + agent 原始输出 + **完整**工具调用参数 | —（`debug` 就是为取证而存在，会含账目细节） |

```rust
// ❌ 错误 —— 把内容写进 trace 级
tracing::info!("parsed transaction: {:?}", tx);   // 整个 struct = 金额 + 商户 + 原文
tracing::info!(total = tx.amount_minor, "confirmed");

// ✅ 正确 —— trace 只记形状
tracing::info!(source_id = %id, draft_count = n, elapsed_ms = ms, "parse finished");

// ✅ 正确 —— 内容只在 debug 级，且走独立的落盘通道
if log_level >= LogLevel::Debug { debug_sink.record_tool_call(name, &raw_args); }
```

- 位置 `<数据目录>/logs/`，与 SQLite 和 `evidence/` 同级——**用户看得见、能自己删**
- 一次 agent 会话一个 JSONL 文件，文件名含 `agent_session_id`；**默认保留期后自动清除**
- **`debug` 的默认值分构建**：发布构建默认关，开发构建（`npm run tauri dev` / `cargo` debug profile）默认开——夹具导出依赖它。两种情况下开关都必须在 UI 上可见并注明「会记录完整账目细节」
- **绝不上传、绝不上报、绝不代理转发**。ADR-0001 禁的是「数据离开本机」，不是「数据写进本机磁盘」——这个区分是日志落盘成立的基础

**判据**：`trace` 级的写入路径上有没有金额、`evidence_text` 或 prompt 文本？有就是缺陷。

## 9. 门禁

改完 Rust 代码，三条都要绿（[`CLAUDE.md`](../../CLAUDE.md) 约束 16）：

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

**`-D warnings` 是硬要求**——不允许 `#[allow(...)]` 掉一片，个别必要的 allow 要带注释说明理由。
