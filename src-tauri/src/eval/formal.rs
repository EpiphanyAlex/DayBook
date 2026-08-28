//! M0 正式首轮与三轮诊断的 live 执行器。
//!
//! 首轮每 case **恰好一次**，不自动重试。`live::run_trial` 会把完成协议 / 模型输出类
//! case 质量失败转成可评分的失败 outcome；其余运行 / 基础设施错误原样返回并中止整轮。
//! 持久化与零额度 finalize 在 `m0.rs`，因此 finalize 模块在结构上不可达 agent。

use std::collections::BTreeSet;

use serde::Serialize;

use super::{
    expected::ExpectedSet,
    live,
    m0::DIAGNOSIS_ROUNDS,
    manifest::ValidatedCase,
    metrics::CaseOutcome,
    replay::{scratch_root, FixtureEnv},
    report::{Attribution, CaseReport, Report},
    EvalResult,
};
use crate::{agent::runtime::AgentRuntime, db::Database};

pub async fn run_first(agent: &AgentRuntime, cases: &[ValidatedCase]) -> EvalResult<Report> {
    let probe_root = scratch_root()?;
    let probed = live::ensure_backend_ready(agent, &probe_root).await;
    let _ = std::fs::remove_dir_all(&probe_root);
    probed?;

    let mut reports = Vec::new();
    let mut outcomes = Vec::new();
    for case in cases {
        // **没有 retry loop**。质量失败由 run_trial 变成 outcome；基础设施错误在 `?` 处中止。
        let (report, outcome) = run_once(agent, case).await?;
        reports.push(report);
        outcomes.push(outcome);
    }
    Ok(Report::build("live", reports, &outcomes))
}

