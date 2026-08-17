//! 夹具重放（07 §3.6）。
//!
//! **agent 是非确定性的，所以「复现一个 bug」不能是「重新跑一次 agent」。** 重放那次录
//! 下来的工具调用序列，把它喂回同一套工具面。因此它测的**不是模型**，是——
//!
//! > **当 agent 读错时，代码闸门有没有拦住。**
//!
//! 一条「把 168 读成 1680」的夹具，断言是「总额交叉校验必须报警、批量确认必须被拒」。
//! 谁把闸门改坏了，这条夹具立刻变红。
//!
//! 零额度、确定性、进 CI —— 与烧额度的 eval 轮次是两件事（07 §3.6 的成本表）。
//!
//! ## 本模块刻意够不着 agent
//!
//! 这里不引用启动器、不引用后端 trait、不起任何子进程。`mod.rs` 的
//! `replay_path_cannot_reach_the_agent` 用 `include_str!` 守着这一点——计数式断言证明
//! 「这一次没起」，那一条证明「根本没有那条路」。

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use super::{join::PredictedItem, EvalError, EvalResult};
use crate::{
    agent::{registry::tool_surface_version, types::DraftTransactionArgs},
    db::{Database, LATEST_SCHEMA_VERSION},
    domain::draft::{Assignment, DraftStore},
    ingest::{self, SourceState},
};

