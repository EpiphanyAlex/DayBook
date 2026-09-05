//! M0 正式 verdict 的持久报告与人工裁定协议。
//!
//! 本模块**刻意不引用 agent runtime / backend / 进程 API**。首轮与诊断的模型调用在
//! `formal.rs`；这里仅处理本机 JSON：immutable first report、独立 adjudications、零额度
//! finalize、诊断目标集合与 create-new 写入。

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::{
    manifest::ValidatedCase, metrics::FALSE_ALARM_MAX_PERMILLE, replay::FixtureEnv, report::Report,
    EvalError, EvalResult,
};

pub const FORMAT_VERSION: u32 = 2;
pub const LEGACY_FORMAT_VERSION: u32 = 1;
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
    /// 兼容 v1：只覆盖 manifest，不能冒充完整 fixture-set 指纹。
    pub manifest_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture_set_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture_file_count: Option<usize>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureSetSnapshot {
    pub sha256: String,
    pub file_count: usize,
}

/// formal v2 的规范化 fixture set：manifest + 每个启用 case 的 expected/env/input。
pub fn snapshot_fixture_set_from_manifest(
    repository_root: &Path,
    manifest_path: &Path,
    cases: &[ValidatedCase],
    parsed_manifest_bytes: &[u8],
) -> EvalResult<FixtureSetSnapshot> {
    let mut files = BTreeMap::<String, Vec<u8>>::new();
    let (path, content) = read_fixture_file(repository_root, manifest_path)?;
    if content != parsed_manifest_bytes {
        return Err(EvalError::Manifest(
            "manifest 在解析校验后已变化，拒绝冻结混合集合".to_owned(),
        ));
    }
    files.insert(path, content);
    for case in cases {
        let (path, content) = read_fixture_file(repository_root, &case.expected_path)?;
        files.insert(path, content);
        let (env_path, env_bytes) = read_fixture_file(repository_root, &case.env_path)?;
        let env: FixtureEnv = serde_json::from_slice(&env_bytes)?;
        files.insert(env_path, env_bytes);
        let (path, content) = read_fixture_file(repository_root, &case.dir.join(env.source.input))?;
        files.insert(path, content);
    }

    let mut hash = Sha256::new();
    for (path, content) in &files {
        let path = path.as_bytes();
        hash.update(path.len().to_string().as_bytes());
        hash.update(b":");
        hash.update(path);
        hash.update(content.len().to_string().as_bytes());
        hash.update(b":");
        hash.update(content);
    }
    Ok(FixtureSetSnapshot {
        sha256: format!("{:x}", hash.finalize()),
        file_count: files.len(),
    })
}

fn snapshot_fixture_set(
    repository_root: &Path,
    manifest_path: &Path,
    cases: &[ValidatedCase],
) -> EvalResult<FixtureSetSnapshot> {
    snapshot_fixture_set_from_manifest(
        repository_root,
        manifest_path,
        cases,
        &std::fs::read(manifest_path)?,
    )
}

fn read_fixture_file(repository_root: &Path, path: &Path) -> EvalResult<(String, Vec<u8>)> {
    Ok((
        repository_relative(repository_root, path)?,
        std::fs::read(path)?,
    ))
}

pub fn assert_fixture_set_unchanged(
    repository_root: &Path,
    manifest_path: &Path,
    cases: &[ValidatedCase],
    expected: &FixtureSetSnapshot,
) -> EvalResult<()> {
    let actual = snapshot_fixture_set(repository_root, manifest_path, cases)?;
    if actual != *expected {
        return Err(EvalError::Manifest(format!(
            "正式 fixture set 已变化：期望 {} / {} files，实际 {} / {} files",
            expected.sha256, expected.file_count, actual.sha256, actual.file_count
        )));
    }
    Ok(())
}

