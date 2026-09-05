//! 结构化报告。
//!
//! **Rust 这一侧只出计数，不出比率**——`0.967 (58/60)` 的那个 `0.967` 由
//! `scripts/eval.mjs` 渲染。理由不只是那条浮点门禁：报表的排版是改得最频繁的东西，
//! 而计数口径是最不该动的东西，把它们放在同一个文件里迟早会一起改。
//!
//! 07 §3.5 要求 diff 表带**模型标识、后端标识与 `prompt_hash`**——「否则无法区分模型
//! 退步与提示词变更导致的回归」。它们都在 `parse_attempts` 上，随每条用例带出来。

use std::collections::BTreeMap;

use serde::Serialize;

use super::{
    expected::{evaluate_scope, ClaimIdentity, ExpectedItem, ReconciliationScope},
    join::{DegradedMatch, HardField, OrdinalJoin, PredictedItem, HARD_FIELDS},
    live::TrialDiagnostics,
    manifest::Pool,
    metrics::{
        compute_control_pool, compute_decision_metrics, compute_pool, overall_verdict, CaseOutcome,
        Metric, PoolReport, UsageCounts,
    },
    replay::CallResult,
};
use crate::domain::confirm::TotalCheck;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    pub backend_id: String,
    pub backend_version: Option<String>,
    pub model_id: Option<String>,
    pub prompt_hash: String,
    pub tool_surface_version: String,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardFieldValues {
    pub amount_minor: String,
    pub currency: String,
    pub occurred_on: String,
    pub direction: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HardFieldDiff {
    pub source_ordinal: i64,
    pub pairing: &'static str,
    pub expected: Option<HardFieldValues>,
    pub predicted: Option<HardFieldValues>,
    pub wrong_fields: Vec<HardField>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceExcerpt {
    pub text: String,
    pub original_code_point_length: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportedClaim {
    pub amount_minor: String,
    pub currency: String,
    pub kind: String,
    pub evidence_excerpt: EvidenceExcerpt,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationEvidence {
    pub expected_scope: ReconciliationScope,
    pub reported_claim_matches_expected: Option<bool>,
    pub scope_violation: bool,
    pub reported: Option<ReportedClaim>,
    pub computed_minor: Option<String>,
    pub delta_minor: Option<String>,
}

pub fn reported_claim_identity(check: &TotalCheck) -> Option<ClaimIdentity> {
    Some(ClaimIdentity {
        amount_minor: check.reported_total_minor?.0.to_string(),
        currency: check.reported_total_currency.clone()?,
        kind: check.reported_total_kind.clone()?,
    })
}

pub fn build_reconciliation_evidence(
    scope: Option<&ReconciliationScope>,
    check: &TotalCheck,
) -> Option<ReconciliationEvidence> {
    let scope = scope?;
    let identity = reported_claim_identity(check);
    let evaluated = evaluate_scope(Some(scope), identity.as_ref()).expect("scope 已存在");
    let reported = identity.map(|identity| {
        let evidence = check
            .reported_total_evidence_text
            .as_deref()
            .unwrap_or_default();
        let original_code_point_length = evidence.chars().count();
        ReportedClaim {
            amount_minor: identity.amount_minor,
            currency: identity.currency,
            kind: identity.kind,
            evidence_excerpt: EvidenceExcerpt {
                text: evidence.chars().take(160).collect(),
                original_code_point_length,
                truncated: original_code_point_length > 160,
            },
        }
    });
    let computed_minor = check
        .calculated_total_minor
        .map(|amount| amount.0.to_string());
    let delta_minor = match (check.calculated_total_minor, check.reported_total_minor) {
        (Some(computed), Some(reported)) => {
            Some((i128::from(computed.0) - i128::from(reported.0)).to_string())
        }
        _ => None,
    };
    Some(ReconciliationEvidence {
        expected_scope: scope.clone(),
        reported_claim_matches_expected: evaluated.reported_claim_matches_expected,
        scope_violation: evaluated.scope_violation,
        reported,
        computed_minor,
        delta_minor,
    })
}

pub fn build_hard_field_diffs(
    expected: &[ExpectedItem],
    predicted: &[PredictedItem],
    join: &OrdinalJoin,
) -> Vec<HardFieldDiff> {
    let expected = expected
        .iter()
        .map(|item| (item.source_ordinal, item))
        .collect::<BTreeMap<_, _>>();
    let predicted = predicted
        .iter()
        .map(|item| (item.source_ordinal, item))
        .collect::<BTreeMap<_, _>>();
    let matched = join
        .matched
        .iter()
        .map(|pair| (pair.source_ordinal, pair))
        .collect::<BTreeMap<_, _>>();
    let mut ordinals = matched
        .keys()
        .copied()
        .chain(join.missed.iter().copied())
        .chain(join.extra.iter().copied())
        .collect::<Vec<_>>();
    ordinals.sort_unstable();
    ordinals.dedup();

    ordinals
        .into_iter()
        .filter_map(|ordinal| {
            let left = expected.get(&ordinal).copied();
            let right = predicted.get(&ordinal).copied();
            let (pairing, wrong_fields) = match (left, right) {
                (Some(_), Some(_)) => {
                    let pair = matched.get(&ordinal)?;
                    if pair.wrong_fields.is_empty() {
                        return None;
                    }
                    ("matched", pair.wrong_fields.clone())
                }
                (Some(_), None) => ("expected_only", HARD_FIELDS.to_vec()),
                (None, Some(_)) => ("predicted_only", HARD_FIELDS.to_vec()),
                (None, None) => return None,
            };
            Some(HardFieldDiff {
                source_ordinal: ordinal,
                pairing,
                expected: left.map(|item| HardFieldValues {
                    amount_minor: item.amount_minor.clone(),
                    currency: item.currency.clone(),
                    occurred_on: item.occurred_on.clone(),
                    direction: item.direction.clone(),
                }),
                predicted: right.map(|item| HardFieldValues {
                    amount_minor: item.amount_minor.to_string(),
                    currency: item.currency.clone(),
                    occurred_on: item.occurred_on.clone(),
                    direction: item.direction.clone(),
                }),
                wrong_fields,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseReport {
    pub id: String,
    pub pool: &'static str,
    pub judged: bool,
    pub flaky: bool,
    pub source_kind: String,
    pub attribution: Attribution,
    pub reconciliation_status: String,
    pub confirmation_policy: String,
    pub unparsed_note: String,
    /// formal v2 只保存错误 / 漏读 / 多读项；正确项不重复膨胀报告。
    pub hard_field_diffs: Vec<HardFieldDiff>,
    /// formal v2 的 bounded 对账上下文；旧普通夹具没有 scope 真值时为 `null`。
    pub reconciliation_evidence: Option<ReconciliationEvidence>,
    pub matched: i64,
    /// **漏读单独一列。** 条数相等不代表读对了。
    pub missed: i64,
    /// **多读单独一列。**
    pub extra: i64,
    pub join: OrdinalJoin,
    /// 诊断栏，永远带着「诊断用」的标签。
    pub degraded: DegradedMatch,
    /// 重放路径上是那次录像的回放结果；真跑轮次为空（那一路没有「录下来的调用」）。
    pub calls: Vec<CallResult>,
    /// **诊断栏**：`--trials N` 第 2 轮起的产物。`None` = 只跑了 1 轮。
    /// **永远不覆盖正式数**（§9.4 口径③）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trial_diagnostics: Option<TrialDiagnostics>,
    /// `evidence_text` 不是转写文本真子串的草稿 id（只对 `kind = utterance` 有意义）。
    pub substring_violations: Vec<String>,
    /// 与 §9.4「干净来源」同口径；正式诊断据此取首轮失败集合。
    pub case_passed: bool,
    /// 单 case 的模型输出 / 完成协议质量失败。基础设施错误不会生成半份 case report。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_error: Option<String>,
    /// 指标 9：只记录，不进 verdict。重放为 `None`。
    pub duration_ms: Option<i64>,
    /// 指标 10：只记录整数 usage；拿不到就 `null`，不得伪装成 0。
    pub usage: Option<UsageCounts>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// `replay` = 重放夹具（零额度）；`live` = 真跑 agent（烧额度）。
    pub mode: &'static str,
    /// 07 §3.4 口径③：`--trials` 默认 1，**每条 1 轮出正式数**。
    /// 大于 1 时多出来的轮次只进各用例的诊断栏。
    pub trials: u32,
    pub thresholds_source: &'static str,
    pub cases: Vec<CaseReport>,
    pub pools: Vec<PoolReport>,
    /// 指标 4–8 在截图池 + 口述池的正式集合上聚合；不在两池各造一套判定。
    pub decision_metrics: Vec<Metric>,
    /// 正式判定池的 transcript 硬计数；必须精确为 0，对照栏只留逐例证据。
    pub scope_invalid_total_reports: i64,
    /// `go` / `conditional_go` / `no_go`（§9.4「判定」）。**对照栏不参与。**
    pub verdict: &'static str,
}

impl Report {
    pub fn build(mode: &'static str, cases: Vec<CaseReport>, outcomes: &[CaseOutcome]) -> Self {
        Self::with_trials(mode, 1, cases, outcomes)
    }

    pub fn with_trials(
        mode: &'static str,
        trials: u32,
        cases: Vec<CaseReport>,
        outcomes: &[CaseOutcome],
    ) -> Self {
        let mut pools = Vec::new();
        for pool in [Pool::Screenshot, Pool::Utterance] {
            let subset: Vec<&CaseOutcome> = outcomes
                .iter()
                .filter(|outcome| outcome.pool == pool)
                .collect();
            if !subset.is_empty() {
                pools.push(compute_pool(pool, &subset));
            }
        }
        let control: Vec<&CaseOutcome> = outcomes
            .iter()
            .filter(|outcome| outcome.pool == Pool::Control)
            .collect();
        if !control.is_empty() {
            pools.push(compute_control_pool(&control));
        }
        let judged: Vec<&CaseOutcome> = outcomes
            .iter()
            .filter(|outcome| outcome.pool.is_judged())
            .collect();
        let decision_metrics = compute_decision_metrics(&judged, None);
        let scope_invalid_total_reports = judged
            .iter()
            .filter(|outcome| {
                outcome
                    .scope_evaluation
                    .as_ref()
                    .is_some_and(|scope| scope.scope_violation)
            })
            .count() as i64;
        let verdict = if scope_invalid_total_reports > 0 {
            "no_go"
        } else {
            overall_verdict(&pools, &decision_metrics)
        };
        Self {
            mode,
            trials,
            thresholds_source: "docs/PRD.md §9.4",
            cases,
            pools,
            decision_metrics,
            scope_invalid_total_reports,
            verdict,
        }
    }
}

#[cfg(test)]
mod eval {
    use super::*;
    use crate::eval::join::MatchedPair;

    fn outcome(id: &str, pool: Pool, missed: Vec<i64>) -> CaseOutcome {
        CaseOutcome {
            id: id.to_owned(),
            pool,
            source_kind: "file".to_owned(),
            join: OrdinalJoin {
                matched: vec![MatchedPair {
                    source_ordinal: 1,
                    wrong_fields: Vec::new(),
                    free_text_differences: Vec::new(),
                }],
                missed,
                extra: Vec::new(),
            },
            degraded: DegradedMatch::default(),
            reconciliation_status: "passed".to_owned(),
            confirmation_policy: "reconciled_batch".to_owned(),
            unparsed_note: String::new(),
            scope_evaluation: None,
            stated_item_count: 1,
            duration_ms: None,
            usage: None,
        }
    }

    /// 07 §6：「把 manifest 里标为对照栏的用例**单独成栏，且不计入判定池的任何指标**。」
    #[test]
    fn control_column_is_its_own_column() {
        let outcomes = vec![
            outcome("shot", Pool::Screenshot, Vec::new()),
            outcome("receipt", Pool::Control, vec![2, 3, 4]),
        ];
        let report = Report::build("replay", Vec::new(), &outcomes);

        let screenshot = report
            .pools
            .iter()
            .find(|pool| pool.pool == "screenshot")
            .unwrap();
        let control = report
            .pools
            .iter()
            .find(|pool| pool.pool == "control")
            .unwrap();
        assert_eq!(screenshot.case_count, 1);
        assert_eq!(control.case_count, 1);
        assert!(!control.judged);

        let recall = screenshot
            .metrics
            .iter()
            .find(|metric| metric.key == "item_recall")
            .unwrap();
        assert_eq!(
            recall.ratio.den, 1,
            "对照栏那三条漏读不该出现在判定池的分母里"
        );
        assert_eq!(report.verdict, "go");
    }

    #[test]
    fn formal_control_scope_violation_does_not_change_verdict() {
        let mut control = outcome("control", Pool::Control, Vec::new());
        control.scope_evaluation = Some(crate::eval::expected::ScopeEvaluation {
            reported_claim_matches_expected: Some(false),
            scope_violation: true,
            exact_eligible_report: false,
        });
        let report = Report::build(
            "live",
            Vec::new(),
            &[outcome("shot", Pool::Screenshot, Vec::new()), control],
        );
        assert_eq!(report.scope_invalid_total_reports, 0);
        assert_eq!(report.verdict, "go");
    }

    /// 07 §6 口径①：两池并列，各自带判定。
    #[test]
    fn pools_are_reported_side_by_side() {
        let outcomes = vec![
            outcome("shot", Pool::Screenshot, Vec::new()),
            CaseOutcome {
                source_kind: "utterance".to_owned(),
                ..outcome("said", Pool::Utterance, Vec::new())
            },
        ];
        let report = Report::build("replay", Vec::new(), &outcomes);
        let names: Vec<&str> = report.pools.iter().map(|pool| pool.pool).collect();
        assert_eq!(names, vec!["screenshot", "utterance"]);
        assert!(report.pools.iter().all(|pool| pool.judged));
    }

    /// 口径③：默认 1 轮，且报告里写明阈值出处。
    #[test]
    fn report_declares_trials_and_threshold_source() {
        let report = Report::build("replay", Vec::new(), &[]);
        assert_eq!(report.trials, 1);
        assert_eq!(report.thresholds_source, "docs/PRD.md §9.4");
    }

    #[test]
    fn formal_report_persists_expected_and_predicted_hard_fields() {
        use crate::eval::{
            expected::ExpectedItem,
            join::{ordinal_full_outer_join, PredictedItem},
        };

        let expected = vec![
            ExpectedItem {
                source_ordinal: 1,
                occurred_on: "2026-08-01".to_owned(),
                amount_minor: "100".to_owned(),
                currency: "AUD".to_owned(),
                direction: "expense".to_owned(),
                evidence_span_start: None,
                evidence_span_end: None,
                merchant: None,
            },
            ExpectedItem {
                source_ordinal: 2,
                occurred_on: "2026-08-02".to_owned(),
                amount_minor: "200".to_owned(),
                currency: "AUD".to_owned(),
                direction: "expense".to_owned(),
                evidence_span_start: None,
                evidence_span_end: None,
                merchant: None,
            },
        ];
        let predicted = vec![
            PredictedItem {
                source_ordinal: 1,
                occurred_on: "2026-08-01".to_owned(),
                amount_minor: 999,
                currency: "AUD".to_owned(),
                direction: "expense".to_owned(),
                merchant: "A".to_owned(),
                category: None,
                channel: None,
            },
            PredictedItem {
                source_ordinal: 3,
                occurred_on: "2026-08-03".to_owned(),
                amount_minor: 300,
                currency: "AUD".to_owned(),
                direction: "income".to_owned(),
                merchant: "B".to_owned(),
                category: None,
                channel: None,
            },
        ];
        let join = ordinal_full_outer_join(&expected, &predicted);
        let diffs = build_hard_field_diffs(&expected, &predicted, &join);
        assert_eq!(diffs.len(), 3);
        let value = serde_json::to_value(&diffs).unwrap();
        assert_eq!(value[0]["pairing"], "matched");
        assert_eq!(value[0]["expected"]["amountMinor"], "100");
        assert_eq!(value[0]["predicted"]["amountMinor"], "999");
        assert_eq!(value[1]["pairing"], "expected_only");
        assert!(value[1]["predicted"].is_null());
        assert_eq!(value[2]["pairing"], "predicted_only");
        assert!(value[2]["expected"].is_null());
        assert!(value.to_string().contains("direction"));
        assert!(!value.to_string().contains("merchant"));
    }

    #[test]
    fn formal_report_persists_bounded_reconciliation_evidence() {
        use crate::{
            domain::confirm::TotalCheck,
            eval::expected::{
                CandidateClaim, CandidateScope, ClaimIdentity, ClaimReason, ReconciliationScope,
                ReconciliationScopeStatus,
            },
            money::DecimalI64,
        };

        let scope = ReconciliationScope {
            status: ReconciliationScopeStatus::Eligible,
            reason: ClaimReason::CurrentSourceAllApplicable,
            expected_claim: Some(ClaimIdentity {
                amount_minor: "90".to_owned(),
                currency: "AUD".to_owned(),
                kind: "expense_total".to_owned(),
            }),
            candidate_claims: vec![CandidateClaim {
                amount_minor: "90".to_owned(),
                currency: "AUD".to_owned(),
                kind: "expense_total".to_owned(),
                scope: CandidateScope::Valid,
                reason: ClaimReason::CurrentSourceAllApplicable,
            }],
        };
        let check = TotalCheck {
            attempt_id: "a".to_owned(),
            source_id: "s".to_owned(),
            source_kind: "file".to_owned(),
            reconciliation_status: "failed".to_owned(),
            confirmation_policy: "single_only".to_owned(),
            reported_total_minor: Some(DecimalI64(100)),
            calculated_total_minor: Some(DecimalI64(90)),
            reported_total_currency: Some("AUD".to_owned()),
            reported_total_kind: Some("expense_total".to_owned()),
            reported_total_evidence_text: Some("界".repeat(161)),
            unavailable_draft_ids: Vec::new(),
            outcome: Some("completed".to_owned()),
            unparsed_note: Some("不得进入报告的完整说明".to_owned()),
        };
        let evidence = build_reconciliation_evidence(Some(&scope), &check).unwrap();
        assert_eq!(evidence.reported_claim_matches_expected, Some(false));
        assert!(evidence.scope_violation);
        assert_eq!(evidence.computed_minor.as_deref(), Some("90"));
        assert_eq!(evidence.delta_minor.as_deref(), Some("-10"));
        let excerpt = &evidence.reported.as_ref().unwrap().evidence_excerpt;
        assert_eq!(excerpt.text.chars().count(), 160);
        assert_eq!(excerpt.original_code_point_length, 161);
        assert!(excerpt.truncated);
        let json = serde_json::to_string(&evidence).unwrap();
        assert!(!json.contains("不得进入报告的完整说明"));
    }
}
