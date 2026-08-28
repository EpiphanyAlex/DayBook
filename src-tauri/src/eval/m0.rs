//! M0 正式 verdict 的持久报告与人工裁定协议。
//!
//! 本模块**刻意不引用 agent runtime / backend / 进程 API**。首轮与诊断的模型调用在
//! `formal.rs`；这里仅处理本机 JSON：immutable first report、独立 adjudications、零额度
//! finalize、诊断目标集合与 create-new 写入。

use std::{
    collections::BTreeSet,
    io::Write,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::{metrics::FALSE_ALARM_MAX_PERMILLE, report::Report, EvalError, EvalResult};

pub const FORMAT_VERSION: u32 = 1;
pub const DIAGNOSIS_ROUNDS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormalExit {
    Complete,
    Incomplete,
    NoGo,
}

impl FormalExit {
    pub fn code(self) -> u8 {
        match self {
            Self::Complete => 0,
            Self::Incomplete => 2,
            Self::NoGo => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormalEnvelope {
    pub format_version: u32,
    pub mode: String,
    pub stage: String,
    pub status: String,
    pub verdict: String,
    pub exit_code: u8,
    pub report_id: String,
    pub created_at: String,
    /// 仓库内 `fixtures/local/...` 的相对路径；不保存真实 input 绝对路径。
    pub manifest_path: String,
    pub manifest_sha256: String,
    pub adjudications_file: Option<String>,
    pub first_report_id: Option<String>,
    pub failed_reconciliation_case_ids: Vec<String>,
    pub failed_case_ids: Vec<String>,
    pub flaky_case_ids: Vec<String>,
    pub base_verdict_without_manual: String,
    pub evaluation: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjudication_summary: Option<AdjudicationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjudicationFile {
    pub version: u32,
    pub report_id: String,
    pub adjudications: Vec<Adjudication>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Adjudication {
    pub case_id: String,
    /// `true` = agent 实际读对、对账却报警；`false` = 真警报。
    pub false_alarm: Option<bool>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjudicationSummary {
    pub false_alarms: i64,
    pub failed_reconciliations: i64,
    pub source_file: String,
}

#[derive(Debug, Clone)]
pub struct FirstWrite {
    pub report_path: PathBuf,
    pub adjudications_path: Option<PathBuf>,
    pub envelope: FormalEnvelope,
    pub exit: FormalExit,
}

#[derive(Debug, Clone)]
pub enum FinalizeResult {
    Complete {
        report_path: PathBuf,
        envelope: Box<FormalEnvelope>,
        exit: FormalExit,
    },
    Incomplete {
        missing_case_ids: Vec<String>,
        adjudications_path: PathBuf,
    },
}

/// 首轮报告 create-new 保存；有待裁定来源时同时 create-new 写 sidecar 模板。
pub fn save_first(
    report: &Report,
    repository_root: &Path,
    manifest_path: &Path,
    manifest_snapshot: &[u8],
    output_path: &Path,
) -> EvalResult<FirstWrite> {
    let manifest_path_text = repository_relative(repository_root, manifest_path)?;
    if !manifest_path_text.starts_with("fixtures/local/") {
        return Err(EvalError::Manifest(
            "M0 正式报告只能引用 fixtures/local/ 下的 manifest".to_owned(),
        ));
    }
    if std::fs::read(manifest_path)? != manifest_snapshot {
        return Err(EvalError::Manifest(
            "正式运行期间 manifest 发生变化，拒绝保存混合样本报告".to_owned(),
        ));
    }
    let manifest_sha256 = format!("{:x}", Sha256::digest(manifest_snapshot));
    let evaluation = serde_json::to_value(report)?;
    let failed_reconciliation_case_ids = report
        .cases
        .iter()
        .filter(|case| case.judged && case.reconciliation_status == "failed")
        .map(|case| case.id.clone())
        .collect::<Vec<_>>();
    let failed_case_ids = report
        .cases
        .iter()
        .filter(|case| case.judged && !case.case_passed)
        .map(|case| case.id.clone())
        .collect::<Vec<_>>();
    let flaky_case_ids = report
        .cases
        .iter()
        .filter(|case| case.flaky)
        .map(|case| case.id.clone())
        .collect::<Vec<_>>();
    let created_at = now()?;
    let report_id = report_id(&manifest_sha256, &created_at, &evaluation);
    let pending = !failed_reconciliation_case_ids.is_empty();
    let exit = if pending {
        FormalExit::Incomplete
    } else if report.verdict == "no_go" {
        FormalExit::NoGo
    } else {
        FormalExit::Complete
    };
    let adjudications_path = pending.then(|| adjudications_path(output_path));
    let envelope = FormalEnvelope {
        format_version: FORMAT_VERSION,
        mode: "m0_go_no_go".to_owned(),
        stage: "first".to_owned(),
        status: if pending { "incomplete" } else { "complete" }.to_owned(),
        verdict: if pending {
            "incomplete"
        } else {
            report.verdict
        }
        .to_owned(),
        exit_code: exit.code(),
        report_id: report_id.clone(),
        created_at,
        manifest_path: manifest_path_text,
        manifest_sha256,
        adjudications_file: adjudications_path
            .as_deref()
            .and_then(Path::file_name)
            .map(|value| value.to_string_lossy().into_owned()),
        first_report_id: None,
        failed_reconciliation_case_ids: failed_reconciliation_case_ids.clone(),
        failed_case_ids,
        flaky_case_ids,
        base_verdict_without_manual: report.verdict.to_owned(),
        evaluation,
        adjudication_summary: None,
    };

    write_json_new(output_path, &envelope)?;
    if let Some(path) = &adjudications_path {
        let template = AdjudicationFile {
            version: 1,
            report_id,
            adjudications: failed_reconciliation_case_ids
                .iter()
                .map(|case_id| Adjudication {
                    case_id: case_id.clone(),
                    false_alarm: None,
                    note: String::new(),
                })
                .collect(),
        };
        write_json_new(path, &template)?;
    }
    Ok(FirstWrite {
        report_path: output_path.to_path_buf(),
        adjudications_path,
        envelope,
        exit,
    })
}

/// 零额度 finalize：只读 immutable first report 与独立 sidecar，另写 final report。
pub fn finalize(first_report: &Path, output_path: &Path) -> EvalResult<FinalizeResult> {
    let first_bytes = std::fs::read(first_report)?;
    let mut first: FormalEnvelope = serde_json::from_slice(&first_bytes)?;
    ensure_first(&first)?;
    if first.failed_reconciliation_case_ids.is_empty() {
        return Err(EvalError::Usage(
            "首轮没有待人工裁定的 failed 对账来源；该 first report 已经是完整 verdict，无需 finalize"
                .to_owned(),
        ));
    }
    let adjudications_path = adjudications_path(first_report);
    let adjudications: AdjudicationFile = match std::fs::read(&adjudications_path) {
        Ok(raw) => serde_json::from_slice(&raw)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FinalizeResult::Incomplete {
                missing_case_ids: first.failed_reconciliation_case_ids.clone(),
                adjudications_path,
            })
        }
        Err(error) => return Err(error.into()),
    };
    if adjudications.version != 1 || adjudications.report_id != first.report_id {
        return Err(EvalError::Fixture(
            "adjudications 与首轮报告的 version / reportId 不匹配".to_owned(),
        ));
    }

    let expected: BTreeSet<&str> = first
        .failed_reconciliation_case_ids
        .iter()
        .map(String::as_str)
        .collect();
    let found: BTreeSet<&str> = adjudications
        .adjudications
        .iter()
        .map(|item| item.case_id.as_str())
        .collect();
    if expected != found || found.len() != adjudications.adjudications.len() {
        return Err(EvalError::Fixture(
            "adjudications 必须与首轮全部 failed 对账来源一一对应，不得缺项、多项或重复".to_owned(),
        ));
    }
    let missing_case_ids = adjudications
        .adjudications
        .iter()
        .filter(|item| item.false_alarm.is_none())
        .map(|item| item.case_id.clone())
        .collect::<Vec<_>>();
    if !missing_case_ids.is_empty() {
        return Ok(FinalizeResult::Incomplete {
            missing_case_ids,
            adjudications_path,
        });
    }

    let false_alarms = adjudications
        .adjudications
        .iter()
        .filter(|item| item.false_alarm == Some(true))
        .count() as i64;
    let failed = adjudications.adjudications.len() as i64;
    let false_alarm_failed = false_alarms * 1000 > FALSE_ALARM_MAX_PERMILLE * failed;
    apply_metric_five(
        &mut first.evaluation,
        false_alarms,
        failed,
        false_alarm_failed,
    )?;
    let verdict = if first.base_verdict_without_manual == "no_go" {
        "no_go"
    } else if false_alarm_failed || first.base_verdict_without_manual == "conditional_go" {
        "conditional_go"
    } else {
        "go"
    };
    let exit = if verdict == "no_go" {
        FormalExit::NoGo
    } else {
        FormalExit::Complete
    };
    first.evaluation["verdict"] = json!(verdict);
    let first_report_id = first.report_id.clone();
    first.stage = "final".to_owned();
    first.status = "complete".to_owned();
    first.verdict = verdict.to_owned();
    first.exit_code = exit.code();
    first.created_at = now()?;
    first.first_report_id = Some(first_report_id);
    first.adjudication_summary = Some(AdjudicationSummary {
        false_alarms,
        failed_reconciliations: failed,
        source_file: adjudications_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    });
    first.report_id = report_id(&first.manifest_sha256, &first.created_at, &first.evaluation);
    write_json_new(output_path, &first)?;

    // 首轮必须保持字节级不变；不仅「没打算写」，还在返回前实测一次。
    if std::fs::read(first_report)? != first_bytes {
        return Err(EvalError::Fixture(
            "finalize 检测到首轮报告被改写，拒绝接受".to_owned(),
        ));
    }
    Ok(FinalizeResult::Complete {
        report_path: output_path.to_path_buf(),
        envelope: Box::new(first),
        exit,
    })
}

pub fn read_first(path: &Path) -> EvalResult<FormalEnvelope> {
    let envelope: FormalEnvelope = serde_json::from_slice(&std::fs::read(path)?)?;
    ensure_first(&envelope)?;
    Ok(envelope)
}

pub fn diagnosis_targets(first: &FormalEnvelope) -> Vec<String> {
    let mut targets = BTreeSet::new();
    targets.extend(first.failed_case_ids.iter().cloned());
    targets.extend(first.flaky_case_ids.iter().cloned());
    targets.into_iter().collect()
}

pub fn ensure_manifest_unchanged(
    repository_root: &Path,
    first: &FormalEnvelope,
) -> EvalResult<PathBuf> {
    let path = repository_root.join(&first.manifest_path);
    let digest = format!("{:x}", Sha256::digest(std::fs::read(&path)?));
    if digest != first.manifest_sha256 {
        return Err(EvalError::Manifest(
            "首轮之后 manifest 已变化；诊断必须针对首轮同一批样本".to_owned(),
        ));
    }
    Ok(path)
}

pub fn adjudications_path(first_report: &Path) -> PathBuf {
    sibling_with_suffix(first_report, ".adjudications.json")
}

pub fn final_report_path(first_report: &Path) -> PathBuf {
    sibling_with_suffix(first_report, ".final.json")
}

pub fn diagnosis_report_path(first_report: &Path, stamp: &str) -> PathBuf {
    sibling_with_suffix(first_report, &format!(".diagnosis-{stamp}.json"))
}

pub fn write_diagnosis_new<T: Serialize>(path: &Path, report: &T) -> EvalResult<()> {
    write_json_new(path, report)
}

fn ensure_first(first: &FormalEnvelope) -> EvalResult<()> {
    if first.format_version != FORMAT_VERSION
        || first.mode != "m0_go_no_go"
        || first.stage != "first"
    {
        return Err(EvalError::Fixture(
            "--m0-finalize / --m0-diagnose 只接受 M0 正式首轮报告".to_owned(),
        ));
    }
    Ok(())
}

fn apply_metric_five(
    evaluation: &mut Value,
    false_alarms: i64,
    failed: i64,
    threshold_failed: bool,
) -> EvalResult<()> {
    let metrics = evaluation
        .get_mut("decisionMetrics")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| EvalError::Fixture("首轮报告缺 decisionMetrics".to_owned()))?;
    let metric = metrics
        .iter_mut()
        .find(|metric| metric.get("key").and_then(Value::as_str) == Some("false_alarm_rate"))
        .ok_or_else(|| EvalError::Fixture("首轮报告缺指标 5".to_owned()))?;
    metric["ratio"]["num"] = json!(false_alarms);
    metric["ratio"]["den"] = json!(failed);
    metric["verdict"] = json!(if threshold_failed { "fail" } else { "pass" });
    Ok(())
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{stem}{suffix}"))
}

fn repository_relative(repository_root: &Path, path: &Path) -> EvalResult<String> {
    let repository_root = std::fs::canonicalize(repository_root)?;
    let path = std::fs::canonicalize(path)?;
    let relative = path.strip_prefix(&repository_root).map_err(|_| {
        EvalError::Manifest("正式 manifest 必须位于当前仓库 fixtures/local/".to_owned())
    })?;
    Ok(relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/"))
}

fn report_id(manifest_sha256: &str, created_at: &str, evaluation: &Value) -> String {
    let mut hash = Sha256::new();
    hash.update(manifest_sha256.as_bytes());
    hash.update(created_at.as_bytes());
    hash.update(evaluation.to_string().as_bytes());
    format!("{:x}", hash.finalize())
}

fn now() -> EvalResult<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| EvalError::Fixture(format!("时间格式化失败：{error}")))
}

fn write_json_new<T: Serialize>(path: &Path, value: &T) -> EvalResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let mut json = serde_json::to_vec_pretty(value)?;
    json.push(b'\n');
    file.write_all(&json)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod eval {
    use super::*;
    use crate::eval::{
        join::{DegradedMatch, MatchedPair, OrdinalJoin},
        manifest::Pool,
        metrics::CaseOutcome,
        report::{Attribution, CaseReport, Report},
    };

    fn clean_join() -> OrdinalJoin {
        OrdinalJoin {
            matched: vec![MatchedPair {
                source_ordinal: 1,
                wrong_fields: Vec::new(),
                free_text_differences: Vec::new(),
            }],
            ..OrdinalJoin::default()
        }
    }

    fn report_case(id: &str, reconciliation: &str, passed: bool, flaky: bool) -> CaseReport {
        let join = if passed {
            clean_join()
        } else {
            OrdinalJoin {
                missed: vec![1],
                ..OrdinalJoin::default()
            }
        };
        CaseReport {
            id: id.to_owned(),
            pool: "screenshot",
            judged: true,
            flaky,
            source_kind: "file".to_owned(),
            attribution: Attribution {
                backend_id: "scripted".to_owned(),
                backend_version: Some("1".to_owned()),
                model_id: Some("model".to_owned()),
                prompt_hash: "0".repeat(64),
                tool_surface_version: "1".to_owned(),
                app_version: "0.1.0".to_owned(),
            },
            reconciliation_status: reconciliation.to_owned(),
            confirmation_policy: "single_only".to_owned(),
            unparsed_note: String::new(),
            matched: join.matched_count(),
            missed: join.missed_count(),
            extra: join.extra_count(),
            join,
            degraded: DegradedMatch::default(),
            calls: Vec::new(),
            trial_diagnostics: None,
            substring_violations: Vec::new(),
            case_passed: passed,
            execution_error: None,
            duration_ms: Some(1),
            usage: None,
        }
    }

    fn outcome(id: &str, reconciliation: &str, passed: bool) -> CaseOutcome {
        let join = if passed {
            clean_join()
        } else {
            OrdinalJoin {
                missed: vec![1],
                ..OrdinalJoin::default()
            }
        };
        CaseOutcome {
            id: id.to_owned(),
            pool: Pool::Screenshot,
            source_kind: "file".to_owned(),
            join,
            degraded: DegradedMatch::default(),
            reconciliation_status: reconciliation.to_owned(),
            confirmation_policy: "single_only".to_owned(),
            unparsed_note: String::new(),
            stated_item_count: 1,
            duration_ms: Some(1),
            usage: None,
        }
    }

    fn repository() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let manifest = directory.path().join("fixtures/local/m0/manifest.json");
        std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        std::fs::write(&manifest, r#"{"version":1}"#).unwrap();
        (directory, manifest)
    }

    fn save_test_first(
        report: &Report,
        repository: &Path,
        manifest: &Path,
        first: &Path,
    ) -> FirstWrite {
        save_first(
            report,
            repository,
            manifest,
            &std::fs::read(manifest).unwrap(),
            first,
        )
        .unwrap()
    }

    #[test]
    fn pending_manual_is_incomplete_and_exit_two() {
        let (repository, manifest) = repository();
        let cases = vec![report_case("m0-screenshot-001", "failed", true, false)];
        let outcomes = vec![outcome("m0-screenshot-001", "failed", true)];
        let report = Report::build("live", cases, &outcomes);
        let first_path = repository.path().join("output/first.json");
        let written = save_test_first(&report, repository.path(), &manifest, &first_path);

        assert_eq!(written.exit, FormalExit::Incomplete);
        assert_eq!(written.exit.code(), 2);
        assert_eq!(written.envelope.status, "incomplete");
        assert_eq!(written.envelope.verdict, "incomplete");
        assert!(first_path.exists(), "incomplete 也必须永久保存首轮报告");
        let adjudications = written.adjudications_path.unwrap();
        let template: AdjudicationFile =
            serde_json::from_slice(&std::fs::read(adjudications).unwrap()).unwrap();
        assert_eq!(template.adjudications[0].false_alarm, None);
    }

    #[test]
    fn finalize_uses_adjudications_without_agent() {
        let (repository, manifest) = repository();
        let cases = vec![report_case("m0-screenshot-001", "failed", true, false)];
        let outcomes = vec![outcome("m0-screenshot-001", "failed", true)];
        let report = Report::build("live", cases, &outcomes);
        let first_path = repository.path().join("output/first.json");
        let written = save_test_first(&report, repository.path(), &manifest, &first_path);
        let before = std::fs::read(&first_path).unwrap();
        let adjudications_path = written.adjudications_path.unwrap();
        let mut adjudications: AdjudicationFile =
            serde_json::from_slice(&std::fs::read(&adjudications_path).unwrap()).unwrap();
        adjudications.adjudications[0].false_alarm = Some(false);
        std::fs::write(
            &adjudications_path,
            serde_json::to_vec_pretty(&adjudications).unwrap(),
        )
        .unwrap();

        let final_path = repository.path().join("output/final.json");
        let FinalizeResult::Complete { envelope, exit, .. } =
            finalize(&first_path, &final_path).unwrap()
        else {
            panic!("裁定补齐后必须完成");
        };
        assert_eq!(exit, FormalExit::Complete);
        assert_eq!(envelope.stage, "final");
        assert_eq!(envelope.verdict, "go");
        assert_eq!(envelope.evaluation["verdict"], "go");
        assert_eq!(
            std::fs::read(&first_path).unwrap(),
            before,
            "首轮字节必须不变"
        );
        assert!(final_path.exists());
    }

    #[test]
    fn diagnosis_targets_first_failures_union_flaky_and_keeps_official_values() {
        let (repository, manifest) = repository();
        let cases = vec![
            report_case("m0-screenshot-001", "passed", false, false),
            report_case("m0-screenshot-002", "passed", true, true),
        ];
        let outcomes = vec![
            outcome("m0-screenshot-001", "passed", false),
            outcome("m0-screenshot-002", "passed", true),
        ];
        let report = Report::build("live", cases, &outcomes);
        let first_path = repository.path().join("output/first.json");
        let written = save_test_first(&report, repository.path(), &manifest, &first_path);
        let before = std::fs::read(&first_path).unwrap();

        assert_eq!(
            diagnosis_targets(&written.envelope),
            vec!["m0-screenshot-001", "m0-screenshot-002"]
        );
        let diagnosis = repository.path().join("output/diagnosis.json");
        write_diagnosis_new(
            &diagnosis,
            &json!({"roundsPerCase": DIAGNOSIS_ROUNDS, "officialValuesOverwritten": false}),
        )
        .unwrap();
        assert_eq!(std::fs::read(&first_path).unwrap(), before);
        assert_eq!(DIAGNOSIS_ROUNDS, 3);
    }
}