pub fn fixture_set_from_first(first: &FormalEnvelope) -> EvalResult<FixtureSetSnapshot> {
    if first.format_version != FORMAT_VERSION {
        return Err(EvalError::Fixture(
            "v1 formal 报告只可作为历史证据只读打开；finalize / diagnosis 需要 v2 fixtureSetSha256"
                .to_owned(),
        ));
    }
    let sha256 = first
        .fixture_set_sha256
        .clone()
        .ok_or_else(|| EvalError::Fixture("formal v2 首轮缺 fixtureSetSha256".to_owned()))?;
    let file_count = first
        .fixture_file_count
        .ok_or_else(|| EvalError::Fixture("formal v2 首轮缺 fixtureFileCount".to_owned()))?;
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || file_count == 0
    {
        return Err(EvalError::Fixture(
            "formal v2 首轮的 fixtureSetSha256 / fixtureFileCount 形状不合法".to_owned(),
        ));
    }
    Ok(FixtureSetSnapshot { sha256, file_count })
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
    cases: &[ValidatedCase],
    fixture_set: &FixtureSetSnapshot,
    output_path: &Path,
) -> EvalResult<FirstWrite> {
    ensure_private_report_path(repository_root, output_path)?;
    assert_fixture_set_unchanged(repository_root, manifest_path, cases, fixture_set)?;
    let manifest_path_text = repository_relative(repository_root, manifest_path)?;
    if !manifest_path_text.starts_with("fixtures/local/") {
        return Err(EvalError::Manifest(
            "M0 正式报告只能引用 fixtures/local/ 下的 manifest".to_owned(),
        ));
    }
    let manifest_bytes = std::fs::read(manifest_path)?;
    let manifest_sha256 = format!("{:x}", Sha256::digest(&manifest_bytes));
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
    let report_id = report_id(&fixture_set.sha256, &created_at, &evaluation);
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
        fixture_set_sha256: Some(fixture_set.sha256.clone()),
        fixture_file_count: Some(fixture_set.file_count),
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
    ensure_v2_first(&first)?;
    let repository_root = repository_root_for_report(first_report)?;
    ensure_private_report_path(&repository_root, first_report)?;
    ensure_private_report_path(&repository_root, output_path)?;
    ensure_sibling_report_path(first_report, output_path)?;
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
    let fixture_set = fixture_set_from_first(&first)?;
    first.report_id = report_id(&fixture_set.sha256, &first.created_at, &first.evaluation);
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

pub fn ensure_private_report_path(repository_root: &Path, path: &Path) -> EvalResult<()> {
    let root = std::fs::canonicalize(repository_root)?;
    let parent = resolved_directory(
        path.parent()
            .ok_or_else(|| EvalError::Usage("formal 报告路径缺父目录".to_owned()))?,
    )?;
    // 隐私边界固定在仓库根下，不能把 output → docs 的目标提升为白名单。
    let allowed = [root.join("output"), root.join("fixtures/local")];
    if allowed
        .iter()
        .any(|directory| parent.starts_with(directory))
        && !std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        Ok(())
    } else {
        Err(EvalError::Usage(
            "formal 持久报告只能写入当前仓库 Git 已忽略的 output/ 或 fixtures/local/".to_owned(),
        ))
    }
}

pub fn ensure_fixture_set_for_first(
    repository_root: &Path,
    first: &FormalEnvelope,
    cases: &[ValidatedCase],
) -> EvalResult<PathBuf> {
    let path = repository_root.join(&first.manifest_path);
    let expected = fixture_set_from_first(first)?;
    assert_fixture_set_unchanged(repository_root, &path, cases, &expected)?;
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
    let root = repository_root_for_report(path)?;
    ensure_private_report_path(&root, path)?;
    write_json_new(path, report)
}

fn ensure_first(first: &FormalEnvelope) -> EvalResult<()> {
    if !matches!(first.format_version, LEGACY_FORMAT_VERSION | FORMAT_VERSION)
        || first.mode != "m0_go_no_go"
        || first.stage != "first"
    {
        return Err(EvalError::Fixture(
            "只接受 M0 正式首轮报告；v1 可只读，新的 finalize / diagnosis 只接受 v2".to_owned(),
        ));
    }
    if first.format_version == FORMAT_VERSION {
        fixture_set_from_first(first)?;
    }
    Ok(())
}

fn ensure_v2_first(first: &FormalEnvelope) -> EvalResult<()> {
    ensure_first(first)?;
    if first.format_version != FORMAT_VERSION {
        return fixture_set_from_first(first).map(|_| ());
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

fn repository_root_for_report(path: &Path) -> EvalResult<PathBuf> {
    let parent = resolved_directory(
        path.parent()
            .ok_or_else(|| EvalError::Usage("formal 报告路径缺父目录".to_owned()))?,
    )?;
    parent
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            EvalError::Usage(
                "formal 报告必须位于 Git 仓库已忽略的 output/ 或 fixtures/local/".to_owned(),
            )
        })
}

