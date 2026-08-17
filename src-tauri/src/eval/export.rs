//! 夹具导出器（07 §3.6）。
//!
//! 把**散落三处**的数据打包成一个自包含目录：证据原件在 `evidence/` 下、工具调用序列
//! 在 `debug` 级日志里、条目与归因在 SQLite 里。
//!
//! ```text
//! fixtures/local/<date>-<slug>/
//! ├── input.png | input.txt   ← 截图原件，或 utterance 的转写文本
//! ├── tool-calls.json         ← agent 那次调了哪些工具、每次的完整参数
//! ├── expected.json           ← 该来源的期望条目集合（**导出时只是预填，必须人工核对**）
//! └── env.json                ← 重放所需的环境
//! ```
//!
//! ## 三条不显然的边界
//!
//! 1. **导出的一律是真实数据，所以只能写 `fixtures/local/`**（§3.7）。写进 `fixtures/ci/`
//!    会让真实账目进 git，本模块直接拒绝。
//! 2. **`expected.json` 只能预填，不能当真值。** 导出器手上只有 agent 那次的输出，而
//!    §3.2 说得很清楚：`drafted_json` 是**被评分的那一侧**。所以写出去的那份带
//!    `annotated: false`，评分器见到就拒绝——**否则等于模型给自己判卷，每项满分而什么
//!    也没测**。
//! 3. **`tool-calls.json` 的原料只在 `debug` 级日志里**（ADR-0007）。`trace` 级只记参数
//!    的**形状**，重放不出来。debug 开关关着、或日志已过保留期，导出就该明说是这个原因。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::{
    replay::{FixtureAttempt, FixtureEnv, FixtureSource, ToolCall, ToolCallLog},
    EvalError, EvalResult,
};
use crate::{db::Database, domain::confirm};

#[derive(Debug)]
pub struct ExportSummary {
    pub out_dir: PathBuf,
    pub attempt_id: String,
    pub source_id: String,
    pub tool_call_count: usize,
    pub prefilled_item_count: usize,
}

/// macOS 上 Tauri 的 `data_dir()` 是 `~/Library/Application Support`，应用再往下一层。
/// 与 `lib.rs::run()` 里的 `app.path().data_dir()?.join("Daybook")` 是同一个位置。
pub fn default_data_root() -> EvalResult<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| EvalError::Fixture("读不到 $HOME，请显式给 --data-dir".to_owned()))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Daybook"))
}

pub fn export_fixture(
    data_root: &Path,
    agent_session_id: &str,
    out_dir: &Path,
) -> EvalResult<ExportSummary> {
    refuse_committed_set(out_dir)?;

    let database = Database::open(data_root)?;
    let attempt = load_attempt(&database, agent_session_id)?;
    let source = load_source(&database, &attempt.source_id)?;
    let calls = read_tool_calls(data_root, agent_session_id)?;

    // 证据原件先落到夹具目录里，之后 env.json 只引用夹具内的相对文件名——
    // 「不引用当前数据库、不引用 `evidence/`」是 §6 那条自包含验收的字面要求。
    std::fs::create_dir_all(out_dir)?;
    let input_name = format!("input.{}", source.ext);
    std::fs::copy(
        data_root.join(&source.evidence_relpath),
        out_dir.join(&input_name),
    )?;

    let drafts = load_drafted(&database, &attempt.id)?;
    let check = confirm::total_check(&database, &attempt.id)?;
    let base_currency = database.require_base_currency()?;
    let schema_version = database.read(|connection| {
        connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
    })?;

    let env = FixtureEnv {
        tool_surface_version: attempt.tool_surface_version.clone(),
        app_version: attempt.app_version.clone(),
        schema_version: u32::try_from(schema_version).unwrap_or(u32::MAX),
        base_currency,
        source: FixtureSource {
            id: source.id.clone(),
            kind: source.kind.clone(),
            input: input_name,
        },
        attempt: FixtureAttempt {
            backend_id: attempt.backend_id.clone(),
            backend_version: attempt.backend_version.clone().unwrap_or_default(),
            model_id: attempt.model_id.clone(),
            prompt_hash: attempt.prompt_hash.clone(),
            effective_capability_hash: attempt.effective_capability_hash.clone(),
        },
        expected_state: source.state.clone(),
        expected_reconciliation_status: check.reconciliation_status.clone(),
        expected_confirmation_policy: check.confirmation_policy.clone(),
    };

    write_json(
        &out_dir.join("env.json"),
        &serde_json::to_value(&env)?,
        "重放所需的环境（07 §3.6）。版本三元组对不上时重放会明确报夹具过期。",
    )?;
    write_json(
        &out_dir.join("tool-calls.json"),
        &serde_json::to_value(ToolCallLog {
            calls: calls.clone(),
        })?,
        "agent 那次调了哪些工具、每次的完整参数。重放时跳过模型，直接把它喂回工具面。",
    )?;
    write_json(
        &out_dir.join("expected.json"),
        &json!({
            "sourceKind": source.kind,
            "annotated": false,
            "items": drafts,
        }),
        "⚠️ 这份是**用 agent 那次的输出预填的，还不是真值**。逐条对着 input.* 核对、\
         改对之后把 annotated 改成 true 再跑分——直接拿它评分等于让模型给自己判卷（07 §3.2）。",
    )?;

    Ok(ExportSummary {
        out_dir: out_dir.to_path_buf(),
        attempt_id: attempt.id,
        source_id: source.id,
        tool_call_count: calls.len(),
        prefilled_item_count: drafts.len(),
    })
}

