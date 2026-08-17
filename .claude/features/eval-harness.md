# 评测与夹具重放

> 规格：[docs/prd/07-eval.md](../../docs/prd/07-eval.md) · 阈值与口径：[docs/PRD.md §9.4](../../docs/PRD.md) · 最后更新：2026-08-17

## 一句话

把「agent 那次读出来的东西」和「人工标注的真值」按 `source_ordinal` 连起来算分，并把录下来的工具调用重放回同一套工具面，检查**agent 读错时代码闸门有没有拦住**。

## 现在实现了哪一半

07 §3.6 把两件事按成本拆开，**只有零额度那一列落地了**：

| | 测什么 | 怎么跑 | 成本 | 进 CI | 现状 |
|---|---|---|---|---|---|
| **回归** | 代码改了会不会挂 | 重放夹具 | 零额度、确定性 | ✅ | **已落地** |
| **eval** | 模型读得准不准 | 真调 agent CLI | 烧额度 | ❌ | **已落地** |

**夹具导出器已落地**（2026-08-17）：`node scripts/export-fixture.mjs <agent_session_id>` 把一次真实解析打包成可重放的夹具目录。它不调 agent，读的是已经落盘的日志与库。

**`node scripts/eval.mjs`（不带参数）会烧订阅额度**——它真起用户自己的 agent CLI，一条用例 ≈ 一次真实导入。零额度的是 `--dry-run` 与 `--replay`。

`node scripts/eval.mjs`（不带参数）会**非零退出并明说真实轮次没做**，不静默返回成功。

## 数据流

```
node scripts/eval.mjs --dry-run
  → scripts/eval.mjs                         ← 只起进程，不碰 SQLite
  → src-tauri/target/debug/daybook-eval validate
  → src-tauri/src/eval/manifest.rs::Manifest::validate    ← 缺分池标记在这里拒绝
  → src-tauri/src/eval/expected.rs::ExpectedSet::load     ← 真值解析（ordinal 唯一性）

node scripts/export-fixture.mjs <agent_session_id>
  → scripts/export-fixture.mjs                            ← 只起进程
  → daybook-eval export-fixture --session <id>
  → src-tauri/src/eval/export.rs::export_fixture
      ├ refuse_committed_set                              ← 拒绝写进 fixtures/ci/
      ├ load_attempt / load_source                        ← parse_attempts + sources
      ├ read_tool_calls                                   ← <data>/logs/<id>.debug.jsonl
      ├ copy evidence → input.png | input.txt
      ├ load_drafted → expected.json（annotated: false）    ← **只是预填，不是真值**
      └ env.json（版本三元组取自那次尝试自己的记录）

node scripts/eval.mjs                 ← **烧额度**：真起 agent CLI
  → daybook-eval run
  → src-tauri/src/eval/live.rs::ensure_backend_ready   ← 跑任何用例之前，探测不出就非零退出
  → src-tauri/src/eval/live.rs::run_trial × (flaky ? --trials : 1)
      └ 与 --replay 之后完全同一条打分路径
  → 第 1 轮出正式数；其余进 TrialDiagnostics（全过 / 部分过 / 全不过，不取平均）

node scripts/eval.mjs --replay
  → daybook-eval replay-score
  → src-tauri/src/eval/replay.rs::replay_fixture
      ├ FixtureEnv::ensure_current                        ← 版本三元组，先查再动库
      ├ ingest::import_file / import_utterance            ← 走真实导入路径
      ├ INSERT parse_attempts + apply_transition(Parsing)
      ├ domain::draft::DraftStore::handle × N             ← 把 tool-calls.json 喂回工具面
      └ finalize_attempt → apply_transition(Parsed)
  → src-tauri/src/eval/replay.rs::predictions_from_drafted_json   ← 预测侧只读 drafted_json
  → src-tauri/src/eval/join.rs::ordinal_full_outer_join
  → src-tauri/src/eval/metrics.rs::compute_pool           ← 十项指标，整数对
  → src-tauri/src/eval/report.rs::Report::build           ← 结构化 JSON
  → scripts/eval.mjs 渲染 diff 表                          ← 唯一做除法的地方
```