async fn run_once(
    agent: &AgentRuntime,
    case: &ValidatedCase,
) -> EvalResult<(CaseReport, CaseOutcome)> {
    let expected = ExpectedSet::load(&case.expected_path)?;
    let env = FixtureEnv::load(&case.env_path)?;
    let scratch = scratch_root()?;
    let trial = live::run_trial(agent, &case.dir, &env, &expected, &scratch).await;
    let _ = std::fs::remove_dir_all(&scratch);
    let trial = trial?;
    let degraded = live::degraded_for(&expected, &trial)?;
    let attribution = attribution_of(&trial.database, &trial.attempt_id)?;
    let unparsed_note = trial.check.unparsed_note.clone().unwrap_or_default();
    // 正式报告只需要知道「有没有解释」来审计指标 6，不保存可能复述真实原文的内容。
    let report_unparsed_note = report_unparsed_note(&unparsed_note);
    let join = trial.join.clone();
    let case_passed = trial.passed();

    let outcome = CaseOutcome {
        id: case.id.clone(),
        pool: case.pool,
        source_kind: expected.source_kind.clone(),
        join: join.clone(),
        degraded: degraded.clone(),
        reconciliation_status: trial.check.reconciliation_status.clone(),
        confirmation_policy: trial.check.confirmation_policy.clone(),
        unparsed_note: unparsed_note.clone(),
        stated_item_count: expected.stated_item_count(),
        duration_ms: Some(trial.duration_ms),
        usage: trial.usage.clone(),
    };
    let report = CaseReport {
        id: case.id.clone(),
        pool: case.pool.as_str(),
        judged: case.pool.is_judged(),
        flaky: case.flaky,
        source_kind: expected.source_kind,
        attribution,
        reconciliation_status: trial.check.reconciliation_status,
        confirmation_policy: trial.check.confirmation_policy,
        unparsed_note: report_unparsed_note,
        matched: join.matched_count(),
        missed: join.missed_count(),
        extra: join.extra_count(),
        join,
        degraded,
        calls: Vec::new(),
        trial_diagnostics: None,
        substring_violations: trial.substring_violations,
        case_passed,
        execution_error: trial.execution_error,
        duration_ms: Some(trial.duration_ms),
        usage: trial.usage,
    };
    Ok((report, outcome))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosisReport {
    pub format_version: u32,
    pub mode: &'static str,
    pub stage: &'static str,
    pub first_report_id: String,
    pub rounds_per_case: usize,
    pub cases: Vec<DiagnosisCase>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosisCase {
    pub id: String,
    pub rounds: Vec<DiagnosisRound>,
    pub verdict: live::TrialVerdict,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosisRound {
    pub round: usize,
    pub passed: bool,
    pub execution_error: Option<String>,
    pub duration_ms: i64,
    pub usage: Option<super::metrics::UsageCounts>,
    pub attribution: Attribution,
}

pub async fn run_diagnosis(
    agent: &AgentRuntime,
    cases: &[ValidatedCase],
    target_ids: &[String],
    first_report_id: &str,
) -> EvalResult<DiagnosisReport> {
    let targets: BTreeSet<&str> = target_ids.iter().map(String::as_str).collect();
    let selected = cases
        .iter()
        .filter(|case| targets.contains(case.id.as_str()))
        .collect::<Vec<_>>();
    if selected.len() != targets.len() {
        return Err(super::EvalError::Manifest(
            "首轮报告的诊断目标与当前 manifest 不一致".to_owned(),
        ));
    }

    let probe_root = scratch_root()?;
    let probed = live::ensure_backend_ready(agent, &probe_root).await;
    let _ = std::fs::remove_dir_all(&probe_root);
    probed?;

    let mut reports = Vec::new();
    for case in selected {
        let expected = ExpectedSet::load(&case.expected_path)?;
        let env = FixtureEnv::load(&case.env_path)?;
        let mut rounds = Vec::new();
        let mut passes = Vec::new();
        for round in 1..=DIAGNOSIS_ROUNDS {
            let scratch = scratch_root()?;
            let trial = live::run_trial(agent, &case.dir, &env, &expected, &scratch).await;
            let _ = std::fs::remove_dir_all(&scratch);
            let trial = trial?;
            let passed = trial.passed();
            passes.push(passed);
            rounds.push(DiagnosisRound {
                round,
                passed,
                execution_error: trial.execution_error,
                duration_ms: trial.duration_ms,
                usage: trial.usage,
                attribution: attribution_of(&trial.database, &trial.attempt_id)?,
            });
        }
        reports.push(DiagnosisCase {
            id: case.id.clone(),
            rounds,
            verdict: live::summarize_trials(&passes).expect("固定追加 3 轮"),
        });
    }
    Ok(DiagnosisReport {
        format_version: 1,
        mode: "m0_go_no_go",
        stage: "diagnosis",
        first_report_id: first_report_id.to_owned(),
        rounds_per_case: DIAGNOSIS_ROUNDS,
        cases: reports,
    })
}

fn report_unparsed_note(note: &str) -> String {
    if note.is_empty() {
        String::new()
    } else {
        "[redacted: present]".to_owned()
    }
}

pub fn attribution_of(database: &Database, attempt_id: &str) -> EvalResult<Attribution> {
    Ok(database.read(|connection| {
        connection.query_row(
            "SELECT backend_id, backend_version, model_id, prompt_hash,
                    tool_surface_version, app_version
             FROM parse_attempts WHERE id = ?1",
            [attempt_id],
            |row| {
                Ok(Attribution {
                    backend_id: row.get(0)?,
                    backend_version: row.get(1)?,
                    model_id: row.get(2)?,
                    prompt_hash: row.get(3)?,
                    tool_surface_version: row.get(4)?,
                    app_version: row.get(5)?,
                })
            },
        )
    })?)
}

#[cfg(test)]
mod eval {
    use std::{
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    use async_trait::async_trait;
    use serde_json::json;
    use tokio::sync::watch;

    use super::*;
    use crate::{
        agent::{
            backend::{AgentBackend, AgentTask, AgentTaskResult, BackendStatus, ProbeResult},
            registry::{effective_capability_hash, expected_capabilities},
        },
        domain::draft::{Assignment, DraftStore},
        error::{AppError, AppResult},
        eval::manifest::Pool,
    };

    #[test]
    fn formal_report_redacts_unparsed_note_content() {
        assert_eq!(report_unparsed_note(""), "");
        assert_eq!(
            report_unparsed_note("某真实账户与原文内容"),
            "[redacted: present]"
        );
    }

    #[derive(Clone, Copy)]
    enum Action {
        QualityFailure,
        InfrastructureFailure,
        Pass,
    }

    struct SequenceBackend {
        actions: Vec<Action>,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AgentBackend for SequenceBackend {
        fn id(&self) -> &'static str {
            "scripted"
        }

        async fn status(&self) -> BackendStatus {
            BackendStatus::qualified("scripted", PathBuf::from("/fake/agent"), "1".to_owned())
        }

        async fn probe(&self, _database: Arc<Database>) -> AppResult<ProbeResult> {
            let manifest = expected_capabilities();
            Ok(ProbeResult {
                backend_id: "scripted".to_owned(),
                backend_version: "1".to_owned(),
                effective_capability_hash: effective_capability_hash(&manifest),
                sealed_config_fingerprint: "scripted".to_owned(),
                manifest,
            })
        }

        async fn run_task(
            &self,
            database: Arc<Database>,
            task: AgentTask,
            _cancel: watch::Receiver<bool>,
        ) -> AppResult<AgentTaskResult> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            match self.actions[index.min(self.actions.len() - 1)] {
                Action::InfrastructureFailure => {
                    return Err(AppError::new(
                        "agent.spawn_failed",
                        "injected infrastructure",
                    ))
                }
                Action::QualityFailure => {
                    return Ok(AgentTaskResult {
                        success: false,
                        model_id: Some("scripted".to_owned()),
                        agent_session_id: Some(task.agent_session_id),
                        stdout: String::new(),
                        stderr: String::new(),
                        trace_events: Vec::new(),
                        debug_events: Vec::new(),
                    })
                }
                Action::Pass => {}
            }

            let store = DraftStore::for_task(
                Arc::clone(&database),
                Assignment {
                    source_id: task.source_id.clone(),
                    attempt_id: task.attempt_id,
                },
            );
            store.handle("read_source", json!({"sourceId": task.source_id}))?;
            store.handle(
                "draft_transaction",
                json!({
                    "sourceId": task.source_id,
                    "evidenceText": "咖啡 5 澳元",
                    "sourceOrdinal": 1,
                    "evidenceSpan": {"start": 0, "end": 7},
                    "occurredOn": "2026-08-24",
                    "amountMinor": "500",
                    "currency": "AUD",
                    "baseAmountMinor": "500",
                    "baseCurrency": "AUD",
                    "ratePpm": "1000000",
                    "direction": "expense",
                    "merchant": "SHOP",
                    "category": null,
                    "channel": null,
                    "confidence": 90
                }),
            )?;
            store.handle(
                "complete_source",
                json!({"sourceId": task.source_id, "itemCount": 1, "unparsedNote": ""}),
            )?;
            Ok(AgentTaskResult {
                success: true,
                model_id: Some("scripted".to_owned()),
                agent_session_id: Some(task.agent_session_id),
                stdout: String::new(),
                stderr: String::new(),
                trace_events: store.trace_events(),
                debug_events: store.debug_events(),
            })
        }
    }

    fn case(repository: &Path, id: &str) -> ValidatedCase {
        let directory = repository.join(id);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("input.txt"), "咖啡 5 澳元").unwrap();
        let expected_path = directory.join("expected.json");
        std::fs::write(
            &expected_path,
            serde_json::to_vec_pretty(&json!({
                "sourceKind": "utterance",
                "items": [{
                    "sourceOrdinal": 1,
                    "occurredOn": "2026-08-24",
                    "amountMinor": "500",
                    "currency": "AUD",
                    "direction": "expense"
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        let env_path = directory.join("env.json");
        std::fs::write(
            &env_path,
            serde_json::to_vec_pretty(&json!({
                "toolSurfaceVersion": crate::agent::registry::tool_surface_version(),
                "appVersion": "0.1.0",
                "schemaVersion": crate::db::LATEST_SCHEMA_VERSION,
                "baseCurrency": "AUD",
                "source": {"id": "11111111-1111-4111-8111-111111111111", "kind": "utterance", "input": "input.txt"},
                "attempt": {
                    "backendId": "scripted", "backendVersion": "1", "modelId": null,
                    "promptHash": "0".repeat(64), "effectiveCapabilityHash": "0".repeat(64)
                },
                "expectedState": "parsed",
                "expectedReconciliationStatus": "not_applicable",
                "expectedConfirmationPolicy": "user_attested_batch"
            }))
            .unwrap(),
        )
        .unwrap();
        ValidatedCase {
            id: id.to_owned(),
            pool: Pool::Utterance,
            flaky: false,
            sample: None,
            dir: directory,
            expected_path,
            env_path,
            tool_calls_path: None,
        }
    }

    #[tokio::test]
    async fn case_quality_failure_is_recorded_and_next_case_runs() {
        let repository = tempfile::tempdir().unwrap();
        let cases = vec![
            case(repository.path(), "first"),
            case(repository.path(), "second"),
        ];
        let calls = Arc::new(AtomicUsize::new(0));
        let runtime = AgentRuntime::new(Arc::new(SequenceBackend {
            actions: vec![Action::QualityFailure, Action::Pass],
            calls: Arc::clone(&calls),
        }));

        let report = run_first(&runtime, &cases).await.unwrap();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "每 case 恰好 1 次，不自动 retry"
        );
        assert_eq!(report.cases.len(), 2, "第一条质量失败不能吞掉第二条");
        assert_eq!(
            report.cases[0].execution_error.as_deref(),
            Some("agent.protocol_violation")
        );
        assert!(!report.cases[0].case_passed);
        assert!(report.cases[1].case_passed);
    }

    #[tokio::test]
    async fn infrastructure_error_aborts_formal_run() {
        let repository = tempfile::tempdir().unwrap();
        let cases = vec![
            case(repository.path(), "first"),
            case(repository.path(), "second"),
        ];
        let calls = Arc::new(AtomicUsize::new(0));
        let runtime = AgentRuntime::new(Arc::new(SequenceBackend {
            actions: vec![Action::InfrastructureFailure, Action::Pass],
            calls: Arc::clone(&calls),
        }));

        let error = run_first(&runtime, &cases).await.unwrap_err().to_string();
        assert!(error.contains("agent.spawn_failed"), "{error}");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "基础设施错误中止，不碰下一 case"
        );
    }
}