/// `env.json` —— **「自包含」这个词的兑现**（07 §3.6）。
///
/// `input + tool-calls + expected` 三样做不到「换一台机器解压即可重放」：重放要把工具
/// 调用喂回系统，而调用里带着 `sourceId` 这类 UUID，还依赖库里已经存在的那条 `sources`
/// 行。少了这些，重放的第一步就报「来源不存在」。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureEnv {
    /// 版本三元组之一。工具签名一改，旧夹具可能失效（07 §5 R4）——**失效要报得明白**。
    pub tool_surface_version: String,
    pub app_version: String,
    /// 迁移号（`PRAGMA user_version`）。
    pub schema_version: u32,
    pub base_currency: String,
    pub source: FixtureSource,
    pub attempt: FixtureAttempt,
    /// 期望的中间状态：重放后该来源应达到的 `state` 与对账结果。
    pub expected_state: String,
    pub expected_reconciliation_status: String,
    pub expected_confirmation_policy: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureSource {
    /// 夹具里的 UUID。重放时会新生成一个，两者的对应关系就是 07 §3.6 说的「ID 映射」。
    pub id: String,
    pub kind: String,
    /// 证据原件在夹具目录里的文件名（`input.png` / `input.txt`）。
    pub input: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureAttempt {
    pub backend_id: String,
    pub backend_version: String,
    pub model_id: Option<String>,
    pub prompt_hash: String,
    pub effective_capability_hash: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub tool: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallLog {
    pub calls: Vec<ToolCall>,
}

/// 一次工具调用在重放里的结果——**原样记下被测系统的反应**，包括它拒绝了。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallResult {
    pub tool: String,
    pub ok: bool,
    pub error_code: Option<String>,
}

#[derive(Debug)]
pub struct ReplayOutcome {
    pub database: Arc<Database>,
    pub source_id: String,
    pub attempt_id: String,
    pub calls: Vec<CallResult>,
    pub env: FixtureEnv,
}

impl FixtureEnv {
    pub fn load(path: &Path) -> EvalResult<Self> {
        let raw = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// 07 §6：`env.json` 的 `tool_surface_version` 与当前不符时，**明确报夹具过期，不是
    /// 跑到一半报个别的错**。
    ///
    /// 先查再动库，所以「过期」永远是第一个出现的错误。
    pub fn ensure_current(&self) -> EvalResult<()> {
        let current = tool_surface_version();
        if self.tool_surface_version != current {
            return Err(EvalError::StaleFixture {
                field: "toolSurfaceVersion",
                expected: current,
                found: self.tool_surface_version.clone(),
            });
        }
        if i64::from(self.schema_version) != LATEST_SCHEMA_VERSION {
            return Err(EvalError::StaleFixture {
                field: "schemaVersion",
                expected: LATEST_SCHEMA_VERSION.to_string(),
                found: self.schema_version.to_string(),
            });
        }
        Ok(())
    }
}

/// 把一条夹具重放进一个全新的数据目录。
///
/// `data_root` 由调用方给——生产代码不该依赖 `tempfile`（它是 dev-dependency），
/// 而「临时目录建在哪」本来也是调用方的事。
pub fn replay_fixture(case_dir: &Path, data_root: &Path) -> EvalResult<ReplayOutcome> {
    let env = FixtureEnv::load(&case_dir.join("env.json"))?;
    // 先查版本，再碰任何东西。
    env.ensure_current()?;

    let log: ToolCallLog =
        serde_json::from_str(&std::fs::read_to_string(case_dir.join("tool-calls.json"))?)?;

    let database = Arc::new(Database::open(data_root)?);
    database.set_base_currency(&env.base_currency)?;
    database.set_debug_logging(false)?;

    // 走真实的导入路径，而不是自己拼一条 `sources` 行 —— 证据落盘顺序、幂等键、
    // `evidence_relpath` 的算法都在那条路上，绕过去就等于重放了一个不存在的系统。
    let input_path = case_dir.join(&env.source.input);
    let imported = match env.source.kind.as_str() {
        "file" => ingest::import_file(&database, &input_path)?,
        "utterance" => {
            let text = std::fs::read_to_string(&input_path)?;
            ingest::import_utterance(&database, &text, &format!("fixture:{}", env.source.id))?
        }
        other => {
            return Err(EvalError::Fixture(format!(
                "不认识的来源类型 `{other}`（只能是 file 或 utterance）"
            )))
        }
    };
    let source_id = imported.source_id;
    let attempt_id = Uuid::new_v4().to_string();
    let agent_session_id = Uuid::new_v4().to_string();
    let started_at = now()?;

    database.write(|transaction| {
        ingest::apply_transition(transaction, &source_id, SourceState::Parsing, None)?;
        transaction.execute(
            "INSERT INTO parse_attempts (
                id, source_id, agent_session_id, backend_id, backend_version, model_id,
                prompt_hash, tool_surface_version, effective_capability_hash,
                app_version, started_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                attempt_id,
                source_id,
                agent_session_id,
                env.attempt.backend_id,
                env.attempt.backend_version,
                env.attempt.model_id,
                env.attempt.prompt_hash,
                env.tool_surface_version,
                env.attempt.effective_capability_hash,
                env.app_version,
                started_at,
            ],
        )?;
        transaction.execute(
            "UPDATE sources SET latest_attempt_id = ?1 WHERE id = ?2",
            rusqlite::params![attempt_id, source_id],
        )?;
        Ok(())
    })?;

    let store = DraftStore::for_task(
        Arc::clone(&database),
        Assignment {
            source_id: source_id.clone(),
            attempt_id: attempt_id.clone(),
        },
    );

    let mut calls = Vec::new();
    for call in &log.calls {
        // ID 映射：夹具里的 `sourceId` 换成本次重放真实生成的那个（07 §3.6）。
        let arguments = remap_source_id(call.arguments.clone(), &env.source.id, &source_id);
        let result = store.handle(&call.tool, arguments);
        calls.push(CallResult {
            tool: call.tool.clone(),
            ok: result.is_ok(),
            error_code: result.err().map(|error| error.code),
        });
    }

    finalize_attempt(
        &database,
        &source_id,
        &attempt_id,
        env.attempt.model_id.clone(),
    )?;

    Ok(ReplayOutcome {
        database,
        source_id,
        attempt_id,
        calls,
        env,
    })
}

fn remap_source_id(mut arguments: Value, from: &str, to: &str) -> Value {
    if let Some(object) = arguments.as_object_mut() {
        if object.get("sourceId").and_then(Value::as_str) == Some(from) {
            object.insert("sourceId".to_owned(), Value::String(to.to_owned()));
        }
    }
    arguments
}

/// 收尾与 `agent/runtime.rs` 的成功路径同形：由 `complete_source` 留下的元数据决定
/// `outcome`，再把来源推进到 `parsed`。
fn finalize_attempt(
    database: &Database,
    source_id: &str,
    attempt_id: &str,
    model_id: Option<String>,
) -> EvalResult<()> {
    let (item_count, unparsed_note) = database.read(|connection| {
        connection.query_row(
            "SELECT reported_item_count, unparsed_note FROM parse_attempts WHERE id = ?1",
            [attempt_id],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
    })?;
    let (Some(_), Some(unparsed_note)) = (item_count, unparsed_note) else {
        // 夹具里没调 `complete_source`。这是一次协议违规的录像，重放它照样要如实收尾。
        let ended_at = now()?;
        database.write(|transaction| {
            transaction.execute(
                "UPDATE parse_attempts SET ended_at = ?1, outcome = 'protocol_violation',
                    error_code = 'agent.protocol_violation' WHERE id = ?2 AND ended_at IS NULL",
                rusqlite::params![ended_at, attempt_id],
            )?;
            ingest::apply_transition(
                transaction,
                source_id,
                SourceState::Failed,
                Some("agent.protocol_violation"),
            )?;
            Ok(())
        })?;
        return Ok(());
    };
    let outcome = if unparsed_note.is_empty() {
        "completed"
    } else {
        "completed_with_gaps"
    };
    let ended_at = now()?;
    database.write(|transaction| {
        transaction.execute(
            "UPDATE parse_attempts SET ended_at = ?1, outcome = ?2, model_id = ?3
             WHERE id = ?4 AND ended_at IS NULL",
            rusqlite::params![ended_at, outcome, model_id, attempt_id],
        )?;
        ingest::apply_transition(transaction, source_id, SourceState::Parsed, None)?;
        Ok(())
    })?;
    Ok(())
}

/// 预测侧 —— **读 `drafted_json`，不读草稿行的当前值**（07 §3.2）。
///
/// 「用户把 1680 改回 168 之后，读草稿当前值算出来的错误率恒为零。」这一句是本函数
/// 存在的全部理由：`SELECT amount_minor FROM draft_transactions` 会让评分器闭嘴。
pub fn predictions_from_drafted_json(
    database: &Database,
    attempt_id: &str,
) -> EvalResult<Vec<PredictedItem>> {
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
        let drafted: DraftTransactionArgs = serde_json::from_str(&raw)?;
        let amount_minor = drafted.amount_minor.parse::<i64>().map_err(|_| {
            EvalError::Fixture(format!(
                "drafted_json 里的 amountMinor 不是整数：`{}`",
                drafted.amount_minor
            ))
        })?;
        items.push(PredictedItem {
            source_ordinal: drafted.source_ordinal,
            occurred_on: drafted.occurred_on,
            amount_minor,
            currency: drafted.currency,
            direction: drafted.direction,
            merchant: drafted.merchant,
            category: drafted.category,
            channel: drafted.channel,
        });
    }
    Ok(items)
}

/// 07 §3.3 的 transcript 检查之一：`kind = utterance` 的草稿，其抽取声明必须是转写文本
/// 的真实子串。
///
/// **`kind = file` 无法这样断言**——系统里没有 OCR，对一张 PNG 没有可用于比较的真值
/// 文本（07 §3.3 的修正）。那一侧只断言非空，其真实性由审核界面上的原件兜。
pub fn utterance_substring_violations(
    database: &Database,
    attempt_id: &str,
) -> EvalResult<Vec<String>> {
    let source_text = database.read(|connection| {
        connection.query_row(
            "SELECT s.kind, s.evidence_relpath FROM parse_attempts a
             JOIN sources s ON s.id = a.source_id WHERE a.id = ?1",
            [attempt_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
    })?;
    if source_text.0 != "utterance" {
        return Ok(Vec::new());
    }
    let transcript = std::fs::read_to_string(database.root().join(source_text.1))?;

    let claims: Vec<(String, String)> = database.read(|connection| {
        let mut statement = connection.prepare(
            "SELECT id, evidence_text FROM draft_transactions
             WHERE attempt_id = ?1 AND voided_at IS NULL",
        )?;
        let rows = statement
            .query_map([attempt_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })?;

    Ok(claims
        .into_iter()
        .filter(|(_, claim)| !transcript.contains(claim.as_str()))
        .map(|(id, _)| id)
        .collect())
}

/// 给一次重放建一个独立的数据目录。调用方负责删。
pub fn scratch_root() -> EvalResult<PathBuf> {
    let root = std::env::temp_dir().join(format!("daybook-eval-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

fn now() -> EvalResult<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| EvalError::Fixture(format!("时间格式化失败：{error}")))
}

#[cfg(test)]
mod eval {
    use super::*;
    use crate::{
        domain::confirm,
        eval::{
            expected::ExpectedSet,
            join::{ordinal_full_outer_join, HardField},
        },
    };

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("fixtures")
            .join("ci")
            .join("2026-08-17-misread-amount")
    }

    fn replay_the_fixture() -> (tempfile::TempDir, ReplayOutcome) {
        let directory = tempfile::tempdir().unwrap();
        let outcome = replay_fixture(&fixture_dir(), directory.path()).unwrap();
        (directory, outcome)
    }

    /// 07 §6：「重放一条『金额读错』夹具，总额校验报 `review.total_mismatch` 且
    /// `transactions` 保持为空。」
    #[test]
    fn replay_fixture_catches_total_mismatch() {
        let (_directory, outcome) = replay_the_fixture();
        assert!(
            outcome.calls.iter().all(|call| call.ok),
            "夹具录的是「读错了但工具调用都成功」——闸门在对账那一层，不在工具层：{:?}",
            outcome.calls
        );

        let check = confirm::total_check(&outcome.database, &outcome.attempt_id).unwrap();
        assert_eq!(
            check.reconciliation_status, "failed",
            "逐笔之和与声明合计对不上"
        );
        assert_eq!(
            check.confirmation_policy, "single_only",
            "kind = file 永远拿不到 user_attested_batch"
        );

        let drafts = confirm::list_active_drafts(&outcome.database, &outcome.source_id).unwrap();
        let ids: Vec<String> = drafts.iter().map(|draft| draft.id.clone()).collect();
        let rejected =
            confirm::confirm_batch(&outcome.database, &ids, None).expect_err("批量确认必须被拒");
        assert_eq!(rejected.code, "review.total_mismatch", "{rejected:?}");

        let transactions: i64 = outcome
            .database
            .read(|connection| {
                connection.query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
            })
            .unwrap();
        assert_eq!(transactions, 0, "闸门拦住了就不该有事实行");
    }

    /// 07 §6：「重放路径上没有 spawn 子进程。」
    ///
    /// **这里数的是留在库里的痕迹**，因为那是重放之后唯一还能看见的东西：真起过后端就
    /// 会有一次探测、`parse_attempts` 的 `backend_id` 也会是那个真实后端的名字。
    /// 「根本没有那条路」那一半是结构断言，在 `mod.rs`——两条合起来才说得完整。
    #[test]
    fn replay_does_not_invoke_agent() {
        let (_directory, outcome) = replay_the_fixture();
        assert!(!outcome.calls.is_empty(), "夹具确实跑了");

        let (attempts, backend): (i64, String) = outcome
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*), MAX(backend_id) FROM parse_attempts",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
            })
            .unwrap();
        assert_eq!(attempts, 1);
        assert_eq!(backend, "fixture");
    }

    /// 07 §6：「`env.json` 里的 `tool_surface_version` 与当前不符时，重放**明确报夹具
    /// 过期**，不是跑到一半报个别的错（§5 R4）。」
    #[test]
    fn replay_rejects_stale_fixture() {
        let directory = tempfile::tempdir().unwrap();
        let case = directory.path().join("case");
        std::fs::create_dir_all(&case).unwrap();
        for name in ["env.json", "tool-calls.json", "expected.json", "input.png"] {
            std::fs::copy(fixture_dir().join(name), case.join(name)).unwrap();
        }
        let mut env = FixtureEnv::load(&case.join("env.json")).unwrap();
        env.tool_surface_version = "0".repeat(64);
        std::fs::write(
            case.join("env.json"),
            serde_json::to_string_pretty(&env).unwrap(),
        )
        .unwrap();

        let data_root = directory.path().join("data");
        std::fs::create_dir_all(&data_root).unwrap();
        let error = replay_fixture(&case, &data_root).expect_err("过期夹具必须被拒");
        let EvalError::StaleFixture { field, .. } = error else {
            panic!("必须是「夹具过期」，收到 {error}");
        };
        assert_eq!(field, "toolSurfaceVersion");
        // 「不是跑到一半报个别的错」：库都没建起来。
        assert!(!data_root.join("daybook.db").exists());
    }

    /// 07 §6：「把一条草稿行内改过之后跑评分，错误**仍被计出**（改回读当前行时该用例
    /// 必须变红）。」
    #[test]
    fn prediction_uses_drafted_json_not_current_row() {
        let (_directory, outcome) = replay_the_fixture();
        let expected = ExpectedSet::load(&fixture_dir().join("expected.json")).unwrap();

        let before = predictions_from_drafted_json(&outcome.database, &outcome.attempt_id).unwrap();
        let join_before = ordinal_full_outer_join(&expected.items, &before);
        let wrong_before = join_before
            .matched
            .iter()
            .filter(|pair| pair.wrong_fields.contains(&HardField::AmountMinor))
            .count();
        assert_eq!(wrong_before, 1, "夹具里正好有一条金额读错");

        // 用户在审核界面把 1680 改回 168。
        let drafts = confirm::list_active_drafts(&outcome.database, &outcome.source_id).unwrap();
        let broken = drafts
            .iter()
            .find(|draft| draft.source_ordinal == 1)
            .unwrap();
        confirm::edit_draft(
            &outcome.database,
            &broken.id,
            &confirm::DraftPatch {
                amount_minor: Some(crate::money::DecimalI64(1680)),
                ..Default::default()
            },
        )
        .unwrap();

        // 草稿行的当前值已经对了 —— 这正是「读当前值错误率恒为零」的那一刻。
        let current: i64 = outcome
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT amount_minor FROM draft_transactions WHERE id = ?1",
                    [&broken.id],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(current, 1680, "行内编辑确实改掉了当前值");

        let after = predictions_from_drafted_json(&outcome.database, &outcome.attempt_id).unwrap();
        let join_after = ordinal_full_outer_join(&expected.items, &after);
        let wrong_after = join_after
            .matched
            .iter()
            .filter(|pair| pair.wrong_fields.contains(&HardField::AmountMinor))
            .count();
        assert_eq!(
            wrong_after, 1,
            "改回读当前行时这里会变成 0 —— 那就是该变红的地方"
        );
    }

    /// 07 §6：「`kind = utterance` 的草稿，`evidence_text` 是转写文本的真实子串。」
    ///
    /// 用一段现造的口述，而不是再提交一条夹具：这条断言测的是评分器读得对不对，
    /// 不需要一份录像。
    #[test]
    fn utterance_evidence_text_is_substring() {
        let directory = tempfile::tempdir().unwrap();
        let database = Arc::new(Database::open(directory.path()).unwrap());
        database.set_base_currency("AUD").unwrap();
        let transcript = "早上咖啡 5 澳元，中午三明治 12 澳元";
        let imported =
            ingest::import_utterance(&database, transcript, "fixture:utterance").unwrap();

        let attempt_id = Uuid::new_v4().to_string();
        database
            .write(|transaction| {
                ingest::apply_transition(
                    transaction,
                    &imported.source_id,
                    SourceState::Parsing,
                    None,
                )?;
                transaction.execute(
                    "INSERT INTO parse_attempts (
                        id, source_id, agent_session_id, backend_id, backend_version,
                        prompt_hash, tool_surface_version, effective_capability_hash,
                        app_version, started_at
                     ) VALUES (?1, ?2, ?3, 'fixture', '1', ?4, ?5, ?4, '0.1.0', ?6)",
                    rusqlite::params![
                        attempt_id,
                        imported.source_id,
                        Uuid::new_v4().to_string(),
                        "0".repeat(64),
                        tool_surface_version(),
                        now().unwrap(),
                    ],
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
        let draft = |ordinal: i64, claim: &str, start: i64, end: i64, amount: &str| {
            store
                .handle(
                    "draft_transaction",
                    serde_json::json!({
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
        };
        // `evidence_span` 是**字符下标**，不是字节偏移（01 §3.2 / `draft_span_must_match_text`）。
        let span_of = |needle: &str| -> (i64, i64) {
            let byte_offset = transcript.find(needle).expect("片段必须在原文里");
            let start = transcript[..byte_offset].chars().count() as i64;
            (start, start + needle.chars().count() as i64)
        };
        let (start, end) = span_of("早上咖啡 5 澳元");
        draft(1, "早上咖啡 5 澳元", start, end, "500");
        let (start, end) = span_of("中午三明治 12 澳元");
        draft(2, "中午三明治 12 澳元", start, end, "1200");
        assert!(
            utterance_substring_violations(&database, &attempt_id)
                .unwrap()
                .is_empty(),
            "经过工具面的口述草稿，其抽取声明本来就该是原文的真子串"
        );

        // **这条检查不能是恒真的。** 上面两条之所以合格，是因为工具层拦住了
        // 「span 与文本对不上」；一旦那道校验被改坏，这里必须还能报出来。
        // 直接插一行绕过工具面的草稿，验证检测器本身有效。
        database
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO draft_transactions (
                        id, source_id, evidence_text, source_ordinal,
                        evidence_span_start, evidence_span_end, attempt_id, drafted_json,
                        occurred_on, amount_minor, currency, direction, merchant, created_at
                     ) VALUES (?1, ?2, '晚饭 30 澳元', 3, 0, 5, ?3, '{}',
                        '2026-08-17', 3000, 'AUD', 'expense', 'SHOP', ?4)",
                    rusqlite::params![
                        Uuid::new_v4().to_string(),
                        imported.source_id,
                        attempt_id,
                        now().unwrap(),
                    ],
                )?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            utterance_substring_violations(&database, &attempt_id)
                .unwrap()
                .len(),
            1,
            "原文里没有「晚饭 30 澳元」这句话——检测器必须报出来"
        );
    }

    /// 07 §3.6：`env.json` 里「期望的中间状态」不是装饰，重放后要真的对得上。
    #[test]
    fn fixture_env_declares_the_intermediate_state() {
        let (_directory, outcome) = replay_the_fixture();
        let state: String = outcome
            .database
            .read(|connection| {
                connection.query_row(
                    "SELECT state FROM sources WHERE id = ?1",
                    [&outcome.source_id],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(state, outcome.env.expected_state);

        let check = confirm::total_check(&outcome.database, &outcome.attempt_id).unwrap();
        assert_eq!(
            check.reconciliation_status,
            outcome.env.expected_reconciliation_status
        );
        assert_eq!(
            check.confirmation_policy,
            outcome.env.expected_confirmation_policy
        );
    }
}