fn ensure_sibling_report_path(first_report: &Path, output_path: &Path) -> EvalResult<()> {
    let first_parent = resolved_directory(
        first_report
            .parent()
            .ok_or_else(|| EvalError::Usage("首轮报告路径缺父目录".to_owned()))?,
    )?;
    let output_parent = resolved_directory(
        output_path
            .parent()
            .ok_or_else(|| EvalError::Usage("final 报告路径缺父目录".to_owned()))?,
    )?;
    if first_parent != output_parent {
        return Err(EvalError::Usage(
            "final 必须与 immutable first 写在同一 Git 已忽略目录".to_owned(),
        ));
    }
    Ok(())
}

fn resolved_directory(path: &Path) -> EvalResult<PathBuf> {
    // 未存在的尾段不能只拼回 ..；保守拒绝，而不是模拟符号链接与 .. 的组合语义。
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(EvalError::Usage(
            "formal Git 已忽略报告路径不得包含 ..".to_owned(),
        ));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut ancestor = absolute.as_path();
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| EvalError::Usage(format!("无法解析报告目录：{}", path.display())))?;
    }
    let canonical = std::fs::canonicalize(ancestor)?;
    let suffix = absolute
        .strip_prefix(ancestor)
        .map_err(|_| EvalError::Usage(format!("无法规范化报告目录：{}", path.display())))?;
    Ok(canonical.join(suffix))
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
    normalized_utf8_relative(relative)
}

