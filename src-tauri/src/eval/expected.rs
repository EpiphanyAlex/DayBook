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
use crate::money::{currency_exponent, ensure_range};

/// 一条期望条目。位置标识 `source_ordinal` 由人工标注（07 §3.2）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedItem {
    pub source_ordinal: i64,
    pub occurred_on: String,
    pub amount_minor: String,
    pub currency: String,
    pub direction: String,
    /// 新 formal 口述真值用「实际交易首次出现位置」守住 ordinal；普通旧夹具可省略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_span_start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_span_end: Option<i64>,
    /// 不进任何分数——判据是 07 §5 R1 待决（银行流水的商户文本常带门店号与流水号）。
    /// 留字段是为了让 R1 定下来之后不用重标一遍样本。
    #[serde(default)]
    pub merchant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationScopeStatus {
    Eligible,
    ScopeInvalid,
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimReason {
    CurrentSourceAllApplicable,
    OutsideViewport,
    Pagination,
    DayGroup,
    CategoryGroup,
    SingleItem,
    Subset,
    MultipleClaims,
    NoClaim,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateScope {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimIdentity {
    pub amount_minor: String,
    pub currency: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateClaim {
    pub amount_minor: String,
    pub currency: String,
    pub kind: String,
    pub scope: CandidateScope,
    pub reason: ClaimReason,
}

impl CandidateClaim {
    pub fn identity(&self) -> ClaimIdentity {
        ClaimIdentity {
            amount_minor: self.amount_minor.clone(),
            currency: self.currency.clone(),
            kind: self.kind.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationScope {
    pub status: ReconciliationScopeStatus,
    pub reason: ClaimReason,
    pub expected_claim: Option<ClaimIdentity>,
    pub candidate_claims: Vec<CandidateClaim>,
}

/// 一条 formal case 对 scope 契约的代码判定；指标 4 与 transcript 硬失败共用。
#[derive(Debug, Clone)]
pub struct ScopeEvaluation {
    pub reported_claim_matches_expected: Option<bool>,
    pub scope_violation: bool,
    pub exact_eligible_report: bool,
}

pub fn evaluate_scope(
    scope: Option<&ReconciliationScope>,
    reported: Option<&ClaimIdentity>,
) -> Option<ScopeEvaluation> {
    let scope = scope?;
    let matches = reported.map(|reported| {
        scope.status == ReconciliationScopeStatus::Eligible
            && scope.expected_claim.as_ref() == Some(reported)
    });
    Some(ScopeEvaluation {
        reported_claim_matches_expected: matches,
        scope_violation: reported.is_some() && matches != Some(true),
        exact_eligible_report: matches == Some(true),
    })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSet {
    pub source_kind: String,
    /// **这份真值有没有经人工核对过。**
    ///
    /// 手写的夹具按构造就是标注过的，所以缺省为 `true`；**导出器一律写 `false`**，
    /// 因为它只能拿 agent 那次的输出预填，而 [07 §3.2](../../../docs/prd/07-eval.md)
    /// 说得很清楚：`drafted_json` 是**被评分的那一侧**，不是评分的依据。
    ///
    /// 不设这个闸门的后果不是「少一道流程」，是**模型给自己判卷**——导出一条夹具、
    /// 直接跑分，每一项都会是满分，而那个满分什么也没测。
    #[serde(default = "annotated_by_default")]
    pub annotated: bool,
    /// 口述来源在原文里说了几笔——指标 6（口述静默遗漏率）的分子要用。
    /// 缺省等于 `items.len()`。
    #[serde(default)]
    pub stated_item_count: Option<i64>,
    /// M0 formal 的范围真值；普通 dry-run / replay 旧夹具可省略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciliation_scope: Option<ReconciliationScope>,
    pub items: Vec<ExpectedItem>,
}

fn annotated_by_default() -> bool {
    true
}

impl ExpectedSet {
    pub fn load(path: &Path) -> EvalResult<Self> {
        let raw = std::fs::read_to_string(path)?;
        let set: Self = serde_json::from_str(&raw)?;
        set.validate(path)?;
        Ok(set)
    }

    fn validate(&self, path: &Path) -> EvalResult<()> {
        if !self.annotated {
            return Err(EvalError::Fixture(format!(
                "{}: `annotated` 为 false —— 这份真值还是导出器用 agent 的输出预填的。\n  \
                 逐条对着原件核对、改对之后把 `annotated` 改成 true 再跑分；\n  \
                 直接拿它评分等于让模型给自己判卷（07 §3.2）",
                path.display()
            )));
        }
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

    /// 只有 M0 formal 调用：范围真值与口述第一交易 span 在加载 backend 前完成校验。
    pub fn validate_formal(&self, path: &Path, input_path: &Path) -> EvalResult<()> {
        let scope = self.reconciliation_scope.as_ref().ok_or_else(|| {
            EvalError::Fixture(format!(
                "{}: M0 formal expected.json 缺 reconciliationScope",
                path.display()
            ))
        })?;
        validate_reconciliation_scope(scope, path)?;

        if self.source_kind == "utterance" {
            let text = std::fs::read_to_string(input_path)?;
            let length = i64::try_from(text.chars().count()).unwrap_or(i64::MAX);
            let mut items = self.items.iter().collect::<Vec<_>>();
            items.sort_by_key(|item| item.source_ordinal);
            let mut previous_start = None;
            for (index, item) in items.iter().enumerate() {
                let expected_ordinal = i64::try_from(index + 1).unwrap_or(i64::MAX);
                if item.source_ordinal != expected_ordinal {
                    return Err(EvalError::Fixture(format!(
                        "{}: formal 口述 ordinal 必须恰为 1..N",
                        path.display()
                    )));
                }
                let (Some(start), Some(end)) = (item.evidence_span_start, item.evidence_span_end)
                else {
                    return Err(EvalError::Fixture(format!(
                        "{}: formal 口述每条期望交易必须带 evidenceSpanStart / evidenceSpanEnd",
                        path.display()
                    )));
                };
                if start < 0 || start >= end || end > length {
                    return Err(EvalError::Fixture(format!(
                        "{}: formal 口述 evidence span 越出原始未 normalize 文本",
                        path.display()
                    )));
                }
                if previous_start.is_some_and(|previous| start <= previous) {
                    return Err(EvalError::Fixture(format!(
                        "{}: formal 口述 ordinal 必须随第一处交易 span start 严格递增",
                        path.display()
                    )));
                }
                previous_start = Some(start);
            }
        }
        Ok(())
    }

    pub fn stated_item_count(&self) -> i64 {
        self.stated_item_count
            .unwrap_or_else(|| i64::try_from(self.items.len()).unwrap_or(i64::MAX))
    }
}

fn validate_reconciliation_scope(scope: &ReconciliationScope, path: &Path) -> EvalResult<()> {
    if scope.candidate_claims.len() > 16 {
        return Err(EvalError::Fixture(format!(
            "{}: reconciliationScope.candidateClaims 最多 16 条",
            path.display()
        )));
    }
    if let Some(claim) = &scope.expected_claim {
        validate_claim(claim, path)?;
    }
    for candidate in &scope.candidate_claims {
        validate_claim(&candidate.identity(), path)?;
        match candidate.scope {
            CandidateScope::Valid
                if candidate.reason != ClaimReason::CurrentSourceAllApplicable =>
            {
                return Err(EvalError::Fixture(format!(
                    "{}: valid candidate 的 reason 必须是 current_source_all_applicable",
                    path.display()
                )))
            }
            CandidateScope::Invalid
                if candidate.reason == ClaimReason::CurrentSourceAllApplicable =>
            {
                return Err(EvalError::Fixture(format!(
                    "{}: invalid candidate 不能标 current_source_all_applicable",
                    path.display()
                )))
            }
            _ => {}
        }
        if matches!(
            candidate.reason,
            ClaimReason::MultipleClaims | ClaimReason::NoClaim
        ) {
            return Err(EvalError::Fixture(format!(
                "{}: candidate reason 只能描述该候选自己的范围，不能是 multiple_claims / no_claim",
                path.display()
            )));
        }
    }

    match scope.status {
        ReconciliationScopeStatus::Eligible => {
            if scope.reason != ClaimReason::CurrentSourceAllApplicable {
                return Err(EvalError::Fixture(format!(
                    "{}: eligible 的 reason 只能是 current_source_all_applicable",
                    path.display()
                )));
            }
            let expected = scope.expected_claim.as_ref().ok_or_else(|| {
                EvalError::Fixture(format!("{}: eligible 缺 expectedClaim", path.display()))
            })?;
            let valid = scope
                .candidate_claims
                .iter()
                .filter(|candidate| candidate.scope == CandidateScope::Valid)
                .collect::<Vec<_>>();
            let identity_count = scope
                .candidate_claims
                .iter()
                .filter(|candidate| candidate.identity() == *expected)
                .count();
            if valid.len() != 1 || valid[0].identity() != *expected || identity_count != 1 {
                return Err(EvalError::Fixture(format!(
                    "{}: eligible 必须有且仅有一条 valid candidate，且 expectedClaim 三元组在全部候选中唯一；相同三元组 decoy 应改标 scope_invalid / multiple_claims",
                    path.display()
                )));
            }
        }
        ReconciliationScopeStatus::ScopeInvalid => {
            if scope.expected_claim.is_some() || scope.candidate_claims.is_empty() {
                return Err(EvalError::Fixture(format!(
                    "{}: scope_invalid 必须 expectedClaim = null 且 candidateClaims 非空",
                    path.display()
                )));
            }
            if matches!(
                scope.reason,
                ClaimReason::CurrentSourceAllApplicable | ClaimReason::NoClaim
            ) {
                return Err(EvalError::Fixture(format!(
                    "{}: scope_invalid 的 reason 不合法",
                    path.display()
                )));
            }
            let valid = scope
                .candidate_claims
                .iter()
                .filter(|candidate| candidate.scope == CandidateScope::Valid)
                .collect::<Vec<_>>();
            if scope.reason != ClaimReason::MultipleClaims && !valid.is_empty() {
                return Err(EvalError::Fixture(format!(
                    "{}: 非 multiple_claims 的 scope_invalid 不能含 valid candidate",
                    path.display()
                )));
            }
            if scope.reason == ClaimReason::MultipleClaims && scope.candidate_claims.len() < 2 {
                return Err(EvalError::Fixture(format!(
                    "{}: multiple_claims 至少需要两条 candidate",
                    path.display()
                )));
            }
            if scope.reason == ClaimReason::MultipleClaims && valid.len() == 1 {
                let identity = valid[0].identity();
                let identity_count = scope
                    .candidate_claims
                    .iter()
                    .filter(|candidate| candidate.identity() == identity)
                    .count();
                if identity_count == 1 {
                    return Err(EvalError::Fixture(format!(
                        "{}: 唯一 valid claim 且三元组无同值 decoy 时应标 eligible，不得标 scope_invalid / multiple_claims",
                        path.display()
                    )));
                }
            }
        }
        ReconciliationScopeStatus::Absent => {
            if scope.reason != ClaimReason::NoClaim
                || scope.expected_claim.is_some()
                || !scope.candidate_claims.is_empty()
            {
                return Err(EvalError::Fixture(format!(
                    "{}: absent 必须 reason = no_claim、expectedClaim = null、candidateClaims = []",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn validate_claim(claim: &ClaimIdentity, path: &Path) -> EvalResult<()> {
    let amount_is_valid = claim
        .amount_minor
        .parse::<i64>()
        .ok()
        .filter(|amount| amount.to_string() == claim.amount_minor)
        .and_then(|amount| ensure_range(amount).ok())
        .is_some();
    if !amount_is_valid
        || currency_exponent(&claim.currency).is_err()
        || !matches!(
            claim.kind.as_str(),
            "expense_total" | "income_total" | "net_change"
        )
    {
        return Err(EvalError::Fixture(format!(
            "{}: claim identity 必须使用范围内规范十进制整数字符串（无前导零、正号或负零）、ISO 4217 币种与既有 total kind",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod eval {
    use serde_json::json;

    use super::*;

    fn write(directory: &Path, body: &str) -> std::path::PathBuf {
        let path = directory.join("expected.json");
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn formal_claim_identity_rejects_numeric_aliases() {
        for alias in ["0300", "+300", "-0", "00"] {
            let claim = ClaimIdentity {
                amount_minor: alias.to_owned(),
                currency: "AUD".to_owned(),
                kind: "expense_total".to_owned(),
            };
            assert!(
                validate_claim(&claim, Path::new("synthetic.json")).is_err(),
                "金额别名不能逃过 decoy 身份比较：{alias}"
            );
        }
        for amount in ["300", "0", "-300"] {
            let claim = ClaimIdentity {
                amount_minor: amount.to_owned(),
                currency: "AUD".to_owned(),
                kind: "net_change".to_owned(),
            };
            validate_claim(&claim, Path::new("synthetic.json")).unwrap();
        }
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

    /// 07 §3.2：`drafted_json` 是被评分的那一侧，不是真值。导出器预填的那一份在人工
    /// 核对之前不许参与跑分——否则每一项都是满分，而那个满分什么也没测。
    #[test]
    fn unannotated_expected_set_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = write(
            directory.path(),
            r#"{"sourceKind":"file","annotated":false,"items":[
                {"sourceOrdinal":1,"occurredOn":"2026-08-03","amountMinor":"1680","currency":"AUD","direction":"expense"}]}"#,
        );
        let message = ExpectedSet::load(&path).unwrap_err().to_string();
        assert!(message.contains("模型给自己判卷"), "{message}");
    }

    /// 手写的夹具没有这个字段，按构造就是标注过的。
    #[test]
    fn handwritten_expected_set_is_annotated_by_default() {
        let directory = tempfile::tempdir().unwrap();
        let path = write(
            directory.path(),
            r#"{"sourceKind":"file","items":[
                {"sourceOrdinal":1,"occurredOn":"2026-08-03","amountMinor":"1680","currency":"AUD","direction":"expense"}]}"#,
        );
        assert!(ExpectedSet::load(&path).unwrap().annotated);
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

    #[test]
    fn formal_utterance_ordinals_follow_first_transaction_appearance() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.txt");
        std::fs::write(&input, "甲乙 丙丁").unwrap();
        let scope = json!({
            "status": "absent",
            "reason": "no_claim",
            "expectedClaim": null,
            "candidateClaims": []
        });
        let item = |ordinal: i64, start: Option<i64>, end: Option<i64>| {
            let mut value = json!({
                "sourceOrdinal": ordinal,
                "occurredOn": "2026-08-03",
                "amountMinor": "100",
                "currency": "AUD",
                "direction": "expense"
            });
            if let Some(start) = start {
                value["evidenceSpanStart"] = json!(start);
            }
            if let Some(end) = end {
                value["evidenceSpanEnd"] = json!(end);
            }
            value
        };
        for (name, items) in [
            ("missing-span", vec![item(1, None, None)]),
            (
                "non-contiguous",
                vec![item(1, Some(0), Some(2)), item(3, Some(3), Some(5))],
            ),
            (
                "span-order",
                vec![item(1, Some(3), Some(5)), item(2, Some(0), Some(2))],
            ),
            ("out-of-bounds", vec![item(1, Some(0), Some(99))]),
        ] {
            let path = directory.path().join(format!("{name}.json"));
            std::fs::write(
                &path,
                serde_json::to_vec(&json!({
                    "sourceKind": "utterance",
                    "reconciliationScope": scope,
                    "items": items
                }))
                .unwrap(),
            )
            .unwrap();
            let set = ExpectedSet::load(&path).unwrap();
            assert!(
                set.validate_formal(&path, &input).is_err(),
                "{name} 必须被 backend 前门禁拒绝"
            );
        }

        let valid_path = directory.path().join("valid.json");
        std::fs::write(
            &valid_path,
            serde_json::to_vec(&json!({
                "sourceKind": "utterance",
                "reconciliationScope": scope,
                "items": [item(1, Some(0), Some(2)), item(2, Some(3), Some(5))]
            }))
            .unwrap(),
        )
        .unwrap();
        let set = ExpectedSet::load(&valid_path).unwrap();
        set.validate_formal(&valid_path, &input).unwrap();
    }

    #[test]
    fn formal_reconciliation_scope_requires_unique_expected_claim() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.png");
        std::fs::write(&input, b"synthetic").unwrap();
        let claim = json!({"amountMinor":"100","currency":"AUD","kind":"expense_total"});
        let candidate = |scope: &str, reason: &str| {
            json!({
                "amountMinor":"100","currency":"AUD","kind":"expense_total",
                "scope": scope,"reason": reason
            })
        };
        let path = directory.path().join("eligible.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&json!({
                "sourceKind":"file",
                "reconciliationScope": {
                    "status":"eligible","reason":"current_source_all_applicable",
                    "expectedClaim":claim,
                    "candidateClaims":[
                        candidate("valid", "current_source_all_applicable"),
                        candidate("invalid", "day_group")
                    ]
                },
                "items":[]
            }))
            .unwrap(),
        )
        .unwrap();
        let set = ExpectedSet::load(&path).unwrap();
        assert!(
            set.validate_formal(&path, &input).is_err(),
            "相同三元组 decoy 让身份不可审计"
        );

        let wrongly_ambiguous_path = directory.path().join("wrongly-ambiguous.json");
        std::fs::write(
            &wrongly_ambiguous_path,
            serde_json::to_vec(&json!({
                "sourceKind":"file",
                "reconciliationScope": {
                    "status":"scope_invalid","reason":"multiple_claims",
                    "expectedClaim":null,
                    "candidateClaims":[
                        candidate("valid", "current_source_all_applicable"),
                        {"amountMinor":"200","currency":"AUD","kind":"expense_total","scope":"invalid","reason":"day_group"}
                    ]
                },
                "items":[]
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(
            ExpectedSet::load(&wrongly_ambiguous_path)
                .unwrap()
                .validate_formal(&wrongly_ambiguous_path, &input)
                .is_err(),
            "唯一 valid claim 与不同三元组 decoy 应标 eligible"
        );

        let unknown_currency_path = directory.path().join("unknown-currency.json");
        std::fs::write(
            &unknown_currency_path,
            serde_json::to_vec(&json!({
                "sourceKind":"file",
                "reconciliationScope": {
                    "status":"scope_invalid","reason":"outside_viewport",
                    "expectedClaim":null,
                    "candidateClaims":[
                        {"amountMinor":"100","currency":"ZZZ","kind":"expense_total","scope":"invalid","reason":"outside_viewport"}
                    ]
                },
                "items":[]
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(
            ExpectedSet::load(&unknown_currency_path)
                .unwrap()
                .validate_formal(&unknown_currency_path, &input)
                .is_err(),
            "candidate claim 必须使用仓库 ISO 4217 表"
        );

        let ambiguous_path = directory.path().join("ambiguous.json");
        std::fs::write(
            &ambiguous_path,
            serde_json::to_vec(&json!({
                "sourceKind":"file",
                "reconciliationScope": {
                    "status":"scope_invalid","reason":"multiple_claims",
                    "expectedClaim":null,
                    "candidateClaims":[
                        candidate("valid", "current_source_all_applicable"),
                        candidate("invalid", "day_group")
                    ]
                },
                "items":[]
            }))
            .unwrap(),
        )
        .unwrap();
        ExpectedSet::load(&ambiguous_path)
            .unwrap()
            .validate_formal(&ambiguous_path, &input)
            .unwrap();
    }
}