## 关键文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/eval/manifest.rs` | `fixtures/manifest.json` 解析与校验；`Pool::{Screenshot,Utterance,Control}` |
| `src-tauri/src/eval/expected.rs` | `expected.json`（唯一真值）解析；ordinal 唯一、金额必须是整数字符串 |
| `src-tauri/src/eval/join.rs` | ordinal full outer join；`HARD_FIELDS`；降级集合匹配（诊断用） |
| `src-tauri/src/eval/metrics.rs` | 十项阈值常量、`Ratio{num,den}`、`compute_pool`、`overall_verdict` |
| `src-tauri/src/eval/replay.rs` | `FixtureEnv`、`replay_fixture`、`predictions_from_drafted_json`、口述子串检查 |
| `src-tauri/src/eval/live.rs` | 真跑轮次；后端探测 fail-closed；多轮汇总（不取平均） |
| `src-tauri/src/eval/export.rs` | 夹具导出器；CI 集拒写；`annotated: false` 预填；debug 日志缺失时的解释 |
| `src-tauri/src/eval/report.rs` | 报告结构；分池成栏；归因（backend / model / prompt_hash） |
| `src-tauri/src/eval/mod.rs` | `EvalError`；两条结构断言（`eval_guards`） |
| `src-tauri/src/bin/daybook-eval.rs` | `version` / `validate` / `replay-score` / `export-fixture` / `run` 五个子命令，手搓参数解析 |
| `scripts/export-fixture.mjs` | 导出器薄壳；默认输出 `fixtures/local/<date>-<slug>/` |
| `src-tauri/tests/eval_export_cli.rs` | 子命令级验收（只有集成测试拿得到 `CARGO_BIN_EXE_*`） |
| `scripts/eval.mjs` | 薄壳：起进程 + 渲染 diff 表；比率格式化只在这里 |
| `fixtures/manifest.json` | 用例清单（进 git 的显式清单） |
| `fixtures/ci/2026-08-17-misread-amount/` | 合成夹具：`input.png` · `tool-calls.json` · `expected.json` · `env.json` |

## 数据结构

只读，不写：`draft_transactions.drafted_json` · `.source_ordinal` · `.evidence_text`（重放期间由 `DraftStore` 写入）；`parse_attempts.{backend_id, model_id, prompt_hash, tool_surface_version, app_version, reported_total_*, unparsed_note, outcome}`；`sources.{kind, state, evidence_relpath}`。

`fixtures/manifest.json` 每条用例：`id` · `dir` · `expected` · **`pool`（必填）** · `enabled` · `addedOn` · `flaky`。

## 业务规则（不显然的那些）