/// §3.7：本机集与 CI 集**靠目录分离，不靠自觉**。导出的是真实截图与真实金额，
/// 一旦落进 `fixtures/ci/` 就会跟着下一次提交进 git。
fn refuse_committed_set(out_dir: &Path) -> EvalResult<()> {
    let normalized = out_dir.to_string_lossy().replace('\\', "/");
    if normalized.contains("fixtures/ci/") || normalized.ends_with("fixtures/ci") {
        return Err(EvalError::Fixture(format!(
            "拒绝写入 {}：导出的是真实数据，只能进 fixtures/local/（07 §3.7）。\n  \
             要进 CI 集得先脱敏或改用合成样本，再手工移过去",
            out_dir.display()
        )));
    }
    Ok(())
}

/// 默认目录名 `fixtures/local/<date>-<slug>`。
pub fn default_out_dir(repository_root: &Path, date: &str, slug: &str) -> PathBuf {
    repository_root
        .join("fixtures")
        .join("local")
        .join(format!("{date}-{slug}"))
}

#[derive(Debug)]
struct AttemptRow {
    id: String,
    source_id: String,
    backend_id: String,
    backend_version: Option<String>,
    model_id: Option<String>,
    prompt_hash: String,
    tool_surface_version: String,
    effective_capability_hash: String,
    app_version: String,
}

fn load_attempt(database: &Database, agent_session_id: &str) -> EvalResult<AttemptRow> {
    let row = database.read(|connection| {
        connection
            .query_row(
                "SELECT id, source_id, backend_id, backend_version, model_id, prompt_hash,
                        tool_surface_version, effective_capability_hash, app_version
                 FROM parse_attempts WHERE agent_session_id = ?1
                 ORDER BY started_at DESC LIMIT 1",
                [agent_session_id],
                |row| {
                    Ok(AttemptRow {
                        id: row.get(0)?,
                        source_id: row.get(1)?,
                        backend_id: row.get(2)?,
                        backend_version: row.get(3)?,
                        model_id: row.get(4)?,
                        prompt_hash: row.get(5)?,
                        tool_surface_version: row.get(6)?,
                        effective_capability_hash: row.get(7)?,
                        app_version: row.get(8)?,
                    })
                },
            )
            .optional()
    })?;
    row.ok_or_else(|| {
        EvalError::Fixture(format!(
            "库里没有 agent_session_id = {agent_session_id} 的解析尝试。\n  \
             会话 id 取自 `<数据目录>/logs/` 下的文件名，或 parse_attempts.agent_session_id"
        ))
    })
}

#[derive(Debug)]
struct SourceRow {
    id: String,
    kind: String,
    ext: String,
    evidence_relpath: String,
    state: String,
}

fn load_source(database: &Database, source_id: &str) -> EvalResult<SourceRow> {
    Ok(database.read(|connection| {
        connection.query_row(
            "SELECT id, kind, ext, evidence_relpath, state FROM sources WHERE id = ?1",
            [source_id],
            |row| {
                Ok(SourceRow {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    ext: row.get(2)?,
                    evidence_relpath: row.get(3)?,
                    state: row.get(4)?,
                })
            },
        )
    })?)
}

