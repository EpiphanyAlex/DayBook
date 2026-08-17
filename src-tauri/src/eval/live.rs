//! 真跑 agent 的 eval 轮次（07 §3.1、§3.4）。
//!
//! **走生产的同一条路径**：起 MCP server、spawn 用户自己的 agent CLI、落进一个临时数据
//! 目录、然后查表打分。**不直接调厂商 API**——那样测的是另一个系统（无工具面、无提示词
//! 模板、无闸门），跑绿了不说明产品对（§4 的第一条否决）。
//!
//! 代价如实写下来：**每跑一轮就烧一轮订阅额度**，与做一次真实导入同价。20 个用例 ≈ 20
//! 次导入。所以它**不进 CI**，只在改提示词、换后端、发版前手动跑（§3.1）。
//!
//! ## 三条口径落在这里
//!
//! - **每条 1 轮出正式数**（§9.4 口径③）。`--trials N` 的第 2 轮起只进诊断栏，
//!   **不覆盖正式数**——多跑几轮挑一个好看的写进报告，与看完答卷再改阈值是同一种作弊。
//! - **多轮报「全过 / 部分过 / 全不过」，不取平均**（§3.4）。`部分过` 本身就是结论：
//!   它说明这条用例在当前模型下不稳定，比一个 66% 的分数有信息量得多。
//! - **检测不到可用后端就非零退出**（§6）。探测放在跑任何一条用例**之前**，
//!   既是 fail closed，也省得烧了一半额度才发现没登录。

use std::{path::Path, sync::Arc};

use serde::Serialize;

use super::{
    expected::ExpectedSet,
    join::{degraded_set_match, ordinal_full_outer_join, OrdinalJoin},
    replay::{predictions_from_drafted_json, utterance_substring_violations, FixtureEnv},
    EvalError, EvalResult,
};
use crate::{
    agent::runtime::AgentRuntime,
    db::Database,
    domain::confirm::{self, TotalCheck},
    ingest,
};

/// 一轮（一条用例的一次运行）的结果。
pub struct TrialOutcome {
    pub attempt_id: String,
    pub source_id: String,
    pub database: Arc<Database>,
    pub join: OrdinalJoin,
    pub check: TotalCheck,
    pub substring_violations: Vec<String>,
}

impl TrialOutcome {
    /// 这一轮算不算「过」——**条目完整 + 四个硬字段全对**，与 §9.4 的「干净」同口径。
    pub fn passed(&self) -> bool {
        self.join.is_clean_source()
    }
}

/// 多轮的汇总。**不取平均**（§3.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrialVerdict {
    /// 全过
    AllPassed,
    /// **部分过——这本身就是结论**：这条用例在当前模型下不稳定。
    Mixed,
    /// 全不过
    NonePassed,
}

pub fn summarize_trials(passes: &[bool]) -> Option<TrialVerdict> {
    if passes.is_empty() {
        return None;
    }
    let passed = passes.iter().filter(|value| **value).count();
    Some(if passed == passes.len() {
        TrialVerdict::AllPassed
    } else if passed == 0 {
        TrialVerdict::NonePassed
    } else {
        TrialVerdict::Mixed
    })
}

/// **跑任何一条用例之前先探测一次。**
///
/// 07 §6：「在检测不到可用 agent CLI 时**非零退出**并明确报原因，**不静默降级为通过**。」
/// 放在最前面还有一个实际好处：不会烧了一半额度才发现没登录。
///
/// 探测本身不产生 `parse_attempts` 行（[01 §3.7](../../../docs/prd/01-agent-runtime.md)）。
pub async fn ensure_backend_ready(runtime: &AgentRuntime, probe_root: &Path) -> EvalResult<()> {
    let database = Arc::new(Database::open(probe_root)?);
    runtime.probe(database).await.map_err(|error| {
        EvalError::Fixture(format!(
            "后端不可用，本轮 eval 不启动：{}（{}）\n  \
             这不是「跑过了但没结果」——检测不到可用的 agent CLI 时必须非零退出，\n  \
             不得静默降级为通过（07 §6）。先在 CLI 自身把安装 / 登录处理好再跑。",
            error.message, error.code
        ))
    })?;
    Ok(())
}

