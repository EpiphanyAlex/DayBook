//! 截图专项的零额度冻结与报告骨架。真实执行仍受 01 的 Codex 密封权限门槛约束。

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{EvalError, EvalResult};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonInput {
    pub case_id: String,
    pub original: PathBuf,
    pub expected: PathBuf,
    pub expected_items: usize,
    pub annotated: bool,
    pub source_kind: String,
    pub currencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonSnapshot {
    pub algorithm: &'static str,
    pub sha256: String,
    pub file_count: usize,
    pub case_ids: Vec<String>,
}

/// 调用方提供从 manifest/selection 解析出的完整文件清单；其中可包括合计真值与人工裁定依据。
/// 指纹使用独立域标识，绝不冒充 formal v2 的 `fixtureSetSha256`。
pub fn preflight(
    repository_root: &Path,
    manifest: &Path,
    selection: &Path,
    inputs: &[ComparisonInput],
    truth_files: &[PathBuf],
    human_confirmed: bool,
) -> EvalResult<ComparisonSnapshot> {
    if !human_confirmed {
        return Err(EvalError::Fixture("截图专项缺少人工确认状态".to_owned()));
    }
    if inputs.len() != 9 {
        return Err(EvalError::Fixture("截图专项必须恰好 9 张截图".to_owned()));
    }
    if inputs
        .iter()
        .map(|input| input.expected_items)
        .sum::<usize>()
        != 56
    {
        return Err(EvalError::Fixture("截图专项真值必须恰好 56 笔".to_owned()));
    }
    if inputs.iter().any(|input| {
        !input.annotated
            || input.source_kind != "file"
            || input.currencies.is_empty()
            || input.currencies.iter().any(|currency| currency != "AUD")
    }) {
        return Err(EvalError::Fixture(
            "截图专项须为人工核对的 AUD 文件来源".to_owned(),
        ));
    }
    let mut case_ids = Vec::with_capacity(9);
    for input in inputs {
        if input.case_id.is_empty() || case_ids.contains(&input.case_id) {
            return Err(EvalError::Fixture("截图专项 case ID 缺失或重复".to_owned()));
        }
        case_ids.push(input.case_id.clone());
    }

    let root = std::fs::canonicalize(repository_root)?;
    let mut files = BTreeMap::<String, Vec<u8>>::new();
    for path in [manifest, selection]
        .into_iter()
        .chain(
            inputs
                .iter()
                .flat_map(|input| [input.original.as_path(), input.expected.as_path()]),
        )
        .chain(truth_files.iter().map(PathBuf::as_path))
    {
        let canonical = std::fs::canonicalize(path)?;
        let relative = canonical
            .strip_prefix(&root)
            .map_err(|_| EvalError::Fixture("截图专项文件逃出仓库根目录".to_owned()))?;
        let key = relative.to_string_lossy().replace('\\', "/");
        if files.insert(key, std::fs::read(&canonical)?).is_some() {
            return Err(EvalError::Fixture("截图专项冻结文件重复".to_owned()));
        }
    }
    if truth_files.is_empty() {
        return Err(EvalError::Fixture(
            "截图专项缺少合计真值或人工裁定依据".to_owned(),
        ));
    }
    let mut digest = Sha256::new();
    digest.update(b"daybook-backend-screenshot-comparison-v1\0");
    for (path, bytes) in &files {
        digest.update(path.len().to_string().as_bytes());
        digest.update(b":");
        digest.update(path.as_bytes());
        digest.update(bytes.len().to_string().as_bytes());
        digest.update(b":");
        digest.update(bytes);
    }
    Ok(ComparisonSnapshot {
        algorithm: "daybook-backend-screenshot-comparison-v1",
        sha256: format!("{:x}", digest.finalize()),
        file_count: files.len(),
        case_ids,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonCaseResult {
    pub case_id: String,
    pub source_id: Option<String>,
    pub attempt_id: Option<String>,
    pub backend_id: String,
    pub backend_version: Option<String>,
    pub requested_model_mode: String,
    pub requested_model_id: Option<String>,
    pub actual_model_id: Option<String>,
    pub effective_capability_hash: Option<String>,
    pub outcome: String,
    pub error_code: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonReport {
    pub kind: &'static str,
    pub format_version: u32,
    pub status: String,
    pub snapshot: ComparisonSnapshot,
    pub claude_code: Vec<ComparisonCaseResult>,
    pub codex: Vec<ComparisonCaseResult>,
}

impl ComparisonReport {
    pub fn validate(&self) -> EvalResult<()> {
        if self.kind != "backend_screenshot_comparison" || self.format_version != FORMAT_VERSION {
            return Err(EvalError::Fixture("截图专项报告格式不匹配".to_owned()));
        }
        for (backend, results) in [(&"claude-code", &self.claude_code), (&"codex", &self.codex)] {
            if results.len() > 9 || (self.status == "complete" && results.len() != 9) {
                return Err(EvalError::Fixture(
                    "截图专项报告缺少结果或结果过多".to_owned(),
                ));
            }
            for (index, result) in results.iter().enumerate() {
                if result.backend_id != *backend || result.case_id != self.snapshot.case_ids[index]
                {
                    return Err(EvalError::Fixture(
                        "截图专项报告的来源顺序或后端不符".to_owned(),
                    ));
                }
            }
        }
        if !matches!(self.status.as_str(), "complete" | "incomplete") {
            return Err(EvalError::Fixture("截图专项报告状态不合法".to_owned()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod eval {
    use super::*;

    #[test]
    fn comparison_preflight_freezes_every_declared_file_and_rejects_bad_composition() {
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join("manifest.json");
        let selection = root.path().join("selection.json");
        let truth = root.path().join("truth.json");
        std::fs::write(&manifest, b"manifest").unwrap();
        std::fs::write(&selection, b"selection").unwrap();
        std::fs::write(&truth, b"truth").unwrap();
        let inputs: Vec<_> = (0..9)
            .map(|index| {
                let original = root.path().join(format!("{index}.png"));
                let expected = root.path().join(format!("{index}.json"));
                std::fs::write(&original, [index as u8]).unwrap();
                std::fs::write(&expected, b"expected").unwrap();
                ComparisonInput {
                    case_id: format!("case-{index}"),
                    original,
                    expected,
                    expected_items: if index == 0 { 8 } else { 6 },
                    annotated: true,
                    source_kind: "file".to_owned(),
                    currencies: vec!["AUD".to_owned()],
                }
            })
            .collect();
        let first = preflight(
            root.path(),
            &manifest,
            &selection,
            &inputs,
            std::slice::from_ref(&truth),
            true,
        )
        .unwrap();
        assert_eq!(first.file_count, 21);
        std::fs::write(&truth, b"changed").unwrap();
        let changed =
            preflight(root.path(), &manifest, &selection, &inputs, &[truth], true).unwrap();
        assert_ne!(first.sha256, changed.sha256);
        assert!(preflight(root.path(), &manifest, &selection, &inputs[..8], &[], true).is_err());
        assert!(preflight(root.path(), &manifest, &selection, &inputs, &[], false).is_err());
    }

    #[test]
    fn incomplete_report_cannot_claim_complete_comparison() {
        let snapshot = ComparisonSnapshot {
            algorithm: "daybook-backend-screenshot-comparison-v1",
            sha256: "x".to_owned(),
            file_count: 21,
            case_ids: (0..9).map(|index| format!("case-{index}")).collect(),
        };
        let report = ComparisonReport {
            kind: "backend_screenshot_comparison",
            format_version: FORMAT_VERSION,
            status: "complete".to_owned(),
            snapshot,
            claude_code: Vec::new(),
            codex: Vec::new(),
        };
        assert!(report.validate().is_err());
    }
}