/// **预填**的期望条目——读 `drafted_json`，也就是 agent 当初写的值。
///
/// 读的是 `drafted_json` 而不是草稿行当前值，理由和评分器那边一样（07 §3.2）：人已经
/// 在审核界面改过的话，当前值是**人的答案**，拿它预填会让「哪几条被改过」这个信息消失。
/// 预填的目的恰恰是让人看见 agent 当初读成了什么、再动手改。
fn load_drafted(database: &Database, attempt_id: &str) -> EvalResult<Vec<Value>> {
    let rows: Vec<String> = database.read(|connection| {
        let mut statement = connection.prepare(
            "SELECT drafted_json FROM draft_transactions
             WHERE attempt_id = ?1 AND voided_at IS NULL
             ORDER BY source_ordinal",
        )?;
        let rows = statement
            .query_map([attempt_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })?;

    let mut items = Vec::new();
    for raw in rows {
        let drafted: Value = serde_json::from_str(&raw)?;
        items.push(json!({
            "sourceOrdinal": drafted.get("sourceOrdinal").cloned().unwrap_or(Value::Null),
            "occurredOn": drafted.get("occurredOn").cloned().unwrap_or(Value::Null),
            "amountMinor": drafted.get("amountMinor").cloned().unwrap_or(Value::Null),
            "currency": drafted.get("currency").cloned().unwrap_or(Value::Null),
            "direction": drafted.get("direction").cloned().unwrap_or(Value::Null),
            "merchant": drafted.get("merchant").cloned().unwrap_or(Value::Null),
        }));
    }
    Ok(items)
}

/// `tool-calls.json` 的原料：`debug` 级日志里的 `kind = "tool_call"` 记录。
///
/// **`trace` 级不行**——它只记参数的形状（ADR-0007 与 [01 §3.4](../../../docs/prd/01-agent-runtime.md)），
/// 重放不出来。这是刻意的隐私分级，不是遗漏。
fn read_tool_calls(data_root: &Path, agent_session_id: &str) -> EvalResult<Vec<ToolCall>> {
    let path = data_root
        .join("logs")
        .join(format!("{agent_session_id}.debug.jsonl"));
    if !path.exists() {
        return Err(EvalError::Fixture(format!(
            "找不到 {}。\n  \
             两种可能，都不是 bug：① 那次解析跑的时候 debug 日志开关是关的（发布构建默认关，\
             `trace` 级只记参数形状、重放不出来）；② 日志已过默认保留期被清掉了。",
            path.display()
        )));
    }
    let raw = std::fs::read_to_string(&path)?;
    let mut calls = Vec::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)?;
        if value.get("kind").and_then(Value::as_str) != Some("tool_call") {
            continue;
        }
        let (Some(tool), Some(arguments)) = (
            value.get("tool").and_then(Value::as_str),
            value.get("arguments"),
        ) else {
            continue;
        };
        calls.push(ToolCall {
            tool: tool.to_owned(),
            arguments: arguments.clone(),
        });
    }
    if calls.is_empty() {
        return Err(EvalError::Fixture(format!(
            "{} 里没有一条 tool_call 记录——那次会话没有可重放的内容",
            path.display()
        )));
    }
    Ok(calls)
}