/// 真跑一条用例一轮。
///
/// `runtime` 由调用方给——生产入口传 `AgentRuntime::claude_default()`，测试传一个脚本化
/// 后端。**这条缝让整条流水线能在零额度下测通**，剩下的不确定性只有模型本身。
pub async fn run_trial(
    runtime: &AgentRuntime,
    case_dir: &Path,
    env: &FixtureEnv,
    expected: &ExpectedSet,
    scratch: &Path,
) -> EvalResult<TrialOutcome> {
    let database = Arc::new(Database::open(scratch)?);
    database.set_base_currency(&env.base_currency)?;
    // eval 要留取证材料：夹具导出器的原料就是这一份（07 §3.6）。
    database.set_debug_logging(true)?;

    let input_path = case_dir.join(&env.source.input);
    let imported = match env.source.kind.as_str() {
        "file" => ingest::import_file(&database, &input_path)?,
        "utterance" => {
            let text = std::fs::read_to_string(&input_path)?;
            ingest::import_utterance(&database, &text, &format!("eval:{}", env.source.id))?
        }
        other => {
            return Err(EvalError::Fixture(format!(
                "不认识的来源类型 `{other}`（只能是 file 或 utterance）"
            )))
        }
    };

    let summary = runtime
        .parse_source(Arc::clone(&database), imported.source_id.clone())
        .await?;

    let predicted = predictions_from_drafted_json(&database, &summary.attempt_id)?;
    let join = ordinal_full_outer_join(&expected.items, &predicted);
    let check = confirm::total_check(&database, &summary.attempt_id)?;
    let substring_violations = utterance_substring_violations(&database, &summary.attempt_id)?;

    Ok(TrialOutcome {
        attempt_id: summary.attempt_id,
        source_id: imported.source_id,
        database,
        join,
        check,
        substring_violations,
    })
}

/// 诊断栏：第 2 轮起的产物。**不覆盖正式数**（§9.4 口径③）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialDiagnostics {
    pub label: &'static str,
    pub trials: usize,
    /// 每一轮过没过，**按轮次顺序**——`部分过` 时看得出是哪几轮。
    pub passed_per_trial: Vec<bool>,
    pub verdict: TrialVerdict,
}

impl TrialDiagnostics {
    pub fn new(passes: Vec<bool>) -> Option<Self> {
        let verdict = summarize_trials(&passes)?;
        Some(Self {
            label: "诊断用",
            trials: passes.len(),
            passed_per_trial: passes,
            verdict,
        })
    }
}

/// 降级的集合匹配（诊断用），与重放路径同一个实现。
pub fn degraded_for(
    expected: &ExpectedSet,
    outcome: &TrialOutcome,
) -> EvalResult<super::join::DegradedMatch> {
    let predicted = predictions_from_drafted_json(&outcome.database, &outcome.attempt_id)?;
    Ok(degraded_set_match(&expected.items, &predicted))
}

#[cfg(test)]
mod eval {
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::watch;

    use super::*;
    use crate::{
        agent::{
            backend::{AgentBackend, AgentTask, AgentTaskResult, BackendStatus, ProbeResult},
            registry::{effective_capability_hash, expected_capabilities},
        },
        domain::draft::{Assignment, DraftStore},
        error::{AppError, AppResult},
    };

    /// 一个脚本化的后端：不起进程，直接把预设的工具调用喂进 `DraftStore`。
    ///
    /// 它让**真跑轮次的整条流水线**（导入 → parse_source → 打分 → 多轮汇总）能在零额度
    /// 下测通；剩下的不确定性只有模型本身，而那正是这轮 eval 要量的东西。
    struct ScriptedBackend {
        /// 每一轮第一条草稿报的金额——用来造「同一条用例时对时错」。
        amounts: Vec<&'static str>,
        round: AtomicUsize,
    }

    #[async_trait]
    impl AgentBackend for ScriptedBackend {
        fn id(&self) -> &'static str {
            "scripted"
        }

