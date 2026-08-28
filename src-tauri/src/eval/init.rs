//! M0 正式样本清单初始化器。
//!
//! **只建中性目录与 manifest 骨架**：不加载 agent backend、不复制原件，也不把真实输入
//! 绝对路径写进任何文件。使用者随后在 `fixtures/local/` 内自行放入 `input.*`、
//! `expected.json` 与 `env.json`。输出若不在 `fixtures/local/`（尤其是会进 git 的
//! `fixtures/ci/`）则拒绝。

use std::path::{Component, Path, PathBuf};

use super::{
    manifest::{Case, Manifest, ManifestProfile, Pool, SampleMetadata},
    EvalError, EvalResult,
};

#[derive(Debug, Clone)]
pub struct InitOptions {
    pub screenshot_count: usize,
    pub control_count: usize,
    pub utterance_single: usize,
    pub utterance_two_to_three: usize,
    pub utterance_four_plus: usize,
    pub added_on: String,
}

impl Default for InitOptions {
    fn default() -> Self {
        Self {
            screenshot_count: 22,
            control_count: 4,
            utterance_single: 4,
            utterance_two_to_three: 9,
            utterance_four_plus: 7,
            added_on: "2026-08-24".to_owned(),
        }
    }
}

impl InitOptions {
    fn validate(&self) -> EvalResult<()> {
        if !(20..=25).contains(&self.screenshot_count)
            || !(3..=5).contains(&self.control_count)
            || !(3..=4).contains(&self.utterance_single)
            || !(8..=10).contains(&self.utterance_two_to_three)
            || !(6..=8).contains(&self.utterance_four_plus)
            || self.utterance_single + self.utterance_two_to_three + self.utterance_four_plus != 20
        {
            return Err(EvalError::Usage(
                "M0 构成必须是：截图 20–25、对照 3–5、口述 single 3–4 / two_to_three 8–10 / four_plus 6–8，且口述合计 20"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

/// 建 `<out>/manifest.json` 与 `<out>/cases/*` 空目录。
///
/// manifest 只含中性编号与相对的夹具目录；**没有任何 input 字段或调用方原始路径**。
pub fn initialize(
    repository_root: &Path,
    out: &Path,
    options: &InitOptions,
) -> EvalResult<PathBuf> {
    options.validate()?;
    let repository_root = std::fs::canonicalize(repository_root)?;
    let local_root = repository_root.join("fixtures/local");
    std::fs::create_dir_all(&local_root)?;
    let local_root = std::fs::canonicalize(local_root)?;

    let out = absolute_without_parent_components(&repository_root, out)?;
    if !out.starts_with(&local_root) || out == local_root {
        return Err(EvalError::Usage(format!(
            "M0 初始化输出只能是 fixtures/local/ 下的新目录，拒绝：{}",
            out.display()
        )));
    }
    let parent = out
        .parent()
        .ok_or_else(|| EvalError::Usage("初始化输出缺父目录".to_owned()))?;
    let parent = std::fs::canonicalize(parent).map_err(|error| {
        EvalError::Usage(format!(
            "初始化输出的父目录必须已存在：{}（{error}）",
            parent.display()
        ))
    })?;
    if !parent.starts_with(&local_root) {
        return Err(EvalError::Usage(
            "初始化输出的父目录经符号链接解析后逃出了 fixtures/local/".to_owned(),
        ));
    }
    if out.exists() {
        return Err(EvalError::Usage(format!(
            "初始化目录已存在，拒绝覆盖：{}",
            out.display()
        )));
    }
    std::fs::create_dir_all(out.join("cases"))?;

    let mut cases = Vec::new();
    for index in 1..=options.screenshot_count {
        cases.push(case(
            &repository_root,
            &out,
            Pool::Screenshot,
            index,
            &options.added_on,
            SampleMetadata {
                source_type: "transaction_list".to_owned(),
                // 真实版式必须由人看样本后填；初始化器不能从路径猜，更不能把机构名写进去。
                layout: None,
                utterance_length: None,
            },
        )?);
    }

    let utterance_bands = [
        ("single", options.utterance_single),
        ("two_to_three", options.utterance_two_to_three),
        ("four_plus", options.utterance_four_plus),
    ];
    let mut utterance_index = 1;
    for (band, count) in utterance_bands {
        for _ in 0..count {
            cases.push(case(
                &repository_root,
                &out,
                Pool::Utterance,
                utterance_index,
                &options.added_on,
                SampleMetadata {
                    source_type: "utterance".to_owned(),
                    layout: None,
                    utterance_length: Some(band.to_owned()),
                },
            )?);
            utterance_index += 1;
        }
    }

    for index in 1..=options.control_count {
        cases.push(case(
            &repository_root,
            &out,
            Pool::Control,
            index,
            &options.added_on,
            SampleMetadata {
                source_type: if index % 2 == 0 {
                    "statement"
                } else {
                    "receipt"
                }
                .to_owned(),
                layout: None,
                utterance_length: None,
            },
        )?);
    }

    let manifest = Manifest {
        version: 1,
        profile: Some(ManifestProfile {
            kind: "m0_go_no_go".to_owned(),
        }),
        cases,
    };
    let manifest_path = out.join("manifest.json");
    let json = serde_json::to_string_pretty(&manifest)?;
    write_new(&manifest_path, format!("{json}\n").as_bytes())?;
    Ok(manifest_path)
}

fn case(
    repository_root: &Path,
    out: &Path,
    pool: Pool,
    index: usize,
    added_on: &str,
    sample: SampleMetadata,
) -> EvalResult<Case> {
    let id = format!("m0-{}-{index:03}", pool.as_str());
    let directory = out.join("cases").join(&id);
    std::fs::create_dir(&directory)?;
    let relative = directory
        .strip_prefix(repository_root)
        .map_err(|_| EvalError::Usage("初始化目录必须位于仓库内的 fixtures/local/".to_owned()))?;
    Ok(Case {
        id,
        dir: path_for_manifest(relative),
        expected: "expected.json".to_owned(),
        pool: Some(pool),
        enabled: true,
        added_on: added_on.to_owned(),
        flaky: false,
        sample: Some(sample),
    })
}

fn absolute_without_parent_components(repository_root: &Path, path: &Path) -> EvalResult<PathBuf> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(EvalError::Usage(
            "初始化输出路径不得含 `..`（拒绝目录逃逸）".to_owned(),
        ));
    }
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        repository_root.join(path)
    })
}

fn path_for_manifest(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn write_new(path: &Path, bytes: &[u8]) -> EvalResult<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod eval {
    use super::*;

    #[test]
    fn m0_initializer_creates_neutral_local_manifest_without_inputs() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repository.path().join("fixtures/local")).unwrap();
        let options = InitOptions::default();
        let manifest_path = initialize(
            repository.path(),
            Path::new("fixtures/local/m0-round-a"),
            &options,
        )
        .unwrap();
        let raw = std::fs::read_to_string(&manifest_path).unwrap();
        let manifest: Manifest = serde_json::from_str(&raw).unwrap();

        assert_eq!(manifest.version, 1);
        assert_eq!(manifest.profile.unwrap().kind, "m0_go_no_go");
        assert_eq!(manifest.cases.len(), 46, "22 截图 + 20 口述 + 4 对照");
        assert!(manifest.cases.iter().all(|case| {
            crate::eval::manifest::neutral_id(&case.id, case.pool.unwrap())
                && case.dir.starts_with("fixtures/local/")
        }));
        assert!(
            !raw.contains("input"),
            "初始化器不写真实输入路径，甚至不建 input 字段"
        );
        for case in &manifest.cases {
            let directory = repository.path().join(&case.dir);
            assert!(directory.is_dir());
            assert_eq!(std::fs::read_dir(directory).unwrap().count(), 0);
        }
    }

    #[test]
    fn m0_initializer_refuses_committed_set() {
        let repository = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repository.path().join("fixtures/local")).unwrap();
        std::fs::create_dir_all(repository.path().join("fixtures/ci")).unwrap();
        let error = initialize(
            repository.path(),
            Path::new("fixtures/ci/m0-should-not-exist"),
            &InitOptions::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("fixtures/local/"));
        assert!(!repository
            .path()
            .join("fixtures/ci/m0-should-not-exist")
            .exists());
    }
}