/// 写 JSON 时把说明塞进 `_` 字段——夹具是给人读的，而目录里没有别处放注释。
fn write_json(path: &Path, value: &Value, note: &str) -> EvalResult<()> {
    let mut object = serde_json::Map::new();
    object.insert("_".to_owned(), Value::String(note.to_owned()));
    if let Some(fields) = value.as_object() {
        for (key, item) in fields {
            object.insert(key.clone(), item.clone());
        }
    }
    std::fs::write(path, serde_json::to_string_pretty(&Value::Object(object))?)?;
    Ok(())
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod eval {
    use std::sync::Arc;

    use super::*;
    use crate::{
        agent::registry::tool_surface_version,
        domain::draft::{Assignment, DraftStore},
        eval::{expected::ExpectedSet, replay::replay_fixture},
        ingest,
    };
    use uuid::Uuid;

    /// 造一个「真实使用过」的数据目录：一条口述来源、一次解析尝试、三条草稿，
    /// 外加那次会话的 debug 日志。
    fn used_data_root() -> (tempfile::TempDir, String) {
        let directory = tempfile::tempdir().unwrap();
        let database = Arc::new(Database::open(directory.path()).unwrap());
        database.set_base_currency("AUD").unwrap();
        let transcript = "咖啡 5 澳元，三明治 12 澳元，总共 17 澳元";
        let imported = ingest::import_utterance(&database, transcript, "export-test").unwrap();

        let attempt_id = Uuid::new_v4().to_string();
        let agent_session_id = Uuid::new_v4().to_string();
        database
            .write(|transaction| {
                ingest::apply_transition(
                    transaction,
                    &imported.source_id,
                    ingest::SourceState::Parsing,
                    None,
                )?;
                transaction.execute(
                    "INSERT INTO parse_attempts (
                        id, source_id, agent_session_id, backend_id, backend_version, model_id,
                        prompt_hash, tool_surface_version, effective_capability_hash,
                        app_version, started_at
                     ) VALUES (?1, ?2, ?3, 'claude-code', '1.2.3', 'test-model',
                        ?4, ?5, ?4, '0.1.0', '2026-08-17T00:00:00Z')",
                    rusqlite::params![
                        attempt_id,
                        imported.source_id,
                        agent_session_id,
                        "0".repeat(64),
                        tool_surface_version(),
                    ],
                )?;
                transaction.execute(
                    "UPDATE sources SET latest_attempt_id = ?1 WHERE id = ?2",
                    rusqlite::params![attempt_id, imported.source_id],
                )?;
                Ok(())
            })
            .unwrap();

        let store = DraftStore::for_task(
            Arc::clone(&database),
            Assignment {
                source_id: imported.source_id.clone(),
                attempt_id: attempt_id.clone(),
            },
        );
        let span_of = |needle: &str| -> (i64, i64) {
            let byte_offset = transcript.find(needle).unwrap();
            let start = transcript[..byte_offset].chars().count() as i64;
            (start, start + needle.chars().count() as i64)
        };
        for (ordinal, claim, amount) in [(1, "咖啡 5 澳元", "500"), (2, "三明治 12 澳元", "1200")]
        {
            let (start, end) = span_of(claim);
            store
                .handle(
                    "draft_transaction",
                    json!({
                        "sourceId": imported.source_id,
                        "evidenceText": claim,
                        "sourceOrdinal": ordinal,
                        "evidenceSpan": { "start": start, "end": end },
                        "occurredOn": "2026-08-17",
                        "amountMinor": amount,
                        "currency": "AUD",
                        "baseAmountMinor": amount,
                        "baseCurrency": "AUD",
                        "ratePpm": "1000000",
                        "direction": "expense",
                        "merchant": "SHOP",
                        "category": null,
                        "channel": null,
                        "confidence": 90
                    }),
                )
                .unwrap();
        }
        store
            .handle(
                "report_source_total",
                json!({
                    "sourceId": imported.source_id,
                    "amountMinor": "1700",
                    "currency": "AUD",
                    "kind": "expense_total",
                    "evidenceText": "总共 17 澳元"
                }),
            )
            .unwrap();
        store
            .handle(
                "complete_source",
                json!({ "sourceId": imported.source_id, "itemCount": 2, "unparsedNote": "" }),
            )
            .unwrap();
        database
            .write(|transaction| {
                transaction.execute(
                    "UPDATE parse_attempts SET ended_at = '2026-08-17T00:01:00Z', outcome = 'completed'
                     WHERE id = ?1",
                    [&attempt_id],
                )?;
                ingest::apply_transition(
                    transaction,
                    &imported.source_id,
                    ingest::SourceState::Parsed,
                    None,
                )?;
                Ok(())
            })
            .unwrap();

        // 真实链路里这一步由 `agent/runtime.rs::write_session_logs` 做；这里照它的形状写。
        let logs = directory.path().join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let mut lines = vec![json!({
            "kind": "debug",
            "attemptId": attempt_id,
            "agentSessionId": agent_session_id,
            "prompt": "……",
            "stdout": "",
            "stderr": "",
            "modelId": "test-model",
        })
        .to_string()];
        lines.extend(
            store
                .debug_events()
                .iter()
                .map(std::string::ToString::to_string),
        );
        std::fs::write(
            logs.join(format!("{agent_session_id}.debug.jsonl")),
            format!("{}\n", lines.join("\n")),
        )
        .unwrap();

        (directory, agent_session_id)
    }

    /// 07 §6：「产出的目录自包含……**换一台机器解压即可重放**」。
    ///
    /// 判据不是「四个文件都在」，而是**把它拿到一个全新的数据目录里真的能重放**——
    /// 那才排除掉「还在偷偷引用当前数据库或 `evidence/`」。
    #[test]
    fn exported_fixture_is_self_contained() {
        let (data, session) = used_data_root();
        let target = tempfile::tempdir().unwrap();
        let out_dir = target.path().join("2026-08-17-exported");
        let summary = export_fixture(data.path(), &session, &out_dir).unwrap();
        assert_eq!(summary.tool_call_count, 4);
        assert_eq!(summary.prefilled_item_count, 2);

        for name in ["input.txt", "tool-calls.json", "expected.json", "env.json"] {
            assert!(out_dir.join(name).exists(), "{name} 应当在夹具目录里");
        }
        // 目录里不得出现指向原数据目录的绝对路径。
        for name in ["tool-calls.json", "env.json", "expected.json"] {
            let body = std::fs::read_to_string(out_dir.join(name)).unwrap();
            assert!(
                !body.contains(&data.path().to_string_lossy().to_string()),
                "{name} 仍在引用导出时的数据目录"
            );
        }

        // 真正的判据：换一个全新的数据目录，重放跑得起来且状态与 env.json 一致。
        drop(data);
        let fresh = tempfile::tempdir().unwrap();
        let replayed = replay_fixture(&out_dir, fresh.path()).unwrap();
        assert!(replayed.calls.iter().all(|call| call.ok));
        let check = confirm::total_check(&replayed.database, &replayed.attempt_id).unwrap();
        assert_eq!(
            check.reconciliation_status,
            replayed.env.expected_reconciliation_status
        );
    }

    /// 导出的 `expected.json` 是**预填**，不是真值。在人工核对之前评分器必须拒绝它。
    #[test]
    fn exported_expected_set_needs_human_annotation() {
        let (data, session) = used_data_root();
        let target = tempfile::tempdir().unwrap();
        let out_dir = target.path().join("2026-08-17-exported");
        export_fixture(data.path(), &session, &out_dir).unwrap();

        let message = ExpectedSet::load(&out_dir.join("expected.json"))
            .unwrap_err()
            .to_string();
        assert!(message.contains("模型给自己判卷"), "{message}");

        // 人工核对之后（这里模拟：把 annotated 改成 true）才可用。
        let raw = std::fs::read_to_string(out_dir.join("expected.json")).unwrap();
        std::fs::write(
            out_dir.join("expected.json"),
            raw.replace("\"annotated\": false", "\"annotated\": true"),
        )
        .unwrap();
        let set = ExpectedSet::load(&out_dir.join("expected.json")).unwrap();
        assert_eq!(set.items.len(), 2);
        assert_eq!(set.source_kind, "utterance");
    }

    /// §3.7：本机集与 CI 集靠目录分离。导出的是真实账目，写进 `fixtures/ci/` 就会进 git。
    #[test]
    fn export_refuses_to_write_into_the_committed_set() {
        let (data, session) = used_data_root();
        let target = tempfile::tempdir().unwrap();
        let out_dir = target.path().join("fixtures/ci/2026-08-17-oops");
        let message = export_fixture(data.path(), &session, &out_dir)
            .unwrap_err()
            .to_string();
        assert!(message.contains("fixtures/local/"), "{message}");
        assert!(!out_dir.exists(), "被拒绝时不该已经建好目录");
    }

    /// `trace` 级只记参数形状，重放不出来（ADR-0007）。缺 debug 日志时要说清是这个原因，
    /// 而不是丢一个「文件不存在」。
    #[test]
    fn export_explains_why_the_debug_log_is_required() {
        let (data, session) = used_data_root();
        std::fs::remove_file(
            data.path()
                .join("logs")
                .join(format!("{session}.debug.jsonl")),
        )
        .unwrap();
        let target = tempfile::tempdir().unwrap();
        let message = export_fixture(data.path(), &session, &target.path().join("x"))
            .unwrap_err()
            .to_string();
        assert!(message.contains("debug 日志开关"), "{message}");
        assert!(message.contains("保留期"), "{message}");
    }

    /// 版本三元组必须来自那次尝试自己的记录，而不是导出当时的代码——否则一条旧夹具
    /// 会被盖上今天的版本号，07 §5 R4 的过期检测就永远不会触发。
    #[test]
    fn exported_env_carries_the_recorded_version_triple() {
        let (data, session) = used_data_root();
        let target = tempfile::tempdir().unwrap();
        let out_dir = target.path().join("2026-08-17-exported");
        export_fixture(data.path(), &session, &out_dir).unwrap();
        let env = FixtureEnv::load(&out_dir.join("env.json")).unwrap();
        assert_eq!(env.app_version, "0.1.0");
        assert_eq!(env.schema_version, 1);
        assert_eq!(env.attempt.backend_id, "claude-code");
        assert_eq!(env.attempt.backend_version, "1.2.3");
        assert_eq!(env.attempt.model_id.as_deref(), Some("test-model"));
    }
}