        fn status(&self) -> BackendStatus {
            BackendStatus {
                backend_id: self.id().to_owned(),
                executable: None,
                version: Some("1".to_owned()),
                available: true,
                authenticated: Some(true),
                error_code: None,
            }
        }

        async fn probe(&self, _database: Arc<Database>) -> AppResult<ProbeResult> {
            let manifest = expected_capabilities();
            Ok(ProbeResult {
                backend_id: self.id().to_owned(),
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
            let index = self.round.fetch_add(1, Ordering::SeqCst);
            let amount = self.amounts[index.min(self.amounts.len() - 1)];
            let store = DraftStore::for_task(
                Arc::clone(&database),
                Assignment {
                    source_id: task.source_id.clone(),
                    attempt_id: task.attempt_id,
                },
            );
            store.handle("read_source", json!({ "sourceId": task.source_id }))?;
            store.handle(
                "draft_transaction",
                json!({
                    "sourceId": task.source_id,
                    "evidenceText": "咖啡 5 澳元",
                    "sourceOrdinal": 1,
                    "evidenceSpan": { "start": 0, "end": 7 },
                    "occurredOn": "2026-08-17",
                    "amountMinor": amount,
                    "currency": "AUD",
                    "baseAmountMinor": amount,
                    "baseCurrency": "AUD",
                    "ratePpm": "1000000",
                    "direction": "expense",
                    "merchant": "咖啡",
                    "category": null,
                    "channel": null,
                    "confidence": 90
                }),
            )?;
            store.handle(
                "complete_source",
                json!({ "sourceId": task.source_id, "itemCount": 1, "unparsedNote": "" }),
            )?;
            Ok(AgentTaskResult {
                success: store.is_complete(),
                model_id: Some("scripted-model".to_owned()),
                agent_session_id: Some(task.agent_session_id),
                stdout: String::new(),
                stderr: String::new(),
                trace_events: store.trace_events(),
                debug_events: store.debug_events(),
            })
        }
    }

    /// 一个永远探测不出来的后端——「没装 / 没登录」在 eval 这一层的样子。
    struct UnavailableBackend;

    #[async_trait]
    impl AgentBackend for UnavailableBackend {
        fn id(&self) -> &'static str {
            "unavailable"
        }

        fn status(&self) -> BackendStatus {
            BackendStatus {
                backend_id: self.id().to_owned(),
                executable: None,
                version: None,
                available: false,
                authenticated: None,
                error_code: Some("agent.backend_unavailable".to_owned()),
            }
        }

        async fn probe(&self, _database: Arc<Database>) -> AppResult<ProbeResult> {
            Err(AppError::new(
                "agent.backend_unavailable",
                "未检测到 Claude Code CLI，请先安装后再解析",
            ))
        }

        async fn run_task(
            &self,
            _database: Arc<Database>,
            _task: AgentTask,
            _cancel: watch::Receiver<bool>,
        ) -> AppResult<AgentTaskResult> {
            unreachable!("探测就该拦住，不该走到下发任务")
        }
    }

    fn case_dir(
        directory: &Path,
        transcript: &str,
    ) -> (std::path::PathBuf, FixtureEnv, ExpectedSet) {
        let case = directory.join("case");
        std::fs::create_dir_all(&case).unwrap();
        std::fs::write(case.join("input.txt"), transcript).unwrap();
        let env: FixtureEnv = serde_json::from_value(json!({
            "toolSurfaceVersion": crate::agent::registry::tool_surface_version(),
            "appVersion": "0.1.0",
            "schemaVersion": 1,
            "baseCurrency": "AUD",
            "source": { "id": "11111111-1111-4111-8111-111111111111", "kind": "utterance", "input": "input.txt" },
            "attempt": {
                "backendId": "scripted", "backendVersion": "1", "modelId": null,
                "promptHash": "0".repeat(64), "effectiveCapabilityHash": "0".repeat(64)
            },
            "expectedState": "parsed",
            "expectedReconciliationStatus": "not_applicable",
            "expectedConfirmationPolicy": "user_attested_batch"
        }))
        .unwrap();
        let expected: ExpectedSet = serde_json::from_value(json!({
            "sourceKind": "utterance",
            "items": [{
                "sourceOrdinal": 1, "occurredOn": "2026-08-17", "amountMinor": "500",
                "currency": "AUD", "direction": "expense", "merchant": "咖啡"
            }]
        }))
        .unwrap();
        (case, env, expected)
    }

