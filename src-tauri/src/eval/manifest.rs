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
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    pub cases: Vec<Case>,
}

/// 一条已经通过校验的用例——`pool` 不再是 `Option`，路径已解析成绝对路径。
#[derive(Debug, Clone)]
pub struct ValidatedCase {
    pub id: String,
    pub pool: Pool,
    pub flaky: bool,
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

    #[test]
    fn control_pool_is_not_judged() {
        assert!(!Pool::Control.is_judged());
        assert!(Pool::Screenshot.is_judged());
        assert!(Pool::Utterance.is_judged());
    }
}
