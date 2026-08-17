//! `expected.json` —— **唯一的 ground truth**（07 §3.2）。
//!
//! 「一份人工确认过的、以来源为单位的条目清单。`transactions` 里的行只是它的一个常见
//! 素材来源——**因为用户丢弃掉的多读条目不在 `transactions` 里，而它必须计入错误**。」
//!
//! 金额是**十进制字符串**，与 `drafted_json` 里的表示一致（00 §3.4「金额怎么过 IPC」）：
//! JSON 数字会让超 `2^53` 的值被静默舍入，而那正是 agent 读错数字时产生的值。

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{EvalError, EvalResult};

/// 一条期望条目。位置标识 `source_ordinal` 由人工标注（07 §3.2）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedItem {
    pub source_ordinal: i64,
    pub occurred_on: String,
    pub amount_minor: String,
    pub currency: String,
    pub direction: String,
    /// 不进任何分数——判据是 07 §5 R1 待决（银行流水的商户文本常带门店号与流水号）。
    /// 留字段是为了让 R1 定下来之后不用重标一遍样本。
    #[serde(default)]
    pub merchant: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSet {
    pub source_kind: String,
    /// 口述来源在原文里说了几笔——指标 6（口述静默遗漏率）的分子要用。
    /// 缺省等于 `items.len()`。
    #[serde(default)]
    pub stated_item_count: Option<i64>,
    pub items: Vec<ExpectedItem>,
}

impl ExpectedSet {
    pub fn load(path: &Path) -> EvalResult<Self> {
        let raw = std::fs::read_to_string(path)?;
        let set: Self = serde_json::from_str(&raw)?;
        set.validate(path)?;
        Ok(set)
    }

    fn validate(&self, path: &Path) -> EvalResult<()> {
        if !matches!(self.source_kind.as_str(), "file" | "utterance") {
            return Err(EvalError::Fixture(format!(
                "{}: sourceKind 只能是 file 或 utterance",
                path.display()
            )));
        }
        let mut ordinals: Vec<i64> = self.items.iter().map(|item| item.source_ordinal).collect();
        ordinals.sort_unstable();
        ordinals.dedup();
        if ordinals.len() != self.items.len() {
            return Err(EvalError::Fixture(format!(
                "{}: source_ordinal 在期望侧必须唯一——整套对齐建立在它是键上（07 §3.2）",
                path.display()
            )));
        }
        for item in &self.items {
            if item.source_ordinal <= 0 {
                return Err(EvalError::Fixture(format!(
                    "{}: source_ordinal 从 1 起",
                    path.display()
                )));
            }
            if item.amount_minor.parse::<i64>().is_err() {
                return Err(EvalError::Fixture(format!(
                    "{}: amountMinor 必须是最小货币单位的十进制字符串，收到 `{}`",
                    path.display(),
                    item.amount_minor
                )));
            }
        }
        Ok(())
    }

    pub fn stated_item_count(&self) -> i64 {
        self.stated_item_count
            .unwrap_or_else(|| i64::try_from(self.items.len()).unwrap_or(i64::MAX))
    }
}

#[cfg(test)]
mod eval {
    use super::*;

    fn write(directory: &Path, body: &str) -> std::path::PathBuf {
        let path = directory.join("expected.json");
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn expected_rejects_duplicate_ordinals() {
        let directory = tempfile::tempdir().unwrap();
        let path = write(
            directory.path(),
            r#"{"sourceKind":"file","items":[
                {"sourceOrdinal":1,"occurredOn":"2026-08-03","amountMinor":"100","currency":"AUD","direction":"expense"},
                {"sourceOrdinal":1,"occurredOn":"2026-08-03","amountMinor":"200","currency":"AUD","direction":"expense"}]}"#,
        );
        assert!(ExpectedSet::load(&path).is_err());
    }

    #[test]
    fn expected_rejects_float_amounts() {
        let directory = tempfile::tempdir().unwrap();
        let path = write(
            directory.path(),
            r#"{"sourceKind":"file","items":[
                {"sourceOrdinal":1,"occurredOn":"2026-08-03","amountMinor":"16.80","currency":"AUD","direction":"expense"}]}"#,
        );
        let message = ExpectedSet::load(&path).unwrap_err().to_string();
        assert!(message.contains("最小货币单位"), "{message}");
    }

    #[test]
    fn stated_item_count_defaults_to_item_count() {
        let directory = tempfile::tempdir().unwrap();
        let path = write(
            directory.path(),
            r#"{"sourceKind":"utterance","items":[
                {"sourceOrdinal":1,"occurredOn":"2026-08-03","amountMinor":"100","currency":"AUD","direction":"expense"}]}"#,
        );
        assert_eq!(ExpectedSet::load(&path).unwrap().stated_item_count(), 1);
    }
}
