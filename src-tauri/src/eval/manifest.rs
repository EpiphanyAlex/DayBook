//! `fixtures/manifest.json` 的解析与校验（07 §3.4）。
//!
//! **用例清单是显式的、进 git 的**，不是每次临时从库里挑——「每次跑动态从数据库挑
//! 20 条，跑出来的两轮数字就不可比」，而 07 §3.5 的整套判定建立在「逐条对比上一轮」上。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{EvalError, EvalResult};

/// 分池标记。
///
/// **三个值把「分池」和「对照栏」合成了一个必填字段**：一条用例要么在截图池、要么在
/// 口述池、要么在对照栏；对照栏本来就不属于任何判定池（07 §3.4）。分成两个字段会
/// 允许「pool = screenshot 且 control = true」这种表达不出意义的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pool {
    /// 判定池：交易列表类截图（beachhead）。
    Screenshot,
    /// 判定池：口述。
    Utterance,
    /// **对照栏：如实报数，不计入判定池的任何指标。**
    Control,
}

impl Pool {
    pub fn is_judged(self) -> bool {
        !matches!(self, Self::Control)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Screenshot => "screenshot",
            Self::Utterance => "utterance",
            Self::Control => "control",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestProfile {
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleMetadata {
    /// `transaction_list` / `utterance` / `receipt` / `statement`。
    pub source_type: String,
    /// 截图版式的中性标签。正式截图样本至少两种；不得放机构名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// `single` / `two_to_three` / `four_plus`，只属于口述样本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utterance_length: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Case {
    pub id: String,
    /// 夹具目录，相对仓库根。
    pub dir: String,
    /// 期望集合路径，相对 `dir`。
    pub expected: String,
    /// **分池标记。缺了就拒绝跑**——它是判定口径的一部分，不是展示选项（07 §3.4 口径①）。
    ///
    /// `Option` 是刻意的：`serde` 的缺字段错误会指向整个 manifest，而我们要能说出
    /// **是哪一条用例**缺了标记。
    pub pool: Option<Pool>,
    pub enabled: bool,
    pub added_on: String,
    #[serde(default)]
    pub flaky: bool,
    /// M0 正式判定需要；普通 dry-run / replay / ad-hoc live 为向后兼容可省略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<SampleMetadata>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    /// M0 正式判定使用 `{ "kind": "m0_go_no_go" }`。version 仍为 1。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<ManifestProfile>,
    pub cases: Vec<Case>,
}

/// 一条已经通过校验的用例——`pool` 不再是 `Option`，路径已解析成绝对路径。
#[derive(Debug, Clone)]
pub struct ValidatedCase {
    pub id: String,
    pub pool: Pool,
    pub flaky: bool,
    pub sample: Option<SampleMetadata>,
    pub dir: PathBuf,
    pub expected_path: PathBuf,
    pub env_path: PathBuf,
    /// 只有录过工具调用的用例才能重放。**按文件在不在推断，不另设字段**——它不是判定
    /// 口径的一部分（那才需要显式声明，见 `pool`），而多一个可以和现实不一致的开关，
    /// 迟早会出现「标了 replayable 却没有 tool-calls.json」。
    pub tool_calls_path: Option<PathBuf>,
}

impl ValidatedCase {
    pub fn is_replayable(&self) -> bool {
        self.tool_calls_path.is_some()
    }
}

impl Manifest {
    pub fn load(path: &Path) -> EvalResult<Self> {
        let raw = std::fs::read_to_string(path)?;
        let manifest: Self = serde_json::from_str(&raw)?;
        if manifest.version != 1 {
            return Err(EvalError::Manifest(format!(
                "不认识的 manifest version {}（当前只支持 1）",
                manifest.version
            )));
        }
        Ok(manifest)
    }

    /// 校验全部**启用**的用例，返回可跑的清单。
    ///
    /// 07 §6 的两条验收都落在这里：`--dry-run` 在「用例缺输入 / 期望集合 / `env.json`
    /// 或路径不存在」时非零退出；在「有用例缺分池标记」时同样非零退出。
    ///
    /// **`tool-calls.json` 不是必需的。** §6 那条只点名「输入、期望集合与 `env.json`」，
    /// 而一条**还没跑过**的用例本来就没有工具调用可录——要求它有，等于逼人先跑一轮
    /// 真实 agent 才能把用例加进清单。有就可重放（零额度回归），没有就只能真跑。
    pub fn validate(&self, repository_root: &Path) -> EvalResult<Vec<ValidatedCase>> {
        let mut problems = Vec::new();
        let mut validated = Vec::new();
        let mut seen = Vec::new();

        for case in &self.cases {
            if seen.contains(&case.id) {
                problems.push(format!("用例 id `{}` 重复", case.id));
                continue;
            }
            seen.push(case.id.clone());
            if !case.enabled {
                continue;
            }
            let Some(pool) = case.pool else {
                problems.push(format!(
                    "用例 `{}` 缺 `pool` —— 分池是判定口径的一部分，不是展示选项（07 §3.4 口径①）",
                    case.id
                ));
                continue;
            };
            if case.added_on.len() != 10 {
                problems.push(format!("用例 `{}` 的 `addedOn` 必须是 YYYY-MM-DD", case.id));
            }
            let dir = repository_root.join(&case.dir);
            let expected_path = dir.join(&case.expected);
            let env_path = dir.join("env.json");
            let tool_calls_path = dir.join("tool-calls.json");
            for (label, path) in [
                ("目录", &dir),
                ("期望集合", &expected_path),
                ("env.json", &env_path),
            ] {
                if !path.exists() {
                    problems.push(format!(
                        "用例 `{}` 的{}不存在：{}",
                        case.id,
                        label,
                        path.display()
                    ));
                }
            }
            validated.push(ValidatedCase {
                id: case.id.clone(),
                pool,
                flaky: case.flaky,
                sample: case.sample.clone(),
                dir,
                expected_path,
                env_path,
                tool_calls_path: tool_calls_path.exists().then_some(tool_calls_path),
            });
        }

        if !problems.is_empty() {
            return Err(EvalError::Manifest(problems.join("\n  ")));
        }
        Ok(validated)
    }

    /// [`docs/PRD.md`](../../../docs/PRD.md) §9.4 的正式样本门禁。
    ///
    /// 普通 [`validate`](Self::validate) **刻意不调这里**：manifest version 仍为 1，既有 CI
    /// 夹具没有 profile / sample 元数据也必须继续可 dry-run 与 replay。只有
    /// `--m0-go-no-go` 的入口显式调用本函数。
    pub fn validate_m0(
        &self,
        repository_root: &Path,
        manifest_path: &Path,
    ) -> EvalResult<Vec<ValidatedCase>> {
        let cases = self.validate(repository_root)?;
        let profile = self
            .profile
            .as_ref()
            .ok_or_else(|| EvalError::Manifest("M0 正式 manifest 缺 `profile`".to_owned()))?;
        if profile.kind != "m0_go_no_go" {
            return Err(EvalError::Manifest(format!(
                "M0 正式 manifest 的 profile.kind 必须是 `m0_go_no_go`，收到 `{}`",
                profile.kind
            )));
        }

        let local_root = ensure_m0_manifest_location(repository_root, manifest_path)?;

        for raw in &self.cases {
            let Some(pool) = raw.pool else {
                continue;
            };
            if !neutral_id(&raw.id, pool) {
                return Err(EvalError::Manifest(format!(
                    "正式 case ID `{}` 不是中性编号；必须使用 m0-{}-NNN",
                    raw.id,
                    pool.as_str()
                )));
            }
        }

        let mut screenshot_count = 0_usize;
        let mut utterance_count = 0_usize;
        let mut control_count = 0_usize;
        let mut single_count = 0_usize;
        let mut two_to_three_count = 0_usize;
        let mut four_plus_count = 0_usize;
        let mut layouts = std::collections::BTreeSet::new();
        let mut currencies = std::collections::BTreeSet::new();

        for case in &cases {
            let directory = canonical_existing(&case.dir, &format!("用例 `{}` 的目录", case.id))?;
            ensure_inside(
                &directory,
                &local_root,
                &format!("用例 `{}` 的目录", case.id),
            )?;
            for (label, path) in [
                ("expected.json", &case.expected_path),
                ("env.json", &case.env_path),
            ] {
                let path = canonical_existing(path, &format!("用例 `{}` 的 {label}", case.id))?;
                ensure_inside(&path, &directory, &format!("用例 `{}` 的 {label}", case.id))?;
            }

            let expected = super::expected::ExpectedSet::load(&case.expected_path)?;
            let env = super::replay::FixtureEnv::load(&case.env_path)?;
            let input = canonical_existing(
                &case.dir.join(&env.source.input),
                &format!("用例 `{}` 的输入", case.id),
            )?;
            ensure_inside(&input, &directory, &format!("用例 `{}` 的输入", case.id))?;

            let sample = case.sample.as_ref().ok_or_else(|| {
                EvalError::Manifest(format!("正式用例 `{}` 缺 `sample` 元数据", case.id))
            })?;
            match case.pool {
                Pool::Screenshot => {
                    screenshot_count += 1;
                    if expected.source_kind != "file" || env.source.kind != "file" {
                        return Err(EvalError::Manifest(format!(
                            "正式截图用例 `{}` 的 expected/env 来源类型必须都是 file",
                            case.id
                        )));
                    }
                    if sample.source_type != "transaction_list" {
                        return Err(EvalError::Manifest(format!(
                            "正式截图用例 `{}` 必须是 transaction_list beachhead",
                            case.id
                        )));
                    }
                    let layout = sample.layout.as_deref().filter(|value| {
                        !value.trim().is_empty() && !value.eq_ignore_ascii_case("todo")
                    });
                    let Some(layout) = layout else {
                        return Err(EvalError::Manifest(format!(
                            "正式截图用例 `{}` 缺中性 layout 标签",
                            case.id
                        )));
                    };
                    layouts.insert(layout.to_owned());
                    currencies.extend(expected.items.iter().map(|item| item.currency.clone()));
                }
                Pool::Utterance => {
                    utterance_count += 1;
                    if expected.source_kind != "utterance" || env.source.kind != "utterance" {
                        return Err(EvalError::Manifest(format!(
                            "正式口述用例 `{}` 的 expected/env 来源类型必须都是 utterance",
                            case.id
                        )));
                    }
                    if sample.source_type != "utterance" {
                        return Err(EvalError::Manifest(format!(
                            "正式口述用例 `{}` 的 sample.sourceType 必须是 utterance",
                            case.id
                        )));
                    }
                    let stated = expected.stated_item_count();
                    match sample.utterance_length.as_deref() {
                        Some("single") if stated == 1 => single_count += 1,
                        Some("two_to_three") if (2..=3).contains(&stated) => {
                            two_to_three_count += 1
                        }
                        Some("four_plus") if stated >= 4 => four_plus_count += 1,
                        other => {
                            return Err(EvalError::Manifest(format!(
                                "正式口述用例 `{}` 的 utteranceLength 与 statedItemCount 不符：{:?} / {stated}",
                                case.id, other
                            )))
                        }
                    }
                }
                Pool::Control => {
                    control_count += 1;
                    if expected.source_kind != "file" || env.source.kind != "file" {
                        return Err(EvalError::Manifest(format!(
                            "正式对照用例 `{}` 的 expected/env 来源类型必须都是 file",
                            case.id
                        )));
                    }
                    if !matches!(sample.source_type.as_str(), "receipt" | "statement") {
                        return Err(EvalError::Manifest(format!(
                            "正式对照用例 `{}` 必须是 receipt 或 statement，不能混入 beachhead",
                            case.id
                        )));
                    }
                }
            }
        }

        let valid = (20..=25).contains(&screenshot_count)
            && utterance_count == 20
            && (3..=5).contains(&control_count)
            && (3..=4).contains(&single_count)
            && (8..=10).contains(&two_to_three_count)
            && (6..=8).contains(&four_plus_count)
            && layouts.len() >= 2
            && currencies.len() >= 2;
        if !valid {
            return Err(EvalError::Manifest(format!(
                "M0 正式样本构成不合格：截图 {screenshot_count}（需 20–25）· 口述 {utterance_count}（需 20；single {single_count}/3–4，two_to_three {two_to_three_count}/8–10，four_plus {four_plus_count}/6–8）· 对照 {control_count}（需 3–5）· 版式 {}（至少 2）· 币种 {}（至少 2）",
                layouts.len(),
                currencies.len(),
            )));
        }
        Ok(cases)
    }
}

/// 在读取正式 manifest 内容前先做路径门禁，避免误把仓库外或 committed 夹具当正式样本读入。
pub fn ensure_m0_manifest_location(
    repository_root: &Path,
    manifest_path: &Path,
) -> EvalResult<PathBuf> {
    let local_root = canonical_existing(&repository_root.join("fixtures/local"), "fixtures/local")?;
    let manifest = canonical_existing(manifest_path, "正式 manifest")?;
    ensure_inside(&manifest, &local_root, "正式 manifest")?;
    Ok(local_root)
}

fn canonical_existing(path: &Path, label: &str) -> EvalResult<PathBuf> {
    std::fs::canonicalize(path).map_err(|error| {
        EvalError::Manifest(format!(
            "{label} 不存在或不可读取：{}（{error}）",
            path.display()
        ))
    })
}

fn ensure_inside(path: &Path, parent: &Path, label: &str) -> EvalResult<()> {
    if path.starts_with(parent) {
        return Ok(());
    }
    Err(EvalError::Manifest(format!(
        "{label} 必须位于 fixtures/local/ 内且不得目录逃逸：{}",
        path.display()
    )))
}

pub(crate) fn neutral_id(id: &str, pool: Pool) -> bool {
    let prefix = format!("m0-{}-", pool.as_str());
    let Some(number) = id.strip_prefix(&prefix) else {
        return false;
    };
    number.len() == 3 && number.bytes().all(|byte| byte.is_ascii_digit()) && number != "000"
}

#[cfg(test)]
mod eval {
    use super::*;

    fn manifest_json(pool_line: &str) -> String {
        format!(
            r#"{{"version":1,"cases":[{{"id":"a","dir":"fixtures/ci/x","expected":"expected.json",{pool_line}"enabled":true,"addedOn":"2026-08-17"}}]}}"#
        )
    }

    #[test]
    fn manifest_rejects_case_without_pool() {
        let manifest: Manifest = serde_json::from_str(&manifest_json("")).unwrap();
        let error = manifest
            .validate(Path::new("/nonexistent"))
            .expect_err("缺分池标记必须拒绝");
        let EvalError::Manifest(message) = error else {
            panic!("应当是 manifest 错误");
        };
        assert!(message.contains("缺 `pool`"), "{message}");
    }

    /// 缺分池标记时，报的必须是「缺 pool」而不是「路径不存在」——两者都成立时，
    /// 先说哪一个决定了使用者会去改什么。
    #[test]
    fn missing_pool_is_reported_before_missing_paths() {
        let manifest: Manifest = serde_json::from_str(&manifest_json("")).unwrap();
        let EvalError::Manifest(message) =
            manifest.validate(Path::new("/nonexistent")).unwrap_err()
        else {
            panic!("应当是 manifest 错误");
        };
        assert!(!message.contains("不存在"), "{message}");
    }

    #[test]
    fn manifest_reports_missing_fixture_files() {
        let manifest: Manifest =
            serde_json::from_str(&manifest_json(r#""pool":"screenshot","#)).unwrap();
        let EvalError::Manifest(message) =
            manifest.validate(Path::new("/nonexistent")).unwrap_err()
        else {
            panic!("应当是 manifest 错误");
        };
        assert!(message.contains("env.json"), "{message}");
        assert!(message.contains("期望集合"), "{message}");
    }

    /// 一条**还没跑过**的用例没有工具调用可录。要求它有，等于逼人先跑一轮真实 agent
    /// 才能把用例加进清单（07 §6 那条只点名输入、期望集合与 `env.json`）。
    #[test]
    fn case_without_tool_calls_is_valid_but_not_replayable() {
        let directory = tempfile::tempdir().unwrap();
        let case_dir = directory.path().join("fixtures/ci/x");
        std::fs::create_dir_all(&case_dir).unwrap();
        std::fs::write(case_dir.join("expected.json"), "{}").unwrap();
        std::fs::write(case_dir.join("env.json"), "{}").unwrap();

        let manifest: Manifest =
            serde_json::from_str(&manifest_json(r#""pool":"screenshot","#)).unwrap();
        let cases = manifest.validate(directory.path()).unwrap();
        assert_eq!(cases.len(), 1);
        assert!(!cases[0].is_replayable(), "没有 tool-calls.json 就不可重放");

        std::fs::write(case_dir.join("tool-calls.json"), r#"{"calls":[]}"#).unwrap();
        let cases = manifest.validate(directory.path()).unwrap();
        assert!(cases[0].is_replayable(), "有了就可重放");
    }

    fn formal_repository() -> (tempfile::TempDir, PathBuf) {
        let repository = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repository.path().join("fixtures/local")).unwrap();
        let manifest_path = crate::eval::init::initialize(
            repository.path(),
            Path::new("fixtures/local/m0-formal"),
            &crate::eval::init::InitOptions::default(),
        )
        .unwrap();
        let mut manifest = Manifest::load(&manifest_path).unwrap();
        for (index, case) in manifest.cases.iter_mut().enumerate() {
            let pool = case.pool.unwrap();
            let directory = repository.path().join(&case.dir);
            let (kind, input) = if pool == Pool::Utterance {
                ("utterance", "input.txt")
            } else {
                ("file", "input.png")
            };
            if pool == Pool::Screenshot {
                case.sample.as_mut().unwrap().layout = Some(format!("layout-{:03}", index % 2 + 1));
            }
            let item_count = match case
                .sample
                .as_ref()
                .and_then(|sample| sample.utterance_length.as_deref())
            {
                Some("single") => 1,
                Some("two_to_three") => 2,
                Some("four_plus") => 4,
                _ => 1,
            };
            let currency = if pool == Pool::Screenshot && index % 2 == 0 {
                "AUD"
            } else if pool == Pool::Screenshot {
                "CNY"
            } else {
                "AUD"
            };
            let items = (1..=item_count)
                .map(|ordinal| {
                    serde_json::json!({
                        "sourceOrdinal": ordinal,
                        "occurredOn": "2026-08-24",
                        "amountMinor": "100",
                        "currency": currency,
                        "direction": "expense"
                    })
                })
                .collect::<Vec<_>>();
            std::fs::write(
                directory.join("expected.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "sourceKind": kind,
                    "statedItemCount": item_count,
                    "items": items,
                }))
                .unwrap(),
            )
            .unwrap();
            std::fs::write(
                directory.join(input),
                if kind == "file" {
                    "png"
                } else {
                    "一段口述"
                },
            )
            .unwrap();
            std::fs::write(
                directory.join("env.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "toolSurfaceVersion": crate::agent::registry::tool_surface_version(),
                    "appVersion": "0.1.0",
                    "schemaVersion": crate::db::LATEST_SCHEMA_VERSION,
                    "baseCurrency": "AUD",
                    "source": { "id": format!("source-{index}"), "kind": kind, "input": input },
                    "attempt": {
                        "backendId": "fixture", "backendVersion": "1", "modelId": null,
                        "promptHash": "0".repeat(64), "effectiveCapabilityHash": "0".repeat(64)
                    },
                    "expectedState": "parsed",
                    "expectedReconciliationStatus": if kind == "file" { "passed" } else { "not_applicable" },
                    "expectedConfirmationPolicy": if kind == "file" { "reconciled_batch" } else { "user_attested_batch" }
                }))
                .unwrap(),
            )
            .unwrap();
        }
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        (repository, manifest_path)
    }

    #[test]
    fn formal_manifest_enforces_m0_composition_only_in_formal_mode() {
        let (repository, manifest_path) = formal_repository();
        let manifest = Manifest::load(&manifest_path).unwrap();
        assert_eq!(
            manifest
                .validate_m0(repository.path(), &manifest_path)
                .unwrap()
                .len(),
            46
        );

        let mut legacy = manifest.clone();
        legacy.profile = None;
        for case in &mut legacy.cases {
            case.sample = None;
        }
        // version 1 的旧 manifest 仍走普通门禁；只有正式模式要求新增元数据与构成。
        assert_eq!(legacy.version, 1);
        assert_eq!(legacy.validate(repository.path()).unwrap().len(), 46);
        let error = legacy
            .validate_m0(repository.path(), &manifest_path)
            .unwrap_err()
            .to_string();
        assert!(error.contains("profile"), "{error}");
    }

    #[test]
    fn formal_manifest_rejects_committed_fixtures_and_non_neutral_ids() {
        let (repository, manifest_path) = formal_repository();
        let mut manifest = Manifest::load(&manifest_path).unwrap();
        manifest.cases[0].id = "real-bank-name".to_owned();
        let error = manifest
            .validate_m0(repository.path(), &manifest_path)
            .unwrap_err()
            .to_string();
        assert!(error.contains("中性编号"), "{error}");

        let ci = repository.path().join("fixtures/ci/formal-manifest.json");
        std::fs::create_dir_all(ci.parent().unwrap()).unwrap();
        let valid = Manifest::load(&manifest_path).unwrap();
        std::fs::write(&ci, serde_json::to_vec_pretty(&valid).unwrap()).unwrap();
        let error = valid
            .validate_m0(repository.path(), &ci)
            .unwrap_err()
            .to_string();
        assert!(error.contains("fixtures/local"), "{error}");
    }

    #[test]
    fn control_pool_is_not_judged() {
        assert!(!Pool::Control.is_judged());
        assert!(Pool::Screenshot.is_judged());
        assert!(Pool::Utterance.is_judged());
    }
}