fn normalized_utf8_relative(relative: &Path) -> EvalResult<String> {
    let relative = relative.to_str().ok_or_else(|| {
        EvalError::Manifest("formal fixture 路径必须是 UTF-8，不能用有损替换参与 hash".to_owned())
    })?;
    Ok(relative.replace(std::path::MAIN_SEPARATOR, "/"))
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
    let root = repository_root_for_report(path)?;
    ensure_private_report_path(&root, path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // 所有持久写入共用此处复验（包括 sidecar 与独立 diagnosis 调用者）。
    ensure_private_report_path(&root, path)?;
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
        manifest::{Pool, ValidatedCase},
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
            hard_field_diffs: Vec::new(),
            reconciliation_evidence: None,
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
            scope_evaluation: None,
            stated_item_count: 1,
            duration_ms: Some(1),
            usage: None,
        }
    }

    fn repository() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join(".git")).unwrap();
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
        let snapshot = snapshot_fixture_set(repository, manifest, &[]).unwrap();
        save_first(report, repository, manifest, &[], &snapshot, first).unwrap()
    }

    #[test]
    fn formal_private_paths_reject_parent_traversal() {
        let (repository, manifest) = repository();
        let report = Report::build("live", Vec::new(), &[]);
        let snapshot = snapshot_fixture_set(repository.path(), &manifest, &[]).unwrap();
        for prefix in ["output", "fixtures/local"] {
            let path = repository.path().join(prefix).join("new/../../public.json");
            assert!(
                save_first(&report, repository.path(), &manifest, &[], &snapshot, &path).is_err()
            );
            assert!(!repository.path().join("public.json").exists());
            assert!(!repository.path().join(prefix).join("new").exists());
        }
        assert!(ensure_private_report_path(
            repository.path(),
            &repository.path().join("output/new/first.json")
        )
        .is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn formal_private_paths_reject_symlink_escape() {
        use std::os::unix::fs::symlink;
        let (repository, manifest) = repository();
        let public = repository.path().join("docs");
        std::fs::create_dir(&public).unwrap();
        symlink(&public, repository.path().join("output")).unwrap();
        let report = Report::build("live", Vec::new(), &[]);
        let snapshot = snapshot_fixture_set(repository.path(), &manifest, &[]).unwrap();
        let first = repository.path().join("output/first.json");
        assert!(save_first(
            &report,
            repository.path(),
            &manifest,
            &[],
            &snapshot,
            &first
        )
        .is_err());
        assert!(!public.join("first.json").exists());
        for path in [public.join("new/first.json"), first] {
            assert!(ensure_private_report_path(repository.path(), &path).is_err());
        }
        symlink(&public, repository.path().join("fixtures/local/link")).unwrap();
        assert!(ensure_private_report_path(
            repository.path(),
            &repository.path().join("fixtures/local/link/first.json")
        )
        .is_err());
    }

    #[test]
    fn formal_diagnosis_writer_rejects_public_caller_path() {
        let (repository, _) = repository();
        let public = repository.path().join("public-diagnosis.json");
        assert!(write_diagnosis_new(&public, &json!({"synthetic":true})).is_err());
        assert!(!public.exists());
        let private = repository.path().join("output/new/diagnosis.json");
        write_diagnosis_new(&private, &json!({"synthetic":true})).unwrap();
        assert!(private.exists());
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

    fn fixture_set_repository() -> (tempfile::TempDir, PathBuf, Vec<ValidatedCase>) {
        let repository = tempfile::tempdir().unwrap();
        std::fs::create_dir(repository.path().join(".git")).unwrap();
        let manifest = repository.path().join("fixtures/local/set/manifest.json");
        std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
        std::fs::write(&manifest, br#"{"version":1,"cases":[]}"#).unwrap();
        let mut cases = Vec::new();
        for index in 1..=2 {
            let dir = repository
                .path()
                .join(format!("fixtures/local/set/case-{index}"));
            std::fs::create_dir_all(&dir).unwrap();
            let expected_path = dir.join("expected.json");
            let env_path = dir.join("env.json");
            std::fs::write(&expected_path, format!("expected-{index}")).unwrap();
            std::fs::write(dir.join("input.txt"), format!("input-{index}")).unwrap();
            std::fs::write(
                &env_path,
                serde_json::to_vec(&json!({
                    "toolSurfaceVersion":"v","appVersion":"a","schemaVersion":1,
                    "baseCurrency":"AUD",
                    "source":{"id":format!("source-{index}"),"kind":"utterance","input":"input.txt"},
                    "attempt":{"backendId":"b","backendVersion":"1","modelId":null,
                        "promptHash":"p","effectiveCapabilityHash":"h"},
                    "expectedState":"parsed","expectedReconciliationStatus":"not_applicable",
                    "expectedConfirmationPolicy":"user_attested_batch"
                }))
                .unwrap(),
            )
            .unwrap();
            cases.push(ValidatedCase {
                id: format!("case-{index}"),
                pool: Pool::Utterance,
                flaky: false,
                sample: None,
                dir,
                expected_path,
                env_path,
                tool_calls_path: None,
            });
        }
        (repository, manifest, cases)
    }

    #[test]
    fn fixture_set_snapshot_rejects_changed_manifest_after_validation() {
        let (repository, manifest, cases) = fixture_set_repository();
        let validated_manifest_bytes = std::fs::read(&manifest).unwrap();
        // 确定性模拟 load/validate 与 snapshot 之间增加启用 case 的间隙。
        std::fs::write(
            &manifest,
            br#"{"version":1,"cases":[{"id":"new-enabled-case"}]}"#,
        )
        .unwrap();
        assert!(snapshot_fixture_set_from_manifest(
            repository.path(),
            &manifest,
            &cases,
            &validated_manifest_bytes
        )
        .is_err());
    }

    #[test]
    fn fixture_set_sha256_covers_all_formal_inputs() {
        let (repository, manifest, cases) = fixture_set_repository();
        let original = snapshot_fixture_set(repository.path(), &manifest, &cases).unwrap();
        assert_eq!(original.file_count, 7);

        let mut reversed = cases.clone();
        reversed.reverse();
        assert_eq!(
            snapshot_fixture_set(repository.path(), &manifest, &reversed)
                .unwrap()
                .sha256,
            original.sha256,
            "case / 路径枚举顺序不能影响规范化 hash"
        );

        for path in [
            manifest.clone(),
            cases[0].expected_path.clone(),
            cases[0].env_path.clone(),
            cases[0].dir.join("input.txt"),
        ] {
            let before = std::fs::read(&path).unwrap();
            let changed = if path == cases[0].env_path {
                String::from_utf8(before.clone())
                    .unwrap()
                    .replacen("AUD", "CNY", 1)
                    .into_bytes()
            } else {
                let mut changed = before.clone();
                changed.push(b'!');
                changed
            };
            std::fs::write(&path, changed).unwrap();
            assert_ne!(
                snapshot_fixture_set(repository.path(), &manifest, &cases)
                    .unwrap()
                    .sha256,
                original.sha256,
                "{} 改 1 byte 必须改变 fixtureSetSha256",
                path.display()
            );
            std::fs::write(&path, before).unwrap();
        }

        let disabled = repository
            .path()
            .join("fixtures/local/set/disabled-input.txt");
        std::fs::write(&disabled, "ignored").unwrap();
        assert_eq!(
            snapshot_fixture_set(repository.path(), &manifest, &cases)
                .unwrap()
                .sha256,
            original.sha256,
            "未启用 / 未列入 ValidatedCase 的文件不进入 set"
        );

        #[cfg(unix)]
        {
            use std::{ffi::OsString, os::unix::ffi::OsStringExt};
            let non_utf8 = PathBuf::from(OsString::from_vec(vec![b'e', 0xff, b'.', b'j']));
            assert!(
                normalized_utf8_relative(&non_utf8).is_err(),
                "非 UTF-8 路径必须拒绝，不能用 U+FFFD 有损替换参与 hash"
            );
        }
    }

    #[test]
    fn first_final_and_diagnosis_preserve_fixture_set() {
        let (repository, manifest, cases) = fixture_set_repository();
        let snapshot = snapshot_fixture_set(repository.path(), &manifest, &cases).unwrap();
        assert_fixture_set_unchanged(repository.path(), &manifest, &cases, &snapshot).unwrap();

        let reports = vec![report_case("m0-screenshot-001", "failed", true, false)];
        let outcomes = vec![outcome("m0-screenshot-001", "failed", true)];
        let report = Report::build("live", reports, &outcomes);
        let first_path = repository.path().join("output/first.json");
        let outside_first = repository.path().join("first-with-real-evidence.json");
        assert!(
            save_first(
                &report,
                repository.path(),
                &manifest,
                &cases,
                &snapshot,
                &outside_first,
            )
            .is_err(),
            "bounded 真实证据仍只能写进 ignored output / fixtures/local"
        );
        assert!(!outside_first.exists());
        let input_path = cases[0].dir.join("input.txt");
        let original_input = std::fs::read(&input_path).unwrap();
        std::fs::write(&input_path, "changed before first save").unwrap();
        assert!(
            save_first(
                &report,
                repository.path(),
                &manifest,
                &cases,
                &snapshot,
                &first_path,
            )
            .is_err(),
            "首轮保存动作自身必须检测 backend 运行期间的 fixture 漂移"
        );
        assert!(!first_path.exists());
        std::fs::write(&input_path, original_input).unwrap();
        let written = save_first(
            &report,
            repository.path(),
            &manifest,
            &cases,
            &snapshot,
            &first_path,
        )
        .unwrap();
        assert_eq!(written.envelope.format_version, 2);
        assert_eq!(
            written.envelope.fixture_set_sha256.as_deref(),
            Some(snapshot.sha256.as_str())
        );
        assert_eq!(
            written.envelope.fixture_file_count,
            Some(snapshot.file_count)
        );

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
        let first_before_finalize = std::fs::read(&first_path).unwrap();
        assert!(
            finalize(
                &first_path,
                &repository.path().join("final-with-real-evidence.json")
            )
            .is_err(),
            "final 不能把 bounded 真实证据复制到 first 目录之外"
        );
        let FinalizeResult::Complete { envelope, .. } = finalize(&first_path, &final_path).unwrap()
        else {
            panic!("裁定补齐后必须完成");
        };
        assert_eq!(envelope.fixture_set_sha256, Some(snapshot.sha256.clone()));
        assert_eq!(envelope.fixture_file_count, Some(snapshot.file_count));
        assert_eq!(std::fs::read(&first_path).unwrap(), first_before_finalize);

        let mut legacy = written.envelope.clone();
        legacy.format_version = LEGACY_FORMAT_VERSION;
        legacy.fixture_set_sha256 = None;
        legacy.fixture_file_count = None;
        let legacy_path = repository.path().join("output/legacy-v1.json");
        std::fs::write(&legacy_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
        assert_eq!(read_first(&legacy_path).unwrap().format_version, 1);
        assert!(
            finalize(
                &legacy_path,
                &repository.path().join("output/legacy-final.json")
            )
            .is_err(),
            "v1 只能只读，不能冒充 v2 继续 finalize"
        );

        std::fs::write(&input_path, "changed").unwrap();
        assert!(
            assert_fixture_set_unchanged(repository.path(), &manifest, &cases, &snapshot).is_err(),
            "diagnosis 必须在 backend 前拒绝完整 set 漂移"
        );
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
