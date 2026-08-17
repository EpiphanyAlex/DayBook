//! 结构化报告。
//!
//! **Rust 这一侧只出计数，不出比率**——`0.967 (58/60)` 的那个 `0.967` 由
//! `scripts/eval.mjs` 渲染。理由不只是那条浮点门禁：报表的排版是改得最频繁的东西，
//! 而计数口径是最不该动的东西，把它们放在同一个文件里迟早会一起改。
//!
//! 07 §3.5 要求 diff 表带**模型标识、后端标识与 `prompt_hash`**——「否则无法区分模型
//! 退步与提示词变更导致的回归」。它们都在 `parse_attempts` 上，随每条用例带出来。

use serde::Serialize;

use super::{
    join::{DegradedMatch, OrdinalJoin},
    manifest::Pool,
    metrics::{compute_pool, overall_verdict, CaseOutcome, PoolReport},
    replay::CallResult,
};

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
    pub matched: i64,
    /// **漏读单独一列。** 条数相等不代表读对了。
    pub missed: i64,
    /// **多读单独一列。**
    pub extra: i64,
    pub join: OrdinalJoin,
    /// 诊断栏，永远带着「诊断用」的标签。
    pub degraded: DegradedMatch,
    pub calls: Vec<CallResult>,
    /// `evidence_text` 不是转写文本真子串的草稿 id（只对 `kind = utterance` 有意义）。
    pub substring_violations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// `replay` = 重放夹具（零额度）；真跑 agent 的轮次是另一条路径。
    pub mode: &'static str,
    /// 07 §3.4 口径③：`--trials` 默认 1，**每条 1 轮出正式数**。
    pub trials: u32,
    pub thresholds_source: &'static str,
    pub cases: Vec<CaseReport>,
    pub pools: Vec<PoolReport>,
    /// `go` / `conditional_go` / `no_go`（§9.4「判定」）。**对照栏不参与。**
    pub verdict: &'static str,
}

impl Report {
    pub fn build(mode: &'static str, cases: Vec<CaseReport>, outcomes: &[CaseOutcome]) -> Self {
        let mut pools = Vec::new();
        for pool in [Pool::Screenshot, Pool::Utterance, Pool::Control] {
            let subset: Vec<&CaseOutcome> = outcomes
                .iter()
                .filter(|outcome| outcome.pool == pool)
                .collect();
            if subset.is_empty() {
                continue;
            }
            pools.push(compute_pool(pool, &subset));
        }
        let verdict = overall_verdict(&pools);
        Self {
            mode,
            trials: 1,
            thresholds_source: "docs/PRD.md §9.4",
            cases,
            pools,
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
            stated_item_count: 1,
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
}