    /// 07 §6：「在检测不到可用 agent CLI 时**非零退出**并明确报原因，**不静默降级为通过**。」
    #[tokio::test]
    async fn live_run_refuses_to_start_without_a_backend() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = AgentRuntime::new(Arc::new(UnavailableBackend));
        let error = ensure_backend_ready(&runtime, directory.path())
            .await
            .expect_err("探测不出后端就不该开跑");
        let message = error.to_string();
        assert!(message.contains("agent.backend_unavailable"), "{message}");
        assert!(message.contains("不得静默降级为通过"), "{message}");
    }

    /// 真跑一轮：走生产同一条路径（导入 → `parse_source` → 打分）。
    #[tokio::test]
    async fn live_trial_scores_against_expected() {
        let directory = tempfile::tempdir().unwrap();
        let (case, env, expected) = case_dir(directory.path(), "咖啡 5 澳元");
        let runtime = AgentRuntime::new(Arc::new(ScriptedBackend {
            amounts: vec!["500"],
            round: AtomicUsize::new(0),
        }));
        let scratch = directory.path().join("run");
        std::fs::create_dir_all(&scratch).unwrap();

        let outcome = run_trial(&runtime, &case, &env, &expected, &scratch)
            .await
            .unwrap();
        assert_eq!(outcome.join.matched_count(), 1);
        assert!(outcome.passed(), "金额读对了就该算过");
        assert_eq!(outcome.check.reconciliation_status, "not_applicable");
        assert!(outcome.substring_violations.is_empty());

        // 打分读的是 drafted_json，所以后端报什么这里就量什么。
        let degraded = degraded_for(&expected, &outcome).unwrap();
        assert_eq!(degraded.matched, 1);
    }

    /// 07 §3.4：多轮报「3 轮全过 / 部分过 / 全不过」，**不取平均**。
    #[tokio::test]
    async fn flaky_case_reports_mixed_not_an_average() {
        let directory = tempfile::tempdir().unwrap();
        let (case, env, expected) = case_dir(directory.path(), "咖啡 5 澳元");
        // 三轮里对两次错一次 —— 取平均会得到 0.667，那个数字什么也没说。
        let runtime = AgentRuntime::new(Arc::new(ScriptedBackend {
            amounts: vec!["500", "5000", "500"],
            round: AtomicUsize::new(0),
        }));

        let mut passes = Vec::new();
        for trial in 0..3 {
            let scratch = directory.path().join(format!("run{trial}"));
            std::fs::create_dir_all(&scratch).unwrap();
            let outcome = run_trial(&runtime, &case, &env, &expected, &scratch)
                .await
                .unwrap();
            passes.push(outcome.passed());
        }
        assert_eq!(passes, vec![true, false, true]);

        let diagnostics = TrialDiagnostics::new(passes).unwrap();
        assert_eq!(diagnostics.verdict, TrialVerdict::Mixed);
        assert_eq!(diagnostics.label, "诊断用");
        assert_eq!(diagnostics.trials, 3);
    }

    #[test]
    fn trial_summary_has_three_outcomes_and_no_average() {
        assert_eq!(summarize_trials(&[]), None);
        assert_eq!(
            summarize_trials(&[true, true, true]),
            Some(TrialVerdict::AllPassed)
        );
        assert_eq!(
            summarize_trials(&[false, false]),
            Some(TrialVerdict::NonePassed)
        );
        assert_eq!(
            summarize_trials(&[true, false, false]),
            Some(TrialVerdict::Mixed)
        );
        // 汇总里没有「比例」这种东西——`部分过` 就是结论本身。
        let json = serde_json::to_string(&TrialVerdict::Mixed).unwrap();
        assert_eq!(json, "\"mixed\"");
    }
}