- **预测侧只能读 `drafted_json`，不能读草稿行当前值。** 行内编辑会就地改掉当前值——用户把 1680 改回 168 之后，读当前值算出来的错误率恒为零。`eval::prediction_uses_drafted_json_not_current_row` 守着；实测把查询改成读当前行，该用例立刻变红。
- **Rust 侧不出比率，只出 `{num, den}`。** `scripts/verify-m0.mjs` 禁止 `src-tauri/src` 下出现 `f32|f64`，而这恰好正合 §9.4「每个比率一律连原始计数一起报」。阈值判定用交叉相乘 `num * 1000 >= permille * den`。
- **`pool` 缺失是硬错误，不是缺省。** 分池是判定口径的一部分（§9.4 口径①），缺了 `--dry-run` 直接非零退出。三个值把「分池」与「对照栏」合成一个字段，于是「screenshot 且 control」这种状态表达不出来。
- **对照栏的指标全部标成 `record_only`。** 数字照报，判定去掉——一栏带着 pass/fail 的数字迟早会被当成结论。
- **合成夹具必须是 `kind = file`。** `utterance` 的确认策略恒为 `user_attested_batch`，批量确认会照常放行（[03 §3.3](../../docs/prd/03-review.md)），那条夹具就断言不到「批量被拒」。
- **版本三元组先查再动库。** `FixtureEnv::ensure_current` 在 `Database::open` 之前跑，所以「夹具过期」永远是第一个出现的错误，而不是重放到一半报个别的错。用 `daybook-eval version` 取当前值。
- **`evidence_span` 是字符下标，不是字节偏移**（`draft_span_must_match_text`）。写口述夹具时用 `chars().count()` 算，别用 `len()`。
- **重放模块引用不到 agent。** `eval_guards::replay_path_cannot_reach_the_agent` 用 `include_str!` 扫 `replay.rs`，禁止出现启动器、后端 trait 与进程 API 的名字。需要的字面量放在 `mod.rs` 里，否则断言会命中自己。
- **`scripts/eval.mjs` 不进 `ci.yml`。** CI 只经 `verify-m0.mjs --skip-live` 跑到 `--dry-run` 那一步。
- **导出的 `expected.json` 带 `annotated: false`，评分器会拒绝。** 导出器手上只有 `drafted_json`，那是**被评分的那一侧**——不设这道闸门就会「导出一条夹具直接跑分、每项满分」，而那个满分什么也没测。人工逐条核对后改成 `true` 才可用；**手写夹具缺省就是 `true`**（按构造就是标注过的）。
- **导出器拒绝写进 `fixtures/ci/`。** 导出的是真实账目，§3.7 说「分离靠目录，不靠自觉」——那就得真有人拦，不能只靠默认路径。
- **版本三元组取自那次 `parse_attempts` 行自己的记录**，不是导出当时的代码；否则旧夹具会被盖上今天的版本号，过期检测永远不触发。

## 已知边界与坑

- **`--replay` 的判定不是 §9.4 的 go / no-go。** 重放测的是闸门，不是模型；当前那条夹具故意读错一笔，所以报告里的判定是 `no_go`——那是设计如此，不是产品结论。渲染时已在末尾注明。
- **指标 5（假警报率）的分子要人工裁定**，报告里是 `null` + `pending_manual`，不会显示成 0。
- **指标 9 / 10（耗时、额度）在重放路径上没有意义**，要等真实轮次。
- **`verify-m0.mjs` 现在会扫 `docs/prd/07-eval.md` §6 的 `cargo test` 选择器**，改测试名而不改文档就会红。
- **`--trials N` 只对标 `flaky` 的用例生效**，且第 1 轮出正式数、其余只进诊断栏。多跑几轮挑一个好看的写进报告，与看完答卷再改阈值是同一种作弊。
- **`tool-calls.json` 不是必需的。** 有就可重放（零额度回归），没有就只能真跑——一条还没跑过的用例本来就没有工具调用可录。按文件在不在推断，不另设开关。
- **`--keep-runs <dir>` 是闭环的那一步。** 不给它，eval 每轮的数据目录跑完即删，一次跑砸的轮次就没法用 `export-fixture --data-dir <那一轮>` 变成回归夹具。默认仍是删——那里面是真实解析产物。
- **导出器与日志保留期赛跑。** `tool-calls.json` 的原料是 `<数据目录>/logs/<agent_session_id>.debug.jsonl` 里的 `kind: "tool_call"` 记录，而 `cleanup_expired_logs` 每次应用启动会删 14 天前的 `*.jsonl`；`.debug.jsonl` 还只在 debug 开关打开时才写（发布构建默认关）。想留证据就趁早导。

## 相关

- [ADR-0002 AI 永不直接写入与证据链](../../docs/adr/0002-ai-never-writes-directly.md)
- [`.claude/rules/money-and-data.md` §4.1](../rules/money-and-data.md)（起草值不可变）
- [`.claude/features/total-cross-check.md`](./total-cross-check.md)（重放断言的那道闸门）
- [`.claude/features/agent-runtime.md`](./agent-runtime.md)（真实轮次要接的那条路径）
